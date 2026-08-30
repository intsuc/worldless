use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const FIELD_COUNT: usize = 3;
const UPDATE_ROUNDS: usize = 7;
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const AFFINE_MULTIPLIER: i32 = 31;
const AFFINE_ADDEND: i32 = 7;

#[derive(Clone, Copy)]
enum TraversalOrder {
    RecordMajor,
    FieldMajor,
}

impl TraversalOrder {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RecordMajor => "record_major",
            Self::FieldMajor => "field_major",
        }
    }

    const fn other(self) -> Self {
        match self {
            Self::RecordMajor => Self::FieldMajor,
            Self::FieldMajor => Self::RecordMajor,
        }
    }
}

#[derive(Clone, Copy)]
struct CaseSpec {
    slug: &'static str,
    length: usize,
    order: TraversalOrder,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        slug: "record_major_1",
        length: 1,
        order: TraversalOrder::RecordMajor,
    },
    CaseSpec {
        slug: "record_major_16",
        length: 16,
        order: TraversalOrder::RecordMajor,
    },
    CaseSpec {
        slug: "record_major_64",
        length: 64,
        order: TraversalOrder::RecordMajor,
    },
    CaseSpec {
        slug: "record_major_128",
        length: 128,
        order: TraversalOrder::RecordMajor,
    },
    CaseSpec {
        slug: "field_major_1",
        length: 1,
        order: TraversalOrder::FieldMajor,
    },
    CaseSpec {
        slug: "field_major_16",
        length: 16,
        order: TraversalOrder::FieldMajor,
    },
    CaseSpec {
        slug: "field_major_64",
        length: 64,
        order: TraversalOrder::FieldMajor,
    },
    CaseSpec {
        slug: "field_major_128",
        length: 128,
        order: TraversalOrder::FieldMajor,
    },
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "record_compounds",
    },
    VariantSpec {
        slug: "column_arrays",
    },
    VariantSpec { slug: "flat_array" },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "aggregate_layout",
    world_seed: 0,
    command_limit: 32_768,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    CASES.iter().copied().map(build_case).collect()
}

fn build_case(spec: CaseSpec) -> Result<Case, LabError> {
    let records = generate_records(spec.length, CASE_SEED);
    let checksum = evaluate(records.clone(), spec.order);
    let other_checksum = evaluate(records, spec.order.other());
    assert_eq!(
        checksum, other_checksum,
        "independent scalar updates must not depend on traversal order"
    );
    let length = i32::try_from(spec.length).expect("aggregate-layout case length fits i32");

    Ok(Case {
        slug: spec.slug,
        input: parse(
            spec.slug,
            "input",
            &format!(
                "{{length:{length},seed:{CASE_SEED},order:\"{}\"}}",
                spec.order.as_str()
            ),
        )?,
        expected_output: parse(
            spec.slug,
            "expected output",
            &format!("{{checksum:{checksum}}}"),
        )?,
    })
}

fn generate_records(length: usize, seed: i32) -> Vec<[i32; FIELD_COUNT]> {
    let mut state = seed;
    (0..length)
        .map(|_| std::array::from_fn(|_| emit_and_advance(&mut state)))
        .collect()
}

fn emit_and_advance(state: &mut i32) -> i32 {
    let value = *state;
    *state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
    value
}

fn evaluate(mut records: Vec<[i32; FIELD_COUNT]>, order: TraversalOrder) -> i32 {
    for _ in 0..UPDATE_ROUNDS {
        match order {
            TraversalOrder::RecordMajor => {
                for record in &mut records {
                    for value in record {
                        update(value);
                    }
                }
            }
            TraversalOrder::FieldMajor => {
                for field in 0..FIELD_COUNT {
                    for record in &mut records {
                        update(&mut record[field]);
                    }
                }
            }
        }
    }

    records.iter().flatten().copied().fold(1_i32, fold)
}

fn update(value: &mut i32) {
    *value = value
        .wrapping_mul(AFFINE_MULTIPLIER)
        .wrapping_add(AFFINE_ADDEND);
}

fn fold(checksum: i32, value: i32) -> i32 {
    checksum.wrapping_mul(AFFINE_MULTIPLIER).wrapping_add(value)
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `aggregate_layout`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
