use futures::future::FutureExt;
use http::{HeaderMap, Method, StatusCode, Uri};
use std::panic::AssertUnwindSafe;
use std::time::Instant;
use tracing::{error, info, span, warn, Instrument, Level};

/// Request Log Middleware
///
/// Logging request information, response information and elapsed time
pub struct RequestLoggingMiddleware {
    enabled: bool,
    log_body: bool,
    log_headers: bool,
}

impl RequestLoggingMiddleware {
    /// Create new middleware
    pub fn new() -> Self {
        Self {
            enabled: true,
            log_body: false,
            log_headers: false,
        }
    }

    /// Enable request body logging
    pub fn with_body(mut self) -> Self {
        self.log_body = true;
        self
    }

    /// Enable request header logging
    pub fn with_headers(mut self) -> Self {
        self.log_headers = true;
        self
    }

    /// Disable Log
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Implementation request log
    pub async fn call<F, R>(
        &self,
        method: &Method,
        uri: &Uri,
        headers: Option<&HeaderMap>,
        body: Option<&[u8]>,
        next: F,
    ) -> Result<R, MiddlewareError>
    where
        F: std::future::Future<Output = Result<R, MiddlewareError>>,
    {
        if !self.enabled {
            return next.await;
        }

        let start = Instant::now();
        let method_str = method.to_string();
        let uri_str = uri.to_string();

        // Create log span
        let span = span!(
            Level::INFO,
            "request",
            method = %method_str,
            uri = %uri_str,
        );

        async move {
            // Recording request information
            info!("Request: {} {}", method_str, uri_str,);

            // Record request header
            if self.log_headers {
                if let Some(headers) = headers {
                    for (name, value) in headers.iter() {
                        let header_name = name.as_str();
                        let header_value = self.mask_sensitive_header(header_name, value);
                        info!("  Header: {} = {}", header_name, header_value);
                    }
                }
            }

            // Record request body
            if self.log_body {
                if let Some(body) = body {
                    let body_str = String::from_utf8_lossy(body);
                    info!("  Body: {}", body_str);
                }
            }

            // Execute the next processor
            let result = next.await;

            // computational time
            let duration = start.elapsed();
            let duration_ms = duration.as_millis();

            match &result {
                Ok(_) => {
                    info!("Response completed in {}ms", duration_ms);
                }
                Err(e) => {
                    error!("Request failed after {}ms: {}", duration_ms, e);
                }
            }

            result
        }
        .instrument(span)
        .await
    }

    /// Mask Sensitive Request Header
    fn mask_sensitive_header(&self, name: &str, value: &http::HeaderValue) -> String {
        let lower_name = name.to_lowercase();

        if lower_name == "authorization"
            || lower_name == "cookie"
            || lower_name == "x-api-key"
            || lower_name.contains("token")
            || lower_name.contains("secret")
        {
            "***MASKED***".to_string()
        } else {
            value.to_str().unwrap_or("<invalid>").to_string()
        }
    }
}

impl Default for RequestLoggingMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

/// Recovery middleware
///
/// Catch panics and errors, return friendly error responses
pub struct RecoveryMiddleware {
    enabled: bool,
}

impl RecoveryMiddleware {
    /// Create new middleware
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Disable Recovery
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Perform recovery processing
    pub async fn call<F, R>(&self, next: F) -> Result<R, MiddlewareError>
    where
        F: std::future::Future<Output = Result<R, MiddlewareError>>,
    {
        if !self.enabled {
            return next.await;
        }

        tokio::task::unconstrained(async move {
            let result = AssertUnwindSafe(next).catch_unwind().await;

            match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(e)) => Err(e),
                Err(panic_info) => {
                    let panic_msg = if let Some(msg) = panic_info.downcast_ref::<&str>() {
                        msg.to_string()
                    } else if let Some(msg) = panic_info.downcast_ref::<String>() {
                        msg.clone()
                    } else {
                        "Unknown panic".to_string()
                    };

                    error!("Panic caught: {}", panic_msg);

                    Err(MiddlewareError::Panic(panic_msg))
                }
            }
        })
        .await
    }
}

impl Default for RecoveryMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

/// Middleware error
#[derive(Debug, thiserror::Error)]
pub enum MiddlewareError {
    #[error("Panic: {0}")]
    Panic(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),
}

impl From<std::io::Error> for MiddlewareError {
    fn from(err: std::io::Error) -> Self {
        MiddlewareError::Internal(err.to_string())
    }
}

