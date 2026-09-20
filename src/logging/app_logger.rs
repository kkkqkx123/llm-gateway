use std::path::PathBuf;
use tracing::{Level, Subscriber};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Application Logging Configuration
#[derive(Debug, Clone)]
pub struct AppLoggerConfig {
    /// Log level
    pub level: Level,
    /// Log Output Directory
    pub log_dir: Option<PathBuf>,
    /// Whether to output to the console
    pub console: bool,
    /// Whether to output to file
    pub file: bool,
    /// Log file name prefix
    pub file_prefix: String,
    /// Log rotation period (daily/hourly/minutely/never)
    pub rotation: Rotation,
    /// Whether to include the source code location
    pub include_source: bool,
    /// Whether or not it contains a target
    pub include_target: bool,
    /// Customized filters
    pub filter: Option<String>,
}

/// Log rotation period
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// lit. go by the sun on a daily basis (idiom); fig. to act according to the order of the day
    Daily,
    /// hourly
    Hourly,
    /// Minute-by-minute rotation
    Minutely,
    /// non-rotation
    Never,
}

impl Default for AppLoggerConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            log_dir: Some(PathBuf::from("./logs")),
            console: true,
            file: false,
            file_prefix: "app".to_string(),
            rotation: Rotation::Daily,
            include_source: false,
            include_target: true,
            filter: None,
        }
    }
}

impl AppLoggerConfig {
    /// Creating a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Setting the log level
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Setting the log directory
    pub fn log_dir(mut self, dir: PathBuf) -> Self {
        self.log_dir = Some(dir);
        self
    }

    /// Disable File Logging
    pub fn no_file(mut self) -> Self {
        self.file = false;
        self
    }

    /// Enabling File Logging
    pub fn with_file(mut self) -> Self {
        self.file = true;
        self
    }

    /// Setting Console Output
    pub fn console(mut self, enabled: bool) -> Self {
        self.console = enabled;
        self
    }

    /// Setting the filename prefix
    pub fn file_prefix(mut self, prefix: String) -> Self {
        self.file_prefix = prefix;
        self
    }

    /// Setting the rotation period
    pub fn rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }

    /// Include source code location
    pub fn with_source(mut self) -> Self {
        self.include_source = true;
        self
    }

    /// Setting up custom filters
    pub fn filter(mut self, filter: String) -> Self {
        self.filter = Some(filter);
        self
    }
}

/// Application Log Initializer
pub struct AppLogger;

impl AppLogger {
    /// Initialize the application logging system
    pub fn init(config: AppLoggerConfig) -> Result<(), LoggerInitError> {
        let mut layers = Vec::new();

        // Creating environmental filters
        let filter = if let Some(custom_filter) = config.filter {
            EnvFilter::try_new(custom_filter).map_err(LoggerInitError::FilterError)?
        } else {
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(config.level.as_str()))
        };

        // Console Output Layer
        if config.console {
            let console_layer = fmt::layer()
                .with_target(config.include_target)
                .with_filter(filter.clone());

            layers.push(console_layer.boxed());
        }

        // file output layer
        if config.file {
            let log_dir = config.log_dir.ok_or(LoggerInitError::NoLogDir)?;

            // Ensure that the log directory exists
            std::fs::create_dir_all(&log_dir).map_err(LoggerInitError::IoError)?;

            let (non_blocking, _guard) = match config.rotation {
                Rotation::Daily => {
                    let appender = rolling::daily(&log_dir, &config.file_prefix);
                    non_blocking(appender)
                }
                Rotation::Hourly => {
                    let appender = rolling::hourly(&log_dir, &config.file_prefix);
                    non_blocking(appender)
                }
                Rotation::Minutely => {
                    let appender = rolling::minutely(&log_dir, &config.file_prefix);
                    non_blocking(appender)
                }
                Rotation::Never => {
                    let file_path = log_dir.join(format!("{}.log", config.file_prefix));
                    tracing_appender::non_blocking(std::fs::File::create(file_path)?)
                }
            };

            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_target(config.include_target)
                .with_ansi(false)
                .with_filter(filter);

            layers.push(file_layer.boxed());
        }

        // Initialize global subscribers
        tracing_subscriber::registry().with(layers).init();

        Ok(())
    }

    /// Initialize with default configuration
    pub fn init_default() -> Result<(), LoggerInitError> {
        Self::init(AppLoggerConfig::default())
    }

    /// Initialization with environment variables
    ///
    /// Supported environment variables:
    /// - `RUST_LOG`: log level (e.g. info, debug, warn, error)
    /// - `APP_LOG_DIR`: log directory (default: . /logs)
    /// - `APP_LOG_FILE`: whether to output to file (default: false)
    /// - `APP_LOG_CONSOLE`: whether to output to console (default: true)
    pub fn init_from_env() -> Result<(), LoggerInitError> {
        let level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.parse::<Level>().ok())
            .unwrap_or(Level::INFO);

        let log_dir = std::env::var("APP_LOG_DIR").ok().map(PathBuf::from);

        let file = std::env::var("APP_LOG_FILE")
            .ok()
            .map(|s| s == "1" || s.to_lowercase() == "true")
            .unwrap_or(false);

        let console = std::env::var("APP_LOG_CONSOLE")
            .ok()
            .map(|s| s == "1" || s.to_lowercase() == "true")
            .unwrap_or(true);

        let config = AppLoggerConfig::new().level(level).console(console);

        let config = if file {
            if let Some(dir) = log_dir {
                config.log_dir(dir).with_file()
            } else {
                config.log_dir(PathBuf::from("./logs")).with_file()
            }
        } else {
            config
        };

        Self::init(config)
    }
}

