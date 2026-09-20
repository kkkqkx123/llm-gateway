use crate::scheduler::heap::{HeapItem, RefreshHeap};
use crate::scheduler::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};
use tokio::time::sleep;

impl RefreshScheduler {
    pub async fn run(
        &self,
        shutdown_rx: mpsc::Receiver<()>,
        jobs_tx: broadcast::Sender<String>,
    ) -> Result<(), SchedulerError> {
        let manager = self
            .manager
            .as_ref()
            .ok_or(SchedulerError::ManagerNotInitialized)?
            .clone();

        let workers = self.concurrency;
        let (wake_tx, wake_rx) = broadcast::channel(1);

        for _ in 0..workers {
            let manager = manager.clone();
            let mut jobs_rx = jobs_tx.subscribe();
            let mut wake_rx = wake_tx.subscribe();

            tokio::spawn(async move {
                let handle = tokio::runtime::Handle::current();
                loop {
                    tokio::select! {
                        result = jobs_rx.recv() => {
                            match result {
                                Ok(auth_id) if !auth_id.is_empty() => {
                                    if let Err(e) = manager.refresh_auth(&handle, &auth_id).await {
                                        log::warn!("Failed to refresh auth {}: {:?}", auth_id, e);
                                    }
                                }
                                _ => break,
                            }
                        }
                        _ = wake_rx.recv() => {
                            continue;
                        }
                    }
                }
            });
        }

        self.schedule_loop(shutdown_rx, wake_rx, jobs_tx).await
    }

    pub async fn rebuild(&self) -> Result<(), SchedulerError> {
        let manager = self
            .manager
            .as_ref()
            .ok_or(SchedulerError::ManagerNotInitialized)?
            .clone();

        let now = Instant::now();
        let mut new_heap = RefreshHeap::new();
        let mut new_index = HashMap::new();

        let heap = self.heap.lock().await;
        for item in heap.iter() {
            let refresh_time = manager.get_auth_refresh_time(&item.id).await?;
            if let Some(next) = refresh_time {
                let new_item = HeapItem::new(item.id.clone(), next);
                new_index.insert(item.id.clone(), new_heap.len());
                new_heap.push(new_item);
            }
        }
        drop(heap);

        let mut heap_guard = self.heap.lock().await;
        *heap_guard = new_heap;
        drop(heap_guard);

        let mut index_guard = self.index.write().await;
        *index_guard = new_index;
        drop(index_guard);

        Ok(())
    }

    async fn schedule_loop(
        &self,
        mut shutdown_rx: mpsc::Receiver<()>,
        mut wake_rx: broadcast::Receiver<()>,
        jobs_tx: broadcast::Sender<String>,
    ) -> Result<(), SchedulerError> {
        loop {
            let sleep_duration = self.calculate_sleep_duration(Instant::now()).await;

            tokio::select! {
                _ = shutdown_rx.recv() => {
                    log::info!("Scheduler received shutdown signal");
                    break;
                }
                _ = wake_rx.recv() => {
                    let now = Instant::now();
                    self.apply_dirty(now).await;
                }
                _ = sleep(sleep_duration) => {
                    let now = Instant::now();
                    self.handle_due(now, jobs_tx.clone()).await;
                    self.apply_dirty(now).await;
                }
            }
        }

        Ok(())
    }

    async fn calculate_sleep_duration(&self, now: Instant) -> Duration {
        let next = match self.peek_next().await {
            Some(t) => t,
            None => return Duration::from_secs(3600),
        };

        let wait = if next > now {
            next - now
        } else {
            Duration::ZERO
        };

        wait
    }

    async fn peek_next(&self) -> Option<Instant> {
        let heap = self.heap.lock().await;
        heap.peek().map(|item| item.next)
    }

    async fn handle_due(&self, now: Instant, jobs_tx: broadcast::Sender<String>) {
        let due = self.pop_due(now).await;
        if due.is_empty() {
            return;
        }

        log::debug!("Auto-refresh scheduler due auths: {}", due.len());

        for auth_id in due {
            self.handle_due_auth(now, auth_id, jobs_tx.clone()).await;
        }
    }

    async fn pop_due(&self, now: Instant) -> Vec<String> {
        let mut heap = self.heap.lock().await;
        let mut index = self.index.write().await;
        let mut due = Vec::new();

        while let Some(item) = heap.peek() {
            if item.next > now {
                break;
            }

            if let Some(popped) = heap.pop() {
                index.remove(&popped.id);
                due.push(popped.id);
            }
        }

        due
    }

