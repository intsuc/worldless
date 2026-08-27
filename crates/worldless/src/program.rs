use std::{
    collections::{BTreeMap, HashMap, btree_map::Entry},
    sync::Arc,
};

use crate::execution_context::ContextTransform;
use crate::macro_function::Function;
use crate::nbt::{CompoundTag, JavaString, NbtPath, Tag};
use crate::number_provider::{LootRegistry, NumberProviderReference};
use crate::predicate::PredicateReference;
use crate::resource::{FunctionReference, Identifier};

#[derive(Debug)]
pub(crate) struct Program {
    functions: HashMap<Identifier, Function>,
    function_tags: HashMap<Identifier, Vec<Identifier>>,
    loot_registry: Arc<LootRegistry>,
}

impl Program {
    pub(crate) fn new(
        functions: HashMap<Identifier, Function>,
        function_tags: HashMap<Identifier, Vec<Identifier>>,
        loot_registry: Arc<LootRegistry>,
    ) -> Self {
        Self {
            functions,
            function_tags,
            loot_registry,
        }
    }

    pub(crate) fn function(&self, id: &Identifier) -> Option<&Function> {
        self.functions.get(id)
    }

    pub(crate) fn loot_registry(&self) -> &Arc<LootRegistry> {
        &self.loot_registry
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Instruction {
    pub(crate) modifiers: Vec<Modifier>,
    pub(crate) command: Command,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Modifier {
    ContextTransform(ContextTransform),
    StoreScore {
        kind: StoreKind,
        holders: ScoreHolderSet,
        objective: String,
    },
    StoreStorage {
        kind: StoreKind,
        storage: Identifier,
        path: NbtPath,
        number_type: StorageNumberType,
        scale: f64,
    },
    Condition(ScoreCondition),
    StorageCondition(StorageCondition),
    FunctionCondition {
        expected: bool,
        function: FunctionReference,
    },
    PredicateCondition(PredicateCondition),
    ReturnRun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoreKind {
    Result,
    Success,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Function {
        reference: FunctionReference,
        arguments: Option<FunctionArguments>,
    },
    Return {
        success: bool,
        value: i32,
    },
    Scoreboard(ScoreboardCommand),
    Condition(ScoreCondition),
    StorageCondition(StorageCondition),
    PredicateCondition(PredicateCondition),
    Data(DataCommand),
    Compute(ComputeCommand),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComputeCommand {
    pub(crate) provider: NumberProviderReference,
    pub(crate) mode: ComputeMode,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PredicateCondition {
    pub(crate) expected: bool,
    pub(crate) predicate: PredicateReference,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ComputeMode {
    Float { scale: f32 },
    Integer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FunctionArguments {
    Compound(CompoundTag),
    Storage {
        storage: Identifier,
        path: Option<NbtPath>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageNumberType {
    Byte,
    Short,
    Int,
    Long,
    Float,
    Double,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageCondition {
    pub(crate) expected: bool,
    pub(crate) storage: Identifier,
    pub(crate) path: NbtPath,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DataCommand {
    Merge {
        storage: Identifier,
        value: CompoundTag,
    },
    Get {
        storage: Identifier,
    },
    GetPath {
        storage: Identifier,
        path: NbtPath,
        scale: Option<f64>,
    },
    Remove {
        storage: Identifier,
        path: NbtPath,
    },
    Modify {
        storage: Identifier,
        path: NbtPath,
        operation: DataModifyOperation,
        source: DataSource,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DataModifyOperation {
    Insert(i32),
    Set,
    Merge,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DataSource {
    Value(Tag),
    Storage {
        storage: Identifier,
        path: Option<NbtPath>,
    },
    String {
        storage: Identifier,
        path: Option<NbtPath>,
        substring: Option<DataStringSubstring>,
    },
    Compute {
        provider: NumberProviderReference,
        integer: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DataStringSubstring {
    pub(crate) start: i32,
    pub(crate) end: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScoreHolderSet {
    Named(JavaString),
    Wildcard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScoreReference {
    pub(crate) holder: ScoreHolderSet,
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
    ListObjectives,
    AddObjective {
        objective: String,
    },
    RemoveObjective {
        objective: String,
    },
    ListPlayers,
    ListPlayerScores {
        holder: ScoreHolderSet,
    },
    SetScore {
        holders: ScoreHolderSet,
        objective: String,
        value: i32,
    },
    GetScore {
        holder: ScoreHolderSet,
        objective: String,
    },
    AddScore {
        holders: ScoreHolderSet,
        objective: String,
        value: i32,
    },
    RemoveScore {
        holders: ScoreHolderSet,
        objective: String,
        value: i32,
    },
    ResetScores {
        holders: ScoreHolderSet,
        objective: Option<String>,
    },
    Operation {
        targets: ScoreHolderSet,
        target_objective: String,
        operation: ScoreboardOperation,
        sources: ScoreHolderSet,
        source_objective: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ObjectiveId(u64);

#[derive(Debug, Default)]
pub(crate) struct Scoreboard {
    objectives: HashMap<String, ObjectiveId>,
    holders: BTreeMap<JavaString, HashMap<ObjectiveId, i32>>,
    next_objective_id: u64,
}

impl Scoreboard {
    pub(crate) fn contains_objective(&self, objective: &str) -> bool {
        self.objectives.contains_key(objective)
    }

    pub(crate) fn objective_id(&self, objective: &str) -> Option<ObjectiveId> {
        self.objectives.get(objective).copied()
    }

    pub(crate) fn add_objective(&mut self, objective: &str) -> Option<i32> {
        if self.objectives.contains_key(objective) {
            return None;
        }

        let next_objective_id = self
            .next_objective_id
            .checked_add(1)
            .expect("a scoreboard cannot create more than u64::MAX objectives");
        let id = ObjectiveId(self.next_objective_id);
        self.next_objective_id = next_objective_id;
        self.objectives.insert(objective.to_owned(), id);
        Some(self.list_objectives())
    }

    pub(crate) fn list_objectives(&self) -> i32 {
        scoreboard_count(self.objectives.len(), "objectives")
    }

    pub(crate) fn remove_objective(&mut self, objective: &str) -> Option<i32> {
        let id = self.objectives.remove(objective)?;
        for scores in self.holders.values_mut() {
            scores.remove(&id);
        }
        Some(self.list_objectives())
    }

    pub(crate) fn list_players(&self) -> i32 {
        scoreboard_count(self.holders.len(), "score holders")
    }

    pub(crate) fn list_player_scores(&self, holder: &JavaString) -> i32 {
        scoreboard_count(
            self.holders.get(holder).map_or(0, HashMap::len),
            "scores for one holder",
        )
    }

    pub(crate) fn resolve_holders(&self, holders: &ScoreHolderSet) -> Option<Vec<JavaString>> {
        match holders {
            ScoreHolderSet::Named(holder) => Some(vec![holder.clone()]),
            ScoreHolderSet::Wildcard if self.holders.is_empty() => None,
            ScoreHolderSet::Wildcard => Some(self.holders.keys().cloned().collect()),
        }
    }

    pub(crate) fn set_score_by_id(
        &mut self,
        holder: &JavaString,
        objective: ObjectiveId,
        value: i32,
    ) {
        self.holders
            .entry(holder.clone())
            .or_default()
            .insert(objective, value);
    }

    pub(crate) fn score(&self, holder: &JavaString, objective: &str) -> Option<i32> {
        self.objective_id(objective)
            .and_then(|objective| self.score_by_id(holder, objective))
    }

    pub(crate) fn set_scores(
        &mut self,
        holders: &ScoreHolderSet,
        objective: &str,
        value: i32,
    ) -> Option<i32> {
        let holders = self.resolve_holders(holders)?;
        let objective = self.objective_id(objective)?;
        let mut total = 0i32;
        for holder in holders {
            self.set_score_by_id(&holder, objective, value);
            total = total.wrapping_add(value);
        }
        Some(total)
    }

    pub(crate) fn add_scores(
        &mut self,
        holders: &ScoreHolderSet,
        objective: &str,
        value: i32,
    ) -> Option<i32> {
        self.change_scores(holders, objective, value, i32::wrapping_add)
    }

    pub(crate) fn remove_scores(
        &mut self,
        holders: &ScoreHolderSet,
        objective: &str,
        value: i32,
    ) -> Option<i32> {
        self.change_scores(holders, objective, value, i32::wrapping_sub)
    }

    pub(crate) fn reset_scores(
        &mut self,
        holders: &ScoreHolderSet,
        objective: Option<&str>,
    ) -> Option<i32> {
        let holders = self.resolve_holders(holders)?;
        let objective = match objective {
            Some(objective) => Some(self.objective_id(objective)?),
            None => None,
        };
        let count = scoreboard_count(holders.len(), "selected score holders");

        if let Some(objective) = objective {
            for holder in holders {
                if let Entry::Occupied(mut entry) = self.holders.entry(holder) {
                    entry.get_mut().remove(&objective);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
            }
        } else {
            for holder in holders {
                self.holders.remove(&holder);
            }
        }
        Some(count)
    }

    pub(crate) fn apply_operation(
        &mut self,
        targets: &ScoreHolderSet,
        target_objective: &str,
        operation: ScoreboardOperation,
        sources: &ScoreHolderSet,
        source_objective: &str,
    ) -> Option<i32> {
        let targets = self.resolve_holders(targets)?;
        let target_objective = self.objective_id(target_objective)?;
        let sources = self.resolve_holders(sources)?;
        let source_objective = self.objective_id(source_objective)?;
        let mut total = 0i32;

        for target in targets {
            for source in &sources {
                self.apply_single_operation(
                    &target,
                    target_objective,
                    operation,
                    source,
                    source_objective,
                )?;
            }
            let value = self
                .score_by_id(&target, target_objective)
                .expect("an operation creates its target score");
            total = total.wrapping_add(value);
        }
        Some(total)
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
                let (ScoreHolderSet::Named(left_holder), ScoreHolderSet::Named(right_holder)) =
                    (&left.holder, &right.holder)
                else {
                    return None;
                };
                if !self.contains_objective(&left.objective)
                    || !self.contains_objective(&right.objective)
                {
                    return None;
                }
                let (Some(left), Some(right)) = (
                    self.score(left_holder, &left.objective),
                    self.score(right_holder, &right.objective),
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
                let ScoreHolderSet::Named(holder) = &score.holder else {
                    return None;
                };
                if !self.contains_objective(&score.objective) {
                    return None;
                }
                let Some(value) = self.score(holder, &score.objective) else {
                    return Some(false);
                };
                Some(
                    range.min.is_none_or(|min| value >= min)
                        && range.max.is_none_or(|max| value <= max),
                )
            }
        }
    }

    fn score_by_id(&self, holder: &JavaString, objective: ObjectiveId) -> Option<i32> {
        self.holders
            .get(holder)
            .and_then(|scores| scores.get(&objective))
            .copied()
    }

    fn change_scores(
        &mut self,
        holders: &ScoreHolderSet,
        objective: &str,
        value: i32,
        operation: fn(i32, i32) -> i32,
    ) -> Option<i32> {
        let holders = self.resolve_holders(holders)?;
        let objective = self.objective_id(objective)?;
        let mut total = 0i32;
        for holder in holders {
            let result = operation(self.score_by_id(&holder, objective).unwrap_or(0), value);
            self.set_score_by_id(&holder, objective, result);
            total = total.wrapping_add(result);
        }
        Some(total)
    }

    fn apply_single_operation(
        &mut self,
        target: &JavaString,
        target_objective: ObjectiveId,
        operation: ScoreboardOperation,
        source: &JavaString,
        source_objective: ObjectiveId,
    ) -> Option<i32> {
        let left = self.score_by_id(target, target_objective).unwrap_or(0);
        let right = self.score_by_id(source, source_objective).unwrap_or(0);
        self.set_score_by_id(target, target_objective, left);
        self.set_score_by_id(source, source_objective, right);

        if matches!(operation, ScoreboardOperation::Swap) {
            self.set_score_by_id(target, target_objective, right);
            self.set_score_by_id(source, source_objective, left);
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
        self.set_score_by_id(target, target_objective, result);
        Some(result)
    }
}

fn scoreboard_count(count: usize, subject: &str) -> i32 {
    i32::try_from(count)
        .unwrap_or_else(|_| panic!("a scoreboard cannot contain more than i32::MAX {subject}"))
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
