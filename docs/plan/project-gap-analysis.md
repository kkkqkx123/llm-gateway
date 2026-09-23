# llm-gateway 功能缺口分析与实施计划

## 0. 背景

项目是 Rust 单 crate 的多 Provider LLM 格式转换网关，参考实现为 Go 版 CLIProxyAPI。
骨架已齐：Format 枚举、Registry、Pipeline、Transformer trait、Thinking 扩展、
HTTP server、Config、Registry、Cache、Scheduler、Watcher、Logging。但大量策略层功能
（Credential 轮换 / Routing / Cache / TLS / 鉴权 / 超时 / Function Calling 转换）
只定义了类型字段，没有被 handler 真正消费。

## 1. 已实现（骨架层）

- Format 枚举：OpenAI / OpenAIResponse / Claude / Gemini / GeminiCLI / Codex / Antigravity
- Provider 枚举：OpenAI / Claude / Gemini / Codex / Kimi
- Registry + 8 对 request transformer + 5 对 response（stream / non-stream / token_count）transformer
- ThinkingProcessor：suffix + body 提取、7 档 Level↔budget 互转、Provider 特定 apply
- HTTP 路由：`GET /health`、`GET /config`、`POST /config/reload`、`GET /v1/models`、`GET /v1/models/:id`、`POST /v1/chat/completions`
- Config YAML 加载 + validate（server / logging / providers / routing / polling / thinking / watcher / cache / registry）
- ModelRegistry：内存 HashMap + Provider 索引 + user_defined 标记 + remote update 定时任务
- Cache：LRU + TTL + stats + SignatureCache + CacheManager（bypass、clear_all、all_stats）
- RefreshScheduler：最小堆 + AuthManager trait + broadcast worker 派发
- ConfigWatcher：notify + 事件去重 + 防抖 + 原子替换检测

## 2. 主要缺口

### P0 · 策略层没接（影响正确运行）

1. **Credential 字段没用上**
   - Chat handler 永远 `credentials.first()`，忽略 priority / model_aliases / excluded_models / model_prefix / disable_cooling / headers
2. **RoutingConfig 零消费**
   - round_robin / priority / session_affinity / max_retries 定义了但 handler 里没有多 credential 轮换、没有重试、没有失败切换
3. **CacheManager 完全摆设**
   - Server 启动时构造了，但 chat handler 不查/不写
4. **Provider→Format 映射硬编码**
   - 只覆盖 anthropic / google / openai，其它 provider fallback OpenAI

### P1 · 外围模块没接（影响可用性、可维护性）

5. **ConfigWatcher 只打日志不 reload**
   - watcher 事件只 spawn 一个 task 打印 info，不会刷新 Config、不会触发 Credential 重建
   - POST `/config/reload` handler 没有真正 reload
6. **RefreshScheduler 不启动**
   - Server 流程里根本没有实例化 scheduler 或 AuthManager
7. **ModelRegistry remote update 不启动**
   - `start_remote_update()` 没被调用；内部 60 秒 sleep 硬编码，没用 `remote_update_interval_secs`
8. **每次请求新建 reqwest::Client**
   - 连接池没复用，性能浪费

### P2 · 健壮性 & 安全

9. **TLS 没接**（ServerConfig.tls 字段存在但 axum::serve 没用 TLS acceptor）
10. **零鉴权**（没有 API key / Bearer / IP 白名单 / rate limit）
11. **reqwest Client 没超时**（后端慢会挂住整个请求）
12. **流式 SSE 解析粗糙**（没有跳过注释行、没有 heartbeat 处理）
13. **错误分类不足**（全部归为 UpstreamError，没有 429/401/5xx 细分，没触发 credential 冷却/切换）

### 已知不在本次修复范围

- predefined 模型不扩展（用户明确排除）
- Codex / GeminiCLI / Antigravity / Kimi 格式 converter 不新增（当前 handler 支持的 provider 只有 anthropic/google/openai）
- Function Calling / Tool Use 字段级精确转换（非关键，骨架能跑起来后再补）
- Token streaming 的跨 chunk 状态（reasoning_content + delta 配对、finish_reason + usage 整合）

## 3. 实施顺序

```
Phase 1 (P0): chat handler 策略层
  1.1 credential 选择：priority、model_aliases、excluded_models、model_prefix、headers
  1.2 多 credential 轮换：主 credential 失败时 fallback 到次 credential
  1.3 给 reqwest Client 加 timeout，使用 credential.headers + provider-specific headers
  1.4 接入 RoutingConfig.max_retries（在 credential 内重试）
  1.5 接入 CacheManager（请求签名缓存 + bypass）

Phase 2 (P1): 外围模块接线
  2.1 POST /config/reload 真正 reload Config；ConfigWatcher 事件 → Config reload
  2.2 启动时根据 RegistryConfig.remote_update_enabled 调用 registry.start_remote_update()
      并把 hardcoded 60s 改成 interval 动态调度
  2.3 将 reqwest::Client 提升到 AppState，全应用共享

Phase 3 (P2): 健壮性 & 安全
  3.1 ServerConfig.tls 存在时启用 axum TLS acceptor
  3.2 新增可选的 API key 鉴权（Config.gateway.auth_header + expected_key）
  3.3 改进 SSE 解析：跳过注释 `:` 行、容错空行、chunk 边界处理
  3.4 扩展 ApiError：区分 429 / 401 / 4xx / 5xx；chat handler 根据错误决定 credential 冷却
```

以下逐项落地 Phase 1-3。

