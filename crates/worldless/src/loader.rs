use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::Entry},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use serde_json::{Map, Value};
use worldless_brigadier::{
    Command, CommandDispatcher, LiteralMessage, SINGLE_SUCCESS, StringReader,
    arguments::{ArgumentType, IntegerArgumentType, StringArgumentType},
    builder::{LiteralArgumentBuilder, RequiredArgumentBuilder},
    context::CommandContext,
    exceptions::{CommandSyntaxException, SimpleCommandExceptionType},
};

use crate::{
    program::{
        Command as CompiledCommand, Function, Instruction, Modifier, Program, ScoreComparison,
        ScoreCondition, ScorePredicate, ScoreRange, ScoreReference, ScoreboardCommand,
        ScoreboardOperation, StoreKind,
    },
    resource::{Identifier, is_allowed_in_identifier},
};

const TARGET_PACK_FORMAT: PackFormat = PackFormat {
    major: 118,
    minor: 0,
};
const LAST_PRE_MINOR_DATA_PACK_FORMAT: i32 = 81;
const MAX_COMMAND_LENGTH: usize = 2_000_000;

/// An error encountered while loading a directory data pack.
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidPack {
        path: PathBuf,
        reason: String,
    },
    UnsupportedPack {
        path: PathBuf,
        feature: &'static str,
    },
    InvalidFunction {
        path: PathBuf,
        line: usize,
        reason: String,
    },
}

/// An error encountered while compiling in-memory function source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    InvalidFunctionIdentifier {
        input: String,
    },
    DuplicateFunction {
        id: String,
    },
    InvalidFunction {
        id: String,
        line: usize,
        reason: String,
    },
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFunctionIdentifier { input } => {
                write!(formatter, "invalid function identifier `{input}`")
            }
            Self::DuplicateFunction { id } => {
                write!(formatter, "duplicate function `{id}`")
            }
            Self::InvalidFunction { id, line, reason } => {
                write!(
                    formatter,
                    "invalid function `{id}` at line {line}: {reason}"
                )
            }
        }
    }
}

impl Error for CompileError {}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to access {}: {source}", path.display())
            }
            Self::InvalidPack { path, reason } => {
                write!(
                    formatter,
                    "invalid data pack at {}: {reason}",
                    path.display()
                )
            }
            Self::UnsupportedPack { path, feature } => write!(
                formatter,
                "data pack at {} uses unsupported {feature}",
                path.display()
            ),
            Self::InvalidFunction { path, line, reason } => write!(
                formatter,
                "invalid function {} at line {line}: {reason}",
                path.display()
            ),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub(crate) fn load_directory(root: &Path) -> Result<Program, LoadError> {
    reject_symbolic_links(root)?;
    let metadata = metadata(root)?;
    if !metadata.is_dir() {
        return Err(invalid_pack(root, "the pack path is not a directory"));
    }
    validate_pack_metadata(root)?;

    let compiler = CommandCompiler::new();
    let mut functions = HashMap::new();
    let data = root.join("data");
    for namespace_dir in child_directories(&data)? {
        let Some(namespace) = namespace_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if Identifier::from_parts(namespace, "").is_none() {
            continue;
        }

        let function_root = namespace_dir.join("function");
        for path in regular_files_recursive(&function_root)? {
            let Some(relative) = resource_path(&function_root, &path) else {
                continue;
            };
            if !relative.ends_with(".mcfunction") {
                continue;
            }
            let full_resource_path = format!("function/{relative}");
            if Identifier::from_parts(namespace, &full_resource_path).is_none() {
                continue;
            }
            let function_path = &relative[..relative.len() - ".mcfunction".len()];
            let id = Identifier::from_parts(namespace, function_path)
                .expect("removing a valid suffix preserves an identifier path");
            let contents = read_to_string(&path)?;
            let instructions = parse_function(&contents, &compiler).map_err(|error| {
                LoadError::InvalidFunction {
                    path: path.clone(),
                    line: error.line,
                    reason: error.reason,
                }
            })?;
            functions.insert(id, Function { instructions });
        }
    }

    Ok(Program::new(functions))
}

pub(crate) fn compile_functions<I, N, S>(functions: I) -> Result<Program, CompileError>
where
    I: IntoIterator<Item = (N, S)>,
    N: AsRef<str>,
    S: AsRef<str>,
{
    let compiler = CommandCompiler::new();
    let mut compiled = HashMap::new();
    for (raw_id, source) in functions {
        let raw_id = raw_id.as_ref();
        let id =
            Identifier::parse(raw_id).ok_or_else(|| CompileError::InvalidFunctionIdentifier {
                input: raw_id.to_owned(),
            })?;
        match compiled.entry(id) {
            Entry::Occupied(entry) => {
                return Err(CompileError::DuplicateFunction {
                    id: entry.key().to_string(),
                });
            }
            Entry::Vacant(entry) => {
                let id = entry.key().to_string();
                let instructions = parse_function(source.as_ref(), &compiler).map_err(|error| {
                    CompileError::InvalidFunction {
                        id,
                        line: error.line,
                        reason: error.reason,
                    }
                })?;
                entry.insert(Function { instructions });
            }
        }
    }
    Ok(Program::new(compiled))
}

