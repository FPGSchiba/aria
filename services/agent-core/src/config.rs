//! Service configuration read from the environment.
//! Covers the listen address, the optional OTLP endpoint, whether gRPC reflection is on, and the
//! five settings of the OpenAI-compatible LLM backend (`ARIA_LLM_API_KEY`, which is required, and
//! `ARIA_LLM_BASE_URL`, `ARIA_LLM_MODEL`, `ARIA_LLM_IDLE_TIMEOUT` and `ARIA_LLM_MAX_RETRIES`).

use reqwest::Url;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};
use std::env;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

/// The port the service listens on when no listen address is configured.
pub const ARIA_AGENT_CORE_DEFAULT_PORT: u16 = 6517;
/// Environment variable holding the listen address.
pub const LISTEN_ADDRESS_VAR: &str = "ARIA_AGENT_CORE_LISTEN_ADDRESS";
/// Environment variable holding the OTLP endpoint.
pub const OTLP_ENDPOINT_VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
/// Environment variable switching gRPC server reflection on.
pub const REFLECTION_VAR: &str = "ARIA_AGENT_CORE_REFLECTION";
/// Environment variable holding the API key for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_API_KEY_VAR: &str = "ARIA_LLM_API_KEY";
/// Environment variable holding the base URL for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_BASE_URL_VAR: &str = "ARIA_LLM_BASE_URL";
/// Environment variable holding the model name for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_MODEL_VAR: &str = "ARIA_LLM_MODEL";
/// Environment variable holding the idle timeout for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_IDLE_TIMEOUT_VAR: &str = "ARIA_LLM_IDLE_TIMEOUT";
/// Environment variable holding the maximum number of retries for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_MAX_RETRIES_VAR: &str = "ARIA_LLM_MAX_RETRIES";

/// The default base URL for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_BASE_URL_DEFAULT: &str = "https://api.cerebras.ai/v1";
/// The default model name for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_MODEL_DEFAULT: &str = "qwen-3.8-27b";
/// The default idle timeout for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_IDLE_TIMEOUT_DEFAULT: &str = "30s";
/// The default maximum number of retries for the OpenAI-compatible LLM backend.
pub const ARIA_LLM_MAX_RETRIES_DEFAULT: u16 = 3;

/// Why the configuration could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The listen address is not a valid socket address.
    #[error("invalid listen address in {variable}: '{value}' ({reason})")]
    InvalidListenAddress {
        variable: &'static str,
        value: String,
        reason: String,
    },
    /// The reflection switch is not one of `true`, `false`, `1` or `0`.
    #[error("invalid boolean in {variable}: '{value}' (use true, false, 1 or 0)")]
    InvalidBool {
        variable: &'static str,
        value: String,
    },
    /// An environment variable holds bytes that are not valid Unicode.
    #[error("environment variable {variable} is not valid unicode")]
    NotUnicode { variable: &'static str },
    /// The value of an environment variable is not a valid URL.
    #[error("invalid URL in {variable}: '{value}' ({reason})")]
    InvalidUrl {
        variable: &'static str,
        value: String,
        reason: String,
    },
    /// The value of an environment variable is not a valid duration.
    #[error("invalid duration in {variable}: '{value}' ({reason})")]
    InvalidDuration {
        variable: &'static str,
        value: String,
        reason: String,
    },
    /// The value of an environment variable is not a whole number from 0 to 65535.
    #[error("{variable} must be a whole number from 0 to 65535, got '{value}' ({reason})")]
    InvalidU16 {
        variable: &'static str,
        value: String,
        reason: String,
    },
    /// Missing required environment variable.
    #[error("missing required environment variable {variable}")]
    MissingVariable { variable: &'static str },
    /// A secret cannot be used as given. The value is deliberately not part of the error.
    #[error(
        "{variable} is not a usable key: after trimming it must be a valid HTTP header value, \
         with no control characters or line breaks"
    )]
    InvalidSecret { variable: &'static str },
}

