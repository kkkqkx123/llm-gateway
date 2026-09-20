use crate::format::types::*;
use serde_json::{json, Value};

pub struct OpenAIToGeminiRequestTransformer;

impl RequestTransformer for OpenAIToGeminiRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));

            if let Some(messages) = obj.remove("messages") {
                if let Some(msg_array) = messages.as_array() {
                    let mut contents = Vec::new();
                    for msg in msg_array {
                        if let Some(msg_obj) = msg.as_object() {
                            let role = msg_obj
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("user");
                            let content = msg_obj
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("");

                            let gemini_role = match role {
                                "system" => "user",
                                "user" => "user",
                                "assistant" => "model",
                                _ => "user",
                            };

                            contents.push(json!({
                                "role": gemini_role,
                                "parts": [{"text": content}]
                            }));
                        }
                    }
                    obj.insert("contents".to_string(), json!(contents));
                }
            }

            obj.remove("temperature");
            obj.remove("top_p");
            obj.remove("n");
            obj.remove("presence_penalty");
            obj.remove("frequency_penalty");
            obj.remove("user");

            if let Some(max_tokens) = obj.remove("max_tokens") {
                obj.insert("maxOutputTokens".to_string(), max_tokens);
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_to_gemini_request() {
        let transformer = OpenAIToGeminiRequestTransformer;
        let input = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "hello"}
            ],
            "max_tokens": 100
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gemini-pro", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gemini-pro");
        assert!(output.get("contents").is_some());
        assert!(!output.get("messages").is_some());
        assert_eq!(output["maxOutputTokens"], 100);
    }
}