fn validate_pack_metadata(root: &Path) -> Result<(), LoadError> {
    let path = root.join("pack.mcmeta");
    let contents = read_to_string(&path)?;
    let value: Value = serde_json::from_str(&contents)
        .map_err(|error| invalid_pack(&path, format!("invalid JSON: {error}")))?;
    let root_object = value
        .as_object()
        .ok_or_else(|| invalid_pack(&path, "the root value must be an object"))?;

    for (field, feature) in [
        ("features", "feature flags"),
        ("overlays", "resource overlays"),
    ] {
        if root_object.contains_key(field) {
            return Err(LoadError::UnsupportedPack { path, feature });
        }
    }

    let pack = root_object
        .get("pack")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid_pack(&path, "missing object field `pack`"))?;
    match pack.get("description") {
        Some(Value::String(_)) => {}
        Some(_) => {
            return Err(LoadError::UnsupportedPack {
                path,
                feature: "structured pack descriptions",
            });
        }
        None => return Err(invalid_pack(&path, "missing field `pack.description`")),
    }

    validate_pack_format(&path, pack)
}

fn validate_pack_format(path: &Path, pack: &Map<String, Value>) -> Result<(), LoadError> {
    let min_value = optional_field(pack, "min_format");
    let max_value = optional_field(pack, "max_format");
    if min_value.is_some() != max_value.is_some() {
        return Err(invalid_pack(
            path,
            "`min_format` and `max_format` must both be present",
        ));
    }
    let (Some(min_value), Some(max_value)) = (min_value, max_value) else {
        return Err(invalid_pack(
            path,
            "pack format 118 requires `min_format` and `max_format`",
        ));
    };
    let min = parse_pack_format(path, "min_format", min_value, 0)?;
    let max = parse_pack_format(path, "max_format", max_value, i32::MAX)?;
    if min > max {
        return Err(invalid_pack(
            path,
            format!("`min_format` {min} is greater than `max_format` {max}"),
        ));
    }
    let supported = optional_field(pack, "supported_formats")
        .map(|value| parse_integer_range(path, "supported_formats", value))
        .transpose()?;
    let main = optional_field(pack, "pack_format")
        .map(|value| parse_i32(path, "pack_format", value))
        .transpose()?;

    if min.major > LAST_PRE_MINOR_DATA_PACK_FORMAT {
        if supported.is_some() {
            return Err(invalid_pack(
                path,
                "`supported_formats` is not valid for pack formats 82 and newer",
            ));
        }
    } else {
        let (supported_min, supported_max) = supported.ok_or_else(|| {
            invalid_pack(
                path,
                "ranges including pack formats 81 and older require `supported_formats`",
            )
        })?;
        if supported_min != min.major {
            return Err(invalid_pack(
                path,
                "`supported_formats` and `min_format` have different lower bounds",
            ));
        }
        if supported_max != max.major && supported_max != LAST_PRE_MINOR_DATA_PACK_FORMAT {
            return Err(invalid_pack(
                path,
                "`supported_formats` and `max_format` have incompatible upper bounds",
            ));
        }
        if main.is_none() {
            return Err(invalid_pack(
                path,
                "ranges including pack formats 81 and older require `pack_format`",
            ));
        }
    }
    if let Some(main) = main {
        if main < min.major || main > max.major {
            return Err(invalid_pack(
                path,
                format!("`pack_format` {main} is outside the declared major range"),
            ));
        }
        if main < 15 {
            return Err(invalid_pack(
                path,
                "multi-version packs cannot have `pack_format` below 15",
            ));
        }
    }
    if !(min..=max).contains(&TARGET_PACK_FORMAT) {
        return Err(invalid_pack(
            path,
            format!(
                "declared range {min} through {max} does not include required format {TARGET_PACK_FORMAT}"
            ),
        ));
    }
    Ok(())
}

fn optional_field<'a>(pack: &'a Map<String, Value>, field: &str) -> Option<&'a Value> {
    pack.get(field).filter(|value| !value.is_null())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PackFormat {
    major: i32,
    minor: i32,
}

impl fmt::Display for PackFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.minor == i32::MAX {
            write!(formatter, "{}.*", self.major)
        } else {
            write!(formatter, "{}.{}", self.major, self.minor)
        }
    }
}

fn parse_pack_format(
    path: &Path,
    field: &str,
    value: &Value,
    default_minor: i32,
) -> Result<PackFormat, LoadError> {
    let components = match value {
        Value::Array(values) if (1..=256).contains(&values.len()) => values,
        Value::Array(_) => {
            return Err(invalid_pack(
                path,
                format!("`{field}` must contain between 1 and 256 numbers"),
            ));
        }
        _ => {
            let major = parse_nonnegative_i32(path, field, value)?;
            return Ok(PackFormat {
                major,
                minor: default_minor,
            });
        }
    };
    let parsed = components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            parse_nonnegative_i32(path, &format!("{field}[{index}]"), component)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PackFormat {
        major: parsed[0],
        minor: parsed.get(1).copied().unwrap_or(default_minor),
    })
}

fn parse_nonnegative_i32(path: &Path, field: &str, value: &Value) -> Result<i32, LoadError> {
    let value = parse_i32(path, field, value)?;
    if value < 0 {
        Err(invalid_pack(
            path,
            format!("`{field}` must be a non-negative 32-bit integer"),
        ))
    } else {
        Ok(value)
    }
}

fn parse_i32(path: &Path, field: &str, value: &Value) -> Result<i32, LoadError> {
    value
        .as_number()
        .map(java_number_to_i32)
        .ok_or_else(|| invalid_pack(path, format!("`{field}` must be a number")))
}

