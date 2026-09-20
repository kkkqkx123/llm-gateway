# 模型注册逻辑分析与增强方案

## 一、当前实现分析

### 1.1 Provider定义

**当前实现** (`src/format/types.rs`):

```rust
pub enum Provider {
    OpenAI,
    Claude,
    Gemini,
    Codex,
    Kimi,
}
```

**问题**:
- Provider仅作为枚举类型，不支持动态配置
- 没有区分Provider类型（OAuth vs API Key）
- 缺少Provider级别的配置（如base_url, headers等）

### 1.2 Model定义

**当前实现** (`src/types/model.rs`):

```rust
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub capabilities: ModelCapabilities,
    pub user_defined: bool,
    pub version: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub description: Option<String>,
    pub pricing: Option<ModelPricing>,
}
```

**问题**:
- 没有模型别名机制
- 无法区分客户端模型名和上游模型名
- 缺少模型前缀命名空间支持

### 1.3 ProviderConfig定义

**当前实现** (`src/config/types.rs`):

```rust
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub models: Vec<String>,
    pub enabled: bool,
}
```

**问题**:
- 每个provider只支持一个API key
- 没有per-key配置（如priority, prefix, headers, proxy_url）
- 没有模型别名映射
- 没有模型排除列表
- 没有Priority字段用于负载均衡

### 1.4 模型注册逻辑

**当前实现** (`src/registry.rs`):

```rust
pub async fn add_model(&self, model: ModelInfo) -> Result<(), RegistryError>
pub async fn get_model(&self, id: &str) -> Result<ModelInfo, RegistryError>
```

**问题**:
- 直接使用模型ID查询，没有别名解析
- 没有per-credential的模型注册机制
- 缺少模型前缀命名空间处理

## 二、参考实现分析

### 2.1 模型别名支持

**Go实现** (`internal/config/config.go`):

```go
type GeminiModel struct {
    Name  string `yaml:"name"`   // 上游模型名
    Alias string `yaml:"alias"`  // 客户端别名
}

type OpenAICompatibilityModel struct {
    Name  string `yaml:"name"`
    Alias string `yaml:"alias"`
    Image bool   `yaml:"image,omitempty"`
}
```

**特性**:
- 支持上游模型名和客户端别名分离
- 支持模型级别的额外配置（如image标记）
- 支持全局OAuth模型别名映射

### 2.2 多API Key支持

**Go实现** (`internal/config/config.go`):

```go
type GeminiKey struct {
    APIKey          string         `yaml:"api-key"`
    Priority        int            `yaml:"priority,omitempty"`
    Prefix          string         `yaml:"prefix,omitempty"`
    BaseURL         string         `yaml:"base-url,omitempty"`
    ProxyURL        string         `yaml:"proxy-url,omitempty"`
    Models          []GeminiModel  `yaml:"models,omitempty"`
    Headers         map[string]string `yaml:"headers,omitempty"`
    ExcludedModels  []string       `yaml:"excluded-models,omitempty"`
    DisableCooling  bool           `yaml:"disable-cooling,omitempty"`
}

type OpenAICompatibility struct {
    Name            string                         `yaml:"name"`
    Priority        int                            `yaml:"priority,omitempty"`
    Disabled        bool                           `yaml:"disabled,omitempty"`
    Prefix          string                         `yaml:"prefix,omitempty"`
    BaseURL         string                         `yaml:"base-url"`
    APIKeyEntries   []OpenAICompatibilityAPIKey    `yaml:"api-key-entries,omitempty"`
    Models          []OpenAICompatibilityModel     `yaml:"models"`
    Headers         map[string]string              `yaml:"headers,omitempty"`
    DisableCooling  bool                           `yaml:"disable-cooling,omitempty"`
}
```

**特性**:
- 支持一个provider配置多个API key
- 支持per-key的priority、prefix、proxy_url、headers配置
- 支持per-key的模型排除列表
- 支持模型前缀命名空间（用于多租户隔离）
- 支持冷却策略禁用

### 2.3 模型别名解析

**Go实现** (`sdk/cliproxy/auth/types.go`):

```go
func (mgr *Manager) lookupAPIKeyUpstreamModel(authID, input string) string
```

**特性**:
- 根据auth ID解析客户端模型名为上游模型名
- 支持模型后缀（如thinking budget）保留
- 配置后缀优先于用户后缀
- 支持大小写不敏感匹配

