use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to read {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse terraform JSON: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("unknown resource type {0} (extend ResourceKind to support it)")]
    UnknownResourceType(String),

    #[error("resource {resource} references unknown {field}={target}")]
    MissingReference {
        resource: String,
        field: &'static str,
        target: String,
    },
}
