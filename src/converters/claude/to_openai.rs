use crate::format::types::*;
use serde_json::{json, Value};

pub struct ClaudeToOpenAIRequestTransformer;

impl RequestTransformer for ClaudeToOpenAIRequestTransformer {
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

            obj.remove("anthropic_version");
            obj.remove("top_k");
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeToOpenAIStreamResponseTransformer;

impl StreamResponseTransformer for ClaudeToOpenAIStreamResponseTransformer {
    fn transform(
        &self,
        model: &str,
        _original_request_raw_json: &[u8],
        _request_raw_json: &[u8],
        raw_json: &[u8],
        _param: Option<&Value>,
    ) -> Vec<Vec<u8>> {
        let raw_str = String::from_utf8_lossy(raw_json);
        let lines: Vec<&str> = raw_str.lines().collect();

        let mut chunks = Vec::new();

        for line in lines {
            let line = line.trim();
            if line.is_empty() || line == "data: [DONE]" {
                continue;
            }

            if line.starts_with("data: ") {
                let json_str = &line[6..];
                if let Ok(mut value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("model".to_string(), Value::String(model.to_string()));

                        if let Some(delta) = obj.get_mut("delta").and_then(|d| d.as_object_mut()) {
                            if let Some(text) = delta.remove("text") {
                                delta.insert("content".to_string(), text);
                            }
                            if let Some(content) = delta.remove("content") {
                                delta.insert("content".to_string(), content);
                            }
                        }

                        if let Some(choices) = obj.get_mut("choices").and_then(|c| c.as_array_mut())
                        {
                            for choice in choices {
                                if let Some(choice_obj) = choice.as_object_mut() {
                                    choice_obj.insert("index".to_string(), Value::Number(0.into()));
                                    if let Some(delta) =
                                        choice_obj.get_mut("delta").and_then(|d| d.as_object_mut())
                                    {
                                        if let Some(text) = delta.remove("text") {
                                            delta.insert("content".to_string(), text);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Ok(bytes) = serde_json::to_vec(&value) {
                        chunks.push(
                            format!("data: {}\n\n", String::from_utf8_lossy(&bytes)).into_bytes(),
                        );
                    }
                }
            }
        }

        if chunks.is_empty() {
            chunks.push(raw_json.to_vec());
        }

        chunks
    }
}

pub struct ClaudeToOpenAINonStreamResponseTransformer;

impl NonStreamResponseTransformer for ClaudeToOpenAINonStreamResponseTransformer {
    fn transform(
        &self,
        model: &str,
        _original_request_raw_json: &[u8],
        _request_raw_json: &[u8],
        raw_json: &[u8],
        _param: Option<&Value>,
    ) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));

            if let Some(content) = obj.remove("content") {
                obj.insert(
                    "choices".to_string(),
                    json!([{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": content
                        },
                        "finish_reason": "stop"
                    }]),
                );
            }

            if let Some(usage) = obj.get_mut("usage").and_then(|u| u.as_object_mut()) {
                if !usage.contains_key("total_tokens") {
                    usage.insert("total_tokens".to_string(), json!(0));
                }
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeToOpenAITokenCountTransformer;

impl TokenCountTransformer for ClaudeToOpenAITokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "total_tokens": count,
            "prompt_tokens": count,
            "completion_tokens": 0
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_to_openai_request() {
        let transformer = ClaudeToOpenAIRequestTransformer;
        let input = json!({
            "model": "claude-3-opus",
            "messages": [{"role": "user", "content": "hello"}],
            "anthropic_version": "2023-06-01"
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(!output.get("anthropic_version").is_some());
    }

    #[test]
    fn test_claude_to_openai_stream_response() {
        let transformer = ClaudeToOpenAIStreamResponseTransformer;
        let raw = b"data: {\"id\":\"1\",\"delta\":{\"text\":\"hello\"}}\n\n";

        let result = transformer.transform("gpt-4", b"", b"", raw, None);

        assert_eq!(result.len(), 1);
        let output_str = String::from_utf8_lossy(&result[0]);
        assert!(output_str.contains("data: "));
        assert!(output_str.contains("\"model\":\"gpt-4\""));
        assert!(output_str.contains("\"content\""));
    }

    #[test]
    fn test_claude_to_openai_non_stream_response() {
        let transformer = ClaudeToOpenAINonStreamResponseTransformer;
        let input = json!({
            "id": "1",
            "content": "hello",
            "usage": {"total_tokens": 10}
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", b"", b"", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(output.get("choices").is_some());
        assert_eq!(output["choices"][0]["message"]["content"], "hello");
    }

    #[test]
    fn test_claude_to_openai_token_count() {
        let transformer = ClaudeToOpenAITokenCountTransformer;
        let result = transformer.transform(123);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["total_tokens"], 123);
        assert_eq!(output["prompt_tokens"], 123);
        assert_eq!(output["completion_tokens"], 0);
    }
}