## 三、实现方案

### 3.1 类型定义增强

#### 3.1.1 模型别名类型

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelAlias {
    pub name: String,      // 上游模型名
    pub alias: String,     // 客户端别名
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAICompatModel {
    pub name: String,      // 上游模型名
    pub alias: String,     // 客户端别名
    #[serde(default)]
    pub image: bool,       // 是否支持图像生成
}
```

#### 3.1.2 增强的Provider配置

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct APIKeyEntry {
    pub api_key: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCredential {
    pub api_key: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelAlias>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub excluded_models: Vec<String>,
    #[serde(default)]
    pub disable_cooling: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenAICompatProvider {
    pub name: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub prefix: Option<String>,
    pub base_url: String,
    #[serde(default)]
    pub api_key_entries: Vec<APIKeyEntry>,
    pub models: Vec<OpenAICompatModel>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub disable_cooling: bool,
}
```

### 3.2 配置结构调整

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub gemini_keys: Vec<ProviderCredential>,
    #[serde(default)]
    pub claude_keys: Vec<ProviderCredential>,
    #[serde(default)]
    pub codex_keys: Vec<ProviderCredential>,
    #[serde(default)]
    pub vertex_compat_keys: Vec<ProviderCredential>,
    #[serde(default)]
    pub openai_compat: Vec<OpenAICompatProvider>,
    #[serde(default)]
    pub oauth_excluded_models: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub oauth_model_alias: HashMap<String, Vec<ModelAlias>>,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub thinking: ThinkingConfig,
}
```

### 3.3 ModelInfo增强

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub alias: Option<String>,          // 新增：模型别名
    pub provider: String,
    pub provider_type: ProviderType,    // 新增：OAuth vs API Key
    pub credential_id: Option<String>,  // 新增：关联的credential ID
    pub capabilities: ModelCapabilities,
    pub user_defined: bool,
    pub version: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub description: Option<String>,
    pub pricing: Option<ModelPricing>,
    #[serde(default)]
    pub prefix: Option<String>,         // 新增：命名空间前缀
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProviderType {
    OAuth,
    APIKey,
}
```

### 3.4 ModelRegistry增强

#### 3.4.1 别名解析

```rust
impl ModelRegistry {
    pub async fn resolve_model_alias(
        &self,
        credential_id: &str,
        input: &str,
    ) -> Result<(String, Option<String>), RegistryError> {
        // 输入: "g25p(8192)" 或 "teamA/g25p"
        // 输出: ("gemini-2.5-pro-exp-03-25", Some("8192"))
        // 或 ("gemini-2.5-pro-exp-03-25", None)

        // 1. 提取前缀（如 "teamA/"）
        // 2. 提取后缀（如 "(8192)"）
        // 3. 根据credential_id查找模型别名映射
        // 4. 返回上游模型名和保留的后缀
    }

    pub async fn list_models_with_aliases(&self, credential_id: &str) -> Vec<ModelInfo> {
        // 返回指定credential的所有模型，包括别名信息
    }
}
```

#### 3.4.2 多Credential支持

```rust
impl ModelRegistry {
    pub async fn register_credential(
        &self,
        credential_id: String,
        provider: String,
        config: ProviderCredential,
    ) -> Result<(), RegistryError> {
        // 为每个credential注册模型
        // 支持前缀命名空间
        // 支持模型别名映射
        // 支持模型排除列表
    }

    pub async fn get_credentials_for_provider(
        &self,
        provider: &str,
        model: &str,
    ) -> Vec<CredentialSelection> {
        // 返回支持指定模型的所有credentials
        // 按priority排序
        // 考虑excluded_models和disable_cooling状态
    }
}

#[derive(Debug, Clone)]
pub struct CredentialSelection {
    pub credential_id: String,
    pub priority: i32,
    pub upstream_model: String,
    pub available: bool,
}
```

### 3.5 配置加载增强

```rust
impl Config {
    pub fn sanitize_gemini_keys(&mut self) {
        // 去重（api_key + base_url）
        // 规范化前缀
        // 规范化headers
        // 清理无效的模型别名
    }

    pub fn sanitize_oauth_model_alias(&mut self) {
        // 规范化全局OAuth模型别名
    }
}
```

### 3.6 示例配置

