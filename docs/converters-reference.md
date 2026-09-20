# API 格式转换器快速参考

## 转换器列表

### 请求转换器 (RequestTransformer)

| 源格式 | 目标格式 | 转换器名称 | 文件位置 |
|--------|---------|-----------|---------|
| OpenAI | Claude | `OpenAIToClaudeRequestTransformer` | `converters/openai_to_claude.rs` |
| OpenAI | Gemini | `OpenAIToGeminiRequestTransformer` | `converters/openai_to_gemini.rs` |
| OpenAI | Codex | `OpenAIToCodexRequestTransformer` | `converters/special_providers.rs` |
| OpenAI | Antigravity | `OpenAIToAntigravityRequestTransformer` | `converters/special_providers.rs` |
| OpenAI | Gemini-CLI | `OpenAIToGeminiCLIRequestTransformer` | `converters/special_providers.rs` |
| Claude | OpenAI | `ClaudeToOpenAIRequestTransformer` | `converters/cross_format.rs` |
| Claude | Gemini | `ClaudeToGeminiRequestTransformer` | `converters/cross_format.rs` |
| Gemini | OpenAI | `GeminiToOpenAIRequestTransformer` | `converters/cross_format.rs` |
| Gemini | Claude | `GeminiToClaudeRequestTransformer` | `converters/cross_format.rs` |

### 流式响应转换器 (StreamResponseTransformer)

| 源格式 | 目标格式 | 转换器名称 | 文件位置 |
|--------|---------|-----------|---------|
| Claude | OpenAI | `ClaudeToOpenAIStreamResponseTransformer` | `converters/openai_to_claude.rs` |
| Gemini | OpenAI | `GeminiToOpenAIStreamResponseTransformer` | `converters/openai_to_gemini.rs` |
| Claude | Gemini | `ClaudeToGeminiStreamResponseTransformer` | `converters/cross_format.rs` |
| Gemini | Claude | `GeminiToClaudeStreamResponseTransformer` | `converters/cross_format.rs` |
| Codex | OpenAI | `CodexToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Antigravity | OpenAI | `AntigravityToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Gemini-CLI | OpenAI | `GeminiCLIToOpenAIStructureTransformer` | `converters/special_providers.rs` |

### 非流式响应转换器 (NonStreamResponseTransformer)

| 源格式 | 目标格式 | 转换器名称 | 文件位置 |
|--------|---------|-----------|---------|
| Claude | OpenAI | `ClaudeToOpenAINonStreamResponseTransformer` | `converters/openai_to_claude.rs` |
| Gemini | OpenAI | `GeminiToOpenAINonStreamResponseTransformer` | `converters/openai_to_gemini.rs` |
| Claude | Gemini | `ClaudeToGeminiNonStreamResponseTransformer` | `converters/cross_format.rs` |
| Gemini | Claude | `GeminiToClaudeNonStreamResponseTransformer` | `converters/cross_format.rs` |
| Codex | OpenAI | `CodexToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Antigravity | OpenAI | `AntigravityToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Gemini-CLI | OpenAI | `GeminiCLIToOpenAIStructureTransformer` | `converters/special_providers.rs` |

### Token 计数转换器 (TokenCountTransformer)

| 源格式 | 目标格式 | 转换器名称 | 文件位置 |
|--------|---------|-----------|---------|
| Claude | OpenAI | `ClaudeToOpenAITokenCountTransformer` | `converters/openai_to_claude.rs` |
| Gemini | OpenAI | `GeminiToOpenAITokenCountTransformer` | `converters/openai_to_gemini.rs` |
| Claude | Gemini | `ClaudeToGeminiTokenCountTransformer` | `converters/cross_format.rs` |
| Gemini | Claude | `GeminiToClaudeTokenCountTransformer` | `converters/cross_format.rs` |
| Codex | OpenAI | `CodexToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Antigravity | OpenAI | `AntigravityToOpenAIStructureTransformer` | `converters/special_providers.rs` |
| Gemini-CLI | OpenAI | `GeminiCLIToOpenAIStructureTransformer` | `converters/special_providers.rs` |

## 使用示例

### 基本使用

```rust
use crate::format::registry::*;

// 注册所有转换器
register_default_transformers();

// 转换请求
let transformed = translate_request(
    Format::OpenAI,
    Format::Claude,
    "claude-3-opus",
    &request_body,
    false,
);

// 转换流式响应
let chunks = translate_stream(
    Format::Gemini,
    Format::OpenAI,
    "gpt-4",
    &original_request,
    &translated_request,
    &response_body,
    None,
);

// 转换非流式响应
let transformed = translate_non_stream(
    Format::Claude,
    Format::OpenAI,
    "gpt-4",
    &original_request,
    &translated_request,
    &response_body,
    None,
);

