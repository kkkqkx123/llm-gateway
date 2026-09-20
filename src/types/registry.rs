use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Model alias mapping
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelAlias {
    /// Client alias name
    pub alias: String,
    /// Upstream real model name
    pub upstream_model: String,
    /// Credential ID for this alias (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    /// Additional configuration for this alias
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extra_config: HashMap<String, serde_json::Value>,
}

impl ModelAlias {
    pub fn new(alias: String, upstream_model: String) -> Self {
        Self {
            alias,
            upstream_model,
            credential_id: None,
            extra_config: HashMap::new(),
        }
    }

    pub fn with_credential(mut self, credential_id: String) -> Self {
        self.credential_id = Some(credential_id);
        self
    }
}

/// Credential selection strategy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CredentialSelection {
    /// Select by priority (lower number has higher priority)
    #[default]
    Priority,
    /// Select in round-robin order
    RoundRobin,
}


/// Provider credential configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCredential {
    /// Unique credential ID
    pub id: String,
    /// API Key
    pub api_key: String,
    /// Base URL (optional, overrides provider default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Priority for selection (lower number = higher priority)
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Model prefix for namespace (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_prefix: Option<String>,
    /// Models excluded from this credential
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub excluded_models: Vec<String>,
    /// Custom headers for requests
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Proxy URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// Disable cooling for this credential
    #[serde(default)]
    pub disable_cooling: bool,
    /// Model aliases specific to this credential
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub model_aliases: Vec<ModelAlias>,
}

fn default_priority() -> u32 {
    0
}

impl ProviderCredential {
    pub fn new(id: String, api_key: String) -> Self {
        Self {
            id,
            api_key,
            base_url: None,
            priority: 0,
            model_prefix: None,
            excluded_models: Vec::new(),
            headers: HashMap::new(),
            proxy_url: None,
            disable_cooling: false,
            model_aliases: Vec::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = Some(base_url);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_prefix(mut self, prefix: String) -> Self {
        self.model_prefix = Some(prefix);
        self
    }

    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub fn with_proxy(mut self, proxy_url: String) -> Self {
        self.proxy_url = Some(proxy_url);
        self
    }

    pub fn disable_cooling(mut self) -> Self {
        self.disable_cooling = true;
        self
    }

    pub fn with_alias(mut self, alias: ModelAlias) -> Self {
        self.model_aliases.push(alias);
        self
    }
}

/// Alias cache for fast lookup
#[derive(Debug)]
pub struct AliasCache {
    /// Map from alias to (upstream_model, credential_id)
    cache: RwLock<HashMap<String, (String, Option<String>)>>,
    /// Suffix configuration for alias resolution
    suffix_config: RwLock<HashMap<String, String>>,
}

impl AliasCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            suffix_config: RwLock::new(HashMap::new()),
        }
    }

    /// Register a model alias
    pub fn register_alias(&self, alias: ModelAlias) {
        let mut cache = self.cache.write().expect("Failed to acquire write lock");
        cache.insert(
            alias.alias.clone(),
            (alias.upstream_model, alias.credential_id),
        );
    }

    /// Register batch of aliases
    pub fn register_aliases(&self, aliases: Vec<ModelAlias>) {
        let mut cache = self.cache.write().expect("Failed to acquire write lock");
        for alias in aliases {
            cache.insert(alias.alias, (alias.upstream_model, alias.credential_id));
        }
    }

    /// Set suffix configuration for a credential
    pub fn set_suffix_config(&self, credential_id: String, suffix: String) {
        let mut suffix_config = self
            .suffix_config
            .write()
            .expect("Failed to acquire write lock");
        suffix_config.insert(credential_id, suffix);
    }

    /// Resolve alias to upstream model name
    /// Returns (upstream_model, credential_id) or None if not found
    pub fn resolve(&self, alias: &str) -> Option<(String, Option<String>)> {
        let cache = self.cache.read().expect("Failed to acquire read lock");
        cache.get(alias).cloned()
    }

    /// Clear all aliases
    pub fn clear(&self) {
        let mut cache = self.cache.write().expect("Failed to acquire write lock");
        cache.clear();
        let mut suffix_config = self
            .suffix_config
            .write()
            .expect("Failed to acquire write lock");
        suffix_config.clear();
    }
}

impl Default for AliasCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_alias() {
        let alias = ModelAlias::new("gpt-4".to_string(), "gpt-4-turbo-preview".to_string())
            .with_credential("cred-1".to_string());

        assert_eq!(alias.alias, "gpt-4");
        assert_eq!(alias.upstream_model, "gpt-4-turbo-preview");
        assert_eq!(alias.credential_id, Some("cred-1".to_string()));
    }

    #[test]
    fn test_provider_credential() {
        let credential = ProviderCredential::new("cred-1".to_string(), "sk-xxx".to_string())
            .with_base_url("https://api.openai.com".to_string())
            .with_priority(10)
            .with_prefix("openai:".to_string())
            .with_header("X-Custom-Header".to_string(), "value".to_string());

        assert_eq!(credential.id, "cred-1");
        assert_eq!(credential.api_key, "sk-xxx");
        assert_eq!(
            credential.base_url,
            Some("https://api.openai.com".to_string())
        );
        assert_eq!(credential.priority, 10);
        assert_eq!(credential.model_prefix, Some("openai:".to_string()));
        assert_eq!(
            credential.headers.get("X-Custom-Header"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_alias_cache() {
        let cache = AliasCache::new();

        let alias1 = ModelAlias::new("gpt-4".to_string(), "gpt-4-turbo-preview".to_string())
            .with_credential("cred-1".to_string());
        let alias2 = ModelAlias::new("gpt-3.5".to_string(), "gpt-3.5-turbo".to_string());

        cache.register_alias(alias1);
        cache.register_alias(alias2);

        assert_eq!(
            cache.resolve("gpt-4"),
            Some((
                "gpt-4-turbo-preview".to_string(),
                Some("cred-1".to_string())
            ))
        );
        assert_eq!(
            cache.resolve("gpt-3.5"),
            Some(("gpt-3.5-turbo".to_string(), None))
        );
        assert_eq!(cache.resolve("unknown"), None);
    }

    #[test]
    fn test_alias_cache_batch() {
        let cache = AliasCache::new();

        let aliases = vec![
            ModelAlias::new("gpt-4".to_string(), "gpt-4-turbo-preview".to_string()),
            ModelAlias::new("gpt-3.5".to_string(), "gpt-3.5-turbo".to_string()),
        ];

        cache.register_aliases(aliases);

        assert_eq!(
            cache.resolve("gpt-4").map(|(m, _)| m),
            Some("gpt-4-turbo-preview".to_string())
        );
        assert_eq!(
            cache.resolve("gpt-3.5").map(|(m, _)| m),
            Some("gpt-3.5-turbo".to_string())
        );
    }

    #[test]
    fn test_alias_cache_clear() {
        let cache = AliasCache::new();

        let alias = ModelAlias::new("gpt-4".to_string(), "gpt-4-turbo-preview".to_string());
        cache.register_alias(alias);

        assert!(cache.resolve("gpt-4").is_some());

        cache.clear();
        assert!(cache.resolve("gpt-4").is_none());
    }

    #[test]
    fn test_credential_selection_default() {
        assert_eq!(
            CredentialSelection::default(),
            CredentialSelection::Priority
        );
    }
}
