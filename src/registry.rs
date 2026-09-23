use crate::config::types::RegistryConfig;
use crate::types::{ModelCapabilities, ModelPricing};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Model Information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model ID
    pub id: String,
    /// Name of provider
    pub provider: String,
    /// modeling capability
    pub capabilities: ModelCapabilities,
    /// Whether or not the model is user-defined
    pub user_defined: bool,
    /// model version
    pub version: Option<String>,
    /// Last updated
    pub updated_at: DateTime<Utc>,
    /// model description
    pub description: Option<String>,
    /// Model pricing (per 1K token)
    pub pricing: Option<ModelPricing>,
}

/// model registry
pub struct ModelRegistry {
    /// Model Storage
    models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    /// Provider Index
    provider_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// user-defined model
    user_models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    /// Remote Update Configuration
    remote_config: Option<RemoteUpdateConfig>,
}

/// Remote Update Configuration
#[derive(Debug, Clone)]
pub struct RemoteUpdateConfig {
    /// Remote Source URL
    pub url: String,
    /// update interval
    pub interval: Duration,
    /// Last Updated
    pub last_updated: Arc<RwLock<Option<Instant>>>,
}

impl ModelRegistry {
    /// Creating a new model registry
    pub fn new() -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            provider_index: Arc::new(RwLock::new(HashMap::new())),
            user_models: Arc::new(RwLock::new(HashMap::new())),
            remote_config: None,
        }
    }

    /// Creating a model registry from configuration
    pub fn from_config(config: &RegistryConfig) -> Self {
        let mut registry = Self::new();

        if config.remote_update_enabled {
            if let Some(url) = &config.remote_update_url {
                let interval =
                    Duration::from_secs(config.remote_update_interval_secs.unwrap_or(3600));
                registry.configure_remote_update(url.clone(), interval);
            }
        }

        registry
    }

    /// Adding Models
    pub async fn add_model(&self, model: ModelInfo) -> Result<(), RegistryError> {
        let id = model.id.clone();
        let provider = model.provider.clone();
        let provider_clone = provider.clone();

        // Add to Primary Storage
        {
            let mut models = self.models.write().await;
            models.insert(id.clone(), model.clone());
        }

        // Updating the Provider Index
        {
            let mut index = self.provider_index.write().await;
            index
                .entry(provider)
                .or_insert_with(Vec::new)
                .push(id.clone());
        }

        // If it is a user-defined model, add to the user model store
        if model.user_defined {
            let mut user_models = self.user_models.write().await;
            user_models.insert(id.clone(), model);
        }

        info!("Model added: {} (provider: {})", id, provider_clone);
        Ok(())
    }

    /// Remove Model
    pub async fn remove_model(&self, id: &str) -> Result<(), RegistryError> {
        let model = {
            let mut models = self.models.write().await;
            models.remove(id).ok_or(RegistryError::ModelNotFound)?
        };

        // Remove from Provider Index
        {
            let mut index = self.provider_index.write().await;
            if let Some(models) = index.get_mut(&model.provider) {
                models.retain(|m| m != id);
                if models.is_empty() {
                    index.remove(&model.provider);
                }
            }
        }

        // Remove from user model storage
        if model.user_defined {
            let mut user_models = self.user_models.write().await;
            user_models.remove(id);
        }

        info!("Model removed: {}", id);
        Ok(())
    }

    /// query model
    pub async fn get_model(&self, id: &str) -> Result<ModelInfo, RegistryError> {
        let models = self.models.read().await;
        models.get(id).cloned().ok_or(RegistryError::ModelNotFound)
    }

    /// Query model by provider
    pub async fn get_models_by_provider(&self, provider: &str) -> Vec<ModelInfo> {
        let index = self.provider_index.read().await;
        let models = self.models.read().await;

        if let Some(model_ids) = index.get(provider) {
            model_ids
                .iter()
                .filter_map(|id| models.get(id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Query all models
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        let models = self.models.read().await;
        models.values().cloned().collect()
    }

    /// Querying user-defined models
    pub async fn list_user_models(&self) -> Vec<ModelInfo> {
        let user_models = self.user_models.read().await;
        user_models.values().cloned().collect()
    }

    /// Check if the model exists
    pub async fn model_exists(&self, id: &str) -> bool {
        let models = self.models.read().await;
        models.contains_key(id)
    }

    /// Check if the model supports specific capabilities
    pub async fn supports_capability(
        &self,
        id: &str,
        capability: &str,
    ) -> Result<bool, RegistryError> {
        let model = self.get_model(id).await?;
        Ok(model.capabilities.supports(capability))
    }

    /// Getting Thinking Support for Models
    pub async fn supports_thinking(&self, id: &str) -> Result<bool, RegistryError> {
        let model = self.get_model(id).await?;
        Ok(model.capabilities.thinking)
    }

    /// Get streaming support for models
    pub async fn supports_streaming(&self, id: &str) -> Result<bool, RegistryError> {
        let model = self.get_model(id).await?;
        Ok(model.capabilities.streaming)
    }

    /// Configuring Remote Updates
    pub fn configure_remote_update(&mut self, url: String, interval: Duration) {
        let url_clone = url.clone();
        self.remote_config = Some(RemoteUpdateConfig {
            url,
            interval,
            last_updated: Arc::new(RwLock::new(None)),
        });
        info!(
            "Remote update configured: URL={}, interval={:?}",
            url_clone, interval
        );
    }

    /// Starting a remote update task.
    /// Uses the configured `interval` between fetches instead of a hard-coded 60s sleep.
    pub async fn start_remote_update(&self) -> Result<(), RegistryError> {
        let config = self
            .remote_config
            .as_ref()
            .ok_or(RegistryError::RemoteNotConfigured)?;

        let models = self.models.clone();
        let provider_index = self.provider_index.clone();
        let url = config.url.clone();
        let interval = config.interval;
        let last_updated = config.last_updated.clone();

        tokio::spawn(async move {
            info!(
                "Remote model registry updater started (url={}, interval={:?})",
                url, interval
            );
            // Fetch immediately on start, then on interval.
            loop {
                {
                    let mut models_guard = models.write().await;
                    let mut provider_guard = provider_index.write().await;

                    match Self::fetch_remote_models(&url).await {
                        Ok(new_models) => {
                            info!("Fetched {} models from remote registry", new_models.len());

                            for model in new_models {
                                models_guard.insert(model.id.clone(), model.clone());
                                provider_guard
                                    .entry(model.provider.clone())
                                    .or_insert_with(Vec::new)
                                    .push(model.id);
                            }

                            let mut last = last_updated.write().await;
                            *last = Some(Instant::now());
                            info!("Remote model registry updated");
                        }
                        Err(e) => {
                            error!("Failed to fetch remote models: {}", e);
                        }
                    }
                }

                sleep(interval).await;
            }
        });

        Ok(())
    }

    /// Add models in bulk
    pub async fn add_models(&self, models: Vec<ModelInfo>) -> Result<(), RegistryError> {
        for model in models {
            self.add_model(model).await?;
        }
        Ok(())
    }

    /// Fetch models from remote registry
    async fn fetch_remote_models(url: &str) -> Result<Vec<ModelInfo>, RegistryError> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(RegistryError::RemoteUpdateError(format!(
                "HTTP error: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response.json().await?;

        let models = match json {
            serde_json::Value::Array(arr) => {
                let mut models = Vec::new();
                for item in arr {
                    if let Ok(model) = serde_json::from_value::<ModelInfo>(item) {
                        models.push(model);
                    }
                }
                models
            }
            serde_json::Value::Object(obj) => {
                if let Some(models_array) = obj.get("models").and_then(|v| v.as_array()) {
                    let mut models = Vec::new();
                    for item in models_array {
                        if let Ok(model) = serde_json::from_value::<ModelInfo>(item.clone()) {
                            models.push(model);
                        }
                    }
                    models
                } else {
                    return Err(RegistryError::RemoteUpdateError(
                        "Invalid response format".to_string(),
                    ));
                }
            }
            _ => {
                return Err(RegistryError::RemoteUpdateError(
                    "Invalid response format".to_string(),
                ));
            }
        };

        Ok(models)
    }

    /// Get a list of providers
    pub async fn list_providers(&self) -> Vec<String> {
        let index = self.provider_index.read().await;
        index.keys().cloned().collect()
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// registry error
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Model not found")]
    ModelNotFound,

    #[error("Remote update not configured")]
    RemoteNotConfigured,

    #[error("Remote update error: {0}")]
    RemoteUpdateError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),
}

/// predefined model
pub struct PredefinedModels;

impl PredefinedModels {
    /// Claude 3.5 Sonnet
    pub fn claude_3_5_sonnet() -> ModelInfo {
        ModelInfo {
            id: "claude-3-5-sonnet-20241022".to_string(),
            provider: "anthropic".to_string(),
            capabilities: ModelCapabilities::with_thinking(),
            user_defined: false,
            version: Some("20241022".to_string()),
            updated_at: chrono::Utc::now(),
            description: Some("Claude 3.5 Sonnet: Most intelligent model".to_string()),
            pricing: Some(ModelPricing {
                input_price: 0.003,
                output_price: Some(0.015),
                currency: "USD".to_string(),
            }),
        }
    }

    /// Claude 3 Opus
    pub fn claude_3_opus() -> ModelInfo {
        ModelInfo {
            id: "claude-3-opus-20240229".to_string(),
            provider: "anthropic".to_string(),
            capabilities: ModelCapabilities::with_thinking(),
            user_defined: false,
            version: Some("20240229".to_string()),
            updated_at: chrono::Utc::now(),
            description: Some("Claude 3 Opus: Most capable model".to_string()),
            pricing: Some(ModelPricing {
                input_price: 0.015,
                output_price: Some(0.075),
                currency: "USD".to_string(),
            }),
        }
    }

    /// GPT-4 Turbo
    pub fn gpt_4_turbo() -> ModelInfo {
        ModelInfo {
            id: "gpt-4-turbo-preview".to_string(),
            provider: "openai".to_string(),
            capabilities: ModelCapabilities {
                thinking: false,
                streaming: true,
                function_calling: true,
                multimodal: false,
                max_context_length: Some(128000),
                input_types: vec!["text".to_string()],
                output_types: vec!["text".to_string()],
            },
            user_defined: false,
            version: None,
            updated_at: chrono::Utc::now(),
            description: Some("GPT-4 Turbo: Latest GPT-4 model".to_string()),
            pricing: Some(ModelPricing {
                input_price: 0.01,
                output_price: Some(0.03),
                currency: "USD".to_string(),
            }),
        }
    }

    /// Gemini 1.5 Pro
    pub fn gemini_1_5_pro() -> ModelInfo {
        ModelInfo {
            id: "gemini-1.5-pro".to_string(),
            provider: "google".to_string(),
            capabilities: ModelCapabilities {
                thinking: false,
                streaming: true,
                function_calling: false,
                multimodal: true,
                max_context_length: Some(2800000),
                input_types: vec!["text".to_string(), "image".to_string(), "audio".to_string()],
                output_types: vec!["text".to_string()],
            },
            user_defined: false,
            version: None,
            updated_at: chrono::Utc::now(),
            description: Some("Gemini 1.5 Pro: Large context model".to_string()),
            pricing: Some(ModelPricing {
                input_price: 0.0035,
                output_price: Some(0.0105),
                currency: "USD".to_string(),
            }),
        }
    }

    /// Get all predefined models
    pub fn all() -> Vec<ModelInfo> {
        vec![
            Self::claude_3_5_sonnet(),
            Self::claude_3_opus(),
            Self::gpt_4_turbo(),
            Self::gemini_1_5_pro(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ModelRegistry::new();
        assert_eq!(registry.list_models().await.len(), 0);
    }

    #[tokio::test]
    async fn test_add_and_get_model() {
        let registry = ModelRegistry::new();
        let model = PredefinedModels::claude_3_5_sonnet();

        registry.add_model(model.clone()).await.unwrap();

        let retrieved = registry.get_model(&model.id).await.unwrap();
        assert_eq!(retrieved.id, model.id);
        assert_eq!(retrieved.provider, model.provider);
    }

    #[tokio::test]
    async fn test_remove_model() {
        let registry = ModelRegistry::new();
        let model = PredefinedModels::claude_3_5_sonnet();

        registry.add_model(model.clone()).await.unwrap();
        assert!(registry.model_exists(&model.id).await);

        registry.remove_model(&model.id).await.unwrap();
        assert!(!registry.model_exists(&model.id).await);
    }

    #[tokio::test]
    async fn test_get_models_by_provider() {
        let registry = ModelRegistry::new();

        registry
            .add_model(PredefinedModels::claude_3_5_sonnet())
            .await
            .unwrap();
        registry
            .add_model(PredefinedModels::claude_3_opus())
            .await
            .unwrap();
        registry
            .add_model(PredefinedModels::gpt_4_turbo())
            .await
            .unwrap();

        let anthropic_models = registry.get_models_by_provider("anthropic").await;
        assert_eq!(anthropic_models.len(), 2);

        let openai_models = registry.get_models_by_provider("openai").await;
        assert_eq!(openai_models.len(), 1);
    }

    #[tokio::test]
    async fn test_supports_capability() {
        let registry = ModelRegistry::new();
        let model = PredefinedModels::claude_3_5_sonnet();

        registry.add_model(model).await.unwrap();

        assert!(registry
            .supports_capability("claude-3-5-sonnet-20241022", "thinking")
            .await
            .unwrap());
        assert!(!registry
            .supports_capability("claude-3-5-sonnet-20241022", "function_calling")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_user_models() {
        let registry = ModelRegistry::new();

        let user_model = ModelInfo {
            id: "user-model-1".to_string(),
            provider: "custom".to_string(),
            capabilities: ModelCapabilities::basic(),
            user_defined: true,
            version: None,
            updated_at: chrono::Utc::now(),
            description: None,
            pricing: None,
        };

        registry.add_model(user_model.clone()).await.unwrap();

        let user_models = registry.list_user_models().await;
        assert_eq!(user_models.len(), 1);
        assert_eq!(user_models[0].id, "user-model-1");
    }

    #[tokio::test]
    async fn test_list_providers() {
        let registry = ModelRegistry::new();

        registry
            .add_model(PredefinedModels::claude_3_5_sonnet())
            .await
            .unwrap();
        registry
            .add_model(PredefinedModels::gpt_4_turbo())
            .await
            .unwrap();
        registry
            .add_model(PredefinedModels::gemini_1_5_pro())
            .await
            .unwrap();

        let providers = registry.list_providers().await;
        assert!(providers.contains(&"anthropic".to_string()));
        assert!(providers.contains(&"openai".to_string()));
        assert!(providers.contains(&"google".to_string()));
    }
}
