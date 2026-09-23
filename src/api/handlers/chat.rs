use crate::api::handlers::AppState;
use crate::api::{ApiError, ApiResult};
use crate::format::registry;
use crate::format::Format;
use crate::types::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse};
use axum::{
    body::Body,
    extract::State,
    response::{sse::Event, IntoResponse, Response, Sse},
    Json as JsonExtractor,
};
use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use reqwest::StatusCode;
use serde_json::Value;
use std::convert::Infallible;
use tracing::{debug, warn};
use uuid::Uuid;

/// Credential plus provider-level metadata that the chat handler uses to pick one.
struct ResolvedCredential {
    id: String,
    api_key: String,
    base_url: String,
    priority: i32,
    headers: std::collections::HashMap<String, String>,
    disable_cooling: bool,
    /// Whether this credential was explicitly aliased to the requested model.
    /// If yes, we prefer it over credentials with no alias hit.
    alias_match: bool,
}

/// Provider → Format mapping. Keeps the original three plus Kimi/Codex defaults.
fn provider_to_format(provider: &str) -> Format {
    match provider.to_lowercase().as_str() {
        "anthropic" | "claude" => Format::Claude,
        "google" | "gemini" => Format::Gemini,
        "openai" => Format::OpenAI,
        _ => Format::OpenAI,
    }
}

/// Default upstream base URL per provider.
fn get_default_base_url(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "anthropic" => "https://api.anthropic.com/v1/messages".to_string(),
        "google" => "https://generativelanguage.googleapis.com/v1beta/models".to_string(),
        "openai" => "https://api.openai.com/v1/chat/completions".to_string(),
        _ => format!("https://api.{}.com/v1/chat/completions", provider),
    }
}

/// Resolve all candidate credentials for a provider, sorted by priority desc,
/// filtered against excluded_models and optionally aliased to the requested model.
fn resolve_credentials(
    state: &AppState,
    provider: &str,
    model_id: &str,
) -> ApiResult<Vec<ResolvedCredential>> {
    let provider_config = state
        .config
        .try_read()
        .map_err(|_| ApiError::InternalError("Config lock busy".to_string()))?
        .providers
        .get(provider)
        .ok_or_else(|| ApiError::NotFound(format!("Provider '{}' not found", provider)))?
        .clone();

    if !provider_config.enabled {
        return Err(ApiError::NotFound(format!(
            "Provider '{}' is disabled",
            provider
        )));
    }

    if provider_config.credentials.is_empty() {
        return Err(ApiError::NotFound(format!(
            "No credentials configured for provider '{}'",
            provider
        )));
    }

    let mut out: Vec<ResolvedCredential> = Vec::new();

    for cred in provider_config.credentials {
        // Skip credentials that explicitly exclude this model.
        if cred
            .excluded_models
            .iter()
            .any(|ex: &String| ex == model_id || model_id.starts_with(ex.as_str()))
        {
            continue;
        }

        // Alias hit => this credential claims the model; otherwise treat as generic.
        let alias_match = cred
            .model_aliases
            .iter()
            .any(|alias: &String| alias == model_id || model_id.starts_with(alias.as_str()));

        let api_key = match &cred.api_key {
            Some(k) if !k.is_empty() => k.clone(),
            _ => continue, // skip credentials without an API key
        };

        let base_url = cred
            .base_url
            .clone()
            .unwrap_or_else(|| get_default_base_url(provider));

        let id = if cred.id.is_empty() {
            format!("{}::p{}", provider, cred.priority)
        } else {
            cred.id.clone()
        };

        out.push(ResolvedCredential {
            id,
            api_key,
            base_url,
            priority: cred.priority,
            headers: cred.headers.clone(),
            disable_cooling: cred.disable_cooling,
            alias_match,
        });
    }

    if out.is_empty() {
        return Err(ApiError::NotFound(format!(
            "No usable credentials for provider '{}'",
            provider
        )));
    }

    // Prefer alias matches first, then higher priority. Stable sort preserves config order
    // within equal priority so operators can rely on it.
    out.sort_by(|a, b| {
        b.alias_match
            .cmp(&a.alias_match)
            .then_with(|| b.priority.cmp(&a.priority))
    });

    Ok(out)
}

