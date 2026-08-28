mod common;

use common::context;
use worldless::{
    CommandFeedback, ExecutionOutcome, LoadError, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 256;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn compile(functions: &[(&str, &str)]) -> Vm {
    load_functions(functions.iter().copied()).unwrap()
}

fn load_functions<I, N, S>(functions: I) -> Result<Vm, LoadError>
where
    I: IntoIterator<Item = (N, S)>,
    N: AsRef<str>,
    S: AsRef<str>,
{
    Vm::from_packs(
        [Pack::memory(functions.into_iter().map(|(id, source)| {
            MemoryResource::new(ResourceKind::Function, id.as_ref(), source.as_ref())
        }))],
        0,
    )
}

#[test]
fn objective_lifecycle_reports_post_mutation_counts() {
    let mut vm = compile(&[
        ("example:list", "return run scoreboard objectives list\n"),
        (
            "example:add_first",
            "return run scoreboard objectives add first dummy\n",
        ),
        (
            "example:add_second",
            "return run scoreboard objectives add second dummy\n",
        ),
        (
            "example:add_duplicate",
            "return run scoreboard objectives add first dummy\n",
        ),
        (
            "example:remove_missing",
            "return run scoreboard objectives remove missing\n",
        ),
        (
            "example:remove_first",
            "return run scoreboard objectives remove first\n",
        ),
        (
            "example:remove_second",
            "return run scoreboard objectives remove second\n",
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:list", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:add_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:add_second", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:add_duplicate", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:list", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:remove_missing", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:list", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:remove_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:remove_second", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:list", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn reset_and_list_manage_score_and_holder_lifetimes() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add first dummy\nscoreboard objectives add second dummy\nscoreboard players set #one first 4\nscoreboard players set #one second 5\nscoreboard players set #two first 0\n",
        ),
        ("example:list_all", "return run scoreboard players list\n"),
        (
            "example:list_one",
            "return run scoreboard players list #one\n",
        ),
        (
            "example:list_absent",
            "return run scoreboard players list #absent\n",
        ),
        (
            "example:reset_one_first",
            "return run scoreboard players reset #one first\n",
        ),
        (
            "example:reset_one_second",
            "return run scoreboard players reset #one second\n",
        ),
        (
            "example:reset_two",
            "return run scoreboard players reset #two\n",
        ),
        (
            "example:reset_absent",
            "return run scoreboard players reset #absent\n",
        ),
        (
            "example:reset_unknown_objective",
            "return run scoreboard players reset #absent missing\n",
        ),
        (
            "example:get_one_first",
            "return run scoreboard players get #one first\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:list_one", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:list_absent", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2),
        "listing an absent holder must not start tracking it"
    );

    assert_eq!(
        vm.execute_function("example:reset_one_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:get_one_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "reset deletes a score instead of setting it to zero"
    );
    assert_eq!(
        vm.execute_function("example:list_one", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:reset_one_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1),
        "reset reports addressed holders even when no score was removed"
    );
    assert_eq!(
        vm.execute_function("example:list_one", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );

    assert_eq!(
        vm.execute_function("example:reset_one_second", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1),
        "removing a holder's last score stops tracking that holder"
    );
    assert_eq!(
        vm.execute_function(
            "example:reset_unknown_objective",
            None,
            context(),
            LIMIT,
            drop
        )
        .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:reset_two", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:reset_absent", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn removing_objectives_clears_scores_but_retains_tracked_holders() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add first dummy\nscoreboard objectives add second dummy\nscoreboard players set #only first 7\nscoreboard players set #both first 1\nscoreboard players set #both second 2\n",
        ),
        (
            "example:remove_first",
            "return run scoreboard objectives remove first\n",
        ),
        (
            "example:remove_second",
            "return run scoreboard objectives remove second\n",
        ),
        (
            "example:readd_first",
            "return run scoreboard objectives add first dummy\n",
        ),
        ("example:list_all", "return run scoreboard players list\n"),
        (
            "example:list_only",
            "return run scoreboard players list #only\n",
        ),
        (
            "example:list_both",
            "return run scoreboard players list #both\n",
        ),
        (
            "example:get_only_first",
            "return run scoreboard players get #only first\n",
        ),
        (
            "example:get_both_second",
            "return run scoreboard players get #both second\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:remove_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:list_only", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:list_both", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:get_only_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:get_both_second", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );

    assert_eq!(
        vm.execute_function("example:remove_second", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:list_both", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:readd_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:get_only_first", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "re-adding an objective must not revive scores from its old identity"
    );
}

#[test]
fn wildcard_score_changes_include_empty_tracked_holders_and_sum_final_values() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add old dummy\nscoreboard players set #one old 1\nscoreboard players set #two old 2\nscoreboard objectives remove old\nscoreboard objectives add current dummy\n",
        ),
        (
            "example:set",
            "return run scoreboard players set * current 0\n",
        ),
        (
            "example:add",
            "return run scoreboard players add * current 2\n",
        ),
        (
            "example:remove",
            "return run scoreboard players remove * current 2\n",
        ),
        (
            "example:get_one",
            "return run scoreboard players get #one current\n",
        ),
        (
            "example:get_two",
            "return run scoreboard players get #two current\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:set", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0),
        "the wildcard includes holders retained empty by objective removal"
    );
    assert_eq!(
        vm.execute_function("example:add", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 4)
    );
    assert_eq!(
        vm.execute_function("example:remove", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0),
        "a zero result from nonempty wildcard targets is still successful"
    );
    for function in ["example:get_one", "example:get_two"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(true, 0),
            "{function}"
        );
    }
}

#[test]
fn wildcard_operations_use_the_current_tracked_holder_snapshot() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard objectives add terms dummy\nscoreboard players set #target values 10\nscoreboard players set #one terms 2\nscoreboard players set #two terms 3\n",
        ),
        (
            "example:sum_sources",
            "return run scoreboard players operation #target values += * terms\n",
        ),
        (
            "example:update_targets",
            "return run scoreboard players operation * values += #two terms\n",
        ),
        (
            "example:get_target",
            "return run scoreboard players get #target values\n",
        ),
        (
            "example:get_one",
            "return run scoreboard players get #one values\n",
        ),
        (
            "example:get_two",
            "return run scoreboard players get #two values\n",
        ),
        (
            "example:reset_values",
            "return run scoreboard players reset * values\n",
        ),
        (
            "example:reset_all",
            "return run scoreboard players reset *\n",
        ),
        ("example:list_all", "return run scoreboard players list\n"),
        (
            "example:get_wildcard",
            "return run scoreboard players get * values\n",
        ),
        (
            "example:list_wildcard",
            "return run scoreboard players list *\n",
        ),
        (
            "example:if_wildcard",
            "return run execute if score * values matches 0\n",
        ),
        (
            "example:unless_wildcard",
            "return run execute unless score * values matches 0\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:sum_sources", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 15)
    );
    assert_eq!(
        vm.execute_function("example:update_targets", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 24)
    );
    for (function, value) in [
        ("example:get_target", 18),
        ("example:get_one", 3),
        ("example:get_two", 3),
    ] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(true, value),
            "{function}"
        );
    }

    assert_eq!(
        vm.execute_function("example:get_wildcard", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "single-holder arguments do not resolve the wildcard"
    );
    assert_eq!(
        vm.execute_function("example:list_wildcard", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "player-list's target argument does not resolve the wildcard"
    );
    for function in ["example:if_wildcard", "example:unless_wildcard"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(false, 0),
            "a singular score condition cannot resolve the wildcard"
        );
    }
    assert_eq!(
        vm.execute_function("example:reset_values", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 3)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 3),
        "the source operation created `terms` for every wildcard source"
    );
    assert_eq!(
        vm.execute_function("example:reset_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 3)
    );
    assert_eq!(
        vm.execute_function("example:list_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:reset_all", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "an empty wildcard is a command failure"
    );
}

#[test]
fn store_callbacks_keep_the_removed_objective_identity() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add doomed dummy\nscoreboard players set #sink doomed 7\n",
        ),
        (
            "example:remove_with_store",
            "return run execute store result score #sink doomed run scoreboard objectives remove doomed\n",
        ),
        (
            "example:list_objectives",
            "return run scoreboard objectives list\n",
        ),
        (
            "example:list_holders",
            "return run scoreboard players list\n",
        ),
        (
            "example:list_sink",
            "return run scoreboard players list #sink\n",
        ),
        (
            "example:readd_and_get",
            "scoreboard objectives add doomed dummy\nreturn run scoreboard players get #sink doomed\n",
        ),
        (
            "example:set_current",
            "return run scoreboard players set #sink doomed 4\n",
        ),
        (
            "example:remove_current",
            "return run scoreboard objectives remove doomed\n",
        ),
        (
            "example:reset_sink",
            "return run scoreboard players reset #sink\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:remove_with_store", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0),
        "removing the final objective succeeds with result zero"
    );
    assert_eq!(
        vm.execute_function("example:list_objectives", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0),
        "the callback must not recreate the removed named objective"
    );
    assert_eq!(
        vm.execute_function("example:list_holders", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:list_sink", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1),
        "Minecraft stores the result against the detached objective identity"
    );
    assert_eq!(
        vm.execute_function("example:readd_and_get", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0),
        "a new objective with the same name has a distinct identity"
    );
    assert_eq!(
        vm.execute_function("example:set_current", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 4)
    );
    let mut feedback = Vec::new();
    assert_eq!(
        vm.execute_command("scoreboard players list #sink", context(), LIMIT, |event| {
            feedback.push(event)
        },)
            .unwrap(),
        returned(true, 2),
        "the detached and current objective scores coexist"
    );
    assert_eq!(
        feedback
            .into_iter()
            .map(|event| match event {
                CommandFeedback::Success(text) => text.to_string_lossy(),
                CommandFeedback::Failure(text) => {
                    panic!("unexpected failure feedback: {text}")
                }
            })
            .collect::<Vec<_>>(),
        ["#sink has 2 score(s):", "[doomed]: 0", "[doomed]: 4",],
        "same-named detached identities use their creation order as a stable tie-breaker"
    );
    assert_eq!(
        vm.execute_function("example:remove_current", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:list_sink", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1),
        "removing the current objective does not remove the detached score"
    );
    assert_eq!(
        vm.execute_function("example:reset_sink", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:list_holders", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0),
        "reset all removes detached scores and the tracked holder"
    );
}

#[test]
fn wildcard_store_uses_a_pre_execution_holder_snapshot() {
    let mut vm = compile(&[
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:store_empty",
            "return run execute store result score * values run scoreboard players set #new values 9\n",
        ),
        ("example:list", "return run scoreboard players list\n"),
        (
            "example:track",
            "scoreboard players set #one values 4\nscoreboard players set #two values 5\n",
        ),
        (
            "example:store_across_reset",
            "return run execute store result score * values run scoreboard players reset *\n",
        ),
        (
            "example:store_before_new_holder",
            "return run execute store success score * values run scoreboard players set #new values 7\n",
        ),
        (
            "example:get_one",
            "return run scoreboard players get #one values\n",
        ),
        (
            "example:get_two",
            "return run scoreboard players get #two values\n",
        ),
        (
            "example:get_new",
            "return run scoreboard players get #new values\n",
        ),
    ]);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:store_empty", None, context(), LIMIT, drop)
            .unwrap(),
        ExecutionOutcome::NoResult,
        "an empty wildcard stops before executing the stored command"
    );
    assert_eq!(
        vm.execute_function("example:list", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:get_new", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0)
    );

    vm.execute_function("example:track", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:store_across_reset", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 2)
    );
    for function in ["example:get_one", "example:get_two"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(true, 2),
            "the callback recreates every holder captured before reset"
        );
    }

    assert_eq!(
        vm.execute_function(
            "example:store_before_new_holder",
            None,
            context(),
            LIMIT,
            drop
        )
        .unwrap(),
        returned(true, 7)
    );
    for function in ["example:get_one", "example:get_two"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(true, 1),
            "store success updates the captured holders"
        );
    }
    assert_eq!(
        vm.execute_function("example:get_new", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 7),
        "a holder created downstream is not part of the store snapshot"
    );
}

#[test]
fn scoreboard_state_commands_reject_malformed_and_physical_holder_forms() {
    for command in [
        "scoreboard objectives remove",
        "scoreboard objectives list extra",
        "scoreboard players list #holder extra",
        "scoreboard players reset",
        "scoreboard players reset Player",
        "scoreboard players reset @s",
        "scoreboard players reset 00000000-0000-0000-0000-000000000000",
    ] {
        assert!(
            matches!(
                load_functions([("example:invalid", command)]),
                Err(LoadError::InvalidFunction { .. })
            ),
            "{command:?}"
        );
    }

    assert!(
        load_functions([
            (
                "example:remove_unknown",
                "return run scoreboard objectives remove unknown\n",
            ),
            (
                "example:reset_unknown",
                "return run scoreboard players reset #holder unknown\n",
            ),
        ])
        .is_ok()
    );
}
