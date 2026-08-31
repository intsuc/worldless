use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

#[derive(Clone, Copy)]
struct ProfileSpec {
    x: i64,
    y: i64,
    step: i64,
}

const PROFILES: &[ProfileSpec] = &[
    ProfileSpec {
        x: 1_311_768_464_867_721_232,
        y: 1_311_768_469_162_688_496,
        step: 33,
    },
    ProfileSpec {
        x: 9_223_372_036_854_775_800,
        y: 16,
        step: 4_294_967_297,
    },
    ProfileSpec {
        x: -9_223_372_036_854_775_800,
        y: -9_223_372_036_854_775_801,
        step: 16,
    },
    ProfileSpec {
        x: 7_640_891_576_956_012_809,
        y: -4_942_790_177_534_073_029,
        step: 4_354_685_564_936_845_355,
    },
];

#[derive(Clone, Copy)]
struct CaseSpec {
    slug: &'static str,
    profile: usize,
    rounds: usize,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        slug: "low_order_r1",
        profile: 0,
        rounds: 1,
    },
    CaseSpec {
        slug: "low_order_r8",
        profile: 0,
        rounds: 8,
    },
    CaseSpec {
        slug: "low_order_r64",
        profile: 0,
        rounds: 64,
    },
    CaseSpec {
        slug: "add_wrap_r1",
        profile: 1,
        rounds: 1,
    },
    CaseSpec {
        slug: "add_wrap_r8",
        profile: 1,
        rounds: 8,
    },
    CaseSpec {
        slug: "add_wrap_r64",
        profile: 1,
        rounds: 64,
    },
    CaseSpec {
        slug: "sub_wrap_r1",
        profile: 2,
        rounds: 1,
    },
    CaseSpec {
        slug: "sub_wrap_r8",
        profile: 2,
        rounds: 8,
    },
    CaseSpec {
        slug: "sub_wrap_r64",
        profile: 2,
        rounds: 64,
    },
    CaseSpec {
        slug: "mixed_r1",
        profile: 3,
        rounds: 1,
    },
    CaseSpec {
        slug: "mixed_r8",
        profile: 3,
        rounds: 8,
    },
    CaseSpec {
        slug: "mixed_r64",
        profile: 3,
        rounds: 64,
    },
];

const VARIANTS: &[VariantSpec] = &[
    VariantSpec {
        slug: "two_i32_halves",
    },
    VariantSpec {
        slug: "four_u16_limbs",
    },
];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "i64_lowering",
    world_seed: 0,
    command_limit: 4_096,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    CASES.iter().copied().map(build_case).collect()
}

fn build_case(spec: CaseSpec) -> Result<Case, LabError> {
    let profile = PROFILES[spec.profile];
    let mut x = profile.x;
    let mut y = profile.y;
    let mut less_count = 0_i32;

    for _ in 0..spec.rounds {
        if x < y {
            less_count += 1;
        }
        x = x.wrapping_add(y);
        y = y.wrapping_sub(profile.step);
    }

    let rounds = i32::try_from(spec.rounds).expect("i64-lowering rounds fit i32");
    let [x_high, x_low] = words(x);
    let [y_high, y_low] = words(y);

    Ok(Case {
        slug: spec.slug,
        input: parse(
            spec.slug,
            "input",
            &format!(
                "{{x:{}L,y:{}L,step:{}L,rounds:{rounds}}}",
                profile.x, profile.y, profile.step
            ),
        )?,
        expected_output: parse(
            spec.slug,
            "expected output",
            &format!("{{x:[I;{x_high},{x_low}],y:[I;{y_high},{y_low}],less_count:{less_count}}}"),
        )?,
    })
}

fn words(value: i64) -> [i32; 2] {
    [(value >> 32) as i32, value as i32]
}

fn parse(slug: &str, role: &str, source: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!(
            "suite `i64_lowering`, case `{slug}`: invalid {role}: {error}"
        ))
    })
}
