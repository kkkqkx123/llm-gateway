use crate::api::handlers::AppState;
use crate::api::{ApiError, ApiResult};
use crate::format::registry;
use crate::format::Format;
use crate::types::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};
use axum::{
    body::Body,
    extract::State,
    http::Response,
    response::{sse::Event, IntoResponse, Json, Sse},
    Json as JsonExtractor,
};
use bytes::Bytes;
use chrono::Utc;
use futures::{Stream, stream::StreamExt};
use reqwest::Client;
use serde_json::Value;
use std::convert::Infallible;
use uuid::Uuid;

/// Provider to format mapping
fn provider_to_format(provider: &str) -> Format {
    match provider.to_lowercase().as_str() {
        "anthropic" => Format::Claude,
        "google" => Format::Gemini,
        "openai" => Format::OpenAI,
        _ => Format::OpenAI,
    }
}

/// Get credential for a provider
fn get_provider_credentials(
    state: &AppState,
    provider: &str,
) -> ApiResult<Option<(String, String)>> {
    let provider_config = state
        .config
        .providers
        .get(provider)
        .ok_or_else(|| ApiError::NotFound(format!("Provider '{}' not found", provider)))?;

    let credentials = provider_config.credentials.first().ok_or_else(|| {
        ApiError::NotFound(format!("No credentials found for provider '{}'", provider))
    })?;

    let base_url = credentials
        .base_url
        .clone()
        .unwrap_or_else(|| get_default_base_url(provider));

    let api_key = credentials.api_key.as_ref().ok_or_else(|| {
        ApiError::NotFound(format!("API key not found for provider '{}'", provider))
    })?;

    Ok(Some((base_url, api_key.clone())))
}

/// Get default base URL for provider
fn get_default_base_url(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "anthropic" => "https://api.anthropic.com/v1/messages".to_string(),
        "google" => "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
        "openai" => "https://api.openai.com/v1/chat/completions".to_string(),
        _ => format!("https://api.{}.com/v1/chat/completions", provider),
    }
}

pub async fn create_chat_completion(
    State(state): State<AppState>,
    JsonExtractor(request): JsonExtractor<ChatCompletionRequest>,
) -> ApiResult<Response<Body>> {
    if !state.registry.model_exists(&request.model).await {
        return Err(ApiError::NotFound(format!(
            "Model '{}' not found",
            request.model
        )));
    }

    let model = state.registry.get_model(&request.model).await?;
    let provider = &model.provider;
    let (base_url, api_key) = get_provider_credentials(&state, provider)?
        .ok_or_else(|| ApiError::NotFound("No valid credentials found".to_string()))?;

    let source_format = Format::OpenAI;
    let target_format = provider_to_format(provider);

    let request_json = serde_json::to_vec(&request)
        .map_err(|e| ApiError::BadRequest(format!("Failed to serialize request: {}", e)))?;

    let transformed_request = registry::translate_request(
        source_format,
        target_format,
        &request.model,
        &request_json,
        request.stream,
    );

    if request.stream {
        let stream = create_streaming_response(
            &state,
            &request,
            &model,
            &base_url,
            &api_key,
            request_json,
            transformed_request,
        )
        .await?;
        Ok(Sse::new(stream).into_response())
    } else {
        let response = create_non_streaming_response(
            &state,
            &request,
            &model,
            &base_url,
            &api_key,
            &request_json,
            &transformed_request,
        )
        .await?;
        Ok(Json(response).into_response())
    }
}

