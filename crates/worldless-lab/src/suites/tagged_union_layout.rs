use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const CASE_SEED: i32 = -123_456_789;
const STEP_COUNT: usize = 31;
const PAYLOAD_CAPACITY: usize = 2;
const PAYLOAD_WIDTHS: [usize; 4] = [PAYLOAD_CAPACITY, 0, 1, PAYLOAD_CAPACITY];
const LCG_MULTIPLIER: i32 = 1_664_525;
const LCG_ADDEND: i32 = 1_013_904_223;
const CHECKSUM_MULTIPLIER: i32 = 31;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "narrow_compound",
    },
    VariantSpec {
        slug: "wide_compound",
    },
    VariantSpec {
        slug: "narrow_array",
    },
    VariantSpec { slug: "wide_array" },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "tagged_union_layout",
    world_seed: 0,
    command_limit: 8_192,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    [
        ("pair_early", repeated(0)),
        ("none", repeated(1)),
        ("scalar", repeated(2)),
        ("pair_late", repeated(3)),
        (
            "cycle_4",
            (0..STEP_COUNT)
                .map(|index| i32::try_from(index % PAYLOAD_WIDTHS.len()).expect("tag fits i32"))
                .collect(),
        ),
        (
            "clustered_4",
            [vec![0; 8], vec![1; 8], vec![3; 7], vec![2; 8]].concat(),
        ),
    ]
    .into_iter()
    .map(|(slug, tags)| build_case(slug, tags))
    .collect()
}

fn repeated(tag: i32) -> Vec<i32> {
    vec![tag; STEP_COUNT]
}

fn build_case(slug: &'static str, tags: Vec<i32>) -> Result<Case, LabError> {
    let checksum = evaluate(&tags, CASE_SEED);
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!("{{tags:{},seed:{CASE_SEED}}}", int_array(&tags)),
        )?,
        expected_output: parse(slug, "expected output", &format!("{{checksum:{checksum}}}"))?,
    })
}

fn evaluate(tags: &[i32], seed: i32) -> i32 {
    assert_eq!(tags.len(), STEP_COUNT, "tagged-union trace length");
    let mut state = seed;
    let mut checksum = 1_i32;
    for &tag in tags {
        let payload: [i32; PAYLOAD_CAPACITY] =
            std::array::from_fn(|_| emit_and_advance(&mut state));
        let width = usize::try_from(tag)
            .ok()
            .and_then(|tag| PAYLOAD_WIDTHS.get(tag))
            .copied()
            .expect("tagged-union case tag is in range");
        let mut arm = tag.wrapping_add(1);
        for &value in &payload[..width] {
            arm = arm.wrapping_mul(CHECKSUM_MULTIPLIER).wrapping_add(value);
        }
        checksum = checksum.wrapping_mul(CHECKSUM_MULTIPLIER).wrapping_add(arm);
    }
    checksum
}

fn emit_and_advance(state: &mut i32) -> i32 {
    let value = *state;
    *state = state.wrapping_mul(LCG_MULTIPLIER).wrapping_add(LCG_ADDEND);
    value
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
            "suite `tagged_union_layout`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
