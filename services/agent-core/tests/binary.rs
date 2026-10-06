//! The built binary, run as a process: startup log, one `Decide`, graceful SIGTERM exit, and a
//! readable failure for bad configuration.
#![cfg(unix)]

use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::{DecideRequest, decide_response};
use proto::grpc::health::v1::HealthCheckRequest;
use proto::grpc::health::v1::health_check_response::ServingStatus;
use proto::grpc::health::v1::health_client::HealthClient;
use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio_stream::StreamExt;
use tonic_reflection::pb::v1::ServerReflectionRequest;
use tonic_reflection::pb::v1::server_reflection_client::ServerReflectionClient;
use tonic_reflection::pb::v1::server_reflection_request::MessageRequest;
use tonic_reflection::pb::v1::server_reflection_response::MessageResponse;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(20);

/// The spawned binary; killed on drop so a failing test leaves no process behind. Its stdout is
/// read by a thread, so the test can wait for a line without blocking on the pipe.
struct Server {
    child: Child,
    stdout_lines: Receiver<String>,
    seen: Vec<String>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    /// Spawns the binary with only the listen address set, so there is no OTLP endpoint.
    fn spawn(listen_address: &str) -> Self {
        Self::spawn_with(listen_address, &[])
    }

    /// Like `spawn`, with extra environment variables.
    fn spawn_with(listen_address: &str, env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_agent-core"))
            .env("ARIA_AGENT_CORE_LISTEN_ADDRESS", listen_address)
            .envs(env.iter().copied())
            .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
            .env_remove("RUST_LOG")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the binary");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, stdout_lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdout_lines,
            seen: Vec::new(),
        }
    }

    /// Waits for a stdout line containing `needle` and returns it. Fails at once, with what the
    /// process printed, if it exits first.
    fn wait_for_line(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + LIMIT;
        loop {
            match self.stdout_lines.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    self.seen.push(line.clone());
                    if line.contains(needle) {
                        return line;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {}
            }
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                self.drain_stdout();
                let stderr = self.stderr();
                panic!(
                    "the binary exited ({status:?}) before printing {needle:?}\nstdout: {:#?}\nstderr: {stderr}",
                    self.seen
                );
            }
            assert!(
                Instant::now() < deadline,
                "no line containing {needle:?} in time; stdout so far: {:#?}",
                self.seen
            );
        }
    }

    /// Collects the stdout lines still buffered or arriving until the pipe closes.
    fn drain_stdout(&mut self) -> String {
        let deadline = Instant::now() + LIMIT;
        while Instant::now() < deadline {
            match self.stdout_lines.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => self.seen.push(line),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        self.seen.join("\n")
    }

    /// Everything the exited process wrote to stderr.
    fn stderr(&mut self) -> String {
        let mut out = String::new();
        if let Some(stderr) = self.child.stderr.as_mut() {
            let _ = stderr.read_to_string(&mut out);
        }
        out
    }

    /// Waits for the process to exit, polling `try_wait` until `LIMIT`.
    async fn wait_for_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + LIMIT;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the process did not exit in time"
            );
            sleep(Duration::from_millis(20)).await;
        }
    }
}

/// The address between `on '` and the next quote in the startup line.
fn address_in(line: &str) -> String {
    let rest = line.split("on '").nth(1).expect("an address in the line");
    rest.split('\'')
        .next()
        .expect("a closing quote")
        .to_string()
}

#[tokio::test]
async fn binary_serves_echoed_decide_and_exits_zero_on_sigterm() {
    // Port 0: the binary picks a free port itself and reports it, so there is no bind race.
    let mut server = Server::spawn("127.0.0.1:0");
    let startup = server.wait_for_line("backend on");
    let address = address_in(&startup);
    // The listener is bound before that line is logged, so the connection is accepted at once.
    let mut client =
        tokio::time::timeout(LIMIT, AgentCoreClient::connect(format!("http://{address}")))
            .await
            .expect("connect timed out")
            .expect("connect");

    let response = tokio::time::timeout(
        LIMIT,
        client.decide(DecideRequest {
            session_id: "binary-1".to_string(),
            speaker_name: None,
            text: "hello aria".to_string(),
        }),
    )
    .await
    .expect("decide timed out")
    .expect("decide");
    let version_header = response
        .metadata()
        .get("x-aria-version")
        .map(|v| v.to_str().expect("ascii").to_string());
    let mut stream = response.into_inner();

    let mut echoed = String::new();
    let mut deltas = 0;
    let mut completes = 0;
    while let Some(item) = tokio::time::timeout(LIMIT, stream.next())
        .await
        .expect("stream timed out")
    {
        match item.expect("status").payload.expect("payload") {
            decide_response::Payload::TextDelta(text) => {
                deltas += 1;
                echoed.push_str(&text);
            }
            decide_response::Payload::TurnComplete(_) => completes += 1,
        }
    }
    // Every response carries the build's version.
    assert_eq!(
        version_header.as_deref(),
        Some(env!("CARGO_PKG_VERSION")),
        "x-aria-version"
    );
    assert!(deltas >= 2, "expected at least two chunks, got {deltas}");
    assert_eq!(echoed, "hello aria");
    assert_eq!(completes, 1);

    let pid = server.child.id().to_string();
    let killed = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .expect("run kill");
    assert!(killed.success());
    let status = server.wait_for_exit().await;
    assert_eq!(status.code(), Some(0), "{status:?}");

    let stdout = server.drain_stdout();
    assert!(stdout.contains("'echo' backend"), "{stdout}");
    assert!(stdout.contains("OTLP tracing disabled"), "{stdout}");
}

