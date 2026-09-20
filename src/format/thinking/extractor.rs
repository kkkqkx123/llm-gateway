use crate::config::types::{ThinkingConfig, ThinkingLevel};
use crate::format::thinking::types::Provider;
use serde_json::{json, Value};

/// Extract the trait configured by thinking
pub trait ThinkingExtractor: Send + Sync {
    fn extract(&self, request: &Value) -> Option<ThinkingConfig>;
}

/// OpenAI Extractor
pub struct OpenAIThinkingExtractor;

impl ThinkingExtractor for OpenAIThinkingExtractor {
    fn extract(&self, request: &Value) -> Option<ThinkingConfig> {
        if let Some(effort) = request.get("reasoning_effort").and_then(|v| v.as_str()) {
            let level = match effort.to_lowercase().as_str() {
                "low" => ThinkingLevel::Low,
                "medium" => ThinkingLevel::Medium,
                "high" => ThinkingLevel::High,
                _ => ThinkingLevel::Auto,
            };

            return Some(ThinkingConfig {
                enabled: true,
                budget_tokens: level.to_budget(),
                level: Some(level),
                effort: Some(effort.to_string()),
            });
        }

        None
    }
}

/// Claude Extractor
pub struct ClaudeThinkingExtractor;

impl ThinkingExtractor for ClaudeThinkingExtractor {
    fn extract(&self, request: &Value) -> Option<ThinkingConfig> {
        let mut config = ThinkingConfig::default();

        if let Some(thinking) = request.get("thinking") {
            if let Some(thinking_obj) = thinking.as_object() {
                if let Some(thinking_type) = thinking_obj.get("type").and_then(|v| v.as_str()) {
                    config.level = Some(ThinkingLevel::from_str(thinking_type));
                    config.enabled = true;
                }

                if let Some(budget) = thinking_obj.get("budget_tokens").and_then(|v| v.as_u64()) {
                    config.budget_tokens = Some(budget as u32);
                    config.enabled = true;
                }
            }
        }

        if let Some(output_config) = request.get("output_config") {
            if let Some(output_obj) = output_config.as_object() {
                if let Some(effort) = output_obj.get("effort").and_then(|v| v.as_str()) {
                    config.effort = Some(effort.to_string());
                    config.enabled = true;
                }
            }
        }

        if config.enabled {
            Some(config)
        } else {
            None
        }
    }
}

/// Gemini Extractor
pub struct GeminiThinkingExtractor;

impl ThinkingExtractor for GeminiThinkingExtractor {
    fn extract(&self, request: &Value) -> Option<ThinkingConfig> {
        let mut config = ThinkingConfig::default();

        if let Some(level) = request.get("thinkingLevel").and_then(|v| v.as_str()) {
            config.level = Some(ThinkingLevel::from_str(level));
            config.enabled = true;
        }

        if let Some(budget) = request.get("thinkingBudget").and_then(|v| v.as_u64()) {
            config.budget_tokens = Some(budget as u32);
            config.enabled = true;
        }

        if config.enabled {
            Some(config)
        } else {
            None
        }
    }
}

/// Codex Extractor
pub struct CodexThinkingExtractor;

impl ThinkingExtractor for CodexThinkingExtractor {
    fn extract(&self, request: &Value) -> Option<ThinkingConfig> {
        if let Some(reasoning) = request.get("reasoning") {
            if let Some(reasoning_obj) = reasoning.as_object() {
                if let Some(effort) = reasoning_obj.get("effort").and_then(|v| v.as_str()) {
                    let level = match effort.to_lowercase().as_str() {
                        "low" => ThinkingLevel::Low,
                        "medium" => ThinkingLevel::Medium,
                        "high" => ThinkingLevel::High,
                        _ => ThinkingLevel::Auto,
                    };

                    return Some(ThinkingConfig {
                        enabled: true,
                        budget_tokens: level.to_budget(),
                        level: Some(level),
                        effort: Some(effort.to_string()),
                    });
                }
            }
        }

        None
    }
}

/// Get Extractor
pub fn get_extractor(provider: Provider) -> Box<dyn ThinkingExtractor> {
    match provider {
        Provider::OpenAI => Box::new(OpenAIThinkingExtractor),
        Provider::Claude => Box::new(ClaudeThinkingExtractor),
        Provider::Gemini => Box::new(GeminiThinkingExtractor),
        Provider::Codex => Box::new(CodexThinkingExtractor),
        Provider::Kimi => Box::new(OpenAIThinkingExtractor), // Kimi uses same format as OpenAI
    }
}

/// Extracting thinking configurations from requests
pub fn extract_thinking(provider: Provider, request: &Value) -> Option<ThinkingConfig> {
    let extractor = get_extractor(provider);
    extractor.extract(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_extractor() {
        let extractor = OpenAIThinkingExtractor;
        let request = json!({
            "reasoning_effort": "high",
            "messages": []
        });

        let config = extractor.extract(&request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::High));
        assert_eq!(config.effort, Some("high".to_string()));
    }

    #[test]
    fn test_openai_extractor_no_thinking() {
        let extractor = OpenAIThinkingExtractor;
        let request = json!({
            "messages": []
        });

        assert!(extractor.extract(&request).is_none());
    }

    #[test]
    fn test_claude_extractor_with_thinking() {
        let extractor = ClaudeThinkingExtractor;
        let request = json!({
            "thinking": {
                "type": "high",
                "budget_tokens": 16384
            },
            "messages": []
        });

        let config = extractor.extract(&request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::High));
        assert_eq!(config.budget_tokens, Some(16384));
    }

    #[test]
    fn test_claude_extractor_with_output_config() {
        let extractor = ClaudeThinkingExtractor;
        let request = json!({
            "output_config": {
                "effort": "high"
            },
            "messages": []
        });

        let config = extractor.extract(&request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.effort, Some("high".to_string()));
    }

    #[test]
    fn test_gemini_extractor() {
        let extractor = GeminiThinkingExtractor;
        let request = json!({
            "thinkingLevel": "medium",
            "thinkingBudget": 8192,
            "contents": []
        });

        let config = extractor.extract(&request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::Medium));
        assert_eq!(config.budget_tokens, Some(8192));
    }

    #[test]
    fn test_codex_extractor() {
        let extractor = CodexThinkingExtractor;
        let request = json!({
            "reasoning": {
                "effort": "medium"
            },
            "messages": []
        });

        let config = extractor.extract(&request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::Medium));
        assert_eq!(config.effort, Some("medium".to_string()));
    }

    #[test]
    fn test_extract_thinking() {
        let request = json!({
            "reasoning_effort": "high",
            "messages": []
        });

        let config = extract_thinking(Provider::OpenAI, &request).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, Some(ThinkingLevel::High));
    }
}
