use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
    process::ExitCode,
};

use worldless::{
    ExecutionContext, ExecutionOutcome, FunctionArguments, Pack, Position, Rotation, Vm,
    validate_packs,
};

const USAGE: &str = "usage: worldless check --pack <DIR> [--pack <DIR> ...]\n       worldless run --pack <DIR> [--pack <DIR> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] function [--arguments <COMPOUND_SNBT>] <FUNCTION_ID>\n       worldless run --pack <DIR> [--pack <DIR> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] tag [--arguments <COMPOUND_SNBT>] <TAG_ID>\n       worldless run --pack <DIR> [--pack <DIR> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] command <COMMAND>";
const DEFAULT_COMMAND_LIMIT: usize = 65_536;
const DEFAULT_POSITION: Position = Position::new(0.0, 0.0, 0.0);
const DEFAULT_ROTATION: Rotation = Rotation::new(0.0, 0.0);
const EXIT_USAGE: u8 = 2;
const EXIT_LOAD: u8 = 3;
const EXIT_EXECUTION: u8 = 4;

#[derive(Debug, PartialEq)]
enum CliCommand {
    Check {
        packs: Vec<PathBuf>,
    },
    Run {
        packs: Vec<PathBuf>,
        world_seed: i64,
        command_limit: usize,
        context: ExecutionContext,
        invocation: CliInvocation,
    },
}

#[derive(Debug, PartialEq)]
enum CliInvocation {
    Function {
        reference: String,
        arguments: Option<String>,
    },
    Command(String),
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

    match command {
        CliCommand::Check { packs } => {
            if let Err(error) = validate_packs(packs.into_iter().map(Pack::directory)) {
                eprintln!("error: {error}");
                return ExitCode::from(EXIT_LOAD);
            }
            println!("ok");
        }
        CliCommand::Run {
            packs,
            world_seed,
            command_limit,
            context,
            invocation,
        } => {
            let invocation = match invocation {
                CliInvocation::Function {
                    reference,
                    arguments,
                } => {
                    let arguments = match arguments
                        .as_deref()
                        .map(FunctionArguments::from_snbt)
                        .transpose()
                    {
                        Ok(arguments) => arguments,
                        Err(error) => {
                            eprintln!("error: {error}");
                            return ExitCode::from(EXIT_EXECUTION);
                        }
                    };
                    Invocation::Function {
                        reference,
                        arguments,
                    }
                }
                CliInvocation::Command(command) => Invocation::Command(command),
            };
            let mut vm = match Vm::from_packs(packs.into_iter().map(Pack::directory), world_seed) {
                Ok(vm) => vm,
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_LOAD);
                }
            };
            let outcome = match invocation {
                Invocation::Function {
                    reference,
                    arguments,
                } => vm.execute_function(&reference, arguments.as_ref(), context, command_limit),
                Invocation::Command(command) => {
                    vm.execute_command(&command, context, command_limit)
                }
            };
            match outcome {
                Ok(ExecutionOutcome::NoResult) => println!("no-result"),
                Ok(ExecutionOutcome::Result { success, value }) => {
                    println!("result success={success} value={value}")
                }
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_EXECUTION);
                }
            }
        }
    }

    ExitCode::SUCCESS
}

