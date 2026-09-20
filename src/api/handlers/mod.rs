use crate::cache::CacheManager;
use crate::config::Config;
use crate::registry::ModelRegistry;
use std::sync::Arc;
use std::time::Instant;

pub mod chat;
pub mod config;
pub mod health;
pub mod models;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub registry: Arc<ModelRegistry>,
    pub cache_manager: Arc<CacheManager>,
    pub start_time: Instant,
}

pub use chat::create_chat_completion;
pub use config::{get_config, reload_config};
pub use health::health_check;
pub use models::{get_model, list_models};