/// Response Log Helper Functions
pub fn log_response(status: StatusCode, duration_ms: u128, response_size: Option<usize>) {
    let level = if status.is_server_error() {
        Level::ERROR
    } else if status.is_client_error() {
        Level::WARN
    } else {
        Level::INFO
    };

    if let Some(size) = response_size {
        match level {
            Level::ERROR => error!(
                status = %status.as_u16(),
                duration_ms,
                size,
                "Response: {} ({}ms, {} bytes)",
                status,
                duration_ms,
                size,
            ),
            Level::WARN => warn!(
                status = %status.as_u16(),
                duration_ms,
                size,
                "Response: {} ({}ms, {} bytes)",
                status,
                duration_ms,
                size,
            ),
            _ => info!(
                status = %status.as_u16(),
                duration_ms,
                size,
                "Response: {} ({}ms, {} bytes)",
                status,
                duration_ms,
                size,
            ),
        }
    } else {
        match level {
            Level::ERROR => error!(
                status = %status.as_u16(),
                duration_ms,
                "Response: {} ({}ms)",
                status,
                duration_ms,
            ),
            Level::WARN => warn!(
                status = %status.as_u16(),
                duration_ms,
                "Response: {} ({}ms)",
                status,
                duration_ms,
            ),
            _ => info!(
                status = %status.as_u16(),
                duration_ms,
                "Response: {} ({}ms)",
                status,
                duration_ms,
            ),
        }
    }
}

/// Request Log Helper Functions
pub fn log_request(method: &Method, uri: &Uri, client_ip: Option<&str>, user_agent: Option<&str>) {
    let method_str = method.to_string();
    let uri_str = uri.to_string();

    info!(
        method = %method_str,
        uri = %uri_str,
        client_ip = client_ip.unwrap_or("unknown"),
        user_agent = user_agent.unwrap_or("unknown"),
        "Request: {} {} from {}",
        method_str,
        uri_str,
        client_ip.unwrap_or("unknown"),
    );
}

/// Error Logging Helper Functions
pub fn log_error(method: &Method, uri: &Uri, error: &MiddlewareError, duration_ms: Option<u128>) {
    let method_str = method.to_string();
    let uri_str = uri.to_string();

    match duration_ms {
        Some(duration) => {
            error!(
                method = %method_str,
                uri = %uri_str,
                error = %error,
                duration_ms,
                "Error handling request {} {}: {} ({}ms)",
                method_str,
                uri_str,
                error,
                duration,
            );
        }
        None => {
            error!(
                method = %method_str,
                uri = %uri_str,
                error = %error,
                "Error handling request {} {}: {}",
                method_str,
                uri_str,
                error,
            );
        }
    }
}

/// Health Screening Log
pub fn log_health_check(status: &str, message: Option<&str>) {
    if let Some(msg) = message {
        info!(status = %status, "Health check: {}", msg);
    } else {
        info!(status = %status, "Health check: {}", status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_logging_middleware_default() {
        let middleware = RequestLoggingMiddleware::default();
        assert!(middleware.enabled);
        assert!(!middleware.log_body);
        assert!(!middleware.log_headers);
    }

    #[test]
    fn test_request_logging_middleware_builder() {
        let middleware = RequestLoggingMiddleware::new()
            .with_body()
            .with_headers()
            .disabled();

        assert!(!middleware.enabled);
    }

    #[test]
    fn test_recovery_middleware_default() {
        let middleware = RecoveryMiddleware::default();
        assert!(middleware.enabled);
    }

    #[test]
    fn test_mask_sensitive_header() {
        let middleware = RequestLoggingMiddleware::default();

        // Testing sensitive request headers
        let auth_value = http::HeaderValue::from_static("Bearer secret-token");
        let masked = middleware.mask_sensitive_header("authorization", &auth_value);
        assert_eq!(masked, "***MASKED***");

        // Testing non-sensitive request headers
        let content_type_value = http::HeaderValue::from_static("application/json");
        let masked = middleware.mask_sensitive_header("content-type", &content_type_value);
        assert_eq!(masked, "application/json");
    }

    #[tokio::test]
    async fn test_request_logging_middleware_call() {
        let middleware = RequestLoggingMiddleware::new();

        let method = Method::GET;
        let uri = Uri::from_static("/api/test");
        let headers = None;
        let body = None;

        async fn handler() -> Result<String, MiddlewareError> {
            Ok("success".to_string())
        }

        let result = middleware
            .call(&method, &uri, headers, body, handler())
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_recovery_middleware_success() {
        let middleware = RecoveryMiddleware::new();

        async fn handler() -> Result<String, MiddlewareError> {
            Ok("success".to_string())
        }

        let result = middleware.call(handler()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_recovery_middleware_error() {
        let middleware = RecoveryMiddleware::new();

        async fn handler() -> Result<String, MiddlewareError> {
            Err(MiddlewareError::Internal("test error".to_string()))
        }

        let result = middleware.call(handler()).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(MiddlewareError::Internal(_))));
    }

    #[tokio::test]
    async fn test_recovery_middleware_panic() {
        let middleware = RecoveryMiddleware::new();

        async fn handler() -> Result<String, MiddlewareError> {
            panic!("test panic");
        }

        let result = middleware.call(handler()).await;

        assert!(result.is_err());
        match result {
            Err(MiddlewareError::Panic(msg)) => {
                assert!(msg.contains("test panic"));
            }
            _ => panic!("Expected Panic error"),
        }
    }
}
