# 模型注册增强实现细节

## 一、核心数据结构设计

### 1.1 模型别名映射表

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 模型别名缓存键
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct AliasCacheKey {
    credential_id: String,
    alias: String,
}

/// 模型别名缓存值
#[derive(Debug, Clone)]
struct AliasCacheValue {
    upstream_model: String,
    config_suffix: Option<String>,  // 配置中定义的后缀（如 "(low)"）
}

/// 模型别名缓存
pub struct ModelAliasCache {
    cache: Arc<RwLock<HashMap<AliasCacheKey, AliasCacheValue>>>,
}

impl ModelAliasCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn resolve(
        &self,
        credential_id: &str,
        input: &str,
    ) -> Option<(String, Option<String>)> {
        let key = AliasCacheKey {
            credential_id: credential_id.to_string(),
            alias: input.to_lowercase(),
        };

        let cache = self.cache.read().await;
        cache.get(&key).map(|value| {
            // 检查输入是否有用户后缀
            let user_suffix = Self::extract_suffix(input);
            // 配置后缀优先于用户后缀
            let final_suffix = value.config_suffix.as_ref().or(user_suffix.as_ref());
            (value.upstream_model.clone(), final_suffix.cloned())
        })
    }

    pub async fn insert(
        &self,
        credential_id: String,
        alias: String,
        upstream_model: String,
        config_suffix: Option<String>,
    ) {
        let key = AliasCacheKey {
            credential_id,
            alias: alias.to_lowercase(),
        };
        let value = AliasCacheValue {
            upstream_model,
            config_suffix,
        };

        let mut cache = self.cache.write().await;
        cache.insert(key, value);
    }

    pub async fn clear_credential(&self, credential_id: &str) {
        let mut cache = self.cache.write().await;
        cache.retain(|key, _| key.credential_id != credential_id);
    }

    fn extract_suffix(input: &str) -> Option<String> {
        // 提取后缀，如 "(8192)", "(high)", "(low)"
        let start = input.rfind('(')?;
        let end = input.find(')')?;
        if start < end {
            Some(input[start..=end].to_string())
        } else {
            None
        }
    }
}
```

### 1.2 Credential选择器

```rust
/// Credential选择结果
#[derive(Debug, Clone)]
pub struct CredentialSelection {
    pub credential_id: String,
    pub priority: i32,
    pub upstream_model: String,
    pub available: bool,
    pub cooling_until: Option<DateTime<Utc>>,
    pub quota_exceeded: bool,
}

impl CredentialSelection {
    pub fn is_available(&self) -> bool {
        self.available
            && self.cooling_until.map_or(true, |t| t < Utc::now())
            && !self.quota_exceeded
    }
}

/// Credential选择策略
pub trait CredentialSelector: Send + Sync {
    async fn select(
        &self,
        candidates: Vec<CredentialSelection>,
        request_count: u64,
    ) -> Option<CredentialSelection>;
}

/// 基于优先级的选择策略
pub struct PrioritySelector;

impl CredentialSelector for PrioritySelector {
    async fn select(
        &self,
        mut candidates: Vec<CredentialSelection>,
        _request_count: u64,
    ) -> Option<CredentialSelection> {
        candidates.sort_by(|a, b| {
            // 首先按可用性排序
            match (a.is_available(), b.is_available()) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // 然后按优先级排序（高优先级在前）
                    b.priority.cmp(&a.priority)
                }
            }
        });

        candidates.into_iter().find(|c| c.is_available())
    }
}

/// 轮询选择策略
pub struct RoundRobinSelector {
    current: Arc<RwLock<usize>>,
}

impl RoundRobinSelector {
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(0)),
        }
    }
}

impl CredentialSelector for RoundRobinSelector {
    async fn select(
        &self,
        candidates: Vec<CredentialSelection>,
        _request_count: u64,
    ) -> Option<CredentialSelection> {
        let available: Vec<_> = candidates
            .into_iter()
            .filter(|c| c.is_available())
            .collect();

        if available.is_empty() {
            return None;
        }

        let mut current = self.current.write().await;
        let index = *current % available.len();
        *current = index + 1;

        Some(available[index].clone())
    }
}
```

### 1.3 增强的ModelRegistry

```rust
use crate::config::ProviderCredential;
use crate::types::{ModelCapabilities, ModelPricing, ProviderType};

