use crate::api::handlers::{
    create_chat_completion, get_config, get_model, health_check, list_models, reload_config,
};
use crate::api::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/config", get(get_config))
        .route("/config/reload", post(reload_config))
        .route("/v1/models", get(list_models))
        .route("/v1/models/:model_id", get(get_model))
        .route("/v1/chat/completions", post(create_chat_completion))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheManager;
    use crate::config::Config;
    use crate::registry::ModelRegistry;

    #[test]
    fn test_create_router() {
        let config = Arc::new(RwLock::new(Config::default()));
        let registry = Arc::new(ModelRegistry::new());
        let cache_manager = Arc::new(CacheManager::new(crate::cache::CacheConfig::default()));
        let state = AppState {
            config,
            registry,
            cache_manager,
            start_time: Instant::now(),
            config_path: None,
            http_client: reqwest::Client::new(),
        };

        let router = create_router(state);
        assert!(true);
    }
}
