use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const RANDOM_UPDATE_COUNT: usize = 63;
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const UPDATE_MULTIPLIER: i32 = 31;
const UPDATE_ADDEND: i32 = 7;

#[derive(Clone, Copy)]
enum Workload {
    Build,
    RandomUpdate,
    Churn,
}

impl Workload {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::RandomUpdate => "random_update",
            Self::Churn => "churn",
        }
    }
}

#[derive(Clone, Copy)]
struct CaseSpec {
    slug: &'static str,
    length: usize,
    workload: Workload,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        slug: "build_0",
        length: 0,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_1",
        length: 1,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_15",
        length: 15,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_16",
        length: 16,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_17",
        length: 17,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_64",
        length: 64,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "build_256",
        length: 256,
        workload: Workload::Build,
    },
    CaseSpec {
        slug: "random_update_1",
        length: 1,
        workload: Workload::RandomUpdate,
    },
    CaseSpec {
        slug: "random_update_16",
        length: 16,
        workload: Workload::RandomUpdate,
    },
    CaseSpec {
        slug: "random_update_17",
        length: 17,
        workload: Workload::RandomUpdate,
    },
    CaseSpec {
        slug: "random_update_64",
        length: 64,
        workload: Workload::RandomUpdate,
    },
    CaseSpec {
        slug: "random_update_256",
        length: 256,
        workload: Workload::RandomUpdate,
    },
    CaseSpec {
        slug: "churn_15",
        length: 15,
        workload: Workload::Churn,
    },
    CaseSpec {
        slug: "churn_16",
        length: 16,
        workload: Workload::Churn,
    },
    CaseSpec {
        slug: "churn_17",
        length: 17,
        workload: Workload::Churn,
    },
    CaseSpec {
        slug: "churn_64",
        length: 64,
        workload: Workload::Churn,
    },
    CaseSpec {
        slug: "churn_256",
        length: 256,
        workload: Workload::Churn,
    },
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "primitive_append",
    },
    VariantSpec {
        slug: "preallocated",
    },
    VariantSpec { slug: "chunked_16" },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "dynamic_vector",
    world_seed: 0,
    command_limit: 32_768,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    CASES.iter().copied().map(build_case).collect()
}

fn build_case(spec: CaseSpec) -> Result<Case, LabError> {
    let length = i32::try_from(spec.length).expect("dynamic-vector case length fits i32");
    let checksum = evaluate(spec.length, CASE_SEED, spec.workload);
    Ok(Case {
        slug: spec.slug,
        input: parse(
            spec.slug,
            "input",
            &format!(
                "{{length:{length},seed:{CASE_SEED},workload:\"{}\"}}",
                spec.workload.as_str()
            ),
        )?,
        expected_output: parse(
            spec.slug,
            "expected output",
            &format!("{{length:{length},checksum:{checksum}}}"),
        )?,
    })
}

fn evaluate(length: usize, seed: i32, workload: Workload) -> i32 {
    let mut state = seed;
    let mut values = Vec::with_capacity(length);
    for _ in 0..length {
        values.push(emit_and_advance(&mut state));
    }

    match workload {
        Workload::Build => {}
        Workload::RandomUpdate => {
            let modulus = i32::try_from(length).expect("random-update length fits i32");
            assert!(modulus > 0, "random-update cases have a positive length");
            for _ in 0..RANDOM_UPDATE_COUNT {
                let index = usize::try_from(state.rem_euclid(modulus))
                    .expect("a non-negative remainder fits usize");
                advance(&mut state);
                values[index] = values[index]
                    .wrapping_mul(UPDATE_MULTIPLIER)
                    .wrapping_add(UPDATE_ADDEND);
            }
        }
        Workload::Churn => {
            let churn_count = length / 2;
            values.truncate(length - churn_count);
            for _ in 0..churn_count {
                values.push(emit_and_advance(&mut state));
            }
        }
    }

    assert_eq!(values.len(), length, "workloads preserve logical length");
    values.into_iter().fold(1_i32, fold)
}

fn emit_and_advance(state: &mut i32) -> i32 {
    let value = *state;
    advance(state);
    value
}

fn advance(state: &mut i32) {
    *state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
}

fn fold(checksum: i32, value: i32) -> i32 {
    checksum.wrapping_mul(UPDATE_MULTIPLIER).wrapping_add(value)
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `dynamic_vector`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
