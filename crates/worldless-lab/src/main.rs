use std::{
    ffi::{OsStr, OsString},
    fmt,
    io::{self, Write},
    process::ExitCode,
};

use worldless_lab::{CheckReport, ComparisonReport};

const USAGE: &str = "usage: worldless-lab check [--suite <NAME>] --format <text|json>\n       worldless-lab compare --suite <NAME> --samples <NONZERO_USIZE> --format <text|json>";
const EXIT_USAGE: u8 = 2;
const EXIT_LAB: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Eq, PartialEq)]
struct CommonOptions {
    suite: Option<String>,
    format: OutputFormat,
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Check(CommonOptions),
    Compare {
        common: CommonOptions,
        samples: usize,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn main() -> ExitCode {
    let command = match parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("error: {error}\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let result = match command {
        Command::Check(options) => worldless_lab::check(options.suite.as_deref())
            .and_then(|report| write_check(report, options.format).map_err(output_error)),
        Command::Compare { common, samples } => {
            if cfg!(debug_assertions) {
                Err(worldless_lab::LabError::from_message(
                    "timing comparison requires a release build; run `cargo run --release -p worldless-lab -- compare ...`",
                ))
            } else {
                let suite = common
                    .suite
                    .as_deref()
                    .expect("compare parsing requires --suite");
                worldless_lab::compare(suite, samples).and_then(|report| {
                    write_comparison(report, common.format).map_err(output_error)
                })
            }
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(EXIT_LAB)
        }
    }
}

fn output_error(error: io::Error) -> worldless_lab::LabError {
    worldless_lab::LabError::from_message(format!("failed to write stdout: {error}"))
}

fn write_check(report: CheckReport, format: OutputFormat) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match format {
        OutputFormat::Text => {
            writeln!(
                output,
                "execution vm_state={} macro_cache={}",
                report.execution.vm_state, report.execution.macro_cache
            )?;
            for suite in report.suites {
                writeln!(
                    output,
                    "ok suite={} cases={} variants={} invocations={}",
                    suite.suite, suite.case_count, suite.variant_count, suite.invocation_count
                )?;
            }
        }
        OutputFormat::Json => {
            write_json(&mut output, &report)?;
        }
    }
    output.flush()
}

fn write_comparison(report: ComparisonReport, format: OutputFormat) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match format {
        OutputFormat::Text => {
            writeln!(
                output,
                "execution vm_state={} macro_cache={}",
                report.execution.vm_state, report.execution.macro_cache
            )?;
            writeln!(
                output,
                "suite case variant quota_limit quota_used median_ns min_ns max_ns samples"
            )?;
            for row in report.rows {
                let mut sorted = row.timing.durations_ns.clone();
                sorted.sort_unstable();
                writeln!(
                    output,
                    "{} {} {} {} {} {} {} {} {}",
                    row.suite,
                    row.case,
                    row.variant,
                    row.quota.limit,
                    row.quota.used,
                    sorted[sorted.len() / 2],
                    sorted[0],
                    sorted[sorted.len() - 1],
                    sorted.len()
                )?;
            }
        }
        OutputFormat::Json => {
            write_json(&mut output, &report)?;
        }
    }
    output.flush()
}

fn write_json(output: &mut impl Write, value: &impl serde::Serialize) -> io::Result<()> {
    let mut encoded = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    encoded.push(b'\n');
    output.write_all(&encoded)
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut arguments = arguments.into_iter();
    let command = arguments
        .next()
        .ok_or_else(|| usage_error("missing command"))?;
    if command == "check" {
        let (common, samples) = parse_options(arguments, false)?;
        debug_assert!(samples.is_none());
        Ok(Command::Check(common))
    } else if command == "compare" {
        let (common, samples) = parse_options(arguments, true)?;
        Ok(Command::Compare {
            common,
            samples: samples.expect("compare requires --samples"),
        })
    } else {
        Err(usage_error(format!("unknown command {command:?}")))
    }
}