async fn create_streaming_response(
    state: &AppState,
    request: &ChatCompletionRequest,
    model: &crate::registry::ModelInfo,
    base_url: &str,
    api_key: &str,
    original_request_json: Vec<u8>,
    transformed_request: Vec<u8>,
) -> ApiResult<impl Stream<Item = Result<Event, Infallible>>> {
    let client = Client::new();
    let provider = model.provider.clone();
    let model_id = model.id.clone();
    let target_format = provider_to_format(&provider);

    let mut builder = client
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key));

    builder = add_provider_headers(builder, &provider);

    let response = builder
        .body(Bytes::from(transformed_request.clone()))
        .send()
        .await
        .map_err(|e| ApiError::UpstreamError(format!("Failed to send request: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ApiError::UpstreamError(format!(
            "Provider returned error: {} - {}",
            status, error_text
        )));
    }

    let byte_stream = response.bytes_stream();
    let response_id = Uuid::new_v4().to_string();
    let created = Utc::now().timestamp();

    let stream =
        byte_stream.filter_map(move |chunk| {
            let response_id = response_id.clone();
            let created = created;
            let model_id = model_id.clone();
            let target_format = target_format;
            let original_request_json = original_request_json.clone();
            let transformed_request = transformed_request.clone();

            async move {
                match chunk {
                    Ok(bytes) => {
                        let data = String::from_utf8_lossy(&bytes);

                        for line in data.lines() {
                            let line = line.trim();

                            if line.is_empty() || line == "data: [DONE]" {
                                continue;
                            }

                            if let Some(json_str) = line.strip_prefix("data: ") {
                                if let Ok(chunk_data) = serde_json::from_str::<Value>(json_str) {
                                    let chunk_bytes =
                                        serde_json::to_vec(&chunk_data).unwrap_or_default();

                                    let transformed = registry::translate_stream(
                                        target_format,
                                        Format::OpenAI,
                                        &model_id,
                                        &original_request_json,
                                        &transformed_request,
                                        &chunk_bytes,
                                        None,
                                    );

                                    for transformed_bytes in transformed {
                                        if let Ok(chunk) =
                                            serde_json::from_slice::<ChatCompletionChunk>(
                                                &transformed_bytes,
                                            )
                                        {
                                            return Some(Ok(Event::default()
                                                .json_data(chunk)
                                                .unwrap()));
                                        }
                                    }
                                }
                            }
                        }
                        None
                    }
                    Err(e) => {
                        tracing::error!("Stream error: {}", e);
                        None
                    }
                }
            }
        });

    Ok(stream)
}

async fn create_non_streaming_response(
    _state: &AppState,
    request: &ChatCompletionRequest,
    model: &crate::registry::ModelInfo,
    base_url: &str,
    api_key: &str,
    original_request_json: &[u8],
    transformed_request: &[u8],
) -> ApiResult<ChatCompletionResponse> {
    let client = Client::new();
    let provider = &model.provider;
    let target_format = provider_to_format(provider);

    let mut builder = client
        .post(base_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key));

    builder = add_provider_headers(builder, provider);

    let response = builder
        .body(Bytes::from(transformed_request.to_vec()))
        .send()
        .await
        .map_err(|e| ApiError::UpstreamError(format!("Failed to send request: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(ApiError::UpstreamError(format!(
            "Provider returned error: {} - {}",
            status, error_text
        )));
    }

    let response_bytes = response
        .bytes()
        .await
        .map_err(|e| ApiError::UpstreamError(format!("Failed to read response: {}", e)))?;

    let transformed_response = registry::translate_non_stream(
        target_format,
        Format::OpenAI,
        &model.id,
        original_request_json,
        transformed_request,
        &response_bytes,
        None,
    );

    serde_json::from_slice(&transformed_response)
        .map_err(|e| ApiError::UpstreamError(format!("Failed to parse response: {}", e)))
}

/// Add provider-specific headers
fn add_provider_headers(
    builder: reqwest::RequestBuilder,
    provider: &str,
) -> reqwest::RequestBuilder {
    match provider.to_lowercase().as_str() {
        "anthropic" => builder.header("anthropic-version", "2023-06-01"),
        "google" => builder,
        _ => builder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    #[test]
    fn test_provider_to_format() {
        assert_eq!(provider_to_format("anthropic"), Format::Claude);
        assert_eq!(provider_to_format("google"), Format::Gemini);
        assert_eq!(provider_to_format("openai"), Format::OpenAI);
    }

    #[test]
    fn test_get_default_base_url() {
        assert_eq!(
            get_default_base_url("anthropic"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            get_default_base_url("google"),
            "https://generativelanguage.googleapis.com/v1beta/models"
        );
        assert_eq!(
            get_default_base_url("openai"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_chat_completion_request_serialization() {
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            stream: false,
            temperature: Some(0.7),
            max_tokens: Some(1000),
            top_p: None,
            n: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello"));
        assert!(json.contains("0.7"));
    }
}