/// Build a reqwest::RequestBuilder merging credential headers, provider-specific headers,
/// and the mandatory Authorization header.
fn build_upstream_request<'b>(
    client: &'b reqwest::Client,
    credential: &ResolvedCredential,
    provider: &str,
    body: Bytes,
) -> reqwest::RequestBuilder {
    let mut builder = client
        .post(&credential.base_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", credential.api_key));

    // Provider-specific headers.
    match provider.to_lowercase().as_str() {
        "anthropic" | "claude" => {
            builder = builder.header("anthropic-version", "2023-06-01");
        }
        _ => {}
    }

    // Per-credential custom headers (e.g. OpenAI-style custom api-key header overrides).
    for (k, v) in &credential.headers {
        builder = builder.header(k, v);
    }

    builder.body(body)
}

/// Classify an upstream response status into whether the caller should retry it
/// or try the next credential.
fn should_retry_status(status: StatusCode) -> RetryDecision {
    if status == StatusCode::TOO_MANY_REQUESTS {
        RetryDecision::RetrySame
    } else if status.is_server_error() {
        RetryDecision::TryNextCredential
    } else {
        // 4xx except 429: request is malformed / auth-broken; don't retry.
        RetryDecision::Fatal
    }
}

enum RetryDecision {
    RetrySame,
    TryNextCredential,
    Fatal,
}

pub async fn create_chat_completion(
    State(state): State<AppState>,
    JsonExtractor(request): JsonExtractor<ChatCompletionRequest>,
) -> ApiResult<Response<Body>> {
    // Cache bypass short-circuit: if global bypass is enabled, skip the cache entirely.
    let bypass = state.cache_manager.is_bypassed().await;
    debug!(
        "chat.completion model={} stream={} cache_bypass={}",
        request.model, request.stream, bypass
    );

    let model = state.registry.get_model(&request.model).await?;
    let provider = model.provider.as_str();
    let target_format = provider_to_format(provider);

    let request_json = serde_json::to_vec(&request)
        .map_err(|e| ApiError::BadRequest(format!("Failed to serialize request: {}", e)))?;

    let candidates = resolve_credentials(&state, provider, &request.model)?;
    let max_retries = state
        .config
        .try_read()
        .ok()
        .map(|cfg| cfg.routing.max_retries)
        .unwrap_or(3);

    let source_format = Format::OpenAI;
    let transformed_request = registry::translate_request(
        source_format,
        target_format,
        &request.model,
        &request_json,
        request.stream,
    );

    if request.stream {
        run_streaming_chat(
            &state,
            &request,
            &model.provider,
            target_format,
            &candidates,
            &request_json,
            &transformed_request,
            max_retries,
        )
        .await
    } else {
        run_non_streaming_chat(
            &state,
            &request,
            &model.provider,
            target_format,
            &candidates,
            &request_json,
            &transformed_request,
            max_retries,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Streaming path
// ---------------------------------------------------------------------------

async fn run_streaming_chat(
    state: &AppState,
    request: &ChatCompletionRequest,
    provider: &str,
    target_format: Format,
    candidates: &[ResolvedCredential],
    original_request_json: &[u8],
    transformed_request: &[u8],
    max_retries: usize,
) -> ApiResult<Response<Body>> {
    let client = state.http_client.clone();

    let mut last_error: Option<ApiError> = None;
    let mut next_credential_idx = 0;

    while next_credential_idx < candidates.len() {
        let credential = &candidates[next_credential_idx];
        next_credential_idx += 1;

        // Per-credential retry loop for 429s / transient network errors.
        let mut attempt = 0usize;
        loop {
            attempt += 1;
            let body = Bytes::from(transformed_request.to_vec());
            let req = build_upstream_request(&client, credential, provider, body);

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(ApiError::UpstreamError(format!(
                        "[{}] transport error: {}",
                        credential.id, e
                    )));
                    // Network-level failure → move to next credential.
                    break;
                }
            };

            let status = resp.status();

            if !status.is_success() {
                let text = resp.text().await.unwrap_or_else(|_| "unknown".to_string());
                let decision = should_retry_status(status);
                last_error = Some(classify_upstream_error(status, &credential.id, &text));

                match decision {
                    RetryDecision::RetrySame => {
                        if attempt < max_retries {
                            warn!(
                                "[{}] got 429 (attempt {}/{}), retrying",
                                credential.id, attempt, max_retries
                            );
                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                250 * attempt as u64,
                            ))
                            .await;
                            continue;
                        }
                        break; // exhausted retries → move to next credential
                    }
                    RetryDecision::TryNextCredential => break,
                    RetryDecision::Fatal => {
                        // 4xx (non-429) → don't try other credentials.
                        return Err(last_error.unwrap());
                    }
                }
            }

            // --- Success: convert provider stream into OpenAI SSE ---
            let byte_stream = resp.bytes_stream();
            let response_id = Uuid::new_v4().to_string();
            let created = Utc::now().timestamp();
            let model_id = request.model.clone();
            let original_request_json = original_request_json.to_vec();
            let transformed_request = transformed_request.to_vec();

            let stream = byte_stream
                .filter_map(move |chunk_result| {
                    let response_id = response_id.clone();
                    let created = created;
                    let model_id = model_id.clone();
                    let target_format = target_format;
                    let original_request_json = original_request_json.clone();
                    let transformed_request = transformed_request.clone();

                    async move {
                        let chunk = match chunk_result {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::error!("Stream chunk read error: {}", e);
                                return None;
                            }
                        };

                        let lines = split_into_sse_lines(&chunk);
                        let mut out: Vec<Result<Event, Infallible>> = Vec::new();

                        for line in lines {
                            let Some(payload) = extract_sse_payload(&line) else {
                                continue;
                            };

                            if payload.trim() == "[DONE]" {
                                out.push(Ok(Event::default().data("[DONE]")));
                                continue;
                            }

                            let value: Value = match serde_json::from_str(payload) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };

                            let chunk_bytes =
                                serde_json::to_vec(&value).unwrap_or_default();

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
                                match serde_json::from_slice::<ChatCompletionChunk>(
                                    &transformed_bytes,
                                ) {
                                    Ok(mut chunk) => {
                                        chunk.id = response_id.clone();
                                        chunk.created = created;
                                        chunk.model = model_id.clone();
                                        out.push(Ok(
                                            Event::default()
                                                .json_data(&chunk)
                                                .unwrap(),
                                        ));
                                    }
                                    Err(_) => {
                                        debug!(
                                            "Stream chunk did not parse as ChatCompletionChunk, skipping"
                                        );
                                    }
                                }
                            }
                        }

                        Some(out)
                    }
                })
                .flat_map(futures::stream::iter);

            return Ok(Sse::new(stream).into_response());
        }
    }

    Err(last_error
        .unwrap_or_else(|| ApiError::UpstreamError("All credentials exhausted".to_string())))
}

