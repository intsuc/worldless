use std::collections::HashMap;

use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "linear_scan",
    },
    VariantSpec {
        slug: "nbt_compound",
    },
    VariantSpec { slug: "scoreboard" },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "int_map",
    world_seed: 0,
    command_limit: 262_144,
    variants: VARIANTS,
    build_cases,
};

struct Input {
    slug: &'static str,
    keys: Vec<i32>,
    values: Vec<i32>,
    queries: Vec<i32>,
}

fn build_cases() -> Result<Vec<Case>, LabError> {
    let keys_32 = permuted_keys(32, 13);
    let values_32 = mapped_values(&keys_32);
    let keys_128 = permuted_keys(128, 53);
    let values_128 = mapped_values(&keys_128);

    let inputs = vec![
        Input {
            slug: "empty",
            keys: vec![],
            values: vec![],
            queries: vec![],
        },
        Input {
            slug: "empty_map_misses_4",
            keys: vec![],
            values: vec![],
            queries: vec![i32::MIN, -1, 0, i32::MAX],
        },
        Input {
            slug: "singleton_hit_miss",
            keys: vec![7],
            values: vec![-3],
            queries: vec![7, 8, 7],
        },
        Input {
            slug: "zero_and_missing",
            keys: vec![1],
            values: vec![0],
            queries: vec![1, 2],
        },
        Input {
            slug: "duplicate_extremes",
            keys: vec![i32::MIN, 7, i32::MAX, i32::MIN, 7],
            values: vec![i32::MAX, 0, i32::MIN, -1, 42],
            queries: vec![i32::MIN, 7, i32::MAX, 0],
        },
        Input {
            slug: "hits_32",
            keys: keys_32.clone(),
            values: values_32.clone(),
            queries: keys_32.iter().rev().copied().collect(),
        },
        Input {
            slug: "misses_32",
            keys: keys_32,
            values: values_32,
            queries: (1_000..1_032).collect(),
        },
        Input {
            slug: "entries_128_no_queries",
            keys: keys_128.clone(),
            values: values_128.clone(),
            queries: vec![],
        },
        Input {
            slug: "hits_128",
            keys: keys_128.clone(),
            values: values_128.clone(),
            queries: keys_128.iter().rev().copied().collect(),
        },
        Input {
            slug: "hot_last_128",
            keys: keys_128.clone(),
            values: values_128.clone(),
            queries: vec![*keys_128.last().expect("the generated map is non-empty"); 128],
        },
        Input {
            slug: "hot_first_128",
            keys: keys_128.clone(),
            values: values_128.clone(),
            queries: vec![keys_128[0]; 128],
        },
        Input {
            slug: "mixed_128",
            keys: keys_128.clone(),
            values: values_128,
            queries: (0..128)
                .map(|index| {
                    if index % 2 == 0 {
                        keys_128[(index * 37) % 128]
                    } else {
                        10_000 + i32::try_from(index).expect("case index fits i32")
                    }
                })
                .collect(),
        },
    ];

    inputs.into_iter().map(build_case).collect()
}

fn permuted_keys(length: usize, multiplier: usize) -> Vec<i32> {
    let offset = i32::try_from(length / 2).expect("case length fits i32");
    (0..length)
        .map(|index| {
            i32::try_from((index * multiplier) % length).expect("case key fits i32") - offset
        })
        .collect()
}

fn mapped_values(keys: &[i32]) -> Vec<i32> {
    keys.iter().map(|key| key * 101 + 7).collect()
}

fn build_case(input: Input) -> Result<Case, LabError> {
    let mut map = HashMap::new();
    for (&key, &value) in input.keys.iter().zip(&input.values) {
        map.insert(key, value);
    }

    let mut found = Vec::with_capacity(input.queries.len());
    let mut values = Vec::with_capacity(input.queries.len());
    for query in &input.queries {
        if let Some(value) = map.get(query) {
            found.push(1);
            values.push(*value);
        } else {
            found.push(0);
            values.push(0);
        }
    }

    Ok(Case {
        slug: input.slug,
        input: parse(
            input.slug,
            "input",
            &format!(
                "{{keys:{},values:{},queries:{}}}",
                int_array(&input.keys),
                int_array(&input.values),
                int_array(&input.queries)
            ),
        )?,
        expected_output: parse(
            input.slug,
            "expected output",
            &format!(
                "{{found:{},values:{}}}",
                byte_array(&found),
                int_array(&values)
            ),
        )?,
    })
}

fn int_array(values: &[i32]) -> String {
    format!(
        "[I;{}]",
        values
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn byte_array(values: &[i8]) -> String {
    format!(
        "[B;{}]",
        values
            .iter()
            .map(|value| format!("{value}b"))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `int_map`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
