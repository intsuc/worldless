use std::{
    ffi::{OsStr, OsString},
    fmt,
    io::{self, BufRead, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
};

use worldless::{
    CommandFeedback, CompiledProgram, CompoundTag, ExecutionContext, ExecutionOutcome, Pack,
    Position, Rotation, Vm,
};

const USAGE: &str = "usage: worldless check [--pack <DIR> ...]\n       worldless run --pack <DIR> [--pack <DIR> ...] [--command-storage <NAMESPACE> <FILE> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] function [--arguments <COMPOUND_SNBT>] <FUNCTION_ID>\n       worldless run --pack <DIR> [--pack <DIR> ...] [--command-storage <NAMESPACE> <FILE> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] tag [--arguments <COMPOUND_SNBT>] <TAG_ID>\n       worldless run [--pack <DIR> ...] [--command-storage <NAMESPACE> <FILE> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>] command <COMMAND>\n       worldless repl [--pack <DIR> ...] [--command-storage <NAMESPACE> <FILE> ...] --world-seed <I64> [--command-limit <USIZE>] [--position <X> <Y> <Z>] [--rotation <YAW> <PITCH>]";
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
        options: RuntimeOptions,
        invocation: CliInvocation,
    },
    Repl {
        options: RuntimeOptions,
    },
}

#[derive(Debug, PartialEq)]
struct RuntimeOptions {
    packs: Vec<PathBuf>,
    command_storage_files: Vec<(String, PathBuf)>,
    world_seed: i64,
    command_limit: usize,
    context: ExecutionContext,
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

#[derive(Debug)]
enum LoadVmError {
    Program(worldless::LoadError),
    CommandStorage(worldless::CommandStorageLoadError),
}

impl fmt::Display for LoadVmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Program(error) => fmt::Display::fmt(error, formatter),
            Self::CommandStorage(error) => fmt::Display::fmt(error, formatter),
        }
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
            if let Err(error) = CompiledProgram::from_packs(packs.into_iter().map(Pack::directory))
            {
                eprintln!("error: {error}");
                return ExitCode::from(EXIT_LOAD);
            }
            println!("ok");
        }
        CliCommand::Run {
            options,
            invocation,
        } => {
            let invocation = match invocation {
                CliInvocation::Function {
                    reference,
                    arguments,
                } => {
                    let arguments =
                        match arguments.as_deref().map(CompoundTag::from_snbt).transpose() {
                            Ok(arguments) => arguments,
                            Err(error) => {
                                eprintln!("error: invalid function arguments: {}", error.reason());
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
            let mut vm = match load_vm(&options) {
                Ok(vm) => vm,
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_LOAD);
                }
            };
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            let mut output_error = None;
            let outcome = match invocation {
                Invocation::Function {
                    reference,
                    arguments,
                } => vm
                    .execute_function(
                        &reference,
                        arguments.as_ref(),
                        options.context,
                        options.command_limit,
                        |feedback| {
                            write_feedback_or_record(&mut stdout, feedback, &mut output_error)
                        },
                    )
                    .into_result(),
                Invocation::Command(command) => vm
                    .execute_command(
                        &command,
                        options.context,
                        options.command_limit,
                        |feedback| {
                            write_feedback_or_record(&mut stdout, feedback, &mut output_error)
                        },
                    )
                    .into_result(),
            };
            if let Some(error) = output_error {
                eprintln!("error: failed to write stdout: {error}");
                return ExitCode::from(EXIT_EXECUTION);
            }
            match outcome {
                Ok(outcome) => {
                    if let Err(error) =
                        write_outcome(&mut stdout, outcome).and_then(|()| stdout.flush())
                    {
                        eprintln!("error: failed to write stdout: {error}");
                        return ExitCode::from(EXIT_EXECUTION);
                    }
                }
                Err(error) => {
                    if let Err(output_error) = stdout.flush() {
                        eprintln!("error: failed to write stdout: {output_error}");
                        return ExitCode::from(EXIT_EXECUTION);
                    }
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_EXECUTION);
                }
            }
        }
        CliCommand::Repl { options } => {
            let mut vm = match load_vm(&options) {
                Ok(vm) => vm,
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_LOAD);
                }
            };
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            let stdout = io::stdout();
            let stderr = io::stderr();
            let mut stdin = stdin.lock();
            let mut stdout = stdout.lock();
            let mut stderr = stderr.lock();
            let result = repl(
                &mut vm,
                options.context,
                options.command_limit,
                &mut stdin,
                &mut stdout,
                &mut stderr,
                interactive,
            );
            drop(stdin);
            drop(stdout);
            drop(stderr);
            match result {
                Ok(false) => {}
                Ok(true) => return ExitCode::from(EXIT_EXECUTION),
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::from(EXIT_EXECUTION);
                }
            }
        }
    }

    ExitCode::SUCCESS
}

