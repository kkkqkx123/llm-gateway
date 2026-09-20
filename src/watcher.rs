use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// 配置文件监控器
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    event_tx: mpsc::UnboundedSender<WatchEvent>,
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<WatchEvent>>>>,
    running: Arc<RwLock<bool>>,
    paths: Vec<PathBuf>,
    debounce_duration: Duration,
    atomic_replace_debounce: Duration,
}

/// 监控事件
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// 配置文件修改
    ConfigModified(PathBuf),
    /// 配置文件创建
    ConfigCreated(PathBuf),
    /// 配置文件删除
    ConfigDeleted(PathBuf),
    /// 认证目录变更
    AuthDirChanged(PathBuf),
    /// 认证文件删除（防抖）
    AuthFileDeleted(PathBuf),
    /// 错误
    Error(String),
}

impl ConfigWatcher {
    /// 创建新的监控器
    pub fn new(paths: Vec<PathBuf>) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let event_tx_clone = event_tx.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if let Some(watch_event) = Self::event_to_watch_event(&event) {
                    let _ = event_tx_clone.send(watch_event);
                }
            }
        })
        .expect("Failed to create watcher");

        for path in &paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .unwrap_or_else(|_| panic!("Failed to watch path: {:?}", path));
        }

        ConfigWatcher {
            _watcher: watcher,
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            running: Arc::new(RwLock::new(false)),
            paths,
            debounce_duration: Duration::from_millis(150),
            atomic_replace_debounce: Duration::from_secs(5),
        }
    }

    /// 设置防抖时间
    pub fn with_debounce(mut self, duration: Duration) -> Self {
        self.debounce_duration = duration;
        self
    }

    /// 设置原子替换防抖时间
    pub fn with_atomic_debounce(mut self, duration: Duration) -> Self {
        self.atomic_replace_debounce = duration;
        self
    }

    /// 获取事件接收器
    pub async fn receiver(&self) -> Option<mpsc::UnboundedReceiver<WatchEvent>> {
        let mut rx_guard = self.event_rx.write().await;
        rx_guard.take()
    }

    /// 启动监控
    pub async fn start(&self) -> Result<(), WatcherError> {
        let mut running = self.running.write().await;
        if *running {
            return Err(WatcherError::AlreadyRunning);
        }
        *running = true;
        drop(running);

        let event_tx = self.event_tx.clone();
        let paths = self.paths.clone();
        let debounce_duration = self.debounce_duration;
        let atomic_replace_debounce = self.atomic_replace_debounce;

        tokio::spawn(async move {
            if let Err(e) =
                Self::run_watcher(paths, event_tx, debounce_duration, atomic_replace_debounce).await
            {
                error!("Watcher error: {}", e);
            }
        });

        Ok(())
    }

    /// 停止监控
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// 运行文件系统监控器
    async fn run_watcher(
        paths: Vec<PathBuf>,
        event_tx: mpsc::UnboundedSender<WatchEvent>,
        _debounce_duration: Duration,
        atomic_replace_debounce: Duration,
    ) -> Result<(), WatcherError> {
        // 创建事件去重器
        let deduplicator = Arc::new(RwLock::new(EventDeduplicator::new()));
        let deduplicator_clone = deduplicator.clone();

        // 创建防抖处理器
        let (debounce_tx, mut debounce_rx) = mpsc::unbounded_channel();

        // 启动防抖任务
        let debounce_tx_clone = debounce_tx.clone();
        tokio::spawn(async move {
            Self::debounce_handler(debounce_rx, event_tx, atomic_replace_debounce).await;
        });

        // 创建 watcher
        let mut watcher: RecommendedWatcher = Watcher::new(
            move |res: NotifyResult<Event>| {
                if let Err(e) = Self::handle_event(res, &debounce_tx_clone, &deduplicator_clone) {
                    error!("Event handler error: {}", e);
                }
            },
            notify::Config::default(),
        )?;

        // 添加监控路径
        for path in &paths {
            if path.exists() {
                if path.is_dir() {
                    watcher.watch(path, RecursiveMode::Recursive)?;
                } else {
                    watcher.watch(path, RecursiveMode::NonRecursive)?;
                }
                info!("Watching path: {:?}", path);
            } else {
                warn!("Path does not exist, skipping: {:?}", path);
            }
        }

        // 等待停止信号
        loop {
            sleep(Duration::from_secs(1)).await;
        }
    }

    /// 将文件系统事件转换为监控事件
    fn event_to_watch_event(event: &Event) -> Option<WatchEvent> {
        let path = event.paths.first()?;

        let watch_event = match event.kind {
            EventKind::Create(_) => Some(WatchEvent::ConfigCreated(path.clone())),
            EventKind::Modify(_) => Some(WatchEvent::ConfigModified(path.clone())),
            EventKind::Remove(_) => Some(WatchEvent::ConfigDeleted(path.clone())),
            EventKind::Any => {
                if path
                    .extension()
                    .map_or(false, |e| e == "yaml" || e == "yml")
                {
                    Some(WatchEvent::AuthFileDeleted(path.clone()))
                } else {
                    Some(WatchEvent::AuthDirChanged(path.clone()))
                }
            }
            _ => None,
        };

        watch_event
    }

    /// 处理文件系统事件
    fn handle_event(
        res: NotifyResult<Event>,
        tx: &mpsc::UnboundedSender<DebounceEvent>,
        deduplicator: &Arc<RwLock<EventDeduplicator>>,
    ) -> Result<(), WatcherError> {
        let event = res?;

        let path = event.paths.first().ok_or(WatcherError::InvalidPath)?;

        // 去重处理 - 使用阻塞方式
        {
            let mut dedup = deduplicator.blocking_write();
            if dedup.should_ignore(&event) {
                debug!("Deduplicated event: {:?}", path);
                return Ok(());
            }
            drop(dedup);
        }

        let watch_event = match event.kind {
            EventKind::Create(_) => Some(WatchEvent::ConfigCreated(path.clone())),
            EventKind::Modify(_) => Some(WatchEvent::ConfigModified(path.clone())),
            EventKind::Remove(_) => Some(WatchEvent::ConfigDeleted(path.clone())),
            EventKind::Any => {
                // 认证文件原子替换检测
                if path
                    .extension()
                    .map_or(false, |e| e == "yaml" || e == "yml")
                {
                    Some(WatchEvent::AuthFileDeleted(path.clone()))
                } else {
                    Some(WatchEvent::AuthDirChanged(path.clone()))
                }
            }
            _ => None,
        };

        if let Some(ev) = watch_event {
            // 根据事件类型选择防抖时间
            let debounce_duration = match &ev {
                WatchEvent::AuthFileDeleted(_) => Duration::from_secs(1),
                WatchEvent::ConfigModified(_) | WatchEvent::ConfigCreated(_) => {
                    Duration::from_millis(150)
                }
                _ => Duration::from_millis(50),
            };

            tx.send(DebounceEvent::new(ev, debounce_duration))?;
        }

        Ok(())
    }

    /// 防抖处理器
    async fn debounce_handler(
        mut rx: mpsc::UnboundedReceiver<DebounceEvent>,
        event_tx: mpsc::UnboundedSender<WatchEvent>,
        atomic_replace_debounce: Duration,
    ) {
        let mut pending_events: Vec<DebounceEvent> = Vec::new();

        loop {
            tokio::select! {
                // 接收新事件
                result = rx.recv() => {
                    match result {
                        Some(debounce_event) => {
                            pending_events.push(debounce_event);
                        }
                        None => break, // 通道关闭
                    }
                }

                // 处理到期事件
                _ = Self::process_pending_events(&mut pending_events, &event_tx, atomic_replace_debounce) => {}
            }
        }
    }

    /// 处理待处理事件
    async fn process_pending_events(
        pending_events: &mut Vec<DebounceEvent>,
        event_tx: &mpsc::UnboundedSender<WatchEvent>,
        atomic_replace_debounce: Duration,
    ) {
        let now = std::time::Instant::now();
        let mut to_send = Vec::new();

        pending_events.retain(|de| {
            if now.duration_since(de.timestamp) >= de.debounce_duration {
                to_send.push(de.event.clone());
                false
            } else {
                true
            }
        });

        // 发送去重后的事件
        let mut seen = HashSet::new();
        for event in to_send {
            let event_str = format!("{:?}", event);
            if !seen.contains(&event_str) && event_tx.send(event).is_ok() {
                seen.insert(event_str);
            }
        }
    }
}

