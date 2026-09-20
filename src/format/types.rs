use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    OpenAI,
    Claude,
    Gemini,
    Codex,
    Kimi,
}

impl Provider {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "claude" => Provider::Claude,
            "gemini" => Provider::Gemini,
            "codex" => Provider::Codex,
            "kimi" => Provider::Kimi,
            _ => Provider::OpenAI,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Provider::OpenAI => "openai",
            Provider::Claude => "claude",
            Provider::Gemini => "gemini",
            Provider::Codex => "codex",
            Provider::Kimi => "kimi",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Format {
    OpenAI,
    OpenAIResponse,
    Claude,
    Gemini,
    GeminiCLI,
    Codex,
    Antigravity,
}

impl Format {
    pub fn from_str(s: &str) -> Self {
        match s {
            "openai" => Format::OpenAI,
            "openai-response" => Format::OpenAIResponse,
            "claude" => Format::Claude,
            "gemini" => Format::Gemini,
            "gemini-cli" => Format::GeminiCLI,
            "codex" => Format::Codex,
            "antigravity" => Format::Antigravity,
            _ => panic!("Unknown format: {}", s),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Format::OpenAI => "openai",
            Format::OpenAIResponse => "openai-response",
            Format::Claude => "claude",
            Format::Gemini => "gemini",
            Format::GeminiCLI => "gemini-cli",
            Format::Codex => "codex",
            Format::Antigravity => "antigravity",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestEnvelope {
    pub format: Format,
    pub model: String,
    pub stream: bool,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ResponseEnvelope {
    pub format: Format,
    pub model: String,
    pub stream: bool,
    pub body: Vec<u8>,
    pub chunks: Vec<Vec<u8>>,
}

pub trait RequestTransformer: Send + Sync {
    fn transform(&self, model: &str, raw_json: &[u8], stream: bool) -> Vec<u8>;
}

pub trait StreamResponseTransformer: Send + Sync {
    fn transform(
        &self,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&serde_json::Value>,
    ) -> Vec<Vec<u8>>;
}

pub trait NonStreamResponseTransformer: Send + Sync {
    fn transform(
        &self,
        model: &str,
        original_request_raw_json: &[u8],
        request_raw_json: &[u8],
        raw_json: &[u8],
        param: Option<&serde_json::Value>,
    ) -> Vec<u8>;
}

pub trait TokenCountTransformer: Send + Sync {
    fn transform(&self, count: i64) -> Vec<u8>;
}

pub struct ResponseTransformers {
    pub stream: Option<Box<dyn StreamResponseTransformer>>,
    pub non_stream: Option<Box<dyn NonStreamResponseTransformer>>,
    pub token_count: Option<Box<dyn TokenCountTransformer>>,
}

impl Default for ResponseTransformers {
    fn default() -> Self {
        ResponseTransformers {
            stream: None,
            non_stream: None,
            token_count: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_from_str() {
        assert_eq!(Format::from_str("openai"), Format::OpenAI);
        assert_eq!(Format::from_str("openai-response"), Format::OpenAIResponse);
        assert_eq!(Format::from_str("claude"), Format::Claude);
        assert_eq!(Format::from_str("gemini"), Format::Gemini);
        assert_eq!(Format::from_str("gemini-cli"), Format::GeminiCLI);
        assert_eq!(Format::from_str("codex"), Format::Codex);
        assert_eq!(Format::from_str("antigravity"), Format::Antigravity);
    }

    #[test]
    #[should_panic(expected = "Unknown format")]
    fn test_format_from_str_invalid() {
        Format::from_str("invalid");
    }

    #[test]
    fn test_format_as_str() {
        assert_eq!(Format::OpenAI.as_str(), "openai");
        assert_eq!(Format::OpenAIResponse.as_str(), "openai-response");
        assert_eq!(Format::Claude.as_str(), "claude");
        assert_eq!(Format::Gemini.as_str(), "gemini");
        assert_eq!(Format::GeminiCLI.as_str(), "gemini-cli");
        assert_eq!(Format::Codex.as_str(), "codex");
        assert_eq!(Format::Antigravity.as_str(), "antigravity");
    }

    #[test]
    fn test_format_roundtrip() {
        let formats = [
            Format::OpenAI,
            Format::OpenAIResponse,
            Format::Claude,
            Format::Gemini,
            Format::GeminiCLI,
            Format::Codex,
            Format::Antigravity,
        ];
        for format in formats {
            assert_eq!(Format::from_str(format.as_str()), format);
        }
    }

    #[test]
    fn test_format_equality() {
        assert_eq!(Format::OpenAI, Format::OpenAI);
        assert_ne!(Format::OpenAI, Format::Claude);
    }

    #[test]
    fn test_request_envelope_creation() {
        let envelope = RequestEnvelope {
            format: Format::OpenAI,
            model: "gpt-4".to_string(),
            stream: true,
            body: b"test body".to_vec(),
        };
        assert_eq!(envelope.format, Format::OpenAI);
        assert_eq!(envelope.model, "gpt-4");
        assert!(envelope.stream);
        assert_eq!(envelope.body, b"test body");
    }

    #[test]
    fn test_response_envelope_creation() {
        let envelope = ResponseEnvelope {
            format: Format::OpenAI,
            model: "gpt-4".to_string(),
            stream: true,
            body: b"test body".to_vec(),
            chunks: vec![b"chunk1".to_vec(), b"chunk2".to_vec()],
        };
        assert_eq!(envelope.format, Format::OpenAI);
        assert_eq!(envelope.model, "gpt-4");
        assert!(envelope.stream);
        assert_eq!(envelope.body, b"test body");
        assert_eq!(envelope.chunks.len(), 2);
    }

    #[test]
    fn test_response_transformers_default() {
        let rt = ResponseTransformers::default();
        assert!(rt.stream.is_none());
        assert!(rt.non_stream.is_none());
        assert!(rt.token_count.is_none());
    }
}
