pub mod applier;
pub mod extractor;
pub mod suffix;
pub mod types;

pub use applier::*;
pub use extractor::*;
pub use suffix::*;
pub use types::*;

use crate::config::types::ThinkingConfig;
use crate::format::thinking::types::Provider;
use serde_json::{json, Value};

/// Unified thinking configuration processor
pub struct ThinkingProcessor {
    provider: Provider,
}

impl ThinkingProcessor {
    pub fn new(provider: Provider) -> Self {
        Self { provider }
    }

    /// Process requests: Extract and apply thinking configurations
    /// Priority: Model Suffix> Request Body Configuration
    pub fn process(&self, request: &mut Value, model: &str) -> Result<ThinkingConfig, String> {
        // 1. Extract configuration from model suffix (highest priority)
        let config_from_suffix = SuffixParser::extract_from_model(model);

        // 2. Extract configuration from request body
        let config_from_body = extract_thinking(self.provider, request);

        // 3. Merge configuration (suffix first)
        let final_config = match (config_from_suffix, config_from_body) {
            (Some(suffix_config), Some(body_config)) => body_config.merge(suffix_config),
            (Some(suffix_config), None) => suffix_config,
            (None, Some(body_config)) => body_config,
            (None, None) => ThinkingConfig::default(),
        };

        // 4. Apply configuration to request body
        apply_thinking(self.provider, request, &final_config)?;

        Ok(final_config)
    }

    /// Only extract configuration, not apply
    pub fn extract_only(&self, request: &Value, model: &str) -> Option<ThinkingConfig> {
        let config_from_suffix = SuffixParser::extract_from_model(model);
        let config_from_body = extract_thinking(self.provider, request);

        match (config_from_suffix, config_from_body) {
            (Some(suffix_config), Some(body_config)) => Some(body_config.merge(suffix_config)),
            (Some(suffix_config), None) => Some(suffix_config),
            (None, Some(body_config)) => Some(body_config),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ThinkingLevel;

    #[test]
    fn test_processor_with_suffix() {
        let processor = ThinkingProcessor::new(Provider::OpenAI);
        let mut request = json!({
            "messages": []
        });

        let config = processor.process(&mut request, "gemini-pro(8192)").unwrap();
        assert!(config.enabled);
        assert_eq!(config.budget_tokens, Some(8192));
    }

    #[test]
    fn test_processor_with_body_config() {
        let processor = ThinkingProcessor::new(Provider::OpenAI);
        let mut request = json!({
            "reasoning_effort": "high",
            "messages": []
        });

        let config = processor.process(&mut request, "gemini-pro").unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::High));
    }

    #[test]
    fn test_processor_suffix_overrides_body() {
        let processor = ThinkingProcessor::new(Provider::OpenAI);
        let mut request = json!({
            "reasoning_effort": "low",
            "messages": []
        });

        let config = processor.process(&mut request, "gemini-pro(high)").unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::High));
    }

    #[test]
    fn test_processor_no_config() {
        let processor = ThinkingProcessor::new(Provider::OpenAI);
        let mut request = json!({
            "messages": []
        });

        let config = processor.process(&mut request, "gemini-pro").unwrap();
        assert!(!config.enabled);
    }

    #[test]
    fn test_extract_only() {
        let processor = ThinkingProcessor::new(Provider::OpenAI);
        let request = json!({
            "reasoning_effort": "medium",
            "messages": []
        });

        let config = processor.extract_only(&request, "gemini-pro").unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::Medium));
    }
}