fn parse_options(
    mut arguments: impl Iterator<Item = OsString>,
    allow_samples: bool,
) -> Result<(CommonOptions, Option<usize>), UsageError> {
    let mut suite = None;
    let mut format = None;
    let mut samples = None;
    while let Some(option) = arguments.next() {
        if option == "--suite" {
            if suite.is_some() {
                return Err(usage_error("duplicate --suite"));
            }
            suite = Some(parse_utf8("--suite", arguments.next())?);
        } else if option == "--format" {
            if format.is_some() {
                return Err(usage_error("duplicate --format"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage_error("missing value for --format"))?;
            format = Some(parse_format(&value)?);
        } else if option == "--samples" && allow_samples {
            if samples.is_some() {
                return Err(usage_error("duplicate --samples"));
            }
            samples = Some(parse_samples(arguments.next())?);
        } else {
            return Err(usage_error(format!("unexpected argument {option:?}")));
        }
    }
    let format = format.ok_or_else(|| usage_error("missing required --format"))?;
    if allow_samples && suite.is_none() {
        return Err(usage_error("missing required --suite"));
    }
    if allow_samples && samples.is_none() {
        return Err(usage_error("missing required --samples"));
    }
    Ok((CommonOptions { suite, format }, samples))
}

fn parse_utf8(option: &str, value: Option<OsString>) -> Result<String, UsageError> {
    let value = value.ok_or_else(|| usage_error(format!("missing value for {option}")))?;
    value
        .into_string()
        .map_err(|value| usage_error(format!("value for {option} is not UTF-8: {value:?}")))
}

fn parse_format(value: &OsStr) -> Result<OutputFormat, UsageError> {
    if value == "text" {
        Ok(OutputFormat::Text)
    } else if value == "json" {
        Ok(OutputFormat::Json)
    } else {
        Err(usage_error(format!(
            "invalid --format {value:?}; expected text or json"
        )))
    }
}

fn parse_samples(value: Option<OsString>) -> Result<usize, UsageError> {
    let value = parse_utf8("--samples", value)?;
    match value.parse::<usize>() {
        Ok(0) => Err(usage_error("--samples must be greater than zero")),
        Ok(samples) => Ok(samples),
        Err(_) => Err(usage_error(format!(
            "invalid --samples {value:?}; expected a positive integer"
        ))),
    }
}

fn usage_error(message: impl Into<String>) -> UsageError {
    UsageError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args<'a>(values: &'a [&'a str]) -> impl Iterator<Item = OsString> + 'a {
        values.iter().map(OsString::from)
    }

    #[test]
    fn parses_check_and_compare_options_in_any_order() {
        assert_eq!(
            parse_args(args(&["check", "--format", "json", "--suite", "concat"])),
            Ok(Command::Check(CommonOptions {
                suite: Some("concat".to_owned()),
                format: OutputFormat::Json,
            }))
        );
        assert_eq!(
            parse_args(args(&[
                "compare",
                "--samples",
                "7",
                "--suite",
                "concat",
                "--format",
                "text",
            ])),
            Ok(Command::Compare {
                common: CommonOptions {
                    suite: Some("concat".to_owned()),
                    format: OutputFormat::Text,
                },
                samples: 7,
            })
        );
    }

    #[test]
    fn rejects_duplicate_unknown_and_misplaced_options() {
        for values in [
            &["check", "--suite", "concat", "--suite", "concat"][..],
            &["check", "--samples", "1"][..],
            &["compare", "--unknown"][..],
            &["check"][..],
            &["compare", "--suite", "concat", "--format", "text"][..],
            &["compare", "--samples", "1", "--format", "text"][..],
        ] {
            assert!(parse_args(args(values)).is_err(), "{values:?}");
        }
    }

    #[test]
    fn samples_are_strictly_positive_integers() {
        for value in ["0", "-1", "1.5", "many"] {
            assert!(
                parse_args(args(&[
                    "compare",
                    "--suite",
                    "concat",
                    "--samples",
                    value,
                    "--format",
                    "text",
                ]))
                .is_err(),
                "{value}"
            );
        }
    }
}
