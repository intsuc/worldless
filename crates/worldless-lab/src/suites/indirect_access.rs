use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const ACCESS_COUNT: usize = 63;
const VALUES: [i32; 16] = [
    i32::MIN,
    -1,
    0,
    1,
    2,
    3,
    5,
    8,
    13,
    21,
    34,
    55,
    89,
    144,
    i32::MAX,
    -123_456_789,
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "dynamic_path",
    },
    VariantSpec {
        slug: "specialized_call",
    },
    VariantSpec {
        slug: "binary_dispatch",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "indirect_access",
    world_seed: 0,
    command_limit: 16_384,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    let hot_nine = [0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8];
    [
        ("repeat_1", repeated(&[15])),
        ("cycle_8", repeated(&(0..8).collect::<Vec<_>>())),
        ("cycle_9", repeated(&(0..9).collect::<Vec<_>>())),
        ("cycle_16", repeated(&(0..16).collect::<Vec<_>>())),
        ("hot_9", repeated(&hot_nine)),
    ]
    .into_iter()
    .map(|(slug, indices)| build_case(slug, indices))
    .collect()
}

fn repeated(pattern: &[i32]) -> Vec<i32> {
    pattern.iter().copied().cycle().take(ACCESS_COUNT).collect()
}

fn build_case(slug: &'static str, indices: Vec<i32>) -> Result<Case, LabError> {
    let checksum = indices.iter().fold(1_i32, |checksum, &index| {
        let value = VALUES[usize::try_from(index).expect("case indices are non-negative")];
        checksum.wrapping_mul(31).wrapping_add(value)
    });
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!(
                "{{values:{},indices:{}}}",
                int_array(&VALUES),
                int_array(&indices)
            ),
        )?,
        expected_output: parse(slug, "expected output", &format!("{{checksum:{checksum}}}"))?,
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

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `indirect_access`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
