use std::{cell::Cell, collections::VecDeque, error::Error, fmt, rc::Rc};

use crate::{
    program::{
        Command, Function, Modifier, Program, ScoreCondition, Scoreboard, ScoreboardCommand,
        StoreKind,
    },
    resource::Identifier,
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
struct StoreAction {
    kind: StoreKind,
    holder: String,
    objective: String,
}

#[derive(Clone)]
enum ConsumerEnd {
    Ignore,
    TopLevel,
    FunctionCondition(Rc<Cell<Option<i32>>>),
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
        top_level_result: &mut Option<FunctionOutcome>,
    ) {
        for store in &self.stores {
            let value = match store.kind {
                StoreKind::Result => result.value,
                StoreKind::Success => i32::from(result.success),
            };
            let stored = scoreboard.set_score(&store.holder, &store.objective, value);
            assert!(
                stored,
                "store objectives are resolved before command execution"
            );
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
            | Self::ResumeFunctionCondition { frame, .. } => frame.depth,
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
                                .push(StoreAction {
                                    kind: *kind,
                                    holder: holder.clone(),
                                    objective: objective.clone(),
                                });
                        }
                        Modifier::Condition(condition) => {
                            quota.increment();
                            forked = true;
                            if active {
                                active = scoreboard.evaluate_condition(condition).unwrap_or(false);
                            }
                        }
                        Modifier::FunctionCondition {
                            expected,
                            function: function_id,
                        } => {
                            forked = true;
                            let frame = frame.take().expect("the frame has not been queued");
                            let stores = stores.take().expect("the stores have not been queued");
                            let Some(condition_function) = program.function(function_id) else {
                                if !return_run {
                                    schedule_next_instruction(&mut queue, frame);
                                }
                                break false;
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
                            queue.push_front(QueueEntry::Call(Frame {
                                function: condition_function,
                                next_instruction: 0,
                                depth: isolated_depth,
                                discard_depth: isolated_depth,
                                result_consumer,
                            }));
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
                    Command::Function(id) => execute_function_command(
                        program,
                        scoreboard,
                        &mut queue,
                        &mut top_level_result,
                        frame,
                        id,
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
                            &mut top_level_result,
                        );
                        discard_at_depth_or_higher(&mut queue, frame.discard_depth);
                    }
                    Command::Scoreboard(_) | Command::Condition(_) => {
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
                    Command::Function(_) | Command::Return { .. } => {
                        unreachable!("only ordinary commands are queued for ordinary execution")
                    }
                };
                ResultConsumer::ignoring(stores).accept(result, scoreboard, &mut top_level_result);

                if return_run {
                    frame
                        .result_consumer
                        .accept(result, scoreboard, &mut top_level_result);
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
            QueueEntry::Fallthrough {
                discard_depth,
                result_consumer,
                ..
            } => {
                result_consumer.accept(CommandResult::FAILURE, scoreboard, &mut top_level_result);
                discard_at_depth_or_higher(&mut queue, discard_depth);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_function_command<'a>(
    program: &'a Program,
    scoreboard: &mut Scoreboard,
    queue: &mut VecDeque<QueueEntry<'a>>,
    top_level_result: &mut Option<FunctionOutcome>,
    frame: Frame<'a>,
    id: &Identifier,
    stores: Vec<StoreAction>,
    return_run: bool,
) {
    let Some(function) = program.function(id) else {
        ResultConsumer::ignoring(stores).accept(
            CommandResult::FAILURE,
            scoreboard,
            top_level_result,
        );
        if !return_run {
            schedule_next_instruction(queue, frame);
        }
        return;
    };

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
