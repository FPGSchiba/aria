//! The built binary, run as a process: startup log, one `Decide`, graceful SIGTERM exit, and a
//! readable failure for bad configuration.
#![cfg(unix)]

use proto::agent_core::v1::agent_core_client::AgentCoreClient;
use proto::agent_core::v1::{DecideRequest, decide_response};
use std::io::Read;
use std::net::TcpListener;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio_stream::StreamExt;

/// Upper bound for anything that could hang, so a failure reports instead of blocking.
const LIMIT: Duration = Duration::from_secs(20);

/// The spawned binary; killed on drop so a failing test leaves no process behind.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawns the binary with only the listen address set, so there is no OTLP endpoint.
fn spawn(listen_address: &str) -> Server {
    let child = Command::new(env!("CARGO_BIN_EXE_agent-core"))
        .env("ARIA_AGENT_CORE_LISTEN_ADDRESS", listen_address)
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .env_remove("RUST_LOG")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the binary");
    Server(child)
}

/// A free local port, found by binding and releasing it.
fn free_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("local addr").to_string()
}

/// Waits for the process to exit, polling `try_wait` until `LIMIT`.
async fn wait_for_exit(server: &mut Server) -> ExitStatus {
    let deadline = Instant::now() + LIMIT;
    loop {
        if let Some(status) = server.0.try_wait().expect("try_wait") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "the process did not exit in time"
        );
        sleep(Duration::from_millis(20)).await;
    }
}

fn read_all(reader: &mut impl Read) -> String {
    let mut out = String::new();
    reader.read_to_string(&mut out).expect("read output");
    out
}

#[tokio::test]
async fn binary_serves_echoed_decide_and_exits_zero_on_sigterm() {
    let address = free_address();
    let mut server = spawn(&address);

    // The server needs a moment to bind; retry until it accepts, bounded by `LIMIT`.
    let deadline = Instant::now() + LIMIT;
    let mut client = loop {
        match AgentCoreClient::connect(format!("http://{address}")).await {
            Ok(client) => break client,
            Err(e) => {
                assert!(Instant::now() < deadline, "never connected: {e}");
                sleep(Duration::from_millis(50)).await;
            }
        }
    };

    let mut stream = tokio::time::timeout(
        LIMIT,
        client.decide(DecideRequest {
            session_id: "binary-1".to_string(),
            speaker_name: None,
            text: "hello aria".to_string(),
        }),
    )
    .await
    .expect("decide timed out")
    .expect("decide")
    .into_inner();

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
    assert!(deltas >= 2, "expected at least two chunks, got {deltas}");
    assert_eq!(echoed, "hello aria");
    assert_eq!(completes, 1);

    let pid = server.0.id().to_string();
    let killed = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .expect("run kill");
    assert!(killed.success());
    let status = wait_for_exit(&mut server).await;
    assert_eq!(status.code(), Some(0), "{status:?}");

    let stdout = read_all(server.0.stdout.as_mut().expect("stdout"));
    assert!(stdout.contains("'echo' backend"), "{stdout}");
    assert!(stdout.contains("OTLP tracing disabled"), "{stdout}");
}

#[tokio::test]
async fn invalid_listen_address_exits_non_zero_with_a_readable_message() {
    let mut server = spawn("not-an-address");

    let status = wait_for_exit(&mut server).await;

    assert_eq!(status.code(), Some(1), "{status:?}");
    let stderr = read_all(server.0.stderr.as_mut().expect("stderr"));
    assert!(stderr.contains("invalid listen address"), "{stderr}");
    assert!(
        stderr.contains("ARIA_AGENT_CORE_LISTEN_ADDRESS"),
        "{stderr}"
    );
    assert!(stderr.contains("not-an-address"), "{stderr}");
}