/// 防抖事件
struct DebounceEvent {
    event: WatchEvent,
    debounce_duration: Duration,
    timestamp: std::time::Instant,
}

impl DebounceEvent {
    fn new(event: WatchEvent, debounce_duration: Duration) -> Self {
        Self {
            event,
            debounce_duration,
            timestamp: std::time::Instant::now(),
        }
    }
}

/// 事件去重器
struct EventDeduplicator {
    recent_events: Vec<(EventKind, PathBuf, std::time::Instant)>,
}

impl EventDeduplicator {
    fn new() -> Self {
        Self {
            recent_events: Vec::new(),
        }
    }

    /// 检查是否应该忽略事件
    fn should_ignore(&mut self, event: &Event) -> bool {
        let now = std::time::Instant::now();
        let path = event.paths.first();
        let kind = event.kind;

        // 清理旧事件（超过 1 秒）
        self.recent_events
            .retain(|(_, _, time)| now.duration_since(*time) < Duration::from_secs(1));

        // 检查是否重复
        if let Some(path) = path {
            for (stored_kind, stored_path, _) in &self.recent_events {
                if *stored_kind == kind && stored_path == path {
                    return true;
                }
            }

            // 记录新事件
            self.recent_events.push((kind, path.clone(), now));
            false
        } else {
            false
        }
    }
}

