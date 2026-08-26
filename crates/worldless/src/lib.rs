//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice loads one expanded directory data pack for the repository's
//! target Minecraft version and executes plain functions containing `function`
//! and `return` commands. Loading is atomic: an invalid supported function
//! rejects the whole pack instead of leaving a partially populated VM.

mod loader;
mod program;
mod resource;
mod runtime;

use std::path::Path;

pub use loader::LoadError;
pub use runtime::{ExecutionError, FunctionOutcome};

use program::Program;

/// A loaded worldless data-pack program.
#[derive(Debug)]
pub struct Vm {
    program: Program,
}

impl Vm {
    /// Loads one expanded data pack from `path`.
    pub fn load_directory(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        loader::load_directory(path.as_ref()).map(|program| Self { program })
    }

    /// Executes a function without a physical Minecraft world.
    ///
    /// Function identifiers without a namespace use `minecraft`. The command
    /// limit follows Minecraft's queue limit: reaching the limit stops
    /// execution, so a completed invocation always consumes less than
    /// `command_limit`.
    pub fn execute_function(
        &mut self,
        id: &str,
        command_limit: usize,
    ) -> Result<FunctionOutcome, ExecutionError> {
        runtime::execute(&self.program, id, command_limit)
    }
}
