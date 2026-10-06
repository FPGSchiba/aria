//! Service configuration read from the environment.
//! Covers the listen address and the optional OTLP endpoint.

use std::env;
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

/// The port the service listens on when no listen address is configured.
pub const ARIA_AGENT_CORE_DEFAULT_PORT: u16 = 6517;
/// Environment variable holding the listen address.
pub const LISTEN_ADDRESS_VAR: &str = "ARIA_AGENT_CORE_LISTEN_ADDRESS";
/// Environment variable holding the OTLP endpoint.
pub const OTLP_ENDPOINT_VAR: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

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
    /// The OTLP endpoint is set but blank.
    #[error("invalid OTLP endpoint in {variable}: value is blank")]
    InvalidOtlpEndpoint { variable: &'static str },
    /// An environment variable holds bytes that are not valid Unicode.
    #[error("environment variable {variable} is not valid unicode")]
    NotUnicode { variable: &'static str },
}

/// The service configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: SocketAddr,
    pub otlp_endpoint: Option<String>,
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
    /// The listen address defaults to all interfaces on the default port; a blank OTLP endpoint
    /// is an error.
    pub fn from_lookup(
        lookup: impl Fn(&'static str) -> Result<Option<String>, ConfigError>,
    ) -> Result<Self, ConfigError> {
        let listen_address = match lookup(LISTEN_ADDRESS_VAR)? {
            Some(value) => {
                SocketAddr::from_str(&value).map_err(|e| ConfigError::InvalidListenAddress {
                    variable: LISTEN_ADDRESS_VAR,
                    value,
                    reason: e.to_string(),
                })?
            }
            None => SocketAddr::from((Ipv4Addr::UNSPECIFIED, ARIA_AGENT_CORE_DEFAULT_PORT)),
        };
        let otlp_endpoint = lookup(OTLP_ENDPOINT_VAR)?;
        if let Some(endpoint) = &otlp_endpoint
            && endpoint.trim().is_empty()
        {
            return Err(ConfigError::InvalidOtlpEndpoint {
                variable: OTLP_ENDPOINT_VAR,
            });
        }
        Ok(Self {
            listen_address,
            otlp_endpoint,
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
    fn blank_otlp_endpoint_is_a_config_error_naming_the_variable() {
        let err = Config::from_lookup(lookup_from(None, Some("   "))).expect_err("blank endpoint");

        assert!(err.to_string().contains(OTLP_ENDPOINT_VAR), "{err}");
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
}