fn java_number_to_i32(number: &serde_json::Number) -> i32 {
    if let Some(value) = number.as_i64() {
        return value as i32;
    }
    if let Some(value) = number.as_u64() {
        return value as i32;
    }

    let number = number.to_string();
    let (negative, unsigned) = match number.strip_prefix('-') {
        Some(unsigned) => (true, unsigned),
        None => (false, number.as_str()),
    };
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, 0), |(mantissa, exponent)| {
            (mantissa, parse_decimal_exponent(exponent))
        });
    let fraction_length = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .collect::<Vec<_>>();
    let fraction_length = i64::try_from(fraction_length).unwrap_or(i64::MAX);
    let shift = exponent.saturating_sub(fraction_length);

    let significant_digits = if shift < 0 {
        let dropped = shift.unsigned_abs();
        if dropped >= digits.len() as u64 {
            0
        } else {
            digits.len() - dropped as usize
        }
    } else {
        digits.len()
    };
    let mut narrowed = digits[..significant_digits]
        .iter()
        .fold(0_u32, |value, digit| {
            value
                .wrapping_mul(10)
                .wrapping_add(u32::from(*digit - b'0'))
        });
    if shift > 0 {
        if shift >= 32 {
            narrowed = 0;
        } else {
            for _ in 0..shift {
                narrowed = narrowed.wrapping_mul(10);
            }
        }
    }
    if negative {
        narrowed = 0_u32.wrapping_sub(narrowed);
    }
    narrowed as i32
}

fn parse_decimal_exponent(exponent: &str) -> i64 {
    let (negative, digits) = match exponent.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, exponent.strip_prefix('+').unwrap_or(exponent)),
    };
    let magnitude = digits.bytes().fold(0_i64, |value, digit| {
        value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'))
    });
    if negative { -magnitude } else { magnitude }
}

fn parse_integer_range(path: &Path, field: &str, value: &Value) -> Result<(i32, i32), LoadError> {
    let range = match value {
        Value::Array(values) if values.len() == 2 => (
            parse_i32(path, &format!("{field}[0]"), &values[0])?,
            parse_i32(path, &format!("{field}[1]"), &values[1])?,
        ),
        Value::Object(object) => {
            let min = object.get("min_inclusive").ok_or_else(|| {
                invalid_pack(path, format!("`{field}.min_inclusive` is required"))
            })?;
            let max = object.get("max_inclusive").ok_or_else(|| {
                invalid_pack(path, format!("`{field}.max_inclusive` is required"))
            })?;
            (
                parse_i32(path, &format!("{field}.min_inclusive"), min)?,
                parse_i32(path, &format!("{field}.max_inclusive"), max)?,
            )
        }
        Value::Array(_) => {
            return Err(invalid_pack(
                path,
                format!("`{field}` array must contain exactly two numbers"),
            ));
        }
        _ => {
            let value = parse_i32(path, field, value)?;
            (value, value)
        }
    };
    if range.0 > range.1 {
        Err(invalid_pack(
            path,
            format!("`{field}` lower bound is greater than its upper bound"),
        ))
    } else {
        Ok(range)
    }
}

#[derive(Debug)]
struct FunctionParseError {
    line: usize,
    reason: String,
}

fn parse_function(
    contents: &str,
    compiler: &CommandCompiler,
) -> Result<Vec<Instruction>, FunctionParseError> {
    let lines = java_lines(contents);
    let mut instructions = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line_number = index + 1;
        let mut line = java_trim(lines[index]).to_owned();
        let mut command_length = utf16_length(&line);
        if line.ends_with('\\') {
            loop {
                line.pop();
                command_length -= 1;
                index += 1;
                if index == lines.len() {
                    return Err(invalid_function(
                        line_number,
                        "line continuation at end of file",
                    ));
                }
                let continued = java_trim(lines[index]);
                line.push_str(continued);
                command_length += utf16_length(continued);
                check_command_length(line_number, command_length)?;
                if !line.ends_with('\\') {
                    break;
                }
            }
        }
        check_command_length(line_number, command_length)?;

        if line.is_empty() || line.starts_with('#') {
            index += 1;
            continue;
        }
        if line.starts_with('/') {
            let reason = if line.starts_with("//") {
                "commands cannot start with `/`; comments must start with `#`"
            } else {
                "commands in functions must not start with `/`"
            };
            return Err(invalid_function(line_number, reason));
        }
        if line.starts_with('$') {
            return Err(invalid_function(
                line_number,
                "function macros are not supported",
            ));
        }

        let instruction = compiler
            .compile(&line)
            .map_err(|reason| invalid_function(line_number, reason))?;
        instructions.push(instruction);
        index += 1;
    }
    Ok(instructions)
}

fn check_command_length(line: usize, length: usize) -> Result<(), FunctionParseError> {
    if length > MAX_COMMAND_LENGTH {
        Err(invalid_function(
            line,
            format!("command is {length} UTF-16 code units; maximum is {MAX_COMMAND_LENGTH}"),
        ))
    } else {
        Ok(())
    }
}

fn utf16_length(value: &str) -> usize {
    value.encode_utf16().count()
}

fn java_trim(value: &str) -> &str {
    value.trim_matches(|character| character <= '\u{20}')
}

fn java_lines(contents: &str) -> Vec<&str> {
    let bytes = contents.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            lines.push(&contents[start..index]);
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            start = index + 1;
        }
        index += 1;
    }
    if start < contents.len() {
        lines.push(&contents[start..]);
    }
    lines
}

#[derive(Clone)]
struct LoweringSource {
    sink: Rc<RefCell<Option<Instruction>>>,
    modifiers: Vec<Modifier>,
}

impl LoweringSource {
    fn with_modifier(&self, modifier: Modifier) -> Self {
        let mut modifiers = self.modifiers.clone();
        modifiers.push(modifier);
        Self {
            sink: Rc::clone(&self.sink),
            modifiers,
        }
    }

