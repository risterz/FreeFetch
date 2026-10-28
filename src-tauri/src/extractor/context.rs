use reqwest::Client;

#[derive(Clone)]
pub struct ExtractionContext {
    pub client: Client,
    pub user_agent: String,
    pub proxy: Option<String>,
}

impl Default for ExtractionContext {
    fn default() -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            client,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".to_string(),
            proxy: None,
        }
    }
}
