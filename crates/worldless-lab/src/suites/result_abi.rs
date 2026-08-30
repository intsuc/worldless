use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const CALL_COUNT: usize = 31;
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const CHECKSUM_MULTIPLIER: i32 = 31;
const WIDTHS: &[(&str, usize)] = &[
    ("width_1", 1),
    ("width_2", 2),
    ("width_4", 4),
    ("width_8", 8),
    ("width_16", 16),
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "score_slots",
    },
    VariantSpec {
        slug: "return_head",
    },
    VariantSpec { slug: "caller_out" },
    VariantSpec {
        slug: "callee_frame",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "result_abi",
    world_seed: 0,
    command_limit: 8_192,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    WIDTHS
        .iter()
        .map(|&(slug, width)| build_case(slug, width))
        .collect()
}

fn build_case(slug: &'static str, width: usize) -> Result<Case, LabError> {
    let checksum = evaluate(width, CASE_SEED);
    let width = i32::try_from(width).expect("result-ABI width fits i32");
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!("{{width:{width},seed:{CASE_SEED}}}"),
        )?,
        expected_output: parse(
            slug,
            "expected output",
            &format!("{{width:{width},checksum:{checksum}}}"),
        )?,
    })
}

fn evaluate(width: usize, seed: i32) -> i32 {
    let mut state = seed;
    let mut checksum = 1_i32;
    for _ in 0..CALL_COUNT {
        for _ in 0..width {
            state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
            checksum = checksum
                .wrapping_mul(CHECKSUM_MULTIPLIER)
                .wrapping_add(state);
        }
    }
    checksum
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `result_abi`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