pub struct ModelRegistry {
    models: Arc<RwLock<HashMap<String, ModelInfo>>>,
    provider_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    user_models: Arc<RwLock<HashMap<String, ModelInfo>>>,

    // 新增：credential索引
    credential_index: Arc<RwLock<HashMap<String, Vec<String>>>>,

    // 新增：别名缓存
    alias_cache: ModelAliasCache,

    // 新增：冷却状态跟踪
    cooling_state: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,

    // 新增：配额状态跟踪
    quota_state: Arc<RwLock<HashMap<String, QuotaInfo>>>,

    remote_config: Option<RemoteUpdateConfig>,
}

#[derive(Debug, Clone)]
pub struct QuotaInfo {
    pub quota_exceeded: bool,
    pub exceeded_at: DateTime<Utc>,
    pub request_count: u64,
}

impl ModelRegistry {
    /// 注册credential及其关联的模型
    pub async fn register_credential(
        &self,
        credential_id: String,
        provider: String,
        provider_type: ProviderType,
        config: ProviderCredential,
    ) -> Result<(), RegistryError> {
        // 1. 检查API key是否有效
        if config.api_key.is_empty() {
            return Err(RegistryError::InvalidCredential(
                "API key cannot be empty".to_string(),
            ));
        }

        // 2. 为每个模型别名创建ModelInfo
        for model_alias in &config.models {
            // 构建完整的模型ID（考虑前缀）
            let full_alias = if let Some(prefix) = &config.prefix {
                format!("{}/{}", prefix.trim_end_matches('/'), model_alias.alias)
            } else {
                model_alias.alias.clone()
            };

            // 提取配置后缀
            let config_suffix = Self::extract_config_suffix(&model_alias.name);

            // 创建ModelInfo
            let model_info = ModelInfo {
                id: full_alias.clone(),
                alias: Some(model_alias.alias.clone()),
                provider: provider.clone(),
                provider_type,
                credential_id: Some(credential_id.clone()),
                capabilities: ModelCapabilities::basic(),
                user_defined: false,
                version: None,
                updated_at: Utc::now(),
                description: None,
                pricing: None,
                prefix: config.prefix.clone(),
            };

            // 添加到主存储
            {
                let mut models = self.models.write().await;
                models.insert(full_alias.clone(), model_info);
            }

            // 更新provider索引
            {
                let mut index = self.provider_index.write().await;
                index
                    .entry(provider.clone())
                    .or_insert_with(Vec::new)
                    .push(full_alias.clone());
            }

            // 更新credential索引
            {
                let mut cred_index = self.credential_index.write().await;
                cred_index
                    .entry(credential_id.clone())
                    .or_insert_with(Vec::new)
                    .push(full_alias.clone());
            }

            // 添加到别名缓存
            self.alias_cache
                .insert(
                    credential_id.clone(),
                    model_alias.alias.clone(),
                    model_alias.name.clone(),
                    config_suffix,
                )
                .await;
        }

        // 3. 注册上游模型名（不带别名）
        for model_alias in &config.models {
            let upstream_id = if let Some(prefix) = &config.prefix {
                format!("{}/{}", prefix.trim_end_matches('/'), model_alias.name)
            } else {
                model_alias.name.clone()
            };

            // 检查是否已存在
            {
                let models = self.models.read().await;
                if !models.contains_key(&upstream_id) {
                    drop(models);

                    // 注册上游模型
                    let model_info = ModelInfo {
                        id: upstream_id.clone(),
                        alias: Some(model_alias.alias.clone()),
                        provider: provider.clone(),
                        provider_type,
                        credential_id: Some(credential_id.clone()),
                        capabilities: ModelCapabilities::basic(),
                        user_defined: false,
                        version: None,
                        updated_at: Utc::now(),
                        description: None,
                        pricing: None,
                        prefix: config.prefix.clone(),
                    };

                    let mut models = self.models.write().await;
                    models.insert(upstream_id.clone(), model_info);

                    let mut index = self.provider_index.write().await;
                    index
                        .entry(provider.clone())
                        .or_insert_with(Vec::new)
                        .push(upstream_id.clone());

                    let mut cred_index = self.credential_index.write().await;
                    cred_index
                        .entry(credential_id.clone())
                        .or_insert_with(Vec::new)
                        .push(upstream_id.clone());
                }
            }
        }

        Ok(())
    }

