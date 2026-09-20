use crate::logging::types::*;
use chrono::Utc;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Document Request Logger
pub struct FileRequestLogger {
    log_dir: PathBuf,
}

impl FileRequestLogger {
    /// Creating a new file logger
    pub fn new(log_dir: &str) -> Result<Self, LoggerError> {
        let dir = PathBuf::from(log_dir);
        fs::create_dir_all(&dir)?;

        Ok(Self { log_dir: dir })
    }

    /// Generate log file path
    fn generate_log_path(&self, entry: &LogEntry) -> PathBuf {
        let timestamp = entry.timestamp.format("%Y%m%d-%H%M%S");
        let filename = format!(
            "{}-{}.log",
            entry.endpoint.trim_start_matches('/'),
            timestamp
        );

        // If there is an error, use the error- prefix
        let filename = if entry.error.is_some() {
            format!("error-{}", filename)
        } else {
            filename
        };

        self.log_dir.join(filename)
    }

    /// Writing log entries to a file
    fn write_log_entry(&self, entry: &LogEntry) -> Result<(), LoggerError> {
        let log_path = self.generate_log_path(entry);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)?;

        writeln!(file, "=== Log Entry ===")?;
        writeln!(file, "ID: {}", entry.id)?;
        writeln!(file, "Timestamp: {}", entry.timestamp)?;
        writeln!(file, "Endpoint: {}", entry.endpoint)?;
        writeln!(file, "Method: {}", entry.method)?;
        writeln!(file, "URL: {}", entry.url)?;
        writeln!(file, "Status: {:?}", entry.status)?;
        writeln!(file, "Duration: {:?}ms", entry.duration_ms)?;
        writeln!(file, "Transport: {:?}", entry.transport_type)?;

        if let Some(headers) = &entry.headers {
            writeln!(file, "\n=== Headers ===")?;
            for header in headers {
                writeln!(file, "{}: {}", header.name, header.value)?;
            }
        }

        if let Some(request_body) = &entry.request_body {
            writeln!(file, "\n=== Request Body ===")?;
            writeln!(file, "{}", request_body)?;
        }

        if let Some(response_body) = &entry.response_body {
            writeln!(file, "\n=== Response Body ===")?;
            writeln!(file, "{}", response_body)?;
        }

        if let Some(error) = &entry.error {
            writeln!(file, "\n=== Error ===")?;
            writeln!(file, "{}", error)?;
        }

        Ok(())
    }
}

impl RequestLogger for FileRequestLogger {
    fn log_request(&self, request: &LogEntry) -> Result<(), LoggerError> {
        self.write_log_entry(request)
    }

    fn log_response(&self, response: &LogEntry) -> Result<(), LoggerError> {
        self.write_log_entry(response)
    }
}

/// No operation logger (to disable logging)
pub struct NoOpRequestLogger;

impl RequestLogger for NoOpRequestLogger {
    fn log_request(&self, _request: &LogEntry) -> Result<(), LoggerError> {
        Ok(())
    }

    fn log_response(&self, _response: &LogEntry) -> Result<(), LoggerError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_request_logger_creation() {
        let temp_dir = "/tmp/test-logs";
        let logger = FileRequestLogger::new(temp_dir);
        assert!(logger.is_ok());
    }

    #[test]
    fn test_no_op_request_logger() {
        let logger = NoOpRequestLogger;
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

        assert!(logger.log_request(&entry).is_ok());
        assert!(logger.log_response(&entry).is_ok());
    }
}
