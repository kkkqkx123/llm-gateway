use crate::config::types::{ThinkingConfig, ThinkingLevel};
use serde::{Deserialize, Serialize};

/// Provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    OpenAI,
    Claude,
    Gemini,
    Codex,
    Kimi,
}

impl Provider {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "claude" => Provider::Claude,
            "gemini" => Provider::Gemini,
            "codex" => Provider::Codex,
            "kimi" => Provider::Kimi,
            _ => Provider::OpenAI,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Provider::OpenAI => "openai",
            Provider::Claude => "claude",
            Provider::Gemini => "gemini",
            Provider::Codex => "codex",
            Provider::Kimi => "kimi",
        }
    }
}

/// Thinking configuration error
#[derive(Debug, thiserror::Error)]
pub enum ThinkingError {
    #[error("Invalid thinking level: {0}")]
    InvalidLevel(String),

    #[error("Invalid thinking budget: {0}")]
    InvalidBudget(String),

    #[error("Model does not support thinking: {0}")]
    NotSupported(String),

    #[error("Thinking budget exceeds model limit: {budget} > {limit}")]
    ExceedsLimit { budget: u32, limit: u32 },

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thinking_level_from_str() {
        assert_eq!(ThinkingLevel::from_str("minimal"), ThinkingLevel::Minimal);
        assert_eq!(ThinkingLevel::from_str("low"), ThinkingLevel::Low);
        assert_eq!(ThinkingLevel::from_str("medium"), ThinkingLevel::Medium);
        assert_eq!(ThinkingLevel::from_str("high"), ThinkingLevel::High);
        assert_eq!(ThinkingLevel::from_str("xhigh"), ThinkingLevel::XHigh);
        assert_eq!(ThinkingLevel::from_str("auto"), ThinkingLevel::Auto);
        assert_eq!(ThinkingLevel::from_str("none"), ThinkingLevel::None);
        assert_eq!(ThinkingLevel::from_str("unknown"), ThinkingLevel::None);
    }

    #[test]
    fn test_thinking_level_to_budget() {
        assert_eq!(ThinkingLevel::Minimal.to_budget(), Some(2048));
        assert_eq!(ThinkingLevel::Low.to_budget(), Some(4096));
        assert_eq!(ThinkingLevel::Medium.to_budget(), Some(8192));
        assert_eq!(ThinkingLevel::High.to_budget(), Some(16384));
        assert_eq!(ThinkingLevel::XHigh.to_budget(), Some(32768));
        assert_eq!(ThinkingLevel::Auto.to_budget(), None);
        assert_eq!(ThinkingLevel::None.to_budget(), Some(0));
    }

    #[test]
    fn test_thinking_level_from_budget() {
        assert_eq!(ThinkingLevel::from_budget(0), ThinkingLevel::None);
        assert_eq!(ThinkingLevel::from_budget(1024), ThinkingLevel::Minimal);
        assert_eq!(ThinkingLevel::from_budget(2048), ThinkingLevel::Minimal);
        assert_eq!(ThinkingLevel::from_budget(4096), ThinkingLevel::Low);
        assert_eq!(ThinkingLevel::from_budget(8192), ThinkingLevel::Medium);
        assert_eq!(ThinkingLevel::from_budget(16384), ThinkingLevel::High);
        assert_eq!(ThinkingLevel::from_budget(32768), ThinkingLevel::XHigh);
        assert_eq!(ThinkingLevel::from_budget(65536), ThinkingLevel::XHigh);
    }

    #[test]
    fn test_thinking_config_default() {
        let config = ThinkingConfig::default();
        assert!(!config.enabled);
        assert!(config.budget_tokens.is_none());
        assert!(config.level.is_none());
        assert!(config.effort.is_none());
    }

    #[test]
    fn test_thinking_config_with_budget() {
        let config = ThinkingConfig::new().with_budget(8192);
        assert_eq!(config.budget_tokens, Some(8192));
        assert_eq!(config.level, Some(ThinkingLevel::Medium));
    }

    #[test]
    fn test_thinking_config_with_level() {
        let config = ThinkingConfig::new().with_level(ThinkingLevel::High);
        assert_eq!(config.level, Some(ThinkingLevel::High));
        assert_eq!(config.budget_tokens, Some(16384));
    }

    #[test]
    fn test_thinking_config_merge() {
        let config1 = ThinkingConfig::new().with_budget(4096);
        let config2 = ThinkingConfig::new()
            .with_level(ThinkingLevel::High)
            .with_effort("high".to_string());
        let merged = config1.merge(config2);

        // The merge method will overwrite existing fields
        assert_eq!(merged.budget_tokens, Some(16384)); // High-level budget
        assert_eq!(merged.level, Some(ThinkingLevel::High));
        assert_eq!(merged.effort, Some("high".to_string()));
    }

    #[test]
    fn test_provider_from_str() {
        assert_eq!(Provider::from_str("openai"), Provider::OpenAI);
        assert_eq!(Provider::from_str("claude"), Provider::Claude);
        assert_eq!(Provider::from_str("gemini"), Provider::Gemini);
        assert_eq!(Provider::from_str("codex"), Provider::Codex);
        assert_eq!(Provider::from_str("kimi"), Provider::Kimi);
    }
}
