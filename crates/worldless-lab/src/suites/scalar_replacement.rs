use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const UPDATE_MULTIPLIER: i32 = 31;
const UPDATE_ADDEND: i32 = 7;

#[derive(Clone, Copy)]
struct CaseSpec {
    slug: &'static str,
    width: usize,
    rounds: usize,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        slug: "w1_r1",
        width: 1,
        rounds: 1,
    },
    CaseSpec {
        slug: "w1_r4",
        width: 1,
        rounds: 4,
    },
    CaseSpec {
        slug: "w1_r16",
        width: 1,
        rounds: 16,
    },
    CaseSpec {
        slug: "w4_r1",
        width: 4,
        rounds: 1,
    },
    CaseSpec {
        slug: "w4_r4",
        width: 4,
        rounds: 4,
    },
    CaseSpec {
        slug: "w4_r16",
        width: 4,
        rounds: 16,
    },
    CaseSpec {
        slug: "w8_r1",
        width: 8,
        rounds: 1,
    },
    CaseSpec {
        slug: "w8_r4",
        width: 8,
        rounds: 4,
    },
    CaseSpec {
        slug: "w8_r16",
        width: 8,
        rounds: 16,
    },
    CaseSpec {
        slug: "w16_r1",
        width: 16,
        rounds: 1,
    },
    CaseSpec {
        slug: "w16_r4",
        width: 16,
        rounds: 4,
    },
    CaseSpec {
        slug: "w16_r16",
        width: 16,
        rounds: 16,
    },
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "storage_roundtrip",
    },
    VariantSpec {
        slug: "score_cached",
    },
    VariantSpec {
        slug: "hot_4_cache",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "scalar_replacement",
    world_seed: 0,
    command_limit: 4_096,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    CASES.iter().copied().map(build_case).collect()
}

fn build_case(spec: CaseSpec) -> Result<Case, LabError> {
    let values = generate_values(spec.width, CASE_SEED);
    let final_values = values
        .into_iter()
        .map(|mut value| {
            for _ in 0..spec.rounds {
                value = update(value);
            }
            value
        })
        .collect::<Vec<_>>();
    let checksum = final_values.into_iter().fold(1_i32, fold);
    let width = i32::try_from(spec.width).expect("scalar-replacement width fits i32");
    let rounds = i32::try_from(spec.rounds).expect("scalar-replacement rounds fit i32");

    Ok(Case {
        slug: spec.slug,
        input: parse(
            spec.slug,
            "input",
            &format!("{{width:{width},rounds:{rounds},seed:{CASE_SEED}}}"),
        )?,
        expected_output: parse(
            spec.slug,
            "expected output",
            &format!("{{checksum:{checksum}}}"),
        )?,
    })
}

fn generate_values(width: usize, seed: i32) -> Vec<i32> {
    let mut state = seed;
    (0..width)
        .map(|_| {
            let value = state;
            state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
            value
        })
        .collect()
}

fn update(value: i32) -> i32 {
    value
        .wrapping_mul(UPDATE_MULTIPLIER)
        .wrapping_add(UPDATE_ADDEND)
}

fn fold(checksum: i32, value: i32) -> i32 {
    checksum.wrapping_mul(UPDATE_MULTIPLIER).wrapping_add(value)
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `scalar_replacement`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
