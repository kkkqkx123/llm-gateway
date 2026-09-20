use crate::format::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::RwLock;

pub struct Registry {
    requests: RwLock<HashMap<Format, HashMap<Format, Box<dyn RequestTransformer>>>>,
    responses: RwLock<HashMap<Format, HashMap<Format, ResponseTransformers>>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry {
            requests: RwLock::new(HashMap::new()),
            responses: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        from: Format,
        to: Format,
        request: Option<Box<dyn RequestTransformer>>,
        response: Option<ResponseTransformers>,
    ) {
        if let Some(req_transformer) = request {
            let mut requests = self.requests.write().unwrap();
            if !requests.contains_key(&from) {
                requests.insert(from.clone(), HashMap::new());
            }
            requests
                .get_mut(&from)
                .unwrap()
                .insert(to.clone(), req_transformer);
        }

        if let Some(resp_transformers) = response {
            let mut responses = self.responses.write().unwrap();
            if !responses.contains_key(&from) {
                responses.insert(from.clone(), HashMap::new());
            }
            responses
                .get_mut(&from)
                .unwrap()
                .insert(to, resp_transformers);
        }
    }

    pub fn translate_request(
        &self,
        from: Format,
        to: Format,
        model: &str,
        raw_json: &[u8],
        stream: bool,
    ) -> Vec<u8> {
        let requests = self.requests.read().unwrap();
        if let Some(by_target) = requests.get(&from) {
            if let Some(transformer) = by_target.get(&to) {
                return transformer.transform(model, raw_json, stream);
            }
        }
        drop(requests);

        if !model.is_empty() {
            if let Ok(mut value) = serde_json::from_slice::<Value>(raw_json) {
                if let Some(obj) = value.as_object_mut() {
                    if let Some(current_model) = obj.get("model").and_then(|v| v.as_str()) {
                        if current_model != model {
                            obj.insert("model".to_string(), Value::String(model.to_string()));
                            if let Ok(updated) = serde_json::to_vec(&value) {
                                return updated;
                            }
                        }
                    }
                }
            }
        }
        raw_json.to_vec()
    }

    pub fn has_response_transformer(&self, from: Format, to: Format) -> bool {
        let responses = self.responses.read().unwrap();
        if let Some(by_target) = responses.get(&from) {
            return by_target.contains_key(&to);
        }
        false
    }

    pub fn translate_stream(
        &self,
        from: Format,
        to: Format,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&Value>,
    ) -> Vec<Vec<u8>> {
        let responses = self.responses.read().unwrap();
        if let Some(by_target) = responses.get(&from) {
            if let Some(transformers) = by_target.get(&to) {
                if let Some(stream_transformer) = &transformers.stream {
                    return stream_transformer.transform(
                        model,
                        original_request_raw_json,
                        request_raw_json,
                        raw_json,
                        param,
                    );
                }
            }
        }
        vec![raw_json.to_vec()]
    }

    pub fn translate_non_stream(
        &self,
        from: Format,
        to: Format,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&Value>,
    ) -> Vec<u8> {
        let responses = self.responses.read().unwrap();
        if let Some(by_target) = responses.get(&from) {
            if let Some(transformers) = by_target.get(&to) {
                if let Some(non_stream_transformer) = &transformers.non_stream {
                    return non_stream_transformer.transform(
                        model,
                        original_request_raw_json,
                        request_raw_json,
                        raw_json,
                        param,
                    );
                }
            }
        }
        raw_json.to_vec()
    }

    pub fn translate_token_count(
        &self,
        from: Format,
        to: Format,
        count: i64,
        raw_json: &[u8],
    ) -> Vec<u8> {
        let responses = self.responses.read().unwrap();
        if let Some(by_target) = responses.get(&from) {
            if let Some(transformers) = by_target.get(&to) {
                if let Some(token_count_transformer) = &transformers.token_count {
                    return token_count_transformer.transform(count);
                }
            }
        }
        raw_json.to_vec()
    }
}

static DEFAULT_REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

pub fn default() -> &'static Registry {
    DEFAULT_REGISTRY.get_or_init(|| Registry::new())
}

pub fn register(
    from: Format,
    to: Format,
    request: Option<Box<dyn RequestTransformer>>,
    response: Option<ResponseTransformers>,
) {
    default().register(from, to, request, response);
}

pub fn translate_request(
    from: Format,
    to: Format,
    model: &str,
    raw_json: &[u8],
    stream: bool,
) -> Vec<u8> {
    default().translate_request(from, to, model, raw_json, stream)
}

pub fn has_response_transformer(from: Format, to: Format) -> bool {
    default().has_response_transformer(from, to)
}

pub fn translate_stream(
    from: Format,
    to: Format,
    model: &str,
    original_request_raw_json: &[u8],
    request_raw_json: &[u8],
    raw_json: &[u8],
    param: Option<&Value>,
) -> Vec<Vec<u8>> {
    default().translate_stream(
        from,
        to,
        model,
        original_request_raw_json,
        request_raw_json,
        raw_json,
        param,
    )
}

pub fn translate_non_stream(
    from: Format,
    to: Format,
    model: &str,
    original_request_raw_json: &[u8],
    request_raw_json: &[u8],
    raw_json: &[u8],
    param: Option<&Value>,
) -> Vec<u8> {
    default().translate_non_stream(
        from,
        to,
        model,
        original_request_raw_json,
        request_raw_json,
        raw_json,
        param,
    )
}