    /// 解析模型别名并返回上游模型名和后缀
    pub async fn resolve_model_alias(
        &self,
        credential_id: &str,
        input: &str,
    ) -> Result<(String, Option<String>), RegistryError> {
        // 1. 提取前缀
        let (prefix, alias_part) = Self::split_prefix(input);

        // 2. 尝试从缓存解析
        if let Some((upstream_model, config_suffix)) =
            self.alias_cache.resolve(credential_id, alias_part).await
        {
            // 3. 提取用户后缀
            let user_suffix = ModelAliasCache::extract_suffix(alias_part);

            // 4. 组合最终模型名
            let final_model = if let Some(p) = prefix {
                format!("{}/{}", p, upstream_model)
            } else {
                upstream_model
            };

            // 5. 确定最终后缀（配置后缀优先）
            let final_suffix = config_suffix.or(user_suffix);

            return Ok((final_model, final_suffix));
        }

        // 6. 如果不是别名，可能是直接使用的上游模型名
        Ok((input.to_string(), None))
    }

    /// 获取支持指定模型的所有credentials
    pub async fn get_credentials_for_provider(
        &self,
        provider: &str,
        model: &str,
    ) -> Vec<CredentialSelection> {
        let mut selections = Vec::new();

        // 获取provider的所有模型
        let provider_models = {
            let index = self.provider_index.read().await;
            index.get(provider)
                .cloned()
                .unwrap_or_default()
        };

        // 查找匹配的模型
        for model_id in &provider_models {
            if model_id == model {
                let model_info = {
                    let models = self.models.read().await;
                    models.get(model_id).cloned()
                };

                if let Some(info) = model_info {
                    if let Some(credential_id) = &info.credential_id {
                        let available = self
                            .check_credential_availability(credential_id)
                            .await;

                        let cooling_until = self
                            .get_credential_cooling_until(credential_id)
                            .await;

                        let quota_info = self
                            .get_quota_info(credential_id)
                            .await
                            .unwrap_or_default();

                        // TODO: 从配置中读取priority
                        selections.push(CredentialSelection {
                            credential_id: credential_id.clone(),
                            priority: 0,
                            upstream_model: info.id.clone(),
                            available,
                            cooling_until,
                            quota_exceeded: quota_info.quota_exceeded,
                        });
                    }
                }
            }
        }

        selections
    }

    /// 标记credential为冷却状态
    pub async fn set_credential_cooled(
        &self,
        credential_id: &str,
        until: DateTime<Utc>,
        disable_cooling: bool,
    ) {
        if !disable_cooling {
            let mut state = self.cooling_state.write().await;
            state.insert(credential_id.to_string(), until);
        }
    }

    /// 标记credential配额超限
    pub async fn set_credential_quota_exceeded(
        &self,
        credential_id: &str,
        exceeded: bool,
    ) {
        let mut state = self.quota_state.write().await;
        if exceeded {
            state.insert(
                credential_id.to_string(),
                QuotaInfo {
                    quota_exceeded: true,
                    exceeded_at: Utc::now(),
                    request_count: 0,
                },
            );
        } else {
            state.remove(credential_id);
        }
    }

