use lru::LruCache as LruCacheInner;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// LRU Cache Entry
struct CacheEntry<V> {
    value: V,
    timestamp: Instant,
    access_count: u64,
    last_access: Instant,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        let now = Instant::now();
        Self {
            value,
            timestamp: now,
            access_count: 0,
            last_access: now,
        }
    }

    fn accessed(&mut self) {
        self.access_count += 1;
        self.last_access = Instant::now();
    }
}

/// LRU Cache Configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of cache entries
    pub capacity: usize,
    /// Cache expiration time
    pub ttl: Option<Duration>,
    /// Whether to enable caching
    pub enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            capacity: 1000,
            ttl: Some(Duration::from_secs(3600)), // 1 hour
            enabled: true,
        }
    }
}

impl CacheConfig {
    /// Creating a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Setting the maximum number of entries
    pub fn max_entries(mut self, max: usize) -> Self {
        self.capacity = max;
        self
    }

    /// Setting the TTL
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Disable Cache
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// never expire
    pub fn never_expire(mut self) -> Self {
        self.ttl = None;
        self
    }

    /// Create CacheConfig from Config::CacheConfig
    pub fn from_config(config: &crate::config::types::CacheConfig) -> Self {
        Self {
            capacity: config.capacity,
            ttl: config.ttl_secs.map(Duration::from_secs),
            enabled: config.enabled,
        }
    }
}

/// LRU cache
pub struct LruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
{
    cache: Arc<RwLock<LruCacheInner<K, CacheEntry<V>>>>,
    stats: Arc<RwLock<CacheStats>>,
    config: CacheConfig,
}

impl<K, V> LruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    V: Clone,
{
    /// Creating a New LRU Cache
    pub fn new(config: CacheConfig) -> Self {
        let capacity =
            NonZeroUsize::new(config.capacity.max(1)).unwrap_or(NonZeroUsize::new(1).unwrap());
        Self {
            cache: Arc::new(RwLock::new(LruCacheInner::new(capacity))),
            stats: Arc::new(RwLock::new(CacheStats::default())),
            config,
        }
    }

    /// Creating a New LRU Cache from Config::CacheConfig
    pub fn from_config(config: &crate::config::types::CacheConfig) -> Self {
        Self::new(CacheConfig::from_config(config))
    }

    /// Use the default configuration to create
    pub fn with_defaults() -> Self {
        Self::new(CacheConfig::default())
    }

    /// Getting Cached Values
    pub async fn get(&self, key: &K) -> Option<V> {
        if !self.config.enabled {
            return None;
        }

        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        stats.total_access += 1;

        if let Some(entry) = cache.get_mut(key) {
            // Check for expiration dates
            if let Some(ttl) = self.config.ttl {
                if entry.timestamp.elapsed() > ttl {
                    debug!("Cache entry expired: {:?}", key);
                    cache.pop(key);
                    stats.misses += 1;
                    return None;
                }
            }

            // Updating of access logs
            entry.accessed();
            stats.hits += 1;

            debug!("Cache hit: {:?}", key);
            Some(entry.value.clone())
        } else {
            debug!("Cache miss: {:?}", key);
            stats.misses += 1;
            None
        }
    }

    /// Setting the Cache Value
    pub async fn set(&self, key: K, value: V) {
        if !self.config.enabled {
            return;
        }

        let mut cache = self.cache.write().await;

        cache.put(key.clone(), CacheEntry::new(value));

        debug!("Cache set: {:?}", key);
    }

    /// Deleting Cached Values
    pub async fn remove(&self, key: &K) {
        let mut cache = self.cache.write().await;
        cache.pop(key);

        debug!("Cache removed: {:?}", key);
    }

    /// Check if the cache exists
    pub async fn contains(&self, key: &K) -> bool {
        if !self.config.enabled {
            return false;
        }

        let cache = self.cache.read().await;
        cache.contains(key)
    }

    /// Empty the cache
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();

        info!("Cache cleared");
    }

    /// Getting the cache size
    pub async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Getting Cache Statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let stats = self.stats.read().await;

        let total_access = stats.hits + stats.misses;
        let hit_ratio = if total_access > 0 {
            stats.hits as f64 / total_access as f64
        } else {
            0.0
        };

        CacheStats {
            size: cache.len(),
            max_size: self.config.capacity,
            total_access,
            hits: stats.hits,
            misses: stats.misses,
            hit_ratio,
            oldest_entry: None,
            newest_entry: None,
        }
    }

    /// Get all keys
    pub async fn keys(&self) -> Vec<K> {
        let cache = self.cache.read().await;
        cache.iter().map(|(k, _)| k.clone()).collect()
    }
}

