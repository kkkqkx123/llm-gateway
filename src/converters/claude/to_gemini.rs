use crate::format::types::*;
use serde_json::{json, Value};

pub struct ClaudeToGeminiRequestTransformer;

impl RequestTransformer for ClaudeToGeminiRequestTransformer {
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

            obj.remove("anthropic_version");

            if let Some(max_tokens) = obj.remove("max_tokens") {
                obj.insert("maxOutputTokens".to_string(), max_tokens);
            }
        }

        serde_json::to_vec(&value).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeToGeminiStreamResponseTransformer;

impl StreamResponseTransformer for ClaudeToGeminiStreamResponseTransformer {
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
            if line.is_empty() {
                continue;
            }

            if line.starts_with("event: ") {
                let event_type = &line[7..];
                continue;
            }

            if line.starts_with("data: ") {
                let json_str = &line[6..];
                if let Ok(value) = serde_json::from_str::<Value>(json_str) {
                    if let Some(obj) = value.as_object() {
                        if let Some(type_field) = obj.get("type").and_then(|t| t.as_str()) {
                            match type_field {
                                "content_block_delta" => {
                                    if let Some(delta) =
                                        obj.get("delta").and_then(|d| d.as_object())
                                    {
                                        if let Some(text) =
                                            delta.get("text").and_then(|t| t.as_str())
                                        {
                                            let gemini_response = json!({
                                                "candidates": [{
                                                    "content": {
                                                        "parts": [{"text": text}],
                                                        "role": "model"
                                                    },
                                                    "finishReason": null
                                                }]
                                            });

                                            if let Ok(bytes) = serde_json::to_vec(&gemini_response)
                                            {
                                                chunks.push(
                                                    format!(
                                                        "data: {}\n\n",
                                                        String::from_utf8_lossy(&bytes)
                                                    )
                                                    .into_bytes(),
                                                );
                                            }
                                        }
                                    }
                                }
                                "content_block_stop" | "message_stop" => {
                                    let gemini_response = json!({
                                        "candidates": [{
                                            "content": {
                                                "parts": [],
                                                "role": "model"
                                            },
                                            "finishReason": "STOP"
                                        }]
                                    });

                                    if let Ok(bytes) = serde_json::to_vec(&gemini_response) {
                                        chunks.push(
                                            format!(
                                                "data: {}\n\n",
                                                String::from_utf8_lossy(&bytes)
                                            )
                                            .into_bytes(),
                                        );
                                    }
                                }
                                _ => {}
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

pub struct ClaudeToGeminiNonStreamResponseTransformer;

impl NonStreamResponseTransformer for ClaudeToGeminiNonStreamResponseTransformer {
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

        let mut gemini_response = json!({
            "candidates": []
        });

        let mut combined_text = String::new();
        if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
            for content_block in content {
                if let Some(text) = content_block.get("text").and_then(|t| t.as_str()) {
                    combined_text.push_str(text);
                }
            }
        }

        let stop_reason = match value.get("stop_reason").and_then(|s| s.as_str()) {
            Some("end_turn") => "STOP",
            Some("max_tokens") => "MAX_TOKENS",
            Some("stop_sequence") => "SAFETY",
            _ => "STOP",
        };

        gemini_response["candidates"] = json!([{
            "content": {
                "parts": [{"text": combined_text}],
                "role": "model"
            },
            "finishReason": stop_reason,
            "index": 0
        }]);

        if let Some(usage) = value.get("usage").and_then(|u| u.as_object()) {
            gemini_response["usageMetadata"] = json!({
                "promptTokenCount": usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                "candidatesTokenCount": usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                "totalTokenCount": usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
                    + usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
            });
        }

        serde_json::to_vec(&gemini_response).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct ClaudeToGeminiTokenCountTransformer;

impl TokenCountTransformer for ClaudeToGeminiTokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "candidatesTokenCount": count,
            "totalTokenCount": count
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_to_gemini_request() {
        let transformer = ClaudeToGeminiRequestTransformer;
        let input = json!({
            "model": "claude-3-opus",
            "messages": [{"role": "user", "content": "hello"}],
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

    #[test]
    fn test_claude_to_gemini_non_stream_response() {
        let transformer = ClaudeToGeminiNonStreamResponseTransformer;
        let input = json!({
            "id": "msg-1",
            "role": "assistant",
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gemini-pro", b"", b"", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert!(output.get("candidates").is_some());
        assert_eq!(
            output["candidates"][0]["content"]["parts"][0]["text"],
            "hello"
        );
        assert_eq!(output["candidates"][0]["finishReason"], "STOP");
    }

    #[test]
    fn test_claude_to_gemini_token_count() {
        let transformer = ClaudeToGeminiTokenCountTransformer;
        let result = transformer.transform(100);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["candidatesTokenCount"], 100);
        assert_eq!(output["totalTokenCount"], 100);
    }
}
