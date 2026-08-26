use std::collections::HashMap;

use crate::resource::Identifier;

#[derive(Debug)]
pub(crate) struct Program {
    functions: HashMap<Identifier, Function>,
}

impl Program {
    pub(crate) fn new(functions: HashMap<Identifier, Function>) -> Self {
        Self { functions }
    }

    pub(crate) fn function(&self, id: &Identifier) -> Option<&Function> {
        self.functions.get(id)
    }
}

#[derive(Debug)]
pub(crate) struct Function {
    pub(crate) instructions: Vec<Instruction>,
}

#[derive(Debug)]
pub(crate) struct Instruction {
    pub(crate) kind: InstructionKind,
}

#[derive(Clone, Debug)]
pub(crate) enum InstructionKind {
    Call(Identifier),
    Return { success: bool, value: i32 },
    ReturnRunCall(Identifier),
}
