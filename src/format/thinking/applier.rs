use crate::config::types::{ThinkingConfig, ThinkingLevel};
use crate::format::thinking::types::Provider;
use serde_json::{json, Value};

/// Apply the trait configured by thinking
pub trait ThinkingApplier: Send + Sync {
    fn apply(&self, request: &mut Value, config: &ThinkingConfig) -> Result<(), String>;
}

/// OpenAI Apps
pub struct OpenAIThinkingApplier;

impl ThinkingApplier for OpenAIThinkingApplier {
    fn apply(&self, request: &mut Value, config: &ThinkingConfig) -> Result<(), String> {
        if !config.enabled {
            request
                .as_object_mut()
                .ok_or("Invalid request")?
                .remove("reasoning_effort");
            return Ok(());
        }

        let effort = if let Some(level) = config.level {
            level.as_str().to_string()
        } else if let Some(effort) = &config.effort {
            effort.clone()
        } else {
            "auto".to_string()
        };

        request
            .as_object_mut()
            .ok_or("Invalid request")?
            .insert("reasoning_effort".to_string(), Value::String(effort));

        Ok(())
    }
}

/// Claude Applicator
pub struct ClaudeThinkingApplier;

impl ThinkingApplier for ClaudeThinkingApplier {
    fn apply(&self, request: &mut Value, config: &ThinkingConfig) -> Result<(), String> {
        if !config.enabled {
            request
                .as_object_mut()
                .ok_or("Invalid request")?
                .remove("thinking");
            return Ok(());
        }

        let thinking_obj = if let Some(thinking) = request.get("thinking") {
            thinking
                .as_object()
                .ok_or("Invalid thinking object")?
                .clone()
        } else {
            serde_json::Map::new()
        };

        let mut new_thinking = serde_json::Map::new();

        if let Some(level) = config.level {
            new_thinking.insert(
                "type".to_string(),
                Value::String(level.as_str().to_string()),
            );
        }

        if let Some(budget) = config.budget_tokens {
            new_thinking.insert("budget_tokens".to_string(), Value::Number(budget.into()));
        }

        // Retain the original other fields
        for (k, v) in thinking_obj {
            if !new_thinking.contains_key(&k) {
                new_thinking.insert(k, v);
            }
        }

        request
            .as_object_mut()
            .ok_or("Invalid request")?
            .insert("thinking".to_string(), Value::Object(new_thinking));

        Ok(())
    }
}

/// Gemini Appliance
pub struct GeminiThinkingApplier;

impl ThinkingApplier for GeminiThinkingApplier {
    fn apply(&self, request: &mut Value, config: &ThinkingConfig) -> Result<(), String> {
        if !config.enabled {
            request
                .as_object_mut()
                .ok_or("Invalid request")?
                .remove("thinkingLevel");
            request
                .as_object_mut()
                .ok_or("Invalid request")?
                .remove("thinkingBudget");
            return Ok(());
        }

        let request_obj = request.as_object_mut().ok_or("Invalid request")?;

        if let Some(level) = config.level {
            request_obj.insert(
                "thinkingLevel".to_string(),
                Value::String(level.as_str().to_string()),
            );
        }

        if let Some(budget) = config.budget_tokens {
            request_obj.insert("thinkingBudget".to_string(), Value::Number(budget.into()));
        }

        Ok(())
    }
}

/// Codex Apps
pub struct CodexThinkingApplier;

impl ThinkingApplier for CodexThinkingApplier {
    fn apply(&self, request: &mut Value, config: &ThinkingConfig) -> Result<(), String> {
        if !config.enabled {
            request
                .as_object_mut()
                .ok_or("Invalid request")?
                .remove("reasoning");
            return Ok(());
        }

        let effort = if let Some(level) = config.level {
            level.as_str().to_string()
        } else if let Some(effort) = &config.effort {
            effort.clone()
        } else {
            "auto".to_string()
        };

        let reasoning = json!({
            "effort": effort
        });

        request
            .as_object_mut()
            .ok_or("Invalid request")?
            .insert("reasoning".to_string(), reasoning);

        Ok(())
    }
}

/// Get Applier
pub fn get_applier(provider: Provider) -> Box<dyn ThinkingApplier> {
    match provider {
        Provider::OpenAI => Box::new(OpenAIThinkingApplier),
        Provider::Claude => Box::new(ClaudeThinkingApplier),
        Provider::Gemini => Box::new(GeminiThinkingApplier),
        Provider::Codex => Box::new(CodexThinkingApplier),
        Provider::Kimi => Box::new(OpenAIThinkingApplier),
    }
}

/// Applying the thinking configuration to a request
pub fn apply_thinking(
    provider: Provider,
    request: &mut Value,
    config: &ThinkingConfig,
) -> Result<(), String> {
    let applier = get_applier(provider);
    applier.apply(request, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_applier_enabled() {
        let applier = OpenAIThinkingApplier;
        let mut request = json!({
            "messages": []
        });

        let config = ThinkingConfig {
            enabled: true,
            level: Some(ThinkingLevel::High),
            budget_tokens: Some(16384),
            effort: Some("high".to_string()),
        };

        applier.apply(&mut request, &config).unwrap();
        assert_eq!(request["reasoning_effort"], "high");
    }

    #[test]
    fn test_openai_applier_disabled() {
        let applier = OpenAIThinkingApplier;
        let mut request = json!({
            "reasoning_effort": "high",
            "messages": []
        });

        let config = ThinkingConfig {
            enabled: false,
            level: Some(ThinkingLevel::None),
            budget_tokens: Some(0),
            effort: None,
        };

        applier.apply(&mut request, &config).unwrap();
        assert!(!request.get("reasoning_effort").is_some());
    }

    #[test]
    fn test_claude_applier_enabled() {
        let applier = ClaudeThinkingApplier;
        let mut request = json!({
            "messages": []
        });

        let config = ThinkingConfig {
            enabled: true,
            level: Some(ThinkingLevel::Medium),
            budget_tokens: Some(8192),
            effort: None,
        };

        applier.apply(&mut request, &config).unwrap();
        assert_eq!(request["thinking"]["type"], "medium");
        assert_eq!(request["thinking"]["budget_tokens"], 8192);
    }

    #[test]
    fn test_gemini_applier_enabled() {
        let applier = GeminiThinkingApplier;
        let mut request = json!({
            "contents": []
        });

        let config = ThinkingConfig {
            enabled: true,
            level: Some(ThinkingLevel::Low),
            budget_tokens: Some(4096),
            effort: None,
        };

        applier.apply(&mut request, &config).unwrap();
        assert_eq!(request["thinkingLevel"], "low");
        assert_eq!(request["thinkingBudget"], 4096);
    }

    #[test]
    fn test_codex_applier_enabled() {
        let applier = CodexThinkingApplier;
        let mut request = json!({
            "messages": []
        });

        let config = ThinkingConfig {
            enabled: true,
            level: Some(ThinkingLevel::High),
            budget_tokens: Some(16384),
            effort: Some("high".to_string()),
        };

        applier.apply(&mut request, &config).unwrap();
        assert_eq!(request["reasoning"]["effort"], "high");
    }

    #[test]
    fn test_apply_thinking() {
        let mut request = json!({
            "messages": []
        });

        let config = ThinkingConfig {
            enabled: true,
            level: Some(ThinkingLevel::High),
            budget_tokens: Some(16384),
            effort: Some("high".to_string()),
        };

        apply_thinking(Provider::OpenAI, &mut request, &config).unwrap();
        assert_eq!(request["reasoning_effort"], "high");
    }
}
