use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
    process::ExitCode,
};

use worldless::{FunctionOutcome, Pack, Vm};

const USAGE: &str = "usage: worldless check --pack <DIR> [--pack <DIR> ...]\n       worldless run --pack <DIR> [--pack <DIR> ...] --command-limit <USIZE> <FUNCTION_ID>";
const EXIT_USAGE: u8 = 2;
const EXIT_LOAD: u8 = 3;
const EXIT_EXECUTION: u8 = 4;

#[derive(Debug, Eq, PartialEq)]
enum CliCommand {
    Check {
        packs: Vec<PathBuf>,
    },
    Run {
        packs: Vec<PathBuf>,
        command_limit: usize,
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

    let (packs, operation) = match command {
        CliCommand::Check { packs } => (packs, Operation::Check),
        CliCommand::Run {
            packs,
            command_limit,
            function_id,
        } => (
            packs,
            Operation::Run {
                command_limit,
                function_id,
            },
        ),
    };
    let mut vm = match Vm::from_packs(packs.into_iter().map(Pack::directory)) {
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
            function_id,
        } => match vm.execute_function(&function_id, command_limit) {
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

    match arguments.next() {
        Some(argument) if argument == "--command-limit" => {}
        Some(argument) => {
            return Err(usage_error(format!(
                "expected --command-limit, found {argument:?}"
            )));
        }
        None => return Err(usage_error("missing --command-limit")),
    }
    let command_limit = parse_command_limit(arguments.next())?;
    let function_id = parse_function_id(arguments.next())?;
    if let Some(argument) = arguments.next() {
        if argument == "--command-limit" {
            return Err(usage_error("duplicate --command-limit"));
        }
        return Err(unexpected_argument(argument));
    }

    Ok(CliCommand::Run {
        packs,
        command_limit,
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
    argument == "--pack" || argument == "--command-limit"
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
                "--command-limit",
                "12",
                "example:main",
            ])
            .unwrap(),
            CliCommand::Run {
                packs: vec![PathBuf::from("low"), PathBuf::from("high")],
                command_limit: 12,
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
            &["run", "--command-limit", "1", "example:main"],
            &["run", "--pack", ""],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn requires_one_valid_command_limit() {
        for arguments in [
            &["run", "--pack", "pack", "example:main"][..],
            &["run", "--pack", "pack", "--command-limit"],
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
