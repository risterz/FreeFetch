use reqwest::Client;
use std::time::Duration;

#[derive(Clone)]
pub struct ExtractionContext {
    pub client: Client,
    pub user_agent: String,
    pub proxy: Option<String>,
    pub ytdlp_path: Option<String>,
}

impl Default for ExtractionContext {
    fn default() -> Self {
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".to_string();
        let client = crate::core::security::build_ssrf_safe_client(
            Duration::from_secs(15),
            &user_agent,
            None,
        )
        .unwrap_or_default();

        Self {
            client,
            user_agent,
            proxy: None,
            ytdlp_path: None,
        }
    }
}

impl ExtractionContext {
    pub fn new(proxy: Option<String>, ytdlp_path: Option<String>) -> Self {
        let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".to_string();
        let client = crate::core::security::build_ssrf_safe_client(
            Duration::from_secs(15),
            &user_agent,
            proxy.as_deref(),
        )
        .unwrap_or_default();

        Self {
            client,
            user_agent,
            proxy,
            ytdlp_path,
        }
    }
}