#[tokio::test]
async fn invalid_listen_address_exits_non_zero_with_a_readable_message() {
    let mut server = Server::spawn("not-an-address");

    let status = server.wait_for_exit().await;

    assert_eq!(status.code(), Some(1), "{status:?}");
    let stderr = server.stderr();
    assert!(stderr.contains("invalid listen address"), "{stderr}");
    assert!(
        stderr.contains("ARIA_AGENT_CORE_LISTEN_ADDRESS"),
        "{stderr}"
    );
    assert!(stderr.contains("not-an-address"), "{stderr}");
}

#[tokio::test]
async fn binary_reports_serving_for_the_overall_server_and_agent_core_over_its_port() {
    let mut server = Server::spawn("127.0.0.1:0");
    let startup = server.wait_for_line("backend on");
    let address = address_in(&startup);
    let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut health = HealthClient::new(channel);

    for service in ["", "aria.agent_core.v1.AgentCore"] {
        let response = tokio::time::timeout(
            LIMIT,
            health.check(HealthCheckRequest {
                service: service.to_string(),
            }),
        )
        .await
        .expect("check timed out")
        .expect("check");
        assert_eq!(
            response.into_inner().status(),
            ServingStatus::Serving,
            "{service:?}"
        );
    }
}

#[tokio::test]
async fn open_health_watch_does_not_hold_the_shutdown_drain() {
    let mut server = Server::spawn("127.0.0.1:0");
    let startup = server.wait_for_line("backend on");
    let address = address_in(&startup);
    let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut health = HealthClient::new(channel);
    let mut watch = health
        .watch(HealthCheckRequest {
            service: String::new(),
        })
        .await
        .expect("watch")
        .into_inner();
    let first = tokio::time::timeout(LIMIT, watch.next())
        .await
        .expect("first status timed out")
        .expect("stream ended")
        .expect("status");
    assert_eq!(first.status(), ServingStatus::Serving);

    let started = Instant::now();
    let pid = server.child.id().to_string();
    let killed = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .expect("run kill");
    assert!(killed.success());

    // The watcher is told not-serving, and its stream ends.
    let update = tokio::time::timeout(LIMIT, watch.next())
        .await
        .expect("no update after SIGTERM")
        .expect("stream ended without an update")
        .expect("status");
    assert_eq!(update.status(), ServingStatus::NotServing);
    let end = tokio::time::timeout(LIMIT, watch.next())
        .await
        .expect("the stream did not end");
    assert!(end.is_none(), "{end:?}");

    let status = server.wait_for_exit().await;
    assert_eq!(status.code(), Some(0), "{status:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "shutdown took {:?}, so the drain bound was probably hit",
        started.elapsed()
    );
    let stdout = server.drain_stdout();
    assert!(!stdout.contains("did not finish in time"), "{stdout}");
}

/// Asks the reflection service for its list of services.
async fn list_services(address: &str) -> Result<Vec<String>, tonic::Status> {
    let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = ServerReflectionClient::new(channel);
    let request = ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::ListServices(String::new())),
    };
    let mut stream = client
        .server_reflection_info(tokio_stream::once(request))
        .await?
        .into_inner();
    let response = tokio::time::timeout(LIMIT, stream.next())
        .await
        .expect("reflection reply timed out")
        .expect("stream ended")?;
    match response.message_response.expect("a response") {
        MessageResponse::ListServicesResponse(list) => {
            Ok(list.service.into_iter().map(|s| s.name).collect())
        }
        other => panic!("unexpected reflection response: {other:?}"),
    }
}

/// The encoded file descriptors reflection returns for a symbol, concatenated.
async fn describe_symbol(address: &str, symbol: &str) -> Vec<u8> {
    let channel = tonic::transport::Channel::from_shared(format!("http://{address}"))
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut client = ServerReflectionClient::new(channel);
    let request = ServerReflectionRequest {
        host: String::new(),
        message_request: Some(MessageRequest::FileContainingSymbol(symbol.to_string())),
    };
    let mut stream = client
        .server_reflection_info(tokio_stream::once(request))
        .await
        .expect("reflection")
        .into_inner();
    let response = tokio::time::timeout(LIMIT, stream.next())
        .await
        .expect("reflection reply timed out")
        .expect("stream ended")
        .expect("status");
    match response.message_response.expect("a response") {
        MessageResponse::FileDescriptorResponse(files) => files.file_descriptor_proto.concat(),
        other => panic!("unexpected reflection response: {other:?}"),
    }
}

#[tokio::test]
async fn reflection_on_lists_the_agent_core_and_health_services() {
    let mut server = Server::spawn_with("127.0.0.1:0", &[("ARIA_AGENT_CORE_REFLECTION", "true")]);
    let startup = server.wait_for_line("backend on");
    let address = address_in(&startup);

    let services = list_services(&address).await.expect("reflection is served");

    assert!(
        services.contains(&"aria.agent_core.v1.AgentCore".to_string()),
        "{services:?}"
    );
    assert!(
        services.contains(&"grpc.health.v1.Health".to_string()),
        "{services:?}"
    );
    // The health service's descriptor is ours, so it includes `List`.
    let descriptor = describe_symbol(&address, "grpc.health.v1.Health").await;
    assert!(
        descriptor.windows(15).any(|w| w == b"HealthListReque"),
        "the health descriptor does not mention List"
    );
    // Logged before the startup line, so it is among the lines already read.
    assert!(
        server.seen.iter().any(|l| l.contains("reflection is on")),
        "{:#?}",
        server.seen
    );
}

#[tokio::test]
async fn reflection_off_by_default_is_unimplemented() {
    let mut server = Server::spawn("127.0.0.1:0");
    let startup = server.wait_for_line("backend on");
    let address = address_in(&startup);

    let status = list_services(&address)
        .await
        .expect_err("reflection must not be served");

    assert_eq!(status.code(), tonic::Code::Unimplemented, "{status:?}");
}
