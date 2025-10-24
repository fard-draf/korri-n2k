//! Error set that can occur while generating code during the build step.
use std::env::VarError;
use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
/// Errors returned by the build script (JSON parsing, code generation, etc.).
pub(crate) enum BuildError {
    /// Inconsistent CANboat definition (invalid field length).
    #[error("[MESSAGE]:Unvalid bitlength for [PGN]:{path}, [COMMENT]:{comment}")]
    BitLengthErr { path: String, comment: &'static str },

    /// Failed to read the `OUT_DIR` environment variable.
    #[error("[MESSAGE]:OUT_DIR error. [ERROR]:{source}")]
    OutDirErr {
        #[source]
        source: VarError,
    },

    /// Failure while parsing a JSON document (manifest or CANboat database).
    #[error("[MESSAGE]:Format JSON invalide [Error]:{0:?}")]
    ParseJson(#[from] serde_json::Error),

    /// Unable to read a file from disk.
    #[error("[MESSAGE]:Failed to read file [PATH]:{path} [ERROR]:{source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Provided path is invalid or missing.
    #[error("[MESSAGE]:Failed to read path. [PATH]:{path}")]
    ReadPath { path: &'static str },

    /// Failed to write the generated code to disk.
    #[error("[MESSAGE]:Failed to write file [PATH]:{path} [ERROR]:{source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Formatting error while writing generated code.
    #[error("[MESSAGE]:Failed to display writeln! macro [ERROR]:{source}")]
    WritelnErr {
        #[from]
        source: std::fmt::Error,
    },

    /// I/O-oriented variant of the previous error.
    #[error("[MESSAGE]:Failed to display writeln! macro [ERROR]:{source}")]
    WritelnIoErr {
        #[from]
        source: io::Error,
    },

    /// Invalid lookup configuration for the specified PGN/field.
    #[allow(dead_code)]
    #[error("[MESSAGE]:Unvalid lookup setup [PGN]:{pgn}, [FIELD]:{field}")]
    UnvalidLookupConfiguration { pgn: u32, field: String },

    /// Download failure for canboat.json from the upstream CANboat repository.
    #[error("[MESSAGE]:Failed to download canboat.json from [URL]:{url} [ERROR]:{message}")]
    DownloadError { url: String, message: String },
}