enum Invocation {
    Function {
        reference: String,
        arguments: Option<FunctionArguments>,
    },
    Command(String),
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<CliCommand, UsageError> {
    let mut arguments = arguments.into_iter();
    let command = arguments
        .next()
        .ok_or_else(|| usage_error("missing command"))?;
    if command == "check" {
        parse_check(arguments)
    } else if command == "run" {
        parse_run(arguments)
    } else {
        Err(usage_error(format!("unknown command {command:?}")))
    }
}

fn parse_check(mut arguments: impl Iterator<Item = OsString>) -> Result<CliCommand, UsageError> {
    let mut packs = Vec::new();
    while let Some(argument) = arguments.next() {
        if argument != "--pack" {
            return Err(unexpected_argument(argument));
        }
        packs.push(parse_pack_path(arguments.next())?);
    }
    require_pack(&packs)?;
    Ok(CliCommand::Check { packs })
}

fn parse_run(arguments: impl Iterator<Item = OsString>) -> Result<CliCommand, UsageError> {
    let mut arguments = arguments.peekable();
    let mut packs = Vec::new();
    while arguments
        .peek()
        .is_some_and(|argument| argument == "--pack")
    {
        arguments.next();
        packs.push(parse_pack_path(arguments.next())?);
    }
    require_pack(&packs)?;

    if !arguments
        .peek()
        .is_some_and(|argument| argument == "--world-seed")
    {
        return Err(usage_error("missing required --world-seed"));
    }
    arguments.next();
    let world_seed = parse_world_seed(arguments.next())?;

    let command_limit = if arguments
        .peek()
        .is_some_and(|argument| argument == "--command-limit")
    {
        arguments.next();
        parse_command_limit(arguments.next())?
    } else {
        DEFAULT_COMMAND_LIMIT
    };

    let position = if arguments
        .peek()
        .is_some_and(|argument| argument == "--position")
    {
        arguments.next();
        Position::new(
            parse_finite_f64(arguments.next(), "--position X")?,
            parse_finite_f64(arguments.next(), "--position Y")?,
            parse_finite_f64(arguments.next(), "--position Z")?,
        )
    } else {
        DEFAULT_POSITION
    };

    let rotation = if arguments
        .peek()
        .is_some_and(|argument| argument == "--rotation")
    {
        arguments.next();
        Rotation::new(
            parse_finite_f32(arguments.next(), "--rotation YAW")?,
            parse_finite_f32(arguments.next(), "--rotation PITCH")?,
        )
    } else {
        DEFAULT_ROTATION
    };

    if arguments
        .peek()
        .is_some_and(|argument| argument == "--world-seed")
    {
        return Err(usage_error("duplicate --world-seed"));
    }

    let target = arguments
        .next()
        .ok_or_else(|| usage_error("missing run target; expected function, tag, or command"))?;
    let invocation = if target == "function" {
        parse_function_invocation(&mut arguments, false)?
    } else if target == "tag" {
        parse_function_invocation(&mut arguments, true)?
    } else if target == "command" {
        CliInvocation::Command(parse_command(arguments.next())?)
    } else if is_option(&target) {
        return Err(unexpected_argument(target));
    } else {
        return Err(usage_error(format!("unknown run target {target:?}")));
    };

    if let Some(argument) = arguments.next() {
        if argument == "--world-seed" {
            return Err(usage_error("duplicate --world-seed"));
        }
        if argument == "--command-limit" {
            return Err(usage_error("duplicate --command-limit"));
        }
        if argument == "--arguments" {
            return Err(usage_error("duplicate --arguments"));
        }
        return Err(unexpected_argument(argument));
    }

    Ok(CliCommand::Run {
        packs,
        world_seed,
        command_limit,
        context: ExecutionContext::new(position, rotation),
        invocation,
    })
}

fn parse_function_invocation(
    arguments: &mut impl Iterator<Item = OsString>,
    tag: bool,
) -> Result<CliInvocation, UsageError> {
    let arguments_snbt = match arguments.next() {
        Some(argument) if argument == "--arguments" => {
            Some(parse_arguments_snbt(arguments.next())?)
        }
        Some(argument) => {
            let id = parse_function_id(Some(argument), tag)?;
            return Ok(CliInvocation::Function {
                reference: function_reference(id, tag),
                arguments: None,
            });
        }
        None => return Err(missing_function_identifier(tag)),
    };
    let id = parse_function_id(arguments.next(), tag)?;
    Ok(CliInvocation::Function {
        reference: function_reference(id, tag),
        arguments: arguments_snbt,
    })
}

fn parse_arguments_snbt(argument: Option<OsString>) -> Result<String, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing value for --arguments"))?;
    if is_known_option(&argument) {
        return Err(usage_error("missing value for --arguments"));
    }
    if is_option(&argument) {
        return Err(unexpected_argument(argument));
    }
    argument
        .into_string()
        .map_err(|_| usage_error("--arguments is not valid UTF-8"))
}

fn function_reference(id: String, tag: bool) -> String {
    if tag { format!("#{id}") } else { id }
}

fn parse_command(argument: Option<OsString>) -> Result<String, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing command"))?;
    argument
        .into_string()
        .map_err(|_| usage_error("command is not valid UTF-8"))
}

fn parse_pack_path(argument: Option<OsString>) -> Result<PathBuf, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing value for --pack"))?;
    if is_known_option(&argument) {
        return Err(usage_error("missing value for --pack"));
    }
    if is_option(&argument) {
        return Err(unexpected_argument(argument));
    }
    if argument.is_empty() {
        return Err(usage_error("--pack path must not be empty"));
    }
    Ok(PathBuf::from(argument))
}

