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

#[cfg(test)]
mod tests {
    use worldless::{CompiledProgram, ExecutionError, ExecutionOutcome, Pack, Vm};

    const FRESH_ACTIVATION_QUOTA_USED: usize = 5_111;
    const REACTIVATION_QUOTA_USED: usize = FRESH_ACTIVATION_QUOTA_USED + 1;
    const CLEAN_REACTIVATION_LIMIT: usize = REACTIVATION_QUOTA_USED + 1;

    fn set_active_bank(vm: &mut Vm, bank: i32) {
        let report = vm.execute_command(
            &format!("data modify storage transformer:runtime active_bank set value {bank}"),
            crate::context(),
            8,
            drop,
        );
        assert_eq!(
            report.into_result().unwrap(),
            ExecutionOutcome::Result {
                success: true,
                value: 1,
            }
        );
    }

    fn active_bank(vm: &mut Vm) -> i32 {
        match vm
            .execute_command(
                "data get storage transformer:runtime active_bank",
                crate::context(),
                8,
                drop,
            )
            .into_result()
            .unwrap()
        {
            ExecutionOutcome::Result {
                success: true,
                value,
            } => value,
            outcome => panic!("unexpected active-bank read outcome: {outcome:?}"),
        }
    }

    #[test]
    fn activation_commit_has_explicit_command_limit_boundaries() {
        let program =
            CompiledProgram::from_packs([Pack::directory(crate::pack_path("transformer"))])
                .unwrap();
        let mut vm = program.create_vm(0);
        for function in ["transformer:setup", "transformer:fixture/load_model"] {
            vm.execute_function(function, None, crate::context(), 65_536, drop)
                .into_result()
                .unwrap();
        }

        let fresh = vm.execute_function(
            "transformer:model/activate",
            None,
            crate::context(),
            65_536,
            drop,
        );
        assert_eq!(fresh.quota_used(), FRESH_ACTIVATION_QUOTA_USED);
        assert_eq!(
            fresh.into_result().unwrap(),
            ExecutionOutcome::Result {
                success: true,
                value: 1,
            }
        );
        assert_eq!(active_bank(&mut vm), 0);

        let reactivation = vm.execute_function(
            "transformer:model/activate",
            None,
            crate::context(),
            65_536,
            drop,
        );
        assert_eq!(reactivation.quota_used(), REACTIVATION_QUOTA_USED);
        assert_eq!(
            reactivation.into_result().unwrap(),
            ExecutionOutcome::Result {
                success: true,
                value: 1,
            }
        );
        assert_eq!(active_bank(&mut vm), 1);

        set_active_bank(&mut vm, 0);
        let before_commit = vm.execute_function(
            "transformer:model/activate",
            None,
            crate::context(),
            FRESH_ACTIVATION_QUOTA_USED,
            drop,
        );
        assert_eq!(before_commit.quota_used(), FRESH_ACTIVATION_QUOTA_USED);
        assert_eq!(
            before_commit.into_result(),
            Err(ExecutionError::CommandLimitExceeded {
                limit: FRESH_ACTIVATION_QUOTA_USED,
            })
        );
        assert_eq!(active_bank(&mut vm), 0);

        let after_commit = vm.execute_function(
            "transformer:model/activate",
            None,
            crate::context(),
            REACTIVATION_QUOTA_USED,
            drop,
        );
        assert_eq!(after_commit.quota_used(), REACTIVATION_QUOTA_USED);
        assert_eq!(
            after_commit.into_result(),
            Err(ExecutionError::CommandLimitExceeded {
                limit: REACTIVATION_QUOTA_USED,
            })
        );
        assert_eq!(active_bank(&mut vm), 1);

        set_active_bank(&mut vm, 0);
        let clean = vm.execute_function(
            "transformer:model/activate",
            None,
            crate::context(),
            CLEAN_REACTIVATION_LIMIT,
            drop,
        );
        assert_eq!(clean.quota_used(), REACTIVATION_QUOTA_USED);
        assert_eq!(
            clean.into_result().unwrap(),
            ExecutionOutcome::Result {
                success: true,
                value: 1,
            }
        );
        assert_eq!(active_bank(&mut vm), 1);
    }
}