    async fn handle_due_auth(
        &self,
        now: Instant,
        auth_id: String,
        jobs_tx: broadcast::Sender<String>,
    ) {
        if auth_id.is_empty() {
            return;
        }

        let manager = match self.manager.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        let should_schedule = match manager.should_schedule(&auth_id).await {
            Ok(s) => s,
            Err(_) => {
                self.remove_auth(&auth_id).await;
                return;
            }
        };

        if !should_schedule {
            self.remove_auth(&auth_id).await;
            return;
        }

        let next = match manager.get_auth_refresh_time(&auth_id).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                self.remove_auth(&auth_id).await;
                return;
            }
            Err(_) => {
                self.remove_auth(&auth_id).await;
                return;
            }
        };

        let should_refresh = match self.should_refresh_auth(&manager, &auth_id, now).await {
            Ok(s) => s,
            Err(_) => {
                self.upsert_auth(auth_id.clone(), now + self.interval).await;
                return;
            }
        };

        if !should_refresh {
            self.upsert_auth(auth_id.clone(), next).await;
            return;
        }

        let pending = match manager.mark_refresh_pending(&auth_id).await {
            Ok(p) => p,
            Err(_) => {
                let next = match manager.get_auth_refresh_time(&auth_id).await {
                    Ok(Some(t)) => t,
                    _ => return,
                };
                let should_schedule = match manager.should_schedule(&auth_id).await {
                    Ok(s) => s,
                    _ => false,
                };
                if should_schedule {
                    self.upsert_auth(auth_id.clone(), next).await;
                } else {
                    self.remove_auth(&auth_id).await;
                }
                return;
            }
        };

        if !pending {
            let next = match manager.get_auth_refresh_time(&auth_id).await {
                Ok(Some(t)) => t,
                _ => return,
            };
            let should_schedule = match manager.should_schedule(&auth_id).await {
                Ok(s) => s,
                _ => false,
            };
            if should_schedule {
                self.upsert_auth(auth_id.clone(), next).await;
            } else {
                self.remove_auth(&auth_id).await;
            }
            return;
        }

        let _ = jobs_tx.send(auth_id);
    }

    async fn apply_dirty(&self, now: Instant) {
        let dirty = self.drain_dirty().await;
        if dirty.is_empty() {
            return;
        }

        let manager = match self.manager.as_ref() {
            Some(m) => m.clone(),
            None => return,
        };

        for auth_id in dirty {
            let next = match manager.get_auth_refresh_time(&auth_id).await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    self.remove_auth(&auth_id).await;
                    continue;
                }
                Err(_) => {
                    self.remove_auth(&auth_id).await;
                    continue;
                }
            };

            self.upsert_auth(auth_id, next).await;
        }
    }

    async fn drain_dirty(&self) -> Vec<String> {
        let mut dirty = self.dirty.lock().await;
        let out: Vec<String> = dirty.keys().cloned().collect();
        dirty.clear();
        out
    }

    async fn upsert_auth(&self, auth_id: String, next: Instant) {
        if auth_id.is_empty() {
            return;
        }

        let mut heap = self.heap.lock().await;
        let mut index = self.index.write().await;

        if let Some(&existing_idx) = index.get(&auth_id) {
            if let Some(item) = heap.items.get_mut(existing_idx) {
                item.next = next;
                heap.fix(existing_idx);
            }
        } else {
            let item = HeapItem::new(auth_id.clone(), next);
            index.insert(auth_id.clone(), heap.len());
            heap.push(item);
        }
    }

    async fn remove_auth(&self, auth_id: &str) {
        if auth_id.is_empty() {
            return;
        }

        let mut heap = self.heap.lock().await;
        let mut index = self.index.write().await;

        if let Some(&idx) = index.get(auth_id) {
            heap.remove(idx);
            index.remove(auth_id);
        }
    }

    async fn should_refresh_auth(
        &self,
        manager: &Arc<dyn AuthManager>,
        auth_id: &str,
        now: Instant,
    ) -> Result<bool, SchedulerError> {
        let next = manager.get_auth_refresh_time(auth_id).await?;
        Ok(match next {
            Some(t) => t <= now,
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    struct TestAuthManager {
        refresh_times: tokio::sync::RwLock<HashMap<String, Instant>>,
        should_schedule_map: tokio::sync::RwLock<HashMap<String, bool>>,
    }

    impl TestAuthManager {
        fn new() -> Self {
            TestAuthManager {
                refresh_times: tokio::sync::RwLock::new(HashMap::new()),
                should_schedule_map: tokio::sync::RwLock::new(HashMap::new()),
            }
        }

        async fn set_refresh_time(&self, auth_id: &str, time: Instant) {
            let mut map = self.refresh_times.write().await;
            map.insert(auth_id.to_string(), time);
        }

        async fn set_should_schedule(&self, auth_id: &str, value: bool) {
            let mut map = self.should_schedule_map.write().await;
            map.insert(auth_id.to_string(), value);
        }
    }

    #[async_trait::async_trait]
    impl AuthManager for TestAuthManager {
        async fn refresh_auth(
            &self,
            _ctx: &tokio::runtime::Handle,
            auth_id: &str,
        ) -> Result<(), SchedulerError> {
            let mut times = self.refresh_times.write().await;
            if let Some(time) = times.get_mut(auth_id) {
                *time = Instant::now() + Duration::from_secs(60);
            }
            Ok(())
        }

        async fn get_auth_refresh_time(
            &self,
            auth_id: &str,
        ) -> Result<Option<Instant>, SchedulerError> {
            let times = self.refresh_times.read().await;
            Ok(times.get(auth_id).copied())
        }

        async fn should_schedule(&self, auth_id: &str) -> Result<bool, SchedulerError> {
            let map = self.should_schedule_map.read().await;
            Ok(map.get(auth_id).copied().unwrap_or(true))
        }

        async fn mark_refresh_pending(&self, _auth_id: &str) -> Result<bool, SchedulerError> {
            Ok(true)
        }
    }

    #[test]
    fn test_calculate_sleep_duration_with_no_items() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
            let duration = scheduler.calculate_sleep_duration(Instant::now()).await;
            assert_eq!(duration, Duration::from_secs(3600));
        });
    }

    #[tokio::test]
    async fn test_peek_next_empty() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        assert!(scheduler.peek_next().await.is_none());
    }

    #[tokio::test]
    async fn test_upsert_auth_new() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler.upsert_auth("auth-1".to_string(), now).await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek().unwrap().id, "auth-1");
    }

    #[tokio::test]
    async fn test_upsert_auth_existing() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler.upsert_auth("auth-1".to_string(), now).await;
        scheduler
            .upsert_auth("auth-1".to_string(), now + Duration::from_secs(100))
            .await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek().unwrap().next, now + Duration::from_secs(100));
    }

    #[tokio::test]
    async fn test_upsert_auth_empty_id() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler.upsert_auth("".to_string(), now).await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 0);
    }

    #[tokio::test]
    async fn test_remove_auth() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler.upsert_auth("auth-1".to_string(), now).await;
        scheduler
            .upsert_auth("auth-2".to_string(), now + Duration::from_secs(10))
            .await;

        scheduler.remove_auth("auth-1").await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek().unwrap().id, "auth-2");
    }

    #[tokio::test]
    async fn test_remove_auth_empty_id() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler.upsert_auth("auth-1".to_string(), now).await;
        scheduler.remove_auth("").await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 1);
    }

    #[tokio::test]
    async fn test_pop_due() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let now = Instant::now();

        scheduler
            .upsert_auth("auth-1".to_string(), now - Duration::from_secs(10))
            .await;
        scheduler
            .upsert_auth("auth-2".to_string(), now + Duration::from_secs(10))
            .await;
        scheduler
            .upsert_auth("auth-3".to_string(), now - Duration::from_secs(5))
            .await;

        let due = scheduler.pop_due(now).await;
        assert_eq!(due.len(), 2);
        assert!(due.contains(&"auth-1".to_string()));
        assert!(due.contains(&"auth-3".to_string()));

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 1);
        assert_eq!(heap.peek().unwrap().id, "auth-2");
    }

    #[tokio::test]
    async fn test_drain_dirty() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);

        scheduler.reschedule("auth-1".to_string()).await;
        scheduler.reschedule("auth-2".to_string()).await;

        let dirty = scheduler.drain_dirty().await;
        assert_eq!(dirty.len(), 2);
        assert!(dirty.contains(&"auth-1".to_string()));
        assert!(dirty.contains(&"auth-2".to_string()));

        let dirty_guard = scheduler.dirty.lock().await;
        assert_eq!(dirty_guard.len(), 0);
    }

    #[tokio::test]
    async fn test_apply_dirty_no_manager() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        scheduler.reschedule("auth-1".to_string()).await;

        scheduler.apply_dirty(Instant::now()).await;

        let heap = scheduler.heap.lock().await;
        assert_eq!(heap.len(), 0);
    }

    #[tokio::test]
    async fn test_run_without_manager() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (jobs_tx, _jobs_rx) = broadcast::channel(10);

        let result = scheduler.run(shutdown_rx, jobs_tx).await;
        assert!(matches!(result, Err(SchedulerError::ManagerNotInitialized)));
    }

    #[tokio::test]
    async fn test_rebuild_without_manager() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let result = scheduler.rebuild().await;
        assert!(matches!(result, Err(SchedulerError::ManagerNotInitialized)));
    }

    #[tokio::test]
    async fn test_handle_due_empty() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let (jobs_tx, _jobs_rx) = broadcast::channel(10);

        scheduler.handle_due(Instant::now(), jobs_tx).await;
    }

    #[tokio::test]
    async fn test_handle_due_auth_empty_id() {
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);
        let (jobs_tx, _jobs_rx) = broadcast::channel(10);

        scheduler
            .handle_due_auth(Instant::now(), "".to_string(), jobs_tx)
            .await;
    }

    #[tokio::test]
    async fn test_should_refresh_auth_with_time() {
        let manager = Arc::new(TestAuthManager::new());
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);

        let now = Instant::now();
        manager
            .set_refresh_time("auth-1", now - Duration::from_secs(10))
            .await;

        let result = scheduler
            .should_refresh_auth(&(manager.clone() as Arc<dyn AuthManager>), "auth-1", now)
            .await;
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_should_refresh_auth_no_time() {
        let manager = Arc::new(TestAuthManager::new());
        let scheduler = RefreshScheduler::new(None, Duration::from_secs(60), 1);

        let now = Instant::now();

        let result = scheduler
            .should_refresh_auth(&(manager.clone() as Arc<dyn AuthManager>), "auth-1", now)
            .await;
        assert!(!result.unwrap());
    }
}
