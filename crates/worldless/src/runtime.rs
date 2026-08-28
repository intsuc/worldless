use std::{cell::Cell, collections::VecDeque, error::Error, fmt, rc::Rc, sync::Arc};

use worldless_brigadier::exceptions::{java_f32, java_f64};

use crate::{
    execution_context::ExecutionContext,
    loader::CommandCompiler,
    macro_function::{Function, FunctionInstantiationError},
    nbt::{CommandStorage, CompoundTag, JavaString, NbtEditError, Tag},
    program::{
        Command, ComputeCommand, ComputeMode, DataCommand, DataModifyOperation, DataSource,
        FunctionArguments, Instruction, Modifier, ObjectiveId, PredicateCondition, Program,
        RandomCommand, ResolvedFunctions, ScoreCondition, ScoreHolderSet, ScorePredicate,
        Scoreboard, ScoreboardCommand, StopwatchCommand, StopwatchCondition, StorageCondition,
        StorageNumberType, StoreKind,
    },
    random::{LegacyRandom, RandomState},
    resource::{FunctionReference, Identifier},
    stopwatch::StopwatchState,
};

/// A command-feedback text payload preserved as Java UTF-16 code units.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct FeedbackText(JavaString);

impl FeedbackText {
    /// Returns the Java UTF-16 code units of this message.
    pub fn as_utf16(&self) -> &[u16] {
        self.0.units()
    }

    /// Converts this message to UTF-8, replacing unpaired surrogates.
    pub fn to_string_lossy(&self) -> String {
        self.0.to_string_lossy()
    }
}

impl fmt::Debug for FeedbackText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Display for FeedbackText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// One message accepted by an invocation's console-like command source.
///
/// A VM invocation delivers these events synchronously in execution order.
/// Minecraft components use their default English translations and are flattened
/// to [`FeedbackText`]. Called function bodies are silent for this source. Feedback
/// is independent of [`ExecutionOutcome`], and events already delivered remain
/// observable if the invocation later returns an [`ExecutionError`]. Messages sent
/// to server administrators are outside this channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandFeedback {
    /// Feedback sent through the command source's success channel.
    Success(FeedbackText),
    /// Feedback sent through the command source's failure channel.
    Failure(FeedbackText),
}

/// The observable result of a VM invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionOutcome {
    /// Execution completed without invoking the top-level result callback.
    NoResult,
    /// Execution invoked the top-level result callback.
    Result { success: bool, value: i32 },
}

/// An error that prevents a VM invocation from completing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionError {
    InvalidFunctionReference { input: String },
    CommandCompilationFailed { reason: String },
    PredicateEvaluationFailed { reason: String },
    NumberProviderEvaluationFailed { reason: String },
    CommandLimitExceeded { limit: usize },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunctionReference { input } => {
                write!(formatter, "invalid function reference {input:?}")
            }
            Self::CommandCompilationFailed { reason } => {
                write!(formatter, "command compilation failed: {reason}")
            }
            Self::PredicateEvaluationFailed { reason } => {
                write!(formatter, "predicate evaluation failed: {reason}")
            }
            Self::NumberProviderEvaluationFailed { reason } => {
                write!(formatter, "number provider evaluation failed: {reason}")
            }
            Self::CommandLimitExceeded { limit } => {
                write!(
                    formatter,
                    "command execution stopped at the limit of {limit}"
                )
            }
        }
    }
}

impl Error for ExecutionError {}

struct FeedbackTextBuilder(Vec<u16>);

impl FeedbackTextBuilder {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn push_str(&mut self, value: &str) {
        self.0.extend(value.encode_utf16());
    }

    fn push_java(&mut self, value: &JavaString) {
        self.0.extend_from_slice(value.units());
    }

    fn finish(self) -> FeedbackText {
        FeedbackText(JavaString::from_units(self.0))
    }
}

fn feedback_text(build: impl FnOnce(&mut FeedbackTextBuilder)) -> FeedbackText {
    let mut builder = FeedbackTextBuilder::new();
    build(&mut builder);
    builder.finish()
}

fn literal_feedback(value: &str) -> FeedbackText {
    FeedbackText(JavaString::from(value))
}

macro_rules! send_success {
    ($silent:expr, $text:expr, $feedback:expr $(,)?) => {
        if !$silent {
            ($feedback)(CommandFeedback::Success($text));
        }
    };
}

macro_rules! send_failure {
    ($silent:expr, $forked:expr, $text:expr, $feedback:expr $(,)?) => {
        if !$silent && !$forked {
            ($feedback)(CommandFeedback::Failure($text));
        }
    };
}

#[derive(Clone, Copy)]
struct CommandResult {
    success: bool,
    value: i32,
}

impl CommandResult {
    const FAILURE: Self = Self {
        success: false,
        value: 0,
    };

    fn success(value: i32) -> Self {
        Self {
            success: true,
            value,
        }
    }
}

struct OrdinaryExecution {
    result: CommandResult,
    failure: Option<FeedbackText>,
}

impl OrdinaryExecution {
    fn success(value: i32) -> Self {
        Self {
            result: CommandResult::success(value),
            failure: None,
        }
    }

    fn failure(message: FeedbackText) -> Self {
        Self {
            result: CommandResult::FAILURE,
            failure: Some(message),
        }
    }
}

#[derive(Clone)]
enum StoreAction {
    Score {
        kind: StoreKind,
        holders: Vec<JavaString>,
        objective: ObjectiveId,
    },
    Storage {
        kind: StoreKind,
        storage: Identifier,
        path: crate::nbt::NbtPath,
        number_type: StorageNumberType,
        scale: f64,
    },
}

#[derive(Clone)]
enum ConsumerEnd {
    TopLevel,
    FunctionCondition(Rc<Cell<Option<i32>>>),
    FunctionTag(Rc<Cell<Option<i32>>>),
}

#[derive(Clone)]
enum ConsumerAction {
    Store(StoreAction),
    End(ConsumerEnd),
}

#[derive(Clone, Default)]
struct ResultConsumer {
    actions: Vec<ConsumerAction>,
}

#[derive(Clone, Default)]
struct FeedbackConsumer {
    function_results: Vec<Identifier>,
}

impl FeedbackConsumer {
    fn empty() -> Self {
        Self::default()
    }

    fn function_result(id: Identifier) -> Self {
        Self {
            function_results: vec![id],
        }
    }

    fn chain(&self, other: &Self) -> Self {
        let mut function_results = self.function_results.clone();
        function_results.extend(other.function_results.iter().cloned());
        Self { function_results }
    }

    fn accept(&self, result: CommandResult, feedback: &mut impl FnMut(CommandFeedback)) {
        for id in &self.function_results {
            feedback(CommandFeedback::Success(feedback_text(|text| {
                text.push_str("Function ");
                text.push_str(&id.to_string());
                text.push_str(" returned ");
                text.push_str(&result.value.to_string());
            })));
        }
    }
}

impl ResultConsumer {
    fn empty() -> Self {
        Self::default()
    }

    fn top_level() -> Self {
        Self {
            actions: vec![ConsumerAction::End(ConsumerEnd::TopLevel)],
        }
    }

    fn function_condition(result: Rc<Cell<Option<i32>>>) -> Self {
        Self {
            actions: vec![ConsumerAction::End(ConsumerEnd::FunctionCondition(result))],
        }
    }

    fn function_tag(result: Rc<Cell<Option<i32>>>) -> Self {
        Self {
            actions: vec![ConsumerAction::End(ConsumerEnd::FunctionTag(result))],
        }
    }

    fn with_prefix(&self, mut prefix: Vec<StoreAction>) -> Self {
        let mut actions = prefix
            .drain(..)
            .map(ConsumerAction::Store)
            .collect::<Vec<_>>();
        actions.extend(self.actions.iter().cloned());
        Self { actions }
    }

    fn chain(&self, other: &Self) -> Self {
        let mut actions = self.actions.clone();
        actions.extend(other.actions.iter().cloned());
        Self { actions }
    }

    fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    fn accept(
        &self,
        result: CommandResult,
        scoreboard: &mut Scoreboard,
        command_storage: &mut CommandStorage,
        top_level_result: &mut Option<ExecutionOutcome>,
    ) {
        for action in &self.actions {
            match action {
                ConsumerAction::Store(StoreAction::Score {
                    kind,
                    holders,
                    objective,
                }) => {
                    let value = stored_command_value(*kind, result);
                    for holder in holders {
                        scoreboard.set_score_by_id(holder, *objective, value);
                    }
                }
                ConsumerAction::Store(StoreAction::Storage {
                    kind,
                    storage,
                    path,
                    number_type,
                    scale,
                }) => {
                    let value = storage_number(
                        *number_type,
                        f64::from(stored_command_value(*kind, result)) * scale,
                    );
                    let _ = command_storage.edit(storage, |data| path.set(data, value));
                }
                ConsumerAction::End(ConsumerEnd::TopLevel) => {
                    *top_level_result = Some(ExecutionOutcome::Result {
                        success: result.success,
                        value: result.value,
                    });
                }
                ConsumerAction::End(ConsumerEnd::FunctionCondition(condition_result)) => {
                    condition_result.set(Some(result.value));
                }
                ConsumerAction::End(ConsumerEnd::FunctionTag(tag_result)) => {
                    tag_result.set(Some(
                        tag_result.get().unwrap_or(0).wrapping_add(result.value),
                    ));
                }
            }
        }
    }
}

