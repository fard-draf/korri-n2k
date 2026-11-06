//! Paths and constants used during build-time code generation.
//==================================================================================CONF
/// Manifest containing the list of PGNs to generate.
pub(crate) const PGN_MANIFEST_PATH: &str = "build_core/var/pgn_manifest.json";
/// Manifest listing every PGN the generator currently supports.
/// Selected by the `full-pgns` Cargo feature.
pub(crate) const PGN_MANIFEST_FULL_PATH: &str = "build_core/var/pgn_manifest.full.json";
/// Complete CANboat database (PGNs + metadata).
pub(crate) const CANBOAT_DOC_PATH: &str = "build_core/var/canboat.json";
/// ISO Address Claim: the address management code needs `Pgn60928`, so no manifest may omit it.
pub(crate) const REQUIRED_PGN: u32 = 60928;
/// Generated PGN file name (written to `OUT_DIR`).
pub(crate) const OUT_DIR_PGN_FILE_NAME: &str = "generated_pgns.rs";
/// Generated lookup enumeration file name (written to `OUT_DIR`).
pub(crate) const OUT_DIR_ENUM_FILE_NAME: &str = "generated_lookups.rs";
//==========================================TESTS
// pub(crate) const CANBOAT_DOC_PATH: &str = "_doc/technique/canboat_corrupted.json";
