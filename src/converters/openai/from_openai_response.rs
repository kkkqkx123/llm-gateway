use crate::format::types::*;
use serde_json::{json, Value};

pub struct OpenAIResponseToOpenAIRequestTransformer;

impl RequestTransformer for OpenAIResponseToOpenAIRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        let mut chat_request = json!({
            "model": model
        });

        if let Some(obj) = value.as_object() {
            if let Some(input) = obj.get("input") {
                if let Some(input_str) = input.as_str() {
                    chat_request["messages"] = json!([{
                        "role": "user",
                        "content": input_str
                    }]);
                } else if let Some(input_array) = input.as_array() {
                    chat_request["messages"] = json!(input_array);
                }
            }

            if let Some(reasoning) = obj.get("reasoning").and_then(|r| r.as_object()) {
                if let Some(effort) = reasoning.get("effort").and_then(|e| e.as_str()) {
                    chat_request["reasoning_effort"] = json!(effort);
                }
            }

            if let Some(text) = obj.get("text").and_then(|t| t.as_object()) {
                if let Some(verbosity) = text.get("verbosity").and_then(|v| v.as_str()) {
                    let temp = match verbosity {
                        "low" => 0.2,
                        "medium" => 0.5,
                        "high" => 0.8,
                        _ => 0.5,
                    };
                    chat_request["temperature"] = json!(temp);
                }
            }

            if let Some(tools) = obj.get("tools").and_then(|t| t.as_array()) {
                let converted_tools: Vec<Value> = tools
                    .iter()
                    .filter_map(|tool| {
                        if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                            Some(json!({
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "description": tool.get("description").and_then(|d| d.as_str()).unwrap_or("")
                                }
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !converted_tools.is_empty() {
                    chat_request["tools"] = json!(converted_tools);
                }
            }

            if let Some(max_tokens) = obj.get("max_tokens") {
                chat_request["max_tokens"] = max_tokens.clone();
            }

            if _stream {
                chat_request["stream"] = json!(true);
            }

            if let Some(prev_id) = obj.get("previous_response_id").and_then(|p| p.as_str()) {
                chat_request["previous_response_id"] = json!(prev_id);
            }
        }

        serde_json::to_vec(&chat_request).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct OpenAIResponseToOpenAIStreamResponseTransformer;

impl StreamResponseTransformer for OpenAIResponseToOpenAIStreamResponseTransformer {
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

                        if let Some(reasoning) = obj.remove("reasoning") {
                            if let Some(chain_of_thought) =
                                reasoning.get("chain_of_thought").and_then(|c| c.as_str())
                            {
                                let reasoning_chunk = json!({
                                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                                    "object": "chat.completion.chunk",
                                    "created": chrono::Utc::now().timestamp(),
                                    "model": model,
                                    "choices": [{
                                        "index": 0,
                                        "delta": {
                                            "content": format!("[思考] {}", chain_of_thought)
                                        },
                                        "finish_reason": null
                                    }]
                                });

                                if let Ok(bytes) = serde_json::to_vec(&reasoning_chunk) {
                                    chunks.push(
                                        format!("data: {}\n\n", String::from_utf8_lossy(&bytes))
                                            .into_bytes(),
                                    );
                                }
                            }
                        }

                        if let Some(event_type) = obj.get("type").and_then(|t| t.as_str()) {
                            if event_type == "response.output_text.delta" {
                                if let Some(delta) = obj.remove("delta") {
                                    if let Some(delta_str) = delta.as_str() {
                                        let openai_chunk = json!({
                                            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                                            "object": "chat.completion.chunk",
                                            "created": chrono::Utc::now().timestamp(),
                                            "model": model,
                                            "choices": [{
                                                "index": 0,
                                                "delta": {
                                                    "content": delta_str
                                                },
                                                "finish_reason": null
                                            }]
                                        });

                                        if let Ok(bytes) = serde_json::to_vec(&openai_chunk) {
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

                        if let Some(choices) = obj.get_mut("choices").and_then(|c| c.as_array_mut())
                        {
                            for choice in choices {
                                if let Some(choice_obj) = choice.as_object_mut() {
                                    choice_obj.insert("index".to_string(), Value::Number(0.into()));
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

pub struct OpenAIResponseToOpenAINonStreamResponseTransformer;

impl NonStreamResponseTransformer for OpenAIResponseToOpenAINonStreamResponseTransformer {
    fn transform(
        &self,
        model: &str,
        _original_request_raw_json: &[u8],
        _request_raw_json: &[u8],
        raw_json: &[u8],
        _param: Option<&Value>,
    ) -> Vec<u8> {
        let value: Value = match serde_json::from_slice(raw_json) {
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

        if let Some(obj) = value.as_object() {
            if let Some(choices) = obj.get("choices").and_then(|c| c.as_array()) {
                if let Some(first_choice) = choices.first() {
                    let content = first_choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                        .unwrap_or("");

                    openai_response["choices"] = json!([{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": content
                        },
                        "finish_reason": first_choice.get("finish_reason").unwrap_or(&json!("stop"))
                    }]);
                }
            }

            if let Some(usage) = obj.get("usage").and_then(|u| u.as_object()) {
                openai_response["usage"] = json!({
                    "prompt_tokens": usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                    "completion_tokens": usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                    "total_tokens": usage.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
                });
            }

            if let Some(reasoning) = obj.get("reasoning") {
                openai_response["reasoning"] = reasoning.clone();
            }
        }

        serde_json::to_vec(&openai_response).unwrap_or_else(|_| raw_json.to_vec())
    }
}

pub struct OpenAIResponseTokenCountTransformer;

impl TokenCountTransformer for OpenAIResponseTokenCountTransformer {
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
    fn test_openai_response_to_openai_request() {
        let transformer = OpenAIResponseToOpenAIRequestTransformer;
        let input = json!({
            "model": "gpt-5.1",
            "input": "hello world",
            "reasoning": {
                "effort": "medium"
            },
            "text": {
                "verbosity": "low"
            }
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(output.get("messages").is_some());
        assert_eq!(output["messages"][0]["role"], "user");
        assert!(output.get("reasoning_effort").is_some());
        assert_eq!(output["reasoning_effort"], "medium");
        assert!(output.get("temperature").is_some());
        assert_eq!(output["temperature"], 0.2);
    }

    #[test]
    fn test_openai_response_to_openai_with_tools() {
        let transformer = OpenAIResponseToOpenAIRequestTransformer;
        let input = json!({
            "model": "gpt-5.1",
            "input": "Calculate something",
            "tools": [{
                "type": "custom",
                "name": "calculator",
                "description": "Performs calculations"
            }]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert!(output.get("tools").is_some());
        assert_eq!(output["tools"][0]["type"], "function");
        assert_eq!(output["tools"][0]["function"]["name"], "calculator");
    }

    #[test]
    fn test_openai_response_to_openai_non_stream_response() {
        let transformer = OpenAIResponseToOpenAINonStreamResponseTransformer;
        let input = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-4", b"", b"", &raw, None);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-4");
        assert!(output.get("choices").is_some());
        assert_eq!(
            output["choices"][0]["message"]["content"],
            "Hello! How can I help you?"
        );
        assert_eq!(output["usage"]["total_tokens"], 15);
    }

    #[test]
    fn test_openai_response_token_count() {
        let transformer = OpenAIResponseTokenCountTransformer;
        let result = transformer.transform(123);

        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output["total_tokens"], 123);
        assert_eq!(output["prompt_tokens"], 123);
        assert_eq!(output["completion_tokens"], 0);
    }
}
