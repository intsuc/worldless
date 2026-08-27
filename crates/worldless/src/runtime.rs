use std::{cell::Cell, collections::VecDeque, error::Error, fmt, rc::Rc};

use crate::{
    nbt::{CommandStorage, JavaString, Tag},
    program::{
        Command, DataCommand, DataModifyOperation, DataSource, Function, Modifier, Program,
        ResolvedFunctions, ScoreCondition, Scoreboard, ScoreboardCommand, StorageCondition,
        StorageNumberType, StoreKind,
    },
    resource::{FunctionReference, Identifier},
};

/// The observable result of a function invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionOutcome {
    /// The function reached its end without executing `return`.
    FellThrough,
    /// The function explicitly returned a command result.
    Returned { success: bool, value: i32 },
}

/// An error that prevents a function invocation from completing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecutionError {
    InvalidFunctionIdentifier { input: String },
    UnknownFunction { id: String },
    CommandLimitExceeded { limit: usize },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunctionIdentifier { input } => {
                write!(formatter, "invalid function identifier {input:?}")
            }
            Self::UnknownFunction { id } => write!(formatter, "unknown function {id}"),
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
        holder: String,
        objective: String,
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
    Ignore,
    TopLevel,
    FunctionCondition(Rc<Cell<Option<i32>>>),
    FunctionTag(Rc<Cell<Option<i32>>>),
}

#[derive(Clone)]
struct ResultConsumer {
    stores: Vec<StoreAction>,
    end: ConsumerEnd,
}

impl ResultConsumer {
    fn top_level() -> Self {
        Self {
            stores: Vec::new(),
            end: ConsumerEnd::TopLevel,
        }
    }

    fn ignoring(stores: Vec<StoreAction>) -> Self {
        Self {
            stores,
            end: ConsumerEnd::Ignore,
        }
    }

    fn function_condition(result: Rc<Cell<Option<i32>>>) -> Self {
        Self {
            stores: Vec::new(),
            end: ConsumerEnd::FunctionCondition(result),
        }
    }

    fn function_tag(result: Rc<Cell<Option<i32>>>) -> Self {
        Self {
            stores: Vec::new(),
            end: ConsumerEnd::FunctionTag(result),
        }
    }

    fn with_prefix(&self, mut prefix: Vec<StoreAction>) -> Self {
        prefix.extend(self.stores.iter().cloned());
        Self {
            stores: prefix,
            end: self.end.clone(),
        }
    }

    fn accept(
        &self,
        result: CommandResult,
        scoreboard: &mut Scoreboard,
        command_storage: &mut CommandStorage,
        top_level_result: &mut Option<FunctionOutcome>,
    ) {
        for store in &self.stores {
            match store {
                StoreAction::Score {
                    kind,
                    holder,
                    objective,
                } => {
                    let value = stored_command_value(*kind, result);
                    let stored = scoreboard.set_score(holder, objective, value);
                    assert!(
                        stored,
                        "store objectives are resolved before command execution"
                    );
                }
                StoreAction::Storage {
                    kind,
                    storage,
                    path,
                    number_type,
                    scale,
                } => {
                    let value = storage_number(
                        *number_type,
                        f64::from(stored_command_value(*kind, result)) * scale,
                    );
                    let _ = command_storage.edit(storage, |data| path.set(data, value));
                }
            }
        }

        match &self.end {
            ConsumerEnd::Ignore => {}
            ConsumerEnd::TopLevel => {
                *top_level_result = Some(FunctionOutcome::Returned {
                    success: result.success,
                    value: result.value,
                });
            }
            ConsumerEnd::FunctionCondition(condition_result) => {
                condition_result.set(Some(result.value));
            }
            ConsumerEnd::FunctionTag(tag_result) => {
                tag_result.set(Some(
                    tag_result.get().unwrap_or(0).wrapping_add(result.value),
                ));
            }
        }
    }
}

struct Frame<'a> {
    function: &'a Function,
    next_instruction: usize,
    depth: usize,
    discard_depth: usize,
    result_consumer: ResultConsumer,
}