impl<K, V> Default for LruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + std::fmt::Debug,
    V: Clone,
{
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Cache Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// current size
    pub size: usize,
    /// maximum size
    pub max_size: usize,
    /// Total number of visits
    pub total_access: u64,
    /// Number of hits
    pub hits: u64,
    /// Number of misses
    pub misses: u64,
    /// Hit rate (0.0 - 1.0)
    pub hit_ratio: f64,
    /// Oldest entry key
    pub oldest_entry: Option<String>,
    /// Latest entry key
    pub newest_entry: Option<String>,
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            size: 0,
            max_size: 0,
            total_access: 0,
            hits: 0,
            misses: 0,
            hit_ratio: 0.0,
            oldest_entry: None,
            newest_entry: None,
        }
    }
}

/// Signature cache entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureCacheEntry {
    /// sign (one's name with a pen etc)
    pub signature: String,
    /// request a hash (computing)
    pub request_hash: String,
    /// response hash (computing)
    pub response_hash: Option<String>,
    /// Creating timestamps
    pub created_at: String,
    /// expiration date
    pub expires_at: Option<String>,
}

/// signature cache
pub type SignatureCache = LruCache<String, SignatureCacheEntry>;

impl SignatureCache {
    /// Creating a new signature cache
    pub fn new_signature_cache(config: CacheConfig) -> Self {
        LruCache::new(config)
    }

    /// Use the default configuration to create
    pub fn with_default_config() -> Self {
        Self::new_signature_cache(CacheConfig::default())
    }

    /// Cache Signature
    pub async fn cache_signature(
        &self,
        key: String,
        signature: String,
        request_hash: String,
        response_hash: Option<String>,
        ttl: Option<Duration>,
    ) {
        let now = chrono::Utc::now();
        let expires_at = ttl.map(|d| (now + d).to_rfc3339());

        let entry = SignatureCacheEntry {
            signature,
            request_hash,
            response_hash,
            created_at: now.to_rfc3339(),
            expires_at,
        };

        self.set(key, entry).await;
    }

    /// Getting a cached signature
    pub async fn get_signature(&self, key: &str) -> Option<SignatureCacheEntry> {
        self.get(&key.to_string()).await
    }

    /// Verify Signature
    pub async fn verify_signature(&self, key: &str, signature: &str, request_hash: &str) -> bool {
        if let Some(entry) = self.get(&key.to_string()).await {
            entry.signature == signature && entry.request_hash == request_hash
        } else {
            false
        }
    }
}

/// Cache Manager
pub struct CacheManager {
    signature_cache: SignatureCache,
    bypass_cache: Arc<RwLock<bool>>,
}

impl CacheManager {
    /// Creating a new cache manager
    pub fn new(signature_config: CacheConfig) -> Self {
        Self {
            signature_cache: SignatureCache::new_signature_cache(signature_config),
            bypass_cache: Arc::new(RwLock::new(false)),
        }
    }

    /// Get Signature Cache
    pub fn signature_cache(&self) -> &SignatureCache {
        &self.signature_cache
    }

    /// Enable bypass mode
    pub async fn enable_bypass(&self) {
        let mut bypass = self.bypass_cache.write().await;
        *bypass = true;
        info!("Cache bypass enabled");
    }

    /// Disable bypass mode
    pub async fn disable_bypass(&self) {
        let mut bypass = self.bypass_cache.write().await;
        *bypass = false;
        info!("Cache bypass disabled");
    }

    /// Checking for cache bypass
    pub async fn is_bypassed(&self) -> bool {
        *self.bypass_cache.read().await
    }

    /// Get all cache statistics
    pub async fn all_stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            signature: self.signature_cache.stats().await,
            bypass_enabled: *self.bypass_cache.read().await,
        }
    }

    /// Clear all caches
    pub async fn clear_all(&self) {
        self.signature_cache.clear().await;
        info!("All caches cleared");
    }
}

