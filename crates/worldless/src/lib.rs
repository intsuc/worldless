//! A worldless execution engine for the computation-only subset of Minecraft
//! data packs.
//!
//! The current slice supports function calls and returns, persistent named
//! scoreboard state and arithmetic, command storage and NBT data operations, number
//! providers, worldless loot predicates, `compute`, `seed`, and value/reset
//! forms of `random`, monotonic stopwatches, function macros and tags, supported
//! `execute` conditions and pure context transformations, caller-driven logical
//! normal ticks and function scheduling, and result propagation.
//! Supported resources can be compiled from a statically composed, ordered
//! mixture of expanded directory data packs and in-memory packs. Construction
//! is atomic: an invalid selected resource rejects the whole program instead of
//! leaving a partially populated VM.

mod command_storage_file;
mod execution_context;
mod java_math;
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
mod schedule;
mod stopwatch;

pub use command_storage_file::CommandStorageLoadError;
pub use execution_context::{ExecutionContext, Position, Rotation};
pub use loader::{LoadError, ResourceOrigin};
pub use nbt::{CompoundTag, CompoundTagParseError};
pub use pack::{MemoryResource, Pack, ResourceKind};
pub use runtime::{
    CommandFeedback, ExecutionError, ExecutionOutcome, ExecutionReport, FeedbackText,
};

use std::{error::Error, fmt, path::Path, sync::Arc};

use macro_function::MacroCacheState;
use nbt::CommandStorage;
use program::{Program, Scoreboard};
use random::RandomState;
use resource::{FunctionReference, Identifier};
use schedule::ScheduleState;
use stopwatch::StopwatchState;

/// An invalid command-storage identifier supplied to the host API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageIdParseError {
    input: String,
}

impl StorageIdParseError {
    /// Returns the invalid identifier text.
    pub fn input(&self) -> &str {
        &self.input
    }
}

impl fmt::Display for StorageIdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid storage identifier {:?}", self.input)
    }
}

impl Error for StorageIdParseError {}

/// A statically composed and compiled worldless data-pack program.
#[derive(Clone, Debug)]
pub struct CompiledProgram {
    program: Arc<Program>,
}

impl CompiledProgram {
    /// Statically composes and compiles the supplied data packs.
    ///
    /// Inputs are ordered from lowest to highest priority. A higher ordinary
    /// resource replaces a lower resource with the same identifier. Tag files
    /// instead compose in pack order and may discard accumulated lower entries
    /// with their `replace` field.
    pub fn from_packs(packs: impl IntoIterator<Item = Pack>) -> Result<Self, LoadError> {
        loader::load_packs(packs).map(|program| Self {
            program: Arc::new(program),
        })
    }

    /// Creates a VM with fresh logical state and a VM-local macro cache.
    ///
    /// `world_seed` is the VM's immutable configured level seed. It does not
    /// seed the unnamed random stream or represent a loaded world.
    pub fn create_vm(&self, world_seed: i64) -> Vm {
        Vm::new(Arc::clone(&self.program), world_seed)
    }
}

/// The logical-tick phase in which an automatic function failure occurred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickPhase {
    /// The first-tick `minecraft:load` phase.
    Load,
    /// The per-tick `minecraft:tick` phase.
    Tick,
    /// A function selected by a due `schedule` callback.
    Scheduled,
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
/// A failed automatic or scheduled function does not stop later functions or phases.
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

/// One executable instance of a compiled worldless data-pack program.
#[derive(Debug)]
pub struct Vm {
    program: Arc<Program>,
    macro_cache: MacroCacheState,
    scoreboard: Scoreboard,
    command_storage: CommandStorage,
    random: RandomState,
    stopwatches: StopwatchState,
    schedules: ScheduleState,
    load_pending: bool,
}

impl Vm {
    /// Atomically replaces command-storage namespaces from Minecraft storage files.
    ///
    /// Each tuple supplies the namespace owned by one file and its path. All
    /// namespaces and files are validated before any VM state changes. A loaded
    /// namespace replaces every existing storage in that namespace; namespaces
    /// not named by `files` are preserved.
    pub fn load_command_storage_files<N, P>(
        &mut self,
        files: impl IntoIterator<Item = (N, P)>,
    ) -> Result<(), CommandStorageLoadError>
    where
        N: AsRef<str>,
        P: AsRef<Path>,
    {
        let loaded = command_storage_file::load(files)?;
        for namespace in loaded {
            self.command_storage
                .replace_namespace(&namespace.namespace, namespace.values);
        }
        Ok(())
    }

    /// Returns the exact compound stored under `id`, or `None` when it is absent.
    pub fn storage(&self, id: &str) -> Result<Option<&CompoundTag>, StorageIdParseError> {
        let id = parse_storage_id(id)?;
        Ok(self.command_storage.get_ref(&id))
    }