    /// 移除credential及其关联的模型
    pub async fn unregister_credential(&self, credential_id: &str) -> Result<(), RegistryError> {
        // 1. 获取credential的所有模型
        let model_ids = {
            let cred_index = self.credential_index.read().await;
            cred_index.get(credential_id).cloned().unwrap_or_default()
        };

        // 2. 移除模型
        for model_id in &model_ids {
            self.remove_model(model_id).await?;
        }

        // 3. 清理credential索引
        {
            let mut cred_index = self.credential_index.write().await;
            cred_index.remove(credential_id);
        }

        // 4. 清理别名缓存
        self.alias_cache.clear_credential(credential_id).await;

        // 5. 清理冷却状态
        {
            let mut state = self.cooling_state.write().await;
            state.remove(credential_id);
        }

        // 6. 清理配额状态
        {
            let mut state = self.quota_state.write().await;
            state.remove(credential_id);
        }

        Ok(())
    }

    // 辅助方法
    fn split_prefix(input: &str) -> (Option<String>, &str) {
        if let Some(pos) = input.find('/') {
            let prefix = &input[..pos];
            let rest = &input[pos + 1..];
            (Some(prefix.to_string()), rest)
        } else {
            (None, input)
        }
    }

    fn extract_config_suffix(name: &str) -> Option<String> {
        // 提取配置中的后缀，如 "(low)", "(high)"
        ModelAliasCache::extract_suffix(name)
    }

    async fn check_credential_availability(&self, credential_id: &str) -> bool {
        // 检查credential是否存在
        let cred_index = self.credential_index.read().await;
        cred_index.contains_key(credential_id)
    }

    async fn get_credential_cooling_until(&self, credential_id: &str) -> Option<DateTime<Utc>> {
        let state = self.cooling_state.read().await;
        state.get(credential_id).copied()
    }

    async fn get_quota_info(&self, credential_id: &str) -> Option<QuotaInfo> {
        let state = self.quota_state.read().await;
        state.get(credential_id).cloned()
    }
}
```

## 二、配置加载实现

```rust
impl Config {
    /// 规范化Gemini API keys配置
    pub fn sanitize_gemini_keys(&mut self) {
        if self.gemini_keys.is_empty() {
            return;
        }

        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();

        for mut key in self.gemini_keys.drain(..) {
            // 规范化API key
            key.api_key = key.api_key.trim().to_string();
            if key.api_key.is_empty() {
                continue;
            }

            // 规范化base_url
            key.base_url = key.base_url.map(|s| s.trim().to_string());

            // 规范化proxy_url
            key.proxy_url = key.proxy_url.map(|s| s.trim().to_string());

            // 规范化前缀
            key.prefix = key.prefix.map(|s| s.trim_matches('/').to_string());

            // 规范化headers
            key.headers = key
                .headers
                .into_iter()
                .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
                .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                .collect();

            // 规范化excluded_models
            key.excluded_models = key
                .excluded_models
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // 规范化模型别名
            key.models = key
                .models
                .into_iter()
                .filter_map(|mut model| {
                    model.name = model.name.trim().to_string();
                    model.alias = model.alias.trim().to_string();

                    if model.name.is_empty() || model.alias.is_empty() {
                        None
                    } else {
                        Some(model)
                    }
                })
                .collect();

            // 去重（基于api_key + base_url）
            let unique_key = format!("{}|{}", key.api_key, key.base_url.as_deref().unwrap_or(""));
            if seen.insert(unique_key) {
                keys.push(key);
            }
        }

        self.gemini_keys = keys;
    }

    /// 规范化OAuth模型别名
    pub fn sanitize_oauth_model_alias(&mut self) {
        for (provider, aliases) in self.oauth_model_alias.iter_mut() {
            let sanitized: Vec<_> = aliases
                .drain(..)
                .filter_map(|mut alias| {
                    alias.name = alias.name.trim().to_string();
                    alias.alias = alias.alias.trim().to_string();

                    if alias.name.is_empty() || alias.alias.is_empty() {
                        None
                    } else {
                        Some(alias)
                    }
                })
                .collect();

            *aliases = sanitized;
        }

        // 移除空的provider条目
        self.oauth_model_alias.retain(|_, aliases| !aliases.is_empty());
    }

