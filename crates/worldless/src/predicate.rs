use crate::{
    execution_context::ExecutionContext,
    nbt::{CommandStorage, Tag},
    number_provider::{
        FloatProvider, FloatProviderReference, Input, IntProvider, IntProviderReference,
        LootRegistry, ProviderReference, RegistryResource, ascii_string, identifier_field,
        int_value, parse_float_reference, parse_int_reference, required_field,
    },
    program::Scoreboard,
    random::LegacyRandom,
    resource::Identifier,
    resource_json,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PredicateReference {
    Named(Identifier),
    Inline(Box<LootPredicate>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PredicateSet {
    Direct(Vec<PredicateReference>),
    Tag(Identifier),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LootPredicate {
    AllOf(PredicateSet),
    AnyOf(PredicateSet),
    Inverted(PredicateReference),
    RandomChance {
        chance: FloatProviderReference,
    },
    IntValueCheck {
        value: IntProviderReference,
        range: IntRange,
    },
    FloatValueCheck {
        value: FloatProviderReference,
        range: FloatRange,
    },
    LocationCheck {
        position: PositionPredicate,
        offset: [i32; 3],
    },
    AbsentContext {
        result: bool,
        referenced_int_providers: Vec<IntProviderReference>,
    },
    MissingContextParameter {
        parameter: &'static str,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProviderRange<T> {
    Point(ProviderReference<T>),
    Bounds {
        min: Option<ProviderReference<T>>,
        max: Option<ProviderReference<T>>,
    },
}

pub(crate) type IntRange = ProviderRange<IntProvider>;
pub(crate) type FloatRange = ProviderRange<FloatProvider>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PositionPredicate {
    x: DoubleRange,
    y: DoubleRange,
    z: DoubleRange,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DoubleRange {
    min: Option<f64>,
    max: Option<f64>,
}

impl PositionPredicate {
    const ANY: Self = Self {
        x: DoubleRange::ANY,
        y: DoubleRange::ANY,
        z: DoubleRange::ANY,
    };

    fn test(self, position: [f64; 3]) -> bool {
        self.x.test(position[0]) && self.y.test(position[1]) && self.z.test(position[2])
    }
}

impl DoubleRange {
    const ANY: Self = Self {
        min: None,
        max: None,
    };

    fn test(self, value: f64) -> bool {
        !self.min.is_some_and(|min| min > value) && !self.max.is_some_and(|max| max < value)
    }
}

impl LootPredicate {
    pub(crate) fn test(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        match self {
            Self::AllOf(predicates) => {
                for predicate in registry.predicate_values(predicates) {
                    if !predicate.test(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::AnyOf(predicates) => {
                for predicate in registry.predicate_values(predicates) {
                    if predicate.test(
                        registry,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Inverted(predicate) => registry
                .resolve_predicate(predicate)
                .test(
                    registry,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )
                .map(|result| !result),
            Self::RandomChance { chance } => {
                let chance = registry.get_float(
                    chance,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(random.next_float() < chance)
            }
            Self::IntValueCheck { value, range } => range.test(
                value,
                registry,
                scoreboard,
                command_storage,
                execution_context,
                random,
            ),
            Self::FloatValueCheck { value, range } => range.test(
                value,
                registry,
                scoreboard,
                command_storage,
                execution_context,
                random,
            ),
            Self::LocationCheck { position, offset } => {
                let origin = execution_context.position();
                Ok(position.test([
                    origin.x() + f64::from(offset[0]),
                    origin.y() + f64::from(offset[1]),
                    origin.z() + f64::from(offset[2]),
                ]))
            }
            Self::AbsentContext { result, .. } => Ok(*result),
            Self::MissingContextParameter { parameter } => {
                Err(format!("loot context parameter `{parameter}` is absent"))
            }
        }
    }
}

impl ProviderRange<IntProvider> {
    fn test(
        &self,
        value: &IntProviderReference,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        match self {
            Self::Point(expected) => {
                let value = registry.get_int(
                    value,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(value
                    == registry.get_int(
                        expected,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?)
            }
            Self::Bounds { min, max } => {
                if min.is_none() && max.is_none() {
                    return Ok(true);
                }
                let value = registry.get_int(
                    value,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                if let Some(min) = min
                    && value
                        < registry.get_int(
                            min,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?
                {
                    return Ok(false);
                }
                if let Some(max) = max
                    && value
                        > registry.get_int(
                            max,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?
                {
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }

    pub(crate) fn collect_dependencies(
        &self,
        registry: &LootRegistry,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match self {
            Self::Point(value) => registry.collect_int_reference_dependencies(value, dependencies),
            Self::Bounds { min, max } => {
                if let Some(min) = min {
                    registry.collect_int_reference_dependencies(min, dependencies)?;
                }
                if let Some(max) = max {
                    registry.collect_int_reference_dependencies(max, dependencies)?;
                }
                Ok(())
            }
        }
    }
}

impl ProviderRange<FloatProvider> {
    fn test(
        &self,
        value: &FloatProviderReference,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        execution_context: &ExecutionContext,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        match self {
            Self::Point(expected) => {
                let value = registry.get_float(
                    value,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                Ok(value
                    == registry.get_float(
                        expected,
                        scoreboard,
                        command_storage,
                        execution_context,
                        random,
                    )?)
            }
            Self::Bounds { min, max } => {
                if min.is_none() && max.is_none() {
                    return Ok(true);
                }
                let value = registry.get_float(
                    value,
                    scoreboard,
                    command_storage,
                    execution_context,
                    random,
                )?;
                if let Some(min) = min
                    && value
                        < registry.get_float(
                            min,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?
                {
                    return Ok(false);
                }
                if let Some(max) = max
                    && value
                        > registry.get_float(
                            max,
                            scoreboard,
                            command_storage,
                            execution_context,
                            random,
                        )?
                {
                    return Ok(false);
                }
                Ok(true)
            }
        }
    }

    pub(crate) fn collect_dependencies(
        &self,
        registry: &LootRegistry,
        dependencies: &mut Vec<RegistryResource>,
    ) -> Result<(), String> {
        match self {
            Self::Point(value) => {
                registry.collect_float_reference_dependencies(value, dependencies)
            }
            Self::Bounds { min, max } => {
                if let Some(min) = min {
                    registry.collect_float_reference_dependencies(min, dependencies)?;
                }
                if let Some(max) = max {
                    registry.collect_float_reference_dependencies(max, dependencies)?;
                }
                Ok(())
            }
        }
    }
}

impl<T> ProviderRange<T> {
    fn into_references(self) -> Vec<ProviderReference<T>> {
        match self {
            Self::Point(value) => vec![value],
            Self::Bounds { min, max } => min.into_iter().chain(max).collect(),
        }
    }
}

pub(crate) fn parse_json(contents: &str) -> Result<LootPredicate, String> {
    let value = resource_json::parse(contents)?;
    parse_direct(Input::Json(&value), "root")
}

pub(crate) fn parse_inline_tag(
    value: &Tag,
    registry: &LootRegistry,
) -> Result<LootPredicate, String> {
    let predicate = parse_direct(Input::Nbt(value), "predicate")?;
    registry.validate_inline_predicate(&predicate)?;
    Ok(predicate)
}

pub(crate) fn parse_reference(input: Input<'_>, path: &str) -> Result<PredicateReference, String> {
    if let Some(value) = input.string() {
        let value = ascii_string(&value, path)?;
        let id = Identifier::parse(&value)
            .ok_or_else(|| format!("`{path}` has invalid predicate identifier `{value}`"))?;
        return Ok(PredicateReference::Named(id));
    }
    parse_direct(input, path).map(|predicate| PredicateReference::Inline(Box::new(predicate)))
}

fn parse_direct(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    if !input.is_object() {
        return Err(format!("`{path}` must be an object"));
    }
    let predicate_type = identifier_field(input, path, "type")?;
    if predicate_type.namespace() != "minecraft" {
        return Err(format!(
            "predicate type `{predicate_type}` is not supported"
        ));
    }
    match predicate_type.path() {
        "all_of" => Ok(LootPredicate::AllOf(predicate_set_field(
            input, path, "terms",
        )?)),
        "any_of" => Ok(LootPredicate::AnyOf(predicate_set_field(
            input, path, "terms",
        )?)),
        "inverted" => Ok(LootPredicate::Inverted(predicate_reference_field(
            input, path, "term",
        )?)),
        "random_chance" => Ok(LootPredicate::RandomChance {
            chance: float_provider_reference_field(input, path, "chance")?,
        }),
        "int_value_check" => Ok(LootPredicate::IntValueCheck {
            value: int_provider_reference_field(input, path, "value")?,
            range: parse_int_range(
                required_field(input, path, "test")?,
                &format!("{path}.test"),
            )?,
        }),
        "float_value_check" => Ok(LootPredicate::FloatValueCheck {
            value: float_provider_reference_field(input, path, "value")?,
            range: parse_float_range(
                required_field(input, path, "test")?,
                &format!("{path}.test"),
            )?,
        }),
        "location_check" => parse_location_check(input, path),
        "entity_properties" => parse_entity_properties(input, path),
        "killed_by_player" => Ok(absent_context(false)),
        "entity_scores" => parse_entity_scores(input, path),
        "match_block" => parse_match_block(input, path),
        "match_tool" => parse_match_tool(input, path),
        "survives_explosion" => Ok(absent_context(true)),
        "damage_source_properties" => parse_damage_source_properties(input, path),
        "weather_check" => parse_weather_check(input, path),
        "enchantment_active_check" => parse_enchantment_active_check(input, path),
        "random_chance_with_enchanted_bonus" | "table_bonus" => Err(format!(
            "predicate type `{predicate_type}` requires an enchantment registry that Worldless does not support"
        )),
        "time_check" | "environment_attribute_check" => Err(format!(
            "predicate type `{predicate_type}` depends on a loot context outside Worldless scope"
        )),
        _ => Err(format!(
            "predicate type `{predicate_type}` is not supported"
        )),
    }
}

fn absent_context(result: bool) -> LootPredicate {
    LootPredicate::AbsentContext {
        result,
        referenced_int_providers: Vec::new(),
    }
}

fn parse_entity_properties(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    parse_entity_target(input, path)?;
    let Some(predicate) = input.field("predicate") else {
        return Ok(absent_context(true));
    };
    require_empty_nested_predicate(predicate, &format!("{path}.predicate"), "entity")?;
    Ok(absent_context(false))
}

fn parse_entity_scores(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    parse_entity_target(input, path)?;
    let scores_path = format!("{path}.scores");
    let scores = required_field(input, path, "scores")?
        .object_entries()
        .ok_or_else(|| format!("`{scores_path}` must be an object"))?;
    let mut referenced_int_providers = Vec::new();
    for (score, range) in scores {
        let range = parse_int_range(range, &format!("{scores_path}.{score}"))?;
        referenced_int_providers.extend(range.into_references());
    }
    Ok(LootPredicate::AbsentContext {
        result: false,
        referenced_int_providers,
    })
}

fn parse_entity_target(input: Input<'_>, path: &str) -> Result<(), String> {
    let target_path = format!("{path}.entity");
    let target = required_field(input, path, "entity")?
        .string()
        .ok_or_else(|| format!("`{target_path}` must be a string"))?;
    let target = ascii_string(&target, &target_path)?;
    if [
        "this",
        "attacker",
        "direct_attacker",
        "attacking_player",
        "target_entity",
        "interacting_entity",
    ]
    .contains(&target.as_str())
    {
        Ok(())
    } else {
        Err(format!(
            "`{target_path}` has invalid entity target `{target}`"
        ))
    }
}

fn parse_match_block(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    reject_present_fields(
        input,
        path,
        &["blocks", "state", "nbt", "components", "predicates"],
        "block predicate decoding",
    )?;
    Ok(absent_context(false))
}

fn parse_match_tool(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    if let Some(predicate) = input.field("predicate") {
        reject_nested_predicate_fields(
            predicate,
            &format!("{path}.predicate"),
            &["items", "count", "components", "predicates"],
            "item predicate decoding",
        )?;
    }
    Ok(absent_context(false))
}

fn parse_damage_source_properties(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    if let Some(predicate) = input.field("predicate") {
        reject_nested_predicate_fields(
            predicate,
            &format!("{path}.predicate"),
            &["tags", "direct_entity", "source_entity", "is_direct"],
            "damage-source predicate decoding",
        )?;
    }
    Ok(absent_context(false))
}

fn parse_weather_check(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    let raining = optional_boolean_field(input, path, "raining")?;
    let thundering = optional_boolean_field(input, path, "thundering")?;
    if raining.is_some() || thundering.is_some() {
        return Err(
            "predicate type `minecraft:weather_check` observes physical-world weather outside Worldless scope"
                .to_owned(),
        );
    }
    Ok(absent_context(true))
}

fn parse_enchantment_active_check(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    let active_path = format!("{path}.active");
    required_field(input, path, "active")?
        .boolean()
        .ok_or_else(|| format!("`{active_path}` must be a boolean"))?;
    Ok(LootPredicate::MissingContextParameter {
        parameter: "minecraft:enchantment_active",
    })
}

fn optional_boolean_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<Option<bool>, String> {
    let Some(value) = input.field(field) else {
        return Ok(None);
    };
    value
        .boolean()
        .map(Some)
        .ok_or_else(|| format!("`{path}.{field}` must be a boolean"))
}

fn require_empty_nested_predicate(input: Input<'_>, path: &str, kind: &str) -> Result<(), String> {
    if input.is_empty_object() {
        Ok(())
    } else if input.is_object() {
        Err(format!(
            "`{path}` requires unsupported {kind} predicate decoding"
        ))
    } else {
        Err(format!("`{path}` must be an object"))
    }
}

fn reject_nested_predicate_fields(
    input: Input<'_>,
    path: &str,
    fields: &[&str],
    unsupported: &str,
) -> Result<(), String> {
    if !input.is_object() {
        return Err(format!("`{path}` must be an object"));
    }
    reject_present_fields(input, path, fields, unsupported)
}

fn reject_present_fields(
    input: Input<'_>,
    path: &str,
    fields: &[&str],
    unsupported: &str,
) -> Result<(), String> {
    if let Some(field) = fields.iter().find(|field| input.field(field).is_some()) {
        Err(format!(
            "`{path}.{field}` requires unsupported {unsupported}"
        ))
    } else {
        Ok(())
    }
}

fn parse_location_check(input: Input<'_>, path: &str) -> Result<LootPredicate, String> {
    let position = input
        .field("predicate")
        .map_or(Ok(PositionPredicate::ANY), |value| {
            parse_location_predicate(value, &format!("{path}.predicate"))
        })?;
    let mut offset = [0; 3];
    for (index, field) in ["offsetX", "offsetY", "offsetZ"].into_iter().enumerate() {
        if let Some(value) = input.field(field) {
            offset[index] = int_value(value, &format!("{path}.{field}"))?;
        }
    }
    Ok(LootPredicate::LocationCheck { position, offset })
}

fn parse_location_predicate(input: Input<'_>, path: &str) -> Result<PositionPredicate, String> {
    if !input.is_object() {
        return Err(format!("`{path}` must be an object"));
    }
    for field in [
        "biomes",
        "structures",
        "dimension",
        "smokey",
        "light",
        "block",
        "fluid",
        "can_see_sky",
    ] {
        if input.field(field).is_some() {
            return Err(format!(
                "`{path}.{field}` depends on physical-world state outside Worldless scope"
            ));
        }
    }
    input
        .field("position")
        .map_or(Ok(PositionPredicate::ANY), |value| {
            parse_position_predicate(value, &format!("{path}.position"))
        })
}

fn parse_position_predicate(input: Input<'_>, path: &str) -> Result<PositionPredicate, String> {
    if !input.is_object() {
        return Err(format!("`{path}` must be an object"));
    }
    let mut ranges = [DoubleRange::ANY; 3];
    for (index, field) in ["x", "y", "z"].into_iter().enumerate() {
        if let Some(value) = input.field(field) {
            ranges[index] = parse_double_range(value, &format!("{path}.{field}"))?;
        }
    }
    Ok(PositionPredicate {
        x: ranges[0],
        y: ranges[1],
        z: ranges[2],
    })
}

fn parse_double_range(input: Input<'_>, path: &str) -> Result<DoubleRange, String> {
    if let Some(value) = double_value(input) {
        return Ok(DoubleRange {
            min: Some(value),
            max: Some(value),
        });
    }
    if !input.is_object() {
        return Err(format!("`{path}` must be a number or an object"));
    }
    let min = input
        .field("min")
        .map(|value| required_double(value, &format!("{path}.min")))
        .transpose()?;
    let max = input
        .field("max")
        .map(|value| required_double(value, &format!("{path}.max")))
        .transpose()?;
    if min
        .zip(max)
        .is_some_and(|(min, max)| java_double_compare(min, max).is_gt())
    {
        return Err(format!("`{path}` has swapped minimum and maximum bounds"));
    }
    Ok(DoubleRange { min, max })
}

fn required_double(input: Input<'_>, path: &str) -> Result<f64, String> {
    double_value(input).ok_or_else(|| format!("`{path}` must be a number"))
}

fn double_value(input: Input<'_>) -> Option<f64> {
    match input {
        Input::Json(serde_json::Value::Number(value)) => value.to_string().parse().ok(),
        Input::Nbt(value) => value.double_value(),
        Input::NbtByte(value) => Some(f64::from(value)),
        Input::NbtInt(value) => Some(f64::from(value)),
        Input::NbtLong(value) => Some(value as f64),
        Input::Json(_) => None,
    }
}

fn java_double_compare(left: f64, right: f64) -> std::cmp::Ordering {
    if left < right {
        return std::cmp::Ordering::Less;
    }
    if left > right {
        return std::cmp::Ordering::Greater;
    }
    java_double_to_long_bits(left).cmp(&java_double_to_long_bits(right))
}

fn java_double_to_long_bits(value: f64) -> i64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000_u64 as i64
    } else {
        value.to_bits() as i64
    }
}

fn predicate_set_field(input: Input<'_>, path: &str, field: &str) -> Result<PredicateSet, String> {
    let field_path = format!("{path}.{field}");
    let value = required_field(input, path, field)?;
    if let Some(string) = value.string() {
        let text = ascii_string(&string, &field_path)?;
        if let Some(tag) = text.strip_prefix('#') {
            let id = Identifier::parse(tag)
                .ok_or_else(|| format!("`{field_path}` has invalid tag identifier `{text}`"))?;
            return Ok(PredicateSet::Tag(id));
        }
    }
    let values = value.list().unwrap_or_else(|| vec![value]);
    let predicates = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| parse_reference(value, &format!("{field_path}[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PredicateSet::Direct(predicates))
}

fn predicate_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<PredicateReference, String> {
    parse_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn int_provider_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<IntProviderReference, String> {
    parse_int_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn float_provider_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<FloatProviderReference, String> {
    parse_float_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn parse_int_range(input: Input<'_>, path: &str) -> Result<IntRange, String> {
    parse_provider_range(input, path, parse_int_reference)
}

fn parse_float_range(input: Input<'_>, path: &str) -> Result<FloatRange, String> {
    parse_provider_range(input, path, parse_float_reference)
}

fn parse_provider_range<T>(
    input: Input<'_>,
    path: &str,
    parse_reference: fn(Input<'_>, &str) -> Result<ProviderReference<T>, String>,
) -> Result<ProviderRange<T>, String> {
    match parse_reference(input, path) {
        Ok(value) => return Ok(ProviderRange::Point(value)),
        Err(reason) if !input.is_object() => return Err(reason),
        Err(_) => {}
    }
    let min = input
        .field("min")
        .map(|value| parse_reference(value, &format!("{path}.min")))
        .transpose()?;
    let max = input
        .field("max")
        .map(|value| parse_reference(value, &format!("{path}.max")))
        .transpose()?;
    Ok(ProviderRange::Bounds { min, max })
}

pub(crate) fn builtin_predicates() -> std::collections::HashMap<Identifier, LootPredicate> {
    // These vanilla predicates read BLOCK_STATE or TOOL, neither of which is
    // present in the supported command/default loot contexts.
    [
        "block/fast_cooking",
        "tool/can_silk_touch",
        "tool/can_shear",
    ]
    .into_iter()
    .map(|path| {
        (
            Identifier::from_parts("minecraft", path)
                .expect("built-in predicate identifiers are valid"),
            LootPredicate::AnyOf(PredicateSet::Direct(Vec::new())),
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_context::{Position, Rotation};

    #[test]
    fn parses_supported_typed_predicate_shapes() {
        assert!(matches!(
            parse_json(r#"{"type":"all_of","terms":[]}"#).unwrap(),
            LootPredicate::AllOf(PredicateSet::Direct(values)) if values.is_empty()
        ));
        assert!(matches!(
            parse_json(r#"{"type":"random_chance","chance":0.25}"#).unwrap(),
            LootPredicate::RandomChance {
                chance: ProviderReference::Inline(chance),
            } if *chance == FloatProvider::Constant(0.25)
        ));
        assert!(matches!(
            parse_json(r#"{"type":"int_value_check","value":2,"test":"example:point"}"#).unwrap(),
            LootPredicate::IntValueCheck {
                range: ProviderRange::Point(ProviderReference::Named(id)),
                ..
            } if id.to_string() == "example:point"
        ));
        assert!(matches!(
            parse_json(r#"{"type":"int_value_check","value":2,"test":{"min":1,"max":3}}"#).unwrap(),
            LootPredicate::IntValueCheck {
                range: ProviderRange::Bounds {
                    min: Some(_),
                    max: Some(_),
                },
                ..
            }
        ));
        assert!(matches!(
            parse_json(r#"{"type":"float_value_check","value":1.5,"test":1.5}"#).unwrap(),
            LootPredicate::FloatValueCheck {
                range: ProviderRange::Point(ProviderReference::Inline(expected)),
                ..
            } if *expected == FloatProvider::Constant(1.5)
        ));
        assert!(matches!(
            parse_json(r#"{"type":"float_value_check","value":1.5,"test":{"min":1.0,"max":2.0}}"#)
                .unwrap(),
            LootPredicate::FloatValueCheck {
                range: ProviderRange::Bounds {
                    min: Some(_),
                    max: Some(_),
                },
                ..
            }
        ));
        assert!(
            parse_json(r#"{"type":"value_check","value":1,"range":1}"#)
                .unwrap_err()
                .contains("not supported")
        );
    }

    #[test]
    fn unbounded_typed_range_does_not_evaluate_its_input() {
        let predicate = parse_json(
            r#"{
                "type":"int_value_check",
                "value":{"type":"uniform","min":0,"max":1},
                "test":{}
            }"#,
        )
        .unwrap();
        let mut actual_random = LegacyRandom::default();
        let mut expected_random = LegacyRandom::default();
        let context = ExecutionContext::new(Position::new(0.0, 0.0, 0.0), Rotation::new(0.0, 0.0));

        assert_eq!(
            predicate.test(
                &LootRegistry::empty(),
                &Scoreboard::default(),
                &CommandStorage::default(),
                &context,
                &mut actual_random,
            ),
            Ok(true)
        );
        assert_eq!(actual_random.next_float(), expected_random.next_float());
    }

    #[test]
    fn entity_scores_retains_typed_int_provider_dependencies() {
        let predicate = parse_json(
            r#"{
                "type":"entity_scores",
                "entity":"this",
                "scores":{
                    "point":"example:point",
                    "bounds":{"min":"example:min","max":"example:max"}
                }
            }"#,
        )
        .unwrap();

        assert!(matches!(
            predicate,
            LootPredicate::AbsentContext {
                result: false,
                referenced_int_providers,
            } if referenced_int_providers.len() == 3
        ));
    }

    #[test]
    fn location_check_parses_minecraft_bounds_and_offsets() {
        let predicate = parse_json(
            r#"{
                "type":"location_check",
                "predicate":{"position":{
                    "x":{"min":11.0,"max":11.0},
                    "y":2.5,
                    "z":{"max":29.0}
                }},
                "offsetX":1,
                "offsetZ":-1
            }"#,
        )
        .unwrap();
        let context =
            ExecutionContext::new(Position::new(10.0, 2.5, 30.0), Rotation::new(0.0, 0.0));

        assert_eq!(
            predicate.test(
                &LootRegistry::empty(),
                &Scoreboard::default(),
                &CommandStorage::default(),
                &context,
                &mut LegacyRandom::default(),
            ),
            Ok(true)
        );
    }

    #[test]
    fn location_check_rejects_every_world_dependent_location_field() {
        for (field, value) in [
            ("biomes", "[]"),
            ("structures", "[]"),
            ("dimension", r#""minecraft:overworld""#),
            ("smokey", "false"),
            ("light", "{}"),
            ("block", "{}"),
            ("fluid", "{}"),
            ("can_see_sky", "false"),
        ] {
            let source =
                format!(r#"{{"type":"location_check","predicate":{{"{field}":{value}}}}}"#);
            let error = parse_json(&source).unwrap_err();
            assert!(
                error.contains(&format!("root.predicate.{field}")),
                "{error}"
            );
            assert!(error.contains("physical-world state"), "{error}");
        }
    }

    #[test]
    fn location_check_validates_bounds_with_java_double_ordering() {
        for source in [
            r#"{"type":"location_check","predicate":{"position":{"x":{"min":2.0,"max":1.0}}}}"#,
            r#"{"type":"location_check","predicate":{"position":{"x":{"min":0.0,"max":-0.0}}}}"#,
        ] {
            assert!(parse_json(source).unwrap_err().contains("swapped"));
        }
        assert!(
            parse_json(
                r#"{"type":"location_check","predicate":{"position":{"x":{"min":-0.0,"max":0.0}}}}"#
            )
            .is_ok()
        );
        assert!(
            DoubleRange {
                min: Some(1.0),
                max: Some(1.0),
            }
            .test(f64::NAN)
        );
    }
}
