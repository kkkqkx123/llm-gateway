use crate::api::handlers::AppState;
use crate::types::HealthResponse;
use axum::{extract::State, Json};
use std::collections::HashMap;

pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();

    let mut checks = HashMap::new();

    let cache_stats = state.cache_manager.signature_cache().stats().await;
    checks.insert(
        "cache".to_string(),
        format!(
            "ok (size: {}, hit_ratio: {:.2})",
            cache_stats.size, cache_stats.hit_ratio
        ),
    );
    checks.insert("registry".to_string(), "ok".to_string());

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        use crate::cache::CacheManager;
        use crate::config::Config;
        use crate::registry::ModelRegistry;
        use std::sync::Arc;
        use std::time::Instant;

        let config = Arc::new(Config::default());
        let registry = Arc::new(ModelRegistry::new());
        let cache_manager = Arc::new(CacheManager::new(crate::cache::CacheConfig::default()));
        let state = AppState {
            config,
            registry,
            cache_manager,
            start_time: Instant::now(),
        };

        let response = health_check(State(state)).await;
        assert_eq!(response.status, "healthy");
    }
}
