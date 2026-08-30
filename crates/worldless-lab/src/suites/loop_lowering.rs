use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const CHECKSUM_MULTIPLIER: i32 = 31;

#[derive(Clone, Copy)]
struct CaseSpec {
    slug: &'static str,
    iterations: usize,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        slug: "iterations_0",
        iterations: 0,
    },
    CaseSpec {
        slug: "iterations_1",
        iterations: 1,
    },
    CaseSpec {
        slug: "iterations_3",
        iterations: 3,
    },
    CaseSpec {
        slug: "iterations_4",
        iterations: 4,
    },
    CaseSpec {
        slug: "iterations_5",
        iterations: 5,
    },
    CaseSpec {
        slug: "iterations_15",
        iterations: 15,
    },
    CaseSpec {
        slug: "iterations_16",
        iterations: 16,
    },
    CaseSpec {
        slug: "iterations_17",
        iterations: 17,
    },
    CaseSpec {
        slug: "iterations_64",
        iterations: 64,
    },
    CaseSpec {
        slug: "iterations_256",
        iterations: 256,
    },
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "recursive_call",
    },
    VariantSpec { slug: "return_run" },
    VariantSpec { slug: "unroll_4" },
    VariantSpec { slug: "unroll_16" },
    VariantSpec {
        slug: "full_unroll",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "loop_lowering",
    world_seed: 0,
    command_limit: 32_768,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    CASES.iter().copied().map(build_case).collect()
}

fn build_case(spec: CaseSpec) -> Result<Case, LabError> {
    let iterations = i32::try_from(spec.iterations).expect("loop-lowering iterations fit i32");
    let (value, checksum) = evaluate(spec.iterations, CASE_SEED);
    Ok(Case {
        slug: spec.slug,
        input: parse(
            spec.slug,
            "input",
            &format!("{{iterations:{iterations},seed:{CASE_SEED}}}"),
        )?,
        expected_output: parse(
            spec.slug,
            "expected output",
            &format!("{{iterations:{iterations},value:{value},checksum:{checksum}}}"),
        )?,
    })
}

fn evaluate(iterations: usize, seed: i32) -> (i32, i32) {
    let mut value = seed;
    let mut checksum = 1_i32;
    for _ in 0..iterations {
        value = value.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
        checksum = checksum
            .wrapping_mul(CHECKSUM_MULTIPLIER)
            .wrapping_add(value);
    }
    (value, checksum)
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `loop_lowering`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
