# Rust 实现设计方案
## 设计原则
基于 Go 参考项目 CLIProxyAPI 的结构，本方案专注于高层设计决策，具体实现细节可从参考项目对应文件借鉴。

## 一、项目结构
```text
src/
├── lib.rs
├── format/
│   ├── mod.rs
│   ├── types.rs          # 核心类型定义（借鉴 ref/CLIProxyAPI/sdk/translator/types.go）
│   ├── registry.rs       # 转换器注册表（借鉴 ref/CLIProxyAPI/sdk/translator/registry.go）
│   ├── pipeline.rs       # 转换管道（借鉴 ref/CLIProxyAPI/sdk/translator/pipeline.go）
│   └── converters/       # 各格式转换实现
│       ├── openai.rs
│       ├── claude.rs
│       └── gemini.rs
└── scheduler/
    ├── mod.rs
    ├── types.rs          # 调度器类型（借鉴 ref/CLIProxyAPI/sdk/cliproxy/auth/types.go）
    ├── heap.rs           # 最小堆实现（替代 Go 的 container/heap）
    └── scheduler.rs      # 调度器主逻辑（借鉴 ref/CLIProxyAPI/sdk/cliproxy/auth/auto_refresh_loop.go）
```

## 二、格式转换系统设计
### 2.1 核心类型定义（借鉴 translator/types.go）
| Rust 类型 | 对应 Go 类型 | 来源文件 |
|-----------|-------------|----------|
| `Format` enum | `type Format string` | translator/format.go |
| `RequestEnvelope` struct | `RequestEnvelope struct` | translator/pipeline.go |
| `ResponseEnvelope` struct | `ResponseEnvelope struct` | translator/pipeline.go |
| `RequestTransformer` trait | `RequestTransform func type` | translator/types.go |
| `StreamResponseTransformer` trait | `ResponseStreamTransform func type` | translator/types.go |
| `NonStreamResponseTransformer` trait | `ResponseNonStreamTransform func type` | translator/types.go |
| `TokenCountTransformer` trait | `ResponseTokenCountTransform func type` | translator/types.go |

**设计决策：**
- Go 使用函数类型，Rust 使用 trait，利用 Rust 的零成本抽象
- `Format` 使用 enum 替代 Go 的字符串，提供编译期类型安全
- 参考 translator/formats.go 定义所有支持的格式变体

### 2.2 注册表设计（借鉴 translator/registry.go）
**关键结构：**
```rust
// 借鉴 Go 的 Registry struct
pub struct Registry {
    requests: RwLock<HashMap<(Format, Format), Arc<dyn RequestTransformer>>>,
    responses: RwLock<HashMap<(Format, Format), ResponseTransformers>>,
}
```

**核心方法（参考 registry.go）：**
| 方法名 | 对应 Go 方法 | 说明 |
|--------|-------------|------|
| `register_request` | `Register` | 注册请求转换器 |
| `transform_request` | `TranslateRequest` | 执行请求转换 |
| `has_response_transformer` | `HasResponseTransformer` | 检查转换器存在 |
| `transform_stream` | `TranslateStream` | 流式响应转换 |
| `transform_non_stream` | `TranslateNonStream` | 非流式响应转换 |

**设计决策：**
- 使用 `RwLock` 替代 Go 的 sync.RWMutex，实现读写锁
- 使用 `Arc<dyn Trait>` 替代 Go 的裸函数指针，支持线程安全共享
- 参考 registry.go:49-66 的降级处理逻辑

### 2.3 管道设计（借鉴 translator/pipeline.go）
**关键结构：**
```rust
// 借鉴 Go 的 Pipeline struct
pub struct Pipeline {
    registry: Arc<Registry>,
    request_middleware: Vec<RequestMiddleware>,
    response_middleware: Vec<ResponseMiddleware>,
}
```

**核心方法（参考 pipeline.go）：**
| 方法名 | 对应 Go 方法 | 说明 |
|--------|-------------|------|
| `use_request` | `UseRequest` | 注册请求中间件 |
| `use_response` | `UseResponse` | 注册响应中间件 |
| `translate_request` | `TranslateRequest` | 请求转换（含中间件链） |
| `translate_response` | `TranslateResponse` | 响应转换（含中间件链） |

**中间件类型设计：**
- **Go**: `RequestMiddleware func(ctx context.Context, req RequestEnvelope, next RequestHandler) (RequestEnvelope, error)`
- **Rust**: 使用 trait 或闭包类型实现洋葱模型

**设计决策：**
- 中间件执行顺序：按注册逆序执行（洋葱模型），与 Go 实现保持一致
- 参考 pipeline.go:72-82 的中间件链构造逻辑

## 三、轮询调度系统设计
### 3.1 核心类型（借鉴 auth/auto_refresh_loop.go）
**关键结构（参考 auto_refresh_loop.go:13-25）：**
```rust
// 借鉴 authAutoRefreshLoop
pub struct RefreshScheduler {
    manager: Arc<dyn AuthManager>,
    interval: Duration,
    concurrency: usize,
    
    heap: Mutex<RefreshHeap>,
    index: RwLock<HashMap<String, HeapItem>>,
    dirty: Mutex<HashMap<String, ()>>,
    
    wake_tx: mpsc::Sender<()>,
    jobs_tx: mpsc::Sender<String>,
}
```

**堆项设计（参考 auto_refresh_loop.go:417-420）：**
```rust
// 借鉴 refreshHeapItem
pub struct HeapItem {
    id: String,
    next: Instant,
}
```

