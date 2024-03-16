use async_trait::async_trait;
use color_eyre::{eyre::OptionExt, Result};
use parking_lot::Mutex;
use slab::Slab;
use std::{ffi::OsString, path::PathBuf, process::Stdio};
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStderr, Command},
};

use crate::jobs::types::JobVerificationStatus;

use self::inputs::write_inputs_to_directory;

use super::{ProofClient, ProofRequest};

mod inputs;

const PROOF_FILE: &str = "proof_file.json";
const PRIVATE_INPUT_FILE: &str = "private_input_file.json";
const PUBLIC_INPUT_FILE: &str = "public_input_file.json";
const PROVER_CONFIG_FILE: &str = "prover_config.json";
const PARAMETER_FILE: &str = "parameter_file.json";
const MEMORY_FILE: &str = "memory_file.bin";
const TRACE_FILE: &str = "trace_file.bin";

/// The configuration passed to [`StoneProver`] to configure its behavior.
#[derive(Debug, Clone)]
pub struct StoneConfig {
    /// The working directory in which the Stone prover will be executed.
    ///
    /// This is requiered because [`StoneProver`] invokes a command in the background, and that
    /// commands takes all of its inputs from the file system.
    ///
    /// The inputs of the command are written to the file system at that location.
    ///
    /// # Remarks
    ///
    /// This directory won't be automatically created, so it must exist prior to using the
    /// prover.
    pub working_directory: PathBuf,
    /// The command that will be spawned every time a proof is requested.
    ///
    /// Note that this is relative to `working_directory` (unless the path is absolute or is a
    /// command).
    pub command: OsString,
}

impl Default for StoneConfig {
    fn default() -> Self {
        Self { working_directory: ".".into(), command: "cpu_air_prover".into() }
    }
}

/// Represents a running instance of the stone prover.
struct StoneInstance {
    /// The running child process.
    child: Child,
}

/// Contains the state required to run the Stone prover in the background and generate proofs with
/// it.
///
/// This type implements the [`ProofClient`] trait.
///
/// # Security concerns
///
/// Because an external process is being spawned and given access to the file system, it is
/// important to make sure that the *correct* command is being run.
///
/// Currently, no `chroot` or other sandboxing mechanism is being used to run the prover, meaning
/// that if a malicious command is used inadvertedly, it could potentially access or modify the
/// entire file system.
///
/// The environment is cleared to make sure that the prover can't access unnescessary information.
///
/// # File system assumptions
///
/// When a [`StoneProver`] is created, it is assumed that the working directory exists and that
/// the prover has the necessary permissions to read and write files in that directory. It is
/// expected that the directory is not removed, renamed, or modified in any way while the prover
/// is running.
///
/// Note that the prover will write files to the working directory, and it is important to make
/// sure that those files are not removed while it is running.
pub struct StoneProver {
    /// The working directory in which the Stone prover will be executed.
    working_directory: PathBuf,
    /// The command that will be spawned every time a proof is requested.
    command: Mutex<Command>,
    running_jobs: Mutex<Slab<StoneInstance>>,
}

impl StoneProver {
    /// Creates a new [`StoneProver`] instance from the provided configuration.
    pub fn new(config: StoneConfig) -> Self {
        let command = make_command(&config);
        let working_directory = config.working_directory;
        Self { working_directory, command: Mutex::new(command), running_jobs: Mutex::new(Slab::new()) }
    }
}

#[async_trait]
impl ProofClient for StoneProver {
    async fn create_proof(&self, request: &ProofRequest<'_>) -> Result<String> {
        write_inputs_to_directory(request, &self.working_directory).await?;

        let child = self.command.lock().spawn()?;
        let _id = self.running_jobs.lock().insert(StoneInstance { child });

        Ok(String::new())
    }

    async fn verify_proof(&self, _id: &str) -> Result<JobVerificationStatus> {
        let id = 0;

        let mut running_jobs = self.running_jobs.lock();
        let job = &mut running_jobs.get_mut(id).take().ok_or_eyre("invalid ID provided to StoneProver")?;

        if let Some(status) = job.child.try_wait()? {
            if status.success() {
                Ok(JobVerificationStatus::Verified)
            } else {
                Ok(JobVerificationStatus::Rejected)
            }
        } else {
            Ok(JobVerificationStatus::Pending)
        }
    }
}

/// Returns the [`Command`] that will be used by [`StoneProver`] to spawn the process
/// responsible for generating proofs.
fn make_command(config: &StoneConfig) -> Command {
    let mut command = Command::new(&config.command);

    command
        .current_dir(&config.working_directory)
        .env_clear() // cleared for security
        .arg("--out_file")
        .arg(PROOF_FILE)
        .arg("--private_input_file")
        .arg(PRIVATE_INPUT_FILE)
        .arg("--public_input_file")
        .arg(PUBLIC_INPUT_FILE)
        .arg("--prover-config-file")
        .arg(PROVER_CONFIG_FILE)
        .arg("--parameter_file")
        .arg(PARAMETER_FILE)
        .stdout(Stdio::null())
        .stdin(Stdio::null())
        .stderr(Stdio::piped()) // stderr needs to be piped to capture error messages
        .kill_on_drop(true); // ensures that any error occuring before the child is waited on kills it

    command
}

/// Gets the error message stored in the standard error stream of a child process.
async fn error_message(mut stderr: ChildStderr) -> String {
    let mut buf = Vec::new();
    match stderr.read_to_end(&mut buf).await {
        Ok(_) => (),
        Err(_) => return "<failed to read error message>".into(),
    }
    match String::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => "<error message is not valid UTF-8>".into(),
    }
}
