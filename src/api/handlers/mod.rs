use crate::cache::CacheManager;
use crate::config::Config;
use crate::registry::ModelRegistry;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub mod chat;
pub mod config;
pub mod health;
pub mod models;

/// Shared application state.
/// `config` is wrapped in RwLock so that hot-reload can swap it at runtime.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub registry: Arc<ModelRegistry>,
    pub cache_manager: Arc<CacheManager>,
    pub start_time: Instant,
    /// Path that the server loaded its initial config from; used for reload.
    pub config_path: Option<String>,
    /// Single shared reqwest::Client (connection pool reuse, timeouts, TLS defaults).
    pub http_client: reqwest::Client,
}

impl AppState {
    /// Convenience accessor that takes a read lock and returns a cloned Config snapshot.
    pub async fn config_snapshot(&self) -> Config {
        self.config.read().await.clone()
    }
}

pub use chat::create_chat_completion;
pub use config::{get_config, reload_config};
pub use health::health_check;
pub use models::{get_model, list_models};
