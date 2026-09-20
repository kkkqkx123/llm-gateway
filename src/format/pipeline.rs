use crate::format::types::*;
use crate::format::Registry;
use serde_json::{json, Value};
use std::sync::Arc;

struct TestRequestTransformer;
impl RequestTransformer for TestRequestTransformer {
    fn transform(&self, _model: &str, raw: &[u8], _stream: bool) -> Vec<u8> {
        let mut result = b"converted:".to_vec();
        result.extend_from_slice(raw);
        result
    }
}

struct TestStreamTransformer;
impl StreamResponseTransformer for TestStreamTransformer {
    fn transform(
        &self,
        _model: &str,
        _orig_req: &[u8],
        _req: &[u8],
        raw: &[u8],
        _param: Option<&Value>,
    ) -> Vec<Vec<u8>> {
        vec![b"chunk1:".to_vec(), raw.to_vec()]
    }
}

struct TestNonStreamTransformer;
impl NonStreamResponseTransformer for TestNonStreamTransformer {
    fn transform(
        &self,
        _model: &str,
        _orig_req: &[u8],
        _req: &[u8],
        raw: &[u8],
        _param: Option<&Value>,
    ) -> Vec<u8> {
        let mut result = b"response:".to_vec();
        result.extend_from_slice(raw);
        result
    }
}

pub struct Pipeline {
    registry: Arc<Registry>,
}

impl Pipeline {
    pub fn new(registry: Option<Arc<Registry>>) -> Self {
        let registry = registry.unwrap_or_else(|| Arc::new(Registry::new()));
        Pipeline { registry }
    }

    pub async fn translate_request(
        &self,
        from: Format,
        to: Format,
        req: RequestEnvelope,
    ) -> Result<RequestEnvelope, Box<dyn std::error::Error>> {
        let translated = self
            .registry
            .translate_request(from, to, &req.model, &req.body, req.stream);

        let mut result = req;
        result.body = translated;
        result.format = to;
        Ok(result)
    }

    pub async fn translate_response(
        &self,
        from: Format,
        to: Format,
        resp: ResponseEnvelope,
        original_req: &[u8],
        translated_req: &[u8],
        param: Option<&Value>,
    ) -> Result<ResponseEnvelope, Box<dyn std::error::Error>> {
        let mut result = resp;

        if result.stream {
            result.chunks = self.registry.translate_stream(
                from,
                to,
                &result.model,
                original_req,
                translated_req,
                &result.body,
                param,
            );
        } else {
            result.body = self.registry.translate_non_stream(
                from,
                to,
                &result.model,
                original_req,
                translated_req,
                &result.body,
                param,
            );
        }

        result.format = to;
        Ok(result)
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let pipeline = Pipeline::new(None);
        let default_pipeline = Pipeline::default();
        assert_eq!(Arc::strong_count(&pipeline.registry), 1);
        assert_eq!(Arc::strong_count(&default_pipeline.registry), 1);
    }

    #[test]
    fn test_translate_request() {
        let registry = Arc::new(Registry::new());
        registry.register(
            Format::OpenAI,
            Format::Claude,
            Some(Box::new(TestRequestTransformer)),
            None,
        );

        let pipeline = Pipeline::new(Some(registry));
        let envelope = RequestEnvelope {
            format: Format::OpenAI,
            model: "gpt-4".to_string(),
            stream: false,
            body: b"request".to_vec(),
        };

        tokio_test::block_on(async {
            let result = pipeline
                .translate_request(Format::OpenAI, Format::Claude, envelope)
                .await
                .unwrap();
            assert_eq!(result.format, Format::Claude);
            assert_eq!(result.body, b"converted:request");
        });
    }

    #[test]
    fn test_translate_request_no_transformer() {
        let pipeline = Pipeline::new(None);
        let envelope = RequestEnvelope {
            format: Format::OpenAI,
            model: "gpt-4".to_string(),
            stream: false,
            body: b"request".to_vec(),
        };

        tokio_test::block_on(async {
            let result = pipeline
                .translate_request(Format::OpenAI, Format::Claude, envelope)
                .await
                .unwrap();
            assert_eq!(result.format, Format::Claude);
            assert_eq!(result.body, b"request");
        });
    }

    #[test]
    fn test_translate_response_stream() {
        let registry = Arc::new(Registry::new());

        let response_transformers = ResponseTransformers {
            stream: Some(Box::new(TestStreamTransformer)),
            non_stream: None,
            token_count: None,
        };

        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );

        let pipeline = Pipeline::new(Some(registry));
        let envelope = ResponseEnvelope {
            format: Format::Claude,
            model: "claude-3".to_string(),
            stream: true,
            body: vec![],
            chunks: vec![],
        };

        tokio_test::block_on(async {
            let result = pipeline
                .translate_response(
                    Format::Claude,
                    Format::OpenAI,
                    envelope,
                    b"orig",
                    b"req",
                    None,
                )
                .await
                .unwrap();
            assert_eq!(result.format, Format::OpenAI);
            assert_eq!(result.chunks.len(), 2);
            assert_eq!(result.chunks[0], b"chunk1:");
        });
    }

    #[test]
    fn test_translate_response_non_stream() {
        let registry = Arc::new(Registry::new());

        let response_transformers = ResponseTransformers {
            stream: None,
            non_stream: Some(Box::new(TestNonStreamTransformer)),
            token_count: None,
        };

        registry.register(
            Format::Claude,
            Format::OpenAI,
            None,
            Some(response_transformers),
        );

        let pipeline = Pipeline::new(Some(registry));
        let envelope = ResponseEnvelope {
            format: Format::Claude,
            model: "claude-3".to_string(),
            stream: false,
            body: b"original".to_vec(),
            chunks: vec![],
        };

        tokio_test::block_on(async {
            let result = pipeline
                .translate_response(
                    Format::Claude,
                    Format::OpenAI,
                    envelope,
                    b"orig",
                    b"req",
                    Some(&json!({"key": "value"})),
                )
                .await
                .unwrap();
            assert_eq!(result.format, Format::OpenAI);
            assert_eq!(result.body, b"response:original");
        });
    }

    #[test]
    fn test_translate_response_no_transformer() {
        let pipeline = Pipeline::new(None);
        let envelope = ResponseEnvelope {
            format: Format::Claude,
            model: "claude-3".to_string(),
            stream: false,
            body: b"original".to_vec(),
            chunks: vec![],
        };

        tokio_test::block_on(async {
            let result = pipeline
                .translate_response(
                    Format::Claude,
                    Format::OpenAI,
                    envelope,
                    b"orig",
                    b"req",
                    None,
                )
                .await
                .unwrap();
            assert_eq!(result.format, Format::OpenAI);
            assert_eq!(result.body, b"original");
        });
    }
}
