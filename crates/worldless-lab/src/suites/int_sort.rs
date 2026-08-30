use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec { slug: "insertion" },
    VariantSpec {
        slug: "bottom_up_merge",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "int_sort",
    world_seed: 0,
    command_limit: 131_072,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    let inputs = vec![
        ("empty", vec![]),
        ("singleton", vec![42]),
        (
            "mixed_extremes_7",
            vec![0, -1, 5, -1, i32::MAX, i32::MIN, 5],
        ),
        ("sorted_8", (-4..4).collect()),
        ("reverse_8", (-4..4).rev().collect()),
        ("sorted_32", (-16..16).collect()),
        ("reverse_32", (-16..16).rev().collect()),
        (
            "permuted_32",
            (0..32).map(|index| ((index * 13) % 32) - 16).collect(),
        ),
        ("sorted_128", (-64..64).collect()),
        ("reverse_128", (-64..64).rev().collect()),
        (
            "permuted_128",
            (0..128).map(|index| ((index * 53) % 128) - 64).collect(),
        ),
    ];

    inputs
        .into_iter()
        .map(|(slug, values)| build_case(slug, values))
        .collect()
}

fn build_case(slug: &'static str, values: Vec<i32>) -> Result<Case, LabError> {
    let mut expected = values.clone();
    expected.sort_unstable();
    Ok(Case {
        slug,
        input: parse_values(slug, "input", &values)?,
        expected_output: parse_values(slug, "expected output", &expected)?,
    })
}

fn parse_values(slug: &str, role: &str, values: &[i32]) -> Result<CompoundTag, LabError> {
    let body = values
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    CompoundTag::from_snbt(&format!("{{values:[I;{body}]}}")).map_err(|error| {
        LabError::from_message(format!(
            "suite `int_sort`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