    /// Replaces the whole compound stored under `id` without executing a command.
    ///
    /// An empty compound removes the storage, matching the VM's command-storage
    /// invariant.
    pub fn set_storage(&mut self, id: &str, value: CompoundTag) -> Result<(), StorageIdParseError> {
        let id = parse_storage_id(id)?;
        self.command_storage.set(id, value);
        Ok(())
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
        arguments: Option<&CompoundTag>,
        context: ExecutionContext,
        command_limit: usize,
        feedback: impl FnMut(CommandFeedback),
    ) -> ExecutionReport {
        runtime::execute_function(
            self.program.as_ref(),
            &mut self.macro_cache,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            &mut self.stopwatches,
            &mut self.schedules,
            reference,
            arguments,
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
    ) -> ExecutionReport {
        runtime::execute_command(
            self.program.as_ref(),
            &mut self.macro_cache,
            &mut self.scoreboard,
            &mut self.command_storage,
            &mut self.random,
            &mut self.stopwatches,
            &mut self.schedules,
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
    /// After those tags, the VM advances its logical scheduling tick and runs
    /// callbacks that are due. Each function is an independent top-level
    /// execution with the supplied context and command limit. Its command
    /// feedback and result are suppressed, and a failure is recorded without
    /// stopping later functions.
    pub fn tick(&mut self, context: ExecutionContext, command_limit: usize) -> TickReport {
        let run_load = self.load_pending;
        self.load_pending = false;
        let Self {
            program,
            macro_cache,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            schedules,
            load_pending: _,
        } = self;
        let mut failures = Vec::new();
        if run_load {
            let load_tag = Identifier::parse("minecraft:load")
                .expect("the built-in load function tag identifier is valid");
            execute_automatic_tag(
                program,
                macro_cache,
                scoreboard,
                command_storage,
                random,
                stopwatches,
                schedules,
                &load_tag,
                TickPhase::Load,
                context,
                command_limit,
                &mut failures,
            );
        }
        let tick_tag = Identifier::parse("minecraft:tick")
            .expect("the built-in tick function tag identifier is valid");
        execute_automatic_tag(
            program,
            macro_cache,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            schedules,
            &tick_tag,
            TickPhase::Tick,
            context,
            command_limit,
            &mut failures,
        );
        schedules.advance();
        while let Some(reference) = schedules.pop_due() {
            match reference {
                FunctionReference::Function(function) => execute_tick_function(
                    program,
                    macro_cache,
                    scoreboard,
                    command_storage,
                    random,
                    stopwatches,
                    schedules,
                    &function,
                    TickPhase::Scheduled,
                    context,
                    command_limit,
                    &mut failures,
                ),
                FunctionReference::Tag(tag) => execute_automatic_tag(
                    program,
                    macro_cache,
                    scoreboard,
                    command_storage,
                    random,
                    stopwatches,
                    schedules,
                    &tag,
                    TickPhase::Scheduled,
                    context,
                    command_limit,
                    &mut failures,
                ),
            }
        }
        TickReport { failures }
    }

    fn new(program: Arc<Program>, world_seed: i64) -> Self {
        Self {
            program,
            macro_cache: MacroCacheState::default(),
            scoreboard: Scoreboard::default(),
            command_storage: CommandStorage::default(),
            random: RandomState::new(world_seed),
            stopwatches: StopwatchState::new(),
            schedules: ScheduleState::default(),
            load_pending: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_automatic_tag(
    program: &Program,
    macro_cache: &mut MacroCacheState,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    schedules: &mut ScheduleState,
    tag: &Identifier,
    phase: TickPhase,
    context: ExecutionContext,
    command_limit: usize,
    failures: &mut Vec<TickFunctionFailure>,
) {
    let Some(functions) = program.function_tag(tag) else {
        return;
    };
    for function in functions {
        execute_tick_function(
            program,
            macro_cache,
            scoreboard,
            command_storage,
            random,
            stopwatches,
            schedules,
            function,
            phase,
            context,
            command_limit,
            failures,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_tick_function(
    program: &Program,
    macro_cache: &mut MacroCacheState,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    schedules: &mut ScheduleState,
    function: &Identifier,
    phase: TickPhase,
    context: ExecutionContext,
    command_limit: usize,
    failures: &mut Vec<TickFunctionFailure>,
) {
    if let Err(error) = runtime::execute_automatic_function(
        program,
        macro_cache,
        scoreboard,
        command_storage,
        random,
        stopwatches,
        schedules,
        function,
        context,
        command_limit,
    )
    .into_result()
    {
        failures.push(TickFunctionFailure {
            phase,
            function: function.to_string(),
            error,
        });
    }
}

fn parse_storage_id(input: &str) -> Result<Identifier, StorageIdParseError> {
    Identifier::parse(input).ok_or_else(|| StorageIdParseError {
        input: input.to_owned(),
    })
}
