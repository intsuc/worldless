use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const SEED_COUNT: usize = 31;
const DEPTHS: &[(&str, i32)] = &[
    ("depth_1", 1),
    ("depth_2", 2),
    ("depth_4", 4),
    ("depth_8", 8),
    ("depth_16", 16),
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "static_scores",
    },
    VariantSpec { slug: "word_stack" },
    VariantSpec {
        slug: "compound_stack",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "call_frames",
    world_seed: 0,
    command_limit: 32_768,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    let seeds = seeds();
    DEPTHS
        .iter()
        .map(|&(slug, depth)| build_case(slug, depth, &seeds))
        .collect()
}

fn seeds() -> Vec<i32> {
    (0..SEED_COUNT)
        .map(|index| {
            let index = i32::try_from(index).expect("seed index fits i32");
            if index % 2 == 0 {
                i32::MIN
                    .checked_add(index)
                    .expect("even seed index keeps the value in range")
            } else {
                i32::MAX
                    .checked_sub(index)
                    .expect("odd seed index keeps the value in range")
            }
        })
        .collect()
}

fn build_case(slug: &'static str, depth: i32, seeds: &[i32]) -> Result<Case, LabError> {
    let checksum = seeds.iter().copied().fold(1_i32, |checksum, seed| {
        checksum
            .wrapping_mul(31)
            .wrapping_add(evaluate(depth, seed))
    });
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!("{{depth:{depth},seeds:{}}}", int_array(seeds)),
        )?,
        expected_output: parse(slug, "expected output", &format!("{{checksum:{checksum}}}"))?,
    })
}

fn evaluate(depth: i32, value: i32) -> i32 {
    let left = value.wrapping_mul(17).wrapping_add(depth);
    let right = value.wrapping_mul(31).wrapping_sub(depth);
    match depth {
        1 => left.wrapping_mul(31).wrapping_add(right).wrapping_add(7),
        2.. => {
            let child = evaluate(depth - 1, value.wrapping_add(left));
            child
                .wrapping_mul(31)
                .wrapping_add(if child < 0 { left } else { right })
        }
        _ => unreachable!("case depths are positive"),
    }
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
            "suite `call_frames`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