    fn record(&self, command: CompiledCommand) -> Result<i32, CommandSyntaxException> {
        let mut sink = self.sink.borrow_mut();
        let instruction = Instruction {
            modifiers: self.modifiers.clone(),
            command,
        };
        if sink.replace(instruction).is_some() {
            return Err(syntax_error("command produced more than one instruction"));
        }
        Ok(SINGLE_SUCCESS)
    }
}

struct CommandCompiler {
    dispatcher: CommandDispatcher<LoweringSource>,
}

impl CommandCompiler {
    fn new() -> Self {
        let dispatcher = CommandDispatcher::new();

        let function: Command<LoweringSource> = Rc::new(|context| {
            let id = context
                .argument::<Identifier>("name")
                .expect("the function executor is attached below its name argument");
            context
                .source()
                .record(CompiledCommand::Function((*id).clone()))
        });
        let function_name =
            RequiredArgumentBuilder::argument("name", IdentifierArgument).executes(function);
        dispatcher
            .register(
                LiteralArgumentBuilder::literal("function")
                    .then(function_name)
                    .expect("a literal can contain an argument"),
            )
            .expect("the command tree contains no conflicting function literal");

        let return_value: Command<LoweringSource> = Rc::new(|context| {
            let value = IntegerArgumentType::get_integer(context, "value")
                .expect("the return executor is attached below its integer argument");
            context.source().record(CompiledCommand::Return {
                success: true,
                value,
            })
        });
        let return_fail: Command<LoweringSource> = Rc::new(|context| {
            context.source().record(CompiledCommand::Return {
                success: false,
                value: 0,
            })
        });
        let return_run = LiteralArgumentBuilder::literal("run")
            .redirect_with_modifier(
                dispatcher.root(),
                Rc::new(|context| Ok(Rc::new(context.source().with_modifier(Modifier::ReturnRun)))),
            )
            .expect("the return run literal has no children");
        let return_command = LiteralArgumentBuilder::literal("return")
            .then(
                RequiredArgumentBuilder::argument("value", IntegerArgumentType::integer())
                    .executes(return_value),
            )
            .expect("the return literal can contain an integer argument")
            .then(LiteralArgumentBuilder::literal("fail").executes(return_fail))
            .expect("the return literal can contain the fail literal")
            .then(return_run)
            .expect("the return literal can contain the run redirect");
        dispatcher
            .register(return_command)
            .expect("the command tree contains no conflicting return literal");

        let add_objective: Command<LoweringSource> = Rc::new(|context| {
            let objective = command_string(context, "objective");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::AddObjective { objective },
            ))
        });
        let set_score: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holder(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the set executor is attached below its integer argument");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::SetScore {
                    holder,
                    objective,
                    value,
                }))
        });
        let get_score: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holder(context, "target");
            let objective = command_string(context, "objective");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                    holder,
                    objective,
                }))
        });
        let add_score: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holder(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the add executor is attached below its integer argument");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::AddScore {
                    holder,
                    objective,
                    value,
                }))
        });
        let remove_score: Command<LoweringSource> = Rc::new(|context| {
            let holder = score_holder(context, "targets");
            let objective = command_string(context, "objective");
            let value = IntegerArgumentType::get_integer(context, "score")
                .expect("the remove executor is attached below its integer argument");
            context.source().record(CompiledCommand::Scoreboard(
                ScoreboardCommand::RemoveScore {
                    holder,
                    objective,
                    value,
                },
            ))
        });
        let operate_score: Command<LoweringSource> = Rc::new(|context| {
            let target = score_reference(context, "targets", "targetObjective");
            let operation = scoreboard_operation(context, "operation");
            let source = score_reference(context, "source", "sourceObjective");
            context
                .source()
                .record(CompiledCommand::Scoreboard(ScoreboardCommand::Operation {
                    target,
                    operation,
                    source,
                }))
        });
        let scoreboard = LiteralArgumentBuilder::literal("scoreboard")
            .then(
                LiteralArgumentBuilder::literal("objectives")
                    .then(
                        LiteralArgumentBuilder::literal("add")
                            .then(
                                RequiredArgumentBuilder::argument(
                                    "objective",
                                    StringArgumentType::word(),
                                )
                                .then(
                                    LiteralArgumentBuilder::literal("dummy")
                                        .executes(add_objective),
                                )
                                .expect("an objective name can contain the dummy criterion"),
                            )
                            .expect("the objectives add literal can contain an objective name"),
                    )
                    .expect("the objectives literal can contain the add literal"),
            )
            .expect("the scoreboard literal can contain objectives commands")
            .then(
                LiteralArgumentBuilder::literal("players")
                    .then(
                        LiteralArgumentBuilder::literal("set")
                            .then(
                                RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "objective",
                                            StringArgumentType::word(),
                                        )
                                        .then(
                                            RequiredArgumentBuilder::argument(
                                                "score",
                                                IntegerArgumentType::integer(),
                                            )
                                            .executes(set_score),
                                        )
                                        .expect("an objective can contain a score"),
                                    )
                                    .expect("a score holder can contain an objective"),
                            )
                            .expect("the players set literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the set literal")
                    .then(
                        LiteralArgumentBuilder::literal("get")
                            .then(
                                RequiredArgumentBuilder::argument("target", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "objective",
                                            StringArgumentType::word(),
                                        )
                                        .executes(get_score),
                                    )
                                    .expect("a score holder can contain an objective"),
                            )
                            .expect("the players get literal can contain a score holder"),
                    )
                    .expect("the players literal can contain the get literal")
                    .then(score_delta_branch("add", add_score))
                    .expect("the players literal can contain the add literal")
                    .then(score_delta_branch("remove", remove_score))
                    .expect("the players literal can contain the remove literal")
                    .then(
                        LiteralArgumentBuilder::literal("operation")
                            .then(
                                RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                                    .then(
                                        RequiredArgumentBuilder::argument(
                                            "targetObjective",
                                            StringArgumentType::word(),
                                        )
                                        .then(
                                            RequiredArgumentBuilder::argument(
                                                "operation",
                                                ScoreboardOperationArgument,
                                            )
                                            .then(
                                                RequiredArgumentBuilder::argument(
                                                    "source",
                                                    ScoreHolderArgument,
                                                )
                                                .then(
                                                    RequiredArgumentBuilder::argument(
                                                        "sourceObjective",
                                                        StringArgumentType::word(),
                                                    )
                                                    .executes(operate_score),
                                                )
                                                .expect(
                                                    "an operation source can contain an objective",
                                                ),
                                            )
                                            .expect(
                                                "an operation can contain a source score holder",
                                            ),
                                        )
                                        .expect("a target objective can contain an operation"),
                                    )
                                    .expect("an operation target can contain an objective"),
                            )
                            .expect("the operation literal can contain a target score holder"),
                    )
                    .expect("the players literal can contain the operation literal"),
            )
            .expect("the scoreboard literal can contain players commands");
        dispatcher
            .register(scoreboard)
            .expect("the command tree contains no conflicting scoreboard literal");

        let execute = dispatcher
            .register(LiteralArgumentBuilder::literal("execute"))
            .expect("the command tree contains no conflicting execute literal");
        let execute_command = LiteralArgumentBuilder::literal("execute")
            .then(
                LiteralArgumentBuilder::literal("run")
                    .redirect(dispatcher.root())
                    .expect("the execute run literal has no children"),
            )
            .expect("the execute literal can contain the run literal")
            .then(
                LiteralArgumentBuilder::literal("store")
                    .then(store_score_branch(
                        "result",
                        StoreKind::Result,
                        execute.clone(),
                    ))
                    .expect("the store literal can contain the result literal")
                    .then(store_score_branch(
                        "success",
                        StoreKind::Success,
                        execute.clone(),
                    ))
                    .expect("the store literal can contain the success literal"),
            )
            .expect("the execute literal can contain the store literal")
            .then(execute_condition_branch("if", true, execute.clone()))
            .expect("the execute literal can contain the if literal")
            .then(execute_condition_branch("unless", false, execute))
            .expect("the execute literal can contain the unless literal");
        dispatcher
            .register(execute_command)
            .expect("the command tree contains no conflicting execute literal");

        Self { dispatcher }
    }

    fn compile(&self, command: &str) -> Result<Instruction, String> {
        let sink = Rc::new(RefCell::new(None));
        self.dispatcher
            .execute(
                command,
                LoweringSource {
                    sink: Rc::clone(&sink),
                    modifiers: Vec::new(),
                },
            )
            .map_err(|error| error.to_string())?;
        let instruction = sink.borrow_mut().take();
        instruction.ok_or_else(|| "command did not produce an instruction".to_owned())
    }
}

