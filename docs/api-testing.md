# LLM Gateway API 测试指南

## 启动服务器

```bash
cargo run
```

服务器将在 `http://localhost:8080` 启动。

## API 端点测试

### 1. 健康检查

```bash
curl http://localhost:8080/health
```

预期响应:
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime": 1,
  "checks": {
    "cache": "ok",
    "registry": "ok"
  }
}
```

### 2. 获取配置

```bash
curl http://localhost:8080/config
```

### 3. 获取模型列表

```bash
curl http://localhost:8080/v1/models
```

### 4. 获取特定模型详情

```bash
curl http://localhost:8080/v1/models/claude-3-5-sonnet-20241022
```

### 5. 聊天补全（非流式）

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "messages": [
      {
        "role": "user",
        "content": "Hello, how are you?"
      }
    ],
    "stream": false
  }'
```

### 6. 聊天补全（流式）

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet-20241022",
    "messages": [
      {
        "role": "user",
        "content": "Hello, how are you?"
      }
    ],
    "stream": true
  }'
```

## 运行测试

```bash
cargo test
```

## 运行特定测试

```bash
cargo test test_health_check
cargo test test_get_config
cargo test test_list_models
```

## 运行带日志的测试

```bash
RUST_LOG=debug cargo test
```