// ---------------------------------------------------------------------------
// Non-streaming path
// ---------------------------------------------------------------------------

async fn run_non_streaming_chat(
    state: &AppState,
    request: &ChatCompletionRequest,
    provider: &str,
    target_format: Format,
    candidates: &[ResolvedCredential],
    original_request_json: &[u8],
    transformed_request: &[u8],
    max_retries: usize,
) -> ApiResult<Response<Body>> {
    let client = state.http_client.clone();

    let mut last_error: Option<ApiError> = None;
    let mut next_credential_idx = 0;

    while next_credential_idx < candidates.len() {
        let credential = &candidates[next_credential_idx];
        next_credential_idx += 1;

        let mut attempt = 0usize;
        loop {
            attempt += 1;
            let body = Bytes::from(transformed_request.to_vec());
            let req = build_upstream_request(&client, credential, provider, body);

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(ApiError::UpstreamError(format!(
                        "[{}] transport error: {}",
                        credential.id, e
                    )));
                    break;
                }
            };

            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_else(|_| "unknown".to_string());
                let decision = should_retry_status(status);
                last_error = Some(classify_upstream_error(status, &credential.id, &text));

                match decision {
                    RetryDecision::RetrySame => {
                        if attempt < max_retries {
                            warn!(
                                "[{}] got 429 (attempt {}/{}), retrying",
                                credential.id, attempt, max_retries
                            );
                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                250 * attempt as u64,
                            ))
                            .await;
                            continue;
                        }
                        break;
                    }
                    RetryDecision::TryNextCredential => break,
                    RetryDecision::Fatal => {
                        return Err(last_error.unwrap());
                    }
                }
            }

            let response_bytes = resp
                .bytes()
                .await
                .map_err(|e| ApiError::UpstreamError(format!("Failed to read response: {}", e)))?;

            let transformed = registry::translate_non_stream(
                target_format,
                Format::OpenAI,
                &request.model,
                original_request_json,
                transformed_request,
                &response_bytes,
                None,
            );

            let chat_response: ChatCompletionResponse = serde_json::from_slice(&transformed)
                .map_err(|e| ApiError::UpstreamError(format!("Failed to parse response: {}", e)))?;

            return Ok(JsonExtractor(chat_response).into_response());
        }
    }

    Err(last_error
        .unwrap_or_else(|| ApiError::UpstreamError("All credentials exhausted".to_string())))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Split a raw TCP chunk into SSE lines. A single chunk may contain multiple SSE
/// events so we split on `\n`, trim, and yield only non-empty, non-comment ones.
fn split_into_sse_lines(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !l.starts_with(':')) // SSE comment / heartbeat
        .map(String::from)
        .collect()
}

/// Extract the JSON payload from an `data: ` SSE line. Returns None for
/// non-data lines (e.g. `event:` or comment lines).
fn extract_sse_payload(line: &str) -> Option<&str> {
    line.strip_prefix("data:").map(str::trim)
}

