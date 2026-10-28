use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum FreeFetchError {
    #[error("Network connection failed: {0}")]
    Network(String),

    #[error("Extractor error for {service}: {message}")]
    Extractor { service: String, message: String },

    #[error("HTTP range request rejected by host (Status {status})")]
    HttpRangeNotSupported { status: u16 },

    #[error("Insufficient disk space: required {required_mb} MB, available {available_mb} MB")]
    DiskSpace { required_mb: u64, available_mb: u64 },

    #[error("FFmpeg processing failed with exit code {code}: {log}")]
    RemuxFailed { code: i32, log: String },

    #[error("Database error: {0}")]
    Database(String),

    #[error("Task was cancelled by user")]
    Cancelled,

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("FFmpeg binary not found on system")]
    FfmpegNotFound,

    #[error("Incompatible codec for container {container}: {message}")]
    IncompatibleContainerCodec { container: String, message: String },

    #[error("Item not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Generic(String),
}

impl Serialize for FreeFetchError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        let (type_name, message) = match self {
            FreeFetchError::Network(msg) => ("Network", msg.clone()),
            FreeFetchError::Extractor { service, message } => {
                map.serialize_entry("service", service)?;
                ("Extractor", message.clone())
            }
            FreeFetchError::HttpRangeNotSupported { status } => {
                map.serialize_entry("status", status)?;
                (
                    "HttpRangeNotSupported",
                    format!("HTTP range request rejected by host (Status {status})"),
                )
            }
            FreeFetchError::DiskSpace {
                required_mb,
                available_mb,
            } => {
                map.serialize_entry("required_mb", required_mb)?;
                map.serialize_entry("available_mb", available_mb)?;
                (
                    "DiskSpace",
                    format!(
                        "Insufficient disk space: required {required_mb} MB, available {available_mb} MB"
                    ),
                )
            }
            FreeFetchError::RemuxFailed { code, log } => {
                map.serialize_entry("code", code)?;
                map.serialize_entry("log", log)?;
                (
                    "RemuxFailed",
                    format!("FFmpeg processing failed with exit code {code}: {log}"),
                )
            }
            FreeFetchError::Database(msg) => ("Database", msg.clone()),
            FreeFetchError::Cancelled => ("Cancelled", "Task was cancelled by user".to_string()),
            FreeFetchError::InvalidUrl(msg) => ("InvalidUrl", msg.clone()),
            FreeFetchError::Io(msg) => ("Io", msg.clone()),
            FreeFetchError::FfmpegNotFound => (
                "FfmpegNotFound",
                "FFmpeg binary not found on system".to_string(),
            ),
            FreeFetchError::IncompatibleContainerCodec { container, message } => {
                map.serialize_entry("container", container)?;
                ("IncompatibleContainerCodec", message.clone())
            }
            FreeFetchError::NotFound(msg) => ("NotFound", msg.clone()),
            FreeFetchError::Generic(msg) => ("Generic", msg.clone()),
        };
        map.serialize_entry("type", type_name)?;
        map.serialize_entry("message", &message)?;
        map.end()
    }
}

impl From<reqwest::Error> for FreeFetchError {
    fn from(err: reqwest::Error) -> Self {
        FreeFetchError::Network(err.to_string())
    }
}

impl From<rusqlite::Error> for FreeFetchError {
    fn from(err: rusqlite::Error) -> Self {
        FreeFetchError::Database(err.to_string())
    }
}

impl From<std::io::Error> for FreeFetchError {
    fn from(err: std::io::Error) -> Self {
        FreeFetchError::Io(err.to_string())
    }
}

impl From<url::ParseError> for FreeFetchError {
    fn from(err: url::ParseError) -> Self {
        FreeFetchError::InvalidUrl(err.to_string())
    }
}
