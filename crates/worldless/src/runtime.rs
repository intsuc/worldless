use std::{cell::Cell, collections::VecDeque, error::Error, fmt, rc::Rc, sync::Arc};

use crate::{
    execution_context::ExecutionContext,
    loader::CommandCompiler,
    macro_function::Function,
    nbt::{CommandStorage, CompoundTag, JavaString, Tag},
    program::{
        Command, ComputeCommand, ComputeMode, DataCommand, DataModifyOperation, DataSource,
        FunctionArguments, Instruction, Modifier, ObjectiveId, PredicateCondition, Program,
        RandomCommand, ResolvedFunctions, ScoreCondition, ScoreHolderSet, Scoreboard,
        ScoreboardCommand, StorageCondition, StorageNumberType, StoreKind,
    },
    random::{LegacyRandom, RandomState},
    resource::{FunctionReference, Identifier},
};

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
    MissingWorldSeed { sequence: String },
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
            Self::MissingWorldSeed { sequence } => {
                write!(
                    formatter,
                    "random sequence `{sequence}` requires a configured world seed"
                )
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
        functions: Arc<[Arc<[Instruction]>]>,
        next_function: usize,
        consumer: ResultConsumer,
        result: Option<Rc<Cell<Option<i32>>>>,
    },
    Fallthrough {
        depth: usize,
        discard_depth: usize,
        result_consumer: ResultConsumer,
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
    input: &str,
    arguments: Option<&CompoundTag>,
    context: ExecutionContext,
    command_limit: usize,
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
        instruction,
        None,
        context,
        command_limit,
    )
}

