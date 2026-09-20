use crate::logging::types::*;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// File Streaming Log Writer
pub struct FileStreamingLogWriter {
    log_path: PathBuf,
    writer: BufWriter<File>,
    status_written: bool,
    headers_written: bool,
    request_written: bool,
    response_written: bool,
}

impl FileStreamingLogWriter {
    /// Creating a new streaming log writer
    pub fn new(log_dir: &str, endpoint: &str) -> Result<Self, LoggerError> {
        let dir = PathBuf::from(log_dir);
        fs::create_dir_all(&dir)?;

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        // Replace / in endpoint with - to avoid creating subdirectories
        let endpoint_sanitized = endpoint.trim_start_matches('/').replace('/', "-");
        let filename = format!("{}-{}.log", endpoint_sanitized, timestamp);
        let log_path = dir.join(filename);

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)?;

        Ok(Self {
            log_path,
            writer: BufWriter::new(file),
            status_written: false,
            headers_written: false,
            request_written: false,
            response_written: false,
        })
    }

    /// Write Status Information
    fn write_status_line(&mut self, status: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Status ===")?;
        writeln!(self.writer, "{}", status)?;
        writeln!(self.writer)?;
        self.status_written = true;
        Ok(())
    }

    /// Write header information
    fn write_headers_section(&mut self, headers: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Headers ===")?;
        writeln!(self.writer, "{}", headers)?;
        writeln!(self.writer)?;
        self.headers_written = true;
        Ok(())
    }

    /// Write request body
    fn write_request_section(&mut self, request: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Request ===")?;
        writeln!(self.writer, "{}", request)?;
        writeln!(self.writer)?;
        self.request_written = true;
        Ok(())
    }

    /// Write to the response body
    fn write_response_section(&mut self, response: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Response ===")?;
        writeln!(self.writer, "{}", response)?;
        writeln!(self.writer)?;
        self.response_written = true;
        Ok(())
    }
}

impl StreamingLogWriter for FileStreamingLogWriter {
    fn write_status(&mut self, status: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Status ===")?;
        writeln!(self.writer, "{}", status)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_headers(&mut self, headers: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Headers ===")?;
        writeln!(self.writer, "{}", headers)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_request(&mut self, request: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Request ===")?;
        writeln!(self.writer, "{}", request)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_response(&mut self, response: &str) -> Result<(), LoggerError> {
        writeln!(self.writer, "=== Response ===")?;
        writeln!(self.writer, "{}", response)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), LoggerError> {
        self.writer.flush()?;
        Ok(())
    }
}

impl Drop for FileStreamingLogWriter {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

/// No-operation streaming log writer
pub struct NoOpStreamingLogWriter;

impl StreamingLogWriter for NoOpStreamingLogWriter {
    fn write_status(&mut self, _status: &str) -> Result<(), LoggerError> {
        Ok(())
    }

    fn write_headers(&mut self, _headers: &str) -> Result<(), LoggerError> {
        Ok(())
    }

    fn write_request(&mut self, _request: &str) -> Result<(), LoggerError> {
        Ok(())
    }

    fn write_response(&mut self, _response: &str) -> Result<(), LoggerError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), LoggerError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_streaming_log_writer_creation() {
        let temp_dir = "/tmp/test-logs-streaming";
        std::fs::create_dir_all(temp_dir).unwrap();
        let writer = FileStreamingLogWriter::new(temp_dir, "/api/chat");
        assert!(writer.is_ok());
    }

    #[test]
    fn test_no_op_streaming_log_writer() {
        let mut writer = NoOpStreamingLogWriter;
        assert!(writer.write_status("status").is_ok());
        assert!(writer.write_headers("headers").is_ok());
        assert!(writer.write_request("request").is_ok());
        assert!(writer.write_response("response").is_ok());
        assert!(writer.finish().is_ok());
    }
}
