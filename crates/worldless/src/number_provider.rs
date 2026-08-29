use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::{
    execution_context::ExecutionContext,
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
pub(crate) enum NumberProviderReference {
    Named(Identifier),
    Inline(Box<NumberProvider>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NumberProvider {
    Constant(f32),
    Uniform {
        min: NumberProviderReference,
        max: NumberProviderReference,
    },
    Binomial {
        n: NumberProviderReference,
        p: NumberProviderReference,
    },
    Storage {
        storage: Identifier,
        path: NbtPath,
    },
    Score {
        holder: JavaString,
        objective: String,
        scale: f32,
    },
    Sum(NumberProviderSet),
    Product(NumberProviderSet),
    Minimum(NumberProviderSet),
    Maximum(NumberProviderSet),
    Average(NumberProviderSet),
    NumberDispatcher {
        cases: Vec<NumberDispatcherCase>,
        default: NumberProviderReference,
    },
    Conditional {
        condition: PredicateReference,
        on_true: NumberProviderReference,
        on_false: NumberProviderReference,
    },
    WeightedList {
        distribution: Vec<WeightedProvider>,
        total_weight: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NumberProviderSet {
    Direct(Vec<NumberProviderReference>),
    Tag(Identifier),
}

enum NumberProviderValues<'a> {
    Direct {
        registry: &'a LootRegistry,
        values: std::slice::Iter<'a, NumberProviderReference>,
    },
    Tag {
        registry: &'a LootRegistry,
        values: std::slice::Iter<'a, Identifier>,
    },
}

impl<'a> Iterator for NumberProviderValues<'a> {
    type Item = &'a NumberProvider;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Direct { registry, values } => values
                .next()
                .map(|value| registry.resolve_number_provider(value)),
            Self::Tag { registry, values } => values.next().map(|id| {
                registry
                    .providers
                    .get(id)
                    .expect("number provider tags contain validated providers")
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WeightedProvider {
    pub(crate) provider: NumberProviderReference,
    pub(crate) weight: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NumberDispatcherCase {
    pub(crate) condition: PredicateReference,
    pub(crate) number_provider: NumberProviderReference,
}

#[derive(Debug)]
pub(crate) struct LootRegistry {
    providers: HashMap<Identifier, NumberProvider>,
    provider_tags: HashMap<Identifier, Vec<Identifier>>,
    predicates: HashMap<Identifier, LootPredicate>,
    predicate_tags: HashMap<Identifier, Vec<Identifier>>,
}

impl LootRegistry {
    pub(crate) fn new(
        providers: HashMap<Identifier, NumberProvider>,
        provider_tags: HashMap<Identifier, Vec<Identifier>>,
        predicates: HashMap<Identifier, LootPredicate>,
        predicate_tags: HashMap<Identifier, Vec<Identifier>>,
    ) -> Result<Self, RegistryValidationError> {
        let user_resources = providers
            .keys()
            .cloned()
            .map(RegistryResource::NumberProvider)
            .chain(predicates.keys().cloned().map(RegistryResource::Predicate))
            .collect::<HashSet<_>>();
        let mut all_providers = builtin_providers();
        all_providers.extend(providers);
        let mut all_predicates = builtin_predicates();
        all_predicates.extend(predicates);
        let registry = Self {
            providers: all_providers,
            provider_tags,
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
        )
        .expect("the supported built-in loot resources are valid")
    }

    pub(crate) fn contains_number_provider(&self, id: &Identifier) -> bool {
        self.providers.contains_key(id)
    }

    pub(crate) fn number_provider_ids(&self) -> HashSet<Identifier> {
        self.providers.keys().cloned().collect()
    }

    pub(crate) fn contains_predicate(&self, id: &Identifier) -> bool {
        self.predicates.contains_key(id)
    }

    pub(crate) fn predicate_ids(&self) -> HashSet<Identifier> {
        self.predicates.keys().cloned().collect()
    }

    pub(crate) fn validate_inline_number_provider(
        &self,
        provider: &NumberProvider,
    ) -> Result<(), String> {
        let mut dependencies = Vec::new();
        self.collect_number_provider_dependencies(provider, &mut dependencies)
    }

    pub(crate) fn validate_inline_predicate(
        &self,
        predicate: &LootPredicate,
    ) -> Result<(), String> {
        let mut dependencies = Vec::new();
        self.collect_predicate_dependencies(predicate, &mut dependencies)
    }

    pub(crate) fn get_float(
        &self,
        provider: &NumberProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<f32, String> {
        self.resolve_number_provider(provider).get_float(
            self,
            scoreboard,
            command_storage,
            execution_context,
            random,
        )
    }

    pub(crate) fn get_int(
        &self,
        provider: &NumberProviderReference,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<i32, String> {
        self.resolve_number_provider(provider).get_int(
            self,
            scoreboard,
            command_storage,
            execution_context,
            random,
        )
    }

    pub(crate) fn resolve_number_provider<'a>(
        &'a self,
        provider: &'a NumberProviderReference,
    ) -> &'a NumberProvider {
        match provider {
            NumberProviderReference::Named(id) => self
                .providers
                .get(id)
                .expect("number provider references are validated before execution"),
            NumberProviderReference::Inline(provider) => provider,
        }
    }

    fn number_provider_values<'a>(
        &'a self,
        providers: &'a NumberProviderSet,
    ) -> NumberProviderValues<'a> {
        match providers {
            NumberProviderSet::Direct(values) => NumberProviderValues::Direct {
                registry: self,
                values: values.iter(),
            },
            NumberProviderSet::Tag(tag) => NumberProviderValues::Tag {
                registry: self,
                values: self
                    .provider_tags
                    .get(tag)
                    .expect("number provider tags are validated before execution")
                    .iter(),
            },
        }
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
            .providers
            .keys()
            .cloned()
            .map(RegistryResource::NumberProvider)
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
                RegistryResource::NumberProvider(id) => self.collect_number_provider_dependencies(
                    self.providers
                        .get(id)
                        .expect("the identifier came from the provider map"),
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

    fn collect_number_provider_dependencies(
        &self,
        provider: &NumberProvider,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        let providers = match provider {
            NumberProvider::Sum(providers)
            | NumberProvider::Product(providers)
            | NumberProvider::Minimum(providers)
            | NumberProvider::Maximum(providers)
            | NumberProvider::Average(providers) => providers,
            NumberProvider::Constant(_)
            | NumberProvider::Storage { .. }
            | NumberProvider::Score { .. } => return Ok(()),
            NumberProvider::Uniform { min, max } => {
                self.collect_number_provider_reference_dependencies(min, dependencies)?;
                return self.collect_number_provider_reference_dependencies(max, dependencies);
            }
            NumberProvider::Binomial { n, p } => {
                self.collect_number_provider_reference_dependencies(n, dependencies)?;
                return self.collect_number_provider_reference_dependencies(p, dependencies);
            }
            NumberProvider::WeightedList { distribution, .. } => {
                for entry in distribution {
                    self.collect_number_provider_reference_dependencies(
                        &entry.provider,
                        dependencies,
                    )?;
                }
                return Ok(());
            }
            NumberProvider::NumberDispatcher { cases, default } => {
                for case in cases {
                    self.collect_predicate_reference_dependencies(&case.condition, dependencies)?;
                    self.collect_number_provider_reference_dependencies(
                        &case.number_provider,
                        dependencies,
                    )?;
                }
                return self.collect_number_provider_reference_dependencies(default, dependencies);
            }
            NumberProvider::Conditional {
                condition,
                on_true,
                on_false,
            } => {
                self.collect_predicate_reference_dependencies(condition, dependencies)?;
                self.collect_number_provider_reference_dependencies(on_true, dependencies)?;
                return self.collect_number_provider_reference_dependencies(on_false, dependencies);
            }
        };

        match providers {
            NumberProviderSet::Direct(values) => {
                for value in values {
                    self.collect_number_provider_reference_dependencies(value, dependencies)?;
                }
            }
            NumberProviderSet::Tag(tag) => {
                let values = self
                    .provider_tags
                    .get(tag)
                    .ok_or_else(|| format!("number provider tag `#{tag}` does not exist"))?;
                dependencies.extend(values.iter().cloned().map(RegistryResource::NumberProvider));
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
                self.collect_number_provider_reference_dependencies(chance, dependencies)
            }
            LootPredicate::ValueCheck { value, range } => {
                self.collect_number_provider_reference_dependencies(value, dependencies)?;
                if let Some(min) = &range.min {
                    self.collect_number_provider_reference_dependencies(min, dependencies)?;
                }
                if let Some(max) = &range.max {
                    self.collect_number_provider_reference_dependencies(max, dependencies)?;
                }
                Ok(())
            }
            LootPredicate::AbsentContext {
                referenced_number_providers,
                ..
            } => {
                for provider in referenced_number_providers {
                    self.collect_number_provider_reference_dependencies(provider, dependencies)?;
                }
                Ok(())
            }
            LootPredicate::LocationCheck { .. } | LootPredicate::MissingContextParameter { .. } => {
                Ok(())
            }
        }
    }

    fn collect_number_provider_reference_dependencies(
        &self,
        provider: &NumberProviderReference,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match provider {
            NumberProviderReference::Named(id) => {
                if !self.providers.contains_key(id) {
                    return Err(format!("number provider `{id}` does not exist"));
                }
                dependencies.push(RegistryResource::NumberProvider(id.clone()));
                Ok(())
            }
            NumberProviderReference::Inline(provider) => {
                self.collect_number_provider_dependencies(provider, dependencies)
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

impl NumberProvider {
    fn get_float(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<f32, String> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Uniform { min, max } => {
                let min = registry.resolve_number_provider(min).get_float(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let max = registry.resolve_number_provider(max).get_float(
                    registry,
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
            Self::Binomial { .. } => self
                .get_int(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
                .map(|value| value as f32),
            Self::Storage { storage, path } => Ok(storage_number(command_storage, storage, path)
                .as_ref()
                .and_then(NbtSelection::as_tag)
                .and_then(tag_float_value)
                .unwrap_or(0.0)),
            Self::Score {
                holder,
                objective,
                scale,
            } => Ok(scoreboard
                .score(holder, objective)
                .map_or(0.0, |value| value as f32 * *scale)),
            Self::Sum(providers) => {
                let mut value = 0.0;
                for provider in registry.number_provider_values(providers) {
                    value += provider.get_float(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                }
                Ok(value)
            }
            Self::Product(providers) => {
                let mut value = 1.0;
                for provider in registry.number_provider_values(providers) {
                    value *= provider.get_float(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                }
                Ok(value)
            }
            Self::Minimum(providers) => {
                let mut value = f32::MAX;
                for provider in registry.number_provider_values(providers) {
                    value = java_min(
                        value,
                        provider.get_float(
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
            Self::Maximum(providers) => {
                let mut value = -f32::MAX;
                for provider in registry.number_provider_values(providers) {
                    value = java_max(
                        value,
                        provider.get_float(
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
            Self::Average(providers) => {
                let mut sum = 0.0_f32;
                let mut count = 0_u32;
                for provider in registry.number_provider_values(providers) {
                    sum += provider.get_float(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?;
                    count += 1;
                }
                Ok(if count == 0 { 0.0 } else { sum / count as f32 })
            }
            Self::NumberDispatcher { cases, default } => {
                let mut selected = default;
                for case in cases {
                    if registry.test_predicate(
                        &case.condition,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )? {
                        selected = &case.number_provider;
                        break;
                    }
                }
                registry.resolve_number_provider(selected).get_float(
                    registry,
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
                let selected = if registry.test_predicate(
                    condition,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )? {
                    on_true
                } else {
                    on_false
                };
                registry.resolve_number_provider(selected).get_float(
                    registry,
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
                let selected = random.next_int(*total_weight)?;
                let provider = select_weighted(distribution, selected);
                registry.resolve_number_provider(provider).get_float(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
        }
    }

    fn get_int(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<i32, String> {
        match self {
            Self::Uniform { min, max } => {
                let min = registry.resolve_number_provider(min).get_int(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let max = registry.resolve_number_provider(max).get_int(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                if min >= max {
                    Ok(min)
                } else {
                    let bound = max.wrapping_sub(min).wrapping_add(1);
                    random.next_int(bound).map(|value| value.wrapping_add(min))
                }
            }
            Self::Binomial { n, p } => {
                let n = registry.resolve_number_provider(n).get_int(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let p = registry.resolve_number_provider(p).get_float(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                let mut result = 0;
                for _ in 0..n.max(0) {
                    if random.next_float() < p {
                        result += 1;
                    }
                }
                Ok(result)
            }
            Self::Storage { storage, path } => Ok(storage_number(command_storage, storage, path)
                .as_ref()
                .and_then(NbtSelection::as_tag)
                .and_then(tag_boxed_int_value)
                .unwrap_or(0)),
            Self::Sum(providers) => {
                let mut value = 0_i64;
                for provider in registry.number_provider_values(providers) {
                    value = value.wrapping_add(i64::from(provider.get_int(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?));
                }
                Ok(saturated_i64_to_i32(value))
            }
            Self::Product(providers) => {
                let mut value = 1_i64;
                for provider in registry.number_provider_values(providers) {
                    value = value.wrapping_mul(i64::from(provider.get_int(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?));
                }
                Ok(saturated_i64_to_i32(value))
            }
            Self::Minimum(providers) => {
                let mut value = i32::MAX;
                for provider in registry.number_provider_values(providers) {
                    value = value.min(provider.get_int(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?);
                }
                Ok(value)
            }
            Self::Maximum(providers) => {
                let mut value = -i32::MAX;
                for provider in registry.number_provider_values(providers) {
                    value = value.max(provider.get_int(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?);
                }
                Ok(value)
            }
            Self::Average(providers) => {
                let mut sum = 0_i64;
                let mut count = 0_i64;
                for provider in registry.number_provider_values(providers) {
                    sum = sum.wrapping_add(i64::from(provider.get_int(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?));
                    count += 1;
                }
                if count == 0 {
                    Ok(0)
                } else {
                    Ok(saturated_i64_to_i32(sum / count))
                }
            }
            Self::NumberDispatcher { cases, default } => {
                let mut selected = default;
                for case in cases {
                    if registry.test_predicate(
                        &case.condition,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )? {
                        selected = &case.number_provider;
                        break;
                    }
                }
                registry.resolve_number_provider(selected).get_int(
                    registry,
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
                let selected = if registry.test_predicate(
                    condition,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )? {
                    on_true
                } else {
                    on_false
                };
                registry.resolve_number_provider(selected).get_int(
                    registry,
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
                let selected = random.next_int(*total_weight)?;
                let provider = select_weighted(distribution, selected);
                registry.resolve_number_provider(provider).get_int(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
            }
            Self::Constant(_) | Self::Score { .. } => self
                .get_float(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
                .map(java_round),
        }
    }
}

fn select_weighted(
    distribution: &[WeightedProvider],
    mut selected: i32,
) -> &NumberProviderReference {
    for entry in distribution {
        selected -= entry.weight;
        if selected < 0 {
            return &entry.provider;
        }
    }
    unreachable!("weighted provider selection is below the validated total weight")
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RegistryResource {
    NumberProvider(Identifier),
    Predicate(Identifier),
}

impl RegistryResource {
    fn sort_key(&self) -> (u8, String) {
        match self {
            Self::NumberProvider(id) => (0, id.to_string()),
            Self::Predicate(id) => (1, id.to_string()),
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::NumberProvider(_) => "number provider",
            Self::Predicate(_) => "predicate",
        }
    }

    pub(crate) fn id(&self) -> &Identifier {
        match self {
            Self::NumberProvider(id) | Self::Predicate(id) => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryValidationError {
    pub(crate) resource: RegistryResource,
    pub(crate) reason: String,
}

pub(crate) fn parse_json(contents: &str) -> Result<NumberProvider, String> {
    let value = resource_json::parse(contents)?;
    parse_direct(Input::Json(&value), "root")
}

pub(crate) fn parse_inline_tag(
    value: &Tag,
    registry: &LootRegistry,
) -> Result<NumberProvider, String> {
    let provider = parse_direct(Input::Nbt(value), "provider")?;
    registry.validate_inline_number_provider(&provider)?;
    Ok(provider)
}

fn parse_direct(input: Input<'_>, path: &str) -> Result<NumberProvider, String> {
    if let Some(value) = input.number() {
        return Ok(NumberProvider::Constant(value));
    }
    if !input.is_object() {
        return Err(format!("`{path}` must be a number or an object"));
    }
    let provider_type = identifier_field(input, path, "type")?;
    if provider_type.namespace() != "minecraft" {
        return Err(format!(
            "number provider type `{provider_type}` is not supported"
        ));
    }
    match provider_type.path() {
        "constant" => Ok(NumberProvider::Constant(float_field(input, path, "value")?)),
        "uniform" => Ok(NumberProvider::Uniform {
            min: reference_field(input, path, "min")?,
            max: reference_field(input, path, "max")?,
        }),
        "binomial" => Ok(NumberProvider::Binomial {
            n: reference_field(input, path, "n")?,
            p: reference_field(input, path, "p")?,
        }),
        "storage" => Ok(NumberProvider::Storage {
            storage: identifier_field(input, path, "storage")?,
            path: nbt_path_field(input, path, "path")?,
        }),
        "score" => Ok(NumberProvider::Score {
            holder: fixed_score_holder(input, path)?,
            objective: string_field(input, path, "score")?.to_string_lossy(),
            scale: optional_float_field(input, path, "scale")?.unwrap_or(1.0),
        }),
        "sum" => Ok(NumberProvider::Sum(provider_set_field(
            input, path, "operands",
        )?)),
        "product" => Ok(NumberProvider::Product(provider_set_field(
            input, path, "operands",
        )?)),
        "minimum" => Ok(NumberProvider::Minimum(provider_set_field(
            input, path, "operands",
        )?)),
        "maximum" => Ok(NumberProvider::Maximum(provider_set_field(
            input, path, "operands",
        )?)),
        "average" => Ok(NumberProvider::Average(provider_set_field(
            input, path, "operands",
        )?)),
        "weighted_list" => parse_weighted_list(input, path),
        "number_dispatcher" => parse_number_dispatcher(input, path),
        "conditional" => parse_conditional(input, path),
        "enchantment_level" | "environment_attribute" => Err(format!(
            "number provider type `{provider_type}` depends on a physical-world loot context"
        )),
        _ => Err(format!(
            "number provider type `{provider_type}` is not supported"
        )),
    }
}

fn parse_number_dispatcher(input: Input<'_>, path: &str) -> Result<NumberProvider, String> {
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
        parsed_cases.push(NumberDispatcherCase {
            condition: parse_predicate_reference(
                required_field(case, &case_path, "condition")?,
                &format!("{case_path}.condition"),
            )?,
            number_provider: parse_reference(
                required_field(case, &case_path, "number_provider")?,
                &format!("{case_path}.number_provider"),
            )?,
        });
    }
    let default = input.field("default").map_or_else(
        || Ok(constant_zero_reference()),
        |default| parse_reference(default, &format!("{path}.default")),
    )?;
    Ok(NumberProvider::NumberDispatcher {
        cases: parsed_cases,
        default,
    })
}

fn parse_conditional(input: Input<'_>, path: &str) -> Result<NumberProvider, String> {
    let condition = parse_predicate_reference(
        required_field(input, path, "condition")?,
        &format!("{path}.condition"),
    )?;
    let on_true = parse_reference(
        required_field(input, path, "on_true")?,
        &format!("{path}.on_true"),
    )?;
    let on_false = input.field("on_false").map_or_else(
        || Ok(constant_zero_reference()),
        |value| parse_reference(value, &format!("{path}.on_false")),
    )?;
    Ok(NumberProvider::Conditional {
        condition,
        on_true,
        on_false,
    })
}

fn constant_zero_reference() -> NumberProviderReference {
    NumberProviderReference::Inline(Box::new(NumberProvider::Constant(0.0)))
}

fn parse_weighted_list(input: Input<'_>, path: &str) -> Result<NumberProvider, String> {
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
        let provider = reference_field(entry, &entry_path, "data")?;
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
    Ok(NumberProvider::WeightedList {
        distribution: entries,
        total_weight: total_weight as i32,
    })
}

fn provider_set_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<NumberProviderSet, String> {
    let field_path = format!("{path}.{field}");
    let value = required_field(input, path, field)?;
    if let Some(string) = value.string() {
        let text = ascii_string(&string, &field_path)?;
        if let Some(tag) = text.strip_prefix('#') {
            let id = Identifier::parse(tag)
                .ok_or_else(|| format!("`{field_path}` has invalid tag identifier `{text}`"))?;
            return Ok(NumberProviderSet::Tag(id));
        }
    }
    let values = value.list().unwrap_or_else(|| vec![value]);
    let providers = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| parse_reference(value, &format!("{field_path}[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NumberProviderSet::Direct(providers))
}

pub(crate) fn parse_reference(
    input: Input<'_>,
    path: &str,
) -> Result<NumberProviderReference, String> {
    if let Some(value) = input.string() {
        let value = ascii_string(&value, path)?;
        let id = Identifier::parse(&value)
            .ok_or_else(|| format!("`{path}` has invalid number provider identifier `{value}`"))?;
        return Ok(NumberProviderReference::Named(id));
    }
    parse_direct(input, path).map(|provider| NumberProviderReference::Inline(Box::new(provider)))
}

fn reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<NumberProviderReference, String> {
    parse_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
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

fn optional_float_field(input: Input<'_>, path: &str, field: &str) -> Result<Option<f32>, String> {
    let Some(value) = input.field(field) else {
        return Ok(None);
    };
    value
        .number()
        .map(Some)
        .ok_or_else(|| format!("`{path}.{field}` must be a number"))
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

fn builtin_providers() -> HashMap<Identifier, NumberProvider> {
    fn id(path: &str) -> Identifier {
        Identifier::from_parts("minecraft", path)
            .expect("built-in number provider identifiers are valid")
    }

    fn named(path: &str) -> NumberProviderReference {
        NumberProviderReference::Named(id(path))
    }

    fn predicate(path: &str) -> PredicateReference {
        PredicateReference::Named(id(path))
    }

    fn product(time: f32, multiplier: &str) -> NumberProviderReference {
        NumberProviderReference::Inline(Box::new(NumberProvider::Product(
            NumberProviderSet::Direct(vec![
                NumberProviderReference::Inline(Box::new(NumberProvider::Constant(time))),
                named(multiplier),
            ]),
        )))
    }

    fn compostable(chance: i32) -> NumberProvider {
        NumberProvider::WeightedList {
            distribution: vec![
                WeightedProvider {
                    provider: NumberProviderReference::Inline(Box::new(NumberProvider::Constant(
                        1.0,
                    ))),
                    weight: chance,
                },
                WeightedProvider {
                    provider: NumberProviderReference::Inline(Box::new(NumberProvider::Constant(
                        0.0,
                    ))),
                    weight: 100 - chance,
                },
            ],
            total_weight: 100,
        }
    }

    let mut providers = HashMap::from([
        (id("compostable/low"), compostable(30)),
        (id("compostable/low_medium"), compostable(50)),
        (id("compostable/medium"), compostable(65)),
        (id("compostable/medium_high"), compostable(85)),
        (
            id("compostable/always_add_one"),
            NumberProvider::Constant(1.0),
        ),
        (
            id("cooking/normal_speed_multiplier"),
            NumberProvider::Constant(1.0),
        ),
        (
            id("cooking/fast_speed_multiplier"),
            NumberProvider::Constant(2.0),
        ),
        (
            id("cooking/normal_burn_time_multiplier"),
            NumberProvider::Constant(1.0),
        ),
        (
            id("cooking/fast_burn_time_multiplier"),
            NumberProvider::Constant(0.5),
        ),
        (
            id("cooking/speed_default"),
            NumberProvider::Conditional {
                condition: predicate("block/fast_cooking"),
                on_true: named("cooking/fast_speed_multiplier"),
                on_false: named("cooking/normal_speed_multiplier"),
            },
        ),
        (id("brewing/speed_default"), NumberProvider::Constant(1.0)),
        (id("brewing/uses_default"), NumberProvider::Constant(20.0)),
    ]);
    for (path, time) in [
        ("cooking/time_bamboo", 50.0),
        ("cooking/time_wool_slabs", 50.0),
        ("cooking/time_wool_carpets", 67.0),
        ("cooking/time_dry_plants", 100.0),
        ("cooking/time_wood_items_extra_small", 100.0),
        ("cooking/time_wool", 100.0),
        ("cooking/time_wood_slabs", 150.0),
        ("cooking/time_wood_items_large", 200.0),
        ("cooking/time_roots", 300.0),
        ("cooking/time_wood_blocks", 300.0),
        ("cooking/time_wood_items_small", 300.0),
        ("cooking/time_hanging_signs", 800.0),
        ("cooking/time_boats", 1200.0),
        ("cooking/time_coal", 1600.0),
        ("cooking/time_blaze_rod", 2400.0),
        ("cooking/time_dried_kelp_block", 4001.0),
        ("cooking/time_coal_block", 16000.0),
        ("cooking/time_lava_bucket", 20000.0),
    ] {
        providers.insert(
            id(path),
            NumberProvider::Conditional {
                condition: predicate("block/fast_cooking"),
                on_true: product(time, "cooking/fast_burn_time_multiplier"),
                on_false: product(time, "cooking/normal_burn_time_multiplier"),
            },
        );
    }
    providers
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

fn java_round(value: f32) -> i32 {
    (f64::from(value) + 0.5).floor() as i32
}

fn saturated_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
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

    #[test]
    fn parses_direct_and_compact_holder_sets() {
        let provider =
            parse_json(r#"{"type":"sum","operands":[1,{"type":"product","operands":2}]}"#).unwrap();
        assert!(matches!(provider, NumberProvider::Sum(_)));
    }

    #[test]
    fn json_nesting_matches_gson_limit() {
        fn nested_sum(depth: usize) -> String {
            let mut provider = "1".to_owned();
            for _ in 0..depth {
                provider = format!(r#"{{"type":"sum","operands":[{provider}]}}"#);
            }
            provider
        }

        assert!(parse_json(&nested_sum(65)).is_ok());
        assert!(
            parse_json(&nested_sum(128))
                .unwrap_err()
                .contains("nesting limit 255")
        );
    }

    #[test]
    fn weighted_list_rejects_gson_unsupported_number_scale() {
        let error = parse_json(
            r#"{"type":"weighted_list","distribution":[{"data":1,"weight":1e10000},{"data":2,"weight":1}]}"#,
        )
        .unwrap_err();
        assert!(error.contains("number scale"));
    }

    #[test]
    fn rejects_provider_cycles() {
        let first = Identifier::parse("example:first").unwrap();
        let second = Identifier::parse("example:second").unwrap();
        let providers = HashMap::from([
            (
                first.clone(),
                NumberProvider::Sum(NumberProviderSet::Direct(vec![
                    NumberProviderReference::Named(second.clone()),
                ])),
            ),
            (
                second,
                NumberProvider::Sum(NumberProviderSet::Direct(vec![
                    NumberProviderReference::Named(first),
                ])),
            ),
        ]);
        assert!(
            LootRegistry::new(providers, HashMap::new(), HashMap::new(), HashMap::new()).is_err()
        );
    }

    #[test]
    fn integer_aggregates_round_each_operand() {
        let registry = LootRegistry::empty();
        let provider = NumberProviderReference::Inline(Box::new(
            parse_json(r#"{"type":"sum","operands":[0.6,0.6]}"#).unwrap(),
        ));
        let mut random = LegacyRandom::default();
        let execution_context = ExecutionContext::new(
            crate::execution_context::Position::new(0.0, 0.0, 0.0),
            crate::execution_context::Rotation::new(0.0, 0.0),
        );
        assert_eq!(
            registry.get_float(
                &provider,
                &Scoreboard::default(),
                &CommandStorage::default(),
                &execution_context,
                &mut random,
            ),
            Ok(1.2)
        );
        assert_eq!(
            registry.get_int(
                &provider,
                &Scoreboard::default(),
                &CommandStorage::default(),
                &execution_context,
                &mut random,
            ),
            Ok(2)
        );
    }

    #[test]
    fn java_round_does_not_round_the_half_addition_as_float() {
        assert_eq!(java_round(8_388_609.0), 8_388_609);
        assert_eq!(java_round(-8_388_609.0), -8_388_609);
        assert_eq!(java_round(f32::NAN), 0);
        assert_eq!(java_round(f32::INFINITY), i32::MAX);
        assert_eq!(java_round(f32::NEG_INFINITY), i32::MIN);
    }
}
