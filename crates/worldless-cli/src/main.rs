use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
    process::ExitCode,
};

use worldless::{ExecutionContext, FunctionOutcome, Pack, Position, Rotation, Vm};

const USAGE: &str = "usage: worldless check --pack <DIR> [--pack <DIR> ...]\n       worldless run --pack <DIR> [--pack <DIR> ...] [--world-seed <I64>] [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] <FUNCTION_ID>";
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
        world_seed: Option<i64>,
        command_limit: usize,
        context: ExecutionContext,
        function_id: String,
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

    let (packs, world_seed, operation) = match command {
        CliCommand::Check { packs } => (packs, None, Operation::Check),
        CliCommand::Run {
            packs,
            world_seed,
            command_limit,
            context,
            function_id,
        } => (
            packs,
            world_seed,
            Operation::Run {
                command_limit,
                context,
                function_id,
            },
        ),
    };
    let mut vm = match Vm::from_packs(packs.into_iter().map(Pack::directory), world_seed) {
        Ok(vm) => vm,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(EXIT_LOAD);
        }
    };

    match operation {
        Operation::Check => println!("ok"),
        Operation::Run {
            command_limit,
            context,
            function_id,
        } => match vm.execute_function(&function_id, context, command_limit) {
            Ok(FunctionOutcome::FellThrough) => println!("fell-through"),
            Ok(FunctionOutcome::Returned { success, value }) => {
                println!("returned success={success} value={value}")
            }
            Err(error) => {
                eprintln!("error: {error}");
                return ExitCode::from(EXIT_EXECUTION);
            }
        },
    }

    ExitCode::SUCCESS
}

enum Operation {
    Check,
    Run {
        command_limit: usize,
        context: ExecutionContext,
        function_id: String,
    },
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

    let world_seed = if arguments
        .peek()
        .is_some_and(|argument| argument == "--world-seed")
    {
        arguments.next();
        Some(parse_world_seed(arguments.next())?)
    } else {
        None
    };

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

    if world_seed.is_some()
        && arguments
            .peek()
            .is_some_and(|argument| argument == "--world-seed")
    {
        return Err(usage_error("duplicate --world-seed"));
    }

    let function_id = parse_function_id(arguments.next())?;
    if let Some(argument) = arguments.next() {
        if world_seed.is_some() && argument == "--world-seed" {
            return Err(usage_error("duplicate --world-seed"));
        }
        if argument == "--command-limit" {
            return Err(usage_error("duplicate --command-limit"));
        }
        return Err(unexpected_argument(argument));
    }

    Ok(CliCommand::Run {
        packs,
        world_seed,
        command_limit,
        context: ExecutionContext::new(position, rotation),
        function_id,
    })
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

fn parse_function_id(argument: Option<OsString>) -> Result<String, UsageError> {
    let argument = argument.ok_or_else(|| usage_error("missing function identifier"))?;
    if is_option(&argument) {
        if argument == "--command-limit" {
            return Err(usage_error("duplicate --command-limit"));
        }
        return Err(unexpected_argument(argument));
    }
    argument
        .into_string()
        .map_err(|_| usage_error("function identifier is not valid UTF-8"))
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
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("low"), PathBuf::from("high")],
                world_seed: Some(-123),
                command_limit: 12,
                context: ExecutionContext::new(
                    Position::new(-1.5, 2.0, -3.25),
                    Rotation::new(-90.0, 45.5),
                ),
                function_id: "example:main".to_owned(),
            }
        );
    }

    #[test]
    fn run_options_have_independent_defaults() {
        assert_eq!(
            parse(&["run", "--pack", "pack", "example:main"]).unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: None,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                function_id: "example:main".to_owned(),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "123",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: Some(123),
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                function_id: "example:main".to_owned(),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "12",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: None,
                command_limit: 12,
                context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                function_id: "example:main".to_owned(),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--position",
                "1",
                "2",
                "3",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: None,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(Position::new(1.0, 2.0, 3.0), DEFAULT_ROTATION),
                function_id: "example:main".to_owned(),
            }
        );
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--rotation",
                "90",
                "-45",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("pack")],
                world_seed: None,
                command_limit: DEFAULT_COMMAND_LIMIT,
                context: ExecutionContext::new(DEFAULT_POSITION, Rotation::new(90.0, -45.0)),
                function_id: "example:main".to_owned(),
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
            &["run", "--world-seed", "1", "example:main"],
            &["run", "--command-limit", "1", "example:main"],
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
                    OsString::from("example:main"),
                ])
                .unwrap(),
                CliCommand::Run {
                    packs: vec![PathBuf::from("pack")],
                    world_seed: Some(value),
                    command_limit: DEFAULT_COMMAND_LIMIT,
                    context: ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                    function_id: "example:main".to_owned(),
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
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "+1",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--world-seed",
                "1.0",
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
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--world-seed",
                "2",
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
                    OsString::from("example:main"),
                ])
                .is_err()
            );
        }

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
                "example:main",
            ]),
            Err(usage_error("duplicate --world-seed"))
        );
    }

    #[test]
    fn validates_explicit_command_limits() {
        for arguments in [
            &["run", "--pack", "pack", "--command-limit"][..],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "--pack",
                "other",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "-1",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "+1",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1.0",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--command-limit",
                "2",
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
                OsString::from("--command-limit"),
                OsString::from(overflow),
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
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "example:main",
            ][..],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "0",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "NaN",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "inf",
                "0",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "1e309",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
                "example:main",
            ],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "1e39",
                "0",
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
            &["run", "--pack", "pack", "--command-limit", "1", "--unknown"],
            &[
                "run",
                "--pack",
                "pack",
                "--command-limit",
                "1",
                "--position",
                "0",
                "0",
                "0",
                "--rotation",
                "0",
                "0",
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