fn require_pack(packs: &[PathBuf]) -> Result<(), UsageError> {
    if packs.is_empty() {
        Err(usage_error("at least one --pack is required"))
    } else {
        Ok(())
    }
}

fn parse_world_seed(argument: Option<OsString>) -> Result<i64, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing value for --world-seed"))?;
    if is_known_option(&argument) {
        return Err(usage_error("missing value for --world-seed"));
    }
    if is_option(&argument) {
        return Err(unexpected_argument(argument));
    }
    let text = argument
        .to_str()
        .ok_or_else(|| usage_error("--world-seed is not valid UTF-8"))?;
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(usage_error(format!("invalid --world-seed {text:?}")));
    }
    text.parse::<i64>()
        .map_err(|_| usage_error(format!("invalid --world-seed {text:?}")))
}

fn parse_command_limit(argument: Option<OsString>) -> Result<usize, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing value for --command-limit"))?;
    if is_known_option(&argument) {
        return Err(usage_error("missing value for --command-limit"));
    }
    if is_option(&argument) {
        return Err(unexpected_argument(argument));
    }
    let text = argument
        .to_str()
        .ok_or_else(|| usage_error("--command-limit is not valid UTF-8"))?;
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(usage_error(format!("invalid --command-limit {text:?}")));
    }
    text.parse::<usize>()
        .map_err(|_| usage_error(format!("invalid --command-limit {text:?}")))
}

fn parse_finite_f64(argument: Option<OsString>, value_name: &str) -> Result<f64, UsageError> {
    let text = parse_number_text(argument, value_name)?;
    let value = text
        .parse::<f64>()
        .map_err(|_| invalid_number(value_name, &text))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid_number(value_name, &text))
    }
}

fn parse_finite_f32(argument: Option<OsString>, value_name: &str) -> Result<f32, UsageError> {
    let text = parse_number_text(argument, value_name)?;
    let value = text
        .parse::<f32>()
        .map_err(|_| invalid_number(value_name, &text))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid_number(value_name, &text))
    }
}

fn parse_number_text(argument: Option<OsString>, value_name: &str) -> Result<String, UsageError> {
    let argument =
        argument.ok_or_else(|| usage_error(format!("missing value for {value_name}")))?;
    if is_known_option(&argument) {
        return Err(usage_error(format!("missing value for {value_name}")));
    }
    if is_option(&argument) {
        return Err(unexpected_argument(argument));
    }
    argument
        .into_string()
        .map_err(|_| usage_error(format!("{value_name} is not valid UTF-8")))
}

fn invalid_number(value_name: &str, text: &str) -> UsageError {
    usage_error(format!(
        "invalid {value_name} {text:?}; expected a finite number"
    ))
}

fn parse_function_id(argument: Option<OsString>, tag: bool) -> Result<String, UsageError> {
    let argument = argument.ok_or_else(|| missing_function_identifier(tag))?;
    if is_option(&argument) {
        if argument == "--arguments" {
            return Err(usage_error("duplicate --arguments"));
        }
        if argument == "--command-limit" {
            return Err(usage_error("duplicate --command-limit"));
        }
        return Err(unexpected_argument(argument));
    }
    let id = argument
        .into_string()
        .map_err(|_| usage_error(identifier_encoding_error(tag)))?;
    if id.starts_with('#') {
        return Err(usage_error(if tag {
            "tag identifier must not start with '#'"
        } else {
            "function identifier must not start with '#'"
        }));
    }
    Ok(id)
}

fn missing_function_identifier(tag: bool) -> UsageError {
    usage_error(if tag {
        "missing tag identifier"
    } else {
        "missing function identifier"
    })
}

fn identifier_encoding_error(tag: bool) -> &'static str {
    if tag {
        "tag identifier is not valid UTF-8"
    } else {
        "function identifier is not valid UTF-8"
    }
}

fn is_option(argument: &OsStr) -> bool {
    argument
        .to_str()
        .is_some_and(|argument| argument.starts_with("--"))
}

fn is_known_option(argument: &OsStr) -> bool {
    argument == "--pack"
        || argument == "--world-seed"
        || argument == "--command-limit"
        || argument == "--position"
        || argument == "--rotation"
        || argument == "--arguments"
}