    /// 迁移旧配置格式
    pub fn migrate_legacy_config(&mut self) {
        if self.providers.is_empty() {
            return;
        }

        // 将旧的ProviderConfig转换为新的ProviderCredential
        for (provider_name, old_config) in &self.providers {
            if !old_config.enabled {
                continue;
            }

            if let Some(api_key) = &old_config.api_key {
                let credential = ProviderCredential {
                    api_key: api_key.clone(),
                    priority: 0,
                    prefix: None,
                    base_url: old_config.base_url.clone(),
                    proxy_url: None,
                    models: old_config
                        .models
                        .iter()
                        .map(|m| ModelAlias {
                            name: m.clone(),
                            alias: m.clone(),
                        })
                        .collect(),
                    headers: HashMap::new(),
                    excluded_models: Vec::new(),
                    disable_cooling: false,
                };

                match provider_name.to_lowercase().as_str() {
                    "gemini" => self.gemini_keys.push(credential),
                    "claude" => self.claude_keys.push(credential),
                    "codex" => self.codex_keys.push(credential),
                    _ => continue,
                }
            }
        }

        // 清空旧配置
        self.providers.clear();
    }
}
```

## 三、API Handler集成

```rust
use axum::extract::State;
use axum::Json;

/// 模型列表API - 返回包含别名信息的模型列表
pub async fn list_models_handler(
    State(registry): State<Arc<ModelRegistry>>,
    credential_id: Option<String>,
) -> Json<ModelsResponse> {
    let models = if let Some(cred_id) = credential_id {
        registry
            .list_models_with_aliases(&cred_id)
            .await
    } else {
        registry.list_models().await
    };

    let data = models
        .into_iter()
        .map(|model| Model {
            id: model.id,
            object: "model".to_string(),
            created: model.updated_at.timestamp(),
            owned_by: model.provider.clone(),
        })
        .collect();

    Json(ModelsResponse {
        object: "list".to_string(),
        data,
    })
}