**设计决策：**
- 使用 Rust 标准库 `BinaryHeap<Reverse<HeapItem>>` 替代 Go 的 container/heap
- 需要为 `HeapItem` 实现 `Ord` trait，按 `next` 时间排序
- 参考 auto_refresh_loop.go:423-435 的堆接口设计

### 3.2 调度器方法（借鉴 auto_refresh_loop.go）
| 方法名 | 对应 Go 方法 | 来源行号 | 说明 |
|--------|-------------|--------|------|
| `reschedule` | `queueReschedule` | 49-60 | 标记脏数据并唤醒 |
| `rebuild` | `rebuild` | 93-120 | 重建堆 |
| `run` | `run` | 62-76 | 启动调度器 |
| `schedule_loop` | `loop` | 122-150 | 主调度循环 |
| `handle_due` | `handleDue` | 188-199 | 处理到期任务 |
| `apply_dirty` | `applyDirty` | 274-291 | 应用脏标记 |

### 3.3 调度循环设计（参考 auto_refresh_loop.go:122-150）
**核心逻辑：**
- 使用 `tokio::select!` 替代 Go 的 select 语句
- 监听三个事件源：
  1. 定时器（根据堆顶计算等待时间）
  2. 唤醒通道（wakeCh）
  3. 上下文取消

**设计决策：**
- Go 使用 time.Timer 和 time.NewTimer，Rust 使用 `tokio::time::sleep + select!`
- 参考 auto_refresh_loop.go:152-177 的定时器重置逻辑

### 3.4 工作池设计（参考 auto_refresh_loop.go:78-91）
**关键方法：**
| 方法名 | 对应 Go 方法 | 说明 |
|--------|-------------|------|
| `spawn_worker` | 内联在 run 中 | 创建 worker 任务 |
| `handle_due_auth` | `handleDueAuth` | 处理单个认证刷新 |

**设计决策：**
- 使用 `tokio::spawn` 创建异步任务替代 Go 的 goroutine
- 使用 `mpsc::channel` 作为任务队列替代 Go 的 `chan string`
- 参考 auto_refresh_loop.go:221-272 的刷新决策逻辑

## 四、关键设计对比
### 4.1 并发原语映射
| Go | Rust | 用途 |
|----|------|------|
| sync.RWMutex | tokio::sync::RwLock | 读写锁 |
| sync.Mutex | tokio::sync::Mutex | 互斥锁 |
| chan T | mpsc::Channel<T> | 通道 |
| chan struct{} | mpsc::Sender<()> | 信号通知 |
| goroutine | tokio::task | 异步任务 |
| context.Context | tokio_util::sync::CancellationToken | 取消信号 |

### 4.2 数据结构映射
| Go | Rust | 说明 |
|----|------|------|
| container/heap | `BinaryHeap<Reverse<T>>` | 最小堆 |
| map[K]V | `HashMap<K, V>` | 哈希表 |
| time.Time | Instant | 时间点 |
| time.Duration | Duration | 时间间隔 |

### 4.3 错误处理
**Go（参考项目）：**
- 使用 error 接口
- 显式返回 `(T, error)`

**Rust：**
- 使用 thiserror 定义错误 enum
- 使用 `Result<T, E>` 类型
- 实现 `From` trait 进行错误转换

## 五、接口设计（借鉴 SDK 设计）
### 5.1 Provider 接口（借鉴 cliproxy/types.go）
**Go 参考：**
- TokenClientProvider interface (line 17-28)
- APIKeyClientProvider interface (line 39-50)

**Rust 设计：**
```rust
pub trait TokenClientProvider: Send + Sync {
    async fn load(&self, cfg: &Config) -> Result<TokenClientResult, SchedulerError>;
}

pub trait APIKeyClientProvider: Send + Sync {
    async fn load(&self, cfg: &Config) -> Result<APIKeyClientResult, SchedulerError>;
}
```

### 5.2 Watcher 接口（借鉴 cliproxy/watcher.go）
**Go 参考：**
- WatcherFactory function type (line 81)
- WatcherWrapper struct (line 84-92)

**Rust 设计：**
```rust
pub trait Watcher: Send + Sync {
    async fn start(&self) -> Result<(), SchedulerError>;
    async fn stop(&self) -> Result<(), SchedulerError>;
    fn set_config(&self, cfg: Config);
}
```

## 六、实现优先级
1. 第一阶段：format/types.rs + format/registry.rs（核心转换功能）
2. 第二阶段：format/pipeline.rs（中间件支持）
3. 第三阶段：scheduler/types.rs + scheduler/heap.rs（调度基础）
4. 第四阶段：scheduler/scheduler.rs（完整调度逻辑）
5. 第五阶段：converters/ 具体转换器实现

## 七、参考文件清单
| 功能 | Go 文件路径 | Rust 对应文件 |
|------|------------|-------------|
| 格式定义 | translator/format.go | format/types.rs |
| 转换类型 | translator/types.go | format/types.rs |
| 注册表 | translator/registry.go | format/registry.rs |
| 管道 | translator/pipeline.go | format/pipeline.rs |
| 调度器 | auth/auto_refresh_loop.go | scheduler/scheduler.rs |
| 堆实现 | Go 标准库 container/heap | scheduler/heap.rs（自定义） |
| Provider | cliproxy/providers.go | providers.rs |
| Watcher | cliproxy/watcher.go | watcher.rs |
| 类型定义 | cliproxy/types.go | types.rs |