fn unexpected_argument(argument: OsString) -> UsageError {
    if is_option(&argument) {
        usage_error(format!("unknown option {argument:?}"))
    } else {
        usage_error(format!("unexpected argument {argument:?}"))
    }
}

fn usage_error(message: impl Into<String>) -> UsageError {
    UsageError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<CliCommand, UsageError> {
        parse_args(arguments.iter().copied().map(OsString::from))
    }

    fn function(reference: &str) -> CliInvocation {
        CliInvocation::Function {
            reference: reference.to_owned(),
            arguments: None,
        }
    }

    #[test]
    fn accepts_check_and_run_and_preserves_pack_order() {
        assert_eq!(
            parse(&["check", "--pack", "pack"]).unwrap(),
            CliCommand::Check {
                packs: vec![PathBuf::from("pack")]
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "low",
                "--pack",
                "high",
                "--world-seed",
                "-123",
                "--command-limit",
                "12",
                "--position",
                "-1.5",
                "2",
                "-3.25",
                "--rotation",
                "-90",
                "45.5",
                "function",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("low"), PathBuf::from("high")],
                world_seed: -123,
                command_limit: 12,
                context: ExecutionContext::new(
                    Position::new(-1.5, 2.0, -3.25),
                    Rotation::new(-90.0, 45.5),
                ),
                invocation: function("example:main"),
            }
        );
    }

    #[test]
    fn parses_function_tags_arguments_and_single_commands() {
        let defaults = |invocation| CliCommand::Run {
            packs: vec![PathBuf::from("pack")],
            world_seed: 0,
            command_limit: DEFAULT_COMMAND_LIMIT,
            context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
            invocation,
        };

        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "function",
                "--arguments",
                r#"{name:"Ada",count:3}"#,
                "example:macro",
            ])
            .unwrap(),
            defaults(CliInvocation::Function {
                reference: "example:macro".to_owned(),
                arguments: Some(r#"{name:"Ada",count:3}"#.to_owned()),
            })
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "tag",
                "--arguments",
                "{phase:test}",
                "example:entries",
            ])
            .unwrap(),
            defaults(CliInvocation::Function {
                reference: "#example:entries".to_owned(),
                arguments: Some("{phase:test}".to_owned()),
            })
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "command",
                "scoreboard players get #value example",
            ])
            .unwrap(),
            defaults(CliInvocation::Command(
                "scoreboard players get #value example".to_owned()
            ))
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "command",
                "--not-a-command",
            ])
            .unwrap(),
            defaults(CliInvocation::Command("--not-a-command".to_owned()))
        );
    }

    #[test]
    fn requires_explicit_run_target_and_exact_target_grammar() {
        for arguments in [
            &["run", "--pack", "pack", "--world-seed", "0", "example:main"][..],
            &["run", "--pack", "pack", "--world-seed", "0"],
            &["run", "--pack", "pack", "--world-seed", "0", "function"],
            &["run", "--pack", "pack", "--world-seed", "0", "tag"],
            &["run", "--pack", "pack", "--world-seed", "0", "command"],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "function",
                "example:main",
                "extra",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "command",
                "return 1",
                "extra",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "function",
                "#example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "tag",
                "#example:entries",
            ],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }

        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "function",
                "--arguments",
                "{}",
                "--arguments",
                "{}",
                "example:main",
            ]),
            Err(usage_error("duplicate --arguments"))
        );
    }

    #[test]
    fn run_options_have_defaults_after_the_required_world_seed() {
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "123",
                "function",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: 123,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                invocation: function("example:main"),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "123",
                "--command-limit",
                "12",
                "function",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: 123,
                command_limit: 12,
                context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                invocation: function("example:main"),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "123",
                "--position",
                "1",
                "2",
                "3",
                "function",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: 123,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(Position::new(1.0, 2.0, 3.0), DEFAULT_ROTATION),
                invocation: function("example:main"),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "123",
                "--rotation",
                "90",
                "-45",
                "function",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: 123,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(DEFAULT_POSITION, Rotation::new(90.0, -45.0)),
                invocation: function("example:main"),
            }
        );
    }

    #[test]
    fn rejects_missing_and_unknown_commands() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["inspect", "--pack", "pack"]).is_err());
        assert!(parse(&["--help"]).is_err());
    }

    #[test]
    fn requires_nonempty_explicit_packs() {
        for arguments in [
            &["check"][..],
            &["check", "--pack"],
            &["check", "--pack", "--pack", "pack"],
            &["check", "--pack", "--unknown"],
            &["check", "--pack", ""],
            &["run", "--world-seed", "1", "function", "example:main"],
            &["run", "--command-limit", "1", "function", "example:main"],
            &["run", "--pack", ""],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn validates_explicit_world_seeds() {
        for value in [i64::MIN, 0, i64::MAX] {
            assert_eq!(
                parse_args([
                    OsString::from("run"),
                    OsString::from("--pack"),
                    OsString::from("pack"),
                    OsString::from("--world-seed"),
                    OsString::from(value.to_string()),
                    OsString::from("function"),
                    OsString::from("example:main"),
                ])
                .unwrap(),
                CliCommand::Run {
                    packs: vec![PathBuf::from("pack")],
                    world_seed: value,
                    command_limit: DEFAULT_COMMAND_LIMIT,
                    context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                    invocation: function("example:main"),
                }
            );
        }

        for arguments in [
            &["run", "--pack", "pack", "--world-seed"][..],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "--command-limit",
                "1",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "+1",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "1.0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "1",
                "--world-seed",
                "2",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--world-seed",
                "2",
                "function",
                "example:main",
            ],
            &["check", "--pack", "pack", "--world-seed", "1"],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }

        for overflow in [format!("{}0", i64::MAX), format!("{}0", i64::MIN)] {
            assert!(
                parse_args([
                    OsString::from("run"),
                    OsString::from("--pack"),
                    OsString::from("pack"),
                    OsString::from("--world-seed"),
                    OsString::from(overflow),
                    OsString::from("function"),
                    OsString::from("example:main"),
                ])
                .is_err()
            );
        }

        assert_eq!(
            parse(&["run", "--pack", "pack", "function", "example:main"]),
            Err(usage_error("missing required --world-seed"))
        );
        assert_eq!(
            parse(&["run", "--pack", "pack", "--world-seed"]),
            Err(usage_error("missing value for --world-seed"))
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "invalid",
                "function",
                "example:main",
            ]),
            Err(usage_error("invalid --world-seed \"invalid\""))
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "1",
                "--world-seed",
                "2",
                "function",
                "example:main",
            ]),
            Err(usage_error("duplicate --world-seed"))
        );
    }

    #[test]
    fn validates_explicit_command_limits() {
        for arguments in [
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
            ][..],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "--pack",
                "other",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "-1",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "+1",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1.0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--command-limit",
                "2",
                "function",
                "example:main",
            ],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }

        let overflow = format!("{}0", usize::MAX);
        assert!(
            parse_args([
                OsString::from("run"),
                OsString::from("--pack"),
                OsString::from("pack"),
                OsString::from("--world-seed"),
                OsString::from("0"),
                OsString::from("--command-limit"),
                OsString::from(overflow),
                OsString::from("function"),
                OsString::from("example:main"),
            ])
            .is_err()
        );
    }

    #[test]
    fn validates_explicit_execution_context_values() {
        for arguments in [
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "function",
                "example:main",
            ][..],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "NaN",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "inf",
                "0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "1e309",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "function",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "1e39",
                "0",
                "function",
                "example:main",
            ],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn rejects_unknown_options_and_extra_positionals() {
        for arguments in [
            &["check", "--unknown", "value"][..],
            &["check", "--"],
            &["check", "--pack=pack"],
            &["check", "pack"],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--unknown",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "function",
                "example:main",
                "extra",
            ],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_pack_paths() {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(b"pack-\xff".to_vec());
        assert_eq!(
            parse_args([
                OsString::from("check"),
                OsString::from("--pack"),
                path.clone(),
            ])
            .unwrap(),
            CliCommand::Check {
                packs: vec![PathBuf::from(path)]
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn preserves_non_utf8_pack_paths() {
        use std::os::windows::ffi::OsStringExt;

        let path = OsString::from_wide(&[b'p' as u16, 0xd800]);
        assert_eq!(
            parse_args([
                OsString::from("check"),
                OsString::from("--pack"),
                path.clone(),
            ])
            .unwrap(),
            CliCommand::Check {
                packs: vec![PathBuf::from(path)]
            }
        );
    }
}
