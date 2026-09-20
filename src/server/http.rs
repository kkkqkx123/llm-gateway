use crate::api::handlers::AppState;
use crate::api::router::create_router;
use crate::cache::CacheManager;
use crate::config::Config;
use crate::registry::{ModelRegistry, PredefinedModels};
use crate::watcher::{ConfigWatcher, WatchEvent};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{error, info};

pub struct HttpServer {
    config: Arc<Config>,
    registry: Arc<ModelRegistry>,
    cache_manager: Arc<CacheManager>,
}

impl HttpServer {
    pub fn new(config: Config) -> Self {
        let config = Arc::new(config);
        let registry = Arc::new(ModelRegistry::new());
        let cache_manager = Arc::new(CacheManager::new(crate::cache::CacheConfig::default()));

        Self {
            config,
            registry,
            cache_manager,
        }
    }

    pub fn with_registry(mut self, registry: ModelRegistry) -> Self {
        self.registry = Arc::new(registry);
        self
    }

    pub async fn initialize_registry(&self) -> Result<(), Box<dyn std::error::Error>> {
        let predefined = PredefinedModels::all();
        for model in predefined {
            self.registry.add_model(model).await?;
        }
        info!(
            "Registry initialized with {} models",
            self.registry.list_models().await.len()
        );
        Ok(())
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.initialize_registry().await?;

        let watcher = if self.config.watcher.enabled {
            let watcher = self.start_watcher()?;
            info!("Configuration watcher started");
            Some(Arc::new(watcher))
        } else {
            None
        };

        let app_state = AppState {
            config: self.config.clone(),
            registry: self.registry.clone(),
            cache_manager: self.cache_manager.clone(),
            start_time: Instant::now(),
        };

        let app = create_router(app_state).layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(
                    axum::http::HeaderName::from_static("x-request-id"),
                    MakeRequestUuid,
                ))
                .layer(PropagateRequestIdLayer::new(
                    axum::http::HeaderName::from_static("x-request-id"),
                ))
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        );

        let addr = format!("{}:{}", self.config.server.host, self.config.server.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("Starting HTTP server on {}", addr);
        info!("Available endpoints:");
        info!("  GET  /health");
        info!("  GET  /config");
        info!("  POST /config/reload");
        info!("  GET  /v1/models");
        info!("  GET  /v1/models/:model_id");
        info!("  POST /v1/chat/completions");

        if let Some(watcher) = watcher {
            let mut receiver = watcher.receiver().await;
            if let Some(mut receiver) = receiver {
                tokio::spawn(async move {
                    while let Some(event) = receiver.recv().await {
                        info!("Received watch event: {:?}", event);
                    }
                });
            }
        }

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Start configuration watcher
    fn start_watcher(&self) -> Result<ConfigWatcher, Box<dyn std::error::Error>> {
        let paths: Vec<std::path::PathBuf> = self
            .config
            .watcher
            .paths
            .iter()
            .map(|p| std::path::PathBuf::from(p))
            .collect();

        let watcher = ConfigWatcher::new(paths)
            .with_debounce(std::time::Duration::from_millis(
                self.config.watcher.debounce_ms,
            ))
            .with_atomic_debounce(std::time::Duration::from_millis(
                self.config.watcher.atomic_replace_debounce_ms,
            ));

        Ok(watcher)
    }
}

impl Default for HttpServer {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_server_creation() {
        let server = HttpServer::new(Config::default());
        assert_eq!(server.config.server.port, 8080);
    }

    #[test]
    fn test_http_server_default() {
        let server = HttpServer::default();
        assert_eq!(server.config.server.host, "0.0.0.0");
        assert_eq!(server.config.server.port, 8080);
    }
}
