use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const REPEAT_COUNT: usize = 63;

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "scoreboard_unrolled",
    },
    VariantSpec {
        slug: "compute_fused",
    },
    VariantSpec {
        slug: "compute_chunked",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "numeric_lowering",
    world_seed: 0,
    command_limit: 32_768,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    [
        ("width_1", 1),
        ("width_4", 4),
        ("width_16", 16),
        ("width_64", 64),
    ]
    .into_iter()
    .map(|(slug, width)| build_case(slug, width))
    .collect()
}

fn build_case(slug: &'static str, width: usize) -> Result<Case, LabError> {
    let (a, b): (Vec<_>, Vec<_>) = (0..width).map(operands).unzip();
    let dot = checked_dot_product(&a, &b);
    let checksum = (0..REPEAT_COUNT).fold(1_i32, |checksum, _| {
        checksum.wrapping_mul(31).wrapping_add(dot)
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

fn operands(index: usize) -> (i32, i32) {
    let index = i32::try_from(index).expect("numeric-lowering case index fits i32");
    let a = index
        .checked_mul(73)
        .expect("numeric-lowering case index product fits i32")
        % 256
        - 128;
    let b = 127
        - index
            .checked_mul(151)
            .expect("numeric-lowering case index product fits i32")
            % 256;
    (a, b)
}

fn checked_dot_product(a: &[i32], b: &[i32]) -> i32 {
    assert_eq!(a.len(), b.len(), "dot-product operand lengths match");
    let mut dot = 0_i64;
    for (&a, &b) in a.iter().zip(b) {
        assert!(
            (-128..=127).contains(&a) && (-128..=127).contains(&b),
            "dot-product operands stay in the signed eight-bit domain"
        );
        let product = i64::from(a) * i64::from(b);
        i32::try_from(product).expect("each dot-product term fits i32");
        dot = dot
            .checked_add(product)
            .expect("generated dot-product sum fits i64");
        i32::try_from(dot).expect("every dot-product prefix fits i32");
    }
    i32::try_from(dot).expect("generated dot product fits i32")
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
            "suite `numeric_lowering`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
