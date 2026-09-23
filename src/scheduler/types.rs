use crate::config::types::PollingConfig;
use crate::scheduler::heap::RefreshHeap;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, RwLock};

#[derive(thiserror::Error, Debug)]
pub enum SchedulerError {
    #[error("Authentication manager not initialized")]
    ManagerNotInitialized,

    #[error("Auth not found: {0}")]
    AuthNotFound(String),

    #[error("Invalid auth ID")]
    InvalidAuthID,

    #[error("Scheduler already running")]
    AlreadyRunning,

    #[error("Scheduler not running")]
    NotRunning,

    #[error("Channel error: {0}")]
    ChannelError(#[from] mpsc::error::SendError<String>),
}

#[async_trait::async_trait]
pub trait AuthManager: Send + Sync {
    async fn refresh_auth(
        &self,
        ctx: &tokio::runtime::Handle,
        auth_id: &str,
    ) -> Result<(), SchedulerError>;
    async fn get_auth_refresh_time(&self, auth_id: &str)
        -> Result<Option<Instant>, SchedulerError>;
    async fn should_schedule(&self, auth_id: &str) -> Result<bool, SchedulerError>;
    async fn mark_refresh_pending(&self, auth_id: &str) -> Result<bool, SchedulerError>;
}

/// A do-nothing AuthManager used when the gateway has no real refreshable credentials
/// (e.g. static API keys). Keeps the scheduler thread alive for future extensions.
pub struct NoopAuthManager;

impl NoopAuthManager {
    pub fn new() -> Self {
        NoopAuthManager
    }
}

impl Default for NoopAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AuthManager for NoopAuthManager {
    async fn refresh_auth(
        &self,
        _ctx: &tokio::runtime::Handle,
        _auth_id: &str,
    ) -> Result<(), SchedulerError> {
        Ok(())
    }
    async fn get_auth_refresh_time(
        &self,
        _auth_id: &str,
    ) -> Result<Option<Instant>, SchedulerError> {
        Ok(None)
    }
    async fn should_schedule(&self, _auth_id: &str) -> Result<bool, SchedulerError> {
        Ok(false) // nothing to schedule
    }
    async fn mark_refresh_pending(&self, _auth_id: &str) -> Result<bool, SchedulerError> {
        Ok(false)
    }
}

pub struct RefreshScheduler {
    pub manager: Option<Arc<dyn AuthManager>>,
    pub interval: Duration,
    pub concurrency: usize,