fn load_vm(options: &RuntimeOptions) -> Result<Vm, LoadVmError> {
    let program = CompiledProgram::from_packs(options.packs.iter().map(Pack::directory))
        .map_err(LoadVmError::Program)?;
    let mut vm = program.create_vm(options.world_seed);
    vm.load_command_storage_files(
        options
            .command_storage_files
            .iter()
            .map(|(namespace, path)| (namespace.as_str(), path)),
    )
    .map_err(LoadVmError::CommandStorage)?;
    Ok(vm)
}

fn repl(
    vm: &mut Vm,
    context: ExecutionContext,
    command_limit: usize,
    input: &mut impl BufRead,
    output: &mut impl Write,
    diagnostics: &mut impl Write,
    interactive: bool,
) -> io::Result<bool> {
    let mut line = String::new();
    let mut line_number = 0_usize;
    let mut had_execution_error = false;

    loop {
        if interactive {
            diagnostics
                .write_all(b"worldless> ")
                .and_then(|()| diagnostics.flush())
                .map_err(|error| contextual_io_error("write the REPL prompt", error))?;
        }

        line.clear();
        let bytes_read = input
            .read_line(&mut line)
            .map_err(|error| contextual_io_error("read REPL input", error))?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        remove_line_ending(&mut line);
        if line.is_empty() {
            continue;
        }
        if line == ":quit" {
            break;
        }

        let mut output_error = None;
        let outcome = vm
            .execute_command(&line, context, command_limit, |feedback| {
                write_feedback_or_record(output, feedback, &mut output_error);
            })
            .into_result();
        if let Some(error) = output_error {
            return Err(contextual_io_error("write REPL output", error));
        }
        match outcome {
            Ok(outcome) => {
                write_outcome(output, outcome)
                    .and_then(|()| output.flush())
                    .map_err(|error| contextual_io_error("write REPL output", error))?;
            }
            Err(error) => {
                output
                    .flush()
                    .map_err(|error| contextual_io_error("write REPL output", error))?;
                had_execution_error = true;
                writeln!(diagnostics, "error: line {line_number}: {error}")
                    .and_then(|()| diagnostics.flush())
                    .map_err(|error| contextual_io_error("write REPL diagnostics", error))?;
            }
        }
    }

    Ok(had_execution_error)
}

fn remove_line_ending(line: &mut String) {
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
}

fn write_feedback_or_record(
    output: &mut impl Write,
    feedback: CommandFeedback,
    output_error: &mut Option<io::Error>,
) {
    if output_error.is_none()
        && let Err(error) = write_feedback(output, feedback)
    {
        *output_error = Some(error);
    }
}

fn write_feedback(output: &mut impl Write, feedback: CommandFeedback) -> io::Result<()> {
    let (kind, text) = match feedback {
        CommandFeedback::Success(text) => ("success", text),
        CommandFeedback::Failure(text) => ("failure", text),
    };
    write!(output, "feedback kind={kind} text=\"")?;
    write_escaped_utf16(output, text.as_utf16())?;
    output.write_all(b"\"\n")
}