/// Cache Manager Statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManagerStats {
    /// Signature cache statistics
    pub signature: CacheStats,
    /// Whether to enable bypass
    pub bypass_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lru_cache_basic() {
        let cache = LruCache::<String, String>::with_defaults();

        cache.set("key1".to_string(), "value1".to_string()).await;
        let value = cache.get(&"key1".to_string()).await;

        assert_eq!(value, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_lru_cache_miss() {
        let cache = LruCache::<String, String>::with_defaults();

        let value = cache.get(&"nonexistent".to_string()).await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_lru_cache_remove() {
        let cache = LruCache::<String, String>::with_defaults();

        cache.set("key1".to_string(), "value1".to_string()).await;
        assert!(cache.contains(&"key1".to_string()).await);

        cache.remove(&"key1".to_string()).await;
        assert!(!cache.contains(&"key1".to_string()).await);
    }

    #[tokio::test]
    async fn test_lru_cache_clear() {
        let cache = LruCache::<String, String>::with_defaults();

        cache.set("key1".to_string(), "value1".to_string()).await;
        cache.set("key2".to_string(), "value2".to_string()).await;

        assert_eq!(cache.size().await, 2);

        cache.clear().await;

        assert_eq!(cache.size().await, 0);
    }

    #[tokio::test]
    async fn test_lru_cache_max_entries() {
        let config = CacheConfig::new().max_entries(2);
        let cache = LruCache::<String, String>::new(config);

        cache.set("key1".to_string(), "value1".to_string()).await;
        cache.set("key2".to_string(), "value2".to_string()).await;
        cache.set("key3".to_string(), "value3".to_string()).await;

        // The oldest entries should be eliminated
        assert!(!cache.contains(&"key1".to_string()).await);
        assert!(cache.contains(&"key2".to_string()).await);
        assert!(cache.contains(&"key3".to_string()).await);
    }

    #[tokio::test]
    async fn test_lru_cache_disabled() {
        let config = CacheConfig::new().disabled();
        let cache = LruCache::<String, String>::new(config);

        cache.set("key1".to_string(), "value1".to_string()).await;

        // When caching is disabled, get should return None.
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }

    #[tokio::test]
    async fn test_lru_cache_stats() {
        let cache = LruCache::<String, String>::with_defaults();

        cache.set("key1".to_string(), "value1".to_string()).await;
        cache.set("key2".to_string(), "value2".to_string()).await;

        let stats = cache.stats().await;
        assert_eq!(stats.size, 2);
        assert_eq!(stats.max_size, 1000);
    }

    #[tokio::test]
    async fn test_signature_cache() {
        let cache = SignatureCache::with_default_config();

        cache
            .cache_signature(
                "test-key".to_string(),
                "test-signature".to_string(),
                "test-request-hash".to_string(),
                Some("test-response-hash".to_string()),
                None,
            )
            .await;

        let entry = cache.get_signature("test-key").await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().signature, "test-signature");
    }

    #[tokio::test]
    async fn test_signature_verify() {
        let cache = SignatureCache::with_default_config();

        cache
            .cache_signature(
                "test-key".to_string(),
                "test-signature".to_string(),
                "test-request-hash".to_string(),
                None,
                None,
            )
            .await;

        assert!(
            cache
                .verify_signature("test-key", "test-signature", "test-request-hash")
                .await
        );

        assert!(
            !cache
                .verify_signature("test-key", "wrong-signature", "test-request-hash")
                .await
        );
    }

    #[tokio::test]
    async fn test_cache_manager() {
        let manager = CacheManager::new(CacheConfig::default());

        assert!(!manager.is_bypassed().await);

        manager.enable_bypass().await;
        assert!(manager.is_bypassed().await);

        manager.disable_bypass().await;
        assert!(!manager.is_bypassed().await);

        let stats = manager.all_stats().await;
        assert!(!stats.bypass_enabled);
    }

    #[tokio::test]
    async fn test_cache_manager_clear() {
        let manager = CacheManager::new(CacheConfig::default());

        manager
            .signature_cache()
            .set(
                "key1".to_string(),
                SignatureCacheEntry {
                    signature: "sig1".to_string(),
                    request_hash: "hash1".to_string(),
                    response_hash: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    expires_at: None,
                },
            )
            .await;

        assert!(manager.signature_cache().size().await > 0);

        manager.clear_all().await;

        assert_eq!(manager.signature_cache().size().await, 0);
    }
}
