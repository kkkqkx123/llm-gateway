# HTTP API 设计文档

## 1. 概述

本文档描述了 LLM Gateway 的 HTTP API 设计。该 API 提供统一的接口来访问多个 AI 提供商的服务，支持格式转换、请求路由、缓存等功能。

## 2. 设计原则

- **兼容性**: 兼容 OpenAI API 格式，作为主要接口标准
- **灵活性**: 支持多种 AI 提供商和模型
- **可扩展性**: 易于添加新的提供商和功能
- **高性能**: 支持流式响应和请求缓存
- **可观测性**: 完善的日志和监控

## 3. 技术选型

### 3.1 HTTP 框架
使用 **Axum** 作为 HTTP 服务器框架：
- 基于 Tokio 异步运行时，性能优异
- 类型安全的路由系统
- 支持中间件和状态管理
- 良好的错误处理机制

### 3.2 HTTP 客户端
使用 **Reqwest** 作为 HTTP 客户端：
- 异步 HTTP 请求
- 支持 HTTP/2
- 良好的连接池管理

## 4. API 端点设计

### 4.1 聊天补全接口

**端点**: `POST /v1/chat/completions`

**描述**: 创建聊天补全请求，这是核心接口

**请求格式**:
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [
    {
      "role": "user",
      "content": "Hello, how are you?"
    }
  ],
  "stream": true,
  "temperature": 0.7,
  "max_tokens": 1000
}
```

**响应格式** (非流式):
```json
{
  "id": "chatcmpl-xxx",
  "object": "chat.completion",
  "created": 1234567890,
  "model": "claude-3-5-sonnet-20241022",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "I'm doing well, thank you!"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 10,
    "completion_tokens": 8,
    "total_tokens": 18
  }
}
```

**响应格式** (流式):
```
data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk",...}

data: [DONE]
```

**流程**:
1. 解析请求，提取模型 ID 和参数
2. 从注册表查询模型信息
3. 根据模型提供商选择转换器
4. 转换请求格式
5. 检查缓存（如果启用）
6. 转发到目标提供商
7. 转换响应格式
8. 返回给客户端

**错误处理**:
- 400: 请求参数错误
- 404: 模型不存在
- 429: 速率限制
- 500: 服务器内部错误
- 502: 上游服务错误
- 503: 服务不可用

### 4.2 模型列表接口

**端点**: `GET /v1/models`

**描述**: 获取所有可用的模型列表

**响应格式**:
```json
{
  "object": "list",
  "data": [
    {
      "id": "claude-3-5-sonnet-20241022",
      "object": "model",
      "created": 1234567890,
      "owned_by": "anthropic"
    }
  ]
}
```

**查询参数**:
- `provider`: 按提供商筛选（可选）
- `capability`: 按能力筛选（可选，如：thinking, streaming）

### 4.3 模型详情接口

**端点**: `GET /v1/models/{model_id}`

**描述**: 获取特定模型的详细信息

**响应格式**:
```json
{
  "id": "claude-3-5-sonnet-20241022",
  "object": "model",
  "created": 1234567890,
  "owned_by": "anthropic",
  "capabilities": {
    "thinking": true,
    "streaming": true,
    "function_calling": false,
    "multimodal": false,
    "max_context_length": 200000
  },
  "pricing": {
    "input_price": 0.003,
    "output_price": 0.015,
    "currency": "USD"
  }
}
```

### 4.4 健康检查接口

**端点**: `GET /health`

**描述**: 检查服务健康状态

**响应格式**:
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime": 3600,
  "checks": {
    "cache": "ok",
    "registry": "ok"
  }
}
```

### 4.5 配置管理接口

**端点**: `GET /config`

**描述**: 获取当前配置（仅返回非敏感信息）

**响应格式**:
```json
{
  "server": {
    "host": "0.0.0.0",
    "port": 8080
  },
  "routing": {
    "strategy": "round_robin",
    "max_retries": 3
  },
  "thinking": {
    "enabled": true
  }
}
```

**端点**: `POST /config/reload`

**描述**: 重新加载配置文件

**响应格式**:
```json
{
  "status": "success",
  "message": "Configuration reloaded successfully"
}
```

## 5. 请求处理流程

```
客户端请求
    ↓
[中间件层]
  - 日志记录
  - 请求验证
  - 认证（可选）
  - 速率限制（可选）
    ↓
[路由层]
  - 路由匹配
    ↓
[处理器层]
  - 参数解析
  - 模型查询
  - 格式转换
    ↓
[业务逻辑层]
  - 缓存检查
  - 请求路由
  - 提供商通信
    ↓
[响应处理]
  - 格式转换
  - 流式处理
    ↓
客户端响应
```

## 6. 中间件设计

### 6.1 日志中间件
- 记录所有请求和响应
- 记录请求 ID
- 记录处理时间

### 6.2 错误处理中间件
- 统一错误响应格式
- 错误日志记录

### 6.3 CORS 中间件
- 支持跨域请求
- 可配置允许的来源

### 6.4 请求 ID 中间件
- 生成唯一请求 ID
- 用于日志追踪

## 7. 状态管理

### 7.1 应用状态
```rust
struct AppState {
    config: Arc<Config>,
    registry: Arc<ModelRegistry>,
    cache: Arc<Cache>,
    client: Arc<HttpClient>,
}
```

### 7.2 共享资源
- 配置：读多写少，使用 Arc<RwLock>
- 注册表：读写均衡，使用 Arc<RwLock>
- 缓存：高并发读写，使用自定义 LRU 缓存

## 8. 错误处理

### 8.1 错误类型
```rust
enum ApiError {
    BadRequest(String),
    NotFound(String),
    Unauthorized(String),
    RateLimitExceeded(String),
    UpstreamError(String),
    InternalError(String),
}
```

### 8.2 错误响应格式
```json
{
  "error": {
    "message": "Model not found",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

## 9. 性能优化

### 9.1 缓存策略
- 对相同的请求进行缓存
- TTL 配置
- 缓存键包含模型和参数

### 9.2 连接池
- HTTP 客户端连接池
- 最大连接数配置
- 连接超时配置

### 9.3 流式响应
- 支持 SSE (Server-Sent Events)
- 减少首字节时间
- 实时返回内容

## 10. 安全考虑

### 10.1 API 密钥验证
- 从请求头提取 API 密钥
- 验证密钥有效性
- 记录使用情况

### 10.2 速率限制
- 基于 API 密钥的速率限制
- 基于IP的速率限制
- 使用令牌桶算法

### 10.3 输入验证
- 验证请求参数
- 限制消息长度
- 防止注入攻击

## 11. 测试策略

### 11.1 单元测试
- 每个处理器的单元测试
- 中间件测试
- 错误处理测试

### 11.2 集成测试
- 端到端请求测试
- 流式响应测试
- 错误场景测试

### 11.3 性能测试
- 负载测试
- 并发测试
- 内存泄漏检测

## 12. 监控和日志

### 12.1 日志级别
- ERROR: 错误和异常
- WARN: 警告信息
- INFO: 请求和响应
- DEBUG: 详细调试信息

### 12.2 监控指标
- 请求数量
- 响应时间
- 错误率
- 缓存命中率
- 上游服务可用性

## 13. 未来扩展

### 13.1 支持的功能
- 函数调用
- 多模态输入
- 批处理请求
- Webhook 通知

### 13.2 管理接口
- API 密钥管理
- 用量统计
- 审计日志
- 配置编辑器

### 13.3 高级特性
- 请求排队
- 优先级调度
- 成本预估
- 模型路由策略