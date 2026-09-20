use crate::format::types::*;
use serde_json::{json, Value};

pub struct GeminiToOpenAIRequestTransformer;

impl RequestTransformer for GeminiToOpenAIRequestTransformer {
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

                            let openai_role = match role {
                                "user" => "user",
                                "model" => "assistant",
                                _ => "user",
                            };

                            messages.push(json!({
                                "role": openai_role,
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

pub struct GeminiToOpenAIStreamResponseTransformer;

impl StreamResponseTransformer for GeminiToOpenAIStreamResponseTransformer {
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
                                            let openai_response = json!({
                                                "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                                                "object": "chat.completion.chunk",
                                                "created": chrono::Utc::now().timestamp(),
                                                "model": model,
                                                "choices": [{
                                                    "index": 0,
                                                    "delta": {
                                                        "content": text
                                                    },
                                                    "finish_reason": null
                                                }]
                                            });

                                            if let Ok(bytes) = serde_json::to_vec(&openai_response)
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
                            }
                        }
                    }

                    if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
                        if let Some(first) = candidates.first() {
                            if let Some(finish_reason) = first.get("finishReason") {
                                if !finish_reason.is_null() {
                                    let openai_response = json!({
                                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                                        "object": "chat.completion.chunk",
                                        "created": chrono::Utc::now().timestamp(),
                                        "model": model,
                                        "choices": [{
                                            "index": 0,
                                            "delta": {},
                                            "finish_reason": finish_reason
                                        }]
                                    });

                                    if let Ok(bytes) = serde_json::to_vec(&openai_response) {
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

pub struct GeminiToOpenAINonStreamResponseTransformer;

impl NonStreamResponseTransformer for GeminiToOpenAINonStreamResponseTransformer {
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

        let mut openai_response = json!({
            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": model,
            "choices": []
        });

        if let Some(candidates) = value.get("candidates").and_then(|c| c.as_array()) {
            let mut choices = Vec::new();
            for candidate in candidates {
                if let Some(content) = candidate.get("content").and_then(|c| c.as_object()) {
                    if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                        let mut combined_text = String::new();
                        for part in parts {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                combined_text.push_str(text);
                            }
                        }

                        let finish_reason = candidate
                            .get("finishReason")
                            .and_then(|fr| fr.as_str())
                            .unwrap_or("stop");

                        choices.push(json!({
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": combined_text
                            },
                            "finish_reason": finish_reason
                        }));
                    }
                }
            }
            openai_response["choices"] = json!(choices);
        }

        if let Some(usage) = value.get("usageMetadata") {
            openai_response["usage"] = json!({
                "prompt_tokens": usage.get("promptTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                "completion_tokens": usage.get("candidatesTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                "total_tokens": usage.get("totalTokenCount").and_then(|v| v.as_i64()).unwrap_or(0)
            });
        }

        serde_json::to_vec(&openai_response).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct GeminiToOpenAITokenCountTransformer;

impl TokenCountTransformer for GeminiToOpenAITokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "total_tokens": count,
            "prompt_tokens": 0,
            "completion_tokens": count
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_to_openai_request() {
        let transformer = GeminiToOpenAIRequestTransformer;
        let input = json!({
            "model": "gemini-pro",
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "maxOutputTokens": 100
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(output.get("messages").is_some());
        assert!(!output.get("contents").is_some());
        assert_eq!(output["max_tokens"], 100);
    }

    #[test]
    fn test_gemini_to_openai_non_stream_response() {
        let transformer = GeminiToOpenAINonStreamResponseTransformer;
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

        let result = transformer.transform("gpt-4", b"", b"", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(output.get("choices").is_some());
        assert_eq!(output["choices"][0]["message"]["content"], "hello");
        assert_eq!(output["usage"]["total_tokens"], 15);
    }

    #[test]
    fn test_gemini_to_openai_token_count() {
        let transformer = GeminiToOpenAITokenCountTransformer;
        let result = transformer.transform(100);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["total_tokens"], 100);
        assert_eq!(output["prompt_tokens"], 0);
        assert_eq!(output["completion_tokens"], 100);
    }
}