/// Log initialization error
#[derive(Debug, thiserror::Error)]
pub enum LoggerInitError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Filter error: {0}")]
    FilterError(#[from] tracing_subscriber::filter::ParseError),

    #[error("No log directory specified")]
    NoLogDir,

    #[error("Invalid log configuration")]
    InvalidConfig,
}

/// Log Cleaner
pub struct LogCleaner {
    log_dir: PathBuf,
    max_size_bytes: Option<usize>,
    max_files: Option<usize>,
}

impl LogCleaner {
    /// Creating a new log cleaner
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            max_size_bytes: None,
            max_files: None,
        }
    }

    /// Setting the maximum total size (in bytes)
    pub fn max_size(mut self, size: usize) -> Self {
        self.max_size_bytes = Some(size);
        self
    }

    /// Setting the maximum number of files
    pub fn max_files(mut self, count: usize) -> Self {
        self.max_files = Some(count);
        self
    }

    /// Cleaning up old logs
    pub fn clean(&self) -> Result<usize, LogCleanError> {
        if !self.log_dir.exists() {
            return Ok(0);
        }

        let mut log_files = self.collect_log_files()?;

        // Sort by filename (assuming filename contains a timestamp)
        log_files.sort();

        let mut removed_count = 0;

        // Cleaning up files that exceed the maximum number of files
        if let Some(max_files) = self.max_files {
            if log_files.len() > max_files {
                let files_to_remove = log_files.len() - max_files;
                for file in log_files.drain(..files_to_remove) {
                    self.remove_file(&file)?;
                    removed_count += 1;
                }
            }
        }

        // Clean up files that exceed the maximum size
        if let Some(max_size) = self.max_size_bytes {
            let mut total_size = self.calculate_total_size(&log_files)?;

            while total_size > max_size && !log_files.is_empty() {
                let file = log_files.remove(0); // Delete the oldest files
                let file_size = self.file_size(&file)?;
                total_size -= file_size;
                self.remove_file(&file)?;
                removed_count += 1;
            }
        }

        Ok(removed_count)
    }

    fn collect_log_files(&self) -> Result<Vec<PathBuf>, LogCleanError> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "log" {
                        files.push(path);
                    }
                }
            }
        }

        Ok(files)
    }

    fn remove_file(&self, path: &PathBuf) -> Result<(), LogCleanError> {
        std::fs::remove_file(path)?;
        tracing::info!("Removed log file: {:?}", path);
        Ok(())
    }

    fn calculate_total_size(&self, files: &[PathBuf]) -> Result<usize, LogCleanError> {
        let mut total = 0;
        for file in files {
            total += self.file_size(file)?;
        }
        Ok(total)
    }

    fn file_size(&self, path: &PathBuf) -> Result<usize, LogCleanError> {
        Ok(std::fs::metadata(path)?.len() as usize)
    }
}

/// Log cleanup errors
#[derive(Debug, thiserror::Error)]
pub enum LogCleanError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid log file")]
    InvalidLogFile,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_app_logger_config_default() {
        let config = AppLoggerConfig::default();
        assert_eq!(config.level, Level::INFO);
        assert_eq!(config.console, true);
        assert_eq!(config.file, false);
        assert_eq!(config.file_prefix, "app");
        assert_eq!(config.rotation, Rotation::Daily);
    }

    #[test]
    fn test_app_logger_config_builder() {
        let config = AppLoggerConfig::new()
            .level(Level::DEBUG)
            .console(false)
            .with_file()
            .file_prefix("test".to_string())
            .rotation(Rotation::Hourly)
            .with_source();

        assert_eq!(config.level, Level::DEBUG);
        assert_eq!(config.console, false);
        assert_eq!(config.file, true);
        assert_eq!(config.file_prefix, "test");
        assert_eq!(config.rotation, Rotation::Hourly);
        assert_eq!(config.include_source, true);
    }

    #[test]
    fn test_log_cleaner() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path();

        // Create some test log files
        for i in 0..5 {
            let file_path = log_dir.join(format!("app.{}.log", i));
            std::fs::write(&file_path, vec![0u8; 100 * (i + 1)]).unwrap();
        }

        let cleaner = LogCleaner::new(log_dir.to_path_buf())
            .max_files(3)
            .max_size(300);

        let removed = cleaner.clean().unwrap();

        // At least 2 files should be deleted (exceeding the max_files limit)
        assert!(removed >= 2);

        // Checking the number of remaining documents
        let remaining = std::fs::read_dir(log_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().map_or(false, |e| e == "log"))
            .count();

        assert!(remaining <= 3);
    }

    #[test]
    fn test_log_cleaner_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path();

        let cleaner = LogCleaner::new(log_dir.to_path_buf());
        let removed = cleaner.clean().unwrap();

        assert_eq!(removed, 0);
    }
}