/// 监控器错误
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("Watcher already running")]
    AlreadyRunning,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Notify error: {0}")]
    NotifyError(#[from] notify::Error),

    #[error("Channel send error: {0}")]
    ChannelSendError(#[from] tokio::sync::mpsc::error::SendError<DebounceEvent>),

    #[error("Invalid path")]
    InvalidPath,

    #[error("Watcher stopped")]
    Stopped,
}

/// 监控器构建器
pub struct ConfigWatcherBuilder {
    paths: Vec<PathBuf>,
    debounce_duration: Duration,
    atomic_replace_debounce: Duration,
}

impl ConfigWatcherBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            debounce_duration: Duration::from_millis(150),
            atomic_replace_debounce: Duration::from_millis(50),
        }
    }

    /// 添加监控路径
    pub fn add_path(mut self, path: PathBuf) -> Self {
        self.paths.push(path);
        self
    }

    /// 设置防抖时间
    pub fn debounce(mut self, duration: Duration) -> Self {
        self.debounce_duration = duration;
        self
    }

    /// 设置原子替换防抖时间
    pub fn atomic_debounce(mut self, duration: Duration) -> Self {
        self.atomic_replace_debounce = duration;
        self
    }

    /// 构建监控器
    pub fn build(self) -> ConfigWatcher {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let event_tx_clone = event_tx.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                if let Some(watch_event) = ConfigWatcher::event_to_watch_event(&event) {
                    let _ = event_tx_clone.send(watch_event);
                }
            }
        })
        .expect("Failed to create watcher");

        for path in &self.paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .unwrap_or_else(|_| panic!("Failed to watch path: {:?}", path));
        }

        ConfigWatcher {
            _watcher: watcher,
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            running: Arc::new(RwLock::new(false)),
            paths: self.paths,
            debounce_duration: self.debounce_duration,
            atomic_replace_debounce: self.atomic_replace_debounce,
        }
    }
}

impl Default for ConfigWatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let paths = vec![temp_dir.path().to_path_buf()];

        let watcher = ConfigWatcher::new(paths);
        assert!(!*watcher.running.read().await);
    }

    #[tokio::test]
    async fn test_config_watcher_start_stop() {
        let temp_dir = TempDir::new().unwrap();
        let paths = vec![temp_dir.path().to_path_buf()];

        let watcher = ConfigWatcher::new(paths);

        watcher.start().await.unwrap();
        assert!(*watcher.running.read().await);

        watcher.stop().await;
        assert!(!*watcher.running.read().await);
    }

    #[test]
    fn test_config_watcher_builder() {
        let temp_dir = TempDir::new().unwrap();

        let watcher = ConfigWatcherBuilder::new()
            .add_path(temp_dir.path().to_path_buf())
            .debounce(Duration::from_millis(200))
            .atomic_debounce(Duration::from_millis(100))
            .build();

        assert_eq!(watcher.paths.len(), 1);
        assert_eq!(watcher.debounce_duration, Duration::from_millis(200));
        assert_eq!(watcher.atomic_replace_debounce, Duration::from_millis(100));
    }

    #[test]
    fn test_event_deduplicator() {
        let mut deduplicator = EventDeduplicator::new();
        let path = PathBuf::from("/test/file.yaml");

        // 创建测试事件
        let event = Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
            paths: vec![path.clone()],
            attrs: notify::event::EventAttributes::new(),
        };

        // 第一次不应该被去重
        assert!(!deduplicator.should_ignore(&event));

        // 立即再次应该被去重
        assert!(deduplicator.should_ignore(&event));

        // 等待一段时间后不应该被去重
        std::thread::sleep(Duration::from_secs(2));
        assert!(!deduplicator.should_ignore(&event));
    }
}
