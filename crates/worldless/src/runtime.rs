use std::{collections::VecDeque, error::Error, fmt};

use crate::{
    program::{Function, InstructionKind, Program},
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
enum Completion {
    TopLevel,
    IgnoreResult,
}

struct Frame<'a> {
    function: &'a Function,
    next_instruction: usize,
    completion: Completion,
    fail_on_fallthrough: bool,
}

enum QueueEntry<'a> {
    Call(Frame<'a>),
    Step(Frame<'a>),
}

pub(crate) fn execute(
    program: &Program,
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
        completion: Completion::TopLevel,
        fail_on_fallthrough: false,
    })]);
    let mut remaining = command_limit;

    loop {
        if remaining == 0 {
            return Err(ExecutionError::CommandLimitExceeded {
                limit: command_limit,
            });
        }
        let Some(entry) = queue.pop_front() else {
            return Ok(FunctionOutcome::FellThrough);
        };
        match entry {
            QueueEntry::Call(frame) => {
                remaining -= 1;
                queue.push_front(QueueEntry::Step(frame));
            }
            QueueEntry::Step(mut frame) => {
                let Some(instruction) = frame.function.instructions.get(frame.next_instruction)
                else {
                    let outcome = if frame.fail_on_fallthrough {
                        FunctionOutcome::Returned {
                            success: false,
                            value: 0,
                        }
                    } else {
                        FunctionOutcome::FellThrough
                    };
                    if matches!(frame.completion, Completion::TopLevel) {
                        return Ok(outcome);
                    }
                    continue;
                };
                frame.next_instruction += 1;

                match &instruction.kind {
                    InstructionKind::Call(id) => {
                        if frame.next_instruction < frame.function.instructions.len()
                            || frame.fail_on_fallthrough
                        {
                            queue.push_front(QueueEntry::Step(frame));
                        }
                        if let Some(child) = program.function(id) {
                            queue.push_front(QueueEntry::Call(Frame {
                                function: child,
                                next_instruction: 0,
                                completion: Completion::IgnoreResult,
                                fail_on_fallthrough: false,
                            }));
                        }
                    }
                    InstructionKind::Return { success, value } => {
                        let outcome = FunctionOutcome::Returned {
                            success: *success,
                            value: *value,
                        };
                        if matches!(frame.completion, Completion::TopLevel) {
                            return Ok(outcome);
                        }
                    }
                    InstructionKind::ReturnRunCall(id) => {
                        if let Some(child) = program.function(id) {
                            queue.push_front(QueueEntry::Call(Frame {
                                function: child,
                                next_instruction: 0,
                                completion: frame.completion,
                                fail_on_fallthrough: true,
                            }));
                        }
                    }
                }
            }
        }
    }
}

fn find_function<'a>(
    program: &'a Program,
    id: &Identifier,
) -> Result<&'a Function, ExecutionError> {
    program
        .function(id)
        .ok_or_else(|| ExecutionError::UnknownFunction { id: id.to_string() })
}