fn write_escaped_utf16(output: &mut impl Write, text: &[u16]) -> io::Result<()> {
    for decoded in char::decode_utf16(text.iter().copied()) {
        match decoded {
            Ok('"') => output.write_all(b"\\\"")?,
            Ok('\\') => output.write_all(b"\\\\")?,
            Ok('\n') => output.write_all(b"\\n")?,
            Ok('\r') => output.write_all(b"\\r")?,
            Ok('\t') => output.write_all(b"\\t")?,
            Ok(character)
                if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}') =>
            {
                write!(output, "\\u{{{:x}}}", u32::from(character))?;
            }
            Ok(character) => {
                let mut buffer = [0_u8; 4];
                output.write_all(character.encode_utf8(&mut buffer).as_bytes())?;
            }
            Err(error) => write!(output, "\\u{{{:x}}}", error.unpaired_surrogate())?,
        }
    }
    Ok(())
}

fn write_outcome(output: &mut impl Write, outcome: ExecutionOutcome) -> io::Result<()> {
    match outcome {
        ExecutionOutcome::NoResult => writeln!(output, "no-result"),
        ExecutionOutcome::Result { success, value } => {
            writeln!(output, "result success={success} value={value}")
        }
    }
}

fn contextual_io_error(operation: &str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("failed to {operation}: {error}"))
}

