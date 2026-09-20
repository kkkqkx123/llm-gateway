use crate::format::types::*;
use serde_json::{json, Value};

pub struct OpenAIRequestTransformer;

impl RequestTransformer for OpenAIRequestTransformer {
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

pub struct OpenAIStreamResponseTransformer;

impl StreamResponseTransformer for OpenAIStreamResponseTransformer {
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
            if line.is_empty() || line == "data: [DONE]" {
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

pub struct OpenAINonStreamResponseTransformer;

impl NonStreamResponseTransformer for OpenAINonStreamResponseTransformer {
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

pub struct OpenAITokenCountTransformer;

impl TokenCountTransformer for OpenAITokenCountTransformer {
    fn transform(&self, count: i64) -> Vec<u8> {
        json!({
            "total_tokens": count
        })
        .to_string()
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_request_transformer_valid_json() {
        let transformer = OpenAIRequestTransformer;
        let input = json!({
            "messages": [{"role": "user", "content": "hello"}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(!output.get("stream").is_some());
        assert_eq!(output["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_openai_request_transformer_with_stream() {
        let transformer = OpenAIRequestTransformer;
        let input = json!({
            "messages": [{"role": "user", "content": "hello"}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, true);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert_eq!(output["stream"], true);
    }

    #[test]
    fn test_openai_request_transformer_invalid_json() {
        let transformer = OpenAIRequestTransformer;
        let raw = b"invalid json";

        let result = transformer.transform("gpt-4", raw, false);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_openai_request_transformer_existing_model() {
        let transformer = OpenAIRequestTransformer;
        let input = json!({
            "model": "old-model",
            "messages": []
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("new-model", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "new-model");
    }

    #[test]
    fn test_openai_stream_response_transformer() {
        let transformer = OpenAIStreamResponseTransformer;
        let raw = b"data: {\"id\":\"1\",\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n\n";

        let result = transformer.transform("gpt-4", b"original", b"request", raw, None);

        assert_eq!(result.len(), 1);
        let output_str = String::from_utf8_lossy(&result[0]);
        assert!(output_str.contains("data: "));
        assert!(output_str.contains("\"model\":\"gpt-4\""));
    }

    #[test]
    fn test_openai_stream_response_transformer_with_done() {
        let transformer = OpenAIStreamResponseTransformer;
        let raw = b"data: {\"id\":\"1\"}\n\ndata: [DONE]\n\ndata: {\"id\":\"2\"}\n\n";

        let result = transformer.transform("gpt-4", b"", b"", raw, None);

        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_openai_stream_response_transformer_empty_lines() {
        let transformer = OpenAIStreamResponseTransformer;
        let raw = b"\n\ndata: {\"id\":\"1\"}\n\n\n";

        let result = transformer.transform("gpt-4", b"", b"", raw, None);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_openai_stream_response_transformer_invalid_json() {
        let transformer = OpenAIStreamResponseTransformer;
        let raw = b"data: invalid json\n\n";

        let result = transformer.transform("gpt-4", b"", b"", raw, None);

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_openai_non_stream_response_transformer() {
        let transformer = OpenAINonStreamResponseTransformer;
        let input = json!({
            "id": "1",
            "choices": [{"message": {"content": "hello"}}]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", b"orig", b"req", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert_eq!(output["id"], "1");
    }

    #[test]
    fn test_openai_non_stream_response_transformer_invalid_json() {
        let transformer = OpenAINonStreamResponseTransformer;
        let raw = b"invalid json";

        let result = transformer.transform("gpt-4", b"", b"", raw, None);
        assert_eq!(result, raw);
    }

    #[test]
    fn test_openai_token_count_transformer() {
        let transformer = OpenAITokenCountTransformer;
        let result = transformer.transform(123);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["total_tokens"], 123);
    }

    #[test]
    fn test_openai_token_count_transformer_zero() {
        let transformer = OpenAITokenCountTransformer;
        let result = transformer.transform(0);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["total_tokens"], 0);
    }
}
