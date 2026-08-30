use std::{
    ffi::{OsStr, OsString},
    fmt,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};

use worldless::CompoundTag;
use worldless_lab::{
    BenchmarkEntry, BenchmarkOptions, BenchmarkReport, CheckReport, ComparisonExecution,
    ComparisonReport,
};

const USAGE: &str = "usage: worldless-lab check [--suite <NAME>] --format <text|json>\n       worldless-lab compare --suite <NAME> --execution fresh --samples <NONZERO_USIZE> --format <text|json>\n       worldless-lab compare --suite <NAME> --execution persistent --warmup <NONZERO_USIZE> --samples <NONZERO_USIZE> --format <text|json>\n       worldless-lab benchmark --pack <DIRECTORY> --model-storage <FILE.dat> --entry <text|tokens> --request <COMPOUND_SNBT> --warmup <USIZE> --samples <NONZERO_USIZE> --quota <NONZERO_USIZE> --format <text|json>";
const EXIT_USAGE: u8 = 2;
const EXIT_LAB: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParsedComparisonExecution {
    Fresh,
    Persistent,
}

#[derive(Debug, Eq, PartialEq)]
struct CheckCommandOptions {
    suite: Option<String>,
    format: OutputFormat,
}

#[derive(Debug, Eq, PartialEq)]
struct CompareCommandOptions {
    suite: String,
    execution: ComparisonExecution,
    samples: usize,
    format: OutputFormat,
}