enum QueueEntry<'a> {
    Call(Frame<'a>),
    Step(Frame<'a>),
    Prepare {
        frame: Frame<'a>,
        instruction: usize,
        next_modifier: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
        active: bool,
        forked: bool,
    },
    ExecuteOrdinary {
        frame: Frame<'a>,
        instruction: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
    },
    ResumeFunctionCondition {
        frame: Frame<'a>,
        instruction: usize,
        next_modifier: usize,
        stores: Vec<StoreAction>,
        return_run: bool,
        expected: bool,
        result: Rc<Cell<Option<i32>>>,
    },
    ContinueFunctionTag {
        frame: Frame<'a>,
        functions: &'a [Identifier],
        next_function: usize,
        stores: Vec<StoreAction>,
        result: Option<Rc<Cell<Option<i32>>>>,
    },
    Fallthrough {
        depth: usize,
        discard_depth: usize,
        result_consumer: ResultConsumer,
    },
}

impl QueueEntry<'_> {
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

pub(crate) fn execute(
    program: &Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    input: &str,
    command_limit: usize,
) -> Result<FunctionOutcome, ExecutionError> {
    let id = Identifier::parse(input).ok_or_else(|| ExecutionError::InvalidFunctionIdentifier {
        input: input.to_owned(),
    })?;
    let function = find_function(program, &id)?;
    let mut queue = VecDeque::from([QueueEntry::Call(Frame {
        function,
        next_instruction: 0,
        depth: 0,
        discard_depth: 0,
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
            return Ok(top_level_result.unwrap_or(FunctionOutcome::FellThrough));
        };

        match entry {
            QueueEntry::Call(frame) => {
                quota.increment();
                schedule_next_instruction(&mut queue, frame);
            }
            QueueEntry::Step(mut frame) => {
                let instruction = frame.next_instruction;
                frame.next_instruction += 1;
                queue.push_front(QueueEntry::Prepare {
                    frame,
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
                instruction,
                mut next_modifier,
                stores,
                return_run,
                active,
                forked,
            } => {
                let function = frame.function;
                let compiled = &function.instructions[instruction];
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
                        Modifier::StoreScore {
                            kind,
                            holder,
                            objective,
                        } => {
                            quota.increment();
                            if !active {
                                continue;
                            }
                            if !scoreboard.contains_objective(objective) {
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
                            }
                            stores
                                .as_mut()
                                .expect("the stores have not been queued")
                                .push(StoreAction::Score {
                                    kind: *kind,
                                    holder: holder.clone(),
                                    objective: objective.clone(),
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
                                    instruction,
                                    next_modifier,
                                    stores,
                                    return_run,
                                    active: false,
                                    forked,
                                });
                                break false;
                            }

                            let result = Rc::new(Cell::new(None));
                            let result_consumer =
                                ResultConsumer::function_condition(Rc::clone(&result));
                            let isolated_depth = frame.depth + 1;
                            queue.push_front(QueueEntry::ResumeFunctionCondition {
                                frame,
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
                            match condition_functions {
                                ResolvedFunctions::Single(function) => {
                                    queue.push_front(QueueEntry::Call(Frame {
                                        function,
                                        next_instruction: 0,
                                        depth: isolated_depth,
                                        discard_depth: isolated_depth,
                                        result_consumer,
                                    }));
                                }
                                ResolvedFunctions::Tag(functions) => {
                                    for function_id in functions.iter().rev() {
                                        queue.push_front(QueueEntry::Call(Frame {
                                            function: program.function(function_id).expect(
                                                "resolved function tags contain loaded functions",
                                            ),
                                            next_instruction: 0,
                                            depth: isolated_depth,
                                            discard_depth: isolated_depth,
                                            result_consumer: result_consumer.clone(),
                                        }));
                                    }
                                }
                            }
                            break false;
                        }
                        Modifier::ReturnRun => {
                            let frame = frame.take().expect("the frame has not been queued");
                            if active {
                                discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                                queue.push_front(QueueEntry::Prepare {
                                    frame,
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
                    Command::Function(reference) => execute_function_command(
                        program,
                        scoreboard,
                        command_storage,
                        &mut queue,
                        &mut top_level_result,
                        frame,
                        reference,
                        stores,
                        return_run,
                    ),
                    Command::Return { success, value } => {
                        let result = CommandResult {
                            success: *success,
                            value: *value,
                        };
                        frame.result_consumer.with_prefix(stores).accept(
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
                    | Command::Data(_) => {
                        queue.push_front(QueueEntry::ExecuteOrdinary {
                            frame,
                            instruction,
                            stores,
                            return_run,
                        });
                    }
                }
            }
            QueueEntry::ExecuteOrdinary {
                frame,
                instruction,
                stores,
                return_run,
            } => {
                quota.increment();
                let command = &frame.function.instructions[instruction].command;
                let result = match command {
                    Command::Scoreboard(command) => execute_scoreboard_command(scoreboard, command),
                    Command::Condition(condition) => execute_condition(scoreboard, condition),
                    Command::StorageCondition(condition) => {
                        execute_storage_condition(command_storage, condition)
                    }
                    Command::Data(command) => execute_data_command(command_storage, command),
                    Command::Function(_) | Command::Return { .. } => {
                        unreachable!("only ordinary commands are queued for ordinary execution")
                    }
                };
                ResultConsumer::ignoring(stores).accept(
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
                functions,
                next_function,
                stores,
                result,
            } => {
                let Some(function_id) = functions.get(next_function) else {
                    if let Some(value) = result.as_ref().and_then(|result| result.get()) {
                        ResultConsumer::ignoring(stores).accept(
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
                let result_consumer = result.as_ref().map_or_else(
                    || ResultConsumer::ignoring(Vec::new()),
                    |result| ResultConsumer::function_tag(Rc::clone(result)),
                );
                queue.push_front(QueueEntry::ContinueFunctionTag {
                    frame,
                    functions,
                    next_function: next_function + 1,
                    stores,
                    result,
                });
                queue.push_front(QueueEntry::Call(Frame {
                    function: program
                        .function(function_id)
                        .expect("resolved function tags contain loaded functions"),
                    next_instruction: 0,
                    depth: child_depth,
                    discard_depth: child_depth,
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
fn execute_function_command<'a>(
    program: &'a Program,
    scoreboard: &mut Scoreboard,
    command_storage: &mut CommandStorage,
    queue: &mut VecDeque<QueueEntry<'a>>,
    top_level_result: &mut Option<FunctionOutcome>,
    frame: Frame<'a>,
    reference: &FunctionReference,
    stores: Vec<StoreAction>,
    return_run: bool,
) {
    let functions = match program.resolve_functions(reference) {
        Some(ResolvedFunctions::Tag([])) | None => {
            ResultConsumer::ignoring(stores).accept(
                CommandResult::FAILURE,
                scoreboard,
                command_storage,
                top_level_result,
            );
            if !return_run {
                schedule_next_instruction(queue, frame);
            }
            return;
        }
        Some(functions) => functions,
    };

    match functions {
        ResolvedFunctions::Single(function) => {
            queue_single_function(queue, frame, function, stores, return_run);
        }
        ResolvedFunctions::Tag([function_id]) => {
            queue_single_function(
                queue,
                frame,
                program
                    .function(function_id)
                    .expect("resolved function tags contain loaded functions"),
                stores,
                return_run,
            );
        }
        ResolvedFunctions::Tag(functions) if return_run => {
            let child_depth = frame.depth + 1;
            let fallback_depth = frame.depth;
            let discard_depth = frame.discard_depth;
            let parent_consumer = frame.result_consumer;
            let child_consumer = parent_consumer.with_prefix(stores);
            queue.push_front(QueueEntry::Fallthrough {
                depth: fallback_depth,
                discard_depth,
                result_consumer: parent_consumer,
            });
            for function_id in functions.iter().rev() {
                queue.push_front(QueueEntry::Call(Frame {
                    function: program
                        .function(function_id)
                        .expect("resolved function tags contain loaded functions"),
                    next_instruction: 0,
                    depth: child_depth,
                    discard_depth,
                    result_consumer: child_consumer.clone(),
                }));
            }
        }
        ResolvedFunctions::Tag(functions) => {
            let result = (!stores.is_empty()).then(|| Rc::new(Cell::new(None)));
            queue.push_front(QueueEntry::ContinueFunctionTag {
                frame,
                functions,
                next_function: 0,
                stores,
                result,
            });
        }
    }
}

fn queue_single_function<'a>(
    queue: &mut VecDeque<QueueEntry<'a>>,
    frame: Frame<'a>,
    function: &'a Function,
    stores: Vec<StoreAction>,
    return_run: bool,
) {
    let child_depth = frame.depth + 1;
    if return_run {
        let parent_consumer = frame.result_consumer;
        let child = Frame {
            function,
            next_instruction: 0,
            depth: child_depth,
            discard_depth: frame.discard_depth,
            result_consumer: parent_consumer.with_prefix(stores),
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
            next_instruction: 0,
            depth: child_depth,
            discard_depth: child_depth,
            result_consumer: ResultConsumer::ignoring(stores),
        };
        schedule_next_instruction(queue, frame);
        queue.push_front(QueueEntry::Call(child));
    }
}

fn execute_scoreboard_command(
    scoreboard: &mut Scoreboard,
    command: &ScoreboardCommand,
) -> CommandResult {
    match command {
        ScoreboardCommand::AddObjective { objective } => scoreboard
            .add_objective(objective)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::SetScore {
            holder,
            objective,
            value,
        } => {
            if scoreboard.set_score(holder, objective, *value) {
                CommandResult::success(*value)
            } else {
                CommandResult::FAILURE
            }
        }
        ScoreboardCommand::GetScore { holder, objective } => scoreboard
            .score(holder, objective)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::AddScore {
            holder,
            objective,
            value,
        } => scoreboard
            .add_score(holder, objective, *value)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::RemoveScore {
            holder,
            objective,
            value,
        } => scoreboard
            .remove_score(holder, objective, *value)
            .map_or(CommandResult::FAILURE, CommandResult::success),
        ScoreboardCommand::Operation {
            target,
            operation,
            source,
        } => scoreboard
            .apply_operation(target, *operation, source)
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
    command_storage: &mut CommandStorage,
    command: &DataCommand,
) -> CommandResult {
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
        } => modify_storage(command_storage, storage, path, *operation, source),
    };
    result.map_or(CommandResult::FAILURE, CommandResult::success)
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
    source: &DataSource,
) -> Result<i32, String> {
    let source = resolve_data_source(command_storage, source)?;
    command_storage.edit(storage, |target| {
        let changed = match operation {
            DataModifyOperation::Insert(index) => path.insert(index, target, &source)?,
            DataModifyOperation::Set => path.set(
                target,
                source
                    .last()
                    .expect("data modification sources contain at least one value")
                    .clone(),
            )?,
            DataModifyOperation::Merge => path.merge(target, &source)?,
        };
        (changed != 0)
            .then_some(changed)
            .ok_or_else(|| "NBT modification changed nothing".to_owned())
    })
}

fn resolve_data_source(
    command_storage: &CommandStorage,
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
                    let value = primitive_text(&value)?;
                    let value = substring.map_or_else(
                        || Ok(value.clone()),
                        |range| value.substring(range.start, range.end),
                    )?;
                    Ok(Tag::String(value))
                })
                .collect()
        }
    }
}

fn primitive_text(value: &Tag) -> Result<JavaString, String> {
    use worldless_brigadier::exceptions::{java_f32, java_f64};

    let value = match value {
        Tag::Byte(value) => format!("{value}b"),
        Tag::Short(value) => format!("{value}s"),
        Tag::Int(value) => value.to_string(),
        Tag::Long(value) => format!("{value}L"),
        Tag::Float(bits) => format!("{}f", java_f32(f32::from_bits(*bits))),
        Tag::Double(bits) => format!("{}d", java_f64(f64::from_bits(*bits))),
        Tag::String(value) => return Ok(value.clone()),
        _ => return Err("string source requires a primitive NBT value".to_owned()),
    };
    Ok(JavaString::from(value.as_str()))
}

fn minecraft_floor_to_i32(value: f64) -> i32 {
    value.floor() as i32
}

fn schedule_next_instruction<'a>(queue: &mut VecDeque<QueueEntry<'a>>, frame: Frame<'a>) {
    if frame.next_instruction < frame.function.instructions.len() {
        queue.push_front(QueueEntry::Step(frame));
    }
}

fn discard_at_depth_or_higher(queue: &mut VecDeque<QueueEntry<'_>>, depth: usize) {
    queue.retain(|entry| entry.depth() < depth);
}

fn find_function<'a>(
    program: &'a Program,
    id: &Identifier,
) -> Result<&'a Function, ExecutionError> {
    program
        .function(id)
        .ok_or_else(|| ExecutionError::UnknownFunction { id: id.to_string() })
}
