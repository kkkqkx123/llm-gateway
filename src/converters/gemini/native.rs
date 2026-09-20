use crate::format::types::*;
use serde_json::{json, Value};

pub struct GeminiRequestTransformer;

impl RequestTransformer for GeminiRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], stream: bool) -> Vec<u8> {
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

pub struct GeminiStreamResponseTransformer;

impl StreamResponseTransformer for GeminiStreamResponseTransformer {
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
            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

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

        if chunks.is_empty() {
            chunks.push(raw_json.to_vec());
        }

        chunks
    }
}

pub struct GeminiNonStreamResponseTransformer;

impl NonStreamResponseTransformer for GeminiNonStreamResponseTransformer {
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

pub struct GeminiTokenCountTransformer;

impl TokenCountTransformer for GeminiTokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "usageMetadata": {
                "totalTokenCount": count
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
    fn test_gemini_request_transformer_valid_json() {
        let transformer = GeminiRequestTransformer;
        let input = json!({
            "contents": [{"parts": [{"text": "hello"}]}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gemini-pro", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gemini-pro");
    }

    #[test]
    fn test_gemini_request_transformer_invalid_json() {
        let transformer = GeminiRequestTransformer;
        let raw = b"invalid json";

        let result = transformer.transform("gemini-pro", raw, false);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_gemini_stream_response_transformer() {
        let transformer = GeminiStreamResponseTransformer;
        let raw = b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hello\"}]}}]}\n\n";

        let result = transformer.transform("gemini-pro", b"original", b"request", raw, None);

        assert_eq!(result.len(), 1);
        let output_str = String::from_utf8_lossy(&result[0]);
        assert!(output_str.contains("data: "));
        assert!(output_str.contains("\"model\":\"gemini-pro\""));
    }

    #[test]
    fn test_gemini_stream_response_transformer_empty() {
        let transformer = GeminiStreamResponseTransformer;
        let raw = b"data: {\"candidates\":[]}\n\n";

        let result = transformer.transform("gemini-pro", b"", b"", raw, None);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_gemini_non_stream_response_transformer() {
        let transformer = GeminiNonStreamResponseTransformer;
        let input = json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "hello"}]
                }
            }]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gemini-pro", b"orig", b"req", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gemini-pro");
    }

    #[test]
    fn test_gemini_token_count_transformer() {
        let transformer = GeminiTokenCountTransformer;
        let result = transformer.transform(789);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["usageMetadata"]["totalTokenCount"], 789);
    }
}
