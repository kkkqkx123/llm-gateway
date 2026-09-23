use crate::api::handlers::AppState;
use crate::api::router::create_router;
use crate::cache::CacheManager;
use crate::config::Config;
use crate::format::registration::register_default_transformers;
use crate::registry::{ModelRegistry, PredefinedModels};
use crate::scheduler::{NoopAuthManager, RefreshScheduler};
use crate::watcher::{ConfigWatcher, WatchEvent};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{error, info, warn};

pub struct HttpServer {
    config: Config,
    config_path: Option<String>,
    registry: Arc<ModelRegistry>,
    cache_manager: Arc<CacheManager>,
}

/// Build a shared reqwest::Client with sane timeouts.
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(60))
        .http1_title_case_headers()
        .build()
        .expect("Failed to build reqwest::Client")
}

impl HttpServer {
    pub fn new(config: Config) -> Self {
        let registry = Arc::new(ModelRegistry::from_config(&config.registry));
        let cache_manager = Arc::new(CacheManager::new(crate::cache::CacheConfig::from_config(
            &config.cache,
        )));

        Self {
            config,
            config_path: None,
            registry,
            cache_manager,
        }
    }

    /// Record the path the config was loaded from, enabling POST /config/reload.
    pub fn with_config_path(mut self, path: impl Into<String>) -> Self {
        self.config_path = Some(path.into());
        self
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
            "Registry initialized with {} predefined models",
            self.registry.list_models().await.len()
        );

