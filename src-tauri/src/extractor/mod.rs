pub mod context;
pub mod implementations;
pub mod models;

use async_trait::async_trait;
use std::sync::Arc;
use url::Url;

use crate::core::error::FreeFetchError;
pub use context::ExtractionContext;
pub use models::{MediaFormat, MediaMetadata, ParseResult, StreamManifest};

#[async_trait]
pub trait Extractor: Send + Sync {
    fn name(&self) -> &'static str;
    fn can_handle(&self, url: &Url) -> bool;
    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError>;

    async fn get_subtitles(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<Vec<crate::core::types::SubtitleTrack>, FreeFetchError> {
        let (_meta, manifest) = self.extract(url, ctx).await?;
        Ok(manifest.subtitles)
    }
}

#[derive(Clone)]
pub struct ExtractorRegistry {
    extractors: Vec<Arc<dyn Extractor>>,
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        let extractors: Vec<Arc<dyn Extractor>> = vec![
            Arc::new(implementations::youtube::YoutubeExtractor),
            Arc::new(implementations::generic::GenericExtractor),
        ];
        Self { extractors }
    }

    pub fn register(&mut self, extractor: Arc<dyn Extractor>) {
        self.extractors.insert(0, extractor);
    }

    pub fn find_extractor(&self, url: &Url) -> Option<Arc<dyn Extractor>> {
        self.extractors.iter().find(|e| e.can_handle(url)).cloned()
    }
}