#[derive(Debug, Eq, PartialEq)]
struct BenchmarkCommandOptions {
    pack: PathBuf,
    model_storage: PathBuf,
    entry: BenchmarkEntry,
    request: CompoundTag,
    warmup: usize,
    samples: usize,
    command_limit: usize,
    format: OutputFormat,
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Check(CheckCommandOptions),
    Compare(CompareCommandOptions),
    Benchmark(BenchmarkCommandOptions),
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
        Command::Compare(options) => {
            if cfg!(debug_assertions) {
                Err(worldless_lab::LabError::from_message(
                    "timing comparison requires a release build; run `cargo run --release -p worldless-lab -- compare ...`",
                ))
            } else {
                worldless_lab::compare(&options.suite, options.execution, options.samples).and_then(
                    |report| write_comparison(report, options.format).map_err(output_error),
                )
            }
        }
        Command::Benchmark(options) => {
            if cfg!(debug_assertions) {
                Err(worldless_lab::LabError::from_message(
                    "production benchmark requires a release build; run `cargo run --release -p worldless-lab -- benchmark ...`",
                ))
            } else {
                let BenchmarkCommandOptions {
                    pack,
                    model_storage,
                    entry,
                    request,
                    warmup,
                    samples,
                    command_limit,
                    format,
                } = options;
                worldless_lab::benchmark(BenchmarkOptions {
                    pack: &pack,
                    model_storage: &model_storage,
                    entry,
                    request: &request,
                    warmup,
                    samples,
                    command_limit,
                })
                .and_then(|report| write_benchmark(report, format).map_err(output_error))
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
                "measurement warmup_discarded={} measured_samples={}",
                report.warmup_discarded, report.measured_samples
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

fn write_benchmark(report: BenchmarkReport, format: OutputFormat) -> io::Result<()> {
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
                "entry={} warmup_discarded={} measured_samples={}",
                report.entry.as_str(),
                report.warmup_discarded,
                report.measured_samples
            )?;
            writeln!(
                output,
                "setup quota_limit={} quota_used={}",
                report.setup_quota.limit, report.setup_quota.used
            )?;
            writeln!(
                output,
                "activation quota_limit={} quota_used={}",
                report.activation_quota.limit, report.activation_quota.used
            )?;
            writeln!(
                output,
                "inference quota_limit={} quota_used={}",
                report.quota.limit, report.quota.used
            )?;
            writeln!(
                output,
                "timing median_ns={} p95_ns={} min_ns={} max_ns={}",
                report.timing.median_ns,
                report.timing.p95_ns,
                report.timing.min_ns,
                report.timing.max_ns
            )?;
            writeln!(
                output,
                "response verified_invocations={} identical={} escaped_snbt={}",
                report.response.verified_invocations,
                report.response.identical,
                report.response.escaped_snbt
            )?;
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
        Ok(Command::Check(parse_check_options(arguments)?))
    } else if command == "compare" {
        Ok(Command::Compare(parse_compare_options(arguments)?))
    } else if command == "benchmark" {
        Ok(Command::Benchmark(parse_benchmark_options(arguments)?))
    } else {
        Err(usage_error(format!("unknown command {command:?}")))
    }
}

fn parse_benchmark_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<BenchmarkCommandOptions, UsageError> {
    let mut pack = None;
    let mut model_storage = None;
    let mut entry = None;
    let mut request = None;
    let mut warmup = None;
    let mut samples = None;
    let mut command_limit = None;
    let mut format = None;
    while let Some(option) = arguments.next() {
        if option == "--pack" {
            if pack.is_some() {
                return Err(usage_error("duplicate --pack"));
            }
            pack = Some(parse_path("--pack", arguments.next())?);
        } else if option == "--model-storage" {
            if model_storage.is_some() {
                return Err(usage_error("duplicate --model-storage"));
            }
            model_storage = Some(parse_path("--model-storage", arguments.next())?);
        } else if option == "--entry" {
            if entry.is_some() {
                return Err(usage_error("duplicate --entry"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage_error("missing value for --entry"))?;
            entry = Some(parse_benchmark_entry(&value)?);
        } else if option == "--request" {
            if request.is_some() {
                return Err(usage_error("duplicate --request"));
            }
            let value = parse_utf8("--request", arguments.next())?;
            request = Some(CompoundTag::from_snbt(&value).map_err(|error| {
                usage_error(format!(
                    "invalid --request compound SNBT {value:?}: {error}"
                ))
            })?);
        } else if option == "--warmup" {
            if warmup.is_some() {
                return Err(usage_error("duplicate --warmup"));
            }
            warmup = Some(parse_usize("--warmup", arguments.next())?);
        } else if option == "--samples" {
            if samples.is_some() {
                return Err(usage_error("duplicate --samples"));
            }
            samples = Some(parse_positive_usize("--samples", arguments.next())?);
        } else if option == "--quota" {
            if command_limit.is_some() {
                return Err(usage_error("duplicate --quota"));
            }
            command_limit = Some(parse_positive_usize("--quota", arguments.next())?);
        } else if option == "--format" {
            if format.is_some() {
                return Err(usage_error("duplicate --format"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage_error("missing value for --format"))?;
            format = Some(parse_format(&value)?);
        } else {
            return Err(usage_error(format!("unexpected argument {option:?}")));
        }
    }

    Ok(BenchmarkCommandOptions {
        pack: pack.ok_or_else(|| usage_error("missing required --pack"))?,
        model_storage: model_storage
            .ok_or_else(|| usage_error("missing required --model-storage"))?,
        entry: entry.ok_or_else(|| usage_error("missing required --entry"))?,
        request: request.ok_or_else(|| usage_error("missing required --request"))?,
        warmup: warmup.ok_or_else(|| usage_error("missing required --warmup"))?,
        samples: samples.ok_or_else(|| usage_error("missing required --samples"))?,
        command_limit: command_limit.ok_or_else(|| usage_error("missing required --quota"))?,
        format: format.ok_or_else(|| usage_error("missing required --format"))?,
    })
}

fn parse_check_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<CheckCommandOptions, UsageError> {
    let mut suite = None;
    let mut format = None;
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
        } else {
            return Err(usage_error(format!("unexpected argument {option:?}")));
        }
    }
    let format = format.ok_or_else(|| usage_error("missing required --format"))?;
    Ok(CheckCommandOptions { suite, format })
}

fn parse_compare_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<CompareCommandOptions, UsageError> {
    let mut suite = None;
    let mut execution = None;
    let mut warmup = None;
    let mut samples = None;
    let mut format = None;
    while let Some(option) = arguments.next() {
        if option == "--suite" {
            if suite.is_some() {
                return Err(usage_error("duplicate --suite"));
            }
            suite = Some(parse_utf8("--suite", arguments.next())?);
        } else if option == "--execution" {
            if execution.is_some() {
                return Err(usage_error("duplicate --execution"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage_error("missing value for --execution"))?;
            execution = Some(parse_comparison_execution(&value)?);
        } else if option == "--warmup" {
            if warmup.is_some() {
                return Err(usage_error("duplicate --warmup"));
            }
            warmup = Some(parse_positive_usize("--warmup", arguments.next())?);
        } else if option == "--samples" {
            if samples.is_some() {
                return Err(usage_error("duplicate --samples"));
            }
            samples = Some(parse_samples(arguments.next())?);
        } else if option == "--format" {
            if format.is_some() {
                return Err(usage_error("duplicate --format"));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage_error("missing value for --format"))?;
            format = Some(parse_format(&value)?);
        } else {
            return Err(usage_error(format!("unexpected argument {option:?}")));
        }
    }

    let execution = execution.ok_or_else(|| usage_error("missing required --execution"))?;
    let execution = match (execution, warmup) {
        (ParsedComparisonExecution::Fresh, None) => ComparisonExecution::Fresh,
        (ParsedComparisonExecution::Fresh, Some(_)) => {
            return Err(usage_error("--warmup is not valid with fresh execution"));
        }
        (ParsedComparisonExecution::Persistent, Some(warmup)) => {
            ComparisonExecution::Persistent { warmup }
        }
        (ParsedComparisonExecution::Persistent, None) => {
            return Err(usage_error(
                "missing required --warmup for persistent execution",
            ));
        }
    };
    Ok(CompareCommandOptions {
        suite: suite.ok_or_else(|| usage_error("missing required --suite"))?,
        execution,
        samples: samples.ok_or_else(|| usage_error("missing required --samples"))?,
        format: format.ok_or_else(|| usage_error("missing required --format"))?,
    })
}

fn parse_utf8(option: &str, value: Option<OsString>) -> Result<String, UsageError> {
    let value = value.ok_or_else(|| usage_error(format!("missing value for {option}")))?;
    value
        .into_string()
        .map_err(|value| usage_error(format!("value for {option} is not UTF-8: {value:?}")))
}

fn parse_path(option: &str, value: Option<OsString>) -> Result<PathBuf, UsageError> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| usage_error(format!("missing value for {option}")))
}

fn parse_benchmark_entry(value: &OsStr) -> Result<BenchmarkEntry, UsageError> {
    if value == "text" {
        Ok(BenchmarkEntry::Text)
    } else if value == "tokens" {
        Ok(BenchmarkEntry::Tokens)
    } else {
        Err(usage_error(format!(
            "invalid --entry {value:?}; expected text or tokens"
        )))
    }
}

fn parse_comparison_execution(value: &OsStr) -> Result<ParsedComparisonExecution, UsageError> {
    if value == "fresh" {
        Ok(ParsedComparisonExecution::Fresh)
    } else if value == "persistent" {
        Ok(ParsedComparisonExecution::Persistent)
    } else {
        Err(usage_error(format!(
            "invalid --execution {value:?}; expected fresh or persistent"
        )))
    }
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
    parse_positive_usize("--samples", value)
}

fn parse_positive_usize(option: &str, value: Option<OsString>) -> Result<usize, UsageError> {
    let parsed = parse_usize(option, value)?;
    if parsed == 0 {
        Err(usage_error(format!("{option} must be greater than zero")))
    } else {
        Ok(parsed)
    }
}

fn parse_usize(option: &str, value: Option<OsString>) -> Result<usize, UsageError> {
    let value = parse_utf8(option, value)?;
    value.parse::<usize>().map_err(|_| {
        usage_error(format!(
            "invalid {option} {value:?}; expected a non-negative integer"
        ))
    })
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
            Ok(Command::Check(CheckCommandOptions {
                suite: Some("concat".to_owned()),
                format: OutputFormat::Json,
            }))
        );
        assert_eq!(
            parse_args(args(&[
                "compare",
                "--execution",
                "persistent",
                "--warmup",
                "3",
                "--samples",
                "7",
                "--suite",
                "concat",
                "--format",
                "text",
            ])),
            Ok(Command::Compare(CompareCommandOptions {
                suite: "concat".to_owned(),
                execution: ComparisonExecution::Persistent { warmup: 3 },
                samples: 7,
                format: OutputFormat::Text,
            }))
        );

        assert_eq!(
            parse_args(args(&[
                "compare",
                "--suite",
                "concat",
                "--execution",
                "fresh",
                "--samples",
                "7",
                "--format",
                "json",
            ])),
            Ok(Command::Compare(CompareCommandOptions {
                suite: "concat".to_owned(),
                execution: ComparisonExecution::Fresh,
                samples: 7,
                format: OutputFormat::Json,
            }))
        );
    }

    #[test]
    fn parses_production_benchmark_options_in_any_order() {
        assert_eq!(
            parse_args(args(&[
                "benchmark",
                "--samples",
                "30",
                "--request",
                r#"{prefix:"Once",max_new_tokens:1}"#,
                "--pack",
                "generated-pack",
                "--quota",
                "1000000",
                "--entry",
                "text",
                "--format",
                "json",
                "--model-storage",
                "model.dat",
                "--warmup",
                "20",
            ])),
            Ok(Command::Benchmark(BenchmarkCommandOptions {
                pack: PathBuf::from("generated-pack"),
                model_storage: PathBuf::from("model.dat"),
                entry: BenchmarkEntry::Text,
                request: CompoundTag::from_snbt(r#"{prefix:"Once",max_new_tokens:1}"#).unwrap(),
                warmup: 20,
                samples: 30,
                command_limit: 1_000_000,
                format: OutputFormat::Json,
            }))
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
            &[
                "compare",
                "--execution",
                "fresh",
                "--samples",
                "1",
                "--format",
                "text",
            ][..],
            &[
                "compare",
                "--suite",
                "concat",
                "--execution",
                "persistent",
                "--samples",
                "1",
                "--format",
                "text",
            ][..],
            &[
                "compare",
                "--suite",
                "concat",
                "--execution",
                "fresh",
                "--warmup",
                "1",
                "--samples",
                "1",
                "--format",
                "text",
            ][..],
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
                    "--execution",
                    "fresh",
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

    #[test]
    fn persistent_warmups_are_strictly_positive_integers() {
        for value in ["0", "-1", "1.5", "many"] {
            assert!(
                parse_args(args(&[
                    "compare",
                    "--suite",
                    "concat",
                    "--execution",
                    "persistent",
                    "--warmup",
                    value,
                    "--samples",
                    "1",
                    "--format",
                    "text",
                ]))
                .is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn benchmark_requires_exact_complete_options() {
        let valid = [
            "benchmark",
            "--pack",
            "pack",
            "--model-storage",
            "model.dat",
            "--entry",
            "tokens",
            "--request",
            "{}",
            "--warmup",
            "0",
            "--samples",
            "1",
            "--quota",
            "1",
            "--format",
            "text",
        ];
        assert!(parse_args(args(&valid)).is_ok());

        for invalid in [
            vec!["benchmark", "--format", "text"],
            vec![
                "benchmark",
                "--pack",
                "pack",
                "--pack",
                "other",
                "--model-storage",
                "model.dat",
            ],
            vec!["benchmark", "--entry", "fixture"],
            vec!["benchmark", "--request", "not-snbt"],
            vec!["benchmark", "--warmup", "-1"],
            vec!["benchmark", "--samples", "0"],
            vec!["benchmark", "--quota", "0"],
        ] {
            assert!(parse_args(args(&invalid)).is_err(), "{invalid:?}");
        }
    }
}
