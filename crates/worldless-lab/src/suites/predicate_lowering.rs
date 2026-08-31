use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const TERM_COUNT: usize = 16;
const EVALUATION_COUNT: usize = 63;
const CHECKSUM_MULTIPLIER: i32 = 31;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "execute_chain",
    },
    VariantSpec {
        slug: "guard_return",
    },
    VariantSpec {
        slug: "score_product",
    },
    VariantSpec {
        slug: "predicate_resource",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "predicate_lowering",
    world_seed: 0,
    command_limit: 4_096,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    [
        ("false_0", Some(0)),
        ("false_4", Some(4)),
        ("false_8", Some(8)),
        ("false_15", Some(15)),
        ("all_true", None),
    ]
    .into_iter()
    .map(|(slug, false_index)| build_case(slug, false_index))
    .collect()
}

fn build_case(slug: &'static str, false_index: Option<usize>) -> Result<Case, LabError> {
    let mut terms = [1_i32; TERM_COUNT];
    if let Some(index) = false_index {
        terms[index] = 0;
    }
    let result = if terms.iter().all(|&term| term == 1) {
        1_i32
    } else {
        0_i32
    };
    let checksum = (0..EVALUATION_COUNT).fold(1_i32, |checksum, _| {
        checksum
            .wrapping_mul(CHECKSUM_MULTIPLIER)
            .wrapping_add(result)
    });

    Ok(Case {
        slug,
        input: parse(slug, "input", &format!("{{terms:{}}}", int_array(&terms)))?,
        expected_output: parse(
            slug,
            "expected output",
            &format!("{{result:{result},checksum:{checksum}}}"),
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

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `predicate_lowering`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
