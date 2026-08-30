use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CALL_COUNT: usize = 63;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec { slug: "score_slot" },
    VariantSpec {
        slug: "score_return",
    },
    VariantSpec {
        slug: "storage_return",
    },
    VariantSpec {
        slug: "macro_return",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "call_abi",
    world_seed: 0,
    command_limit: 4_096,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    [
        ("single", vec![0]),
        ("repeat_1", repeated(1)),
        ("cycle_8", repeated(8)),
        ("cycle_9", repeated(9)),
        (
            "unique_63",
            (0..i32::try_from(CALL_COUNT).expect("call count fits i32")).collect(),
        ),
    ]
    .into_iter()
    .map(|(slug, indices)| build_case(slug, indices))
    .collect()
}

fn repeated(period: i32) -> Vec<i32> {
    (0..CALL_COUNT)
        .map(|index| i32::try_from(index).expect("call index fits i32") % period)
        .collect()
}

fn pair(index: i32) -> (i32, i32) {
    (
        i32::MIN
            .checked_add(index)
            .expect("case index keeps the left argument in range"),
        i32::MAX
            .checked_sub(index * 17)
            .expect("case index keeps the right argument in range"),
    )
}

fn build_case(slug: &'static str, indices: Vec<i32>) -> Result<Case, LabError> {
    let (a, b): (Vec<_>, Vec<_>) = indices.iter().copied().map(pair).unzip();
    let checksum = a
        .iter()
        .copied()
        .zip(b.iter().copied())
        .fold(1_i32, |checksum, (a, b)| {
            let leaf = a
                .wrapping_mul(31)
                .wrapping_add(b)
                .wrapping_mul(31)
                .wrapping_add(7);
            checksum.wrapping_mul(31).wrapping_add(leaf)
        });
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!("{{a:{},b:{}}}", int_array(&a), int_array(&b)),
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
            "suite `call_abi`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
