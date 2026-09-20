use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Server Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

/// TLS Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            tls: None,
        }
    }
}

/// Log Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingConfig {
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    #[serde(default = "default_log_max_size_mb")]
    pub log_max_size_mb: u64,
    #[serde(default = "default_error_log_max_files")]
    pub error_log_max_files: usize,
    #[serde(default)]
    pub to_stdout: bool,
}

fn default_log_dir() -> String {
    "./logs".to_string()
}

fn default_log_max_size_mb() -> u64 {
    100
}

fn default_error_log_max_files() -> usize {
    10
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            log_max_size_mb: default_log_max_size_mb(),
            error_log_max_files: default_error_log_max_files(),
            to_stdout: true,
        }
    }
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_selection_strategy")]
    pub selection_strategy: String,
    #[serde(default)]
    pub default_models: Vec<String>,
    #[serde(default)]
    pub credentials: Vec<Credential>,
}

fn default_selection_strategy() -> String {
    "priority".to_string()
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            selection_strategy: default_selection_strategy(),
            default_models: Vec::new(),
            credentials: Vec::new(),
        }
    }
}

/// Credential configuration for a provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Credential {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub model_prefix: String,
    #[serde(default)]
    pub excluded_models: Vec<String>,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub disable_cooling: bool,
    #[serde(default)]
    pub model_aliases: Vec<String>,
}

impl Default for Credential {
    fn default() -> Self {
        Self {
            id: String::new(),
            api_key: None,
            priority: 0,
            base_url: None,
            model_prefix: String::new(),
            excluded_models: Vec::new(),
            headers: std::collections::HashMap::new(),
            disable_cooling: false,
            model_aliases: Vec::new(),
        }
    }
}

/// Routing Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingConfig {
    #[serde(default = "default_routing_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub session_affinity: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
}

fn default_routing_strategy() -> String {
    "round_robin".to_string()
}

fn default_max_retries() -> usize {
    3
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            strategy: default_routing_strategy(),
            session_affinity: false,
            max_retries: default_max_retries(),
        }
    }
}

/// Polling Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PollingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_refresh_before_expiry")]
    pub refresh_before_expiry_secs: u64,
    #[serde(default = "default_max_refresh_retries")]
    pub max_refresh_retries: usize,
}

fn default_refresh_before_expiry() -> u64 {
    300
}

fn default_max_refresh_retries() -> usize {
    3
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_before_expiry_secs: default_refresh_before_expiry(),
            max_refresh_retries: default_max_refresh_retries(),
        }
    }
}

/// Thinking configuration level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Auto,
    None,
}

impl ThinkingLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "minimal" => ThinkingLevel::Minimal,
            "low" => ThinkingLevel::Low,
            "medium" => ThinkingLevel::Medium,
            "high" => ThinkingLevel::High,
            "xhigh" | "extra-high" => ThinkingLevel::XHigh,
            "auto" => ThinkingLevel::Auto,
            "none" => ThinkingLevel::None,
            _ => ThinkingLevel::None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::XHigh => "xhigh",
            ThinkingLevel::Auto => "auto",
            ThinkingLevel::None => "none",
        }
    }

    pub fn to_budget(&self) -> Option<u32> {
        match self {
            ThinkingLevel::Minimal => Some(2048),
            ThinkingLevel::Low => Some(4096),
            ThinkingLevel::Medium => Some(8192),
            ThinkingLevel::High => Some(16384),
            ThinkingLevel::XHigh => Some(32768),
            ThinkingLevel::Auto => None,
            ThinkingLevel::None => Some(0),
        }
    }

    pub fn from_budget(budget: u32) -> Self {
        match budget {
            0 => ThinkingLevel::None,
            n if n <= 2048 => ThinkingLevel::Minimal,
            n if n <= 4096 => ThinkingLevel::Low,
            n if n <= 8192 => ThinkingLevel::Medium,
            n if n <= 16384 => ThinkingLevel::High,
            _ => ThinkingLevel::XHigh,
        }
    }
}

/// Thinking Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub budget_tokens: Option<u32>,
    #[serde(default)]
    pub level: Option<ThinkingLevel>,
    #[serde(default)]
    pub effort: Option<String>,
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            budget_tokens: None,
            level: None,
            effort: None,
        }
    }
}

impl ThinkingConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_budget(mut self, budget: u32) -> Self {
        self.budget_tokens = Some(budget);
        self.level = Some(ThinkingLevel::from_budget(budget));
        self
    }

    pub fn with_level(mut self, level: ThinkingLevel) -> Self {
        self.level = Some(level);
        self.budget_tokens = level.to_budget();
        self
    }

    pub fn with_effort(mut self, effort: String) -> Self {
        self.effort = Some(effort);
        self
    }

    pub fn merge(mut self, other: ThinkingConfig) -> Self {
        if other.budget_tokens.is_some() {
            self.budget_tokens = other.budget_tokens;
        }
        if other.level.is_some() {
            self.level = other.level;
        }
        if other.effort.is_some() {
            self.effort = other.effort;
        }
        self
    }
}

/// Watcher Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatcherConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_watcher_debounce")]
    pub debounce_ms: u64,
    #[serde(default = "default_atomic_replace_debounce")]
    pub atomic_replace_debounce_ms: u64,
}

fn default_watcher_debounce() -> u64 {
    150
}

fn default_atomic_replace_debounce() -> u64 {
    5000
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            paths: vec!["config.yaml".to_string()],
            debounce_ms: default_watcher_debounce(),
            atomic_replace_debounce_ms: default_atomic_replace_debounce(),
        }
    }
}

/// Cache Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheConfig {
    #[serde(default = "default_cache_capacity")]
    pub capacity: usize,
    #[serde(default)]
    pub ttl_secs: Option<u64>,
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
}

fn default_cache_capacity() -> usize {
    1000
}

fn default_cache_enabled() -> bool {
    true
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            capacity: default_cache_capacity(),
            ttl_secs: Some(3600),
            enabled: default_cache_enabled(),
        }
    }
}

/// Registry Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegistryConfig {
    #[serde(default)]
    pub remote_update_enabled: bool,
    #[serde(default)]
    pub remote_update_url: Option<String>,
    #[serde(default)]
    pub remote_update_interval_secs: Option<u64>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            remote_update_enabled: false,
            remote_update_url: None,
            remote_update_interval_secs: Some(3600),
        }
    }
}

/// main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub thinking: ThinkingConfig,
    #[serde(default)]
    pub watcher: WatcherConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub registry: RegistryConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            providers: HashMap::new(),
            routing: RoutingConfig::default(),
            polling: PollingConfig::default(),
            thinking: ThinkingConfig::default(),
            watcher: WatcherConfig::default(),
            cache: CacheConfig::default(),
            registry: RegistryConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file(&self, path: &str) -> Result<(), ConfigError> {
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Verify Configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.port == 0 {
            return Err(ConfigError::InvalidPort(self.server.port));
        }

        if self.logging.log_max_size_mb == 0 {
            return Err(ConfigError::InvalidConfig(
                "log_max_size_mb must be > 0".to_string(),
            ));
        }

        if self.routing.max_retries == 0 {
            return Err(ConfigError::InvalidConfig(
                "max_retries must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// misconfiguration
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] serde_yaml::Error),

    #[error("Invalid port: {0}")]
    InvalidPort(u16),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}
