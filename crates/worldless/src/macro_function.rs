use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use unicode_general_category::{GeneralCategory, get_general_category};

use crate::{
    nbt::{CompoundTag, JavaString},
    program::Instruction,
};

pub(crate) const MAX_COMMAND_LENGTH: usize = 2_000_000;
const MAX_CACHE_ENTRIES: usize = 8;
type MacroCache = VecDeque<(Vec<JavaString>, Arc<[Instruction]>)>;

#[derive(Debug)]
pub(crate) enum Function {
    Plain(Arc<[Instruction]>),
    Macro(MacroFunction),
}

#[derive(Debug)]
pub(crate) struct MacroFunction {
    entries: Vec<MacroEntry>,
    parameters: Vec<JavaString>,
    cache: Mutex<MacroCache>,
}

#[derive(Debug)]
enum MacroEntry {
    Plain(Instruction),
    Template {
        template: StringTemplate,
        parameters: Vec<usize>,
    },
}

#[derive(Debug)]
struct StringTemplate {
    segments: Vec<JavaString>,
    variable_count: usize,
}

pub(crate) struct FunctionBuilder {
    entries: Vec<MacroEntry>,
    parameters: Vec<JavaString>,
}

#[derive(Debug)]
pub(crate) enum FunctionInstantiationError {
    MissingArguments,
    MissingArgument(JavaString),
    Parse { command: JavaString, reason: String },
    Other(String),
}

impl FunctionBuilder {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub(crate) fn add_command(&mut self, instruction: Instruction) {
        self.entries.push(MacroEntry::Plain(instruction));
    }

    pub(crate) fn add_macro(&mut self, command: &str) -> Result<(), String> {
        let (template, variables) = StringTemplate::parse(command)?;
        let parameters = variables
            .into_iter()
            .map(|variable| {
                self.parameters
                    .iter()
                    .position(|parameter| parameter == &variable)
                    .unwrap_or_else(|| {
                        let index = self.parameters.len();
                        self.parameters.push(variable);
                        index
                    })
            })
            .collect();
        self.entries.push(MacroEntry::Template {
            template,
            parameters,
        });
        Ok(())
    }

    pub(crate) fn build(self) -> Function {
        if !self.parameters.is_empty() {
            Function::Macro(MacroFunction {
                entries: self.entries,
                parameters: self.parameters,
                cache: Mutex::new(VecDeque::new()),
            })
        } else {
            let instructions = self
                .entries
                .into_iter()
                .map(|entry| match entry {
                    MacroEntry::Plain(instruction) => instruction,
                    MacroEntry::Template { .. } => {
                        unreachable!("a template marks its function as a macro")
                    }
                })
                .collect::<Vec<_>>();
            Function::Plain(Arc::from(instructions))
        }
    }
}

