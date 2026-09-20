use crate::format::types::*;
use serde_json::{json, Value};

pub struct GeminiToClaudeRequestTransformer;

impl RequestTransformer for GeminiToClaudeRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        if let Some(obj) = value.as_object_mut() {
            obj.insert("model".to_string(), Value::String(model.to_string()));

            if let Some(contents) = obj.remove("contents") {
                if let Some(content_array) = contents.as_array() {
                    let mut messages = Vec::new();
                    for content in content_array {
                        if let Some(content_obj) = content.as_object() {
                            let role = content_obj
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("user");
                            let text = content_obj
                                .get("parts")
                                .and_then(|p| p.as_array())
                                .and_then(|parts| parts.first())
                                .and_then(|first| first.get("text"))
                                .and_then(|t| t.as_str())
                                .unwrap_or("");

                            let claude_role = match role {
                                "user" => "user",
                                "model" => "assistant",
                                _ => "user",
                            };

                            messages.push(json!({
                                "role": claude_role,
                                "content": text
                            }));
                        }
                    }
                    obj.insert("messages".to_string(), json!(messages));
                }
            }

            obj.remove("generationConfig");

            if let Some(max_tokens) = obj.remove("maxOutputTokens") {
                obj.insert("max_tokens".to_string(), max_tokens);
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct GeminiToClaudeStreamResponseTransformer;

impl StreamResponseTransformer for GeminiToClaudeStreamResponseTransformer {
    fn transform(
        &self,
        _model: &str,
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
                if let Ok(value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
                        if let Some(first) = candidates.first() {
                            if let Some(content) = first.get("content").and_then(|c| c.as_object())
                            {
                                if let Some(parts) = content.get("parts").and_then(|p| p.as_array())
                                {
                                    if let Some(first_part) = parts.first() {
                                        if let Some(text) =
                                            first_part.get("text").and_then(|t| t.as_str())
                                        {
                                            let claude_response = json!({
                                                "type": "content_block_delta",
                                                "index": 0,
                                                "delta": {
                                                    "type": "text_delta",
                                                    "text": text
                                                }
                                            });

                                            if let Ok(bytes) = serde_json::to_vec(&claude_response)
                                            {
                                                chunks.push(
                                                    format!(
                                                        "event: content_block_delta\ndata: {}\n\n",
                                                        String::from_utf8_lossy(&bytes)
                                                    )
                                                    .into_bytes(),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
                        if let Some(first) = candidates.first() {
                            if let Some(finish_reason) = first.get("finishReason") {
                                if !finish_reason.is_null() {
                                    let claude_response = json!({
                                        "type": "content_block_stop",
                                        "index": 0
                                    });

                                    if let Ok(bytes) = serde_json::to_vec(&claude_response) {
                                        chunks.push(
                                            format!(
                                                "event: content_block_stop\ndata: {}\n\n",
                                                String::from_utf8_lossy(&bytes)
                                            )
                                            .into_bytes(),
                                        );
                                    }

                                    let stop_response = json!({
                                        "type": "message_stop"
                                    });

                                    if let Ok(bytes) = serde_json::to_vec(&stop_response) {
                                        chunks.push(
                                            format!(
                                                "event: message_stop\ndata: {}\n\n",
                                                String::from_utf8_lossy(&bytes)
                                            )
                                            .into_bytes(),
                                        );
                                    }
                                }
                            }
                        }
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

pub struct GeminiToClaudeNonStreamResponseTransformer;

impl NonStreamResponseTransformer for GeminiToClaudeNonStreamResponseTransformer {
    fn transform(
        &self,
        _model: &str,
        _original_request_raw_json: &[u8],
        _request_raw_json: &[u8],
        raw_json: &[u8],
        _param: Option<&Value>,
    ) -> Vec<u8> {
        let value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        let mut claude_response = json!({
            "id": format!("msg_{}", uuid::Uuid::new_v4()),
            "type": "message",
            "role": "assistant",
            "content": []
        });

        if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
            let mut content_blocks = Vec::new();
            for candidate in candidates {
                if let Some(content) = candidate.get("content").and_then(|c| c.as_object()) {
                    if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                        let mut combined_text = String::new();
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                combined_text.push_str(text);
                            }
                        }

                        content_blocks.push(json!({
                            "type": "text",
                            "text": combined_text
                        }));
                    }
                }
            }
            claude_response["content"] = json!(content_blocks);

            if let Some(first_candidate) = candidates.first() {
                if let Some(finish_reason) = first_candidate
                    .get("finishReason")
                    .and_then(|fr| fr.as_str())
                {
                    let stop_reason = match finish_reason {
                        "STOP" => "end_turn",
                        "MAX_TOKENS" => "max_tokens",
                        "SAFETY" => "stop_sequence",
                        _ => "end_turn",
                    };
                    claude_response["stop_reason"] = json!(stop_reason);
                }
            }
        }

        if let Some(usage) = value.get("usageMetadata") {
            claude_response["usage"] = json!({
                "input_tokens": usage.get("promptTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                "output_tokens": usage.get("candidatesTokenCount").and_then(|v| v.as_i64()).unwrap_or(0)
            });
        }

        serde_json::to_vec(&claude_response).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct GeminiToClaudeTokenCountTransformer;

impl TokenCountTransformer for GeminiToClaudeTokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "output_tokens": count
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_to_claude_request() {
        let transformer = GeminiToClaudeRequestTransformer;
        let input = json!({
            "model": "gemini-pro",
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "maxOutputTokens": 100
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3-opus", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "claude-3-opus");
        assert!(output.get("messages").is_some());
        assert!(!output.get("contents").is_some());
        assert_eq!(output["max_tokens"], 100);
    }

    #[test]
    fn test_gemini_to_claude_non_stream_response() {
        let transformer = GeminiToClaudeNonStreamResponseTransformer;
        let input = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "hello"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("claude-3-opus", b"", b"", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["role"], "assistant");
        assert!(output.get("content").is_some());
        assert_eq!(output["content"][0]["text"], "hello");
        assert_eq!(output["stop_reason"], "end_turn");
    }

    #[test]
    fn test_gemini_to_claude_token_count() {
        let transformer = GeminiToClaudeTokenCountTransformer;
        let result = transformer.transform(100);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["output_tokens"], 100);
    }
}
