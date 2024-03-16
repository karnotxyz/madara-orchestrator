// FIXME: most of the types here do not have any documentation
// In all of the crate that defines them (mainly stone-prover-sdk and cairo-vm, those types
// are not documented at all).
// The stone prover itself doesn't really have any documentation either, and it doesn't seem
// to use the public input or the private input in a meaningful way within the codebase that's
// opensourced.
//
// NOTE: The following types are loosely based on the types expected by the stone prover
// because that's the only prover implementation we have access to right now. If another
// implementation needs to be added, those types should be adapted to the common denominator
// between the different prover implementations.

use std::{fmt, str::FromStr};

use starknet::core::types::FieldElement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    Plain,
    Small,
    Dex,
    Recursive,
    Starknet,
    RecursiveLargeOutput,
    AllCairo,
    AllSolidity,
    StarknetWithKeccak,
}

impl Layout {
    /// Returns the name of the layout as a string.
    ///
    /// The string uses snake case, as it is defined in the original stone prover implementation.
    pub const fn name(self) -> &'static str {
        match self {
            Layout::Plain => "plain",
            Layout::Small => "small",
            Layout::Dex => "dex",
            Layout::Recursive => "recursive",
            Layout::Starknet => "starknet",
            Layout::RecursiveLargeOutput => "recursive_large_output",
            Layout::AllCairo => "all_cairo",
            Layout::AllSolidity => "all_solidity",
            Layout::StarknetWithKeccak => "starknet_with_keccak",
        }
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.name())
    }
}

impl FromStr for Layout {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(Layout::Plain),
            "small" => Ok(Layout::Small),
            "dex" => Ok(Layout::Dex),
            "recursive" => Ok(Layout::Recursive),
            "starknet" => Ok(Layout::Starknet),
            "recursive_large_output" => Ok(Layout::RecursiveLargeOutput),
            "all_cairo" => Ok(Layout::AllCairo),
            "all_solidity" => Ok(Layout::AllSolidity),
            "starknet_with_keccak" => Ok(Layout::StarknetWithKeccak),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemorySegment<'a> {
    pub name: &'a str,
    pub start: FieldElement,
    pub end: FieldElement,
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub address: usize,
    pub value: Option<FieldElement>,
    pub page: usize,
}

/// The request type passed to the [`ProofClient::create_proof`] method.
///
/// This type contains all necessary information that the prover needs to generate a proof.
#[derive(Debug, Clone)]
pub struct ProofRequest<'a> {
    //
    // PUBLIC INPUT
    //
    pub layout: Layout,
    pub rc_min: isize,
    pub rc_max: isize,
    pub n_steps: usize,
    pub memory_segments: &'a [MemorySegment<'a>],
    pub public_memory: &'a [MemoryEntry],
    // FIXME: Find out how `dynamic_params` is supposed to be used.
    // There's a world where it's actually part of the `layout` field if I understood correctly.
    pub dynamic_params: (),
    //
    // PRIVATE INPUT
    //
    pub trace: &'a [u8],
    pub memory: &'a [u8],
    // FIXME: Find out the exact layout of each of those fields. The different implementations
    // I found on github (specifically the cairo-vm and stone-prover-sdk) are not consistent
    // with one another.
    pub pedersen: &'a [()],
    pub range_check: &'a [()],
    pub ecdsa: &'a [()],
    pub bitwise: &'a [()],
    pub ec_ops: &'a [()],
    pub keccak: &'a [()],
    pub poseidon: &'a [()],
}
