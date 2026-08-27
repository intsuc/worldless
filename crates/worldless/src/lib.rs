//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice supports function calls and returns, persistent named
//! scoreboard arithmetic, command storage and NBT data operations, function
//! macros and tags, supported `execute` conditions, and result propagation.
//! Functions and tags can be compiled from an expanded directory data pack or
//! in-memory source. Construction is atomic: an invalid supported resource
//! rejects the whole program instead of leaving a partially populated VM.

mod loader;
mod macro_function;
mod nbt;
mod program;
mod resource;
mod runtime;

use std::path::Path;

pub use loader::{CompileError, LoadError};
pub use runtime::{ExecutionError, FunctionOutcome};

use nbt::CommandStorage;
use program::{Program, Scoreboard};

/// A loaded worldless data-pack program.
#[derive(Debug)]
pub struct Vm {
    program: Program,
    scoreboard: Scoreboard,
    command_storage: CommandStorage,
}

impl Vm {
    /// Compiles functions without reading a data pack from the file system.
    ///
    /// Each item is a function identifier and its source. Identifiers without a
    /// namespace use `minecraft`; duplicate identifiers after that normalization
    /// are rejected. This entry point does not process pack metadata or resource
    /// paths.
    pub fn from_functions<I, N, S>(functions: I) -> Result<Self, CompileError>
    where
        I: IntoIterator<Item = (N, S)>,
        N: AsRef<str>,
        S: AsRef<str>,
    {
        loader::compile_functions(functions).map(Self::new)
    }

    /// Compiles functions and function-tag JSON without reading a data pack
    /// from the file system.
    ///
    /// Function and tag identifiers without a namespace use `minecraft`.
    /// Duplicate identifiers after that normalization are rejected within each
    /// resource type. Tag sources use the same JSON representation as files in
    /// `data/<namespace>/tags/function`.
    pub fn from_functions_and_tags<FI, FN, FS, TI, TN, TS>(
        functions: FI,
        function_tags: TI,
    ) -> Result<Self, CompileError>
    where
        FI: IntoIterator<Item = (FN, FS)>,
        FN: AsRef<str>,
        FS: AsRef<str>,
        TI: IntoIterator<Item = (TN, TS)>,
        TN: AsRef<str>,
        TS: AsRef<str>,
    {
        loader::compile_functions_and_tags(functions, function_tags).map(Self::new)
    }

    /// Loads one expanded data pack from `path`.
    pub fn load_directory(path: impl AsRef<Path>) -> Result<Self, LoadError> {
        loader::load_directory(path.as_ref()).map(Self::new)
    }

    /// Executes a function without a physical Minecraft world.
    ///
    /// Function identifiers without a namespace use `minecraft`. The command
    /// limit follows Minecraft's queue limit: reaching the limit stops
    /// execution, so a completed invocation always consumes less than
    /// `command_limit`. A macro function cannot be invoked directly because
    /// this entry point supplies no argument compound; invoke it from another
    /// function with `function <id> <compound>` or `with storage`.
    pub fn execute_function(
        &mut self,
        id: &str,
        command_limit: usize,
    ) -> Result<FunctionOutcome, ExecutionError> {
        runtime::execute(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            id,
            command_limit,
        )
    }

    fn new(program: Program) -> Self {
        Self {
            program,
            scoreboard: Scoreboard::default(),
            command_storage: CommandStorage::default(),
        }
    }
}
