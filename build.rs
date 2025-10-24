//! Cargo build script: generates PGN structures and lookup tables.

// Re-export the core module from src/core.rs so build_core can reuse it
#[path = "src/core.rs"]
mod core;

mod build_core;
use crate::build_core::{
    conf::*, domain::Manifest, errors::BuildError, gen_lookups::run_lookup_gen,
    gen_pgns::run_pgns_gen,
};

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

// This build script is the core of the code generation pipeline for korri-n2k.
// It reads PGN (Packet Group Number) definitions from JSON files and produces the
// corresponding Rust data structures plus the `ToPayload` (serialization) and
// `FromPayload` (deserialization) trait implementations.
//
// The architecture intentionally separates declarative data definitions (JSON) from their
// runtime manipulation (serialization/deserialization engines in `src/frame/codec/`).
// This script bridges both sides.

//==================================================================================MAIN
fn main() -> Result<(), BuildError> {
    // Tell Cargo to rerun this script whenever one of these files changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build_core/var/pgn_manifest.json");
    println!("cargo:rerun-if-changed=build_core/var/canboat.json");

    // 1. Load the manifest to know which PGNs must be generated.
    // Priority order:
    //   1. KORRI_N2K_MANIFEST_PATH environment variable (absolute or relative path)
    //   2. Default manifest shipped with the crate
    let default_manifest_path =
        PathBuf::from_str(PGN_MANIFEST_PATH).map_err(|_| BuildError::ReadPath {
            path: PGN_MANIFEST_PATH,
        })?;

    let user_manifest_path = std::env::var("KORRI_N2K_MANIFEST_PATH")
        .ok()
        .map(PathBuf::from);

    let manifest_path = if let Some(path) = user_manifest_path {
        if path.exists() {
            println!("cargo:warning=Using custom pgn_manifest.json from {:?}", path);
            println!("cargo:rerun-if-changed={}", path.display());
            path
        } else {
            println!(
                "cargo:warning=Custom manifest path specified but file not found: {:?}",
                path
            );
            println!("cargo:warning=Falling back to the default pgn_manifest");
            default_manifest_path
        }
    } else {
        println!("cargo:warning=Using default pgn_manifest");
        default_manifest_path
    };

    let manifest_string =
        std::fs::read_to_string(&manifest_path).map_err(|e| BuildError::ReadFile {
            path: manifest_path.to_path_buf(),
            source: e,
        })?;
    let manifest: Manifest = serde_json::from_str(&manifest_string)?;
    let pgns_to_generate: Vec<u32> = manifest.pgns.iter().map(|p| p.id).collect();

    // 2. Load the PGN database (download if missing).
    let canboat_doc_path =
        PathBuf::from_str(CANBOAT_DOC_PATH).map_err(|_| BuildError::ReadPath {
            path: CANBOAT_DOC_PATH,
        })?;

    if !canboat_doc_path.exists() {
        println!("cargo:warning=canboat.json not found, downloading from CANboat…");
        download_canboat(&canboat_doc_path)?;
    }
    let canboat_doc_string =
        std::fs::read_to_string(&canboat_doc_path).map_err(|e| BuildError::ReadFile {
            path: canboat_doc_path,
            source: e,
        })?;
    let canboat_value: serde_json::Value = serde_json::from_str(&canboat_doc_string)?;

    // 3. Iterate over the manifest and generate code for every lookup table and requested PGN.
    let buffer_pgn_code: String = run_pgns_gen(&canboat_value, pgns_to_generate)?;
    let buffer_lookup_code = run_lookup_gen(&canboat_value)?;

    // 4. Write the generated code into `OUT_DIR`.
    // The `include!` macro in `src/pgn/generated.rs` will pull it in at compile time.
    let out_dir_str = std::env::var("OUT_DIR").map_err(|e| BuildError::OutDirErr { source: e })?;
    let dest_path = PathBuf::from(out_dir_str);
    let pgn_file_path = dest_path.join(OUT_DIR_PGN_FILE_NAME);
    let lookup_file_path = dest_path.join(OUT_DIR_ENUM_FILE_NAME);

    fs::write(&pgn_file_path, &buffer_pgn_code).map_err(|e| BuildError::WriteFile {
        path: pgn_file_path,
        source: e,
    })?;

    fs::write(&lookup_file_path, &buffer_lookup_code).map_err(|e| BuildError::WriteFile {
        path: lookup_file_path,
        source: (e),
    })?;

    Ok(())
}

/// Download canboat.json from the CANboat repository when missing.
fn download_canboat(dest_path: &PathBuf) -> Result<(), BuildError> {
    const CANBOAT_URL: &str =
        "https://raw.githubusercontent.com/canboat/canboat/master/docs/canboat.json";

    println!("cargo:warning=Downloading canboat.json from {}", CANBOAT_URL);

    // Create the parent directory if required
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| BuildError::WriteFile {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    // Download with ureq (or fall back to curl/wget)
    #[cfg(feature = "build-download")]
    {
        let response = ureq::get(CANBOAT_URL)
            .call()
            .map_err(|e| BuildError::DownloadError {
                url: CANBOAT_URL.to_string(),
                message: e.to_string(),
            })?;

        let mut file = fs::File::create(dest_path).map_err(|e| BuildError::WriteFile {
            path: dest_path.clone(),
            source: e,
        })?;

        std::io::copy(&mut response.into_reader(), &mut file).map_err(|e| {
            BuildError::WriteFile {
                path: dest_path.clone(),
                source: e,
            }
        })?;
    }

    #[cfg(not(feature = "build-download"))]
    {
        // Fall back to curl or wget through a shell command
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                "curl -fsSL {} -o {} || wget -q {} -O {}",
                CANBOAT_URL,
                dest_path.display(),
                CANBOAT_URL,
                dest_path.display()
            ))
            .status()
            .map_err(|e| BuildError::DownloadError {
                url: CANBOAT_URL.to_string(),
                message: format!("Shell command failed: {}", e),
            })?;

        if !status.success() {
            return Err(BuildError::DownloadError {
                url: CANBOAT_URL.to_string(),
                message: "curl and wget both failed. Install one of these tools or enable the 'build-download' feature.".to_string(),
            });
        }
    }

    // Ensure the downloaded file is valid JSON
    let content = fs::read_to_string(dest_path).map_err(|e| BuildError::ReadFile {
        path: dest_path.clone(),
        source: e,
    })?;

    if !content.contains(r#""SchemaVersion""#) {
        fs::remove_file(dest_path).ok();
        return Err(BuildError::DownloadError {
            url: CANBOAT_URL.to_string(),
            message: "The downloaded file is not a valid canboat.json".to_string(),
        });
    }

    println!(
        "cargo:warning=✓ canboat.json downloaded successfully ({} bytes)",
        content.len()
    );

    Ok(())
}