struct Frame {
    function: Arc<[Instruction]>,
    context: ExecutionContext,
    next_instruction: usize,
    depth: usize,
    discard_depth: usize,
    source_consumer: ResultConsumer,
    result_consumer: ResultConsumer,
    result_feedback: FeedbackConsumer,
    silent: bool,
}

#[derive(Clone)]
struct InstantiatedFunction {
    id: Identifier,
    instructions: Arc<[Instruction]>,
}

enum QueueEntry {
    Call(Frame),
    Step(Frame),
    Prepare {
        frame: Frame,
        context: ExecutionContext,
        instruction: usize,
        next_modifier: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
        active: bool,
        forked: bool,
    },
    ExecuteOrdinary {
        frame: Frame,
        context: ExecutionContext,
        instruction: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
        forked: bool,
    },
    ResumeFunctionCondition {
        frame: Frame,
        context: ExecutionContext,
        instruction: usize,
        next_modifier: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
        expected: bool,
        result: Rc<Cell<Option<i32>>>,
    },
    ContinueFunctionTag {
        frame: Frame,
        context: ExecutionContext,
        functions: Arc<[InstantiatedFunction]>,
        next_function: usize,
        consumer: ResultConsumer,
        result: Option<Rc<Cell<Option<i32>>>>,
    },
    Fallthrough {
        depth: usize,
        discard_depth: usize,
        result_consumer: ResultConsumer,
        result_feedback: FeedbackConsumer,
    },
}

impl QueueEntry {
    fn depth(&self) -> usize {
        match self {
            Self::Call(frame)
            | Self::Step(frame)
            | Self::Prepare { frame, .. }
            | Self::ExecuteOrdinary { frame, .. }
            | Self::ResumeFunctionCondition { frame, .. }
            | Self::ContinueFunctionTag { frame, .. } => frame.depth,
            Self::Fallthrough { depth, .. } => *depth,
        }
    }
}

struct CommandQuota {
    limit: usize,
    used: usize,
}

