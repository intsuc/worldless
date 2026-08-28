//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice supports function calls and returns, persistent named
//! scoreboard state and arithmetic, command storage and NBT data operations, number
//! providers, worldless loot predicates, `compute`, `seed`, and value/reset
//! forms of `random`, monotonic stopwatches, function macros and tags, supported
//! `execute` conditions and pure context transformations, caller-driven logical
//! normal ticks, and result propagation.
//! Supported resources can be compiled from a statically composed, ordered
//! mixture of expanded directory data packs and in-memory packs. Construction
//! is atomic: an invalid selected resource rejects the whole program instead of
//! leaving a partially populated VM.

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
mod stopwatch;

pub use execution_context::{ExecutionContext, Position, Rotation};
pub use loader::{LoadError, ResourceOrigin};
pub use pack::{MemoryResource, Pack, ResourceKind};
pub use runtime::{CommandFeedback, ExecutionError, ExecutionOutcome, FeedbackText};

use std::{error::Error, fmt};

use nbt::{CommandStorage, CompoundTag};
use program::{Program, Scoreboard};
use random::RandomState;
use stopwatch::StopwatchState;

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

/// The automatic-function phase in which a logical tick failure occurred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPhase {
    /// The first-tick `minecraft:load` phase.
    Load,
    /// The per-tick `minecraft:tick` phase.
    Tick,
}

impl TickPhase {
    fn function_tag(self) -> &'static str {
        match self {
            Self::Load => "minecraft:load",
            Self::Tick => "minecraft:tick",
        }
    }
}

/// An automatic function whose execution did not complete during a logical tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickFunctionFailure {
    phase: TickPhase,
    function: String,
    error: ExecutionError,
}

impl TickFunctionFailure {
    /// Returns the automatic-function phase that selected the function.
    pub const fn phase(&self) -> TickPhase {
        self.phase
    }

    /// Returns the identifier of the selected function.
    pub fn function(&self) -> &str {
        &self.function
    }

    /// Returns the execution error for this function.
    pub const fn error(&self) -> &ExecutionError {
        &self.error
    }
}

/// Host diagnostics produced while advancing one logical normal tick.
///
/// A failed automatic function does not stop later tag members or phases.
#[must_use = "tick reports contain automatic-function failures"]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickReport {
    failures: Vec<TickFunctionFailure>,
}

impl TickReport {
    /// Returns automatic functions that did not complete, in execution order.
    pub fn failures(&self) -> &[TickFunctionFailure] {
        &self.failures
    }
}

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
    stopwatches: StopwatchState,
    load_pending: bool,
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
    ///
    /// `feedback` receives the invocation's [`CommandFeedback`] events.
    pub fn execute_function(
        &mut self,
        reference: &str,
        arguments: Option<&FunctionArguments>,
        context: ExecutionContext,
        command_limit: usize,
        feedback: impl FnMut(CommandFeedback),
    ) -> Result<ExecutionOutcome, ExecutionError> {
        runtime::execute_function(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            &mut self.stopwatches,
            reference,
            arguments.map(FunctionArguments::compound),
            context,
            command_limit,
            feedback,
        )
    }

    /// Executes one supported Minecraft command without a physical world.
    ///
    /// The input may omit its leading `/` or contain exactly one. Parsing and
    /// support validation complete before the command can mutate VM state.
    ///
    /// `feedback` receives the invocation's [`CommandFeedback`] events.
    pub fn execute_command(
        &mut self,
        command: &str,
        context: ExecutionContext,
        command_limit: usize,
        feedback: impl FnMut(CommandFeedback),
    ) -> Result<ExecutionOutcome, ExecutionError> {
        runtime::execute_command(
            &self.program,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            &mut self.stopwatches,
            command,
            context,
            command_limit,
            feedback,
        )
    }

    /// Advances the VM by one caller-driven logical normal tick.
    ///
    /// The first tick executes every `minecraft:load` member before every
    /// `minecraft:tick` member. Later ticks execute only `minecraft:tick`.
    /// Each member is an independent top-level execution with the supplied
    /// context and command limit. Its command feedback and result are
    /// suppressed, and a failure is recorded without stopping later members.
    pub fn tick(&mut self, context: ExecutionContext, command_limit: usize) -> TickReport {
        let run_load = self.load_pending;
        self.load_pending = false;
        let Self {
            program,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            load_pending: _,
        } = self;
        let mut failures = Vec::new();
        if run_load {
            execute_automatic_tag(
                program,
                scoreboard,
                command_storage,
                random,
                stopwatches,
                TickPhase::Load,
                context,
                command_limit,
                &mut failures,
            );
        }
        execute_automatic_tag(
            program,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            TickPhase::Tick,
            context,
            command_limit,
            &mut failures,
        );
        TickReport { failures }
    }

    fn new(program: Program, world_seed: i64) -> Self {
        Self {
            program,
            scoreboard: Scoreboard::default(),
            command_storage: CommandStorage::default(),
            random: RandomState::new(world_seed),
            stopwatches: StopwatchState::new(),
            load_pending: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_automatic_tag(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    phase: TickPhase,
    context: ExecutionContext,
    command_limit: usize,
    failures: &mut Vec<TickFunctionFailure>,
) {
    let id = resource::Identifier::parse(phase.function_tag())
        .expect("built-in function tag identifiers are valid");
    let Some(functions) = program.function_tag(&id) else {
        return;
    };
    for function in functions {
        if let Err(error) = runtime::execute_automatic_function(
            program,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            function,
            context,
            command_limit,
        ) {
            failures.push(TickFunctionFailure {
                phase,
                function: function.to_string(),
                error,
            });
        }
    }
}
