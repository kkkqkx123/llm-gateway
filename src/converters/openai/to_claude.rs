use crate::format::types::*;
use serde_json::{json, Value};

pub struct OpenAIToClaudeRequestTransformer;

impl RequestTransformer for OpenAIToClaudeRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));

            if let Some(messages) = obj.get_mut("messages") {
                if let Some(msg_array) = messages.as_array_mut() {
                    for msg in msg_array {
                        if let Some(msg_obj) = msg.as_object_mut() {
                            if let Some(role) = msg_obj.get("role").and_then(|r| r.as_str()) {
                                let normalized_role = match role {
                                    "system" => "system",
                                    "user" => "user",
                                    "assistant" => "assistant",
                                    _ => "user",
                                };
                                msg_obj.insert(
                                    "role".to_string(),
                                    Value::String(normalized_role.to_string()),
                                );
                            }
                        }
                    }
                }
            }

            obj.remove("temperature");
            obj.remove("top_p");
            obj.remove("n");
            obj.remove("presence_penalty");
            obj.remove("frequency_penalty");
            obj.remove("user");

            if let Some(max_tokens) = obj.remove("max_tokens") {
                obj.insert("max_tokens".to_string(), max_tokens);
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_to_claude_request() {
        let transformer = OpenAIToClaudeRequestTransformer;
        let input = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 0.7,
            "max_tokens": 100
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3-opus", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "claude-3-opus");
        assert!(!output.get("temperature").is_some());
        assert_eq!(output["max_tokens"], 100);
    }
}
