use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Request logger trait
pub trait RequestLogger: Send + Sync {
    fn log_request(&self, request: &LogEntry) -> Result<(), LoggerError>;
    fn log_response(&self, response: &LogEntry) -> Result<(), LoggerError>;
}

/// Streaming logger trait
pub trait StreamingLogWriter: Send + Sync {
    fn write_status(&mut self, status: &str) -> Result<(), LoggerError>;
    fn write_headers(&mut self, headers: &str) -> Result<(), LoggerError>;
    fn write_request(&mut self, request: &str) -> Result<(), LoggerError>;
    fn write_response(&mut self, response: &str) -> Result<(), LoggerError>;
    fn finish(&mut self) -> Result<(), LoggerError>;
}

/// log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub endpoint: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub headers: Option<Vec<Header>>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub transport_type: TransportType,
}

/// HTTP Header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

/// Transmission type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    Http,
    WebSocket,
}

/// log error
#[derive(Debug, thiserror::Error)]
pub enum LoggerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Logger not initialized")]
    NotInitialized,

    #[error("Invalid log entry: {0}")]
    InvalidEntry(String),
}

/// Sensitive Information Masker
pub struct SensitiveDataMasker;

impl SensitiveDataMasker {
    /// List of headers that require masking
    const SENSITIVE_HEADERS: &'static [&'static str] = &[
        "authorization",
        "cookie",
        "set-cookie",
        "x-api-key",
        "x-auth-token",
        "x-access-token",
    ];

    /// Mask-sensitive header values
    pub fn mask_headers(headers: &mut [Header]) {
        for header in headers {
            if Self::is_sensitive(&header.name) {
                header.value = "***MASKED***".to_string();
            }
        }
    }

    /// Check for head sensitivity
    fn is_sensitive(name: &str) -> bool {
        let name_lower = name.to_lowercase();
        Self::SENSITIVE_HEADERS.iter().any(|&s| s == name_lower)
    }

    /// Masking sensitive information in the request body
    pub fn mask_body(body: &str) -> String {
        // More complex logic can be added here to mask sensitive information in the request body
        // For example, API keys, tokens, etc.
        body.to_string()
    }
}

/// Transmission Type Inferrer
pub struct TransportTypeInferer;

impl TransportTypeInferer {
    /// Inferring transport types from URLs and headers
    pub fn infer(url: &str, headers: &[Header]) -> TransportType {
        let url_lower = url.to_lowercase();

        // Check if the URL contains a WebSocket pattern
        if url_lower.starts_with("ws://") || url_lower.starts_with("wss://") {
            return TransportType::WebSocket;
        }

        // Check the Upgrade field in the header
        for header in headers {
            if header.name.to_lowercase() == "upgrade" && header.value.to_lowercase() == "websocket"
            {
                return TransportType::WebSocket;
            }
        }

        TransportType::Http
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_data_masker() {
        let mut headers = vec![
            Header {
                name: "Authorization".to_string(),
                value: "Bearer secret-token".to_string(),
            },
            Header {
                name: "Content-Type".to_string(),
                value: "application/json".to_string(),
            },
            Header {
                name: "X-API-Key".to_string(),
                value: "secret-key".to_string(),
            },
        ];

        SensitiveDataMasker::mask_headers(&mut headers);

        assert_eq!(headers[0].value, "***MASKED***");
        assert_eq!(headers[1].value, "application/json");
        assert_eq!(headers[2].value, "***MASKED***");
    }

    #[test]
    fn test_transport_type_inferer() {
        let headers = vec![];

        assert_eq!(
            TransportTypeInferer::infer("http://example.com", &headers),
            TransportType::Http
        );

        assert_eq!(
            TransportTypeInferer::infer("ws://example.com", &headers),
            TransportType::WebSocket
        );

        let ws_headers = vec![Header {
            name: "Upgrade".to_string(),
            value: "websocket".to_string(),
        }];

        assert_eq!(
            TransportTypeInferer::infer("http://example.com", &ws_headers),
            TransportType::WebSocket
        );
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry {
            id: "test-id".to_string(),
            timestamp: Utc::now(),
            endpoint: "/api/chat".to_string(),
            method: "POST".to_string(),
            url: "http://example.com/api/chat".to_string(),
            status: Some(200),
            headers: None,
            request_body: None,
            response_body: None,
            duration_ms: Some(100),
            error: None,
            transport_type: TransportType::Http,
        };

        assert_eq!(entry.endpoint, "/api/chat");
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.transport_type, TransportType::Http);
    }
}
