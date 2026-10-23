//! The client configuration for the provider: exactly what `ARIA_LLM_*` says and nothing from the
//! environment. async-openai's own `OpenAIConfig` reads `OPENAI_API_KEY`, `OPENAI_ADMIN_KEY`,
//! `OPENAI_BASE_URL`, `OPENAI_ORG_ID` and `OPENAI_PROJECT_ID`, which would send organisation and
//! project headers to the provider. This one implements the crate's `Config` trait instead.

use async_openai::config::Config;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

/// The provider's base URL and the key to authenticate with.
#[derive(Debug, Clone)]
pub(crate) struct ProviderConfig {
    api_base: String,
    api_key: SecretString,
}

impl ProviderConfig {
    /// A configuration for `api_base` (no trailing slash) authenticating with `api_key`.
    pub(crate) fn new(api_base: impl Into<String>, api_key: SecretString) -> Self {
        Self {
            api_base: api_base.into(),
            api_key,
        }
    }
}

impl Config for ProviderConfig {
    /// Only the `Authorization` header, marked sensitive so it is kept out of debug output and
    /// header compression. The configuration has already checked that the key forms a valid
    /// header value; if it did not, no header is sent and the provider answers 401, instead of a
    /// panic on every turn.
    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(mut value) =
            HeaderValue::from_str(&format!("Bearer {}", self.api_key.expose_secret()))
        {
            value.set_sensitive(true);
            headers.insert(AUTHORIZATION, value);
        }
        headers
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    fn query(&self) -> Vec<(&str, &str)> {
        Vec::new()
    }

    fn api_base(&self) -> &str {
        &self.api_base
    }

    fn api_key(&self) -> &SecretString {
        &self.api_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-provider-config-key";

    fn config(key: &str) -> ProviderConfig {
        ProviderConfig::new("https://llm.example.test/v1", SecretString::from(key))
    }

    #[test]
    fn the_only_header_is_a_sensitive_bearer_authorization() {
        let headers = config(KEY).headers();

        assert_eq!(headers.len(), 1);
        let value = &headers[AUTHORIZATION];
        assert_eq!(value, &format!("Bearer {KEY}"));
        assert!(value.is_sensitive());
    }

    #[test]
    fn no_organisation_or_project_header_is_ever_sent() {
        let headers = config(KEY).headers();

        assert!(headers.get("openai-organization").is_none());
        assert!(headers.get("openai-project").is_none());
    }

    #[test]
    fn a_key_that_cannot_form_a_header_sends_none_instead_of_panicking() {
        assert!(config("bad\nkey").headers().is_empty());
    }

    #[test]
    fn the_route_is_joined_to_the_base_without_a_doubled_slash() {
        assert_eq!(
            config(KEY).url("/chat/completions"),
            "https://llm.example.test/v1/chat/completions"
        );
        assert_eq!(config(KEY).api_base(), "https://llm.example.test/v1");
        assert!(config(KEY).query().is_empty());
    }

    #[test]
    fn debug_formatting_never_contains_the_key() {
        assert!(!format!("{:?}", config(KEY)).contains(KEY));
    }
}