fn score_delta_branch(
    literal: &'static str,
    command: Command<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                .then(
                    RequiredArgumentBuilder::argument("objective", StringArgumentType::word())
                        .then(
                            RequiredArgumentBuilder::argument(
                                "score",
                                IntegerArgumentType::integer_range(0, i32::MAX),
                            )
                            .executes(command),
                        )
                        .expect("an objective can contain a non-negative score"),
                )
                .expect("a score holder can contain an objective"),
        )
        .expect("a score operation can contain a score holder")
}

fn execute_condition_branch(
    literal: &'static str,
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            LiteralArgumentBuilder::literal("score")
                .then(
                    RequiredArgumentBuilder::argument("target", ScoreHolderArgument)
                        .then(
                            RequiredArgumentBuilder::argument(
                                "targetObjective",
                                StringArgumentType::word(),
                            )
                            .then(score_comparison_condition_branch(
                                "=",
                                ScoreComparison::Equal,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain equality comparison")
                            .then(score_comparison_condition_branch(
                                "<",
                                ScoreComparison::LessThan,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain less-than comparison")
                            .then(score_comparison_condition_branch(
                                "<=",
                                ScoreComparison::LessThanOrEqual,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain less-than-or-equal comparison")
                            .then(score_comparison_condition_branch(
                                ">",
                                ScoreComparison::GreaterThan,
                                expected,
                                execute.clone(),
                            ))
                            .expect("a target objective can contain greater-than comparison")
                            .then(score_comparison_condition_branch(
                                ">=",
                                ScoreComparison::GreaterThanOrEqual,
                                expected,
                                execute.clone(),
                            ))
                            .expect(
                                "a target objective can contain greater-than-or-equal comparison",
                            )
                            .then(score_matches_condition_branch(expected, execute.clone()))
                            .expect("a target objective can contain a range match"),
                        )
                        .expect("a target score holder can contain an objective"),
                )
                .expect("the score literal can contain a target score holder"),
        )
        .expect("a conditional can contain the score literal")
        .then(
            LiteralArgumentBuilder::literal("function")
                .then(
                    RequiredArgumentBuilder::argument("name", IdentifierArgument)
                        .fork(
                            execute,
                            Rc::new(move |context: &CommandContext<LoweringSource>| {
                                let function = context.argument::<Identifier>("name").expect(
                                    "the function condition is attached below its name argument",
                                );
                                Ok(vec![Rc::new(context.source().with_modifier(
                                    Modifier::FunctionCondition {
                                        expected,
                                        function: (*function).clone(),
                                    },
                                ))])
                            }),
                        )
                        .expect("a function condition can redirect to execute"),
                )
                .expect("the function literal can contain a function name"),
        )
        .expect("a conditional can contain the function literal")
}

fn score_comparison_condition_branch(
    literal: &'static str,
    comparison: ScoreComparison,
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::Condition(score_comparison_condition(
                context, expected, comparison,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        let condition = score_comparison_condition(context, expected, comparison);
        Ok(vec![Rc::new(
            context
                .source()
                .with_modifier(Modifier::Condition(condition)),
        )])
    });

    LiteralArgumentBuilder::literal(literal)
        .then(
            RequiredArgumentBuilder::argument("source", ScoreHolderArgument)
                .then(
                    RequiredArgumentBuilder::argument(
                        "sourceObjective",
                        StringArgumentType::word(),
                    )
                    .executes(terminal)
                    .fork(execute, modifier)
                    .expect("a complete comparison can redirect to execute"),
                )
                .expect("a comparison source can contain an objective"),
        )
        .expect("a comparison can contain a source score holder")
}

fn score_matches_condition_branch(
    expected: bool,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    let terminal: Command<LoweringSource> = Rc::new(move |context| {
        context
            .source()
            .record(CompiledCommand::Condition(score_matches_condition(
                context, expected,
            )))
    });
    let modifier = Rc::new(move |context: &CommandContext<LoweringSource>| {
        let condition = score_matches_condition(context, expected);
        Ok(vec![Rc::new(
            context
                .source()
                .with_modifier(Modifier::Condition(condition)),
        )])
    });

    LiteralArgumentBuilder::literal("matches")
        .then(
            RequiredArgumentBuilder::argument("range", ScoreRangeArgument)
                .executes(terminal)
                .fork(execute, modifier)
                .expect("a complete range match can redirect to execute"),
        )
        .expect("matches can contain an integer range")
}

fn score_comparison_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
    comparison: ScoreComparison,
) -> ScoreCondition {
    ScoreCondition {
        expected,
        predicate: ScorePredicate::Compare {
            left: score_reference(context, "target", "targetObjective"),
            comparison,
            right: score_reference(context, "source", "sourceObjective"),
        },
    }
}

fn score_matches_condition(
    context: &CommandContext<LoweringSource>,
    expected: bool,
) -> ScoreCondition {
    let range = context
        .argument::<ScoreRange>("range")
        .map(|range| *range)
        .expect("the score condition is attached below its range argument");
    ScoreCondition {
        expected,
        predicate: ScorePredicate::Matches {
            score: score_reference(context, "target", "targetObjective"),
            range,
        },
    }
}

fn store_score_branch(
    literal: &'static str,
    kind: StoreKind,
    execute: worldless_brigadier::tree::Node<LoweringSource>,
) -> LiteralArgumentBuilder<LoweringSource> {
    LiteralArgumentBuilder::literal(literal)
        .then(
            LiteralArgumentBuilder::literal("score")
                .then(
                    RequiredArgumentBuilder::argument("targets", ScoreHolderArgument)
                        .then(
                            RequiredArgumentBuilder::argument(
                                "objective",
                                StringArgumentType::word(),
                            )
                            .redirect_with_modifier(
                                execute,
                                Rc::new(move |context| {
                                    let holder = score_holder(context, "targets");
                                    let objective = command_string(context, "objective");
                                    Ok(Rc::new(context.source().with_modifier(
                                        Modifier::StoreScore {
                                            kind,
                                            holder,
                                            objective,
                                        },
                                    )))
                                }),
                            )
                            .expect("the store objective has no children"),
                        )
                        .expect("a store score holder can contain an objective"),
                )
                .expect("the score literal can contain a score holder"),
        )
        .expect("a store mode can contain the score literal")
}

fn command_string(context: &CommandContext<LoweringSource>, name: &str) -> String {
    StringArgumentType::get_string(context, name)
        .expect("the command executor is attached below the requested string argument")
}

fn score_holder(context: &CommandContext<LoweringSource>, name: &str) -> String {
    context
        .argument::<String>(name)
        .map(|holder| (*holder).clone())
        .expect("the command executor is attached below the requested score holder argument")
}

fn score_reference(
    context: &CommandContext<LoweringSource>,
    holder: &str,
    objective: &str,
) -> ScoreReference {
    ScoreReference {
        holder: score_holder(context, holder),
        objective: command_string(context, objective),
    }
}

fn scoreboard_operation(
    context: &CommandContext<LoweringSource>,
    name: &str,
) -> ScoreboardOperation {
    context
        .argument::<ScoreboardOperation>(name)
        .map(|operation| *operation)
        .expect("the scoreboard executor is attached below its operation argument")
}

#[derive(Clone, Copy)]
struct ScoreHolderArgument;

impl ArgumentType<LoweringSource> for ScoreHolderArgument {
    type Value = String;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && reader.peek() != b' ' as u16 {
            reader.skip();
        }
        let holder = reader.substring(start, reader.cursor());
        if holder.starts_with('#') {
            Ok(holder)
        } else {
            reader.set_cursor(start);
            Err(SimpleCommandExceptionType::new(LiteralMessage::new(
                "only score holders whose names start with `#` are supported",
            ))
            .create_with_context(reader))
        }
    }

    fn examples(&self) -> Vec<String> {
        vec!["#value".to_owned()]
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct ScoreboardOperationArgument;

impl ArgumentType<LoweringSource> for ScoreboardOperationArgument {
    type Value = ScoreboardOperation;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        while reader.can_read() && reader.peek() != b' ' as u16 {
            reader.skip();
        }
        let operation = match reader.substring(start, reader.cursor()).as_str() {
            "=" => ScoreboardOperation::Assign,
            "+=" => ScoreboardOperation::Add,
            "-=" => ScoreboardOperation::Subtract,
            "*=" => ScoreboardOperation::Multiply,
            "/=" => ScoreboardOperation::Divide,
            "%=" => ScoreboardOperation::Modulo,
            "<" => ScoreboardOperation::Min,
            ">" => ScoreboardOperation::Max,
            "><" => ScoreboardOperation::Swap,
            _ => {
                reader.set_cursor(start);
                return Err(SimpleCommandExceptionType::new(LiteralMessage::new(
                    "invalid scoreboard operation",
                ))
                .create_with_context(reader));
            }
        };
        Ok(operation)
    }

    fn examples(&self) -> Vec<String> {
        ["=", "+=", "><"].into_iter().map(str::to_owned).collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy)]
struct ScoreRangeArgument;

impl ArgumentType<LoweringSource> for ScoreRangeArgument {
    type Value = ScoreRange;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let parsed = (|| {
            let min = parse_score_range_bound(reader)?;
            let max = if reader.can_read_n(2)
                && reader.peek() == b'.' as u16
                && reader.peek_offset(1) == b'.' as u16
            {
                reader.skip();
                reader.skip();
                parse_score_range_bound(reader)?
            } else {
                min
            };

            if min.is_none() && max.is_none() {
                return Err("empty score range");
            }
            if min.zip(max).is_some_and(|(min, max)| min > max) {
                return Err("swapped score range bounds");
            }
            Ok(ScoreRange { min, max })
        })();

        parsed.map_err(|message| {
            reader.set_cursor(start);
            SimpleCommandExceptionType::new(LiteralMessage::new(message))
                .create_with_context(reader)
        })
    }

    fn examples(&self) -> Vec<String> {
        ["0..5", "0", "-5", "-100..", "..100"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn parse_score_range_bound(reader: &mut StringReader) -> Result<Option<i32>, &'static str> {
    let start = reader.cursor();
    while reader.can_read() && is_allowed_score_range_number(reader) {
        reader.skip();
    }
    if start == reader.cursor() {
        return Ok(None);
    }
    reader
        .substring(start, reader.cursor())
        .parse()
        .map(Some)
        .map_err(|_| "invalid integer in score range")
}

fn is_allowed_score_range_number(reader: &StringReader) -> bool {
    match reader.peek() {
        unit if matches!(unit, 0x30..=0x39) || unit == b'-' as u16 => true,
        unit if unit == b'.' as u16 => {
            !reader.can_read_n(2) || reader.peek_offset(1) != b'.' as u16
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct IdentifierArgument;

impl ArgumentType<LoweringSource> for IdentifierArgument {
    type Value = Identifier;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        if reader.can_read() && reader.peek() == b'#' as u16 {
            return Err(SimpleCommandExceptionType::new(LiteralMessage::new(
                "function tags are not supported",
            ))
            .create_with_context(reader));
        }
        while reader.can_read() && is_allowed_in_identifier(reader.peek()) {
            reader.skip();
        }
        let raw = reader.substring(start, reader.cursor());
        if let Some(id) = Identifier::parse(&raw) {
            Ok(id)
        } else {
            reader.set_cursor(start);
            Err(
                SimpleCommandExceptionType::new(LiteralMessage::new("invalid function identifier"))
                    .create_with_context(reader),
            )
        }
    }

    fn examples(&self) -> Vec<String> {
        ["foo", "foo:bar"].into_iter().map(str::to_owned).collect()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

fn syntax_error(message: &'static str) -> CommandSyntaxException {
    SimpleCommandExceptionType::new(LiteralMessage::new(message)).create()
}

fn reject_symbolic_links(path: &Path) -> Result<(), LoadError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    if metadata.file_type().is_symlink() {
        return Err(LoadError::UnsupportedPack {
            path: path.to_owned(),
            feature: "symbolic links",
        });
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: path.to_owned(),
            source,
        })?;
        reject_symbolic_links(&entry.path())?;
    }
    Ok(())
}

fn child_directories(path: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: path.to_owned(),
                source,
            });
        }
    };
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: path.to_owned(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| LoadError::Io {
            path: entry.path(),
            source,
        })?;
        if file_type.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn regular_files_recursive(root: &Path) -> Result<Vec<PathBuf>, LoadError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(Vec::new());
        }
        Err(source) => {
            return Err(LoadError::Io {
                path: root.to_owned(),
                source,
            });
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| LoadError::Io {
            path: root.to_owned(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            paths.extend(regular_files_recursive(&path)?);
        } else if file_type.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn resource_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()
        .map(|components| components.join("/"))
}

fn metadata(path: &Path) -> Result<fs::Metadata, LoadError> {
    fs::metadata(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })
}

fn read_to_string(path: &Path) -> Result<String, LoadError> {
    fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_owned(),
        source,
    })
}