impl MacroFunction {
    pub(crate) fn instantiate(
        &self,
        arguments: Option<&CompoundTag>,
        mut compile: impl FnMut(Vec<u16>) -> Result<Instruction, String>,
    ) -> Result<Arc<[Instruction]>, FunctionInstantiationError> {
        let arguments = arguments.ok_or(FunctionInstantiationError::MissingArguments)?;
        let values = self
            .parameters
            .iter()
            .map(
                |parameter| -> Result<JavaString, FunctionInstantiationError> {
                    let value = arguments.get(parameter).ok_or_else(|| {
                        FunctionInstantiationError::MissingArgument(parameter.clone())
                    })?;
                    Ok(value.macro_stringify())
                },
            )
            .collect::<Result<Vec<_>, FunctionInstantiationError>>()?;

        {
            let mut cache = self
                .cache
                .lock()
                .expect("the macro cache lock is not poisoned");
            if let Some(index) = cache.iter().position(|(key, _)| key == &values) {
                let cached = cache
                    .remove(index)
                    .expect("the cached argument vector was found");
                let function = Arc::clone(&cached.1);
                cache.push_back(cached);
                return Ok(function);
            }
        }

        let function = self
            .entries
            .iter()
            .map(|entry| match entry {
                MacroEntry::Plain(instruction) => Ok(instruction.clone()),
                MacroEntry::Template {
                    template,
                    parameters,
                } => {
                    let substitutions = parameters
                        .iter()
                        .map(|&index| &values[index])
                        .collect::<Vec<_>>();
                    let command = template
                        .substitute(&substitutions)
                        .map_err(FunctionInstantiationError::Other)?;
                    compile(command.clone()).map_err(|reason| FunctionInstantiationError::Parse {
                        command: JavaString::from_units(command),
                        reason,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Arc::<[Instruction]>::from)?;
        let mut cache = self
            .cache
            .lock()
            .expect("the macro cache lock is not poisoned");
        if cache.len() == MAX_CACHE_ENTRIES {
            cache.pop_front();
        }
        cache.push_back((values, Arc::clone(&function)));
        Ok(function)
    }
}

impl StringTemplate {
    fn parse(input: &str) -> Result<(Self, Vec<JavaString>), String> {
        let input = input.encode_utf16().collect::<Vec<_>>();
        let mut segments = Vec::new();
        let mut variables = Vec::new();
        let mut start = 0;
        let mut search = 0;

        while let Some(relative) = input[search..].iter().position(|&unit| unit == b'$' as u16) {
            let index = search + relative;
            if input.get(index + 1) == Some(&(b'(' as u16)) {
                segments.push(JavaString::from_units(input[start..index].to_vec()));
                let variable_start = index + 2;
                let variable_end = input[variable_start..]
                    .iter()
                    .position(|&unit| unit == b')' as u16)
                    .map(|relative| variable_start + relative)
                    .ok_or_else(|| "unterminated macro variable".to_owned())?;
                let variable = JavaString::from_units(input[variable_start..variable_end].to_vec());
                if !variable.units().iter().copied().all(is_variable_unit) {
                    return Err(format!(
                        "invalid macro variable name `{}`",
                        variable.to_string_lossy()
                    ));
                }
                variables.push(variable);
                start = variable_end + 1;
                search = start;
            } else {
                search = index + 1;
            }
        }

        if start == 0 {
            return Err("macro line contains no variables".to_owned());
        }
        if start != input.len() {
            segments.push(JavaString::from_units(input[start..].to_vec()));
        }
        Ok((
            Self {
                segments,
                variable_count: variables.len(),
            },
            variables,
        ))
    }

    fn substitute(&self, arguments: &[&JavaString]) -> Result<Vec<u16>, String> {
        debug_assert_eq!(arguments.len(), self.variable_count);
        let mut result = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            result.extend_from_slice(self.segments[index].units());
            result.extend_from_slice(argument.units());
            check_command_length(result.len())?;
        }
        if self.segments.len() > self.variable_count {
            result.extend_from_slice(
                self.segments
                    .last()
                    .expect("a trailing segment is present")
                    .units(),
            );
        }
        check_command_length(result.len())?;
        Ok(result)
    }
}

fn is_variable_unit(unit: u16) -> bool {
    if unit == b'_' as u16 {
        return true;
    }
    let Some(character) = char::from_u32(u32::from(unit)) else {
        return false;
    };
    matches!(
        get_general_category(character),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
            | GeneralCategory::DecimalNumber
    )
}

fn check_command_length(length: usize) -> Result<(), String> {
    if length > MAX_COMMAND_LENGTH {
        Err(format!(
            "expanded command is {length} UTF-16 code units; maximum is {MAX_COMMAND_LENGTH}"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_variable_names_and_literal_dollars() {
        let (template, variables) = StringTemplate::parse("return $$x $(A1_)").unwrap();
        assert_eq!(variables, vec![JavaString::from("A1_")]);
        assert_eq!(
            template.substitute(&[&JavaString::from("7")]).unwrap(),
            "return $$x 7".encode_utf16().collect::<Vec<_>>()
        );
        assert!(
            StringTemplate::parse("return $()")
                .is_ok_and(|(_, variables)| variables == vec![JavaString::from("")])
        );
        assert!(StringTemplate::parse("return $(𐐀)").is_err());
    }
}