/// Chat Completion API - 使用别名解析和credential选择
pub async fn chat_completion_handler(
    State(registry): State<Arc<ModelRegistry>>,
    State(selector): State<Arc<dyn CredentialSelector>>,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, ApiError> {
    // 1. 解析模型别名
    let (upstream_model, suffix) = registry
        .resolve_model_alias(&request.model, &request.model)
        .await?;

    // 2. 获取可用的credentials
    let provider = extract_provider_from_model(&upstream_model);
    let candidates = registry
        .get_credentials_for_provider(&provider, &upstream_model)
        .await;

    // 3. 选择credential
    let selected = selector
        .select(candidates, 0)
        .await
        .ok_or_else(|| ApiError::NoAvailableCredential)?;

    // 4. 执行请求
    // ... 使用selected.credential_id和upstream_model执行实际请求

    Ok(Json(response))
}

fn extract_provider_from_model(model: &str) -> String {
    // 根据模型ID推断provider
    if model.starts_with("gemini-") || model.starts_with("gpt-") {
        "gemini".to_string()
    } else if model.starts_with("claude-") {
        "anthropic".to_string()
    } else {
        "openai".to_string()
    }
}
```

## 四、测试示例

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_alias_resolution() {
        let registry = ModelRegistry::new();

        // 注册credential
        let credential = ProviderCredential {
            api_key: "sk-test".to_string(),
            priority: 0,
            prefix: None,
            base_url: None,
            proxy_url: None,
            models: vec![
                ModelAlias {
                    name: "gemini-2.5-pro-exp-03-25".to_string(),
                    alias: "g25p".to_string(),
                },
                ModelAlias {
                    name: "gemini-2.5-flash(low)".to_string(),
                    alias: "g25f".to_string(),
                },
            ],
            headers: HashMap::new(),
            excluded_models: Vec::new(),
            disable_cooling: false,
        };

        registry
            .register_credential(
                "cred-1".to_string(),
                "gemini".to_string(),
                ProviderType::APIKey,
                credential,
            )
            .await
            .unwrap();

        // 测试别名解析
        let (upstream, suffix) = registry
            .resolve_model_alias("cred-1", "g25p")
            .await
            .unwrap();
        assert_eq!(upstream, "gemini-2.5-pro-exp-03-25");
        assert!(suffix.is_none());

        // 测试带用户后缀的别名
        let (upstream, suffix) = registry
            .resolve_model_alias("cred-1", "g25p(8192)")
            .await
            .unwrap();
        assert_eq!(upstream, "gemini-2.5-pro-exp-03-25");
        assert_eq!(suffix, Some("(8192)".to_string()));

        // 测试配置后缀优先
        let (upstream, suffix) = registry
            .resolve_model_alias("cred-1", "g25f(high)")
            .await
            .unwrap();
        assert_eq!(upstream, "gemini-2.5-flash(low)");
        assert_eq!(suffix, Some("(low)".to_string()));
    }

    #[tokio::test]
    async fn test_credential_selection() {
        let registry = ModelRegistry::new();
        let selector = Arc::new(PrioritySelector);

        // 注册多个credentials
        for i in 0..3 {
            let credential = ProviderCredential {
                api_key: format!("sk-test-{}", i),
                priority: i as i32,
                prefix: None,
                base_url: None,
                proxy_url: None,
                models: vec![ModelAlias {
                    name: "gemini-pro".to_string(),
                    alias: "gpt".to_string(),
                }],
                headers: HashMap::new(),
                excluded_models: Vec::new(),
                disable_cooling: false,
            };

            registry
                .register_credential(
                    format!("cred-{}", i),
                    "gemini".to_string(),
                    ProviderType::APIKey,
                    credential,
                )
                .await
                .unwrap();
        }

        // 测试选择最高优先级的credential
        let candidates = registry
            .get_credentials_for_provider("gemini", "gpt")
            .await;

        let selected = selector.select(candidates, 0).await.unwrap();
        assert_eq!(selected.priority, 2); // 最高优先级
    }

    #[tokio::test]
    async fn test_prefix_namespace() {
        let registry = ModelRegistry::new();

        let credential = ProviderCredential {
            api_key: "sk-test".to_string(),
            priority: 0,
            prefix: Some("teamA".to_string()),
            base_url: None,
            proxy_url: None,
            models: vec![ModelAlias {
                name: "gemini-pro".to_string(),
                alias: "gpt".to_string(),
            }],
            headers: HashMap::new(),
            excluded_models: Vec::new(),
            disable_cooling: false,
        };

        registry
            .register_credential(
                "cred-1".to_string(),
                "gemini".to_string(),
                ProviderType::APIKey,
                credential,
            )
            .await
            .unwrap();

        // 测试带前缀的别名解析
        let (upstream, suffix) = registry
            .resolve_model_alias("cred-1", "teamA/gpt")
            .await
            .unwrap();
        assert_eq!(upstream, "teamA/gemini-pro");
    }
}
```

## 五、性能优化建议

### 5.1 批量注册

```rust
impl ModelRegistry {
    pub async fn register_credentials_batch(
        &self,
        credentials: Vec<(String, String, ProviderType, ProviderCredential)>,
    ) -> Result<(), RegistryError> {
        // 批量写入锁，减少锁竞争
        let mut models = self.models.write().await;
        let mut index = self.provider_index.write().await;
        let mut cred_index = self.credential_index.write().await;

        for (credential_id, provider, provider_type, config) in credentials {
            // 批量注册逻辑
        }

        Ok(())
    }
}
```

### 5.2 缓存预热

```rust
impl ModelRegistry {
    pub async fn warmup_cache(&self, config: &Config) {
        // 预加载所有模型别名到缓存
        for (cred_id, cred_config) in &config.gemini_keys {
            for model_alias in &cred_config.models {
                self.alias_cache
                    .insert(
                        cred_id.clone(),
                        model_alias.alias.clone(),
                        model_alias.name.clone(),
                        None,
                    )
                    .await;
            }
        }
    }
}
```

### 5.3 懒加载

```rust
impl ModelRegistry {
    pub async fn get_model_lazy(&self, id: &str) -> Result<ModelInfo, RegistryError> {
        // 先从缓存查找
        if let Some(model) = self.models.read().await.get(id).cloned() {
            return Ok(model);
        }

        // 尝试从远程获取
        if let Some(config) = &self.remote_config {
            // 异步加载逻辑
        }

        Err(RegistryError::ModelNotFound)
    }
}
```