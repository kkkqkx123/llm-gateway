use crate::config::types::{Config, ConfigError};
use std::path::Path;

/// Configuration Loader
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from specified path
    pub fn load(path: &str) -> Result<Config, ConfigError> {
        let config = Config::from_file(path)?;
        Ok(config)
    }

    /// Load configuration or return to default configuration
    pub fn load_or_default(path: &str) -> Config {
        if Path::new(path).exists() {
            match Self::load(path) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("Failed to load config from {}, using defaults: {}", path, e);
                    Config::default()
                }
            }
        } else {
            eprintln!("Config file {} not found, using defaults", path);
            Config::default()
        }
    }

    /// Overriding Configuration from Environment Variables
    pub fn load_with_env_overrides(path: &str) -> Result<Config, ConfigError> {
        let mut config = Self::load_or_default(path);
        Self::apply_env_overrides(&mut config);
        config.validate()?;
        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(config: &mut Config) {
        if let Ok(host) = std::env::var("CLI_PROXY_HOST") {
            config.server.host = host;
        }

        if let Ok(port) = std::env::var("CLI_PROXY_PORT") {
            if let Ok(p) = port.parse::<u16>() {
                config.server.port = p;
            }
        }

        if let Ok(log_dir) = std::env::var("CLI_PROXY_LOG_DIR") {
            config.logging.log_dir = log_dir;
        }

        if let Ok(to_stdout) = std::env::var("CLI_PROXY_LOG_STDOUT") {
            config.logging.to_stdout = to_stdout.parse().unwrap_or(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.logging.log_dir, "./logs");
        assert!(config.polling.enabled);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        config.server.port = 0;
        assert!(config.validate().is_err());

        config.server.port = 8080;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_load_or_default() {
        let config = ConfigLoader::load_or_default("/non/existent/path.yaml");
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
    }
}
