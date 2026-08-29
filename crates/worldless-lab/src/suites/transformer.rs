use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const VARIANTS: &[VariantSpec] = &[VariantSpec { slug: "text" }, VariantSpec { slug: "tokens" }];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "transformer",
    world_seed: 0,
    command_limit: 65_536,
    variants: VARIANTS,
    build_cases,
};

fn build_cases() -> Result<Vec<Case>, LabError> {
    let invalid_request_output = parse(r#"{ok:0b,error:3}"#, "expected output")?;
    Ok(vec![
        Case {
            slug: "greedy_ab",
            input: parse(
                r#"{prefix:"ab",prefix_tokens:[I;2],max_new_tokens:1}"#,
                "input",
            )?,
            expected_output: parse(
                r#"{
                    ok:1b,
                    generated:[I;0],
                    final_hidden:[I;
                        -67,67,-67,67,-67,67,-67,67,-67,67,-67,67,
                        -36,67,17,-67,-67,67,-67,67,-67,67,-67,67,
                        -67,67,-67,67,8,67,67,-67,-67,67,-67,67,
                        -67,67,-67,67,-67,67,-67,67,-67,67,1,-67,
                        -67,67,-67,67,-67,67,-67,67,-67,67,-67,67,
                        -16,67,48,-67,-67,67,-67,67,-67,67,-67,67,
                        -67,67,-67,67,-67,67,-35,-67,-67,67,-67,67,
                        -67,67,-67,67,-67,67,-67,67,-38,67,15,-67
                    ]
                }"#,
                "expected output",
            )?,
        },
        Case {
            slug: "invalid_prefix_types",
            input: parse(r#"{prefix:0,prefix_tokens:0,max_new_tokens:1}"#, "input")?,
            expected_output: invalid_request_output.clone(),
        },
        Case {
            slug: "invalid_max_new_type",
            input: parse(
                r#"{prefix:"ab",prefix_tokens:[I;2],max_new_tokens:1b}"#,
                "input",
            )?,
            expected_output: invalid_request_output,
        },
    ])
}

fn parse(source: &str, role: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::from_message(format!("suite `transformer`: invalid {role}: {error}"))
    })
}
