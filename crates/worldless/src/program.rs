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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Instruction {
    pub(crate) modifiers: Vec<Modifier>,
    pub(crate) command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Modifier {
    StoreScore {
        kind: StoreKind,
        holder: String,
        objective: String,
    },
    ReturnRun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoreKind {
    Result,
    Success,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Function(Identifier),
    Return { success: bool, value: i32 },
    Scoreboard(ScoreboardCommand),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScoreboardCommand {
    AddObjective {
        objective: String,
    },
    SetScore {
        holder: String,
        objective: String,
        value: i32,
    },
    GetScore {
        holder: String,
        objective: String,
    },
}

#[derive(Debug, Default)]
pub(crate) struct Scoreboard {
    objectives: HashMap<String, HashMap<String, i32>>,
}

impl Scoreboard {
    pub(crate) fn contains_objective(&self, objective: &str) -> bool {
        self.objectives.contains_key(objective)
    }

    pub(crate) fn add_objective(&mut self, objective: &str) -> Option<i32> {
        if self.objectives.contains_key(objective) {
            return None;
        }

        self.objectives.insert(objective.to_owned(), HashMap::new());
        Some(
            i32::try_from(self.objectives.len())
                .expect("a scoreboard cannot contain more than i32::MAX objectives"),
        )
    }

    pub(crate) fn set_score(&mut self, holder: &str, objective: &str, value: i32) -> bool {
        let Some(scores) = self.objectives.get_mut(objective) else {
            return false;
        };
        scores.insert(holder.to_owned(), value);
        true
    }

    pub(crate) fn score(&self, holder: &str, objective: &str) -> Option<i32> {
        self.objectives
            .get(objective)
            .and_then(|scores| scores.get(holder))
            .copied()
    }
}
