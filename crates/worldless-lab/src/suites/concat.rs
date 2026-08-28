use crate::{Case, LabError, SuiteSpec, VariantSpec};
use worldless::CompoundTag;

const VARIANTS: &[VariantSpec] = &[VariantSpec { slug: "adapted" }];

pub(super) const SPEC: SuiteSpec = SuiteSpec {
    slug: "concat",
    world_seed: 0,
    command_limit: 16_384,
    variants: VARIANTS,
    build_cases,
};

struct Input {
    slug: &'static str,
    first: &'static str,
    second: &'static str,
}

// Twelve tokens, including the leading single quote, exceeded the old sequential composer.
const ELEVEN_DOUBLE_QUOTES: &str = concat!(
    "\"", "\"", "\"", "\"", "\"", "\"", "\"", "\"", "\"", "\"", "\""
);
const ELEVEN_BACKSLASHES: &str = concat!(
    "\\", "\\", "\\", "\\", "\\", "\\", "\\", "\\", "\\", "\\", "\\"
);

const INPUTS: &[Input] = &[
    Input {
        slug: "ordinary_fast_path",
        first: "foo",
        second: "bar",
    },
    Input {
        slug: "single_quote_fallback",
        first: "a'b",
        second: "c",
    },
    Input {
        slug: "escape_boundary_slow_path",
        first: "\\",
        second: "n",
    },
    Input {
        slug: "single_special_slow_path",
        first: "",
        second: "\\",
    },
    Input {
        slug: "adjacent_special_pair",
        first: "\"",
        second: "\\",
    },
    Input {
        slug: "odd_special_tokens",
        first: "'",
        second: "\"\\",
    },
    Input {
        slug: "power_of_two_tokens",
        first: "'",
        second: "\"\\\"",
    },
    Input {
        slug: "dense_quote_tokens",
        first: "'",
        second: ELEVEN_DOUBLE_QUOTES,
    },
    Input {
        slug: "dense_backslash_tokens",
        first: "'",
        second: ELEVEN_BACKSLASHES,
    },
    Input {
        slug: "empty_strings",
        first: "",
        second: "",
    },
    Input {
        slug: "readme_example",
        first: "'hello' \\ ",
        second: "\"world\"",
    },
];

fn build_cases() -> Result<Vec<Case>, LabError> {
    INPUTS
        .iter()
        .map(|input| {
            let first = quote(input.first)?;
            let second = quote(input.second)?;
            let expected = quote(&format!("{}{}", input.first, input.second))?;
            Ok(Case {
                slug: input.slug,
                input: parse(
                    &format!("{{first:{first},second:{second}}}"),
                    input.slug,
                    "input",
                )?,
                expected_output: parse(
                    &format!("{{result:{expected}}}"),
                    input.slug,
                    "expected output",
                )?,
            })
        })
        .collect()
}

fn quote(value: &str) -> Result<String, LabError> {
    serde_json::to_string(value)
        .map_err(|error| LabError::new(format!("failed to quote concat string: {error}")))
}

fn parse(source: &str, case: &str, role: &str) -> Result<CompoundTag, LabError> {
    CompoundTag::from_snbt(source).map_err(|error| {
        LabError::new(format!(
            "suite `concat`, case `{case}`: invalid {role}: {error}"
        ))
    })
}
