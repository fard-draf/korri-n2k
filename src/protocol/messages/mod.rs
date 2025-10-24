//! Dynamically generated module built from PGN definitions.
//! `generated_pgns.rs` is produced at build time and exposes the structures/conversions
//! for every PGN selected in the manifest.
include!(concat!(env!("OUT_DIR"), "/generated_pgns.rs"));
use crate::{
    error::DeserializationError,
    infra::codec::traits::{FieldAccess, PgnData},
};

use crate::core::{FieldDescriptor, FieldKind};