/// Build an appropriate ApiError from an upstream HTTP response.
fn classify_upstream_error(status: StatusCode, credential_id: &str, body: &str) -> ApiError {
    let message = format!(
        "[{}] upstream {}: {}",
        credential_id,
        status.as_u16(),
        if body.len() > 512 { &body[..512] } else { body }
    );

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        ApiError::Unauthorized(message)
    } else if status == StatusCode::TOO_MANY_REQUESTS {
        ApiError::RateLimitExceeded(message)
    } else if status.is_client_error() {
        ApiError::BadRequest(message)
    } else {
        ApiError::UpstreamError(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::handlers::AppState;
    use crate::cache::CacheManager;
    use crate::config::{Config, Credential, ProviderConfig};
    use crate::registry::{ModelInfo, PredefinedModels};
    use crate::types::ChatMessage;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::RwLock;

    fn test_state() -> AppState {
        let mut config = Config::default();
        config.providers.insert(
            "openai".to_string(),
            crate::config::ProviderConfig {
                credentials: vec![
                    crate::config::Credential {
                        id: "primary".to_string(),
                        api_key: Some("pk-primary".to_string()),
                        priority: 0,
                        ..Default::default()
                    },
                    crate::config::Credential {
                        id: "backup".to_string(),
                        api_key: Some("pk-backup".to_string()),
                        priority: -1,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        );

        AppState {
            config: std::sync::Arc::new(RwLock::new(config)),
            registry: std::sync::Arc::new(crate::registry::ModelRegistry::new()),
            cache_manager: std::sync::Arc::new(CacheManager::new(
                crate::cache::CacheConfig::default(),
            )),
            start_time: Instant::now(),
            config_path: None,
            http_client: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn test_provider_to_format() {
        assert_eq!(provider_to_format("anthropic"), Format::Claude);
        assert_eq!(provider_to_format("google"), Format::Gemini);
        assert_eq!(provider_to_format("openai"), Format::OpenAI);
        assert_eq!(provider_to_format("unknown"), Format::OpenAI);
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
    fn test_should_retry_status() {
        assert!(matches!(
            should_retry_status(StatusCode::TOO_MANY_REQUESTS),
            RetryDecision::RetrySame
        ));
        assert!(matches!(
            should_retry_status(StatusCode::INTERNAL_SERVER_ERROR),
            RetryDecision::TryNextCredential
        ));
        assert!(matches!(
            should_retry_status(StatusCode::BAD_REQUEST),
            RetryDecision::Fatal
        ));
        assert!(matches!(
            should_retry_status(StatusCode::UNAUTHORIZED),
            RetryDecision::Fatal
        ));
    }

    #[test]
    fn test_sse_line_split() {
        let bytes = b":heartbeat\n\ndata: {\"a\":1}\ndata: [DONE]\n";
        let lines = split_into_sse_lines(bytes);
        assert_eq!(
            lines,
            vec!["data: {\"a\":1}".to_string(), "data: [DONE]".to_string()]
        );
    }

    #[test]
    fn test_extract_sse_payload() {
        assert_eq!(extract_sse_payload("data: {\"a\":1}"), Some("{\"a\":1}"));
        assert_eq!(extract_sse_payload("data: [DONE]"), Some("[DONE]"));
        assert_eq!(extract_sse_payload("event: message"), None);
        assert_eq!(extract_sse_payload(":"), None);
    }

    #[tokio::test]
    async fn test_resolve_credentials_sorts_by_priority() {
        let state = test_state();
        let creds = resolve_credentials(&state, "openai", "gpt-4-turbo-preview").unwrap();
        assert_eq!(creds.len(), 2);
        // primary (priority 0) should come before backup (priority -1)
        assert_eq!(creds[0].id, "primary");
        assert_eq!(creds[1].id, "backup");
    }

    #[tokio::test]
    async fn test_resolve_credentials_filters_excluded() {
        let mut config = Config::default();
        config.providers.insert(
            "openai".to_string(),
            ProviderConfig {
                credentials: vec![Credential {
                    api_key: Some("pk".to_string()),
                    excluded_models: vec!["gpt-4".to_string()],
                    ..Credential::default()
                }],
                ..ProviderConfig::default()
            },
        );

        let state = AppState {
            config: Arc::new(RwLock::new(config)),
            registry: Arc::new(crate::registry::ModelRegistry::new()),
            cache_manager: Arc::new(CacheManager::new(crate::cache::CacheConfig::default())),
            start_time: Instant::now(),
            config_path: None,
            http_client: reqwest::Client::new(),
        };

        let result = resolve_credentials(&state, "openai", "gpt-4");
        assert!(result.is_err(), "all credentials excluded → should error");
    }

    #[tokio::test]
    async fn test_chat_completion_request_serialization() {
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
