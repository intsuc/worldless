use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const DEPTH: i32 = 8;
const SEEDS: &[i32] = &[
    i32::MIN,
    0,
    i32::MAX,
    -1,
    1,
    -123_456_789,
    123_456_789,
    i32::MIN + 7,
    i32::MAX - 8,
    -2_000_000_000,
    2_000_000_000,
    -31,
    31,
    -17,
    17,
];
const WIDTHS: &[(&str, usize)] = &[
    ("width_1", 1),
    ("width_2", 2),
    ("width_4", 4),
    ("width_8", 8),
    ("width_16", 16),
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "static_scores",
    },
    VariantSpec { slug: "word_stack" },
    VariantSpec {
        slug: "compound_stack",
    },
    VariantSpec {
        slug: "hot_4_spill",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "register_pressure",
    world_seed: 0,
    command_limit: 32_768,
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
    let checksum = SEEDS.iter().copied().fold(1_i32, |checksum, seed| {
        checksum
            .wrapping_mul(31)
            .wrapping_add(evaluate(DEPTH, seed, width))
    });
    let width = i32::try_from(width).expect("register-pressure width fits i32");
    Ok(Case {
        slug,
        input: parse(
            slug,
            "input",
            &format!("{{width:{width},seeds:{}}}", int_array(SEEDS)),
        )?,
        expected_output: parse(
            slug,
            "expected output",
            &format!("{{width:{width},checksum:{checksum}}}"),
        )?,
    })
}

fn evaluate(depth: i32, value: i32, width: usize) -> i32 {
    let mut locals = [0_i32; 16];
    for (index, local) in locals[..width].iter_mut().enumerate() {
        let offset = i32::try_from(index + 1).expect("local index fits i32");
        *local = value
            .wrapping_mul(31)
            .wrapping_add(depth)
            .wrapping_add(offset);
    }

    let child = match depth {
        1 => value.wrapping_mul(17).wrapping_add(7),
        2.. => evaluate(depth - 1, value.wrapping_add(locals[0]), width),
        _ => unreachable!("register-pressure depth is positive"),
    };
    locals[..width].iter().copied().fold(child, |child, local| {
        child.wrapping_mul(31).wrapping_add(local)
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
            "suite `register_pressure`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