```yaml
server:
  host: "0.0.0.0"
  port: 8080

# Gemini API Keys with model aliases
gemini-api-key:
  - api-key: "sk-xxx"
    priority: 10
    base-url: "https://generativelanguage.googleapis.com"
    models:
      - name: "gemini-2.5-pro-exp-03-25"
        alias: "g25p"
      - name: "gemini-2.5-flash"
        alias: "g25f"
    headers:
      x-custom-header: "value"

# OpenAI Compatibility with multiple keys
openai-compatibility:
  - name: "kimi"
    base-url: "https://api.moonshot.cn/v1"
    priority: 5
    api-key-entries:
      - api-key: "sk-kimi-1"
      - api-key: "sk-kimi-2"
    models:
      - name: "moonshot-v1-8k"
        alias: "kimi-8k"
      - name: "moonshot-v1-32k"
        alias: "kimi-32k"
      - name: "moonshot-v1-128k"
        alias: "kimi-128k"
        image: true

# OAuth全局模型别名
oauth-model-alias:
  gemini-cli:
    - name: "gemini-2.5-pro-exp-03-25"
      alias: "pro"
    - name: "gemini-2.5-flash"
      alias: "flash"
```

## 四、实现步骤

### 阶段1：类型定义（2-3天）
1. 创建 `src/config/credential.rs` 定义ProviderCredential和相关类型
2. 扩展 `src/config/types.rs` 添加新的配置结构
3. 更新 `src/types/model.rs` 增强ModelInfo

### 阶段2：配置加载（2天）
1. 实现配置规范化方法（sanitize_*）
2. 更新ConfigLoader以支持新配置格式
3. 添加配置验证逻辑

### 阶段3：模型注册增强（3-4天）
1. 实现别名解析逻辑 `resolve_model_alias`
2. 实现per-credential模型注册 `register_credential`
3. 实现credential选择逻辑 `get_credentials_for_provider`
4. 更新ModelRegistry以支持前缀命名空间

### 阶段4：API适配（2-3天）
1. 更新模型列表API以返回别名信息
2. 更新模型路由逻辑以使用别名解析
3. 更新API handler以支持per-key配置

### 阶段5：测试与文档（2-3天）
1. 编写单元测试
2. 编写集成测试
3. 更新配置文档
4. 添加示例配置

## 五、向后兼容性

### 5.1 旧配置支持

为了向后兼容，保留旧的`providers`配置结构：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ProviderConfigLegacy {
    Old(ProviderConfig),
    New(ProviderCredential),
}

impl Config {
    pub fn migrate_legacy_config(&mut self) {
        // 将旧的providers配置迁移到新的credential结构
    }
}
```

### 5.2 渐进式迁移

1. 第一版：同时支持新旧配置格式
2. 第二版：标记旧格式为deprecated
3. 第三版：移除旧格式支持

## 六、优势总结

### 6.1 模型别名
- 简化客户端模型名（如 `g25p` 代替 `gemini-2.5-pro-exp-03-25`）
- 灵活的上游模型切换（无需修改客户端代码）
- 支持多租户命名空间隔离

### 6.2 多Key支持
- 提高可用性（单key失败自动切换）
- 负载均衡（按priority和quota分配）
- 速率限制突破（多key并行使用）

### 6.3 Per-Key配置
- 细粒度控制（每个key独立配置）
- 灵活的路由策略
- 便于多环境部署（生产/测试key分离）

### 6.4 配置灵活性
- 支持自定义headers
- 支持per-key代理配置
- 支持模型排除列表
- 支持冷却策略控制

## 七、风险与注意事项

1. **配置复杂度增加**: 新配置格式更复杂，需要完善的文档和示例
2. **向后兼容**: 需要仔细处理旧配置迁移
3. **性能影响**: 别名解析和credential选择可能增加请求延迟
4. **测试覆盖**: 需要充分的测试覆盖各种场景
5. **文档更新**: 需要同步更新用户文档和API文档

## 八、参考文件

- Go实现: `ref/CLIProxyAPI-7.1.19/internal/config/config.go`
- 模型别名测试: `ref/CLIProxyAPI-7.1.19/sdk/cliproxy/auth/api_key_model_alias_test.go`
- Auth类型定义: `ref/CLIProxyAPI-7.1.19/sdk/cliproxy/auth/types.go`
- Watcher客户端加载: `ref/CLIProxyAPI-7.1.19/internal/watcher/clients.go`