impl CommandQuota {
    fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }

    fn exhausted(&self) -> bool {
        self.used >= self.limit
    }

    fn increment(&mut self) {
        self.used = self.used.saturating_add(1);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_function(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    input: &str,
    arguments: Option<&CompoundTag>,
    context: ExecutionContext,
    command_limit: usize,
    feedback: impl FnMut(CommandFeedback),
) -> Result<ExecutionOutcome, ExecutionError> {
    let reference = FunctionReference::parse(input).ok_or_else(|| {
        ExecutionError::InvalidFunctionReference {
            input: input.to_owned(),
        }
    })?;
    let instruction = Instruction {
        modifiers: Vec::new(),
        command: Command::Function {
            reference,
            arguments: arguments.cloned().map(FunctionArguments::Compound),
        },
    };
    execute_instruction(
        program,
        scoreboard,
        command_storage,
        random,
        stopwatches,
        instruction,
        None,
        context,
        command_limit,
        feedback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_command(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    input: &str,
    context: ExecutionContext,
    command_limit: usize,
    feedback: impl FnMut(CommandFeedback),
) -> Result<ExecutionOutcome, ExecutionError> {
    let command = input.strip_prefix('/').unwrap_or(input);
    let compiler = CommandCompiler::with_loot_registry(Arc::clone(program.loot_registry()));
    let instruction = compiler
        .compile(command)
        .map_err(|reason| ExecutionError::CommandCompilationFailed { reason })?;
    execute_instruction(
        program,
        scoreboard,
        command_storage,
        random,
        stopwatches,
        instruction,
        Some(compiler),
        context,
        command_limit,
        feedback,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_instruction(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    instruction: Instruction,
    compiler: Option<CommandCompiler>,
    context: ExecutionContext,
    command_limit: usize,
    feedback: impl FnMut(CommandFeedback),
) -> Result<ExecutionOutcome, ExecutionError> {
    let function = Arc::<[Instruction]>::from([instruction]);
    let queue = VecDeque::from([QueueEntry::Step(Frame {
        function,
        context,
        next_instruction: 0,
        depth: 0,
        discard_depth: 0,
        source_consumer: ResultConsumer::top_level(),
        result_consumer: ResultConsumer::top_level(),
        result_feedback: FeedbackConsumer::empty(),
        silent: false,
    })]);

    execute_queue(
        program,
        scoreboard,
        command_storage,
        random,
        stopwatches,
        queue,
        compiler,
        command_limit,
        feedback,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_automatic_function(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    id: &Identifier,
    context: ExecutionContext,
    command_limit: usize,
) -> Result<(), ExecutionError> {
    let function = program
        .function(id)
        .expect("a resolved function tag contains loaded functions");
    let mut compiler = None;
    let Ok(function) = instantiate_function(function, None, &mut compiler, program.loot_registry())
    else {
        return Ok(());
    };
    let queue = VecDeque::from([QueueEntry::Call(Frame {
        function,
        context,
        next_instruction: 0,
        depth: 0,
        discard_depth: 0,
        source_consumer: ResultConsumer::empty(),
        result_consumer: ResultConsumer::empty(),
        result_feedback: FeedbackConsumer::empty(),
        silent: true,
    })]);

    execute_queue(
        program,
        scoreboard,
        command_storage,
        random,
        stopwatches,
        queue,
        compiler,
        command_limit,
        drop,
    )
    .map(drop)
}

#[allow(clippy::too_many_arguments)]
fn execute_queue(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    stopwatches: &mut StopwatchState,
    mut queue: VecDeque<QueueEntry>,
    mut compiler: Option<CommandCompiler>,
    command_limit: usize,
    mut feedback: impl FnMut(CommandFeedback),
) -> Result<ExecutionOutcome, ExecutionError> {
    let mut quota = CommandQuota::new(command_limit);
    let mut top_level_result = None;

    loop {
        if quota.exhausted() {
            return Err(ExecutionError::CommandLimitExceeded {
                limit: command_limit,
            });
        }
        let Some(entry) = queue.pop_front() else {
            return Ok(top_level_result.unwrap_or(ExecutionOutcome::NoResult));
        };

        match entry {
            QueueEntry::Call(frame) => {
                quota.increment();
                schedule_next_instruction(&mut queue, frame);
            }
            QueueEntry::Step(mut frame) => {
                let instruction = frame.next_instruction;
                frame.next_instruction += 1;
                let context = frame.context;
                queue.push_front(QueueEntry::Prepare {
                    frame,
                    context,
                    instruction,
                    next_modifier: 0,
                    stores: Vec::new(),
                    return_run: false,
                    active: true,
                    forked: false,
                });
            }
            QueueEntry::Prepare {
                frame,
                mut context,
                instruction,
                mut next_modifier,
                stores,
                return_run,
                active,
                forked,
            } => {
                let function = Arc::clone(&frame.function);
                let compiled = &function[instruction];
                let mut frame = Some(frame);
                let mut stores = Some(stores);
                let mut active = active;
                let mut forked = forked;

                let ready = loop {
                    let Some(modifier) = compiled.modifiers.get(next_modifier) else {
                        break true;
                    };
                    next_modifier += 1;
                    match modifier {
                        Modifier::ContextTransform(transform) => {
                            quota.increment();
                            if active {
                                transform.apply(&mut context);
                            }
                        }
                        Modifier::StoreScore {
                            kind,
                            holders,
                            objective,
                        } => {
                            quota.increment();
                            if !active {
                                continue;
                            }
                            let resolved_holders = scoreboard.resolve_holders(holders);
                            let resolved_objective = scoreboard.objective_id(objective);
                            let (Some(holders), Some(resolved_objective)) =
                                (resolved_holders.as_ref(), resolved_objective)
                            else {
                                send_failure!(
                                    frame
                                        .as_ref()
                                        .expect("the frame has not been queued")
                                        .silent,
                                    forked,
                                    if resolved_holders.is_none() {
                                        literal_feedback("No relevant score holders could be found")
                                    } else {
                                        literal_feedback(&format!(
                                            "Unknown scoreboard objective '{objective}'"
                                        ))
                                    },
                                    &mut feedback,
                                );
                                if forked {
                                    active = false;
                                    continue;
                                } else if !return_run {
                                    schedule_next_instruction(
                                        &mut queue,
                                        frame.take().expect("the frame has not been queued"),
                                    );
                                }
                                break false;
                            };
                            stores
                                .as_mut()
                                .expect("the stores have not been queued")
                                .push(StoreAction::Score {
                                    kind: *kind,
                                    holders: holders.clone(),
                                    objective: resolved_objective,
                                });
                        }
                        Modifier::StoreStorage {
                            kind,
                            storage,
                            path,
                            number_type,
                            scale,
                        } => {
                            quota.increment();
                            if active {
                                stores
                                    .as_mut()
                                    .expect("the stores have not been queued")
                                    .push(StoreAction::Storage {
                                        kind: *kind,
                                        storage: storage.clone(),
                                        path: path.clone(),
                                        number_type: *number_type,
                                        scale: *scale,
                                    });
                            }
                        }
                        Modifier::Condition(condition) => {
                            quota.increment();
                            forked = true;
                            if active {
                                active = scoreboard.evaluate_condition(condition).unwrap_or(false);
                            }
                        }
                        Modifier::StorageCondition(condition) => {
                            quota.increment();
                            forked = true;
                            if active {
                                active = storage_condition_matches(command_storage, condition);
                            }
                        }
                        Modifier::PredicateCondition(condition) => {
                            quota.increment();
                            forked = true;
                            if active {
                                active = program
                                    .loot_registry()
                                    .test_predicate(
                                        &condition.predicate,
                                        scoreboard,
                                        command_storage,
                                        &context,
                                        random.unnamed(),
                                    )
                                    .map_err(predicate_evaluation_failed)?
                                    == condition.expected;
                            }
                        }
                        Modifier::StopwatchCondition(condition) => {
                            quota.increment();
                            forked = true;
                            if active {
                                active = stopwatch_condition_matches(stopwatches, condition)
                                    .unwrap_or(false);
                            }
                        }
                        Modifier::FunctionCondition {
                            expected,
                            function: function_reference,
                        } => {
                            forked = true;
                            let frame = frame.take().expect("the frame has not been queued");
                            let stores = stores.take().expect("the stores have not been queued");
                            let condition_functions =
                                match program.resolve_functions(function_reference) {
                                    Some(ResolvedFunctions::Tag([])) | None => {
                                        if !return_run {
                                            schedule_next_instruction(&mut queue, frame);
                                        }
                                        break false;
                                    }
                                    Some(functions) => functions,
                                };

                            if !active {
                                queue.push_front(QueueEntry::Prepare {
                                    frame,
                                    context,
                                    instruction,
                                    next_modifier,
                                    stores,
                                    return_run,
                                    active: false,
                                    forked,
                                });
                                break false;
                            }

                            let (condition_functions, _) = instantiate_resolved_prefix(
                                program,
                                function_reference,
                                condition_functions,
                                None,
                                &mut compiler,
                            );

                            let result = Rc::new(Cell::new(None));
                            let result_consumer =
                                ResultConsumer::function_condition(Rc::clone(&result));
                            let isolated_depth = frame.depth + 1;
                            queue.push_front(QueueEntry::ResumeFunctionCondition {
                                frame,
                                context,
                                instruction,
                                next_modifier,
                                stores,
                                return_run,
                                expected: *expected,
                                result,
                            });
                            queue.push_front(QueueEntry::Fallthrough {
                                depth: isolated_depth,
                                discard_depth: isolated_depth,
                                result_consumer: result_consumer.clone(),
                                result_feedback: FeedbackConsumer::empty(),
                            });
                            for function in condition_functions.into_iter().rev() {
                                queue.push_front(QueueEntry::Call(Frame {
                                    function: function.instructions,
                                    context,
                                    next_instruction: 0,
                                    depth: isolated_depth,
                                    discard_depth: isolated_depth,
                                    source_consumer: ResultConsumer::empty(),
                                    result_consumer: result_consumer.clone(),
                                    result_feedback: FeedbackConsumer::empty(),
                                    silent: true,
                                }));
                            }
                            break false;
                        }
                        Modifier::ReturnRun => {
                            let frame = frame.take().expect("the frame has not been queued");
                            if active {
                                discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                                queue.push_front(QueueEntry::Prepare {
                                    frame,
                                    context,
                                    instruction,
                                    next_modifier,
                                    stores: stores.take().expect("the stores have not been queued"),
                                    return_run: true,
                                    active: true,
                                    forked,
                                });
                            } else if return_run {
                                let result_feedback = frame.result_feedback.clone();
                                queue.push_front(QueueEntry::Fallthrough {
                                    depth: frame.depth,
                                    discard_depth: frame.discard_depth,
                                    result_consumer: frame.result_consumer,
                                    result_feedback,
                                });
                            } else {
                                schedule_next_instruction(&mut queue, frame);
                            }
                            break false;
                        }
                    }
                };

                if !ready {
                    continue;
                }
                let frame = frame.expect("a ready command retains its frame");
                let stores = stores.expect("a ready command retains its stores");

                if !active {
                    if return_run {
                        let result_feedback = frame.result_feedback.clone();
                        queue.push_front(QueueEntry::Fallthrough {
                            depth: frame.depth,
                            discard_depth: frame.discard_depth,
                            result_consumer: frame.result_consumer,
                            result_feedback,
                        });
                    } else {
                        schedule_next_instruction(&mut queue, frame);
                    }
                    continue;
                }

                match &compiled.command {
                    Command::Function {
                        reference,
                        arguments,
                    } => {
                        let command_consumer = frame.source_consumer.with_prefix(stores);
                        execute_function_command(
                            program,
                            scoreboard,
                            command_storage,
                            &mut compiler,
                            &mut queue,
                            &mut top_level_result,
                            frame,
                            context,
                            reference,
                            arguments.as_ref(),
                            command_consumer,
                            return_run,
                            forked,
                            &mut feedback,
                        );
                    }
                    Command::Return { success, value } => {
                        let result = CommandResult {
                            success: *success,
                            value: *value,
                        };
                        frame.source_consumer.with_prefix(stores).accept(
                            result,
                            scoreboard,
                            command_storage,
                            &mut top_level_result,
                        );
                        frame.result_feedback.accept(result, &mut feedback);
                        frame.result_consumer.accept(
                            result,
                            scoreboard,
                            command_storage,
                            &mut top_level_result,
                        );
                        discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                    }
                    Command::Scoreboard(_)
                    | Command::Seed
                    | Command::Condition(_)
                    | Command::StorageCondition(_)
                    | Command::PredicateCondition(_)
                    | Command::Data(_)
                    | Command::Compute(_)
                    | Command::Random(_)
                    | Command::Stopwatch(_)
                    | Command::StopwatchCondition(_) => {
                        queue.push_front(QueueEntry::ExecuteOrdinary {
                            frame,
                            context,
                            instruction,
                            stores,
                            return_run,
                            forked,
                        });
                    }
                }
            }
            QueueEntry::ExecuteOrdinary {
                frame,
                context,
                instruction,
                stores,
                return_run,
                forked,
            } => {
                quota.increment();
                let command = &frame.function[instruction].command;
                let execution = match command {
                    Command::Scoreboard(command) => {
                        execute_scoreboard_command(scoreboard, command, frame.silent, &mut feedback)
                    }
                    Command::Seed => {
                        let seed = random.world_seed();
                        send_success!(
                            frame.silent,
                            literal_feedback(&format!("Seed: [{seed}]")),
                            &mut feedback,
                        );
                        OrdinaryExecution::success(seed as i32)
                    }
                    Command::Condition(condition) => {
                        execute_condition(scoreboard, condition, frame.silent, &mut feedback)
                    }
                    Command::StorageCondition(condition) => execute_storage_condition(
                        command_storage,
                        condition,
                        frame.silent,
                        &mut feedback,
                    ),
                    Command::PredicateCondition(condition) => execute_predicate_condition(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        condition,
                        frame.silent,
                        &mut feedback,
                    )
                    .map_err(predicate_evaluation_failed)?,
                    Command::Data(command) => execute_data_command(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        command,
                        frame.silent,
                        &mut feedback,
                    )
                    .map_err(|reason| ExecutionError::NumberProviderEvaluationFailed { reason })?,
                    Command::Compute(command) => execute_compute_command(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        command,
                        frame.silent,
                        &mut feedback,
                    )
                    .map_err(|reason| ExecutionError::NumberProviderEvaluationFailed { reason })?,
                    Command::Random(command) => {
                        execute_random_command(random, command, frame.silent, &mut feedback)
                    }
                    Command::Stopwatch(command) => {
                        execute_stopwatch_command(stopwatches, command, frame.silent, &mut feedback)
                    }
                    Command::StopwatchCondition(condition) => execute_stopwatch_condition(
                        stopwatches,
                        condition,
                        frame.silent,
                        &mut feedback,
                    ),
                    Command::Function { .. } | Command::Return { .. } => {
                        unreachable!("only ordinary commands are queued for ordinary execution")
                    }
                };
                let result = execution.result;
                frame.source_consumer.with_prefix(stores).accept(
                    result,
                    scoreboard,
                    command_storage,
                    &mut top_level_result,
                );
                if return_run {
                    frame.result_feedback.accept(result, &mut feedback);
                    frame.result_consumer.accept(
                        result,
                        scoreboard,
                        command_storage,
                        &mut top_level_result,
                    );
                }
                if let Some(message) = execution.failure {
                    // Brigadier notifies the complete executable callback chain before
                    // Minecraft's task-level error handler reports the exception.
                    send_failure!(frame.silent, forked, message, &mut feedback);
                }

                if return_run {
                    discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                } else {
                    schedule_next_instruction(&mut queue, frame);
                }
            }
            QueueEntry::ResumeFunctionCondition {
                frame,
                context,
                instruction,
                next_modifier,
                stores,
                return_run,
                expected,
                result,
            } => {
                let active = result.get().is_some_and(|value| (value != 0) == expected);
                queue.push_front(QueueEntry::Prepare {
                    frame,
                    context,
                    instruction,
                    next_modifier,
                    stores,
                    return_run,
                    active,
                    forked: true,
                });
            }
            QueueEntry::ContinueFunctionTag {
                frame,
                context,
                functions,
                next_function,
                consumer,
                result,
            } => {
                let Some(function) = functions.get(next_function).cloned() else {
                    if let Some(value) = result.as_ref().and_then(|result| result.get()) {
                        consumer.accept(
                            CommandResult::success(value),
                            scoreboard,
                            command_storage,
                            &mut top_level_result,
                        );
                    }
                    schedule_next_instruction(&mut queue, frame);
                    continue;
                };
                let child_depth = frame.depth + 1;
                let result_consumer = result
                    .as_ref()
                    .map_or_else(ResultConsumer::empty, |result| {
                        ResultConsumer::function_tag(Rc::clone(result))
                    });
                let report_feedback = !frame.silent;
                queue.push_front(QueueEntry::ContinueFunctionTag {
                    frame,
                    context,
                    functions,
                    next_function: next_function + 1,
                    consumer,
                    result,
                });
                queue.push_front(QueueEntry::Call(Frame {
                    function: function.instructions,
                    context,
                    next_instruction: 0,
                    depth: child_depth,
                    discard_depth: child_depth,
                    source_consumer: ResultConsumer::empty(),
                    result_consumer,
                    result_feedback: if report_feedback {
                        FeedbackConsumer::function_result(function.id)
                    } else {
                        FeedbackConsumer::empty()
                    },
                    silent: true,
                }));
            }
            QueueEntry::Fallthrough {
                discard_depth,
                result_consumer,
                result_feedback,
                ..
            } => {
                result_feedback.accept(CommandResult::FAILURE, &mut feedback);
                result_consumer.accept(
                    CommandResult::FAILURE,
                    scoreboard,
                    command_storage,
                    &mut top_level_result,
                );
                discard_at_depth_or_higher(&mut queue, discard_depth);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_function_command(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    compiler: &mut Option<CommandCompiler>,
    queue: &mut VecDeque<QueueEntry>,
    top_level_result: &mut Option<ExecutionOutcome>,
    frame: Frame,
    context: ExecutionContext,
    reference: &FunctionReference,
    argument_source: Option<&FunctionArguments>,
    command_consumer: ResultConsumer,
    return_run: bool,
    forked: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) {
    let functions = match program.resolve_functions(reference) {
        None => {
            send_failure!(
                frame.silent,
                forked,
                match reference {
                    FunctionReference::Function(id) => {
                        literal_feedback(&format!("Unknown function {id}"))
                    }
                    FunctionReference::Tag(id) => {
                        literal_feedback(&format!("Unknown function tag '{id}'"))
                    }
                },
                feedback,
            );
            fail_function_command(
                queue,
                frame,
                context,
                Vec::new(),
                command_consumer,
                return_run,
                scoreboard,
                command_storage,
                top_level_result,
            );
            return;
        }
        Some(ResolvedFunctions::Tag([])) => {
            let FunctionReference::Tag(id) = reference else {
                unreachable!("only a function tag resolves to an empty collection")
            };
            send_failure!(
                frame.silent,
                forked,
                literal_feedback(&format!("Can't find any functions for name {id}")),
                feedback,
            );
            fail_function_command(
                queue,
                frame,
                context,
                Vec::new(),
                command_consumer,
                return_run,
                scoreboard,
                command_storage,
                top_level_result,
            );
            return;
        }
        Some(functions) => functions,
    };

    let arguments = match resolve_function_arguments(argument_source, command_storage) {
        Ok(arguments) => arguments,
        Err(reason) => {
            send_failure!(
                frame.silent,
                forked,
                function_argument_feedback(&reason),
                feedback,
            );
            fail_function_command(
                queue,
                frame,
                context,
                Vec::new(),
                command_consumer,
                return_run,
                scoreboard,
                command_storage,
                top_level_result,
            );
            return;
        }
    };
    let is_single = matches!(functions, ResolvedFunctions::Single(_))
        || matches!(functions, ResolvedFunctions::Tag([_]));
    send_success!(
        frame.silent,
        function_scheduled_feedback(reference, &functions),
        feedback,
    );
    let (instances, failure) =
        instantiate_resolved_prefix(program, reference, functions, arguments.as_ref(), compiler);
    if let Some((id, reason)) = failure {
        send_failure!(
            frame.silent,
            forked,
            function_instantiation_feedback(&id, &reason),
            feedback,
        );
        fail_function_command(
            queue,
            frame,
            context,
            instances,
            command_consumer,
            return_run,
            scoreboard,
            command_storage,
            top_level_result,
        );
    } else if is_single {
        let function = instances
            .into_iter()
            .next()
            .expect("a resolved single function produces one instance");
        queue_single_function(
            queue,
            frame,
            context,
            function,
            command_consumer,
            return_run,
        );
    } else {
        queue_function_tag(
            queue,
            frame,
            context,
            instances,
            command_consumer,
            return_run,
        );
    }
}

fn function_scheduled_feedback(
    reference: &FunctionReference,
    functions: &ResolvedFunctions<'_>,
) -> FeedbackText {
    match functions {
        ResolvedFunctions::Single(_) => {
            let FunctionReference::Function(id) = reference else {
                unreachable!("a single resolution has a function reference")
            };
            literal_feedback(&format!("Running function {id}"))
        }
        ResolvedFunctions::Tag([id]) => literal_feedback(&format!("Running function {id}")),
        ResolvedFunctions::Tag(ids) => feedback_text(|text| {
            text.push_str("Running functions ");
            for (index, id) in ids.iter().enumerate() {
                if index != 0 {
                    text.push_str(", ");
                }
                text.push_str(&id.to_string());
            }
        }),
    }
}

enum FunctionArgumentError {
    NotFound(JavaString),
    Multiple,
    NotCompound(&'static str),
}

fn resolve_function_arguments(
    source: Option<&FunctionArguments>,
    command_storage: &CommandStorage,
) -> Result<Option<crate::nbt::CompoundTag>, FunctionArgumentError> {
    match source {
        None => Ok(None),
        Some(FunctionArguments::Compound(arguments)) => Ok(Some(arguments.clone())),
        Some(FunctionArguments::Storage {
            storage,
            path: None,
        }) => Ok(Some(command_storage.get(storage))),
        Some(FunctionArguments::Storage {
            storage,
            path: Some(path),
        }) => {
            let root = command_storage.get(storage);
            let mut selected = path
                .get_with_not_found(&root)
                .map_err(FunctionArgumentError::NotFound)?;
            if selected.len() != 1 {
                return Err(FunctionArgumentError::Multiple);
            }
            match selected.pop().expect("one NBT value was selected") {
                Tag::Compound(arguments) => Ok(Some(arguments)),
                value => Err(FunctionArgumentError::NotCompound(tag_type_name(&value))),
            }
        }
    }
}

fn function_argument_feedback(reason: &FunctionArgumentError) -> FeedbackText {
    match reason {
        FunctionArgumentError::NotFound(prefix) => feedback_text(|text| {
            text.push_str("Found no elements matching ");
            text.push_java(prefix);
        }),
        FunctionArgumentError::Multiple => {
            literal_feedback("This argument accepts a single NBT value")
        }
        FunctionArgumentError::NotCompound(tag_type) => literal_feedback(&format!(
            "Invalid argument type: {tag_type}. Expected Compound"
        )),
    }
}

fn tag_type_name(tag: &Tag) -> &'static str {
    match tag {
        Tag::Byte(_) => "BYTE",
        Tag::Short(_) => "SHORT",
        Tag::Int(_) => "INT",
        Tag::Long(_) => "LONG",
        Tag::Float(_) => "FLOAT",
        Tag::Double(_) => "DOUBLE",
        Tag::ByteArray(_) => "BYTE[]",
        Tag::String(_) => "STRING",
        Tag::List(_) => "LIST",
        Tag::Compound(_) => "COMPOUND",
        Tag::IntArray(_) => "INT[]",
        Tag::LongArray(_) => "LONG[]",
    }
}

fn instantiate_resolved_prefix(
    program: &Program,
    reference: &FunctionReference,
    functions: ResolvedFunctions<'_>,
    arguments: Option<&crate::nbt::CompoundTag>,
    compiler: &mut Option<CommandCompiler>,
) -> (
    Vec<InstantiatedFunction>,
    Option<(Identifier, FunctionInstantiationError)>,
) {
    let mut instances = Vec::new();
    match functions {
        ResolvedFunctions::Single(function) => {
            match instantiate_function(function, arguments, compiler, program.loot_registry()) {
                Ok(instructions) => instances.push(InstantiatedFunction {
                    id: match reference {
                        FunctionReference::Function(id) => id.clone(),
                        FunctionReference::Tag(_) => {
                            unreachable!("a single function resolution has a function reference")
                        }
                    },
                    instructions,
                }),
                Err(reason) => {
                    let FunctionReference::Function(id) = reference else {
                        unreachable!("a single function resolution has a function reference")
                    };
                    return (instances, Some((id.clone(), reason)));
                }
            }
        }
        ResolvedFunctions::Tag(functions) => {
            for id in functions {
                let function = program
                    .function(id)
                    .expect("resolved function tags contain loaded functions");
                match instantiate_function(function, arguments, compiler, program.loot_registry()) {
                    Ok(instructions) => instances.push(InstantiatedFunction {
                        id: id.clone(),
                        instructions,
                    }),
                    Err(reason) => return (instances, Some((id.clone(), reason))),
                }
            }
        }
    }
    (instances, None)
}

fn function_instantiation_feedback(
    id: &Identifier,
    reason: &FunctionInstantiationError,
) -> FeedbackText {
    feedback_text(|text| {
        text.push_str("Failed to instantiate function ");
        text.push_str(&id.to_string());
        text.push_str(": ");
        match reason {
            FunctionInstantiationError::MissingArguments => {
                text.push_str("Missing arguments to function ");
                text.push_str(&id.to_string());
            }
            FunctionInstantiationError::MissingArgument(argument) => {
                text.push_str("Missing argument ");
                text.push_java(argument);
                text.push_str(" to function ");
                text.push_str(&id.to_string());
            }
            FunctionInstantiationError::Parse { command, reason } => {
                text.push_str("While instantiating macro ");
                text.push_str(&id.to_string());
                text.push_str(": Command '");
                text.push_java(command);
                text.push_str("' caused error: ");
                text.push_str(reason);
            }
            FunctionInstantiationError::Other(reason) => text.push_str(reason),
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn fail_function_command(
    queue: &mut VecDeque<QueueEntry>,
    frame: Frame,
    context: ExecutionContext,
    instances: Vec<InstantiatedFunction>,
    command_consumer: ResultConsumer,
    return_run: bool,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    top_level_result: &mut Option<ExecutionOutcome>,
) {
    if return_run {
        command_consumer.accept(
            CommandResult::FAILURE,
            scoreboard,
            command_storage,
            top_level_result,
        );
        let child_depth = frame.depth + 1;
        let discard_depth = frame.discard_depth;
        let child_consumer = command_consumer.chain(&frame.result_consumer);
        let parent_feedback = frame.result_feedback;
        let report_feedback = !frame.silent;
        for function in instances.into_iter().rev() {
            let function_feedback = if report_feedback {
                FeedbackConsumer::function_result(function.id)
            } else {
                FeedbackConsumer::empty()
            };
            queue.push_front(QueueEntry::Call(Frame {
                function: function.instructions,
                context,
                next_instruction: 0,
                depth: child_depth,
                discard_depth,
                source_consumer: ResultConsumer::empty(),
                result_consumer: child_consumer.clone(),
                result_feedback: function_feedback.chain(&parent_feedback),
                silent: true,
            }));
        }
    } else {
        command_consumer.accept(
            CommandResult::FAILURE,
            scoreboard,
            command_storage,
            top_level_result,
        );
        if instances.is_empty() {
            schedule_next_instruction(queue, frame);
        } else {
            queue.push_front(QueueEntry::ContinueFunctionTag {
                frame,
                context,
                functions: Arc::from(instances),
                next_function: 0,
                consumer: ResultConsumer::empty(),
                result: None,
            });
        }
    }
}

fn queue_single_function(
    queue: &mut VecDeque<QueueEntry>,
    frame: Frame,
    context: ExecutionContext,
    function: InstantiatedFunction,
    command_consumer: ResultConsumer,
    return_run: bool,
) {
    let child_depth = frame.depth + 1;
    if return_run {
        let parent_consumer = frame.result_consumer;
        let parent_feedback = frame.result_feedback;
        let function_feedback = if frame.silent {
            FeedbackConsumer::empty()
        } else {
            FeedbackConsumer::function_result(function.id)
        };
        let child_consumer = command_consumer.chain(&parent_consumer);
        let child = Frame {
            function: function.instructions,
            context,
            next_instruction: 0,
            depth: child_depth,
            discard_depth: frame.discard_depth,
            source_consumer: ResultConsumer::empty(),
            result_consumer: child_consumer,
            result_feedback: function_feedback.chain(&parent_feedback),
            silent: true,
        };
        queue.push_front(QueueEntry::Fallthrough {
            depth: frame.depth,
            discard_depth: frame.discard_depth,
            result_consumer: parent_consumer,
            result_feedback: parent_feedback,
        });
        queue.push_front(QueueEntry::Call(child));
    } else {
        let function_feedback = if frame.silent {
            FeedbackConsumer::empty()
        } else {
            FeedbackConsumer::function_result(function.id)
        };
        let child = Frame {
            function: function.instructions,
            context,
            next_instruction: 0,
            depth: child_depth,
            discard_depth: child_depth,
            source_consumer: ResultConsumer::empty(),
            result_consumer: command_consumer,
            result_feedback: function_feedback,
            silent: true,
        };
        schedule_next_instruction(queue, frame);
        queue.push_front(QueueEntry::Call(child));
    }
}

fn queue_function_tag(
    queue: &mut VecDeque<QueueEntry>,
    frame: Frame,
    context: ExecutionContext,
    functions: Vec<InstantiatedFunction>,
    command_consumer: ResultConsumer,
    return_run: bool,
) {
    if return_run {
        let child_depth = frame.depth + 1;
        let fallback_depth = frame.depth;
        let discard_depth = frame.discard_depth;
        let parent_consumer = frame.result_consumer;
        let parent_feedback = frame.result_feedback;
        let report_feedback = !frame.silent;
        let child_consumer = command_consumer.chain(&parent_consumer);
        queue.push_front(QueueEntry::Fallthrough {
            depth: fallback_depth,
            discard_depth,
            result_consumer: parent_consumer,
            result_feedback: parent_feedback.clone(),
        });
        for function in functions.into_iter().rev() {
            let function_feedback = if report_feedback {
                FeedbackConsumer::function_result(function.id)
            } else {
                FeedbackConsumer::empty()
            };
            queue.push_front(QueueEntry::Call(Frame {
                function: function.instructions,
                context,
                next_instruction: 0,
                depth: child_depth,
                discard_depth,
                source_consumer: ResultConsumer::empty(),
                result_consumer: child_consumer.clone(),
                result_feedback: function_feedback.chain(&parent_feedback),
                silent: true,
            }));
        }
    } else {
        let result = (!command_consumer.is_empty()).then(|| Rc::new(Cell::new(None)));
        queue.push_front(QueueEntry::ContinueFunctionTag {
            frame,
            context,
            functions: Arc::from(functions),
            next_function: 0,
            consumer: command_consumer,
            result,
        });
    }
}

fn execute_scoreboard_command(
    scoreboard: &mut Scoreboard,
    command: &ScoreboardCommand,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    match command {
        ScoreboardCommand::ListObjectives => {
            let count = scoreboard.list_objectives();
            send_success!(
                silent,
                if count == 0 {
                    literal_feedback("There are no objectives")
                } else {
                    feedback_text(|text| {
                        text.push_str("There are ");
                        text.push_str(&count.to_string());
                        text.push_str(" objective(s): ");
                        for (index, objective) in scoreboard.objective_names().iter().enumerate() {
                            if index != 0 {
                                text.push_str(", ");
                            }
                            push_formatted_objective(text, objective);
                        }
                    })
                },
                feedback,
            );
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::AddObjective { objective } => {
            let Some(count) = scoreboard.add_objective(objective) else {
                return OrdinaryExecution::failure(literal_feedback(
                    "An objective already exists by that name",
                ));
            };
            send_success!(
                silent,
                feedback_text(|text| {
                    text.push_str("Created new objective ");
                    push_formatted_objective(text, objective);
                }),
                feedback,
            );
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::RemoveObjective { objective } => {
            if !scoreboard.contains_objective(objective) {
                return unknown_objective(objective);
            }
            let count = scoreboard
                .remove_objective(objective)
                .expect("the objective was validated before removal");
            send_success!(
                silent,
                feedback_text(|text| {
                    text.push_str("Removed objective ");
                    push_formatted_objective(text, objective);
                }),
                feedback,
            );
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::ListPlayers => {
            let count = scoreboard.list_players();
            send_success!(
                silent,
                if count == 0 {
                    literal_feedback("There are no tracked entities")
                } else {
                    feedback_text(|text| {
                        text.push_str("There are ");
                        text.push_str(&count.to_string());
                        text.push_str(" tracked entity/entities: ");
                        for (index, holder) in scoreboard.holder_names().enumerate() {
                            if index != 0 {
                                text.push_str(", ");
                            }
                            text.push_java(holder);
                        }
                    })
                },
                feedback,
            );
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::ListPlayerScores {
            holder: ScoreHolderSet::Named(holder),
        } => {
            let count = scoreboard.player_score_count(holder);
            if !silent {
                let scores = scoreboard.player_scores(holder);
                if scores.is_empty() {
                    send_success!(
                        silent,
                        feedback_text(|text| {
                            text.push_java(holder);
                            text.push_str(" has no scores to show");
                        }),
                        feedback,
                    );
                } else {
                    send_success!(
                        silent,
                        feedback_text(|text| {
                            text.push_java(holder);
                            text.push_str(" has ");
                            text.push_str(&count.to_string());
                            text.push_str(" score(s):");
                        }),
                        feedback,
                    );
                    for (objective, value) in scores {
                        send_success!(
                            silent,
                            feedback_text(|text| {
                                push_formatted_objective(text, objective);
                                text.push_str(": ");
                                text.push_str(&value.to_string());
                            }),
                            feedback,
                        );
                    }
                }
            }
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::ListPlayerScores {
            holder: ScoreHolderSet::Wildcard,
        } => no_score_holders(),
        ScoreboardCommand::SetScore {
            holders,
            objective,
            value,
        } => {
            let Some(resolved) = scoreboard.resolve_holders(holders) else {
                return no_score_holders();
            };
            if !scoreboard.contains_objective(objective) {
                return unknown_objective(objective);
            }
            let total = scoreboard
                .set_scores(holders, objective, *value)
                .expect("holders and objective were validated");
            let feedback_holders = if *value == 0 {
                &[][..]
            } else {
                resolved.as_slice()
            };
            send_success!(
                silent,
                score_change_feedback(
                    feedback_holders,
                    |text| {
                        text.push_str("Set ");
                        push_formatted_objective(text, objective);
                        text.push_str(" for ");
                    },
                    |text| {
                        text.push_str(" to ");
                        text.push_str(&value.to_string());
                    }
                ),
                feedback,
            );
            OrdinaryExecution::success(total)
        }
        ScoreboardCommand::GetScore {
            holder: ScoreHolderSet::Named(holder),
            objective,
        } => {
            if !scoreboard.contains_objective(objective) {
                return unknown_objective(objective);
            }
            let Some(value) = scoreboard.score(holder, objective) else {
                return OrdinaryExecution::failure(feedback_text(|text| {
                    text.push_str("Can't get value of ");
                    text.push_str(objective);
                    text.push_str(" for ");
                    text.push_java(holder);
                    text.push_str("; none is set");
                }));
            };
            send_success!(
                silent,
                feedback_text(|text| {
                    text.push_java(holder);
                    text.push_str(" has ");
                    text.push_str(&value.to_string());
                    text.push_str(" ");
                    push_formatted_objective(text, objective);
                }),
                feedback,
            );
            OrdinaryExecution::success(value)
        }
        ScoreboardCommand::GetScore {
            holder: ScoreHolderSet::Wildcard,
            ..
        } => no_score_holders(),
        ScoreboardCommand::AddScore {
            holders,
            objective,
            value,
        } => execute_score_delta(
            scoreboard, holders, objective, *value, true, silent, feedback,
        ),
        ScoreboardCommand::RemoveScore {
            holders,
            objective,
            value,
        } => execute_score_delta(
            scoreboard, holders, objective, *value, false, silent, feedback,
        ),
        ScoreboardCommand::ResetScores { holders, objective } => {
            let Some(resolved) = scoreboard.resolve_holders(holders) else {
                return no_score_holders();
            };
            if objective
                .as_ref()
                .is_some_and(|objective| !scoreboard.contains_objective(objective))
            {
                return unknown_objective(objective.as_ref().expect("an objective was present"));
            }
            let count = scoreboard
                .reset_scores(holders, objective.as_deref())
                .expect("holders and objective were validated");
            send_success!(
                silent,
                score_change_feedback(
                    &resolved,
                    |text| match objective {
                        Some(objective) => {
                            text.push_str("Reset ");
                            push_formatted_objective(text, objective);
                            text.push_str(" for ");
                        }
                        None => text.push_str("Reset all scores for "),
                    },
                    |_| {},
                ),
                feedback,
            );
            OrdinaryExecution::success(count)
        }
        ScoreboardCommand::Operation {
            targets,
            target_objective,
            operation,
            sources,
            source_objective,
        } => {
            let Some(resolved_targets) = scoreboard.resolve_holders(targets) else {
                return no_score_holders();
            };
            if !scoreboard.contains_objective(target_objective) {
                return unknown_objective(target_objective);
            }
            if scoreboard.resolve_holders(sources).is_none() {
                return no_score_holders();
            }
            if !scoreboard.contains_objective(source_objective) {
                return unknown_objective(source_objective);
            }
            let Some(total) = scoreboard.apply_operation(
                targets,
                target_objective,
                *operation,
                sources,
                source_objective,
            ) else {
                return OrdinaryExecution::failure(literal_feedback("Cannot divide by zero"));
            };
            send_success!(
                silent,
                if let [holder] = resolved_targets.as_slice() {
                    feedback_text(|text| {
                        text.push_str("Set ");
                        push_formatted_objective(text, target_objective);
                        text.push_str(" for ");
                        text.push_java(holder);
                        text.push_str(" to ");
                        text.push_str(&total.to_string());
                    })
                } else {
                    feedback_text(|text| {
                        text.push_str("Updated ");
                        push_formatted_objective(text, target_objective);
                        text.push_str(" for ");
                        text.push_str(&resolved_targets.len().to_string());
                        text.push_str(" entities");
                    })
                },
                feedback,
            );
            OrdinaryExecution::success(total)
        }
    }
}

fn push_formatted_objective(text: &mut FeedbackTextBuilder, objective: &str) {
    text.push_str("[");
    text.push_str(objective);
    text.push_str("]");
}

fn unknown_objective(objective: &str) -> OrdinaryExecution {
    OrdinaryExecution::failure(literal_feedback(&format!(
        "Unknown scoreboard objective '{objective}'"
    )))
}

fn no_score_holders() -> OrdinaryExecution {
    OrdinaryExecution::failure(literal_feedback("No relevant score holders could be found"))
}

fn score_change_feedback(
    holders: &[JavaString],
    prefix: impl FnOnce(&mut FeedbackTextBuilder),
    suffix: impl FnOnce(&mut FeedbackTextBuilder),
) -> FeedbackText {
    feedback_text(|text| {
        prefix(text);
        if let [holder] = holders {
            text.push_java(holder);
        } else {
            text.push_str(&holders.len().to_string());
            text.push_str(" entities");
        }
        suffix(text);
    })
}

fn execute_score_delta(
    scoreboard: &mut Scoreboard,
    holders: &ScoreHolderSet,
    objective: &str,
    value: i32,
    add: bool,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    let Some(resolved) = scoreboard.resolve_holders(holders) else {
        return no_score_holders();
    };
    if !scoreboard.contains_objective(objective) {
        return unknown_objective(objective);
    }
    let total = if add {
        scoreboard.add_scores(holders, objective, value)
    } else {
        scoreboard.remove_scores(holders, objective, value)
    }
    .expect("holders and objective were validated");
    send_success!(
        silent,
        score_change_feedback(
            &resolved,
            |text| {
                text.push_str(if add { "Added " } else { "Removed " });
                text.push_str(&value.to_string());
                text.push_str(if add { " to " } else { " from " });
                push_formatted_objective(text, objective);
                text.push_str(" for ");
            },
            |text| {
                if resolved.len() == 1 {
                    text.push_str(" (now ");
                    text.push_str(&total.to_string());
                    text.push_str(")");
                }
            },
        ),
        feedback,
    );
    OrdinaryExecution::success(total)
}

fn execute_condition(
    scoreboard: &Scoreboard,
    condition: &ScoreCondition,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    let references = match &condition.predicate {
        ScorePredicate::Compare { left, right, .. } => [Some(left), Some(right)],
        ScorePredicate::Matches { score, .. } => [Some(score), None],
    };
    for reference in references.into_iter().flatten() {
        if matches!(reference.holder, ScoreHolderSet::Wildcard) {
            return no_score_holders();
        }
        if !scoreboard.contains_objective(&reference.objective) {
            return unknown_objective(&reference.objective);
        }
    }
    if scoreboard.evaluate_condition(condition) == Some(true) {
        send_success!(silent, literal_feedback("Test passed"), feedback);
        OrdinaryExecution::success(1)
    } else {
        OrdinaryExecution::failure(literal_feedback("Test failed"))
    }
}

fn stopwatch_condition_matches(
    stopwatches: &StopwatchState,
    condition: &StopwatchCondition,
) -> Option<bool> {
    let elapsed_seconds = stopwatches.elapsed_seconds(&condition.id)?;
    let at_or_above_min = condition
        .range
        .min
        .is_none_or(|minimum| elapsed_seconds >= minimum);
    let at_or_below_max = condition
        .range
        .max
        .is_none_or(|maximum| elapsed_seconds <= maximum);
    Some((at_or_above_min && at_or_below_max) == condition.expected)
}

fn missing_stopwatch(id: &Identifier) -> FeedbackText {
    literal_feedback(&format!("Stopwatch '{id}' does not exist"))
}

fn execute_stopwatch_command(
    stopwatches: &mut StopwatchState,
    command: &StopwatchCommand,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    match command {
        StopwatchCommand::Create { id } => {
            if !stopwatches.create(id.clone()) {
                return OrdinaryExecution::failure(literal_feedback(&format!(
                    "Stopwatch '{id}' already exists"
                )));
            }
            send_success!(
                silent,
                literal_feedback(&format!("Created stopwatch '{id}'")),
                feedback,
            );
            OrdinaryExecution::success(1)
        }
        StopwatchCommand::Query { id, scale } => {
            let Some(query) = stopwatches.query(id, *scale) else {
                return OrdinaryExecution::failure(missing_stopwatch(id));
            };
            send_success!(
                silent,
                literal_feedback(&format!(
                    "Stopwatch '{id}' has run for {}s",
                    java_f64(query.elapsed_seconds)
                )),
                feedback,
            );
            OrdinaryExecution::success(query.result)
        }
        StopwatchCommand::Restart { id } => {
            if !stopwatches.restart(id) {
                return OrdinaryExecution::failure(missing_stopwatch(id));
            }
            send_success!(
                silent,
                literal_feedback(&format!("Restarted stopwatch '{id}'")),
                feedback,
            );
            OrdinaryExecution::success(1)
        }
        StopwatchCommand::Remove { id } => {
            if !stopwatches.remove(id) {
                return OrdinaryExecution::failure(missing_stopwatch(id));
            }
            send_success!(
                silent,
                literal_feedback(&format!("Removed stopwatch '{id}'")),
                feedback,
            );
            OrdinaryExecution::success(1)
        }
    }
}

fn execute_stopwatch_condition(
    stopwatches: &StopwatchState,
    condition: &StopwatchCondition,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    let Some(matches) = stopwatch_condition_matches(stopwatches, condition) else {
        return OrdinaryExecution::failure(missing_stopwatch(&condition.id));
    };
    if matches {
        send_success!(silent, literal_feedback("Test passed"), feedback);
        OrdinaryExecution::success(1)
    } else {
        OrdinaryExecution::failure(literal_feedback("Test failed"))
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_predicate_condition(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    condition: &PredicateCondition,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> Result<OrdinaryExecution, String> {
    let matches = program.loot_registry().test_predicate(
        &condition.predicate,
        scoreboard,
        command_storage,
        execution_context,
        random,
    )? == condition.expected;
    Ok(if matches {
        send_success!(silent, literal_feedback("Test passed"), feedback);
        OrdinaryExecution::success(1)
    } else {
        OrdinaryExecution::failure(literal_feedback("Test failed"))
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_compute_command(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    command: &ComputeCommand,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> Result<OrdinaryExecution, String> {
    let providers = program.loot_registry();
    let (result, rounded_from) = match command.mode {
        ComputeMode::Float { scale } => {
            let original = providers.get_float(
                &command.provider,
                scoreboard,
                command_storage,
                execution_context,
                random,
            )?;
            let result = (original * scale).floor() as i32;
            (result, (result as f32 != original).then_some(original))
        }
        ComputeMode::Integer => (
            providers.get_int(
                &command.provider,
                scoreboard,
                command_storage,
                execution_context,
                random,
            )?,
            None,
        ),
    };
    let named = match &command.provider {
        crate::number_provider::NumberProviderReference::Named(id) => Some(id),
        crate::number_provider::NumberProviderReference::Inline(_) => None,
    };
    send_success!(
        silent,
        feedback_text(|text| {
            if let Some(id) = named {
                text.push_str(&id.to_string());
                text.push_str(" returned value ");
            } else {
                text.push_str("Number provider returned value ");
            }
            if let Some(original) = rounded_from {
                text.push_str(&java_f32(original));
                text.push_str(" (rounded to ");
                text.push_str(&result.to_string());
                text.push_str(")");
            } else {
                text.push_str(&result.to_string());
            }
        }),
        feedback,
    );
    Ok(OrdinaryExecution::success(result))
}

fn execute_random_command(
    random: &mut RandomState,
    command: &RandomCommand,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    let (random, sequences) = random.parts();
    match command {
        RandomCommand::Value { range, sequence } => {
            if let Some(sequence) = sequence {
                sequences.materialize(sequence);
            }
            let min = range.min.unwrap_or(i32::MIN);
            let max = range.max.unwrap_or(i32::MAX);
            let span = i64::from(max) - i64::from(min);
            if span == 0 {
                return OrdinaryExecution::failure(literal_feedback(
                    "The range of the random value must be at least 1",
                ));
            }
            if span >= i64::from(i32::MAX) {
                return OrdinaryExecution::failure(literal_feedback(
                    "The range of the random value must be at most 2147483647",
                ));
            }
            let bound = i32::try_from(span + 1)
                .expect("an accepted random range has a positive Java int bound");
            let offset = match sequence {
                Some(sequence) => sequences.next_int(sequence, bound),
                None => random
                    .next_int(bound)
                    .expect("an accepted random range has a positive bound"),
            };
            let value = min + offset;
            send_success!(
                silent,
                literal_feedback(&format!("Randomized value: {value}")),
                feedback,
            );
            OrdinaryExecution::success(value)
        }
        RandomCommand::Reset { sequence, settings } => {
            sequences.reset(sequence.clone(), *settings);
            send_success!(
                silent,
                literal_feedback(&format!("Reset random sequence {sequence}")),
                feedback,
            );
            OrdinaryExecution::success(1)
        }
        RandomCommand::ResetAll { settings } => {
            let count = match settings {
                Some(settings) => sequences.set_defaults_and_clear(*settings),
                None => sequences.clear(),
            };
            let count =
                i32::try_from(count).expect("the number of random sequences fits in a Java int");
            send_success!(
                silent,
                literal_feedback(&format!("Reset {count} random sequence(s)")),
                feedback,
            );
            OrdinaryExecution::success(count)
        }
    }
}

fn predicate_evaluation_failed(reason: String) -> ExecutionError {
    ExecutionError::PredicateEvaluationFailed { reason }
}

fn stored_command_value(kind: StoreKind, result: CommandResult) -> i32 {
    match kind {
        StoreKind::Result => result.value,
        StoreKind::Success => i32::from(result.success),
    }
}

fn storage_number(number_type: StorageNumberType, value: f64) -> Tag {
    match number_type {
        StorageNumberType::Byte => Tag::Byte((value as i32) as i8),
        StorageNumberType::Short => Tag::Short((value as i32) as i16),
        StorageNumberType::Int => Tag::Int(value as i32),
        StorageNumberType::Long => Tag::Long(value as i64),
        StorageNumberType::Float => Tag::float(value as f32),
        StorageNumberType::Double => Tag::double(value),
    }
}

fn storage_condition_matches(
    command_storage: &CommandStorage,
    condition: &StorageCondition,
) -> bool {
    let value = command_storage.get(&condition.storage);
    (condition.path.count_matching(&value) > 0) == condition.expected
}

fn execute_storage_condition(
    command_storage: &CommandStorage,
    condition: &StorageCondition,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> OrdinaryExecution {
    let value = command_storage.get(&condition.storage);
    let count = condition.path.count_matching(&value);
    if condition.expected {
        if count == 0 {
            OrdinaryExecution::failure(literal_feedback("Test failed"))
        } else {
            let count = i32::try_from(count).expect("an NBT match collection fits in a Java int");
            send_success!(
                silent,
                literal_feedback(&format!("Test passed. Count: {count}")),
                feedback,
            );
            OrdinaryExecution::success(count)
        }
    } else if count == 0 {
        send_success!(silent, literal_feedback("Test passed"), feedback);
        OrdinaryExecution::success(1)
    } else {
        OrdinaryExecution::failure(literal_feedback(&format!("Test failed. Count: {count}")))
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_data_command(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &mut CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    command: &DataCommand,
    silent: bool,
    feedback: &mut impl FnMut(CommandFeedback),
) -> Result<OrdinaryExecution, String> {
    let result = match command {
        DataCommand::Merge { storage, value } => {
            let result = match merge_storage(command_storage, storage, value) {
                Ok(result) => result,
                Err(reason) => return Ok(OrdinaryExecution::failure(data_failure(&reason, None))),
            };
            send_success!(silent, modified_storage_feedback(storage), feedback);
            result
        }
        DataCommand::Get { storage } => {
            send_success!(
                silent,
                storage_query_feedback(storage, &Tag::Compound(command_storage.get(storage))),
                feedback,
            );
            1
        }
        DataCommand::GetPath {
            storage,
            path,
            scale,
        } => {
            let root = command_storage.get(storage);
            let values = match path.get_with_not_found(&root) {
                Ok(values) => values,
                Err(prefix) => {
                    return Ok(OrdinaryExecution::failure(feedback_text(|text| {
                        text.push_str("Found no elements matching ");
                        text.push_java(&prefix);
                    })));
                }
            };
            let [value] = values.as_slice() else {
                return Ok(OrdinaryExecution::failure(literal_feedback(
                    "This argument accepts a single NBT value",
                )));
            };
            match scale {
                Some(scale) => {
                    let Some(number) = value.double_value() else {
                        return Ok(OrdinaryExecution::failure(feedback_text(|text| {
                            text.push_str("Can't get ");
                            text.push_java(path.original());
                            text.push_str("; only numeric tags are allowed");
                        })));
                    };
                    let result = minecraft_floor_to_i32(number * scale);
                    send_success!(
                        silent,
                        feedback_text(|text| {
                            text.push_java(path.original());
                            text.push_str(" in storage ");
                            text.push_str(&storage.to_string());
                            text.push_str(" after scale factor of ");
                            text.push_str(&format_scale(*scale));
                            text.push_str(" is ");
                            text.push_str(&result.to_string());
                        }),
                        feedback,
                    );
                    result
                }
                None => {
                    let result = data_value_result(value).ok_or_else(|| {
                        "a supported NBT get value must have a Minecraft result".to_owned()
                    })?;
                    send_success!(silent, storage_query_feedback(storage, value), feedback);
                    result
                }
            }
        }
        DataCommand::Remove { storage, path } => {
            let result = command_storage.edit(storage, |data| {
                let count = path.remove(data);
                (count != 0)
                    .then_some(count)
                    .ok_or_else(|| "NBT path removed nothing".to_owned())
            });
            match result {
                Ok(result) => {
                    send_success!(silent, modified_storage_feedback(storage), feedback);
                    result
                }
                Err(reason) => {
                    return Ok(OrdinaryExecution::failure(data_failure(
                        &reason,
                        Some(path),
                    )));
                }
            }
        }
        DataCommand::Modify {
            storage,
            path,
            operation,
            source,
        } => {
            let source_values = match resolve_data_source(
                program,
                scoreboard,
                command_storage,
                execution_context,
                random,
                source,
            ) {
                Ok(values) => values,
                Err(DataSourceError::Evaluation(reason)) => return Err(reason),
                Err(DataSourceError::Command(message)) => {
                    return Ok(OrdinaryExecution::failure(message));
                }
            };
            match modify_storage(command_storage, storage, path, *operation, &source_values) {
                Ok(result) => {
                    send_success!(silent, modified_storage_feedback(storage), feedback);
                    result
                }
                Err(message) => return Ok(OrdinaryExecution::failure(message)),
            }
        }
    };
    Ok(OrdinaryExecution::success(result))
}

fn modified_storage_feedback(storage: &Identifier) -> FeedbackText {
    literal_feedback(&format!("Modified storage {storage}"))
}

fn storage_query_feedback(storage: &Identifier, value: &Tag) -> FeedbackText {
    feedback_text(|text| {
        text.push_str("Storage ");
        text.push_str(&storage.to_string());
        text.push_str(" has the following contents: ");
        text.push_java(&value.pretty_stringify());
    })
}

fn data_value_result(value: &Tag) -> Option<i32> {
    if let Some(value) = value.double_value() {
        return Some(minecraft_floor_to_i32(value));
    }
    if let Some(length) = value.collection_len() {
        return i32::try_from(length).ok();
    }
    match value {
        Tag::String(value) => i32::try_from(value.len()).ok(),
        Tag::Compound(value) => i32::try_from(value.len()).ok(),
        _ => None,
    }
}

fn format_scale(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else {
        java_fixed_2(value)
    }
}

fn java_fixed_2(value: f64) -> String {
    let decimal = java_f64(value);
    let (negative, unsigned) = decimal
        .strip_prefix('-')
        .map_or((false, decimal.as_str()), |unsigned| (true, unsigned));
    let (mantissa, exponent) =
        unsigned
            .split_once('E')
            .map_or((unsigned, 0_i32), |(mantissa, exponent)| {
                (
                    mantissa,
                    exponent
                        .parse::<i32>()
                        .expect("Java floating-point text has an integer exponent"),
                )
            });
    let decimal_point = mantissa
        .find('.')
        .map_or(mantissa.len() as i32, |position| position as i32)
        + exponent;
    let mut digits = mantissa
        .bytes()
        .filter(|unit| *unit != b'.')
        .collect::<Vec<_>>();
    let adjustment = decimal_point - i32::try_from(digits.len()).expect("f64 text is short") + 2;
    if adjustment >= 0 {
        digits.extend(std::iter::repeat_n(
            b'0',
            usize::try_from(adjustment).expect("the adjustment is non-negative"),
        ));
    } else {
        let removed = usize::try_from(-adjustment).expect("the adjustment is negative");
        let retained = digits.len().saturating_sub(removed);
        let round_up = removed <= digits.len() && digits[retained] >= b'5';
        digits.truncate(retained);
        if round_up {
            increment_decimal_digits(&mut digits);
        }
    }
    while digits.len() < 3 {
        digits.insert(0, b'0');
    }
    let decimal_index = digits.len() - 2;
    let mut result = String::with_capacity(digits.len() + 2);
    if negative {
        result.push('-');
    }
    result
        .push_str(std::str::from_utf8(&digits[..decimal_index]).expect("decimal digits are ASCII"));
    result.push('.');
    result
        .push_str(std::str::from_utf8(&digits[decimal_index..]).expect("decimal digits are ASCII"));
    result
}

fn increment_decimal_digits(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit != b'9' {
            *digit += 1;
            return;
        }
        *digit = b'0';
    }
    digits.insert(0, b'1');
}

fn data_failure(reason: &str, path: Option<&crate::nbt::NbtPath>) -> FeedbackText {
    match reason {
        "NBT data is too deep" => literal_feedback("Resulting NBT too deeply nested"),
        "NBT merge changed nothing" | "NBT path removed nothing" => {
            literal_feedback("Nothing changed. The specified properties already have these values")
        }
        "nothing found at NBT path" => path.map_or_else(
            || literal_feedback("Found no elements matching the NBT path"),
            |path| {
                feedback_text(|text| {
                    text.push_str("Found no elements matching ");
                    text.push_java(path.original());
                })
            },
        ),
        _ if reason.starts_with("invalid list index ") => {
            literal_feedback(&reason.replacen("invalid list index", "Invalid list index:", 1))
        }
        _ => literal_feedback(reason),
    }
}

fn merge_storage(
    command_storage: &mut CommandStorage,
    storage: &Identifier,
    value: &crate::nbt::CompoundTag,
) -> Result<i32, String> {
    if Tag::Compound(value.clone()).is_too_deep(0) {
        return Err("NBT data is too deep".to_owned());
    }
    let old = command_storage.get(storage);
    let mut merged = old.clone();
    merged.merge(value);
    if merged == old {
        return Err("NBT merge changed nothing".to_owned());
    }
    command_storage.set(storage.clone(), merged);
    Ok(1)
}

fn modify_storage(
    command_storage: &mut CommandStorage,
    storage: &Identifier,
    path: &crate::nbt::NbtPath,
    operation: DataModifyOperation,
    source: &[Tag],
) -> Result<i32, FeedbackText> {
    command_storage.edit(storage, |target| {
        let changed = match operation {
            DataModifyOperation::Insert(index) => path
                .insert(index, target, source)
                .map_err(nbt_edit_failure)?,
            DataModifyOperation::Set => path
                .set(
                    target,
                    source
                        .last()
                        .expect("data modification sources contain at least one value")
                        .clone(),
                )
                .map_err(nbt_edit_failure)?,
            DataModifyOperation::Merge => path.merge(target, source).map_err(nbt_edit_failure)?,
        };
        (changed != 0).then_some(changed).ok_or_else(|| {
            literal_feedback("Nothing changed. The specified properties already have these values")
        })
    })
}

fn nbt_edit_failure(error: NbtEditError) -> FeedbackText {
    match error {
        NbtEditError::DataTooDeep => literal_feedback("Resulting NBT too deeply nested"),
        NbtEditError::NothingFound(prefix) => feedback_text(|text| {
            text.push_str("Found no elements matching ");
            text.push_java(&prefix);
        }),
        NbtEditError::ExpectedList(value) => feedback_text(|text| {
            text.push_str("Expected a list: got ");
            text.push_java(&value.compact_stringify());
        }),
        NbtEditError::ExpectedObject(value) => feedback_text(|text| {
            text.push_str("Expected an object: got ");
            text.push_java(&value.compact_stringify());
        }),
        NbtEditError::InvalidListIndex(index) => {
            literal_feedback(&format!("Invalid list index: {index}"))
        }
        NbtEditError::Other(reason) => literal_feedback(&reason),
    }
}

enum DataSourceError {
    Command(FeedbackText),
    Evaluation(String),
}

fn resolve_data_source(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    source: &DataSource,
) -> Result<Vec<Tag>, DataSourceError> {
    match source {
        DataSource::Value(value) => Ok(vec![value.clone()]),
        DataSource::Storage { storage, path } => {
            let root = command_storage.get(storage);
            path.as_ref().map_or_else(
                || Ok(vec![Tag::Compound(root.clone())]),
                |path| {
                    path.get_with_not_found(&root).map_err(|prefix| {
                        DataSourceError::Command(feedback_text(|text| {
                            text.push_str("Found no elements matching ");
                            text.push_java(&prefix);
                        }))
                    })
                },
            )
        }
        DataSource::String {
            storage,
            path,
            substring,
        } => {
            let root = command_storage.get(storage);
            let values = path.as_ref().map_or_else(
                || Ok(vec![Tag::Compound(root.clone())]),
                |path| {
                    path.get_with_not_found(&root).map_err(|prefix| {
                        DataSourceError::Command(feedback_text(|text| {
                            text.push_str("Found no elements matching ");
                            text.push_java(&prefix);
                        }))
                    })
                },
            )?;
            values
                .into_iter()
                .map(|value| {
                    let Some(text_value) = value.primitive_text() else {
                        return Err(DataSourceError::Command(feedback_text(|text| {
                            text.push_str("Expected a value: got ");
                            text.push_java(&value.compact_stringify());
                        })));
                    };
                    let value = if let Some(range) = substring {
                        let length = i32::try_from(text_value.len()).unwrap_or(i32::MAX);
                        let start = if range.start < 0 {
                            length.saturating_add(range.start)
                        } else {
                            range.start
                        };
                        let end = range.end.map_or(length, |end| {
                            if end < 0 {
                                length.saturating_add(end)
                            } else {
                                end
                            }
                        });
                        text_value.substring(range.start, range.end).map_err(|_| {
                            DataSourceError::Command(literal_feedback(&format!(
                                "Invalid substring indices: {start} to {end}"
                            )))
                        })?
                    } else {
                        text_value
                    };
                    Ok(Tag::String(value))
                })
                .collect()
        }
        DataSource::Compute { provider, integer } => {
            let providers = program.loot_registry();
            Ok(vec![if *integer {
                Tag::Int(
                    providers
                        .get_int(
                            provider,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )
                        .map_err(DataSourceError::Evaluation)?,
                )
            } else {
                Tag::float(
                    providers
                        .get_float(
                            provider,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )
                        .map_err(DataSourceError::Evaluation)?,
                )
            }])
        }
    }
}

fn minecraft_floor_to_i32(value: f64) -> i32 {
    value.floor() as i32
}

fn schedule_next_instruction(queue: &mut VecDeque<QueueEntry>, frame: Frame) {
    if frame.next_instruction < frame.function.len() {
        queue.push_front(QueueEntry::Step(frame));
    }
}

fn discard_at_depth_or_higher(queue: &mut VecDeque<QueueEntry>, depth: usize) {
    queue.retain(|entry| entry.depth() < depth);
}

fn instantiate_function(
    function: &Function,
    arguments: Option<&crate::nbt::CompoundTag>,
    compiler: &mut Option<CommandCompiler>,
    loot_registry: &Arc<crate::number_provider::LootRegistry>,
) -> Result<Arc<[Instruction]>, FunctionInstantiationError> {
    match function {
        Function::Plain(instructions) => Ok(Arc::clone(instructions)),
        Function::Macro(function) => function.instantiate(arguments, |command| {
            compiler
                .get_or_insert_with(|| {
                    CommandCompiler::with_loot_registry(Arc::clone(loot_registry))
                })
                .compile_utf16(command)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::format_scale;

    #[test]
    fn storage_scale_uses_java_fixed_point_rounding() {
        assert_eq!(format_scale(2.675), "2.68");
        assert_eq!(format_scale(1.005), "1.01");
        assert_eq!(format_scale(-1.005), "-1.01");
        assert_eq!(format_scale(9.995), "10.00");
        assert_eq!(format_scale(0.0049), "0.00");
        assert_eq!(format_scale(-0.0), "-0.00");
        assert_eq!(format_scale(10_000_000.0), "10000000.00");
    }
}
