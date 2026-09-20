use crate::format::types::*;
use serde_json::{json, Value};

pub struct OpenAIToOpenAIResponseRequestTransformer;

impl RequestTransformer for OpenAIToOpenAIResponseRequestTransformer {
    fn transform(&self, model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
        let mut value: Value = match serde_json::from_slice(raw_json) {
            Ok(v) => v,
            Err(_) => return raw_json.to_vec(),
        };

        let mut response_request = json!({
            "model": model
        });

        if let Some(obj) = value.as_object() {
            if let Some(messages) = obj.get("messages").and_then(|m| m.as_array()) {
                let input_text = messages
                    .iter()
                    .filter_map(|msg| {
                        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                        if !content.is_empty() {
                            Some(format!("{}: {}", role, content))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                response_request["input"] = json!(input_text);
            }

            if let Some(reasoning_effort) = obj.get("reasoning_effort").and_then(|r| r.as_str()) {
                response_request["reasoning"] = json!({
                    "effort": reasoning_effort
                });
            }

            if let Some(temp) = obj.get("temperature").and_then(|t| t.as_f64()) {
                let verbosity = if temp < 0.3 {
                    "low"
                } else if temp < 0.7 {
                    "medium"
                } else {
                    "high"
                };
                response_request["text"] = json!({
                    "verbosity": verbosity
                });
            }

            if let Some(tools) = obj.get("tools").and_then(|t| t.as_array()) {
                let converted_tools: Vec<Value> = tools
                    .iter()
                    .filter_map(|tool| {
                        if let Some(function) = tool.get("function").and_then(|f| f.as_object()) {
                            Some(json!({
                                "type": "custom",
                                "name": function.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                                "description": function.get("description").and_then(|d| d.as_str()).unwrap_or("")
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !converted_tools.is_empty() {
                    response_request["tools"] = json!(converted_tools);
                }
            }

            if let Some(prev_id) = obj.get("previous_response_id").and_then(|p| p.as_str()) {
                response_request["previous_response_id"] = json!(prev_id);
            }
        }

        serde_json::to_vec(&response_request).unwrap_or_else(|_| raw_json.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_to_openai_response_request() {
        let transformer = OpenAIToOpenAIResponseRequestTransformer;
        let input = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "hello"}
            ],
            "reasoning_effort": "high",
            "temperature": 0.8
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-5.1", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(output["model"], "gpt-5.1");
        assert!(output.get("input").is_some());
        assert!(output.get("reasoning").is_some());
        assert_eq!(output["reasoning"]["effort"], "high");
        assert!(output.get("text").is_some());
        assert_eq!(output["text"]["verbosity"], "high");
    }

    #[test]
    fn test_openai_to_openai_response_with_tools() {
        let transformer = OpenAIToOpenAIResponseRequestTransformer;
        let input = json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Calculate something"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "calculator",
                    "description": "Performs calculations"
                }
            }]
        });
        let raw = serde_json::to_vec(&input).unwrap();

        let result = transformer.transform("gpt-5.1", &raw, false);
        let output: Value = serde_json::from_slice(&result).unwrap();

        assert!(output.get("tools").is_some());
        assert_eq!(output["tools"][0]["type"], "custom");
        assert_eq!(output["tools"][0]["name"], "calculator");
    }
}