enum Invocation {
    Function {
        reference: String,
        arguments: Option<CompoundTag>,
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
    } else if command == "repl" {
        parse_repl(arguments)
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
    Ok(CliCommand::Check { packs })
}

fn parse_run(arguments: impl Iterator<Item = OsString>) -> Result<CliCommand, UsageError> {
    let mut arguments = arguments.peekable();
    let options = parse_runtime_options(&mut arguments)?;

    let target = arguments
        .next()
        .ok_or_else(|| usage_error("missing run target; expected function, tag, or command"))?;
    if is_runtime_option(&target) {
        return Err(misplaced_runtime_option(target));
    }
    let invocation = if target == "function" {
        require_pack(&options.packs)?;
        parse_function_invocation(&mut arguments, false)?
    } else if target == "tag" {
        require_pack(&options.packs)?;
        parse_function_invocation(&mut arguments, true)?
    } else if target == "command" {
        CliInvocation::Command(parse_command(arguments.next())?)
    } else if is_option(&target) {
        return Err(unexpected_argument(target));
    } else {
        return Err(usage_error(format!("unknown run target {target:?}")));
    };

    if let Some(argument) = arguments.next() {
        if is_runtime_option(&argument) {
            return Err(misplaced_runtime_option(argument));
        }
        if argument == "--arguments" {
            return Err(usage_error("duplicate --arguments"));
        }
        return Err(unexpected_argument(argument));
    }

    Ok(CliCommand::Run {
        options,
        invocation,
    })
}

fn parse_repl(arguments: impl Iterator<Item = OsString>) -> Result<CliCommand, UsageError> {
    let mut arguments = arguments.peekable();
    let options = parse_runtime_options(&mut arguments)?;
    if let Some(argument) = arguments.next() {
        if is_runtime_option(&argument) {
            return Err(misplaced_runtime_option(argument));
        }
        return Err(unexpected_argument(argument));
    }
    Ok(CliCommand::Repl { options })
}

fn parse_runtime_options<I>(
    arguments: &mut std::iter::Peekable<I>,
) -> Result<RuntimeOptions, UsageError>
where
    I: Iterator<Item = OsString>,
{
    let mut packs = Vec::new();
    while arguments
        .peek()
        .is_some_and(|argument| argument == "--pack")
    {
        arguments.next();
        packs.push(parse_pack_path(arguments.next())?);
    }
    let mut command_storage_files = Vec::new();
    while arguments
        .peek()
        .is_some_and(|argument| argument == "--command-storage")
    {
        arguments.next();
        command_storage_files.push(parse_command_storage_file(
            arguments.next(),
            arguments.next(),
        )?);
    }
    if arguments
        .peek()
        .is_some_and(|argument| argument == "--pack")
    {
        return Err(usage_error("--pack must precede --command-storage"));
    }
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

    Ok(RuntimeOptions {
        packs,
        command_storage_files,
        world_seed,
        command_limit,
        context: ExecutionContext::new(position, rotation),
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

fn parse_command_storage_file(
    namespace: Option<OsString>,
    path: Option<OsString>,
) -> Result<(String, PathBuf), UsageError> {
    let namespace =
        namespace.ok_or_else(|| usage_error("missing namespace for --command-storage"))?;
    if is_known_option(&namespace) {
        return Err(usage_error("missing namespace for --command-storage"));
    }
    if is_option(&namespace) {
        return Err(unexpected_argument(namespace));
    }
    let namespace = namespace
        .into_string()
        .map_err(|_| usage_error("--command-storage namespace is not valid UTF-8"))?;

    let path = path.ok_or_else(|| usage_error("missing file for --command-storage"))?;
    if is_known_option(&path) {
        return Err(usage_error("missing file for --command-storage"));
    }
    if is_option(&path) {
        return Err(unexpected_argument(path));
    }
    if path.is_empty() {
        return Err(usage_error("--command-storage file path must not be empty"));
    }
    Ok((namespace, PathBuf::from(path)))
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
        || argument == "--command-storage"
        || argument == "--world-seed"
        || argument == "--command-limit"
        || argument == "--position"
        || argument == "--rotation"
        || argument == "--arguments"
}

fn is_runtime_option(argument: &OsStr) -> bool {
    argument == "--pack"
        || argument == "--command-storage"
        || argument == "--world-seed"
        || argument == "--command-limit"
        || argument == "--position"
        || argument == "--rotation"
}

fn misplaced_runtime_option(argument: OsString) -> UsageError {
    if argument == "--world-seed" {
        usage_error("duplicate --world-seed")
    } else if argument == "--pack" {
        usage_error("--pack must precede --world-seed")
    } else if argument == "--command-storage" {
        usage_error("--command-storage must precede --world-seed")
    } else {
        usage_error(format!(
            "{} is duplicated or out of order",
            argument.to_string_lossy()
        ))
    }
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

    fn options(
        packs: &[&str],
        world_seed: i64,
        command_limit: usize,
        context: ExecutionContext,
    ) -> RuntimeOptions {
        RuntimeOptions {
            packs: packs.iter().map(PathBuf::from).collect(),
            command_storage_files: Vec::new(),
            world_seed,
            command_limit,
            context,
        }
    }

    fn options_with_command_storage(
        packs: &[&str],
        command_storage_files: &[(&str, &str)],
        world_seed: i64,
        command_limit: usize,
        context: ExecutionContext,
    ) -> RuntimeOptions {
        RuntimeOptions {
            packs: packs.iter().map(PathBuf::from).collect(),
            command_storage_files: command_storage_files
                .iter()
                .map(|(namespace, path)| ((*namespace).to_owned(), PathBuf::from(path)))
                .collect(),
            world_seed,
            command_limit,
            context,
        }
    }

    #[test]
    fn accepts_check_and_run_and_preserves_pack_order() {
        assert_eq!(
            parse(&["check"]).unwrap(),
            CliCommand::Check { packs: Vec::new() }
        );
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
                options: options(
                    &["low", "high"],
                    -123,
                    12,
                    ExecutionContext::new(
                        Position::new(-1.5, 2.0, -3.25),
                        Rotation::new(-90.0, 45.5),
                    ),
                ),
                invocation: function("example:main"),
            }
        );
    }

    #[test]
    fn repl_uses_the_shared_runtime_options_without_a_target() {
        assert_eq!(
            parse(&[
                "repl",
                "--pack",
                "low",
                "--pack",
                "high",
                "--world-seed",
                "-7",
                "--command-limit",
                "9",
                "--position",
                "1",
                "2",
                "3",
                "--rotation",
                "90",
                "-45",
            ])
            .unwrap(),
            CliCommand::Repl {
                options: options(
                    &["low", "high"],
                    -7,
                    9,
                    ExecutionContext::new(Position::new(1.0, 2.0, 3.0), Rotation::new(90.0, -45.0),),
                ),
            }
        );

        assert_eq!(
            parse(&["repl", "--pack", "pack", "--world-seed", "0"]).unwrap(),
            CliCommand::Repl {
                options: options(
                    &["pack"],
                    0,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
            }
        );
        assert_eq!(
            parse(&["repl", "--world-seed", "0"]).unwrap(),
            CliCommand::Repl {
                options: options(
                    &[],
                    0,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
            }
        );

        for arguments in [
            &["repl", "--pack", "pack", "--world-seed", "0", "command"][..],
            &[
                "repl",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--world-seed",
                "1",
            ],
            &[
                "repl",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "--position",
                "0",
                "0",
                "0",
                "--command-limit",
                "1",
            ],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn accepts_repeated_command_storage_files_between_packs_and_world_seed() {
        assert_eq!(
            parse(&[
                "run",
                "--pack",
                "pack",
                "--command-storage",
                "probe",
                "probe.dat",
                "--command-storage",
                "other",
                "other.dat",
                "--world-seed",
                "7",
                "command",
                "seed",
            ])
            .unwrap(),
            CliCommand::Run {
                options: options_with_command_storage(
                    &["pack"],
                    &[("probe", "probe.dat"), ("other", "other.dat")],
                    7,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
                invocation: CliInvocation::Command("seed".to_owned()),
            }
        );
        assert_eq!(
            parse(&[
                "repl",
                "--command-storage",
                "probe",
                "probe.dat",
                "--world-seed",
                "-7",
            ])
            .unwrap(),
            CliCommand::Repl {
                options: options_with_command_storage(
                    &[],
                    &[("probe", "probe.dat")],
                    -7,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
            }
        );
    }

    #[test]
    fn validates_command_storage_values_and_option_order() {
        assert_eq!(
            parse(&["run", "--command-storage"]),
            Err(usage_error("missing namespace for --command-storage"))
        );
        assert_eq!(
            parse(&["run", "--command-storage", "probe"]),
            Err(usage_error("missing file for --command-storage"))
        );
        assert_eq!(
            parse(&[
                "run",
                "--command-storage",
                "probe",
                "--world-seed",
                "0",
                "command",
                "seed",
            ]),
            Err(usage_error("missing file for --command-storage"))
        );
        assert_eq!(
            parse(&[
                "run",
                "--command-storage",
                "probe",
                "",
                "--world-seed",
                "0",
                "command",
                "seed",
            ]),
            Err(usage_error("--command-storage file path must not be empty"))
        );
        assert_eq!(
            parse(&[
                "run",
                "--command-storage",
                "probe",
                "probe.dat",
                "--pack",
                "pack",
                "--world-seed",
                "0",
                "command",
                "seed",
            ]),
            Err(usage_error("--pack must precede --command-storage"))
        );
        assert_eq!(
            parse(&[
                "run",
                "--world-seed",
                "0",
                "--command-storage",
                "probe",
                "probe.dat",
                "command",
                "seed",
            ]),
            Err(usage_error("--command-storage must precede --world-seed"))
        );
        assert!(parse(&["check", "--command-storage", "probe", "probe.dat"]).is_err());
        assert!(USAGE.contains("[--command-storage <NAMESPACE> <FILE> ...]"));
    }

    #[test]
    fn parses_function_tags_arguments_and_single_commands() {
        let defaults = |invocation| CliCommand::Run {
            options: options(
                &["pack"],
                0,
                DEFAULT_COMMAND_LIMIT,
                ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
            ),
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
            parse(&["run", "--world-seed", "0", "command", "seed"]).unwrap(),
            CliCommand::Run {
                options: options(
                    &[],
                    0,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
                invocation: CliInvocation::Command("seed".to_owned()),
            }
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
                options: options(
                    &["pack"],
                    123,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
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
                options: options(
                    &["pack"],
                    123,
                    12,
                    ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                ),
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
                options: options(
                    &["pack"],
                    123,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(Position::new(1.0, 2.0, 3.0), DEFAULT_ROTATION),
                ),
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
                options: options(
                    &["pack"],
                    123,
                    DEFAULT_COMMAND_LIMIT,
                    ExecutionContext::new(DEFAULT_POSITION, Rotation::new(90.0, -45.0)),
                ),
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
    fn validates_pack_and_runtime_requirements() {
        for arguments in [
            &["check", "--pack"][..],
            &["check", "--pack", "--pack", "pack"],
            &["check", "--pack", "--unknown"],
            &["check", "--pack", ""],
            &["run", "--world-seed", "1", "function", "example:main"],
            &["run", "--world-seed", "1", "tag", "example:main"],
            &["run", "--command-limit", "1", "function", "example:main"],
            &["run", "--pack", ""],
            &["repl"],
            &["repl", "--pack", "pack"],
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
                    options: options(
                        &["pack"],
                        value,
                        DEFAULT_COMMAND_LIMIT,
                        ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                    ),
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
            &["repl", "--pack", "pack", "--world-seed", "0", "--unknown"],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn removes_only_lf_and_crlf_line_endings() {
        for (input, expected) in [
            ("command\n", "command"),
            ("command\r\n", "command"),
            ("command\r", "command\r"),
            (" command \n", " command "),
            ("", ""),
        ] {
            let mut line = input.to_owned();
            remove_line_ending(&mut line);
            assert_eq!(line, expected);
        }
    }

    #[test]
    fn interactive_repl_prompts_on_stderr_before_each_input_line() {
        let mut vm = CompiledProgram::from_packs([Pack::memory(std::iter::empty::<
            worldless::MemoryResource,
        >())])
        .map(|program| program.create_vm(0))
        .unwrap();
        let mut input = io::Cursor::new(b"\n:quit\n");
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();

        assert!(
            !repl(
                &mut vm,
                ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
                DEFAULT_COMMAND_LIMIT,
                &mut input,
                &mut output,
                &mut diagnostics,
                true,
            )
            .unwrap()
        );
        assert!(output.is_empty());
        assert_eq!(diagnostics, b"worldless> worldless> ");
    }

    #[test]
    fn feedback_text_rendering_preserves_event_boundaries_and_utf16() {
        let mut output = Vec::new();
        write_escaped_utf16(
            &mut output,
            &[
                b'a' as u16,
                b'"' as u16,
                b'\\' as u16,
                b'\n' as u16,
                0x0001,
                0x2028,
                0xd83d,
                0xde00,
                0xd800,
            ],
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "a\\\"\\\\\\n\\u{1}\\u{2028}\u{1f600}\\u{d800}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_paths() {
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

        let path = OsString::from_vec(b"storage-\xff".to_vec());
        let mut expected = options(
            &[],
            0,
            DEFAULT_COMMAND_LIMIT,
            ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
        );
        expected
            .command_storage_files
            .push(("probe".to_owned(), PathBuf::from(path.clone())));
        assert_eq!(
            parse_args([
                OsString::from("run"),
                OsString::from("--command-storage"),
                OsString::from("probe"),
                path,
                OsString::from("--world-seed"),
                OsString::from("0"),
                OsString::from("command"),
                OsString::from("seed"),
            ])
            .unwrap(),
            CliCommand::Run {
                options: expected,
                invocation: CliInvocation::Command("seed".to_owned()),
            }
        );

        assert_eq!(
            parse_args([
                OsString::from("run"),
                OsString::from("--command-storage"),
                OsString::from_vec(b"namespace-\xff".to_vec()),
                OsString::from("storage.dat"),
            ]),
            Err(usage_error(
                "--command-storage namespace is not valid UTF-8"
            ))
        );
    }

    #[cfg(windows)]
    #[test]
    fn preserves_non_utf8_paths() {
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

        let path = OsString::from_wide(&[b's' as u16, 0xd800]);
        let mut expected = options(
            &[],
            0,
            DEFAULT_COMMAND_LIMIT,
            ExecutionContext::new(DEFAULT_POSITION, DEFAULT_ROTATION),
        );
        expected
            .command_storage_files
            .push(("probe".to_owned(), PathBuf::from(path.clone())));
        assert_eq!(
            parse_args([
                OsString::from("run"),
                OsString::from("--command-storage"),
                OsString::from("probe"),
                path,
                OsString::from("--world-seed"),
                OsString::from("0"),
                OsString::from("command"),
                OsString::from("seed"),
            ])
            .unwrap(),
            CliCommand::Run {
                options: expected,
                invocation: CliInvocation::Command("seed".to_owned()),
            }
        );

        assert_eq!(
            parse_args([
                OsString::from("run"),
                OsString::from("--command-storage"),
                OsString::from_wide(&[b'n' as u16, 0xd800]),
                OsString::from("storage.dat"),
            ]),
            Err(usage_error(
                "--command-storage namespace is not valid UTF-8"
            ))
        );
    }
}
