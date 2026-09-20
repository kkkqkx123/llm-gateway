use crate::format::types::*;
use serde_json::{json, Value};

pub struct ClaudeRequestTransformer;

impl RequestTransformer for ClaudeRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));
            if stream {
                obj.insert("stream".to_string(), Value::Bool(true));
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeStreamResponseTransformer;

impl StreamResponseTransformer for ClaudeStreamResponseTransformer {
    fn transform(
        &self,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&Value>,
    ) -> Vec<Vec<u8>> {
        let raw_str = String::from_utf8_lossy(raw_json);
        let lines: Vec<&str> = raw_str.lines().collect();

        let mut chunks = Vec::new();

        for line in lines {
            let line = line.trim();
            if line.is_empty() || line == "event: message_stop" {
                continue;
            }

            if line.starts_with("data: ") {
                let json_str = &line[6..];
                if let Ok(mut value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("model".to_string(), Value::String(model.to_string()));
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

pub struct ClaudeNonStreamResponseTransformer;

impl NonStreamResponseTransformer for ClaudeNonStreamResponseTransformer {
    fn transform(
        &self,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&Value>,
    ) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeTokenCountTransformer;

impl TokenCountTransformer for ClaudeTokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "usage": {
                "input_tokens": count,
                "output_tokens": count
            }
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_request_transformer_valid_json() {
        let transformer = ClaudeRequestTransformer;
        let input = json!({
            "messages": [{"role": "user", "content": "hello"}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "claude-3");
        assert!(!output.get("stream").is_some());
    }

    #[test]
    fn test_claude_request_transformer_with_stream() {
        let transformer = ClaudeRequestTransformer;
        let input = json!({
            "messages": [{"role": "user", "content": "hello"}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3", &raw, true);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "claude-3");
        assert_eq!(output["stream"], true);
    }

    #[test]
    fn test_claude_request_transformer_invalid_json() {
        let transformer = ClaudeRequestTransformer;
        let raw = b"invalid json";

        let result = transformer.transform("claude-3", raw, false);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_claude_stream_response_transformer() {
        let transformer = ClaudeStreamResponseTransformer;
        let raw = b"data: {\"id\":\"1\",\"delta\":{\"text\":\"hello\"}}\n\n";

        let result = transformer.transform("claude-3", b"original", b"request", raw, None);

        assert_eq!(result.len(), 1);
        let output_str = String::from_utf8_lossy(&result[0]);
        assert!(output_str.contains("data: "));
        assert!(output_str.contains("\"model\":\"claude-3\""));
    }

    #[test]
    fn test_claude_stream_response_transformer_with_stop() {
        let transformer = ClaudeStreamResponseTransformer;
        let raw = b"data: {\"id\":\"1\"}\n\nevent: message_stop\ndata: {\"id\":\"2\"}\n\n";

        let result = transformer.transform("claude-3", b"", b"", raw, None);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_claude_non_stream_response_transformer() {
        let transformer = ClaudeNonStreamResponseTransformer;
        let input = json!({
            "id": "1",
            "content": [{"text": "hello"}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3", b"orig", b"req", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "claude-3");
    }

    #[test]
    fn test_claude_token_count_transformer() {
        let transformer = ClaudeTokenCountTransformer;
        let result = transformer.transform(456);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["usage"]["input_tokens"], 456);
        assert_eq!(output["usage"]["output_tokens"], 456);
    }
}