fn invalid_pack(path: &Path, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidPack {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

fn invalid_function(line: usize, reason: impl Into<String>) -> FunctionParseError {
    FunctionParseError {
        line,
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_line_splitting_handles_all_buffered_reader_endings() {
        assert_eq!(java_lines("a\rb\r\nc\nd\n"), ["a", "b", "c", "d"]);
        assert_eq!(java_lines("a\n\n"), ["a", ""]);
        assert!(java_lines("").is_empty());
    }

    #[test]
    fn java_trim_does_not_remove_unicode_whitespace() {
        assert_eq!(java_trim(" \tvalue\r\n"), "value");
        assert_eq!(java_trim("\u{a0}value\u{a0}"), "\u{a0}value\u{a0}");
    }

    #[test]
    fn command_length_uses_utf16_code_units() {
        let within = "\u{1f600}".repeat(MAX_COMMAND_LENGTH / 2);
        assert!(check_command_length(1, utf16_length(&within)).is_ok());
        let beyond = format!("{within}a");
        assert!(check_command_length(1, utf16_length(&beyond)).is_err());
    }

    #[test]
    fn json_numbers_use_java_int_value_narrowing() {
        let path = Path::new("pack.mcmeta");
        for (input, expected) in [
            ("118.9", 118),
            ("1.18e2", 118),
            ("4294967414", 118),
            ("-4294967178", 118),
            ("1e100000", 0),
            ("1e-100000", 0),
        ] {
            let value: Value = serde_json::from_str(input).unwrap();
            assert_eq!(parse_i32(path, "value", &value).unwrap(), expected);
        }
    }

    #[test]
    fn compiler_lowers_only_the_supported_command_set() {
        let compiler = CommandCompiler::new();
        let instruction = compiler.compile("function example:child").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(instruction.command, CompiledCommand::Function(_)));

        let instruction = compiler.compile("return -7").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: true,
                value: -7
            }
        ));

        let instruction = compiler.compile("return fail").unwrap();
        assert!(instruction.modifiers.is_empty());
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: false,
                value: 0
            }
        ));

        let instruction = compiler
            .compile("return run return run function example:child")
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [Modifier::ReturnRun, Modifier::ReturnRun]
        ));
        assert!(matches!(instruction.command, CompiledCommand::Function(_)));

        assert!(matches!(
            compiler
                .compile("scoreboard objectives add values dummy")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::AddObjective { ref objective })
                if objective == "values"
        ));
        assert!(matches!(
            compiler
                .compile("scoreboard players set #value values -7")
                .unwrap()
                .command,
            CompiledCommand::Scoreboard(ScoreboardCommand::SetScore {
                ref holder,
                ref objective,
                value: -7
            }) if holder == "#value" && objective == "values"
        ));
        assert!(matches!(
            compiler
                .compile("return run scoreboard players get #value values")
                .unwrap(),
            Instruction {
                ref modifiers,
                command: CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                    ref holder,
                    ref objective
                })
            } if matches!(modifiers.as_slice(), [Modifier::ReturnRun])
                && holder == "#value"
                && objective == "values"
        ));

        assert!(compiler.compile("function #example:tag").is_err());
        assert!(compiler.compile("scoreboard objectives list").is_err());
        assert!(
            compiler
                .compile("scoreboard objectives add values trigger")
                .is_err()
        );
        assert!(
            compiler
                .compile("scoreboard players set Player values 1")
                .is_err()
        );
        assert!(
            compiler
                .compile("scoreboard players get @s values")
                .is_err()
        );
        assert!(compiler.compile("scoreboard players get * values").is_err());
    }

    #[test]
    fn compiler_preserves_result_action_order() {
        let compiler = CommandCompiler::new();
        let instruction = compiler
            .compile(
                "execute store result score #first values store success score #second values run return run scoreboard players get #source values",
            )
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::StoreScore {
                    kind: StoreKind::Result,
                    holder: first,
                    objective: first_objective
                },
                Modifier::StoreScore {
                    kind: StoreKind::Success,
                    holder: second,
                    objective: second_objective
                },
                Modifier::ReturnRun
            ] if first == "#first"
                && first_objective == "values"
                && second == "#second"
                && second_objective == "values"
        ));
        assert!(matches!(
            instruction.command,
            CompiledCommand::Scoreboard(ScoreboardCommand::GetScore {
                ref holder,
                ref objective
            }) if holder == "#source" && objective == "values"
        ));

        let instruction = compiler
            .compile(
                "return run execute store success score #result values run function example:child",
            )
            .unwrap();
        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::ReturnRun,
                Modifier::StoreScore {
                    kind: StoreKind::Success,
                    ..
                }
            ]
        ));
        assert!(
            compiler
                .compile("execute store result score Player values run return 1")
                .is_err()
        );
    }

    #[test]
    fn compiler_lowers_function_conditions_as_nonterminal_modifiers() {
        let compiler = CommandCompiler::new();
        let instruction = compiler
            .compile(
                "execute if function example:first unless function example:second run return 3",
            )
            .unwrap();

        assert!(matches!(
            instruction.modifiers.as_slice(),
            [
                Modifier::FunctionCondition {
                    expected: true,
                    function: first
                },
                Modifier::FunctionCondition {
                    expected: false,
                    function: second
                }
            ] if first.to_string() == "example:first" && second.to_string() == "example:second"
        ));
        assert!(matches!(
            instruction.command,
            CompiledCommand::Return {
                success: true,
                value: 3
            }
        ));
        assert!(
            compiler
                .compile("execute if function example:first")
                .is_err()
        );
        assert!(
            compiler
                .compile("execute unless function #example:tag run return 1")
                .is_err()
        );
    }

    #[test]
    fn continuation_precedes_comment_handling_and_requires_a_following_line() {
        let compiler = CommandCompiler::new();
        let instructions = parse_function(
            "# consumes the next line\\\nreturn 1\nreturn 2\n",
            &compiler,
        )
        .unwrap();
        assert!(matches!(
            instructions.as_slice(),
            [Instruction {
                modifiers,
                command: CompiledCommand::Return {
                    success: true,
                    value: 2
                }
            }] if modifiers.is_empty()
        ));
        assert!(parse_function("return 1\\", &compiler).is_err());
    }
}
