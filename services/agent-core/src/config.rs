//! Service configuration read from the environment.
//! Covers the listen address, the optional OTLP endpoint and whether gRPC reflection is on.

use reqwest::Url;
use secrecy::SecretString;
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
    /// The value of an environment variable is not a valid u16.
    #[error("invalid u16 in {variable}: '{value}' ({reason})")]
    InvalidU16 {
        variable: &'static str,
        value: String,
        reason: String,
    },
    /// Missing required environment variable.
    #[error("missing required environment variable {variable}")]
    MissingVariable { variable: &'static str },
}

/// Parses `true`, `false`, `1` or `0`, ignoring case and surrounding whitespace.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
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
        let llm_api_key = lookup(ARIA_LLM_API_KEY_VAR)?
            .filter(|v| !is_blank(v))
            .ok_or(ConfigError::MissingVariable {
                variable: ARIA_LLM_API_KEY_VAR,
            })?;
        let llm_api_url = lookup(ARIA_LLM_BASE_URL_VAR)?
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
        let llm_api_model = lookup(ARIA_LLM_MODEL_VAR)?
            .filter(|v| !is_blank(v))
            .unwrap_or_else(|| ARIA_LLM_MODEL_DEFAULT.to_string());
        let llm_api_idle_timeout = lookup(ARIA_LLM_IDLE_TIMEOUT_VAR)?
            .filter(|v| !is_blank(v))
            .map(|v| {
                humantime::parse_duration(&v).map_err(|e| ConfigError::InvalidDuration {
                    variable: ARIA_LLM_IDLE_TIMEOUT_VAR,
                    value: v,
                    reason: e.to_string(),
                })
            })
            .transpose()?
            .unwrap_or_else(|| {
                humantime::parse_duration(ARIA_LLM_IDLE_TIMEOUT_DEFAULT)
                    .expect("default duration is valid")
            });
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

        if llm_api_idle_timeout <= humantime::parse_duration("0s").unwrap() {
            return Err(ConfigError::InvalidDuration {
                variable: ARIA_LLM_IDLE_TIMEOUT_VAR,
                value: humantime::format_duration(llm_api_idle_timeout).to_string(),
                reason: "must be greater than 0s".to_string(),
            });
        }

        Ok(Self {
            listen_address,
            otlp_endpoint,
            reflection,
            llm_api_key: llm_api_key.into(),
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

    fn lookup_from(
        listen: Option<&'static str>,
        otlp: Option<&'static str>,
    ) -> impl Fn(&'static str) -> Result<Option<String>, ConfigError> {
        move |name| {
            Ok(match name {
                LISTEN_ADDRESS_VAR => listen.map(str::to_string),
                OTLP_ENDPOINT_VAR => otlp.map(str::to_string),
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
}