    pub heap: Mutex<RefreshHeap>,
    pub index: RwLock<HashMap<String, usize>>,
    pub dirty: Mutex<HashMap<String, ()>>,
}

impl RefreshScheduler {
    pub fn new(
        manager: Option<Arc<dyn AuthManager>>,
        interval: Duration,
        concurrency: usize,
    ) -> Self {
        let interval = if interval <= Duration::ZERO {
            Duration::from_secs(60)
        } else {
            interval
        };

        let concurrency = if concurrency == 0 { 4 } else { concurrency };

        RefreshScheduler {
            manager,
            interval,
            concurrency,
            heap: Mutex::new(RefreshHeap::new()),
            index: RwLock::new(HashMap::new()),
            dirty: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_config(manager: Option<Arc<dyn AuthManager>>, config: &PollingConfig) -> Self {
        Self::new(
            manager,
            Duration::from_secs(config.refresh_before_expiry_secs),
            config.max_refresh_retries,
        )
    }

    pub async fn reschedule(&self, auth_id: String) {
        if auth_id.is_empty() {
            return;
        }
        let mut dirty = self.dirty.lock().await;
        dirty.insert(auth_id, ());
    }

    pub fn manager(&self) -> Option<&Arc<dyn AuthManager>> {
        self.manager.as_ref()
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }
}

impl Default for RefreshScheduler {
    fn default() -> Self {
        Self::new(None, Duration::ZERO, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockAuthManager;

    #[async_trait::async_trait]
    impl AuthManager for MockAuthManager {
        async fn refresh_auth(
            &self,
            _ctx: &tokio::runtime::Handle,
            _auth_id: &str,
        ) -> Result<(), SchedulerError> {
            Ok(())
        }

        async fn get_auth_refresh_time(
            &self,
            _auth_id: &str,
        ) -> Result<Option<Instant>, SchedulerError> {
            Ok(None)
        }

        async fn should_schedule(&self, _auth_id: &str) -> Result<bool, SchedulerError> {
            Ok(true)
        }

        async fn mark_refresh_pending(&self, _auth_id: &str) -> Result<bool, SchedulerError> {
            Ok(true)
        }
    }

    #[test]
    fn test_scheduler_error_display() {
        let err = SchedulerError::ManagerNotInitialized;
        assert_eq!(err.to_string(), "Authentication manager not initialized");

        let err = SchedulerError::AuthNotFound("test-id".to_string());
        assert_eq!(err.to_string(), "Auth not found: test-id");

        let err = SchedulerError::InvalidAuthID;
        assert_eq!(err.to_string(), "Invalid auth ID");

        let err = SchedulerError::AlreadyRunning;
        assert_eq!(err.to_string(), "Scheduler already running");

        let err = SchedulerError::NotRunning;
        assert_eq!(err.to_string(), "Scheduler not running");
    }

    #[test]
    fn test_refresh_scheduler_creation() {
        let manager = Arc::new(MockAuthManager);
        let scheduler = RefreshScheduler::new(Some(manager), Duration::from_secs(30), 8);

        assert!(scheduler.manager.is_some());
        assert_eq!(scheduler.interval(), Duration::from_secs(30));
        assert_eq!(scheduler.concurrency(), 8);
    }

    #[test]
    fn test_refresh_scheduler_default_values() {
        let scheduler = RefreshScheduler::new(None, Duration::ZERO, 0);

        assert_eq!(scheduler.interval(), Duration::from_secs(60));
        assert_eq!(scheduler.concurrency(), 4);
    }

    #[test]
    fn test_refresh_scheduler_default() {
        let scheduler = RefreshScheduler::default();

        assert_eq!(scheduler.interval(), Duration::from_secs(60));
        assert_eq!(scheduler.concurrency(), 4);
    }

    #[tokio::test]
    async fn test_reschedule() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 4);

        scheduler.reschedule("auth-1".to_string()).await;
        scheduler.reschedule("auth-2".to_string()).await;

        let dirty = scheduler.dirty.lock().await;
        assert_eq!(dirty.len(), 2);
        assert!(dirty.contains_key("auth-1"));
        assert!(dirty.contains_key("auth-2"));
    }

    #[tokio::test]
    async fn test_reschedule_empty_id() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 4);

        scheduler.reschedule("".to_string()).await;

        let dirty = scheduler.dirty.lock().await;
        assert_eq!(dirty.len(), 0);
    }

    #[test]
    fn test_manager_none() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 4);
        assert!(scheduler.manager().is_none());
    }

    #[test]
    fn test_manager_some() {
        let manager: Arc<dyn AuthManager> = Arc::new(MockAuthManager);
        let scheduler = RefreshScheduler::new(Some(manager.clone()), Duration::from_secs(60), 4);

        let returned_manager = scheduler.manager().unwrap();
        assert_eq!(
            returned_manager.as_ref() as *const dyn AuthManager,
            manager.as_ref() as *const dyn AuthManager
        );
    }

    #[tokio::test]
    async fn test_scheduler_error_channel_error() {
        let (tx, _rx) = mpsc::channel(1);
        drop(_rx);

        let err = tx.send("test".to_string()).await.err();
        if let Some(send_err) = err {
            let scheduler_err: SchedulerError = send_err.into();
            assert!(scheduler_err.to_string().contains("Channel error"));
        }
    }
}
