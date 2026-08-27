use crate::{
    nbt::{CommandStorage, Tag},
    number_provider::{
        Input, LegacyRandom, LootRegistry, NumberProviderReference, ascii_string, identifier_field,
        int_value, parse_reference as parse_number_provider_reference, required_field,
    },
    program::Scoreboard,
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
        chance: NumberProviderReference,
    },
    ValueCheck {
        value: NumberProviderReference,
        range: IntRange,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct IntRange {
    pub(crate) min: Option<NumberProviderReference>,
    pub(crate) max: Option<NumberProviderReference>,
}

impl LootPredicate {
    pub(crate) fn test(
        &self,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        match self {
            Self::AllOf(predicates) => {
                for predicate in registry.predicate_values(predicates) {
                    if !predicate.test(registry, scoreboard, command_storage, random)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::AnyOf(predicates) => {
                for predicate in registry.predicate_values(predicates) {
                    if predicate.test(registry, scoreboard, command_storage, random)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Inverted(predicate) => registry
                .resolve_predicate(predicate)
                .test(registry, scoreboard, command_storage, random)
                .map(|result| !result),
            Self::RandomChance { chance } => {
                let chance = registry.get_float(chance, scoreboard, command_storage, random)?;
                Ok(random.next_float() < chance)
            }
            Self::ValueCheck { value, range } => {
                let value = registry.get_int(value, scoreboard, command_storage, random)?;
                range.test(value, registry, scoreboard, command_storage, random)
            }
        }
    }
}

impl IntRange {
    fn test(
        &self,
        value: i32,
        registry: &LootRegistry,
        scoreboard: &Scoreboard,
        command_storage: &CommandStorage,
        random: &mut LegacyRandom,
    ) -> Result<bool, String> {
        if let Some(min) = &self.min
            && value < registry.get_int(min, scoreboard, command_storage, random)?
        {
            return Ok(false);
        }
        if let Some(max) = &self.max
            && value > registry.get_int(max, scoreboard, command_storage, random)?
        {
            return Ok(false);
        }
        Ok(true)
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
            chance: number_provider_reference_field(input, path, "chance")?,
        }),
        "value_check" => Ok(LootPredicate::ValueCheck {
            value: number_provider_reference_field(input, path, "value")?,
            range: parse_int_range(
                required_field(input, path, "range")?,
                &format!("{path}.range"),
            )?,
        }),
        "random_chance_with_enchanted_bonus"
        | "entity_properties"
        | "killed_by_player"
        | "entity_scores"
        | "match_block"
        | "match_tool"
        | "table_bonus"
        | "survives_explosion"
        | "damage_source_properties"
        | "location_check"
        | "weather_check"
        | "time_check"
        | "enchantment_active_check"
        | "environment_attribute_check" => Err(format!(
            "predicate type `{predicate_type}` depends on a loot context outside Worldless scope"
        )),
        _ => Err(format!(
            "predicate type `{predicate_type}` is not supported"
        )),
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

fn number_provider_reference_field(
    input: Input<'_>,
    path: &str,
    field: &str,
) -> Result<NumberProviderReference, String> {
    parse_number_provider_reference(
        required_field(input, path, field)?,
        &format!("{path}.{field}"),
    )
}

fn parse_int_range(input: Input<'_>, path: &str) -> Result<IntRange, String> {
    if input.number().is_some() {
        let value = NumberProviderReference::Inline(Box::new(
            crate::number_provider::NumberProvider::Constant(int_value(input, path)? as f32),
        ));
        return Ok(IntRange {
            min: Some(value.clone()),
            max: Some(value),
        });
    }
    if !input.is_object() {
        return Err(format!("`{path}` must be an integer or an object"));
    }
    let min = input
        .field("min")
        .map(|value| parse_number_provider_reference(value, &format!("{path}.min")))
        .transpose()?;
    let max = input
        .field("max")
        .map(|value| parse_number_provider_reference(value, &format!("{path}.max")))
        .transpose()?;
    Ok(IntRange { min, max })
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

    #[test]
    fn parses_supported_predicate_shapes() {
        assert!(matches!(
            parse_json(r#"{"type":"all_of","terms":[]}"#).unwrap(),
            LootPredicate::AllOf(PredicateSet::Direct(values)) if values.is_empty()
        ));
        assert!(matches!(
            parse_json(r#"{"type":"value_check","value":1.5,"range":{"min":1,"max":2}}"#).unwrap(),
            LootPredicate::ValueCheck { .. }
        ));
    }
}
