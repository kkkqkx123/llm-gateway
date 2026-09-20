use crate::api::handlers::{
    create_chat_completion, get_config, get_model, health_check, list_models, reload_config,
};
use crate::api::AppState;
use axum::{
    routing::{get, post},
    Router,
};

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

    #[test]
    fn test_create_router() {
        let config = std::sync::Arc::new(crate::config::Config::default());
        let registry = std::sync::Arc::new(crate::registry::ModelRegistry::new());
        let cache_manager = std::sync::Arc::new(crate::cache::CacheManager::new(
            crate::cache::CacheConfig::default(),
        ));
        let state = crate::api::handlers::AppState {
            config,
            registry,
            cache_manager,
            start_time: std::time::Instant::now(),
        };

        let router = create_router(state);
        assert!(true);
    }
}
