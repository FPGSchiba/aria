//! Service configuration read from the environment.
//! Covers the listen address, the optional OTLP endpoint and whether gRPC reflection is on.

use std::env;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

/// The port the service listens on when no listen address is configured.
pub const ARIA_AGENT_CORE_DEFAULT_PORT: u16 = 6517;
/// Environment variable holding the listen address.
pub const LISTEN_ADDRESS_VAR: &str = "ARIA_AGENT_CORE_LISTEN_ADDRESS";
/// Environment variable holding the OTLP endpoint.
pub const OTLP_ENDPOINT_VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
/// Environment variable switching gRPC server reflection on.
pub const REFLECTION_VAR: &str = "ARIA_AGENT_CORE_REFLECTION";

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
        Ok(Self {
            listen_address,
            otlp_endpoint,
            reflection,
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