// 转换 token 计数
let count = translate_token_count(
    Format::Gemini,
    Format::OpenAI,
    123,
    &original_response,
);
```

### 检查转换器是否存在

```rust
use crate::format::registry::*;

if has_response_transformer(Format::Claude, Format::OpenAI) {
    // 存在转换器，可以安全调用
    let result = translate_non_stream(/* ... */);
}
```

## 格式枚举

```rust
pub enum Format {
    OpenAI,
    Claude,
    Gemini,
    Codex,
    Antigravity,
    GeminiCLI,
}
```

## 注册转换器

```rust
use crate::format::registry::{register, ResponseTransformers};

// 注册自定义转换器
register(
    Format::OpenAI,
    Format::Custom,
    Some(Box::new(MyRequestTransformer)),
    Some(ResponseTransformers {
        stream: Some(Box::new(MyStreamTransformer)),
        non_stream: Some(Box::new(MyNonStreamTransformer)),
        token_count: Some(Box::new(MyTokenCountTransformer)),
    }),
);
```

## 转换器特性

### OpenAI 格式
- 标准 Chat Completions API 格式
- 使用 `messages` 数组
- 参数：`temperature`, `top_p`, `max_tokens`, `n` 等

### Claude 格式
- Anthropic Messages API 格式
- 使用 `messages` 数组
- 参数：`max_tokens`, `top_k`, `anthropic_version` 等
- 响应使用事件流格式（event: xxx）

### Gemini 格式
- Google Generative AI 格式
- 使用 `contents` 数组
- 参数：`maxOutputTokens`, `generationConfig` 等
- 角色映射：user ↔ user, assistant ↔ model

### Codex 格式
- 类似 OpenAI 格式
- 支持 `reasoning` 配置
- 响应格式与 OpenAI 兼容

### Antigravity 格式
- 自定义格式
- 使用 `prompt` 字段代替 `messages`
- 响应使用 `text` 字段

### Gemini-CLI 格式
- Gemini CLI 工具格式
- 简化的请求/响应格式
- 使用 `response` 字段

## 转换规则

### 消息格式转换

**OpenAI/Claude → Gemini**:
```json
// 输入
{
  "messages": [
    {"role": "user", "content": "Hello"}
  ]
}

// 输出
{
  "contents": [
    {"role": "user", "parts": [{"text": "Hello"}]}
  ]
}
```

**Gemini → OpenAI/Claude**:
```json
// 输入
{
  "contents": [
    {"role": "model", "parts": [{"text": "Hi there"}]}
  ]
}

// 输出
{
  "messages": [
    {"role": "assistant", "content": "Hi there"}
  ]
}
```

### 角色映射

| 源格式 | 目标格式 | system | user | assistant |
|--------|---------|--------|------|-----------|
| OpenAI/Claude | Gemini | → user | → user | → model |
| Gemini | OpenAI/Claude | - | → user | → assistant |
| OpenAI/Claude | OpenAI/Claude | ✓ | ✓ | ✓ |

### 参数名映射

| OpenAI | Claude | Gemini |
|--------|--------|--------|
| `max_tokens` | `max_tokens` | `maxOutputTokens` |
| `temperature` | - | - |
| `top_p` | - | - |
| - | `top_k` | - |
| - | `anthropic_version` | - |
| - | - | `generationConfig` |

## 测试

每个转换器都包含单元测试：

```bash
# 运行所有测试
cargo test

# 运行特定模块的测试
cargo test converters::cross_format
cargo test converters::special_providers

# 运行单个测试
cargo test test_claude_to_gemini_request
```

## 故障排查

### 转换失败

如果转换失败，转换器会返回原始输入：

```rust
let result = translate_request(from, to, model, &body, false);
if result == body {
    // 转换失败或没有合适的转换器
}
```

### 检查转换器存在

```rust
if !has_response_transformer(from, to) {
    // 响应转换器不存在
}
```

### 调试

启用日志查看转换过程：

```rust
env_logger::init();
// 或者
tracing_subscriber::fmt::init();
```

## 扩展

### 添加新转换器

1. 创建转换器结构体
2. 实现 `RequestTransformer`、`StreamResponseTransformer`、`NonStreamResponseTransformer` 或 `TokenCountTransformer` trait
3. 在 `registration.rs` 中注册

```rust
// 创建转换器
pub struct MyTransformer;

impl RequestTransformer for MyTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        // 实现转换逻辑
    }
}

// 注册
register(
    Format::Source,
    Format::Target,
    Some(Box::new(MyTransformer)),
    None,
);
```

### 添加新格式

1. 在 `types.rs` 中添加新的 `Format` 枚举值
2. 实现相应的转换器
3. 更新文档

```rust
pub enum Format {
    // ... 现有格式
    MyFormat,  // 新格式
}
```

## 相关文档

- [实现总结](implementation-summary.md)
- [功能缺口分析](feature-gaps.md)
- [技术规格](internal-analysis.md)