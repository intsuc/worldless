use std::collections::HashMap;

use crate::resource::{FunctionReference, Identifier};

#[derive(Debug)]
pub(crate) struct Program {
    functions: HashMap<Identifier, Function>,
    function_tags: HashMap<Identifier, Vec<Identifier>>,
}

impl Program {
    pub(crate) fn new(
        functions: HashMap<Identifier, Function>,
        function_tags: HashMap<Identifier, Vec<Identifier>>,
    ) -> Self {
        Self {
            functions,
            function_tags,
        }
    }

    pub(crate) fn function(&self, id: &Identifier) -> Option<&Function> {
        self.functions.get(id)
    }

    pub(crate) fn resolve_functions(
        &self,
        reference: &FunctionReference,
    ) -> Option<ResolvedFunctions<'_>> {
        match reference {
            FunctionReference::Function(id) => self.function(id).map(ResolvedFunctions::Single),
            FunctionReference::Tag(id) => self
                .function_tags
                .get(id)
                .map(|functions| ResolvedFunctions::Tag(functions)),
        }
    }
}

pub(crate) enum ResolvedFunctions<'a> {
    Single(&'a Function),
    Tag(&'a [Identifier]),
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
    Condition(ScoreCondition),
    FunctionCondition {
        expected: bool,
        function: FunctionReference,
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
    Function(FunctionReference),
    Return { success: bool, value: i32 },
    Scoreboard(ScoreboardCommand),
    Condition(ScoreCondition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScoreReference {
    pub(crate) holder: String,
    pub(crate) objective: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScoreComparison {
    Equal,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ScoreRange {
    pub(crate) min: Option<i32>,
    pub(crate) max: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScorePredicate {
    Compare {
        left: ScoreReference,
        comparison: ScoreComparison,
        right: ScoreReference,
    },
    Matches {
        score: ScoreReference,
        range: ScoreRange,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScoreCondition {
    pub(crate) expected: bool,
    pub(crate) predicate: ScorePredicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScoreboardOperation {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Min,
    Max,
    Swap,
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
    AddScore {
        holder: String,
        objective: String,
        value: i32,
    },
    RemoveScore {
        holder: String,
        objective: String,
        value: i32,
    },
    Operation {
        target: ScoreReference,
        operation: ScoreboardOperation,
        source: ScoreReference,
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

    pub(crate) fn add_score(&mut self, holder: &str, objective: &str, value: i32) -> Option<i32> {
        let score = self
            .objectives
            .get_mut(objective)?
            .entry(holder.to_owned())
            .or_default();
        *score = score.wrapping_add(value);
        Some(*score)
    }

    pub(crate) fn remove_score(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
    ) -> Option<i32> {
        let score = self
            .objectives
            .get_mut(objective)?
            .entry(holder.to_owned())
            .or_default();
        *score = score.wrapping_sub(value);
        Some(*score)
    }

    pub(crate) fn apply_operation(
        &mut self,
        target: &ScoreReference,
        operation: ScoreboardOperation,
        source: &ScoreReference,
    ) -> Option<i32> {
        if !self.contains_objective(&target.objective)
            || !self.contains_objective(&source.objective)
        {
            return None;
        }

        let left = self.score(&target.holder, &target.objective).unwrap_or(0);
        let right = self.score(&source.holder, &source.objective).unwrap_or(0);
        self.create_score(target);
        self.create_score(source);

        if matches!(operation, ScoreboardOperation::Swap) {
            self.set_existing_score(target, right);
            self.set_existing_score(source, left);
            return Some(right);
        }

        let result = match operation {
            ScoreboardOperation::Assign => right,
            ScoreboardOperation::Add => left.wrapping_add(right),
            ScoreboardOperation::Subtract => left.wrapping_sub(right),
            ScoreboardOperation::Multiply => left.wrapping_mul(right),
            ScoreboardOperation::Divide => floor_div_mod(left, right)?.0,
            ScoreboardOperation::Modulo => floor_div_mod(left, right)?.1,
            ScoreboardOperation::Min => left.min(right),
            ScoreboardOperation::Max => left.max(right),
            ScoreboardOperation::Swap => unreachable!("swap is handled before simple operations"),
        };
        self.set_existing_score(target, result);
        Some(result)
    }

    pub(crate) fn evaluate_condition(&self, condition: &ScoreCondition) -> Option<bool> {
        self.evaluate_predicate(&condition.predicate)
            .map(|result| result == condition.expected)
    }

    fn evaluate_predicate(&self, predicate: &ScorePredicate) -> Option<bool> {
        match predicate {
            ScorePredicate::Compare {
                left,
                comparison,
                right,
            } => {
                if !self.contains_objective(&left.objective)
                    || !self.contains_objective(&right.objective)
                {
                    return None;
                }
                let (Some(left), Some(right)) = (
                    self.score(&left.holder, &left.objective),
                    self.score(&right.holder, &right.objective),
                ) else {
                    return Some(false);
                };
                Some(match comparison {
                    ScoreComparison::Equal => left == right,
                    ScoreComparison::LessThan => left < right,
                    ScoreComparison::LessThanOrEqual => left <= right,
                    ScoreComparison::GreaterThan => left > right,
                    ScoreComparison::GreaterThanOrEqual => left >= right,
                })
            }
            ScorePredicate::Matches { score, range } => {
                if !self.contains_objective(&score.objective) {
                    return None;
                }
                let Some(value) = self.score(&score.holder, &score.objective) else {
                    return Some(false);
                };
                Some(
                    range.min.is_none_or(|min| value >= min)
                        && range.max.is_none_or(|max| value <= max),
                )
            }
        }
    }

    fn create_score(&mut self, score: &ScoreReference) {
        self.objectives
            .get_mut(&score.objective)
            .expect("score objectives are resolved before score creation")
            .entry(score.holder.clone())
            .or_default();
    }

    fn set_existing_score(&mut self, score: &ScoreReference, value: i32) {
        *self
            .objectives
            .get_mut(&score.objective)
            .expect("score objectives are resolved before operation")
            .get_mut(&score.holder)
            .expect("operation scores are created before mutation") = value;
    }
}

fn floor_div_mod(left: i32, right: i32) -> Option<(i32, i32)> {
    if right == 0 {
        return None;
    }
    let left = i64::from(left);
    let right = i64::from(right);
    let quotient = left / right;
    let remainder = left % right;
    let floor = if remainder != 0 && (left < 0) != (right < 0) {
        quotient - 1
    } else {
        quotient
    };
    Some((floor as i32, (left - floor * right) as i32))
}