/// Parses `true`, `false`, `1` or `0`, ignoring case and surrounding whitespace.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// The key without the whitespace around it, which is easily pasted in (a trailing newline from
/// `echo` or a secret file).
fn trimmed_key(key: SecretString) -> SecretString {
    SecretString::from(key.expose_secret().trim())
}

/// Whether an environment value is empty or only whitespace.
fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

/// The service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    listen_address: SocketAddr,
    otlp_endpoint: Option<String>,
    reflection: bool,
    llm_api_key: SecretString,
    llm_url: Url,
    llm_model: String,
    llm_idle_timeout: Duration,
    llm_max_retries: u16,
}

impl Config {
    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| match env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode { variable: name }),
        })
    }

    /// Builds the configuration from a lookup of variables by name (`Ok(None)` means unset).
    /// A blank value (empty or only whitespace) counts as unset. The listen address defaults to all
    /// interfaces on the default port, no OTLP endpoint means span export is off, and reflection
    /// is off unless set to `true` or `1`.
    ///
    /// The LLM backend needs `ARIA_LLM_API_KEY`; a missing or blank key is an error naming the
    /// variable and the key is never echoed. The base URL, model, idle timeout (a duration with a
    /// unit, such as `30s`, greater than zero) and retry count (0 to 65535) fall back to their
    /// defaults when unset or blank, and an invalid value is an error naming the variable and the
    /// value. Trailing slashes are dropped from the base URL's path.
    pub fn from_lookup(
        lookup: impl Fn(&'static str) -> Result<Option<String>, ConfigError>,
    ) -> Result<Self, ConfigError> {
        let listen_address = match lookup(LISTEN_ADDRESS_VAR)?.filter(|v| !is_blank(v)) {
            Some(value) => {
                SocketAddr::from_str(&value).map_err(|e| ConfigError::InvalidListenAddress {
                    variable: LISTEN_ADDRESS_VAR,
                    value,
                    reason: e.to_string(),
                })?
            }
            None => SocketAddr::from((Ipv4Addr::UNSPECIFIED, ARIA_AGENT_CORE_DEFAULT_PORT)),
        };
        let otlp_endpoint = lookup(OTLP_ENDPOINT_VAR)?.filter(|v| !is_blank(v));
        let reflection = match lookup(REFLECTION_VAR)?.filter(|v| !is_blank(v)) {
            Some(value) => parse_bool(&value).ok_or(ConfigError::InvalidBool {
                variable: REFLECTION_VAR,
                value,
            })?,
            None => false,
        };
        // Wrapped as soon as it is read, so the plain text is never held in a `String` here.
        let llm_api_key = lookup(ARIA_LLM_API_KEY_VAR)?
            .map(SecretString::from)
            .filter(|key| !is_blank(key.expose_secret()))
            .map(trimmed_key)
            .ok_or(ConfigError::MissingVariable {
                variable: ARIA_LLM_API_KEY_VAR,
            })?;
        // Checked once here, so the client never fails building its header on every turn.
        if HeaderValue::from_str(&format!("Bearer {}", llm_api_key.expose_secret())).is_err() {
            return Err(ConfigError::InvalidSecret {
                variable: ARIA_LLM_API_KEY_VAR,
            });
        }
        let mut llm_api_url = lookup(ARIA_LLM_BASE_URL_VAR)?
            .filter(|v| !is_blank(v))
            .map(|v| {
                Url::parse(&v).map_err(|e| ConfigError::InvalidUrl {
                    variable: ARIA_LLM_BASE_URL_VAR,
                    value: v,
                    reason: e.to_string(),
                })
            })
            .transpose()?
            .unwrap_or_else(|| {
                Url::parse(ARIA_LLM_BASE_URL_DEFAULT).expect("default base URL is valid")
            });
        // `https://host/v1/` and `https://host/v1` are the same base; a trailing slash would
        // otherwise make the route `.../v1//chat/completions`.
        let trimmed_path = llm_api_url.path().trim_end_matches('/').to_string();
        llm_api_url.set_path(&trimmed_path);
        let llm_api_model = lookup(ARIA_LLM_MODEL_VAR)?
            .filter(|v| !is_blank(v))
            .unwrap_or_else(|| ARIA_LLM_MODEL_DEFAULT.to_string());
        let llm_api_idle_timeout = match lookup(ARIA_LLM_IDLE_TIMEOUT_VAR)?.filter(|v| !is_blank(v))
        {
            Some(raw) => {
                let invalid = |reason: String| ConfigError::InvalidDuration {
                    variable: ARIA_LLM_IDLE_TIMEOUT_VAR,
                    value: raw.clone(),
                    reason,
                };
                let timeout =
                    humantime::parse_duration(&raw).map_err(|e| invalid(e.to_string()))?;
                if timeout == Duration::ZERO {
                    return Err(invalid("must be greater than 0s".to_string()));
                }
                timeout
            }
            None => humantime::parse_duration(ARIA_LLM_IDLE_TIMEOUT_DEFAULT)
                .expect("default duration is valid"),
        };
        let llm_api_max_retries = lookup(ARIA_LLM_MAX_RETRIES_VAR)?
            .filter(|v| !is_blank(v))
            .map(|v| {
                v.parse::<u16>().map_err(|e| ConfigError::InvalidU16 {
                    variable: ARIA_LLM_MAX_RETRIES_VAR,
                    value: v,
                    reason: e.to_string(),
                })
            })
            .transpose()?
            .unwrap_or(ARIA_LLM_MAX_RETRIES_DEFAULT);

        Ok(Self {
            listen_address,
            otlp_endpoint,
            reflection,
            llm_api_key,
            llm_url: llm_api_url,
            llm_model: llm_api_model,
            llm_idle_timeout: llm_api_idle_timeout,
            llm_max_retries: llm_api_max_retries,
        })
    }

    /// The address to listen on.
    pub fn listen_address(&self) -> SocketAddr {
        self.listen_address
    }

    /// The OTLP endpoint to export spans to, if configured.
    pub fn otlp_endpoint(&self) -> Option<&str> {
        self.otlp_endpoint.as_deref()
    }

    /// Whether to serve gRPC server reflection. Off unless switched on.
    pub fn reflection(&self) -> bool {
        self.reflection
    }

    /// The API key for the OpenAI-compatible LLM backend.
    pub fn llm_api_key(&self) -> &SecretString {
        &self.llm_api_key
    }

    /// The base URL for the OpenAI-compatible LLM backend.
    pub fn llm_url(&self) -> &Url {
        &self.llm_url
    }

    /// The host of the base URL, for logs and traces; "unknown" for a URL without one.
    pub fn llm_host(&self) -> &str {
        self.llm_url.host_str().unwrap_or("unknown")
    }

    /// The port of the base URL, or the scheme's default (443 for https, 80 for http).
    pub fn llm_port(&self) -> Option<u16> {
        self.llm_url.port_or_known_default()
    }

    /// The model name for the OpenAI-compatible LLM backend.
    pub fn llm_model(&self) -> &str {
        &self.llm_model
    }

    /// The idle timeout for the OpenAI-compatible LLM backend.
    pub fn llm_idle_timeout(&self) -> Duration {
        self.llm_idle_timeout
    }

    /// The maximum number of retries for the OpenAI-compatible LLM backend.
    pub fn llm_max_retries(&self) -> u16 {
        self.llm_max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    /// The key every test that is not about the LLM settings supplies, so the configuration builds.
    const TEST_KEY: &str = "test-key";

    fn lookup_from(
        listen: Option<&'static str>,
        otlp: Option<&'static str>,
    ) -> impl Fn(&'static str) -> Result<Option<String>, ConfigError> {
        move |name| {
            Ok(match name {
                LISTEN_ADDRESS_VAR => listen.map(str::to_string),
                OTLP_ENDPOINT_VAR => otlp.map(str::to_string),
                ARIA_LLM_API_KEY_VAR => Some(TEST_KEY.to_string()),
                _ => None,
            })
        }
    }

    #[test]
    fn no_otlp_endpoint_means_export_off() {
        let config = Config::from_lookup(lookup_from(None, None)).expect("config");

        assert_eq!(config.otlp_endpoint(), None);
    }

    #[test]
    fn otlp_endpoint_set_means_export_on_at_that_endpoint() {
        let config =
            Config::from_lookup(lookup_from(None, Some("http://collector:4317"))).expect("config");

        assert_eq!(config.otlp_endpoint(), Some("http://collector:4317"));
    }

    #[test]
    fn blank_otlp_endpoint_means_export_off() {
        let config = Config::from_lookup(lookup_from(None, Some("   "))).expect("config");

        assert_eq!(config.otlp_endpoint(), None);
    }

    #[test]
    fn blank_listen_address_uses_the_default() {
        let config = Config::from_lookup(lookup_from(Some(""), None)).expect("config");

        assert_eq!(
            config.listen_address(),
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, ARIA_AGENT_CORE_DEFAULT_PORT))
        );
    }

    #[test]
    fn unset_listen_address_uses_the_default() {
        let config = Config::from_lookup(lookup_from(None, None)).expect("config");

        assert_eq!(
            config.listen_address(),
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, ARIA_AGENT_CORE_DEFAULT_PORT))
        );
    }

    #[test]
    fn valid_listen_address_is_used() {
        let config =
            Config::from_lookup(lookup_from(Some("127.0.0.1:9000"), None)).expect("config");

        assert_eq!(config.listen_address(), "127.0.0.1:9000".parse().unwrap());
    }

    #[test]
    fn unparsable_listen_address_is_a_config_error_naming_variable_and_value() {
        let err = Config::from_lookup(lookup_from(Some("not-an-address"), None))
            .expect_err("bad address");

        assert!(
            matches!(err, ConfigError::InvalidListenAddress { .. }),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(message.contains(LISTEN_ADDRESS_VAR), "{message}");
        assert!(message.contains("not-an-address"), "{message}");
    }

    #[test]
    fn lookup_error_such_as_non_unicode_value_propagates() {
        let err = Config::from_lookup(|name| Err(ConfigError::NotUnicode { variable: name }))
            .expect_err("lookup failure");

        assert!(matches!(err, ConfigError::NotUnicode { .. }), "{err:?}");
        assert!(err.to_string().contains(LISTEN_ADDRESS_VAR), "{err}");
    }

    fn reflection_from(value: Option<&'static str>) -> Result<Config, ConfigError> {
        Config::from_lookup(move |name| {
            Ok(match name {
                REFLECTION_VAR => value.map(str::to_string),
                ARIA_LLM_API_KEY_VAR => Some(TEST_KEY.to_string()),
                _ => None,
            })
        })
    }

    #[test]
    fn unset_reflection_is_off() {
        assert!(!reflection_from(None).expect("config").reflection());
    }

    #[test]
    fn blank_reflection_is_off() {
        assert!(!reflection_from(Some("  ")).expect("config").reflection());
    }

    #[test]
    fn true_and_one_switch_reflection_on_ignoring_case() {
        for value in ["true", "TRUE", "True", "1", " true "] {
            assert!(
                reflection_from(Some(value)).expect("config").reflection(),
                "{value}"
            );
        }
    }

    #[test]
    fn false_and_zero_switch_reflection_off_ignoring_case() {
        for value in ["false", "FALSE", "0"] {
            assert!(
                !reflection_from(Some(value)).expect("config").reflection(),
                "{value}"
            );
        }
    }

    #[test]
    fn invalid_reflection_value_is_a_config_error_naming_variable_and_value() {
        let err = reflection_from(Some("yes")).expect_err("invalid bool");

        assert!(matches!(err, ConfigError::InvalidBool { .. }), "{err:?}");
        let message = err.to_string();
        assert!(message.contains(REFLECTION_VAR), "{message}");
        assert!(message.contains("yes"), "{message}");
    }

    // ------------------------------------------------------------ LLM settings (C1)

    /// A lookup over fixed `(variable, value)` pairs; anything not listed is unset.
    fn lookup_pairs(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&'static str) -> Result<Option<String>, ConfigError> {
        move |name| {
            Ok(pairs
                .iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| value.to_string()))
        }
    }

    /// A key that no message or debug output may ever contain.
    const SECRET: &str = "sk-very-secret-key-value";

    #[test]
    fn with_only_the_key_set_the_llm_settings_take_their_documented_defaults() {
        let config =
            Config::from_lookup(lookup_pairs(&[(ARIA_LLM_API_KEY_VAR, SECRET)])).expect("config");

        assert_eq!(config.llm_api_key().expose_secret(), SECRET);
        assert_eq!(config.llm_url().as_str(), "https://api.cerebras.ai/v1");
        assert_eq!(config.llm_model(), "qwen-3.8-27b");
        assert_eq!(config.llm_idle_timeout(), Duration::from_secs(30));
        assert_eq!(config.llm_max_retries(), 3);
    }

    #[test]
    fn the_default_constants_describe_the_defaults() {
        assert_eq!(ARIA_LLM_BASE_URL_DEFAULT, "https://api.cerebras.ai/v1");
        assert_eq!(ARIA_LLM_MODEL_DEFAULT, "qwen-3.8-27b");
        assert_eq!(ARIA_LLM_IDLE_TIMEOUT_DEFAULT, "30s");
        assert_eq!(ARIA_LLM_MAX_RETRIES_DEFAULT, 3);
    }

    #[test]
    fn an_unset_key_is_a_missing_variable_error_naming_the_key_variable() {
        let err = Config::from_lookup(lookup_pairs(&[])).expect_err("no key");

        assert!(
            matches!(err, ConfigError::MissingVariable { variable } if variable == "ARIA_LLM_API_KEY"),
            "{err:?}"
        );
        assert!(err.to_string().contains("ARIA_LLM_API_KEY"), "{err}");
    }

    #[test]
    fn a_blank_key_is_a_missing_variable_error_naming_the_key_variable() {
        for blank in ["", "   ", "\t"] {
            let lookup = move |name: &'static str| {
                Ok((name == ARIA_LLM_API_KEY_VAR).then(|| blank.to_string()))
            };

            let err = Config::from_lookup(lookup).expect_err("blank key");

            assert!(
                matches!(err, ConfigError::MissingVariable { .. }),
                "{blank:?}: {err:?}"
            );
            assert!(err.to_string().contains("ARIA_LLM_API_KEY"), "{err}");
        }
    }

    /// The key as the configuration holds it, or the error for the given raw value.
    fn key_from(raw: &'static str) -> Result<Config, ConfigError> {
        Config::from_lookup(move |name| Ok((name == ARIA_LLM_API_KEY_VAR).then(|| raw.to_string())))
    }

    #[test]
    fn whitespace_around_the_key_is_trimmed() {
        for raw in ["sk-abc\n", "sk-abc\r\n", "  sk-abc", "\tsk-abc \n\n"] {
            let config = key_from(raw).expect("a trimmed key is accepted");

            assert_eq!(config.llm_api_key().expose_secret(), "sk-abc", "{raw:?}");
        }
    }

    #[test]
    fn a_key_that_cannot_be_a_header_value_is_rejected_without_echoing_it() {
        for raw in ["sk-ab\ncd", "sk-ab\rcd", "sk-\u{7}abc", "sk-ab\u{7f}cd"] {
            let err = key_from(raw).expect_err("an unusable key");

            assert!(
                matches!(err, ConfigError::InvalidSecret { variable } if variable == "ARIA_LLM_API_KEY"),
                "{raw:?}: {err:?}"
            );
            let message = format!("{err} {err:?}");
            assert!(message.contains("ARIA_LLM_API_KEY"), "{message}");
            for part in ["sk-", "abc", "cd"] {
                assert!(
                    !message.contains(part),
                    "{raw:?} leaked {part:?}: {message}"
                );
            }
        }
    }

    #[test]
    fn a_whitespace_only_key_is_missing() {
        for raw in [" ", "\n", " \t\r\n "] {
            let err = key_from(raw).expect_err("blank");

            assert!(
                matches!(err, ConfigError::MissingVariable { variable } if variable == "ARIA_LLM_API_KEY"),
                "{raw:?}: {err:?}"
            );
        }
    }

    #[test]
    fn each_optional_llm_setting_is_used_when_set() {
        let config = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_BASE_URL_VAR, "https://llm.example.test/v2"),
            (ARIA_LLM_MODEL_VAR, "other-model"),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "45s"),
            (ARIA_LLM_MAX_RETRIES_VAR, "5"),
        ]))
        .expect("config");

        assert_eq!(config.llm_url().as_str(), "https://llm.example.test/v2");
        assert_eq!(config.llm_model(), "other-model");
        assert_eq!(config.llm_idle_timeout(), Duration::from_secs(45));
        assert_eq!(config.llm_max_retries(), 5);
    }

    #[test]
    fn the_idle_timeout_accepts_humantime_units() {
        let config = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "1m 30s"),
        ]))
        .expect("config");

        assert_eq!(config.llm_idle_timeout(), Duration::from_secs(90));
    }

    #[test]
    fn blank_optional_llm_settings_fall_back_to_their_defaults() {
        let config = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_BASE_URL_VAR, "  "),
            (ARIA_LLM_MODEL_VAR, ""),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, " "),
            (ARIA_LLM_MAX_RETRIES_VAR, "   "),
        ]))
        .expect("config");

        assert_eq!(config.llm_url().as_str(), "https://api.cerebras.ai/v1");
        assert_eq!(config.llm_model(), "qwen-3.8-27b");
        assert_eq!(config.llm_idle_timeout(), Duration::from_secs(30));
        assert_eq!(config.llm_max_retries(), 3);
    }

    #[test]
    fn an_unparsable_idle_timeout_is_an_error_naming_variable_and_value() {
        let err = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "soon"),
        ]))
        .expect_err("bad timeout");

        assert!(
            matches!(err, ConfigError::InvalidDuration { .. }),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(message.contains("ARIA_LLM_IDLE_TIMEOUT"), "{message}");
        assert!(message.contains("soon"), "{message}");
    }

    #[test]
    fn a_bare_number_idle_timeout_has_no_unit_and_is_rejected() {
        let err = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "30"),
        ]))
        .expect_err("a bare number is not a duration");

        assert!(
            matches!(err, ConfigError::InvalidDuration { .. }),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(message.contains("ARIA_LLM_IDLE_TIMEOUT"), "{message}");
        assert!(message.contains("30"), "{message}");
    }

    #[test]
    fn a_zero_idle_timeout_is_rejected_naming_the_variable() {
        for zero in ["0s", "0ms"] {
            let lookup = move |name: &'static str| {
                Ok(match name {
                    ARIA_LLM_API_KEY_VAR => Some(SECRET.to_string()),
                    ARIA_LLM_IDLE_TIMEOUT_VAR => Some(zero.to_string()),
                    _ => None,
                })
            };

            let err = Config::from_lookup(lookup).expect_err("zero timeout");

            assert!(
                matches!(err, ConfigError::InvalidDuration { .. }),
                "{zero}: {err:?}"
            );
            let message = err.to_string();
            assert!(message.contains("ARIA_LLM_IDLE_TIMEOUT"), "{message}");
            // The value the operator wrote, not a reformatted duration such as "0ns".
            assert!(message.contains(&format!("'{zero}'")), "{message}");
        }
    }

    #[test]
    fn a_negative_idle_timeout_is_rejected() {
        let err = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "-5s"),
        ]))
        .expect_err("negative timeout");

        assert!(
            matches!(err, ConfigError::InvalidDuration { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("-5s"), "{err}");
    }

    #[test]
    fn an_unparsable_retry_count_is_an_error_naming_variable_and_value() {
        for bad in ["many", "-1", "70000", "1.5"] {
            let lookup = move |name: &'static str| {
                Ok(match name {
                    ARIA_LLM_API_KEY_VAR => Some(SECRET.to_string()),
                    ARIA_LLM_MAX_RETRIES_VAR => Some(bad.to_string()),
                    _ => None,
                })
            };

            let err = Config::from_lookup(lookup).expect_err("bad retry count");

            assert!(
                matches!(err, ConfigError::InvalidU16 { .. }),
                "{bad}: {err:?}"
            );
            let message = err.to_string();
            assert!(message.contains("ARIA_LLM_MAX_RETRIES"), "{message}");
            assert!(message.contains(bad), "{message}");
            assert!(
                message.contains("whole number from 0 to 65535"),
                "{message}"
            );
        }
    }

    #[test]
    fn zero_retries_is_accepted() {
        let config = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_MAX_RETRIES_VAR, "0"),
        ]))
        .expect("zero retries means one attempt");

        assert_eq!(config.llm_max_retries(), 0);
    }

    /// The configured base URL as the backend will use it.
    fn base_url_from(value: &'static str) -> Url {
        let lookup = move |name: &'static str| {
            Ok(match name {
                ARIA_LLM_API_KEY_VAR => Some(SECRET.to_string()),
                ARIA_LLM_BASE_URL_VAR => Some(value.to_string()),
                _ => None,
            })
        };
        Config::from_lookup(lookup)
            .expect("config")
            .llm_url()
            .clone()
    }

    #[test]
    fn trailing_slashes_on_the_base_url_path_are_dropped() {
        for (given, expected) in [
            (
                "https://llm.example.test/v1/",
                "https://llm.example.test/v1",
            ),
            (
                "https://llm.example.test/v1///",
                "https://llm.example.test/v1",
            ),
            (
                "https://llm.example.test/a/b/",
                "https://llm.example.test/a/b",
            ),
            ("https://llm.example.test/v1", "https://llm.example.test/v1"),
        ] {
            assert_eq!(base_url_from(given).as_str(), expected, "{given}");
        }
    }

    #[test]
    fn a_bare_host_base_url_has_an_empty_path_so_the_backend_can_drop_the_root_slash() {
        let url = base_url_from("http://localhost:11434/");

        assert_eq!(url.path(), "/");
        // A URL cannot render an empty path, so the consumer trims the root slash it prints.
        assert_eq!(url.as_str().trim_end_matches('/'), "http://localhost:11434");
    }

    #[test]
    fn an_invalid_base_url_is_an_error_naming_variable_and_value() {
        let err = Config::from_lookup(lookup_pairs(&[
            (ARIA_LLM_API_KEY_VAR, SECRET),
            (ARIA_LLM_BASE_URL_VAR, "not a url"),
        ]))
        .expect_err("bad url");

        assert!(matches!(err, ConfigError::InvalidUrl { .. }), "{err:?}");
        let message = err.to_string();
        assert!(message.contains("ARIA_LLM_BASE_URL"), "{message}");
        assert!(message.contains("not a url"), "{message}");
    }

    #[test]
    fn debug_formatting_a_configuration_never_contains_the_key() {
        let config =
            Config::from_lookup(lookup_pairs(&[(ARIA_LLM_API_KEY_VAR, SECRET)])).expect("config");

        let compact = format!("{config:?}");
        let pretty = format!("{config:#?}");

        assert!(!compact.contains(SECRET), "{compact}");
        assert!(!pretty.contains(SECRET), "{pretty}");
    }

    #[test]
    fn no_error_message_echoes_the_key() {
        // Every other setting is wrong in turn while a real key is present.
        let bad_settings = [
            (ARIA_LLM_BASE_URL_VAR, "not a url"),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "soon"),
            (ARIA_LLM_IDLE_TIMEOUT_VAR, "0s"),
            (ARIA_LLM_MAX_RETRIES_VAR, "many"),
        ];
        for (variable, value) in bad_settings {
            let lookup = move |name: &'static str| {
                Ok(if name == ARIA_LLM_API_KEY_VAR {
                    Some(SECRET.to_string())
                } else if name == variable {
                    Some(value.to_string())
                } else {
                    None
                })
            };

            let err = Config::from_lookup(lookup).expect_err("bad setting");

            assert!(!err.to_string().contains(SECRET), "{err}");
            assert!(!format!("{err:?}").contains(SECRET), "{err:?}");
        }
    }
}
