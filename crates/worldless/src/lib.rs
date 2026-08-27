//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice supports function calls and returns, persistent named
//! scoreboard state and arithmetic, command storage and NBT data operations, number
//! providers, worldless loot predicates, `compute`, `seed`, and value/reset
//! forms of `random`, function macros and tags, supported `execute` conditions and pure
//! context transformations, and result propagation. Supported resources can
//! be compiled from a statically composed, ordered mixture of expanded
//! directory data packs and in-memory packs. Construction is atomic: an invalid
//! selected resource rejects the whole program instead of leaving a partially
//! populated VM.

mod execution_context;
mod loader;
mod macro_function;
mod nbt;
mod number_provider;
mod pack;
mod predicate;
mod program;
mod random;
mod resource;
mod resource_json;
mod runtime;

pub use execution_context::{ExecutionContext, Position, Rotation};
pub use loader::{LoadError, ResourceOrigin};
pub use pack::{MemoryResource, Pack, ResourceKind};
pub use runtime::{ExecutionError, ExecutionOutcome};

use std::{error::Error, fmt};

use nbt::{CommandStorage, CompoundTag};
use program::{Program, Scoreboard};
use random::RandomState;

/// A compound NBT value supplied to a function invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionArguments(CompoundTag);

impl FunctionArguments {
    /// Parses one complete compound SNBT value.
    pub fn from_snbt(input: &str) -> Result<Self, FunctionArgumentsParseError> {
        nbt::parse_compound_fully(input)
            .map(Self)
            .map_err(|reason| FunctionArgumentsParseError { reason })
    }

    pub(crate) fn compound(&self) -> &CompoundTag {
        &self.0
    }
}

/// An error produced while parsing function arguments from SNBT.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionArgumentsParseError {
    reason: String,
}

impl fmt::Display for FunctionArgumentsParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid function arguments: {}", self.reason)
    }
}

impl Error for FunctionArgumentsParseError {}

/// Statically composes and validates the supplied data packs without creating
/// an executable VM.
pub fn validate_packs(packs: impl IntoIterator<Item = Pack>) -> Result<(), LoadError> {
    loader::load_packs(packs).map(drop)
}

/// A loaded worldless data-pack program.
#[derive(Debug)]
pub struct Vm {
    program: Program,
    scoreboard: Scoreboard,
    command_storage: CommandStorage,
    random: RandomState,
}

impl Vm {
    /// Statically composes and compiles the supplied data packs.
    ///
    /// Inputs are ordered from lowest to highest priority. A higher ordinary
    /// resource replaces a lower resource with the same identifier. Tag files
    /// instead compose in pack order and may discard accumulated lower entries
    /// with their `replace` field.
    /// `world_seed` is the VM's immutable configured level seed. It does not
    /// seed the VM's unnamed random stream or represent a loaded world.
    pub fn from_packs(
        packs: impl IntoIterator<Item = Pack>,
        world_seed: i64,
    ) -> Result<Self, LoadError> {
        loader::load_packs(packs).map(|program| Self::new(program, world_seed))
    }

    /// Executes Minecraft's `function` command without a physical world.
    ///
    /// A reference beginning with `#` selects a function tag; other references
    /// select one function. Identifiers without a namespace use `minecraft`.
    /// The same argument compound is supplied to every selected function.
    /// The command source starts with the supplied position and rotation. An
    /// `execute` context transformation applies only to its command chain;
    /// called functions inherit the transformed context, while the caller's
    /// next function line starts from the caller's context.
    ///
    /// The command limit follows Minecraft's queue limit: reaching the limit stops
    /// execution, so a completed invocation always consumes less than
    /// `command_limit`.
    pub fn execute_function(
        &mut self,
        reference: &str,
        arguments: Option<&FunctionArguments>,
        context: ExecutionContext,
        command_limit: usize,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        runtime::execute_function(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            reference,
            arguments.map(FunctionArguments::compound),
            context,
            command_limit,
        )
    }

    /// Executes one supported Minecraft command without a physical world.
    ///
    /// The input may omit its leading `/` or contain exactly one. Parsing and
    /// support validation complete before the command can mutate VM state.
    pub fn execute_command(
        &mut self,
        command: &str,
        context: ExecutionContext,
        command_limit: usize,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        runtime::execute_command(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            command,
            context,
            command_limit,
        )
    }

    fn new(program: Program, world_seed: i64) -> Self {
        Self {
            program,
            scoreboard: Scoreboard::default(),
            command_storage: CommandStorage::default(),
            random: RandomState::new(world_seed),
        }
    }
}
