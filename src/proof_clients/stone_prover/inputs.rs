//! This module provides ways to serialize the inputs of the prover to the file system in the
//! JSON format.

use color_eyre::Result;
use serde::{Serialize, Serializer};
use starknet::core::types::FieldElement;
use std::path::Path;

use crate::proof_clients::{MemoryEntry, MemorySegment, ProofRequest};

use super::{MEMORY_FILE, PRIVATE_INPUT_FILE, PUBLIC_INPUT_FILE, TRACE_FILE};

/// The value part of a memory segment.
#[derive(Serialize)]
struct MemorySegmentAddress {
    pub begin_addr: FieldElement,
    pub stop_ptr: FieldElement,
}

/// Seiralizes a collection of memory segments.
fn serialize_memory_segments<S>(memory_segments: &[MemorySegment], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.collect_map(
        memory_segments.iter().map(|seg| (seg.name, MemorySegmentAddress { begin_addr: seg.start, stop_ptr: seg.end })),
    )
}

#[derive(Serialize)]
struct PublicMemoryEntry {
    address: usize,
    value: Option<FieldElement>,
    page: usize,
}

fn serialize_public_memory<S>(memory: &[MemoryEntry], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.collect_seq(memory.iter().map(|entry| PublicMemoryEntry {
        address: entry.address,
        value: entry.value,
        page: entry.page,
    }))
}

#[derive(Serialize)]
struct PublicInput<'a> {
    pub layout: &'a str,
    pub rc_min: isize,
    pub rc_max: isize,
    pub n_steps: usize,
    #[serde(serialize_with = "serialize_memory_segments")]
    pub memory_segments: &'a [MemorySegment<'a>],
    #[serde(serialize_with = "serialize_public_memory")]
    pub public_memory: &'a [MemoryEntry],
    // FIXME: add dynamic params
}

/// Serializes the provided `req` into an instance of the public input that the stone
/// prover expects.
pub fn serialize_public_input(req: &ProofRequest, buf: &mut Vec<u8>) -> serde_json::Result<()> {
    let val = PublicInput {
        layout: req.layout.name(),
        rc_min: req.rc_min,
        rc_max: req.rc_max,
        n_steps: req.n_steps,
        memory_segments: req.memory_segments,
        public_memory: req.public_memory,
    };

    serde_json::to_writer(buf, &val)
}

#[derive(Serialize)]
struct PrivateInput<'a> {
    pub trace_path: &'a Path,
    pub memory_path: &'a Path,
    // FIXME: add builtins
}

/// Serializes the provided `req` into an instance of the private input that the stone
/// prover expects.
pub fn serialize_private_input(_req: &ProofRequest, buf: &mut Vec<u8>) -> serde_json::Result<()> {
    let val = PrivateInput { trace_path: Path::new(TRACE_FILE), memory_path: Path::new(MEMORY_FILE) };

    serde_json::to_writer(buf, &val)
}

/// Writes the public and private inputs of the provided [`ProofRequest`] to the provided directroy.
///
/// The file names are spcified by the constants defined in the [main module](crate).
pub async fn write_inputs_to_directory(req: &ProofRequest<'_>, dir: &Path) -> Result<()> {
    let mut buf = Vec::new();

    serialize_public_input(req, &mut buf)?;
    tokio::fs::write(dir.join(PUBLIC_INPUT_FILE), &buf).await?;
    buf.clear();
    serialize_private_input(req, &mut buf)?;
    tokio::fs::write(dir.join(PRIVATE_INPUT_FILE), &buf).await?;
    buf.clear();

    tokio::fs::write(dir.join(MEMORY_FILE), &req.memory).await?;
    tokio::fs::write(dir.join(TRACE_FILE), &req.trace).await?;

    Ok(())
}