pub fn translate_token_count(from: Format, to: Format, count: i64, raw_json: &[u8]) -> Vec<u8> {
    default().translate_token_count(from, to, count, raw_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct MockRequestTransformer;
    impl RequestTransformer for MockRequestTransformer {
        fn transform(&self, _model: &str, raw_json: &[u8], _stream: bool) -> Vec<u8> {
            let mut result = b"transformed:".to_vec();
            result.extend_from_slice(raw_json);
            result
        }
    }

    struct MockStreamResponseTransformer;
    impl StreamResponseTransformer for MockStreamResponseTransformer {
        fn transform(
            &self,
            _model: &str,
            _original_request_raw_json: &[u8],
            _request_raw_json: &[u8],
            raw_json: &[u8],
            _param: Option<&Value>,
        ) -> Vec<Vec<u8>> {
            vec![b"stream:".to_vec(), raw_json.to_vec()]
        }
    }

    struct MockNonStreamResponseTransformer;
    impl NonStreamResponseTransformer for MockNonStreamResponseTransformer {
        fn transform(
            &self,
            _model: &str,
            _original_request_raw_json: &[u8],
            _request_raw_json: &[u8],
            raw_json: &[u8],
            _param: Option<&Value>,
        ) -> Vec<u8> {
            let mut result = b"non-stream:".to_vec();
            result.extend_from_slice(raw_json);
            result
        }
    }

    struct MockTokenCountTransformer;
    impl TokenCountTransformer for MockTokenCountTransformer {
        fn transform(&self, count: i64) -> Vec<u8> {
            format!("tokens:{}", count).into_bytes()
        }
    }

    #[test]
    fn test_registry_creation() {
        let registry = Registry::new();
        let registry_default = Registry::default();
        assert_eq!(
            registry.requests.read().unwrap().len(),
            registry_default.requests.read().unwrap().len()
        );
    }

    #[test]
    fn test_register_request_transformer() {
        let registry = Registry::new();
        registry.register(
            Format::OpenAI,
            Format::Claude,
            Some(Box::new(MockRequestTransformer)),
            None,
        );
        let result = registry.translate_request(
            Format::OpenAI,
            Format::Claude,
            "gpt-4",
            b"test data",
            false,
        );
        assert_eq!(result, b"transformed:test data");
    }

    #[test]
    fn test_register_response_transformers() {
        let registry = Registry::new();
        let response_transformers = ResponseTransformers {
            stream: Some(Box::new(MockStreamResponseTransformer)),
            non_stream: Some(Box::new(MockNonStreamResponseTransformer)),
            token_count: Some(Box::new(MockTokenCountTransformer)),
        };
        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );
        assert!(registry.has_response_transformer(Format::Claude, Format::OpenAI));
    }

    #[test]
    fn test_translate_stream() {
        let registry = Registry::new();
        let response_transformers = ResponseTransformers {
            stream: Some(Box::new(MockStreamResponseTransformer)),
            non_stream: None,
            token_count: None,
        };
        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );

        let result = registry.translate_stream(
            Format::Claude,
            Format::OpenAI,
            "claude-3",
            b"original",
            b"request",
            b"stream data",
            None,
        );
        assert_eq!(result, vec![b"stream:".to_vec(), b"stream data".to_vec()]);
    }

    #[test]
    fn test_translate_non_stream() {
        let registry = Registry::new();
        let response_transformers = ResponseTransformers {
            stream: None,
            non_stream: Some(Box::new(MockNonStreamResponseTransformer)),
            token_count: None,
        };
        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );

        let result = registry.translate_non_stream(
            Format::Claude,
            Format::OpenAI,
            "claude-3",
            b"original",
            b"request",
            b"response data",
            None,
        );
        assert_eq!(result, b"non-stream:response data");
    }

    #[test]
    fn test_translate_token_count() {
        let registry = Registry::new();
        let response_transformers = ResponseTransformers {
            stream: None,
            non_stream: None,
            token_count: Some(Box::new(MockTokenCountTransformer)),
        };
        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );

        let result =
            registry.translate_token_count(Format::Claude, Format::OpenAI, 123, b"original");
        assert_eq!(result, b"tokens:123");
    }

    #[test]
    fn test_translate_request_no_transformer() {
        let registry = Registry::new();
        let result =
            registry.translate_request(Format::OpenAI, Format::Claude, "gpt-4", b"test", false);
        assert_eq!(result, b"test");
    }

    #[test]
    fn test_translate_stream_no_transformer() {
        let registry = Registry::new();
        let result = registry.translate_stream(
            Format::OpenAI,
            Format::Claude,
            "gpt-4",
            b"orig",
            b"req",
            b"stream data",
            None,
        );
        assert_eq!(result, vec![b"stream data".to_vec()]);
    }

    #[test]
    fn test_translate_non_stream_no_transformer() {
        let registry = Registry::new();
        let result = registry.translate_non_stream(
            Format::OpenAI,
            Format::Claude,
            "gpt-4",
            b"orig",
            b"req",
            b"response data",
            None,
        );
        assert_eq!(result, b"response data");
    }

    #[test]
    fn test_translate_token_count_no_transformer() {
        let registry = Registry::new();
        let result =
            registry.translate_token_count(Format::OpenAI, Format::Claude, 123, b"original");
        assert_eq!(result, b"original");
    }

    #[test]
    fn test_has_response_transformer_false() {
        let registry = Registry::new();
        assert!(!registry.has_response_transformer(Format::OpenAI, Format::Claude));
    }

    #[test]
    fn test_translate_request_with_model_update() {
        let registry = Registry::new();
        let json = json!({
            "model": "old-model",
            "messages": []
        });
        let raw_json = serde_json::to_vec(&json).unwrap();

        let result = registry.translate_request(
            Format::OpenAI,
            Format::Claude,
            "new-model",
            &raw_json,
            false,
        );

        let updated: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(updated["model"], "new-model");
    }

    #[test]
    fn test_default_registry() {
        let default1 = default();
        let default2 = default();
        assert!(std::ptr::eq(default1, default2));
    }
}