        if self.config.registry.remote_update_enabled {
            match self.registry.start_remote_update().await {
                Ok(()) => info!("Remote model registry update scheduled"),
                Err(e) => error!("Failed to start remote registry update: {}", e),
            }
        }
        Ok(())
    }

    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        // --- Critical: register all format transformers into the global Registry ---
        register_default_transformers();
        info!("Default format transformers registered");

        self.initialize_registry().await?;

        // Start the auth refresh scheduler if polling is enabled. The NoopAuthManager is
        // the default — real provider credential refreshers can replace it later.
        if self.config.polling.enabled {
            let scheduler = RefreshScheduler::from_config(
                Some(Arc::new(NoopAuthManager::new())),
                &self.config.polling,
            );
            let (_shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
            let (jobs_tx, _jobs_rx) = tokio::sync::broadcast::channel::<String>(16);
            tokio::spawn(async move {
                match scheduler.run(shutdown_rx, jobs_tx).await {
                    Ok(_) => info!("Auth refresh scheduler exited normally"),
                    Err(e) => warn!("Auth refresh scheduler exited with error: {}", e),
                }
            });
            info!("Auth refresh scheduler started (NoopAuthManager)");
        }

        let http_client = build_http_client();

        let app_state = AppState {
            config: Arc::new(RwLock::new(self.config.clone())),
            registry: self.registry.clone(),
            cache_manager: self.cache_manager.clone(),
            start_time: Instant::now(),
            config_path: self.config_path.clone(),
            http_client: http_client.clone(),
        };

        // Start ConfigWatcher and connect its events to a real config reload.
        self.start_config_watcher(app_state.clone())?;

        // Apply auth middleware if configured.
        let mut router = create_router(app_state.clone());
        if app_state.config_snapshot().await.gateway.api_key.is_some() {
            info!(
                "Gateway API key authentication enabled (header: {})",
                app_state.config_snapshot().await.gateway.auth_header
            );
            router = router.layer(axum::middleware::from_fn_with_state(
                app_state.clone(),
                require_gateway_auth,
            ));
        }

        let app = router.layer(
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

        if let Some(tls) = &self.config.server.tls {
            warn!(
                "ServerConfig.tls is set (cert={}, key={}) but this binary was built \
                 without the `tls-rustls` feature — falling back to plain HTTP. \
                 Rebuild with TLS support to enable HTTPS.",
                tls.cert_path, tls.key_path
            );
        }

        let listener = TcpListener::bind(&addr).await?;

        info!("Starting HTTP server on {}", addr);
        info!("Available endpoints:");
        info!("  GET  /health");
        info!("  GET  /config");
        info!("  POST /config/reload");
        info!("  GET  /v1/models");
        info!("  GET  /v1/models/:model_id");
        info!("  POST /v1/chat/completions");

        axum::serve(listener, app).await?;
        Ok(())
    }

    /// Start a ConfigWatcher; its modifications will trigger a live config reload in AppState.
    fn start_config_watcher(&self, state: AppState) -> Result<(), Box<dyn std::error::Error>> {
        if !self.config.watcher.enabled {
            return Ok(());
        }

        let paths: Vec<std::path::PathBuf> = self
            .config
            .watcher
            .paths
            .iter()
            .map(std::path::PathBuf::from)
            .collect();

        if paths.is_empty() {
            warn!("Config watcher enabled but no paths configured — skipping");
            return Ok(());
        }

        let watcher = ConfigWatcher::new(paths)
            .with_debounce(Duration::from_millis(self.config.watcher.debounce_ms))
            .with_atomic_debounce(Duration::from_millis(
                self.config.watcher.atomic_replace_debounce_ms,
            ));

        tokio::spawn(async move {
            let Some(mut rx) = watcher.receiver().await else {
                warn!("Watcher receiver already taken — hot-reload disabled");
                return;
            };
            while let Some(event) = rx.recv().await {
                tracing::info!("Watcher event: {:?}", event);
                match event {
                    WatchEvent::ConfigModified(_)
                    | WatchEvent::ConfigCreated(_)
                    | WatchEvent::ConfigDeleted(_)
                    | WatchEvent::AuthFileDeleted(_) => {
                        Self::try_reload(&state).await;
                    }
                    WatchEvent::AuthDirChanged(_) | WatchEvent::Error(_) => {}
                }
            }
        });

        Ok(())
    }

    /// Attempt a graceful reload of AppState.config from disk; swallows errors so a bad
    /// config file does not take the server down.
    async fn try_reload(state: &AppState) {
        let Some(path) = state.config_path.as_deref() else {
            return;
        };

        match Config::from_file(path) {
            Ok(new_config) => {
                let mut guard = state.config.write().await;
                *guard = new_config;
                drop(guard);
                tracing::info!("Config hot-reloaded from {}", path);
            }
            Err(e) => {
                tracing::warn!("Config hot-reload failed (keeping previous config): {}", e);
            }
        }
    }

    /// Convenience helper that builds a HttpServer from a pre-loaded config file.
    pub fn from_config_file(path: &str) -> Result<Self, crate::config::ConfigError> {
        let config = Config::from_file(path)?;
        Ok(Self::new(config).with_config_path(path))
    }
}

impl Default for HttpServer {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

/// Axum middleware that verifies an inbound request carries the configured API key.
/// Always allows `/health` so load balancers can probe without credentials.
async fn require_gateway_auth(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Always allow health probes.
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    let cfg = state.config_snapshot().await;
    let Some(expected) = cfg.gateway.api_key.as_deref() else {
        return next.run(req).await; // no API key configured → pass through
    };

    let header_name = cfg.gateway.auth_header.as_str();
    let actual = req
        .headers()
        .get(header_name)
        .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
        .unwrap_or("");

    let matches = actual == expected
        || actual
            .strip_prefix("Bearer ")
            .map_or(false, |v| v == expected);

    if !matches {
        tracing::warn!(
            "Rejected request {} {} — invalid/missing API key in header '{}'",
            req.method(),
            req.uri().path(),
            header_name,
        );
        return axum::response::IntoResponse::into_response((
            axum::http::StatusCode::UNAUTHORIZED,
            "Unauthorized",
        ));
    }

    next.run(req).await
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

    #[test]
    fn test_build_http_client() {
        // A lightweight build test; we only care that it does not panic.
        let _client = build_http_client();
    }
}
