# 模型注册增强 - 新架构使用指南

## 概述

本次重构为模型注册系统添加了以下核心功能：

1. **模型别名支持** - 支持上游模型名和客户端别名分离
2. **多API Key配置** - 每个Provider支持多个API Key
3. **Credential选择策略** - 支持Priority和RoundRobin选择策略
4. **Per-Key配置** - 支持priority、prefix、headers、excluded_models等配置
5. **Credential冷却机制** - 自动管理失败凭证的冷却时间
6. **命名空间支持** - 支持模型前缀命名空间用于多租户隔离

## 架构变化

### 类型定义

新增的核心类型：

```rust
// 模型别名映射
pub struct ModelAlias {
    pub alias: String,           // 客户端别名
    pub upstream_model: String,   // 上游真实模型名
    pub credential_id: Option<String>,  // 关联的凭证ID（可选）
    pub extra_config: HashMap<String, serde_json::Value>,  // 额外配置
}

// Credential选择策略
pub enum CredentialSelection {
    Priority,     // 按优先级选择（数字越小优先级越高）
    RoundRobin,   // 轮询选择
}

// Provider凭证配置
pub struct ProviderCredential {
    pub id: String,                    // 唯一凭证ID
    pub api_key: String,               // API密钥
    pub base_url: Option<String>,      // 基础URL（覆盖Provider默认）
    pub priority: u32,                 // 优先级
    pub model_prefix: Option<String>,  // 模型前缀命名空间
    pub excluded_models: Vec<String>,  // 排除的模型列表
    pub headers: HashMap<String, String>,  // 自定义请求头
    pub proxy_url: Option<String>,     // 代理URL
    pub disable_cooling: bool,         // 禁用冷却
    pub model_aliases: Vec<ModelAlias>,  // 该凭证的模型别名
}
```

### 配置结构

新的配置格式：

```yaml
providers:
  openai:
    enabled: true
    selection_strategy: "priority"  # 或 "round_robin"
    default_models:
      - "gpt-4"
      - "gpt-4-turbo-preview"
    credentials:
      - id: "openai-key-1"
        api_key: "sk-proj-xxxxxxxxxxxx"
        priority: 10
        base_url: "https://api.openai.com/v1"
        model_prefix: "openai:"
        excluded_models: []
        headers:
          X-Custom-Header: "value"
        disable_cooling: false
        model_aliases:
          - alias: "gpt-4"
            upstream_model: "gpt-4-turbo-preview"
            credential_id: "openai-key-1"
      - id: "openai-key-2"
        api_key: "sk-proj-yyyyyyyyyyyy"
        priority: 20
        # ... 其他配置
```

## 使用示例

### 1. 注册凭证

```rust
use llm_gateway::registry::ModelRegistry;
use llm_gateway::types::registry::{ProviderCredential, CredentialSelection};

let registry = ModelRegistry::new();

let cred1 = ProviderCredential::new("openai-key-1".to_string(), "sk-proj-xxx".to_string())
    .with_priority(10)
    .with_base_url("https://api.openai.com/v1".to_string())
    .with_prefix("openai:".to_string());

let cred2 = ProviderCredential::new("openai-key-2".to_string(), "sk-proj-yyy".to_string())
    .with_priority(20);

registry
    .register_credentials("openai", vec![cred1, cred2], CredentialSelection::Priority)
    .await;
```

### 2. 选择凭证

```rust
// 自动选择凭证（基于配置的策略）
let credential = registry.select_credential("openai").await?;

// 使用凭证信息进行API调用
let api_key = &credential.api_key;
let base_url = &credential.base_url.unwrap_or_else(|| "https://api.openai.com".to_string());
```

### 3. 解析模型别名

```rust
use llm_gateway::types::registry::ModelAlias;

let alias = ModelAlias::new("gpt-4".to_string(), "gpt-4-turbo-preview".to_string())
    .with_credential("openai-key-1".to_string());

registry.alias_cache.register_alias(alias);

// 解析别名
let (upstream_model, credential_id) = registry.resolve_model_alias("gpt-4").await.unwrap();
```

### 4. Credential冷却管理

```rust
// 标记凭证为错误状态（自动冷却）
registry.mark_credential_error("openai-key-1").await;

// 重置凭证状态
registry.reset_credential("openai-key-1").await;
```

## API集成

### Chat Completion集成

API处理器现在会自动解析模型别名并选择合适的凭证：

```rust
pub async fn create_chat_completion(
    State(state): State<AppState>,
    JsonExtractor(request): JsonExtractor<ChatCompletionRequest>,
) -> ApiResult<Response> {
    // 1. 尝试解析为别名
    let (upstream_model, credential_id) = match state.registry.resolve_model_alias(&request.model).await {
        Some(resolved) => resolved,
        None => {
            // 2. 检查是否为直接模型ID
            if !state.registry.model_exists(&request.model).await {
                return Err(ApiError::NotFound(format!("Model '{}' not found", &request.model)));
            }
            (request.model.clone(), None)
        }
    };

    // 3. 获取模型信息
    let model = state.registry.get_model(&upstream_model).await?;

    // 4. 选择凭证
    let selected_credential = if credential_id.is_none() {
        state.registry.select_credential(&model.provider).await.ok()
    } else {
        None  // 使用别名指定的凭证
    };

    // 5. 处理请求...
}
```

## 测试

### 单元测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_credential_registration

# 运行集成测试
cargo test --test integration_test
```

### 配置测试

```bash
# 测试配置加载
cargo test test_config_loading_new_format

# 测试凭证选择
cargo test test_credential_selection_round_robin
```

## 性能考虑

1. **别名缓存** - 使用RwLock保护的HashMap，支持高并发读取
2. **Credential选择** - O(n)复杂度，其中n为凭证数量（通常很小）
3. **冷却机制** - 基于内存时间戳检查，开销极低

## 迁移指南

从旧架构迁移到新架构：

1. 更新配置文件格式（参见`config.example.yaml`）
2. 确保所有Provider配置都包含credentials列表
3. 更新任何直接使用`api_key`的代码
4. 测试别名解析和凭证选择功能

## 故障排查

### 问题：凭证选择失败

```rust
match registry.select_credential("openai").await {
    Ok(credential) => {
        println!("Selected credential: {}", credential.id);
    }
    Err(RegistryError::NoCredentials(provider)) => {
        eprintln!("No available credentials for provider: {}", provider);
    }
    Err(RegistryError::ProviderNotFound(provider)) => {
        eprintln!("Provider not found: {}", provider);
    }
    Err(e) => {
        eprintln!("Error selecting credential: {}", e);
    }
}
```

### 问题：别名解析失败

检查别名是否正确注册：
```rust
let resolved = registry.resolve_model_alias("your-alias").await;
if resolved.is_none() {
    eprintln!("Alias not found. Check if it was registered correctly.");
}
```

## 参考文档

- Go实现参考：`ref/CLIProxyAPI-7.1.19/internal/config/config.go`
- 增强方案：`docs/plan/model-registry-enhancement.md`
- 实现细节：`docs/plan/model-registry-implementation.md`