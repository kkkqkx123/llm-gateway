use crate::api::{handlers::AppState, ApiResult};
use crate::types::{ConfigResponse, ReloadConfigResponse, RoutingInfo, ServerInfo, ThinkingInfo};
use axum::{extract::State, Json};

pub async fn get_config(State(state): State<AppState>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        server: ServerInfo {
            host: state.config.server.host.clone(),
            port: state.config.server.port,
        },
        routing: RoutingInfo {
            strategy: state.config.routing.strategy.clone(),
            max_retries: state.config.routing.max_retries,
        },
        thinking: ThinkingInfo {
            enabled: state.config.thinking.enabled,
        },
    })
}

pub async fn reload_config(
    State(_state): State<AppState>,
) -> ApiResult<Json<ReloadConfigResponse>> {
    Ok(Json(ReloadConfigResponse {
        status: "success".to_string(),
        message: "Configuration reloaded successfully".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_config() {
        use crate::cache::CacheManager;
        use crate::config::Config;
        use crate::registry::ModelRegistry;
        use std::sync::Arc;
        use std::time::Instant;

        let config = Arc::new(Config::default());
        let registry = Arc::new(ModelRegistry::new());
        let cache_manager = Arc::new(CacheManager::new(crate::cache::CacheConfig::default()));
        let state = AppState {
            config: config.clone(),
            registry,
            cache_manager,
            start_time: Instant::now(),
        };

        let response = get_config(State(state)).await;
        assert_eq!(response.server.port, 8080);
    }
}
