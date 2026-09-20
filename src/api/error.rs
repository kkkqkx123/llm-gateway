use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::fmt;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Unauthorized(String),
    RateLimitExceeded(String),
    UpstreamError(String),
    InternalError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            ApiError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            ApiError::RateLimitExceeded(msg) => write!(f, "Rate Limit Exceeded: {}", msg),
            ApiError::UpstreamError(msg) => write!(f, "Upstream Error: {}", msg),
            ApiError::InternalError(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    pub fn to_response(&self) -> Response {
        let (status, error_type, code) = match self {
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                Some("bad_request"),
            ),
            ApiError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "invalid_request_error",
                Some("not_found"),
            ),
            ApiError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                "authentication_error",
                Some("unauthorized"),
            ),
            ApiError::RateLimitExceeded(msg) => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limit_error",
                Some("rate_limit_exceeded"),
            ),
            ApiError::UpstreamError(msg) => {
                (StatusCode::BAD_GATEWAY, "api_error", Some("upstream_error"))
            }
            ApiError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                Some("internal_error"),
            ),
        };

        let error_detail = ErrorDetail {
            message: self.to_string(),
            error_type: error_type.to_string(),
            param: None,
            code: code.map(|s| s.to_string()),
        };

        let response = ErrorResponse {
            error: error_detail,
        };
        (status, Json(response)).into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.to_response()
    }
}

impl From<crate::registry::RegistryError> for ApiError {
    fn from(err: crate::registry::RegistryError) -> Self {
        match err {
            crate::registry::RegistryError::ModelNotFound => {
                ApiError::NotFound("Model not found".to_string())
            }
            _ => ApiError::InternalError(err.to_string()),
        }
    }
}

impl From<crate::config::ConfigError> for ApiError {
    fn from(err: crate::config::ConfigError) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_response_serialization() {
        let error = ErrorResponse {
            error: ErrorDetail {
                message: "Model not found".to_string(),
                error_type: "invalid_request_error".to_string(),
                param: Some("model".to_string()),
                code: Some("model_not_found".to_string()),
            },
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Model not found"));
        assert!(json.contains("invalid_request_error"));
    }

    #[test]
    fn test_api_error_display() {
        let error = ApiError::NotFound("Model not found".to_string());
        assert_eq!(error.to_string(), "Not Found: Model not found");
    }

    #[test]
    fn test_api_error_to_response() {
        let error = ApiError::NotFound("Model not found".to_string());
        let response = error.to_response();

        let status = response.status();
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
