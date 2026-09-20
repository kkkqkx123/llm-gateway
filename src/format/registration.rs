use crate::converters::claude_to_gemini::*;
use crate::converters::claude_to_openai::*;
use crate::converters::gemini_to_claude::*;
use crate::converters::gemini_to_openai::*;
use crate::converters::openai_response_to_openai::*;
use crate::converters::openai_to_claude::*;
use crate::converters::openai_to_gemini::*;
use crate::converters::openai_to_openai_response::*;
use crate::format::types::*;

pub fn register_default_transformers() {
    use crate::format::registry::register;

    register(
        Format::OpenAI,
        Format::Claude,
        Some(Box::new(OpenAIToClaudeRequestTransformer)),
        None,
    );

    register(
        Format::OpenAI,
        Format::Gemini,
        Some(Box::new(OpenAIToGeminiRequestTransformer)),
        None,
    );

    register(
        Format::OpenAI,
        Format::OpenAIResponse,
        Some(Box::new(OpenAIToOpenAIResponseRequestTransformer)),
        None,
    );

    register(
        Format::OpenAIResponse,
        Format::OpenAI,
        Some(Box::new(OpenAIResponseToOpenAIRequestTransformer)),
        Some(ResponseTransformers {
            stream: Some(Box::new(OpenAIResponseToOpenAIStreamResponseTransformer)),
            non_stream: Some(Box::new(OpenAIResponseToOpenAINonStreamResponseTransformer)),
            token_count: Some(Box::new(OpenAIResponseTokenCountTransformer)),
        }),
    );

    register(
        Format::Claude,
        Format::OpenAI,
        Some(Box::new(ClaudeToOpenAIRequestTransformer)),
        Some(ResponseTransformers {
            stream: Some(Box::new(ClaudeToOpenAIStreamResponseTransformer)),
            non_stream: Some(Box::new(ClaudeToOpenAINonStreamResponseTransformer)),
            token_count: Some(Box::new(ClaudeToOpenAITokenCountTransformer)),
        }),
    );

    register(
        Format::Claude,
        Format::Gemini,
        Some(Box::new(ClaudeToGeminiRequestTransformer)),
        Some(ResponseTransformers {
            stream: Some(Box::new(ClaudeToGeminiStreamResponseTransformer)),
            non_stream: Some(Box::new(ClaudeToGeminiNonStreamResponseTransformer)),
            token_count: Some(Box::new(ClaudeToGeminiTokenCountTransformer)),
        }),
    );

    register(
        Format::Gemini,
        Format::OpenAI,
        Some(Box::new(GeminiToOpenAIRequestTransformer)),
        Some(ResponseTransformers {
            stream: Some(Box::new(GeminiToOpenAIStreamResponseTransformer)),
            non_stream: Some(Box::new(GeminiToOpenAINonStreamResponseTransformer)),
            token_count: Some(Box::new(GeminiToOpenAITokenCountTransformer)),
        }),
    );

    register(
        Format::Gemini,
        Format::Claude,
        Some(Box::new(GeminiToClaudeRequestTransformer)),
        Some(ResponseTransformers {
            stream: Some(Box::new(GeminiToClaudeStreamResponseTransformer)),
            non_stream: Some(Box::new(GeminiToClaudeNonStreamResponseTransformer)),
            token_count: Some(Box::new(GeminiToClaudeTokenCountTransformer)),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::registry::default;

    #[test]
    fn test_register_default_transformers() {
        register_default_transformers();
        let registry = default();

        assert!(registry.has_response_transformer(Format::Claude, Format::OpenAI));
        assert!(registry.has_response_transformer(Format::Gemini, Format::OpenAI));
        assert!(registry.has_response_transformer(Format::Claude, Format::Gemini));
        assert!(registry.has_response_transformer(Format::Gemini, Format::Claude));
        assert!(registry.has_response_transformer(Format::OpenAIResponse, Format::OpenAI));
    }
}
