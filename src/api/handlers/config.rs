use crate::api::{handlers::AppState, ApiError, ApiResult};
use crate::config::{Config, ConfigError};
use crate::types::{ConfigResponse, ReloadConfigResponse, RoutingInfo, ServerInfo, ThinkingInfo};
use axum::{extract::State, Json};
use std::sync::Arc;

pub async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    let cfg = state.config_snapshot().await;
    Json(ConfigResponse {
        server: ServerInfo {
            host: cfg.server.host.clone(),
            port: cfg.server.port,
        },
        routing: RoutingInfo {
            strategy: cfg.routing.strategy.clone(),
            max_retries: cfg.routing.max_retries,
        },
        thinking: ThinkingInfo {
            enabled: cfg.thinking.enabled,
        },
    })
}

pub async fn reload_config(State(state): State<AppState>) -> ApiResult<Json<ReloadConfigResponse>> {
    let path = state
        .config_path
        .as_deref()
        .ok_or_else(|| ApiError::InternalError("No config path set — cannot reload".to_string()))?;

    let new_config = Config::from_file(path).map_err(|e| match e {
        ConfigError::IoError(io) => {
            ApiError::InternalError(format!("Cannot read config file: {}", io))
        }
        ConfigError::ParseError(pe) => ApiError::BadRequest(format!("Invalid YAML: {}", pe)),
        ConfigError::InvalidPort(p) => ApiError::BadRequest(format!("Invalid port: {}", p)),
        ConfigError::InvalidConfig(msg) => ApiError::BadRequest(msg),
    })?;

    let mut guard = state.config.write().await;
    *guard = new_config;
    drop(guard);

    tracing::info!("Configuration reloaded successfully from {}", path);

    Ok(Json(ReloadConfigResponse {
        status: "success".to_string(),
        message: format!("Configuration reloaded successfully from {}", path),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheManager;
    use crate::registry::ModelRegistry;
    use std::time::Instant;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_get_config() {
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

        let response = get_config(State(state)).await;
        assert_eq!(response.server.port, 8080);
    }

    #[tokio::test]
    async fn test_reload_config_without_path() {
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

        let result = reload_config(State(state)).await;
        assert!(result.is_err());
    }
}
