pub mod app_logger;
pub mod file_logger;
pub mod middleware;
pub mod streaming_logger;
pub mod types;

pub use app_logger::{AppLogger, AppLoggerConfig, LogCleaner, Rotation};
pub use file_logger::*;
pub use middleware::*;
pub use streaming_logger::*;
pub use types::*;

use crate::config::types::LoggingConfig;
use crate::logging::types::{LogEntry, LoggerError, TransportType};
use chrono::Utc;
use std::sync::Arc;
use tracing_subscriber;

/// Initialize logging with default settings
pub fn initialize_logging() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    Ok(())
}

/// Log Manager
pub struct LogManager {
    request_logger: Option<Arc<dyn RequestLogger>>,
    log_dir: String,
    enabled: bool,
}

impl LogManager {
    /// Creating a new log manager
    pub fn new(log_dir: &str, enabled: bool) -> Self {
        let request_logger = if enabled {
            Some(Arc::new(FileRequestLogger::new(log_dir).unwrap()) as Arc<dyn RequestLogger>)
        } else {
            None
        };

        Self {
            request_logger,
            log_dir: log_dir.to_string(),
            enabled,
        }
    }

    /// Creating a log manager from configuration
    pub fn from_config(config: &LoggingConfig) -> Self {
        Self::new(&config.log_dir, true)
    }

    /// Creating a Streaming Log Writer
    pub fn create_streaming_writer(
        &self,
        endpoint: &str,
    ) -> Result<Box<dyn StreamingLogWriter>, LoggerError> {
        if self.enabled {
            Ok(Box::new(FileStreamingLogWriter::new(
                &self.log_dir,
                endpoint,
            )?))
        } else {
            Ok(Box::new(NoOpStreamingLogWriter))
        }
    }

    /// Requests for recording
    pub fn log_request(&self, entry: &LogEntry) -> Result<(), LoggerError> {
        if let Some(logger) = &self.request_logger {
            logger.log_request(entry)
        } else {
            Ok(())
        }
    }

    /// Record Response
    pub fn log_response(&self, entry: &LogEntry) -> Result<(), LoggerError> {
        if let Some(logger) = &self.request_logger {
            logger.log_response(entry)
        } else {
            Ok(())
        }
    }

    /// Check to see if it is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get log directory
    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }
}

impl Default for LogManager {
    fn default() -> Self {
        Self::new("./logs", true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_manager_creation() {
        let manager = LogManager::new("/tmp/test-logs", true);
        assert!(manager.is_enabled());
        assert_eq!(manager.log_dir(), "/tmp/test-logs");
    }

    #[test]
    fn test_log_manager_default() {
        let manager = LogManager::default();
        assert!(manager.is_enabled());
        assert_eq!(manager.log_dir(), "./logs");
    }

    #[test]
    fn test_log_manager_disabled() {
        let manager = LogManager::new("/tmp/test-logs", false);
        assert!(!manager.is_enabled());

        let entry = LogEntry {
            id: "test-id".to_string(),
            timestamp: Utc::now(),
            endpoint: "/api/chat".to_string(),
            method: "POST".to_string(),
            url: "http://example.com/api/chat".to_string(),
            status: Some(200),
            headers: None,
            request_body: None,
            response_body: None,
            duration_ms: Some(100),
            error: None,
            transport_type: TransportType::Http,
        };

        // Should not return an error when disabled
        assert!(manager.log_request(&entry).is_ok());
        assert!(manager.log_response(&entry).is_ok());
    }
}