pub(crate) fn execute_command(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    input: &str,
    context: ExecutionContext,
    command_limit: usize,
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
        instruction,
        Some(compiler),
        context,
        command_limit,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_instruction(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    random: &mut RandomState,
    instruction: Instruction,
    mut compiler: Option<CommandCompiler>,
    context: ExecutionContext,
    command_limit: usize,
) -> Result<ExecutionOutcome, ExecutionError> {
    let function = Arc::<[Instruction]>::from([instruction]);
    let mut queue = VecDeque::from([QueueEntry::Step(Frame {
        function,
        context,
        next_instruction: 0,
        depth: 0,
        discard_depth: 0,
        source_consumer: ResultConsumer::top_level(),
        result_consumer: ResultConsumer::top_level(),
    })]);
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
                            let Some((holders, objective)) =
                                scoreboard.resolve_holders(holders).and_then(|holders| {
                                    scoreboard
                                        .objective_id(objective)
                                        .map(|objective| (holders, objective))
                                })
                            else {
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
                                    holders,
                                    objective,
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
                            });
                            for function in condition_functions.into_iter().rev() {
                                queue.push_front(QueueEntry::Call(Frame {
                                    function,
                                    context,
                                    next_instruction: 0,
                                    depth: isolated_depth,
                                    discard_depth: isolated_depth,
                                    source_consumer: ResultConsumer::empty(),
                                    result_consumer: result_consumer.clone(),
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
                                queue.push_front(QueueEntry::Fallthrough {
                                    depth: frame.depth,
                                    discard_depth: frame.discard_depth,
                                    result_consumer: frame.result_consumer,
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
                        queue.push_front(QueueEntry::Fallthrough {
                            depth: frame.depth,
                            discard_depth: frame.discard_depth,
                            result_consumer: frame.result_consumer,
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
                        frame.result_consumer.accept(
                            result,
                            scoreboard,
                            command_storage,
                            &mut top_level_result,
                        );
                        discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                    }
                    Command::Scoreboard(_)
                    | Command::Condition(_)
                    | Command::StorageCondition(_)
                    | Command::PredicateCondition(_)
                    | Command::Data(_)
                    | Command::Compute(_)
                    | Command::Random(_) => {
                        queue.push_front(QueueEntry::ExecuteOrdinary {
                            frame,
                            context,
                            instruction,
                            stores,
                            return_run,
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
            } => {
                quota.increment();
                let command = &frame.function[instruction].command;
                let result = match command {
                    Command::Scoreboard(command) => {
                        Some(execute_scoreboard_command(scoreboard, command))
                    }
                    Command::Condition(condition) => Some(execute_condition(scoreboard, condition)),
                    Command::StorageCondition(condition) => {
                        Some(execute_storage_condition(command_storage, condition))
                    }
                    Command::PredicateCondition(condition) => execute_predicate_condition(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        condition,
                    )
                    .map(Some)
                    .map_err(predicate_evaluation_failed)?,
                    Command::Data(command) => execute_data_command(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        command,
                    )
                    .map(Some)
                    .map_err(|reason| ExecutionError::NumberProviderEvaluationFailed { reason })?,
                    Command::Compute(command) => execute_compute_command(
                        program,
                        scoreboard,
                        command_storage,
                        &context,
                        random.unnamed(),
                        command,
                    )
                    .map(Some)
                    .map_err(|reason| ExecutionError::NumberProviderEvaluationFailed { reason })?,
                    Command::Random(command) => execute_random_command(random, command)?,
                    Command::Function { .. } | Command::Return { .. } => {
                        unreachable!("only ordinary commands are queued for ordinary execution")
                    }
                };
                let Some(result) = result else {
                    if !return_run {
                        schedule_next_instruction(&mut queue, frame);
                    }
                    continue;
                };
                frame.source_consumer.with_prefix(stores).accept(
                    result,
                    scoreboard,
                    command_storage,
                    &mut top_level_result,
                );

                if return_run {
                    frame.result_consumer.accept(
                        result,
                        scoreboard,
                        command_storage,
                        &mut top_level_result,
                    );
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
                let Some(function) = functions.get(next_function).map(Arc::clone) else {
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
                queue.push_front(QueueEntry::ContinueFunctionTag {
                    frame,
                    context,
                    functions,
                    next_function: next_function + 1,
                    consumer,
                    result,
                });
                queue.push_front(QueueEntry::Call(Frame {
                    function,
                    context,
                    next_instruction: 0,
                    depth: child_depth,
                    discard_depth: child_depth,
                    source_consumer: ResultConsumer::empty(),
                    result_consumer,
                }));
            }
            QueueEntry::Fallthrough {
                discard_depth,
                result_consumer,
                ..
            } => {
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
) {
    let functions = match program.resolve_functions(reference) {
        Some(ResolvedFunctions::Tag([])) | None => {
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
        Err(()) => {
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
    let (instances, failed) =
        instantiate_resolved_prefix(program, functions, arguments.as_ref(), compiler);
    if failed {
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
        queue_single_function(
            queue,
            frame,
            context,
            Arc::clone(
                instances
                    .first()
                    .expect("a resolved single function produces one instance"),
            ),
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

fn resolve_function_arguments(
    source: Option<&FunctionArguments>,
    command_storage: &CommandStorage,
) -> Result<Option<crate::nbt::CompoundTag>, ()> {
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
            let mut selected = path.get(&root).map_err(|_| ())?;
            if selected.len() != 1 {
                return Err(());
            }
            match selected.pop().expect("one NBT value was selected") {
                Tag::Compound(arguments) => Ok(Some(arguments)),
                _ => Err(()),
            }
        }
    }
}

fn instantiate_resolved_prefix(
    program: &Program,
    functions: ResolvedFunctions<'_>,
    arguments: Option<&crate::nbt::CompoundTag>,
    compiler: &mut Option<CommandCompiler>,
) -> (Vec<Arc<[Instruction]>>, bool) {
    let mut instances = Vec::new();
    match functions {
        ResolvedFunctions::Single(function) => {
            match instantiate_function(function, arguments, compiler, program.loot_registry()) {
                Ok(instance) => instances.push(instance),
                Err(_) => return (instances, true),
            }
        }
        ResolvedFunctions::Tag(functions) => {
            for id in functions {
                let function = program
                    .function(id)
                    .expect("resolved function tags contain loaded functions");
                match instantiate_function(function, arguments, compiler, program.loot_registry()) {
                    Ok(instance) => instances.push(instance),
                    Err(_) => return (instances, true),
                }
            }
        }
    }
    (instances, false)
}

#[allow(clippy::too_many_arguments)]
fn fail_function_command(
    queue: &mut VecDeque<QueueEntry>,
    frame: Frame,
    context: ExecutionContext,
    instances: Vec<Arc<[Instruction]>>,
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
        for function in instances.into_iter().rev() {
            queue.push_front(QueueEntry::Call(Frame {
                function,
                context,
                next_instruction: 0,
                depth: child_depth,
                discard_depth,
                source_consumer: ResultConsumer::empty(),
                result_consumer: child_consumer.clone(),
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
    function: Arc<[Instruction]>,
    command_consumer: ResultConsumer,
    return_run: bool,
) {
    let child_depth = frame.depth + 1;
    if return_run {
        let parent_consumer = frame.result_consumer;
        let child_consumer = command_consumer.chain(&parent_consumer);
        let child = Frame {
            function,
            context,
            next_instruction: 0,
            depth: child_depth,
            discard_depth: frame.discard_depth,
            source_consumer: ResultConsumer::empty(),
            result_consumer: child_consumer,
        };
        queue.push_front(QueueEntry::Fallthrough {
            depth: frame.depth,
            discard_depth: frame.discard_depth,
            result_consumer: parent_consumer,
        });
        queue.push_front(QueueEntry::Call(child));
    } else {
        let child = Frame {
            function,
            context,
            next_instruction: 0,
            depth: child_depth,
            discard_depth: child_depth,
            source_consumer: ResultConsumer::empty(),
            result_consumer: command_consumer,
        };
        schedule_next_instruction(queue, frame);
        queue.push_front(QueueEntry::Call(child));
    }
}

fn queue_function_tag(
    queue: &mut VecDeque<QueueEntry>,
    frame: Frame,
    context: ExecutionContext,
    functions: Vec<Arc<[Instruction]>>,
    command_consumer: ResultConsumer,
    return_run: bool,
) {
    if return_run {
        let child_depth = frame.depth + 1;
        let fallback_depth = frame.depth;
        let discard_depth = frame.discard_depth;
        let parent_consumer = frame.result_consumer;
        let child_consumer = command_consumer.chain(&parent_consumer);
        queue.push_front(QueueEntry::Fallthrough {
            depth: fallback_depth,
            discard_depth,
            result_consumer: parent_consumer,
        });
        for function in functions.into_iter().rev() {
            queue.push_front(QueueEntry::Call(Frame {
                function,
                context,
                next_instruction: 0,
                depth: child_depth,
                discard_depth,
                source_consumer: ResultConsumer::empty(),
                result_consumer: child_consumer.clone(),
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
) -> CommandResult {
    match command {
        ScoreboardCommand::ListObjectives => CommandResult::success(scoreboard.list_objectives()),
        ScoreboardCommand::AddObjective { objective } => scoreboard
            .add_objective(objective)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::RemoveObjective { objective } => scoreboard
            .remove_objective(objective)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::ListPlayers => CommandResult::success(scoreboard.list_players()),
        ScoreboardCommand::ListPlayerScores {
            holder: ScoreHolderSet::Named(holder),
        } => CommandResult::success(scoreboard.list_player_scores(holder)),
        ScoreboardCommand::ListPlayerScores {
            holder: ScoreHolderSet::Wildcard,
        } => CommandResult::FAILURE,
        ScoreboardCommand::SetScore {
            holders,
            objective,
            value,
        } => scoreboard
            .set_scores(holders, objective, *value)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::GetScore {
            holder: ScoreHolderSet::Named(holder),
            objective,
        } => scoreboard
            .score(holder, objective)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::GetScore {
            holder: ScoreHolderSet::Wildcard,
            ..
        } => CommandResult::FAILURE,
        ScoreboardCommand::AddScore {
            holders,
            objective,
            value,
        } => scoreboard
            .add_scores(holders, objective, *value)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::RemoveScore {
            holders,
            objective,
            value,
        } => scoreboard
            .remove_scores(holders, objective, *value)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::ResetScores { holders, objective } => scoreboard
            .reset_scores(holders, objective.as_deref())
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::Operation {
            targets,
            target_objective,
            operation,
            sources,
            source_objective,
        } => scoreboard
            .apply_operation(
                targets,
                target_objective,
                *operation,
                sources,
                source_objective,
            )
            .map_or(CommandResult::FAILURE, CommandResult::success),
    }
}

fn execute_condition(scoreboard: &Scoreboard, condition: &ScoreCondition) -> CommandResult {
    if scoreboard.evaluate_condition(condition) == Some(true) {
        CommandResult::success(1)
    } else {
        CommandResult::FAILURE
    }
}

fn execute_predicate_condition(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    condition: &PredicateCondition,
) -> Result<CommandResult, String> {
    let matches = program.loot_registry().test_predicate(
        &condition.predicate,
        scoreboard,
        command_storage,
        execution_context,
        random,
    )? == condition.expected;
    Ok(if matches {
        CommandResult::success(1)
    } else {
        CommandResult::FAILURE
    })
}

fn execute_compute_command(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    command: &ComputeCommand,
) -> Result<CommandResult, String> {
    let providers = program.loot_registry();
    let value = match command.mode {
        ComputeMode::Float { scale } => providers
            .get_float(
                &command.provider,
                scoreboard,
                command_storage,
                execution_context,
                random,
            )
            .map(|value| (value * scale).floor() as i32),
        ComputeMode::Integer => providers.get_int(
            &command.provider,
            scoreboard,
            command_storage,
            execution_context,
            random,
        ),
    };
    value.map(CommandResult::success)
}

fn execute_random_command(
    random: &mut RandomState,
    command: &RandomCommand,
) -> Result<Option<CommandResult>, ExecutionError> {
    let (random, sequences) = random.parts();
    match command {
        RandomCommand::Value { range, sequence } => {
            if let Some(sequence) = sequence {
                sequences
                    .materialize(sequence)
                    .map_err(missing_world_seed)?;
            }
            let min = range.min.unwrap_or(i32::MIN);
            let max = range.max.unwrap_or(i32::MAX);
            let span = i64::from(max) - i64::from(min);
            if span == 0 || span >= i64::from(i32::MAX) {
                return Ok(None);
            }
            let bound = i32::try_from(span + 1)
                .expect("an accepted random range has a positive Java int bound");
            let offset = match sequence {
                Some(sequence) => sequences
                    .next_int(sequence, bound)
                    .map_err(missing_world_seed)?,
                None => random
                    .next_int(bound)
                    .expect("an accepted random range has a positive bound"),
            };
            Ok(Some(CommandResult::success(min + offset)))
        }
        RandomCommand::Reset { sequence, settings } => {
            sequences
                .reset(sequence.clone(), *settings)
                .map_err(missing_world_seed)?;
            Ok(Some(CommandResult::success(1)))
        }
        RandomCommand::ResetAll { settings } => {
            let count = match settings {
                Some(settings) => sequences.set_defaults_and_clear(*settings),
                None => sequences.clear(),
            };
            Ok(Some(CommandResult::success(i32::try_from(count).expect(
                "the number of random sequences fits in a Java int",
            ))))
        }
    }
}

fn missing_world_seed(error: crate::random::MissingWorldSeed) -> ExecutionError {
    ExecutionError::MissingWorldSeed {
        sequence: error.sequence().to_string(),
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
) -> CommandResult {
    let value = command_storage.get(&condition.storage);
    let count = condition.path.count_matching(&value);
    if condition.expected {
        if count == 0 {
            CommandResult::FAILURE
        } else {
            CommandResult::success(
                i32::try_from(count).expect("an NBT match collection fits in a Java int"),
            )
        }
    } else if count == 0 {
        CommandResult::success(1)
    } else {
        CommandResult::FAILURE
    }
}

fn execute_data_command(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &mut CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    command: &DataCommand,
) -> Result<CommandResult, String> {
    let result = match command {
        DataCommand::Merge { storage, value } => merge_storage(command_storage, storage, value),
        DataCommand::Get { .. } => Ok(1),
        DataCommand::GetPath {
            storage,
            path,
            scale,
        } => get_storage_path(command_storage, storage, path, *scale),
        DataCommand::Remove { storage, path } => command_storage.edit(storage, |data| {
            let count = path.remove(data);
            (count != 0)
                .then_some(count)
                .ok_or_else(|| "NBT path removed nothing".to_owned())
        }),
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
                Err(reason) if matches!(source, DataSource::Compute { .. }) => {
                    return Err(reason);
                }
                Err(_) => return Ok(CommandResult::FAILURE),
            };
            modify_storage(command_storage, storage, path, *operation, &source_values)
        }
    };
    Ok(result.map_or(CommandResult::FAILURE, CommandResult::success))
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

fn get_storage_path(
    command_storage: &CommandStorage,
    storage: &Identifier,
    path: &crate::nbt::NbtPath,
    scale: Option<f64>,
) -> Result<i32, String> {
    let root = command_storage.get(storage);
    let values = path.get(&root)?;
    let [value] = values.as_slice() else {
        return Err("an NBT get path must select exactly one value".to_owned());
    };
    if let Some(scale) = scale {
        return value
            .double_value()
            .map(|value| minecraft_floor_to_i32(value * scale))
            .ok_or_else(|| "scaled NBT get requires a number".to_owned());
    }
    if let Some(value) = value.double_value() {
        return Ok(minecraft_floor_to_i32(value));
    }
    if let Some(length) = value.collection_len() {
        return i32::try_from(length).map_err(|_| "NBT collection is too large".to_owned());
    }
    match value {
        Tag::String(value) => {
            i32::try_from(value.len()).map_err(|_| "NBT string is too large".to_owned())
        }
        Tag::Compound(value) => {
            i32::try_from(value.len()).map_err(|_| "NBT compound is too large".to_owned())
        }
        _ => Err("unsupported NBT get value".to_owned()),
    }
}

fn modify_storage(
    command_storage: &mut CommandStorage,
    storage: &Identifier,
    path: &crate::nbt::NbtPath,
    operation: DataModifyOperation,
    source: &[Tag],
) -> Result<i32, String> {
    command_storage.edit(storage, |target| {
        let changed = match operation {
            DataModifyOperation::Insert(index) => path.insert(index, target, source)?,
            DataModifyOperation::Set => path.set(
                target,
                source
                    .last()
                    .expect("data modification sources contain at least one value")
                    .clone(),
            )?,
            DataModifyOperation::Merge => path.merge(target, source)?,
        };
        (changed != 0)
            .then_some(changed)
            .ok_or_else(|| "NBT modification changed nothing".to_owned())
    })
}

fn resolve_data_source(
    program: &Program,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
    source: &DataSource,
) -> Result<Vec<Tag>, String> {
    match source {
        DataSource::Value(value) => Ok(vec![value.clone()]),
        DataSource::Storage { storage, path } => {
            let root = command_storage.get(storage);
            path.as_ref().map_or_else(
                || Ok(vec![Tag::Compound(root.clone())]),
                |path| path.get(&root),
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
                |path| path.get(&root),
            )?;
            values
                .into_iter()
                .map(|value| {
                    let value = value
                        .primitive_text()
                        .ok_or_else(|| "string source requires a primitive NBT value".to_owned())?;
                    let value = substring.map_or_else(
                        || Ok(value.clone()),
                        |range| value.substring(range.start, range.end),
                    )?;
                    Ok(Tag::String(value))
                })
                .collect()
        }
        DataSource::Compute { provider, integer } => {
            let providers = program.loot_registry();
            Ok(vec![if *integer {
                Tag::Int(providers.get_int(
                    provider,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?)
            } else {
                Tag::float(providers.get_float(
                    provider,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?)
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
) -> Result<Arc<[Instruction]>, String> {
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
