use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::{
    execution_context::{ExecutionContext, mth_cos, mth_sin},
    java_math::round_float_to_int,
    nbt::{CommandStorage, JavaString, NbtPath, NbtSelection, Tag},
    predicate::{
        LootPredicate, PredicateReference, PredicateSet, builtin_predicates,
        parse_reference as parse_predicate_reference,
    },
    program::Scoreboard,
    random::LegacyRandom,
    resource::Identifier,
    resource_json,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProviderReference<T> {
    Named(Identifier),
    Inline(Box<T>),
}

pub(crate) type IntProviderReference = ProviderReference<IntProvider>;
pub(crate) type FloatProviderReference = ProviderReference<FloatProvider>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProviderSet<T> {
    Direct(Vec<ProviderReference<T>>),
    Tag(Identifier),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IntUnaryOperation {
    Absolute,
    Negate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IntBinaryOperation {
    Difference,
    FloorModulus,
    FloorQuotient,
    Modulus,
    Quotient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IntAggregateOperation {
    Average,
    Maximum,
    Minimum,
    Product,
    Sum,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum IntProvider {
    Constant(i32),
    Uniform {
        min: IntProviderReference,
        max: IntProviderReference,
    },
    Binomial {
        n: IntProviderReference,
        p: FloatProviderReference,
    },
    Storage {
        storage: Identifier,
        path: NbtPath,
        fallback: IntProviderReference,
    },
    Score {
        holder: JavaString,
        objective: JavaString,
        fallback: IntProviderReference,
    },
    Unary {
        operation: IntUnaryOperation,
        input: IntProviderReference,
    },
    Binary {
        operation: IntBinaryOperation,
        left: IntProviderReference,
        right: IntProviderReference,
    },
    Power {
        base: IntProviderReference,
        exponent: IntProviderReference,
    },
    Aggregate {
        operation: IntAggregateOperation,
        inputs: ProviderSet<IntProvider>,
    },
    FromFloat(FloatProviderReference),
    NumberDispatcher {
        cases: Vec<DispatcherCase<IntProvider>>,
        default: IntProviderReference,
    },
    Conditional {
        condition: PredicateReference,
        on_true: IntProviderReference,
        on_false: IntProviderReference,
    },
    WeightedList {
        distribution: Vec<WeightedProvider<IntProvider>>,
        total_weight: i32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FloatUnaryOperation {
    Absolute,
    Ceiling,
    Cosine,
    Floor,
    Negate,
    Round,
    Sine,
    SquareRoot,
    Truncate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FloatBinaryOperation {
    Difference,
    Modulus,
    Quotient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FloatAggregateOperation {
    Average,
    Length,
    Maximum,
    Minimum,
    Product,
    Sum,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FloatProvider {
    Constant(f32),
    Uniform {
        min: FloatProviderReference,
        max: FloatProviderReference,
    },
    Storage {
        storage: Identifier,
        path: NbtPath,
        fallback: FloatProviderReference,
    },
    Unary {
        operation: FloatUnaryOperation,
        input: FloatProviderReference,
    },
    Binary {
        operation: FloatBinaryOperation,
        left: FloatProviderReference,
        right: FloatProviderReference,
    },
    Power {
        base: FloatProviderReference,
        exponent: FloatProviderReference,
    },
    Aggregate {
        operation: FloatAggregateOperation,
        inputs: ProviderSet<FloatProvider>,
    },
    FromInt(IntProviderReference),
    NumberDispatcher {
        cases: Vec<DispatcherCase<FloatProvider>>,
        default: FloatProviderReference,
    },
    Conditional {
        condition: PredicateReference,
        on_true: FloatProviderReference,
        on_false: FloatProviderReference,
    },
    WeightedList {
        distribution: Vec<WeightedProvider<FloatProvider>>,
        total_weight: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WeightedProvider<T> {
    pub(crate) provider: ProviderReference<T>,
    pub(crate) weight: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DispatcherCase<T> {
    pub(crate) condition: PredicateReference,
    pub(crate) value: ProviderReference<T>,
}

enum ProviderValues<'a, T> {
    Direct {
        providers: &'a HashMap<Identifier, T>,
        values: std::slice::Iter<'a, ProviderReference<T>>,
    },
    Tag {
        providers: &'a HashMap<Identifier, T>,
        values: std::slice::Iter<'a, Identifier>,
    },
}

impl<'a, T> Iterator for ProviderValues<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Direct { providers, values } => values.next().map(|value| match value {
                ProviderReference::Named(id) => providers
                    .get(id)
                    .expect("provider references are validated before execution"),
                ProviderReference::Inline(provider) => provider,
            }),
            Self::Tag { providers, values } => values.next().map(|id| {
                providers
                    .get(id)
                    .expect("provider tags contain validated providers")
            }),
        }
    }
}

#[derive(Debug)]
enum EvaluationError {
    Arithmetic(String),
    Context(String),
    InvalidArgument(String),
}

impl EvaluationError {
    fn into_reason(self) -> String {
        match self {
            Self::Arithmetic(reason) | Self::Context(reason) | Self::InvalidArgument(reason) => {
                reason
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct LootRegistry {
    int_providers: HashMap<Identifier, IntProvider>,
    int_provider_tags: HashMap<Identifier, Vec<Identifier>>,
    float_providers: HashMap<Identifier, FloatProvider>,
    float_provider_tags: HashMap<Identifier, Vec<Identifier>>,
    predicates: HashMap<Identifier, LootPredicate>,
    predicate_tags: HashMap<Identifier, Vec<Identifier>>,
}

impl LootRegistry {
    pub(crate) fn new(
        int_providers: HashMap<Identifier, IntProvider>,
        int_provider_tags: HashMap<Identifier, Vec<Identifier>>,
        float_providers: HashMap<Identifier, FloatProvider>,
        float_provider_tags: HashMap<Identifier, Vec<Identifier>>,
        predicates: HashMap<Identifier, LootPredicate>,
        predicate_tags: HashMap<Identifier, Vec<Identifier>>,
    ) -> Result<Self, RegistryValidationError> {
        let user_resources = int_providers
            .keys()
            .cloned()
            .map(RegistryResource::IntProvider)
            .chain(
                float_providers
                    .keys()
                    .cloned()
                    .map(RegistryResource::FloatProvider),
            )
            .chain(predicates.keys().cloned().map(RegistryResource::Predicate))
            .collect::<HashSet<_>>();
        let mut all_int_providers = builtin_int_providers();
        all_int_providers.extend(int_providers);
        let mut all_float_providers = builtin_float_providers();
        all_float_providers.extend(float_providers);
        let mut all_predicates = builtin_predicates();
        all_predicates.extend(predicates);
        let registry = Self {
            int_providers: all_int_providers,
            int_provider_tags,
            float_providers: all_float_providers,
            float_provider_tags,
            predicates: all_predicates,
            predicate_tags,
        };
        registry.validate(&user_resources)?;
        Ok(registry)
    }

    pub(crate) fn empty() -> Self {
        Self::new(
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("the supported built-in loot resources are valid")
    }

    pub(crate) fn contains_int_provider(&self, id: &Identifier) -> bool {
        self.int_providers.contains_key(id)
    }

    pub(crate) fn int_provider_ids(&self) -> HashSet<Identifier> {
        self.int_providers.keys().cloned().collect()
    }

    pub(crate) fn contains_float_provider(&self, id: &Identifier) -> bool {
        self.float_providers.contains_key(id)
    }

    pub(crate) fn float_provider_ids(&self) -> HashSet<Identifier> {
        self.float_providers.keys().cloned().collect()
    }

    pub(crate) fn contains_predicate(&self, id: &Identifier) -> bool {
        self.predicates.contains_key(id)
    }

    pub(crate) fn predicate_ids(&self) -> HashSet<Identifier> {
        self.predicates.keys().cloned().collect()
    }

    pub(crate) fn validate_inline_int_provider(
        &self,
        provider: &IntProvider,
    ) -> Result<(), String> {
        self.collect_int_provider_dependencies(provider, &mut Vec::new())
    }

    pub(crate) fn validate_inline_float_provider(
        &self,
        provider: &FloatProvider,
    ) -> Result<(), String> {
        self.collect_float_provider_dependencies(provider, &mut Vec::new())
    }

    pub(crate) fn validate_inline_predicate(
        &self,
        predicate: &LootPredicate,
    ) -> Result<(), String> {
        self.collect_predicate_dependencies(predicate, &mut Vec::new())
    }

    pub(crate) fn get_int_unsafe(
        &self,
        provider: &IntProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<i32, String> {
        self.resolve_int_provider(provider)
            .get_int_unsafe(self, scoreboard, command_storage, execution_context, random)
            .map_err(EvaluationError::into_reason)
    }

    pub(crate) fn get_int(
        &self,
        provider: &IntProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<i32, String> {
        match self.resolve_int_provider(provider).get_int_unsafe(
            self,
            scoreboard,
            command_storage,
            execution_context,
            random,
        ) {
            Ok(value) => Ok(value),
            Err(EvaluationError::Arithmetic(_)) => Ok(0),
            Err(EvaluationError::Context(reason) | EvaluationError::InvalidArgument(reason)) => {
                Err(reason)
            }
        }
    }

    pub(crate) fn get_float_unsafe(
        &self,
        provider: &FloatProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<f32, String> {
        self.resolve_float_provider(provider)
            .get_float_unsafe(self, scoreboard, command_storage, execution_context, random)
            .map_err(EvaluationError::into_reason)
    }

    pub(crate) fn get_float(
        &self,
        provider: &FloatProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<f32, String> {
        match self.resolve_float_provider(provider).get_float_unsafe(
            self,
            scoreboard,
            command_storage,
            execution_context,
            random,
        ) {
            Ok(value) if value.is_finite() => Ok(value),
            Ok(_) | Err(EvaluationError::Arithmetic(_)) => Ok(0.0),
            Err(EvaluationError::Context(reason) | EvaluationError::InvalidArgument(reason)) => {
                Err(reason)
            }
        }
    }

    pub(crate) fn resolve_int_provider<'a>(
        &'a self,
        provider: &'a IntProviderReference,
    ) -> &'a IntProvider {
        resolve_provider(&self.int_providers, provider)
    }

    pub(crate) fn resolve_float_provider<'a>(
        &'a self,
        provider: &'a FloatProviderReference,
    ) -> &'a FloatProvider {
        resolve_provider(&self.float_providers, provider)
    }

    fn int_provider_values<'a>(
        &'a self,
        providers: &'a ProviderSet<IntProvider>,
    ) -> ProviderValues<'a, IntProvider> {
        provider_values(&self.int_providers, &self.int_provider_tags, providers)
    }

    fn float_provider_values<'a>(
        &'a self,
        providers: &'a ProviderSet<FloatProvider>,
    ) -> ProviderValues<'a, FloatProvider> {
        provider_values(&self.float_providers, &self.float_provider_tags, providers)
    }

    pub(crate) fn resolve_predicate<'a>(
        &'a self,
        predicate: &'a PredicateReference,
    ) -> &'a LootPredicate {
        match predicate {
            PredicateReference::Named(id) => self
                .predicates
                .get(id)
                .expect("predicate references are validated before execution"),
            PredicateReference::Inline(predicate) => predicate,
        }
    }

    pub(crate) fn predicate_values<'a>(
        &'a self,
        predicates: &'a PredicateSet,
    ) -> Box<dyn Iterator<Item = &'a LootPredicate> + 'a> {
        match predicates {
            PredicateSet::Direct(predicates) => Box::new(
                predicates
                    .iter()
                    .map(|predicate| self.resolve_predicate(predicate)),
            ),
            PredicateSet::Tag(tag) => Box::new(
                self.predicate_tags
                    .get(tag)
                    .expect("predicate tags are validated before execution")
                    .iter()
                    .map(|id| {
                        self.predicates
                            .get(id)
                            .expect("predicate tags contain validated predicates")
                    }),
            ),
        }
    }

    pub(crate) fn test_predicate(
        &self,
        predicate: &PredicateReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        self.resolve_predicate(predicate).test(
            self,
            scoreboard,
            command_storage,
            execution_context,
            random,
        )
    }

    fn validate(
        &self,
        user_resources: &HashSet<RegistryResource>,
    ) -> Result<(), RegistryValidationError> {
        let mut graph = HashMap::new();
        let mut resources = self
            .int_providers
            .keys()
            .cloned()
            .map(RegistryResource::IntProvider)
            .chain(
                self.float_providers
                    .keys()
                    .cloned()
                    .map(RegistryResource::FloatProvider),
            )
            .chain(
                self.predicates
                    .keys()
                    .cloned()
                    .map(RegistryResource::Predicate),
            )
            .collect::<Vec<_>>();
        resources.sort_by_key(RegistryResource::sort_key);
        for resource in &resources {
            let mut dependencies = Vec::new();
            match resource {
                RegistryResource::IntProvider(id) => self.collect_int_provider_dependencies(
                    self.int_providers
                        .get(id)
                        .expect("the identifier came from the int provider map"),
                    &mut dependencies,
                ),
                RegistryResource::FloatProvider(id) => self.collect_float_provider_dependencies(
                    self.float_providers
                        .get(id)
                        .expect("the identifier came from the float provider map"),
                    &mut dependencies,
                ),
                RegistryResource::Predicate(id) => self.collect_predicate_dependencies(
                    self.predicates
                        .get(id)
                        .expect("the identifier came from the predicate map"),
                    &mut dependencies,
                ),
            }
            .map_err(|reason| RegistryValidationError {
                resource: resource.clone(),
                reason,
            })?;
            dependencies.sort_by_key(RegistryResource::sort_key);
            dependencies.dedup();
            graph.insert(resource.clone(), dependencies);
        }

        let mut remaining = graph
            .iter()
            .map(|(resource, dependencies)| (resource.clone(), dependencies.len()))
            .collect::<HashMap<_, _>>();
        let mut dependents = resources
            .iter()
            .cloned()
            .map(|resource| (resource, Vec::new()))
            .collect::<HashMap<_, _>>();
        for (resource, dependencies) in &graph {
            for dependency in dependencies {
                dependents
                    .get_mut(dependency)
                    .expect("all loot resource dependencies are validated")
                    .push(resource.clone());
            }
        }

        let mut ready = resources
            .iter()
            .filter(|resource| remaining.get(*resource) == Some(&0))
            .cloned()
            .collect::<Vec<_>>();
        let mut next = 0;
        while let Some(resource) = ready.get(next).cloned() {
            next += 1;
            for dependent in dependents
                .get(&resource)
                .expect("every loot resource owns a dependents list")
            {
                let count = remaining
                    .get_mut(dependent)
                    .expect("every dependent is a loot resource");
                *count -= 1;
                if *count == 0 {
                    ready.push(dependent.clone());
                }
            }
        }

        if ready.len() != resources.len() {
            let resource = resources
                .into_iter()
                .filter(|resource| user_resources.contains(resource))
                .find(|resource| remaining.get(resource).is_some_and(|count| *count != 0))
                .or_else(|| {
                    remaining
                        .iter()
                        .find_map(|(resource, count)| (*count != 0).then(|| resource.clone()))
                })
                .expect("a cyclic graph retains at least one loot resource");
            return Err(RegistryValidationError {
                reason: format!(
                    "cyclic loot resource reference involving {} `{}`",
                    resource.kind(),
                    resource.id()
                ),
                resource,
            });
        }
        Ok(())
    }

    fn collect_int_provider_dependencies(
        &self,
        provider: &IntProvider,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match provider {
            IntProvider::Constant(_) => Ok(()),
            IntProvider::Uniform { min, max } => {
                self.collect_int_reference_dependencies(min, dependencies)?;
                self.collect_int_reference_dependencies(max, dependencies)
            }
            IntProvider::Binomial { n, p } => {
                self.collect_int_reference_dependencies(n, dependencies)?;
                self.collect_float_reference_dependencies(p, dependencies)
            }
            IntProvider::Storage { fallback, .. }
            | IntProvider::Score { fallback, .. }
            | IntProvider::Unary {
                input: fallback, ..
            } => self.collect_int_reference_dependencies(fallback, dependencies),
            IntProvider::Binary { left, right, .. } => {
                self.collect_int_reference_dependencies(left, dependencies)?;
                self.collect_int_reference_dependencies(right, dependencies)
            }
            IntProvider::Power { base, exponent } => {
                self.collect_int_reference_dependencies(base, dependencies)?;
                self.collect_int_reference_dependencies(exponent, dependencies)
            }
            IntProvider::Aggregate { inputs, .. } => {
                self.collect_int_set_dependencies(inputs, dependencies)
            }
            IntProvider::FromFloat(input) => {
                self.collect_float_reference_dependencies(input, dependencies)
            }
            IntProvider::NumberDispatcher { cases, default } => {
                for case in cases {
                    self.collect_predicate_reference_dependencies(&case.condition, dependencies)?;
                    self.collect_int_reference_dependencies(&case.value, dependencies)?;
                }
                self.collect_int_reference_dependencies(default, dependencies)
            }
            IntProvider::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                self.collect_predicate_reference_dependencies(condition, dependencies)?;
                self.collect_int_reference_dependencies(on_true, dependencies)?;
                self.collect_int_reference_dependencies(on_false, dependencies)
            }
            IntProvider::WeightedList { distribution, .. } => {
                for entry in distribution {
                    self.collect_int_reference_dependencies(&entry.provider, dependencies)?;
                }
                Ok(())
            }
        }
    }

    fn collect_float_provider_dependencies(
        &self,
        provider: &FloatProvider,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match provider {
            FloatProvider::Constant(_) => Ok(()),
            FloatProvider::Uniform { min, max } => {
                self.collect_float_reference_dependencies(min, dependencies)?;
                self.collect_float_reference_dependencies(max, dependencies)
            }
            FloatProvider::Storage { fallback, .. }
            | FloatProvider::Unary {
                input: fallback, ..
            } => self.collect_float_reference_dependencies(fallback, dependencies),
            FloatProvider::Binary { left, right, .. } => {
                self.collect_float_reference_dependencies(left, dependencies)?;
                self.collect_float_reference_dependencies(right, dependencies)
            }
            FloatProvider::Power { base, exponent } => {
                self.collect_float_reference_dependencies(base, dependencies)?;
                self.collect_float_reference_dependencies(exponent, dependencies)
            }
            FloatProvider::Aggregate { inputs, .. } => {
                self.collect_float_set_dependencies(inputs, dependencies)
            }
            FloatProvider::FromInt(input) => {
                self.collect_int_reference_dependencies(input, dependencies)
            }
            FloatProvider::NumberDispatcher { cases, default } => {
                for case in cases {
                    self.collect_predicate_reference_dependencies(&case.condition, dependencies)?;
                    self.collect_float_reference_dependencies(&case.value, dependencies)?;
                }
                self.collect_float_reference_dependencies(default, dependencies)
            }
            FloatProvider::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                self.collect_predicate_reference_dependencies(condition, dependencies)?;
                self.collect_float_reference_dependencies(on_true, dependencies)?;
                self.collect_float_reference_dependencies(on_false, dependencies)
            }
            FloatProvider::WeightedList { distribution, .. } => {
                for entry in distribution {
                    self.collect_float_reference_dependencies(&entry.provider, dependencies)?;
                }
                Ok(())
            }
        }
    }

    fn collect_int_set_dependencies(
        &self,
        providers: &ProviderSet<IntProvider>,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match providers {
            ProviderSet::Direct(values) => {
                if values.is_empty() {
                    return Err("provider `inputs` must contain at least one value".to_owned());
                }
                for value in values {
                    self.collect_int_reference_dependencies(value, dependencies)?;
                }
            }
            ProviderSet::Tag(tag) => {
                let values = self
                    .int_provider_tags
                    .get(tag)
                    .ok_or_else(|| format!("context int provider tag `#{tag}` does not exist"))?;
                if values.is_empty() {
                    return Err(format!(
                        "context int provider tag `#{tag}` must contain at least one value"
                    ));
                }
                dependencies.extend(values.iter().cloned().map(RegistryResource::IntProvider));
            }
        }
        Ok(())
    }

    fn collect_float_set_dependencies(
        &self,
        providers: &ProviderSet<FloatProvider>,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match providers {
            ProviderSet::Direct(values) => {
                if values.is_empty() {
                    return Err("provider `inputs` must contain at least one value".to_owned());
                }
                for value in values {
                    self.collect_float_reference_dependencies(value, dependencies)?;
                }
            }
            ProviderSet::Tag(tag) => {
                let values = self
                    .float_provider_tags
                    .get(tag)
                    .ok_or_else(|| format!("context float provider tag `#{tag}` does not exist"))?;
                if values.is_empty() {
                    return Err(format!(
                        "context float provider tag `#{tag}` must contain at least one value"
                    ));
                }
                dependencies.extend(values.iter().cloned().map(RegistryResource::FloatProvider));
            }
        }
        Ok(())
    }

    fn collect_predicate_dependencies(
        &self,
        predicate: &LootPredicate,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match predicate {
            LootPredicate::AllOf(predicates) | LootPredicate::AnyOf(predicates) => {
                match predicates {
                    PredicateSet::Direct(values) => {
                        for value in values {
                            self.collect_predicate_reference_dependencies(value, dependencies)?;
                        }
                    }
                    PredicateSet::Tag(tag) => {
                        let values = self
                            .predicate_tags
                            .get(tag)
                            .ok_or_else(|| format!("predicate tag `#{tag}` does not exist"))?;
                        dependencies
                            .extend(values.iter().cloned().map(RegistryResource::Predicate));
                    }
                }
                Ok(())
            }
            LootPredicate::Inverted(predicate) => {
                self.collect_predicate_reference_dependencies(predicate, dependencies)
            }
            LootPredicate::RandomChance { chance } => {
                self.collect_float_reference_dependencies(chance, dependencies)
            }
            LootPredicate::IntValueCheck { value, range } => {
                self.collect_int_reference_dependencies(value, dependencies)?;
                range.collect_dependencies(self, dependencies)
            }
            LootPredicate::FloatValueCheck { value, range } => {
                self.collect_float_reference_dependencies(value, dependencies)?;
                range.collect_dependencies(self, dependencies)
            }
            LootPredicate::AbsentContext {
                referenced_int_providers,
                ..
            } => {
                for provider in referenced_int_providers {
                    self.collect_int_reference_dependencies(provider, dependencies)?;
                }
                Ok(())
            }
            LootPredicate::LocationCheck { .. } | LootPredicate::MissingContextParameter { .. } => {
                Ok(())
            }
        }
    }

    pub(crate) fn collect_int_reference_dependencies(
        &self,
        provider: &IntProviderReference,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match provider {
            ProviderReference::Named(id) => {
                if !self.int_providers.contains_key(id) {
                    return Err(format!("context int provider `{id}` does not exist"));
                }
                dependencies.push(RegistryResource::IntProvider(id.clone()));
                Ok(())
            }
            ProviderReference::Inline(provider) => {
                self.collect_int_provider_dependencies(provider, dependencies)
            }
        }
    }

    pub(crate) fn collect_float_reference_dependencies(
        &self,
        provider: &FloatProviderReference,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match provider {
            ProviderReference::Named(id) => {
                if !self.float_providers.contains_key(id) {
                    return Err(format!("context float provider `{id}` does not exist"));
                }
                dependencies.push(RegistryResource::FloatProvider(id.clone()));
                Ok(())
            }
            ProviderReference::Inline(provider) => {
                self.collect_float_provider_dependencies(provider, dependencies)
            }
        }
    }

    fn collect_predicate_reference_dependencies(
        &self,
        predicate: &PredicateReference,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match predicate {
            PredicateReference::Named(id) => {
                if !self.predicates.contains_key(id) {
                    return Err(format!("predicate `{id}` does not exist"));
                }
                dependencies.push(RegistryResource::Predicate(id.clone()));
                Ok(())
            }
            PredicateReference::Inline(predicate) => {
                self.collect_predicate_dependencies(predicate, dependencies)
            }
        }
    }
}

fn resolve_provider<'a, T>(
    providers: &'a HashMap<Identifier, T>,
    provider: &'a ProviderReference<T>,
) -> &'a T {
    match provider {
        ProviderReference::Named(id) => providers
            .get(id)
            .expect("provider references are validated before execution"),
        ProviderReference::Inline(provider) => provider,
    }
}

fn provider_values<'a, T>(
    providers: &'a HashMap<Identifier, T>,
    tags: &'a HashMap<Identifier, Vec<Identifier>>,
    set: &'a ProviderSet<T>,
) -> ProviderValues<'a, T> {
    match set {
        ProviderSet::Direct(values) => ProviderValues::Direct {
            providers,
            values: values.iter(),
        },
        ProviderSet::Tag(tag) => ProviderValues::Tag {
            providers,
            values: tags
                .get(tag)
                .expect("provider tags are validated before execution")
                .iter(),
        },
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RegistryResource {
    IntProvider(Identifier),
    FloatProvider(Identifier),
    Predicate(Identifier),
}

impl RegistryResource {
    fn sort_key(&self) -> (u8, String) {
        match self {
            Self::IntProvider(id) => (0, id.to_string()),
            Self::FloatProvider(id) => (1, id.to_string()),
            Self::Predicate(id) => (2, id.to_string()),
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::IntProvider(_) => "context int provider",
            Self::FloatProvider(_) => "context float provider",
            Self::Predicate(_) => "predicate",
        }
    }

    pub(crate) fn id(&self) -> &Identifier {
        match self {
            Self::IntProvider(id) | Self::FloatProvider(id) | Self::Predicate(id) => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryValidationError {
    pub(crate) resource: RegistryResource,
    pub(crate) reason: String,
}

impl IntProvider {
    #[allow(clippy::too_many_arguments)]
    fn get_int_unsafe(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<i32, EvaluationError> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Uniform { min, max } => {
                let min = evaluate_int(
                    registry,
                    min,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let max = evaluate_int(
                    registry,
                    max,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                if min >= max {
                    Ok(min)
                } else {
                    let bound = max.wrapping_sub(min).wrapping_add(1);
                    random
                        .next_int(bound)
                        .map(|value| value.wrapping_add(min))
                        .map_err(EvaluationError::InvalidArgument)
                }
            }
            Self::Binomial { n, p } => {
                let n = evaluate_int(
                    registry,
                    n,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let p = evaluate_float(
                    registry,
                    p,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                if !p.is_finite() {
                    return Err(EvaluationError::Arithmetic(format!(
                        "invalid binomial probability {p}"
                    )));
                }
                let mut result = 0;
                for _ in 0..n.max(0) {
                    if random.next_float() < p {
                        result += 1;
                    }
                }
                Ok(result)
            }
            Self::Storage {
                storage,
                path,
                fallback,
            } => match storage_number(command_storage, storage, path)
                .as_ref()
                .and_then(NbtSelection::as_tag)
                .and_then(tag_boxed_int_value)
            {
                Some(value) => Ok(value),
                None => evaluate_int(
                    registry,
                    fallback,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                ),
            },
            Self::Score {
                holder,
                objective,
                fallback,
            } => match String::from_utf16(objective.units())
                .ok()
                .and_then(|objective| scoreboard.score(holder, &objective))
            {
                Some(value) => Ok(value),
                None => evaluate_int(
                    registry,
                    fallback,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                ),
            },
            Self::Unary { operation, input } => {
                let input = evaluate_int(
                    registry,
                    input,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                match operation {
                    IntUnaryOperation::Absolute => input.checked_abs().ok_or_else(|| {
                        EvaluationError::Arithmetic(format!(
                            "integer absolute value overflow for {input}"
                        ))
                    }),
                    IntUnaryOperation::Negate => input.checked_neg().ok_or_else(|| {
                        EvaluationError::Arithmetic(format!(
                            "integer negation overflow for {input}"
                        ))
                    }),
                }
            }
            Self::Binary {
                operation,
                left,
                right,
            } => {
                let left = evaluate_int(
                    registry,
                    left,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let right = evaluate_int(
                    registry,
                    right,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                evaluate_int_binary(*operation, left, right)
            }
            Self::Power { base, exponent } => {
                let base = evaluate_int(
                    registry,
                    base,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let exponent = evaluate_int(
                    registry,
                    exponent,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                int_pow_exact(base, exponent)
            }
            Self::Aggregate { operation, inputs } => match operation {
                IntAggregateOperation::Average => {
                    let mut sum = 0_i64;
                    let mut count = 0_i64;
                    for provider in registry.int_provider_values(inputs) {
                        sum = sum.wrapping_add(i64::from(provider.get_int_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?));
                        count += 1;
                    }
                    long_to_int_safe(sum / count)
                }
                IntAggregateOperation::Maximum => {
                    let mut value = i32::MIN;
                    for provider in registry.int_provider_values(inputs) {
                        value = value.max(provider.get_int_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?);
                    }
                    Ok(value)
                }
                IntAggregateOperation::Minimum => {
                    let mut value = i32::MAX;
                    for provider in registry.int_provider_values(inputs) {
                        value = value.min(provider.get_int_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?);
                    }
                    Ok(value)
                }
                IntAggregateOperation::Product => {
                    let mut value = 1_i64;
                    for provider in registry.int_provider_values(inputs) {
                        value = value.wrapping_mul(i64::from(provider.get_int_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?));
                    }
                    long_to_int_safe(value)
                }
                IntAggregateOperation::Sum => {
                    let mut value = 0_i64;
                    for provider in registry.int_provider_values(inputs) {
                        value = value.wrapping_add(i64::from(provider.get_int_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?));
                    }
                    long_to_int_safe(value)
                }
            },
            Self::FromFloat(input) => {
                let input = evaluate_float(
                    registry,
                    input,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                float_to_int_safe(input)
            }
            Self::NumberDispatcher { cases, default } => {
                let mut selected = default;
                for case in cases {
                    if registry
                        .test_predicate(
                            &case.condition,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )
                        .map_err(EvaluationError::Context)?
                    {
                        selected = &case.value;
                        break;
                    }
                }
                evaluate_int(
                    registry,
                    selected,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
            Self::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                let selected = if registry
                    .test_predicate(
                        condition,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )
                    .map_err(EvaluationError::Context)?
                {
                    on_true
                } else {
                    on_false
                };
                evaluate_int(
                    registry,
                    selected,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
            Self::WeightedList {
                distribution,
                total_weight,
            } => {
                let selected = random
                    .next_int(*total_weight)
                    .map_err(EvaluationError::Arithmetic)?;
                evaluate_int(
                    registry,
                    select_weighted(distribution, selected),
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
        }
    }
}

impl FloatProvider {
    #[allow(clippy::too_many_arguments)]
    fn get_float_unsafe(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<f32, EvaluationError> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Uniform { min, max } => {
                let min = evaluate_float(
                    registry,
                    min,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let max = evaluate_float(
                    registry,
                    max,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(if min >= max {
                    min
                } else {
                    random.next_float() * (max - min) + min
                })
            }
            Self::Storage {
                storage,
                path,
                fallback,
            } => match storage_number(command_storage, storage, path)
                .as_ref()
                .and_then(NbtSelection::as_tag)
                .and_then(tag_float_value)
            {
                Some(value) => Ok(value),
                None => evaluate_float(
                    registry,
                    fallback,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                ),
            },
            Self::Unary { operation, input } => {
                let input = evaluate_float(
                    registry,
                    input,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(match operation {
                    FloatUnaryOperation::Absolute => input.abs(),
                    FloatUnaryOperation::Ceiling => (input.ceil() as i32) as f32,
                    FloatUnaryOperation::Cosine => mth_cos(f64::from(input)),
                    FloatUnaryOperation::Floor => input.floor(),
                    FloatUnaryOperation::Negate => -input,
                    FloatUnaryOperation::Round => round_float_to_int(input) as f32,
                    FloatUnaryOperation::Sine => mth_sin(f64::from(input)),
                    FloatUnaryOperation::SquareRoot => libm::sqrt(f64::from(input)) as f32,
                    FloatUnaryOperation::Truncate => {
                        if input > 0.0 {
                            input.floor()
                        } else {
                            input.ceil()
                        }
                    }
                })
            }
            Self::Binary {
                operation,
                left,
                right,
            } => match operation {
                FloatBinaryOperation::Modulus => {
                    let right = evaluate_float(
                        registry,
                        right,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                    if right == 0.0 {
                        return Ok(f32::NAN);
                    }
                    let left = evaluate_float(
                        registry,
                        left,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                    Ok(((left % right) + right) % right)
                }
                FloatBinaryOperation::Difference | FloatBinaryOperation::Quotient => {
                    let left = evaluate_float(
                        registry,
                        left,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                    let right = evaluate_float(
                        registry,
                        right,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                    Ok(match operation {
                        FloatBinaryOperation::Difference => left - right,
                        FloatBinaryOperation::Quotient => left / right,
                        FloatBinaryOperation::Modulus => unreachable!(
                            "float modulus is evaluated in its operand-order-specific branch"
                        ),
                    })
                }
            },
            Self::Power { base, exponent } => {
                let base = evaluate_float(
                    registry,
                    base,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let exponent = evaluate_float(
                    registry,
                    exponent,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(java_float_pow(base, exponent))
            }
            Self::Aggregate { operation, inputs } => match operation {
                FloatAggregateOperation::Average => {
                    let mut sum = 0.0_f32;
                    let mut count = 0_u32;
                    for provider in registry.float_provider_values(inputs) {
                        sum += provider.get_float_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?;
                        count += 1;
                    }
                    Ok(sum / count as f32)
                }
                FloatAggregateOperation::Length => {
                    let mut sum_of_squares = 0.0_f32;
                    for provider in registry.float_provider_values(inputs) {
                        let value = provider.get_float_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?;
                        sum_of_squares += value * value;
                    }
                    Ok(libm::sqrt(f64::from(sum_of_squares)) as f32)
                }
                FloatAggregateOperation::Maximum => {
                    let mut value = -f32::MAX;
                    for provider in registry.float_provider_values(inputs) {
                        value = java_max(
                            value,
                            provider.get_float_unsafe(
                                registry,
                                scoreboard,
                                command_storage,
                                execution_context,
                                random,
                            )?,
                        );
                    }
                    Ok(value)
                }
                FloatAggregateOperation::Minimum => {
                    let mut value = f32::MAX;
                    for provider in registry.float_provider_values(inputs) {
                        value = java_min(
                            value,
                            provider.get_float_unsafe(
                                registry,
                                scoreboard,
                                command_storage,
                                execution_context,
                                random,
                            )?,
                        );
                    }
                    Ok(value)
                }
                FloatAggregateOperation::Product => {
                    let mut value = 1.0_f32;
                    for provider in registry.float_provider_values(inputs) {
                        value *= provider.get_float_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?;
                    }
                    Ok(value)
                }
                FloatAggregateOperation::Sum => {
                    let mut value = 0.0_f32;
                    for provider in registry.float_provider_values(inputs) {
                        value += provider.get_float_unsafe(
                            registry,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?;
                    }
                    Ok(value)
                }
            },
            Self::FromInt(input) => evaluate_int(
                registry,
                input,
                scoreboard,
                command_storage,
                execution_context,
                random,
            )
            .map(|value| value as f32),
            Self::NumberDispatcher { cases, default } => {
                let mut selected = default;
                for case in cases {
                    if registry
                        .test_predicate(
                            &case.condition,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )
                        .map_err(EvaluationError::Context)?
                    {
                        selected = &case.value;
                        break;
                    }
                }
                evaluate_float(
                    registry,
                    selected,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
            Self::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                let selected = if registry
                    .test_predicate(
                        condition,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )
                    .map_err(EvaluationError::Context)?
                {
                    on_true
                } else {
                    on_false
                };
                evaluate_float(
                    registry,
                    selected,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
            Self::WeightedList {
                distribution,
                total_weight,
            } => {
                let selected = random
                    .next_int(*total_weight)
                    .map_err(EvaluationError::Arithmetic)?;
                evaluate_float(
                    registry,
                    select_weighted(distribution, selected),
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_int(
    registry: &LootRegistry,
    provider: &IntProviderReference,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
) -> Result<i32, EvaluationError> {
    registry.resolve_int_provider(provider).get_int_unsafe(
        registry,
        scoreboard,
        command_storage,
        execution_context,
        random,
    )
}

#[allow(clippy::too_many_arguments)]
fn evaluate_float(
    registry: &LootRegistry,
    provider: &FloatProviderReference,
    scoreboard: &Scoreboard,
    command_storage: &CommandStorage,
    execution_context: &ExecutionContext,
    random: &mut LegacyRandom,
) -> Result<f32, EvaluationError> {
    registry.resolve_float_provider(provider).get_float_unsafe(
        registry,
        scoreboard,
        command_storage,
        execution_context,
        random,
    )
}

fn evaluate_int_binary(
    operation: IntBinaryOperation,
    left: i32,
    right: i32,
) -> Result<i32, EvaluationError> {
    match operation {
        IntBinaryOperation::Difference => left.checked_sub(right).ok_or_else(|| {
            EvaluationError::Arithmetic(format!(
                "integer subtraction overflow for {left} - {right}"
            ))
        }),
        IntBinaryOperation::FloorModulus => {
            if right == 0 {
                return Err(EvaluationError::Arithmetic(
                    "integer division by zero".to_owned(),
                ));
            }
            let quotient = if left == i32::MIN && right == -1 {
                i32::MIN
            } else {
                let quotient = left / right;
                let remainder = left % right;
                if remainder != 0 && (left < 0) != (right < 0) {
                    quotient - 1
                } else {
                    quotient
                }
            };
            Ok((i64::from(left) - i64::from(quotient) * i64::from(right)) as i32)
        }
        IntBinaryOperation::FloorQuotient => floor_div_exact(left, right),
        IntBinaryOperation::Modulus => {
            if right == 0 {
                Err(EvaluationError::Arithmetic(
                    "integer division by zero".to_owned(),
                ))
            } else if left == i32::MIN && right == -1 {
                Ok(0)
            } else {
                Ok(left % right)
            }
        }
        IntBinaryOperation::Quotient => {
            if right == 0 {
                Err(EvaluationError::Arithmetic(
                    "integer division by zero".to_owned(),
                ))
            } else if left == i32::MIN && right == -1 {
                Ok(i32::MIN)
            } else {
                Ok(left / right)
            }
        }
    }
}

fn floor_div_exact(left: i32, right: i32) -> Result<i32, EvaluationError> {
    if right == 0 {
        return Err(EvaluationError::Arithmetic(
            "integer division by zero".to_owned(),
        ));
    }
    if left == i32::MIN && right == -1 {
        return Err(EvaluationError::Arithmetic(format!(
            "integer division overflow for {left} / {right}"
        )));
    }
    let quotient = left / right;
    let remainder = left % right;
    Ok(if remainder != 0 && (left < 0) != (right < 0) {
        quotient - 1
    } else {
        quotient
    })
}

fn int_pow_exact(base: i32, exponent: i32) -> Result<i32, EvaluationError> {
    if base == 0 && exponent == 0 {
        return Err(EvaluationError::Arithmetic(
            "result of 0 to the power of 0 is undefined".to_owned(),
        ));
    }
    let exponent = u32::try_from(exponent).map_err(|_| {
        EvaluationError::Arithmetic(format!("negative integer exponent {exponent}"))
    })?;
    base.checked_pow(exponent).ok_or_else(|| {
        EvaluationError::Arithmetic(format!(
            "integer power overflow for {base} to exponent {exponent}"
        ))
    })
}

fn java_float_pow(base: f32, exponent: f32) -> f32 {
    if exponent.is_nan() || (exponent.is_infinite() && base.abs() == 1.0) {
        f32::NAN
    } else {
        libm::pow(f64::from(base), f64::from(exponent)) as f32
    }
}

fn long_to_int_safe(value: i64) -> Result<i32, EvaluationError> {
    i32::try_from(value).map_err(|_| {
        EvaluationError::Arithmetic(format!("value {value} cannot be safely converted to int"))
    })
}

fn float_to_int_safe(value: f32) -> Result<i32, EvaluationError> {
    if !value.is_finite() || !(-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        return Err(EvaluationError::Arithmetic(format!(
            "value {value} cannot be safely converted to int"
        )));
    }
    Ok(value as i32)
}

fn select_weighted<T>(
    distribution: &[WeightedProvider<T>],
    mut selected: i32,
) -> &ProviderReference<T> {
    for entry in distribution {
        selected -= entry.weight;
        if selected < 0 {
            return &entry.provider;
        }
    }
    unreachable!("weighted provider selection is below the validated total weight")
}

pub(crate) fn parse_int_json(contents: &str) -> Result<IntProvider, String> {
    let value = resource_json::parse(contents)?;
    parse_int_direct(Input::Json(&value), "root")
}

pub(crate) fn parse_float_json(contents: &str) -> Result<FloatProvider, String> {
    let value = resource_json::parse(contents)?;
    parse_float_direct(Input::Json(&value), "root")
}

pub(crate) fn parse_inline_int_tag(
    value: &Tag,
    registry: &LootRegistry,
) -> Result<IntProvider, String> {
    let provider = parse_int_direct(Input::Nbt(value), "provider")?;
    registry.validate_inline_int_provider(&provider)?;
    Ok(provider)
}

pub(crate) fn parse_inline_float_tag(
    value: &Tag,
    registry: &LootRegistry,
) -> Result<FloatProvider, String> {
    let provider = parse_float_direct(Input::Nbt(value), "provider")?;
    registry.validate_inline_float_provider(&provider)?;
    Ok(provider)
}

fn parse_int_direct(input: Input<'_>, path: &str) -> Result<IntProvider, String> {
    if input.number().is_some() {
        return int_value(input, path).map(IntProvider::Constant);
    }
    if !input.is_object() {
        return Err(format!("`{path}` must be a number or an object"));
    }
    let provider_type = identifier_field(input, path, "type")?;
    if provider_type.namespace() != "minecraft" {
        return Err(format!(
            "context int provider type `{provider_type}` is not supported"
        ));
    }
    match provider_type.path() {
        "constant" => Ok(IntProvider::Constant(int_field(input, path, "value")?)),
        "uniform" => Ok(IntProvider::Uniform {
            min: int_reference_field(input, path, "min")?,
            max: int_reference_field(input, path, "max")?,
        }),
        "binomial" => Ok(IntProvider::Binomial {
            n: int_reference_field(input, path, "n")?,
            p: float_reference_field(input, path, "p")?,
        }),
        "storage" => Ok(IntProvider::Storage {
            storage: identifier_field(input, path, "storage")?,
            path: nbt_path_field(input, path, "path")?,
            fallback: optional_int_reference_field(input, path, "fallback")?
                .unwrap_or_else(int_zero_reference),
        }),
        "score" => Ok(IntProvider::Score {
            holder: fixed_score_holder(input, path)?,
            objective: string_field(input, path, "score")?,
            fallback: optional_int_reference_field(input, path, "fallback")?
                .unwrap_or_else(int_zero_reference),
        }),
        "abs" => Ok(IntProvider::Unary {
            operation: IntUnaryOperation::Absolute,
            input: int_reference_field(input, path, "input")?,
        }),
        "negate" => Ok(IntProvider::Unary {
            operation: IntUnaryOperation::Negate,
            input: int_reference_field(input, path, "input")?,
        }),
        "sub" => parse_int_binary(input, path, IntBinaryOperation::Difference),
        "floor_mod" => parse_int_binary(input, path, IntBinaryOperation::FloorModulus),
        "floor_div" => parse_int_binary(input, path, IntBinaryOperation::FloorQuotient),
        "mod" => parse_int_binary(input, path, IntBinaryOperation::Modulus),
        "div" => parse_int_binary(input, path, IntBinaryOperation::Quotient),
        "pow" => Ok(IntProvider::Power {
            base: int_reference_field(input, path, "base")?,
            exponent: int_reference_field(input, path, "exponent")?,
        }),
        "avg" => parse_int_aggregate(input, path, IntAggregateOperation::Average),
        "max" => parse_int_aggregate(input, path, IntAggregateOperation::Maximum),
        "min" => parse_int_aggregate(input, path, IntAggregateOperation::Minimum),
        "mul" => parse_int_aggregate(input, path, IntAggregateOperation::Product),
        "add" => parse_int_aggregate(input, path, IntAggregateOperation::Sum),
        "from_float" => Ok(IntProvider::FromFloat(float_reference_field(
            input, path, "input",
        )?)),
        "weighted_list" => {
            let (distribution, total_weight) =
                parse_weighted_list(input, path, parse_int_reference)?;
            Ok(IntProvider::WeightedList {
                distribution,
                total_weight,
            })
        }
        "number_dispatcher" => parse_int_dispatcher(input, path),
        "conditional" => parse_int_conditional(input, path),
        "environment_attribute" => Err(format!(
            "context int provider type `{provider_type}` depends on a physical-world loot context"
        )),
        _ => Err(format!(
            "context int provider type `{provider_type}` is not supported"
        )),
    }
}

fn parse_float_direct(input: Input<'_>, path: &str) -> Result<FloatProvider, String> {
    if let Some(value) = input.number() {
        return Ok(FloatProvider::Constant(value));
    }
    if !input.is_object() {
        return Err(format!("`{path}` must be a number or an object"));
    }
    let provider_type = identifier_field(input, path, "type")?;
    if provider_type.namespace() != "minecraft" {
        return Err(format!(
            "context float provider type `{provider_type}` is not supported"
        ));
    }
    match provider_type.path() {
        "constant" => Ok(FloatProvider::Constant(float_field(input, path, "value")?)),
        "uniform" => Ok(FloatProvider::Uniform {
            min: float_reference_field(input, path, "min")?,
            max: float_reference_field(input, path, "max")?,
        }),
        "storage" => Ok(FloatProvider::Storage {
            storage: identifier_field(input, path, "storage")?,
            path: nbt_path_field(input, path, "path")?,
            fallback: optional_float_reference_field(input, path, "fallback")?
                .unwrap_or_else(float_zero_reference),
        }),
        "abs" => parse_float_unary(input, path, FloatUnaryOperation::Absolute),
        "ceil" => parse_float_unary(input, path, FloatUnaryOperation::Ceiling),
        "cos" => parse_float_unary(input, path, FloatUnaryOperation::Cosine),
        "floor" => parse_float_unary(input, path, FloatUnaryOperation::Floor),
        "negate" => parse_float_unary(input, path, FloatUnaryOperation::Negate),
        "round" => parse_float_unary(input, path, FloatUnaryOperation::Round),
        "sin" => parse_float_unary(input, path, FloatUnaryOperation::Sine),
        "sqrt" => parse_float_unary(input, path, FloatUnaryOperation::SquareRoot),
        "truncate" => parse_float_unary(input, path, FloatUnaryOperation::Truncate),
        "sub" => parse_float_binary(input, path, FloatBinaryOperation::Difference),
        "mod" => parse_float_binary(input, path, FloatBinaryOperation::Modulus),
        "div" => parse_float_binary(input, path, FloatBinaryOperation::Quotient),
        "pow" => Ok(FloatProvider::Power {
            base: float_reference_field(input, path, "base")?,
            exponent: float_reference_field(input, path, "exponent")?,
        }),
        "avg" => parse_float_aggregate(input, path, FloatAggregateOperation::Average),
        "length" => parse_float_aggregate(input, path, FloatAggregateOperation::Length),
        "max" => parse_float_aggregate(input, path, FloatAggregateOperation::Maximum),
        "min" => parse_float_aggregate(input, path, FloatAggregateOperation::Minimum),
        "mul" => parse_float_aggregate(input, path, FloatAggregateOperation::Product),
        "add" => parse_float_aggregate(input, path, FloatAggregateOperation::Sum),
        "from_int" => Ok(FloatProvider::FromInt(int_reference_field(
            input, path, "input",
        )?)),
        "weighted_list" => {
            let (distribution, total_weight) =
                parse_weighted_list(input, path, parse_float_reference)?;
            Ok(FloatProvider::WeightedList {
                distribution,
                total_weight,
            })
        }
        "number_dispatcher" => parse_float_dispatcher(input, path),
        "conditional" => parse_float_conditional(input, path),
        "environment_attribute" => Err(format!(
            "context float provider type `{provider_type}` depends on a physical-world loot context"
        )),
        "enchantment_level" => Err(format!(
            "context float provider type `{provider_type}` requires an enchantment loot context"
        )),
        _ => Err(format!(
            "context float provider type `{provider_type}` is not supported"
        )),
    }
}

fn parse_int_binary(
    input: Input<'_>,
    path: &str,
    operation: IntBinaryOperation,
) -> Result<IntProvider, String> {
    Ok(IntProvider::Binary {
        operation,
        left: int_reference_field(input, path, "left")?,
        right: int_reference_field(input, path, "right")?,
    })
}

fn parse_float_binary(
    input: Input<'_>,
    path: &str,
    operation: FloatBinaryOperation,
) -> Result<FloatProvider, String> {
    Ok(FloatProvider::Binary {
        operation,
        left: float_reference_field(input, path, "left")?,
        right: float_reference_field(input, path, "right")?,
    })
}

fn parse_float_unary(
    input: Input<'_>,
    path: &str,
    operation: FloatUnaryOperation,
) -> Result<FloatProvider, String> {
    Ok(FloatProvider::Unary {
        operation,
        input: float_reference_field(input, path, "input")?,
    })
}

fn parse_int_aggregate(
    input: Input<'_>,
    path: &str,
    operation: IntAggregateOperation,
) -> Result<IntProvider, String> {
    Ok(IntProvider::Aggregate {
        operation,
        inputs: provider_set_field(input, path, "inputs", parse_int_reference)?,
    })
}

fn parse_float_aggregate(
    input: Input<'_>,
    path: &str,
    operation: FloatAggregateOperation,
) -> Result<FloatProvider, String> {
    Ok(FloatProvider::Aggregate {
        operation,
        inputs: provider_set_field(input, path, "inputs", parse_float_reference)?,
    })
}

fn parse_int_dispatcher(input: Input<'_>, path: &str) -> Result<IntProvider, String> {
    let cases_path = format!("{path}.cases");
    let cases = required_field(input, path, "cases")?
        .list()
        .ok_or_else(|| format!("`{cases_path}` must be a list"))?;
    let mut parsed_cases = Vec::with_capacity(cases.len());
    for (index, case) in cases.into_iter().enumerate() {
        let case_path = format!("{cases_path}[{index}]");
        if !case.is_object() {
            return Err(format!("`{case_path}` must be an object"));
        }
        parsed_cases.push(DispatcherCase {
            condition: parse_predicate_reference(
                required_field(case, &case_path, "condition")?,
                &format!("{case_path}.condition"),
            )?,
            value: parse_int_reference(
                required_field(case, &case_path, "value")?,
                &format!("{case_path}.value"),
            )?,
        });
    }
    let default =
        optional_int_reference_field(input, path, "default")?.unwrap_or_else(int_zero_reference);
    Ok(IntProvider::NumberDispatcher {
        cases: parsed_cases,
        default,
    })
}

fn parse_float_dispatcher(input: Input<'_>, path: &str) -> Result<FloatProvider, String> {
    let cases_path = format!("{path}.cases");
    let cases = required_field(input, path, "cases")?
        .list()
        .ok_or_else(|| format!("`{cases_path}` must be a list"))?;
    let mut parsed_cases = Vec::with_capacity(cases.len());
    for (index, case) in cases.into_iter().enumerate() {
        let case_path = format!("{cases_path}[{index}]");
        if !case.is_object() {
            return Err(format!("`{case_path}` must be an object"));
        }
        parsed_cases.push(DispatcherCase {
            condition: parse_predicate_reference(
                required_field(case, &case_path, "condition")?,
                &format!("{case_path}.condition"),
            )?,
            value: parse_float_reference(
                required_field(case, &case_path, "value")?,
                &format!("{case_path}.value"),
            )?,
        });
    }
    let default = optional_float_reference_field(input, path, "default")?
        .unwrap_or_else(float_zero_reference);
    Ok(FloatProvider::NumberDispatcher {
        cases: parsed_cases,
        default,
    })
}

fn parse_int_conditional(input: Input<'_>, path: &str) -> Result<IntProvider, String> {
    Ok(IntProvider::Conditional {
        condition: parse_predicate_reference(
            required_field(input, path, "condition")?,
            &format!("{path}.condition"),
        )?,
        on_true: int_reference_field(input, path, "on_true")?,
        on_false: optional_int_reference_field(input, path, "on_false")?
            .unwrap_or_else(int_zero_reference),
    })
}

fn parse_float_conditional(input: Input<'_>, path: &str) -> Result<FloatProvider, String> {
    Ok(FloatProvider::Conditional {
        condition: parse_predicate_reference(
            required_field(input, path, "condition")?,
            &format!("{path}.condition"),
        )?,
        on_true: float_reference_field(input, path, "on_true")?,
        on_false: optional_float_reference_field(input, path, "on_false")?
            .unwrap_or_else(float_zero_reference),
    })
}

fn int_zero_reference() -> IntProviderReference {
    ProviderReference::Inline(Box::new(IntProvider::Constant(0)))
}

fn float_zero_reference() -> FloatProviderReference {
    ProviderReference::Inline(Box::new(FloatProvider::Constant(0.0)))
}

fn parse_weighted_list<T>(
    input: Input<'_>,
    path: &str,
    parse_reference: fn(Input<'_>, &str) -> Result<ProviderReference<T>, String>,
) -> Result<(Vec<WeightedProvider<T>>, i32), String> {
    let distribution_path = format!("{path}.distribution");
    let distribution = required_field(input, path, "distribution")?
        .list()
        .ok_or_else(|| format!("`{distribution_path}` must be a list"))?;
    let mut entries = Vec::with_capacity(distribution.len());
    let mut total_weight = 0_i64;
    for (index, entry) in distribution.into_iter().enumerate() {
        let entry_path = format!("{distribution_path}[{index}]");
        if !entry.is_object() {
            return Err(format!("`{entry_path}` must be an object"));
        }
        let provider = parse_reference(
            required_field(entry, &entry_path, "data")?,
            &format!("{entry_path}.data"),
        )?;
        let weight = int_field(entry, &entry_path, "weight")?;
        if weight < 0 {
            return Err(format!("`{entry_path}.weight` must be non-negative"));
        }
        total_weight += i64::from(weight);
        if total_weight > i64::from(i32::MAX) {
            return Err(format!(
                "`{distribution_path}` total weight must not exceed {}",
                i32::MAX
            ));
        }
        entries.push(WeightedProvider { provider, weight });
    }
    if total_weight == 0 {
        return Err(format!(
            "`{distribution_path}` must contain at least one entry with non-zero weight"
        ));
    }
    Ok((entries, total_weight as i32))
}

fn provider_set_field<T>(
    input: Input<'_>,
    path: &str,
    field: &str,
    parse_reference: fn(Input<'_>, &str) -> Result<ProviderReference<T>, String>,
) -> Result<ProviderSet<T>, String> {
    let field_path = format!("{path}.{field}");
    let value = required_field(input, path, field)?;
    if let Some(string) = value.string() {
        let text = ascii_string(&string, &field_path)?;
        if let Some(tag) = text.strip_prefix('#') {
            let id = Identifier::parse(tag)
                .ok_or_else(|| format!("`{field_path}` has invalid tag identifier `{text}`"))?;
            return Ok(ProviderSet::Tag(id));
        }
    }
    let values = value.list().unwrap_or_else(|| vec![value]);
    let providers = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| parse_reference(value, &format!("{field_path}[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProviderSet::Direct(providers))
}

pub(crate) fn parse_int_reference(
    input: Input<'_>,
    path: &str,
) -> Result<IntProviderReference, String> {
    parse_provider_reference(input, path, "context int provider", parse_int_direct)
}

pub(crate) fn parse_float_reference(
    input: Input<'_>,
    path: &str,
) -> Result<FloatProviderReference, String> {
    parse_provider_reference(input, path, "context float provider", parse_float_direct)
}

fn parse_provider_reference<T>(
    input: Input<'_>,
    path: &str,
    kind: &str,
    parse_direct: fn(Input<'_>, &str) -> Result<T, String>,
) -> Result<ProviderReference<T>, String> {
    if let Some(value) = input.string() {
        let value = ascii_string(&value, path)?;
        let id = Identifier::parse(&value)
            .ok_or_else(|| format!("`{path}` has invalid {kind} identifier `{value}`"))?;
        return Ok(ProviderReference::Named(id));
    }
    parse_direct(input, path).map(|provider| ProviderReference::Inline(Box::new(provider)))
}

fn int_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<IntProviderReference, String> {
    parse_int_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn float_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<FloatProviderReference, String> {
    parse_float_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn optional_int_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<Option<IntProviderReference>, String> {
    input
        .field(field)
        .map(|value| parse_int_reference(value, &format!("{path}.{field}")))
        .transpose()
}

fn optional_float_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<Option<FloatProviderReference>, String> {
    input
        .field(field)
        .map(|value| parse_float_reference(value, &format!("{path}.{field}")))
        .transpose()
}

fn fixed_score_holder(input: Input<'_>, path: &str) -> Result<JavaString, String> {
    let target_path = format!("{path}.target");
    let target = required_field(input, path, "target")?;
    if target.string().is_some() {
        return Err(format!(
            "`{target_path}` uses an entity-context score target outside Worldless scope"
        ));
    }
    if !target.is_object() {
        return Err(format!("`{target_path}` must be an object"));
    }
    let target_type = identifier_field(target, &target_path, "type")?;
    if target_type.to_string() != "minecraft:fixed" {
        return Err(format!(
            "score target type `{target_type}` depends on a physical-world entity context"
        ));
    }
    string_field(target, &target_path, "name")
}

pub(crate) fn required_field<'a>(
    input: Input<'a>,
    path: &str,
    field: &str,
) -> Result<Input<'a>, String> {
    input
        .field(field)
        .ok_or_else(|| format!("`{path}` is missing field `{field}`"))
}

fn string_field(input: Input<'_>, path: &str, field: &str) -> Result<JavaString, String> {
    let field_path = format!("{path}.{field}");
    required_field(input, path, field)?
        .string()
        .ok_or_else(|| format!("`{field_path}` must be a string"))
}

pub(crate) fn identifier_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<Identifier, String> {
    let field_path = format!("{path}.{field}");
    let value = string_field(input, path, field)?;
    let value = ascii_string(&value, &field_path)?;
    Identifier::parse(&value)
        .ok_or_else(|| format!("`{field_path}` has invalid identifier `{value}`"))
}

fn float_field(input: Input<'_>, path: &str, field: &str) -> Result<f32, String> {
    let field_path = format!("{path}.{field}");
    required_field(input, path, field)?
        .number()
        .ok_or_else(|| format!("`{field_path}` must be a number"))
}

fn int_field(input: Input<'_>, path: &str, field: &str) -> Result<i32, String> {
    let field_path = format!("{path}.{field}");
    int_value(required_field(input, path, field)?, &field_path)
}

pub(crate) fn int_value(input: Input<'_>, path: &str) -> Result<i32, String> {
    match input {
        Input::Json(Value::Number(value)) => crate::loader::java_number_to_i32(value)
            .map_err(|reason| format!("invalid `{path}`: {reason}")),
        Input::Nbt(value) => {
            tag_boxed_int_value(value).ok_or_else(|| format!("`{path}` must be a number"))
        }
        Input::NbtByte(value) => Ok(i32::from(value)),
        Input::NbtInt(value) => Ok(value),
        Input::NbtLong(value) => Ok(value as i32),
        Input::Json(_) => Err(format!("`{path}` must be a number")),
    }
}

fn nbt_path_field(input: Input<'_>, path: &str, field: &str) -> Result<NbtPath, String> {
    let field_path = format!("{path}.{field}");
    let value = string_field(input, path, field)?;
    let mut reader = worldless_brigadier::StringReader::from_utf16(value.units().to_vec());
    NbtPath::parse_codec(&mut reader)
        .map_err(|reason| format!("invalid NBT path in `{field_path}`: {reason}"))
}

pub(crate) fn ascii_string(value: &JavaString, path: &str) -> Result<String, String> {
    if !value.units().iter().all(|unit| *unit <= 0x7f) {
        return Err(format!("`{path}` must contain an ASCII identifier"));
    }
    Ok(value
        .units()
        .iter()
        .map(|unit| *unit as u8 as char)
        .collect())
}

#[derive(Clone, Copy)]
pub(crate) enum Input<'a> {
    Json(&'a Value),
    Nbt(&'a Tag),
    NbtByte(i8),
    NbtInt(i32),
    NbtLong(i64),
}

impl<'a> Input<'a> {
    pub(crate) fn boolean(self) -> Option<bool> {
        match self {
            Self::Json(Value::Bool(value)) => Some(*value),
            Self::Nbt(value) => value.double_value().map(|value| value != 0.0),
            Self::NbtByte(value) => Some(value != 0),
            Self::NbtInt(value) => Some(value != 0),
            Self::NbtLong(value) => Some(value != 0),
            Self::Json(_) => None,
        }
    }

    pub(crate) fn number(self) -> Option<f32> {
        match self {
            Self::Json(Value::Number(value)) => value.to_string().parse().ok(),
            Self::Nbt(value) => tag_float_value(value),
            Self::NbtByte(value) => Some(f32::from(value)),
            Self::NbtInt(value) => Some(value as f32),
            Self::NbtLong(value) => Some(value as f32),
            Self::Json(_) => None,
        }
    }

    pub(crate) fn string(self) -> Option<JavaString> {
        match self {
            Self::Json(Value::String(value)) => Some(resource_json::decode_string(value)),
            Self::Nbt(Tag::String(value)) => Some(value.clone()),
            Self::Json(_)
            | Self::Nbt(_)
            | Self::NbtByte(_)
            | Self::NbtInt(_)
            | Self::NbtLong(_) => None,
        }
    }

    pub(crate) fn is_object(self) -> bool {
        matches!(
            self,
            Self::Json(Value::Object(_)) | Self::Nbt(Tag::Compound(_))
        )
    }

    pub(crate) fn is_empty_object(self) -> bool {
        match self {
            Self::Json(Value::Object(value)) => value.is_empty(),
            Self::Nbt(Tag::Compound(value)) => value.is_empty(),
            Self::Json(_)
            | Self::Nbt(_)
            | Self::NbtByte(_)
            | Self::NbtInt(_)
            | Self::NbtLong(_) => false,
        }
    }

    pub(crate) fn object_entries(self) -> Option<Vec<(JavaString, Self)>> {
        match self {
            Self::Json(Value::Object(value)) => Some(
                value
                    .iter()
                    .map(|(key, value)| (resource_json::decode_string(key), Self::Json(value)))
                    .collect(),
            ),
            Self::Nbt(Tag::Compound(value)) => Some(
                value
                    .entries()
                    .map(|(key, value)| (key.clone(), Self::Nbt(value)))
                    .collect(),
            ),
            Self::Json(_)
            | Self::Nbt(_)
            | Self::NbtByte(_)
            | Self::NbtInt(_)
            | Self::NbtLong(_) => None,
        }
    }

    pub(crate) fn field(self, name: &str) -> Option<Self> {
        match self {
            Self::Json(Value::Object(value)) => resource_json::field(value, name).map(Self::Json),
            Self::Nbt(Tag::Compound(value)) => value.get(&JavaString::from(name)).map(Self::Nbt),
            Self::Json(_)
            | Self::Nbt(_)
            | Self::NbtByte(_)
            | Self::NbtInt(_)
            | Self::NbtLong(_) => None,
        }
    }

    pub(crate) fn list(self) -> Option<Vec<Self>> {
        match self {
            Self::Json(Value::Array(values)) => {
                Some(values.iter().map(Self::Json).collect::<Vec<_>>())
            }
            Self::Nbt(Tag::List(values)) => Some(values.iter().map(Self::Nbt).collect::<Vec<_>>()),
            Self::Nbt(Tag::ByteArray(values)) => {
                Some(values.iter().copied().map(Self::NbtByte).collect())
            }
            Self::Nbt(Tag::IntArray(values)) => {
                Some(values.iter().copied().map(Self::NbtInt).collect())
            }
            Self::Nbt(Tag::LongArray(values)) => {
                Some(values.iter().copied().map(Self::NbtLong).collect())
            }
            Self::Json(_)
            | Self::Nbt(_)
            | Self::NbtByte(_)
            | Self::NbtInt(_)
            | Self::NbtLong(_) => None,
        }
    }
}

fn builtin_int_providers() -> HashMap<Identifier, IntProvider> {
    fn id(path: &str) -> Identifier {
        Identifier::from_parts("minecraft", path)
            .expect("built-in context int provider identifiers are valid")
    }

    fn named(path: &str) -> IntProviderReference {
        ProviderReference::Named(id(path))
    }

    fn direct(value: i32) -> IntProviderReference {
        ProviderReference::Inline(Box::new(IntProvider::Constant(value)))
    }

    fn predicate(path: &str) -> PredicateReference {
        PredicateReference::Named(id(path))
    }

    fn compostable(chance: i32) -> IntProvider {
        IntProvider::WeightedList {
            distribution: vec![
                WeightedProvider {
                    provider: direct(1),
                    weight: chance,
                },
                WeightedProvider {
                    provider: direct(0),
                    weight: 100 - chance,
                },
            ],
            total_weight: 100,
        }
    }

    fn cooking(time: i32) -> IntProvider {
        IntProvider::Binary {
            operation: IntBinaryOperation::Quotient,
            left: direct(time),
            right: ProviderReference::Inline(Box::new(IntProvider::Conditional {
                condition: predicate("block/fast_cooking"),
                on_true: named("cooking/fast_burn_time_reduction_factor"),
                on_false: named("cooking/normal_burn_time_reduction_factor"),
            })),
        }
    }

    let mut providers = HashMap::from([
        (id("compostable/low"), compostable(30)),
        (id("compostable/low_medium"), compostable(50)),
        (id("compostable/medium"), compostable(65)),
        (id("compostable/medium_high"), compostable(85)),
        (id("compostable/always_add_one"), IntProvider::Constant(1)),
        (
            id("cooking/normal_burn_time_reduction_factor"),
            IntProvider::Constant(1),
        ),
        (
            id("cooking/fast_burn_time_reduction_factor"),
            IntProvider::Constant(2),
        ),
        (id("brewing/uses_default"), IntProvider::Constant(20)),
    ]);
    for (path, time) in [
        ("cooking/time_bamboo", 50),
        ("cooking/time_wool_slabs", 50),
        ("cooking/time_wool_carpets", 67),
        ("cooking/time_dry_plants", 100),
        ("cooking/time_wood_items_extra_small", 100),
        ("cooking/time_wool", 100),
        ("cooking/time_wood_slabs", 150),
        ("cooking/time_wood_items_large", 200),
        ("cooking/time_roots", 300),
        ("cooking/time_wood_blocks", 300),
        ("cooking/time_wood_items_small", 300),
        ("cooking/time_hanging_signs", 800),
        ("cooking/time_boats", 1200),
        ("cooking/time_coal", 1600),
        ("cooking/time_blaze_rod", 2400),
        ("cooking/time_dried_kelp_block", 4001),
        ("cooking/time_coal_block", 16000),
        ("cooking/time_lava_bucket", 20000),
    ] {
        providers.insert(id(path), cooking(time));
    }
    providers
}

fn builtin_float_providers() -> HashMap<Identifier, FloatProvider> {
    fn id(path: &str) -> Identifier {
        Identifier::from_parts("minecraft", path)
            .expect("built-in context float provider identifiers are valid")
    }

    fn named(path: &str) -> FloatProviderReference {
        ProviderReference::Named(id(path))
    }

    fn predicate(path: &str) -> PredicateReference {
        PredicateReference::Named(id(path))
    }

    HashMap::from([
        (
            id("cooking/normal_speed_multiplier"),
            FloatProvider::Constant(1.0),
        ),
        (
            id("cooking/fast_speed_multiplier"),
            FloatProvider::Constant(2.0),
        ),
        (
            id("cooking/speed_default"),
            FloatProvider::Conditional {
                condition: predicate("block/fast_cooking"),
                on_true: named("cooking/fast_speed_multiplier"),
                on_false: named("cooking/normal_speed_multiplier"),
            },
        ),
        (id("brewing/speed_default"), FloatProvider::Constant(1.0)),
    ])
}

fn storage_number<'a>(
    command_storage: &'a CommandStorage,
    storage: &Identifier,
    path: &NbtPath,
) -> Option<NbtSelection<'a>> {
    let root = command_storage.get_ref(storage)?;
    path.select_single(root)
}

fn tag_float_value(value: &Tag) -> Option<f32> {
    match value {
        Tag::Byte(value) => Some(f32::from(*value)),
        Tag::Short(value) => Some(f32::from(*value)),
        Tag::Int(value) => Some(*value as f32),
        Tag::Long(value) => Some(*value as f32),
        Tag::Float(bits) => Some(f32::from_bits(*bits)),
        Tag::Double(bits) => Some(f64::from_bits(*bits) as f32),
        Tag::ByteArray(_)
        | Tag::String(_)
        | Tag::List(_)
        | Tag::Compound(_)
        | Tag::IntArray(_)
        | Tag::LongArray(_) => None,
    }
}

fn tag_boxed_int_value(value: &Tag) -> Option<i32> {
    match value {
        Tag::Byte(value) => Some(i32::from(*value)),
        Tag::Short(value) => Some(i32::from(*value)),
        Tag::Int(value) => Some(*value),
        Tag::Long(value) => Some(*value as i32),
        Tag::Float(bits) => Some(f32::from_bits(*bits) as i32),
        Tag::Double(bits) => Some(f64::from_bits(*bits) as i32),
        Tag::ByteArray(_)
        | Tag::String(_)
        | Tag::List(_)
        | Tag::Compound(_)
        | Tag::IntArray(_)
        | Tag::LongArray(_) => None,
    }
}

fn java_min(left: f32, right: f32) -> f32 {
    if left.is_nan() {
        left
    } else if right.is_nan() {
        right
    } else if left == 0.0 && right == 0.0 {
        if left.is_sign_negative() || right.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else if left <= right {
        left
    } else {
        right
    }
}

fn java_max(left: f32, right: f32) -> f32 {
    if left.is_nan() {
        left
    } else if right.is_nan() {
        right
    } else if left == 0.0 && right == 0.0 {
        if left.is_sign_positive() || right.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else if left >= right {
        left
    } else {
        right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_context::{Position, Rotation};

    fn context() -> ExecutionContext {
        ExecutionContext::new(Position::new(0.0, 0.0, 0.0), Rotation::new(0.0, 0.0))
    }

    #[test]
    fn parses_typed_direct_and_compact_holder_sets() {
        let int_provider =
            parse_int_json(r#"{"type":"add","inputs":[1,{"type":"mul","inputs":2}]}"#).unwrap();
        let float_provider =
            parse_float_json(r#"{"type":"add","inputs":[1,{"type":"mul","inputs":2}]}"#).unwrap();
        assert!(matches!(
            int_provider,
            IntProvider::Aggregate {
                operation: IntAggregateOperation::Sum,
                ..
            }
        ));
        assert!(matches!(
            float_provider,
            FloatProvider::Aggregate {
                operation: FloatAggregateOperation::Sum,
                ..
            }
        ));
    }

    #[test]
    fn json_nesting_matches_gson_limit() {
        fn nested_sum(depth: usize) -> String {
            let mut provider = "1".to_owned();
            for _ in 0..depth {
                provider = format!(r#"{{"type":"add","inputs":[{provider}]}}"#);
            }
            provider
        }

        assert!(parse_int_json(&nested_sum(65)).is_ok());
        assert!(
            parse_int_json(&nested_sum(128))
                .unwrap_err()
                .contains("nesting limit 255")
        );
    }

    #[test]
    fn weighted_list_rejects_gson_unsupported_number_scale() {
        let error = parse_int_json(
            r#"{"type":"weighted_list","distribution":[{"data":1,"weight":1e10000},{"data":2,"weight":1}]}"#,
        )
        .unwrap_err();
        assert!(error.contains("number scale"));
    }

    #[test]
    fn integer_arithmetic_uses_exact_results() {
        assert!(matches!(
            evaluate_int_binary(IntBinaryOperation::Difference, i32::MIN, 1),
            Err(EvaluationError::Arithmetic(_))
        ));
        assert_eq!(
            evaluate_int_binary(IntBinaryOperation::Quotient, i32::MIN, -1).unwrap(),
            i32::MIN
        );
        assert_eq!(
            evaluate_int_binary(IntBinaryOperation::FloorModulus, i32::MIN, -1).unwrap(),
            0
        );
        assert!(int_pow_exact(0, 0).is_err());
        assert!(int_pow_exact(2, 31).is_err());
    }

    #[test]
    fn invalid_uniform_bound_is_not_caught_as_arithmetic() {
        let provider = ProviderReference::Inline(Box::new(IntProvider::Uniform {
            min: ProviderReference::Inline(Box::new(IntProvider::Constant(i32::MIN))),
            max: ProviderReference::Inline(Box::new(IntProvider::Constant(i32::MAX))),
        }));
        let error = LootRegistry::empty()
            .get_int(
                &provider,
                &Scoreboard::default(),
                &CommandStorage::default(),
                &context(),
                &mut LegacyRandom::default(),
            )
            .unwrap_err();
        assert!(error.contains("bound must be positive"));
    }

    #[test]
    fn float_modulus_evaluates_zero_right_operand_first() {
        let provider = ProviderReference::Inline(Box::new(FloatProvider::Binary {
            operation: FloatBinaryOperation::Modulus,
            left: ProviderReference::Inline(Box::new(FloatProvider::Uniform {
                min: ProviderReference::Inline(Box::new(FloatProvider::Constant(0.0))),
                max: ProviderReference::Inline(Box::new(FloatProvider::Constant(1.0))),
            })),
            right: ProviderReference::Inline(Box::new(FloatProvider::Constant(0.0))),
        }));
        let mut actual_random = LegacyRandom::default();
        let mut expected_random = LegacyRandom::default();

        assert!(
            LootRegistry::empty()
                .get_float_unsafe(
                    &provider,
                    &Scoreboard::default(),
                    &CommandStorage::default(),
                    &context(),
                    &mut actual_random,
                )
                .unwrap()
                .is_nan()
        );
        assert_eq!(actual_random.next_float(), expected_random.next_float());
    }

    #[test]
    fn float_power_uses_java_special_cases() {
        assert!(java_float_pow(1.0, f32::NAN).is_nan());
        assert!(java_float_pow(1.0, f32::INFINITY).is_nan());
        assert!(java_float_pow(-1.0, f32::NEG_INFINITY).is_nan());
        assert_eq!(java_float_pow(f32::NAN, -0.0).to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn score_objective_preserves_java_utf16() {
        let holder = JavaString::from("#holder");
        let mut scoreboard = Scoreboard::default();
        scoreboard.add_objective("\u{fffd}").unwrap();
        assert_eq!(
            scoreboard.set_scores(
                &crate::program::ScoreHolderSet::Named(holder.clone()),
                "\u{fffd}",
                7,
            ),
            Some(7)
        );
        let provider = ProviderReference::Inline(Box::new(IntProvider::Score {
            holder,
            objective: JavaString::from_units(vec![0xd800]),
            fallback: ProviderReference::Inline(Box::new(IntProvider::Constant(1))),
        }));

        assert_eq!(
            LootRegistry::empty()
                .get_int_unsafe(
                    &provider,
                    &scoreboard,
                    &CommandStorage::default(),
                    &context(),
                    &mut LegacyRandom::default(),
                )
                .unwrap(),
            1
        );
    }
}
