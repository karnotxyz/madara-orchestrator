use async_trait::async_trait;
use color_eyre::Result;

use crate::jobs::types::JobVerificationStatus;

mod model;
mod stone_prover;

pub use self::model::*;
pub use self::stone_prover::*;

/// Describes the functionalities required by a proof client.
#[async_trait]
pub trait ProofClient {
    /// Requests creation of a proof using the execution information stored in the
    /// provided [`ProofRequest`].
    ///
    /// # Returns
    ///
    /// This function returns an ID that can be used to track the status of the job
    /// and retrieve the proof once it is ready.
    async fn create_proof(&self, req: &ProofRequest<'_>) -> Result<String>;

    /// Fetches the state of the proof for the job with the given `external_id`.
    ///
    /// # Returns
    ///
    /// This function fetches the state of the proof job and returns it as a
    /// [`JobVerificationStatus`].
    async fn verify_proof(&self, external_id: &str) -> Result<JobVerificationStatus>;
}
