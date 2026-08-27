//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice supports function calls and returns, persistent named
//! scoreboard state and arithmetic, command storage and NBT data operations, number
//! providers, worldless loot predicates and `compute`, function macros and
//! tags, supported `execute` conditions and pure context transformations, and
//! result propagation. Supported resources can be compiled from a statically
//! composed, ordered mixture of expanded directory data packs and in-memory
//! packs. Construction is atomic: an invalid selected resource rejects the
//! whole program instead of leaving a partially populated VM.

mod execution_context;
mod loader;
mod macro_function;
mod nbt;
mod number_provider;
mod pack;
mod predicate;
mod program;
mod resource;
mod resource_json;
mod runtime;

pub use execution_context::{ExecutionContext, Position, Rotation};
pub use loader::{LoadError, ResourceOrigin};
pub use pack::{MemoryResource, Pack, ResourceKind};
pub use runtime::{ExecutionError, FunctionOutcome};

use nbt::CommandStorage;
use number_provider::LegacyRandom;
use program::{Program, Scoreboard};

/// A loaded worldless data-pack program.
#[derive(Debug)]
pub struct Vm {
    program: Program,
    scoreboard: Scoreboard,
    command_storage: CommandStorage,
    random: LegacyRandom,
}

impl Vm {
    /// Statically composes and compiles the supplied data packs.
    ///
    /// Inputs are ordered from lowest to highest priority. A higher ordinary
    /// resource replaces a lower resource with the same identifier. Tag files
    /// instead compose in pack order and may discard accumulated lower entries
    /// with their `replace` field.
    pub fn from_packs(packs: impl IntoIterator<Item = Pack>) -> Result<Self, LoadError> {
        loader::load_packs(packs).map(Self::new)
    }

    /// Executes a function without a physical Minecraft world.
    ///
    /// Function identifiers without a namespace use `minecraft`. The command
    /// source starts with the supplied position and rotation. An `execute`
    /// context transformation applies only to its command chain; called
    /// functions inherit the transformed context, while the caller's next
    /// function line starts from the caller's context.
    ///
    /// The command limit follows Minecraft's queue limit: reaching the limit stops
    /// execution, so a completed invocation always consumes less than
    /// `command_limit`. A macro function cannot be invoked directly because
    /// this entry point supplies no argument compound; invoke it from another
    /// function with `function <id> <compound>` or `with storage`.
    pub fn execute_function(
        &mut self,
        id: &str,
        context: ExecutionContext,
        command_limit: usize,
    ) -> Result<FunctionOutcome, ExecutionError> {
        runtime::execute(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            id,
            context,
            command_limit,
        )
    }

    fn new(program: Program) -> Self {
        Self {
            program,
            scoreboard: Scoreboard::default(),
            command_storage: CommandStorage::default(),
            random: LegacyRandom::default(),
        }
    }
}
