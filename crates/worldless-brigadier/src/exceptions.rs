use std::{
    any::Any,
    cell::RefCell,
    error::Error,
    fmt,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    message::{LiteralMessage, Message, MessageRef},
    reader::ImmutableStringReader,
};

pub const CONTEXT_AMOUNT: usize = 10;

const DOUBLE_TOO_LOW: u64 = 1;
const DOUBLE_TOO_HIGH: u64 = 2;
const FLOAT_TOO_LOW: u64 = 3;
const FLOAT_TOO_HIGH: u64 = 4;
const INTEGER_TOO_LOW: u64 = 5;
const INTEGER_TOO_HIGH: u64 = 6;
const LONG_TOO_LOW: u64 = 7;
const LONG_TOO_HIGH: u64 = 8;
const LITERAL_INCORRECT: u64 = 9;
const READER_EXPECTED_START_OF_QUOTE: u64 = 10;
const READER_EXPECTED_END_OF_QUOTE: u64 = 11;
const READER_INVALID_ESCAPE: u64 = 12;
const READER_INVALID_BOOL: u64 = 13;
const READER_INVALID_INT: u64 = 14;
const READER_EXPECTED_INT: u64 = 15;
const READER_INVALID_LONG: u64 = 16;
const READER_EXPECTED_LONG: u64 = 17;
const READER_INVALID_DOUBLE: u64 = 18;
const READER_EXPECTED_DOUBLE: u64 = 19;
const READER_INVALID_FLOAT: u64 = 20;
const READER_EXPECTED_FLOAT: u64 = 21;
const READER_EXPECTED_BOOL: u64 = 22;
const READER_EXPECTED_SYMBOL: u64 = 23;
const DISPATCHER_UNKNOWN_COMMAND: u64 = 24;
const DISPATCHER_UNKNOWN_ARGUMENT: u64 = 25;
const DISPATCHER_EXPECTED_ARGUMENT_SEPARATOR: u64 = 26;
const DISPATCHER_PARSE_EXCEPTION: u64 = 27;
const FIRST_CUSTOM_EXCEPTION_TYPE: u64 = 1 << 32;

static NEXT_EXCEPTION_TYPE: AtomicU64 = AtomicU64::new(FIRST_CUSTOM_EXCEPTION_TYPE);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExceptionTypeId(u64);

impl ExceptionTypeId {
    pub fn custom() -> Self {
        Self(NEXT_EXCEPTION_TYPE.fetch_add(1, Ordering::Relaxed))
    }

    const fn built_in(value: u64) -> Self {
        Self(value)
    }
}

pub trait CommandExceptionType {
    fn identity(&self) -> ExceptionTypeId;
}

#[derive(Clone)]
pub struct CommandSyntaxException {
    exception_type: ExceptionTypeId,
    raw_message: MessageRef,
    input: Option<Rc<[u16]>>,
    cursor: isize,
}

impl CommandSyntaxException {
    pub fn new(exception_type: ExceptionTypeId, message: MessageRef) -> Self {
        Self {
            exception_type,
            raw_message: message,
            input: None,
            cursor: -1,
        }
    }

    pub fn with_context(
        exception_type: ExceptionTypeId,
        message: MessageRef,
        reader: &impl ImmutableStringReader,
    ) -> Self {
        Self {
            exception_type,
            raw_message: message,
            input: Some(Rc::from(reader.utf16())),
            cursor: reader.cursor() as isize,
        }
    }

    pub fn with_input_utf16(
        exception_type: ExceptionTypeId,
        message: MessageRef,
        input: Vec<u16>,
        cursor: isize,
    ) -> Self {
        Self {
            exception_type,
            raw_message: message,
            input: Some(input.into()),
            cursor,
        }
    }

    pub fn exception_type(&self) -> ExceptionTypeId {
        self.exception_type
    }

    pub fn is_type(&self, exception_type: &impl CommandExceptionType) -> bool {
        self.exception_type == exception_type.identity()
    }

    pub fn raw_message(&self) -> &dyn Message {
        self.raw_message.as_ref()
    }

    pub fn input(&self) -> Option<String> {
        self.input.as_deref().map(String::from_utf16_lossy)
    }

    pub fn input_utf16(&self) -> Option<&[u16]> {
        self.input.as_deref()
    }

    pub fn cursor(&self) -> isize {
        self.cursor
    }

    pub fn context(&self) -> Option<String> {
        self.context_utf16()
            .map(|context| String::from_utf16_lossy(&context))
    }

    pub fn context_utf16(&self) -> Option<Vec<u16>> {
        let input = self.input.as_deref()?;
        if self.cursor < 0 {
            return None;
        }
        let cursor = usize::min(input.len(), self.cursor as usize);
        let mut result = Vec::new();
        if cursor > CONTEXT_AMOUNT {
            result.extend("...".encode_utf16());
        }
        result.extend_from_slice(&input[cursor.saturating_sub(CONTEXT_AMOUNT)..cursor]);
        result.extend("<--[HERE]".encode_utf16());
        Some(result)
    }
}

impl fmt::Debug for CommandSyntaxException {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommandSyntaxException")
            .field("exception_type", &self.exception_type)
            .field("raw_message", &self.raw_message.string())
            .field("input", &self.input())
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl fmt::Display for CommandSyntaxException {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw_message.string())?;
        if let Some(context) = self.context() {
            write!(formatter, " at position {}: {context}", self.cursor)?;
        }
        Ok(())
    }
}

impl Error for CommandSyntaxException {}

pub trait IntoMessageRef {
    fn into_message_ref(self) -> MessageRef;
}

impl<M> IntoMessageRef for M
where
    M: Message + 'static,
{
    fn into_message_ref(self) -> MessageRef {
        Rc::new(self)
    }
}

impl IntoMessageRef for MessageRef {
    fn into_message_ref(self) -> MessageRef {
        self
    }
}

#[derive(Clone)]
pub struct SimpleCommandExceptionType {
    identity: ExceptionTypeId,
    message: MessageRef,
}

impl SimpleCommandExceptionType {
    pub fn new(message: impl IntoMessageRef) -> Self {
        Self {
            identity: ExceptionTypeId::custom(),
            message: message.into_message_ref(),
        }
    }

    fn built_in(identity: u64, message: &'static str) -> Self {
        Self {
            identity: ExceptionTypeId::built_in(identity),
            message: Rc::new(LiteralMessage::new(message)),
        }
    }

    pub fn create(&self) -> CommandSyntaxException {
        CommandSyntaxException::new(self.identity, Rc::clone(&self.message))
    }

    pub fn create_with_context(
        &self,
        reader: &impl ImmutableStringReader,
    ) -> CommandSyntaxException {
        CommandSyntaxException::with_context(self.identity, Rc::clone(&self.message), reader)
    }
}

impl CommandExceptionType for SimpleCommandExceptionType {
    fn identity(&self) -> ExceptionTypeId {
        self.identity
    }
}

impl fmt::Debug for SimpleCommandExceptionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SimpleCommandExceptionType")
            .field("identity", &self.identity)
            .field("message", &self.message.string())
            .finish()
    }
}

impl fmt::Display for SimpleCommandExceptionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message.string())
    }
}

impl PartialEq for SimpleCommandExceptionType {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for SimpleCommandExceptionType {}

macro_rules! dynamic_exception_type {
    ($name:ident, ($($argument:ident : $type_parameter:ident),+)) => {
        pub struct $name<$($type_parameter),+> {
            identity: ExceptionTypeId,
            function: Rc<dyn Fn($($type_parameter),+) -> MessageRef>,
        }

        impl<$($type_parameter),+> Clone for $name<$($type_parameter),+> {
            fn clone(&self) -> Self {
                Self {
                    identity: self.identity,
                    function: Rc::clone(&self.function),
                }
            }
        }

        impl<$($type_parameter: 'static),+> $name<$($type_parameter),+> {
            pub fn new<F, M>(function: F) -> Self
            where
                F: Fn($($type_parameter),+) -> M + 'static,
                M: IntoMessageRef,
            {
                Self {
                    identity: ExceptionTypeId::custom(),
                    function: Rc::new(move |$($argument),+| {
                        function($($argument),+).into_message_ref()
                    }),
                }
            }

            #[allow(dead_code)]
            fn built_in<F, M>(identity: u64, function: F) -> Self
            where
                F: Fn($($type_parameter),+) -> M + 'static,
                M: IntoMessageRef,
            {
                Self {
                    identity: ExceptionTypeId::built_in(identity),
                    function: Rc::new(move |$($argument),+| {
                        function($($argument),+).into_message_ref()
                    }),
                }
            }

            pub fn create(&self, $($argument: $type_parameter),+) -> CommandSyntaxException {
                CommandSyntaxException::new(self.identity, (self.function)($($argument),+))
            }

            pub fn create_with_context(
                &self,
                reader: &impl ImmutableStringReader,
                $($argument: $type_parameter),+
            ) -> CommandSyntaxException {
                CommandSyntaxException::with_context(
                    self.identity,
                    (self.function)($($argument),+),
                    reader,
                )
            }
        }

        impl<$($type_parameter),+> CommandExceptionType for $name<$($type_parameter),+> {
            fn identity(&self) -> ExceptionTypeId {
                self.identity
            }
        }

        impl<$($type_parameter),+> fmt::Debug for $name<$($type_parameter),+> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("identity", &self.identity)
                    .finish_non_exhaustive()
            }
        }

        impl<$($type_parameter),+> PartialEq for $name<$($type_parameter),+> {
            fn eq(&self, other: &Self) -> bool {
                self.identity == other.identity
            }
        }

        impl<$($type_parameter),+> Eq for $name<$($type_parameter),+> {}
    };
}

dynamic_exception_type!(DynamicCommandExceptionType, (argument: A));
dynamic_exception_type!(Dynamic2CommandExceptionType, (first: A, second: B));
dynamic_exception_type!(Dynamic3CommandExceptionType, (first: A, second: B, third: C));
dynamic_exception_type!(Dynamic4CommandExceptionType, (first: A, second: B, third: C, fourth: D));

pub type DynamicArguments = Vec<Rc<dyn Any>>;
type DynamicNFunction = Rc<dyn Fn(DynamicArguments) -> MessageRef>;

pub struct DynamicNCommandExceptionType {
    identity: ExceptionTypeId,
    function: DynamicNFunction,
}

impl Clone for DynamicNCommandExceptionType {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity,
            function: Rc::clone(&self.function),
        }
    }
}

impl DynamicNCommandExceptionType {
    pub fn new<F, M>(function: F) -> Self
    where
        F: Fn(DynamicArguments) -> M + 'static,
        M: IntoMessageRef,
    {
        Self {
            identity: ExceptionTypeId::custom(),
            function: Rc::new(move |arguments| function(arguments).into_message_ref()),
        }
    }

    pub fn create(
        &self,
        _first_argument: Rc<dyn Any>,
        arguments: DynamicArguments,
    ) -> CommandSyntaxException {
        CommandSyntaxException::new(self.identity, (self.function)(arguments))
    }

    pub fn create_with_context(
        &self,
        reader: &impl ImmutableStringReader,
        arguments: DynamicArguments,
    ) -> CommandSyntaxException {
        CommandSyntaxException::with_context(self.identity, (self.function)(arguments), reader)
    }
}

impl CommandExceptionType for DynamicNCommandExceptionType {
    fn identity(&self) -> ExceptionTypeId {
        self.identity
    }
}

impl fmt::Debug for DynamicNCommandExceptionType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicNCommandExceptionType")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DynamicNCommandExceptionType {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for DynamicNCommandExceptionType {}

pub trait BuiltInExceptionProvider: 'static {
    fn double_too_low(&self) -> Dynamic2CommandExceptionType<f64, f64>;
    fn double_too_high(&self) -> Dynamic2CommandExceptionType<f64, f64>;
    fn float_too_low(&self) -> Dynamic2CommandExceptionType<f32, f32>;
    fn float_too_high(&self) -> Dynamic2CommandExceptionType<f32, f32>;
    fn integer_too_low(&self) -> Dynamic2CommandExceptionType<i32, i32>;
    fn integer_too_high(&self) -> Dynamic2CommandExceptionType<i32, i32>;
    fn long_too_low(&self) -> Dynamic2CommandExceptionType<i64, i64>;
    fn long_too_high(&self) -> Dynamic2CommandExceptionType<i64, i64>;
    fn literal_incorrect(&self) -> DynamicCommandExceptionType<String>;
    fn reader_expected_start_of_quote(&self) -> SimpleCommandExceptionType;
    fn reader_expected_end_of_quote(&self) -> SimpleCommandExceptionType;
    fn reader_invalid_escape(&self) -> DynamicCommandExceptionType<String>;
    fn reader_invalid_bool(&self) -> DynamicCommandExceptionType<String>;
    fn reader_invalid_int(&self) -> DynamicCommandExceptionType<String>;
    fn reader_expected_int(&self) -> SimpleCommandExceptionType;
    fn reader_invalid_long(&self) -> DynamicCommandExceptionType<String>;
    fn reader_expected_long(&self) -> SimpleCommandExceptionType;
    fn reader_invalid_double(&self) -> DynamicCommandExceptionType<String>;
    fn reader_expected_double(&self) -> SimpleCommandExceptionType;
    fn reader_invalid_float(&self) -> DynamicCommandExceptionType<String>;
    fn reader_expected_float(&self) -> SimpleCommandExceptionType;
    fn reader_expected_bool(&self) -> SimpleCommandExceptionType;
    fn reader_expected_symbol(&self) -> DynamicCommandExceptionType<String>;
    fn dispatcher_unknown_command(&self) -> SimpleCommandExceptionType;
    fn dispatcher_unknown_argument(&self) -> SimpleCommandExceptionType;
    fn dispatcher_expected_argument_separator(&self) -> SimpleCommandExceptionType;
    fn dispatcher_parse_exception(&self) -> DynamicCommandExceptionType<String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInExceptions;

#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltInExceptionsAccessor;

pub const BUILT_IN_EXCEPTIONS: BuiltInExceptionsAccessor = BuiltInExceptionsAccessor;

thread_local! {
    static CURRENT_BUILT_IN_EXCEPTIONS: RefCell<Rc<dyn BuiltInExceptionProvider>> =
        RefCell::new(Rc::new(BuiltInExceptions));
}

/// Replaces the built-in exception provider for the current VM thread.
///
/// The previous provider is returned so scoped integrations can restore it.
pub fn set_built_in_exceptions(
    provider: Rc<dyn BuiltInExceptionProvider>,
) -> Rc<dyn BuiltInExceptionProvider> {
    CURRENT_BUILT_IN_EXCEPTIONS.with(|current| current.replace(provider))
}

fn with_built_in_exceptions<T>(action: impl FnOnce(&dyn BuiltInExceptionProvider) -> T) -> T {
    CURRENT_BUILT_IN_EXCEPTIONS.with(|current| {
        let provider = current.borrow().clone();
        action(provider.as_ref())
    })
}

impl BuiltInExceptionProvider for BuiltInExceptionsAccessor {
    fn double_too_low(&self) -> Dynamic2CommandExceptionType<f64, f64> {
        with_built_in_exceptions(|provider| provider.double_too_low())
    }

    fn double_too_high(&self) -> Dynamic2CommandExceptionType<f64, f64> {
        with_built_in_exceptions(|provider| provider.double_too_high())
    }

    fn float_too_low(&self) -> Dynamic2CommandExceptionType<f32, f32> {
        with_built_in_exceptions(|provider| provider.float_too_low())
    }

    fn float_too_high(&self) -> Dynamic2CommandExceptionType<f32, f32> {
        with_built_in_exceptions(|provider| provider.float_too_high())
    }

    fn integer_too_low(&self) -> Dynamic2CommandExceptionType<i32, i32> {
        with_built_in_exceptions(|provider| provider.integer_too_low())
    }

    fn integer_too_high(&self) -> Dynamic2CommandExceptionType<i32, i32> {
        with_built_in_exceptions(|provider| provider.integer_too_high())
    }

    fn long_too_low(&self) -> Dynamic2CommandExceptionType<i64, i64> {
        with_built_in_exceptions(|provider| provider.long_too_low())
    }

    fn long_too_high(&self) -> Dynamic2CommandExceptionType<i64, i64> {
        with_built_in_exceptions(|provider| provider.long_too_high())
    }

    fn literal_incorrect(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.literal_incorrect())
    }

    fn reader_expected_start_of_quote(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_start_of_quote())
    }

    fn reader_expected_end_of_quote(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_end_of_quote())
    }

    fn reader_invalid_escape(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_escape())
    }

    fn reader_invalid_bool(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_bool())
    }

    fn reader_invalid_int(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_int())
    }

    fn reader_expected_int(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_int())
    }

    fn reader_invalid_long(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_long())
    }

    fn reader_expected_long(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_long())
    }

    fn reader_invalid_double(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_double())
    }

    fn reader_expected_double(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_double())
    }

    fn reader_invalid_float(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_invalid_float())
    }

    fn reader_expected_float(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_float())
    }

    fn reader_expected_bool(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.reader_expected_bool())
    }

    fn reader_expected_symbol(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.reader_expected_symbol())
    }

    fn dispatcher_unknown_command(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.dispatcher_unknown_command())
    }

    fn dispatcher_unknown_argument(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.dispatcher_unknown_argument())
    }

    fn dispatcher_expected_argument_separator(&self) -> SimpleCommandExceptionType {
        with_built_in_exceptions(|provider| provider.dispatcher_expected_argument_separator())
    }

    fn dispatcher_parse_exception(&self) -> DynamicCommandExceptionType<String> {
        with_built_in_exceptions(|provider| provider.dispatcher_parse_exception())
    }
}

impl BuiltInExceptionProvider for BuiltInExceptions {
    fn double_too_low(&self) -> Dynamic2CommandExceptionType<f64, f64> {
        Dynamic2CommandExceptionType::built_in(DOUBLE_TOO_LOW, |found, minimum| {
            LiteralMessage::new(format!(
                "Double must not be less than {}, found {}",
                java_f64(minimum),
                java_f64(found)
            ))
        })
    }

    fn double_too_high(&self) -> Dynamic2CommandExceptionType<f64, f64> {
        Dynamic2CommandExceptionType::built_in(DOUBLE_TOO_HIGH, |found, maximum| {
            LiteralMessage::new(format!(
                "Double must not be more than {}, found {}",
                java_f64(maximum),
                java_f64(found)
            ))
        })
    }

    fn float_too_low(&self) -> Dynamic2CommandExceptionType<f32, f32> {
        Dynamic2CommandExceptionType::built_in(FLOAT_TOO_LOW, |found, minimum| {
            LiteralMessage::new(format!(
                "Float must not be less than {}, found {}",
                java_f32(minimum),
                java_f32(found)
            ))
        })
    }

    fn float_too_high(&self) -> Dynamic2CommandExceptionType<f32, f32> {
        Dynamic2CommandExceptionType::built_in(FLOAT_TOO_HIGH, |found, maximum| {
            LiteralMessage::new(format!(
                "Float must not be more than {}, found {}",
                java_f32(maximum),
                java_f32(found)
            ))
        })
    }

    fn integer_too_low(&self) -> Dynamic2CommandExceptionType<i32, i32> {
        Dynamic2CommandExceptionType::built_in(INTEGER_TOO_LOW, |found, minimum| {
            LiteralMessage::new(format!(
                "Integer must not be less than {minimum}, found {found}"
            ))
        })
    }

    fn integer_too_high(&self) -> Dynamic2CommandExceptionType<i32, i32> {
        Dynamic2CommandExceptionType::built_in(INTEGER_TOO_HIGH, |found, maximum| {
            LiteralMessage::new(format!(
                "Integer must not be more than {maximum}, found {found}"
            ))
        })
    }

    fn long_too_low(&self) -> Dynamic2CommandExceptionType<i64, i64> {
        Dynamic2CommandExceptionType::built_in(LONG_TOO_LOW, |found, minimum| {
            LiteralMessage::new(format!(
                "Long must not be less than {minimum}, found {found}"
            ))
        })
    }

    fn long_too_high(&self) -> Dynamic2CommandExceptionType<i64, i64> {
        Dynamic2CommandExceptionType::built_in(LONG_TOO_HIGH, |found, maximum| {
            LiteralMessage::new(format!(
                "Long must not be more than {maximum}, found {found}"
            ))
        })
    }

    fn literal_incorrect(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(LITERAL_INCORRECT, |expected| {
            LiteralMessage::new(format!("Expected literal {expected}"))
        })
    }

    fn reader_expected_start_of_quote(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(
            READER_EXPECTED_START_OF_QUOTE,
            "Expected quote to start a string",
        )
    }

    fn reader_expected_end_of_quote(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_END_OF_QUOTE, "Unclosed quoted string")
    }

    fn reader_invalid_escape(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_ESCAPE, |character| {
            LiteralMessage::new(format!(
                "Invalid escape sequence '{character}' in quoted string"
            ))
        })
    }

    fn reader_invalid_bool(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_BOOL, |value| {
            LiteralMessage::new(format!(
                "Invalid bool, expected true or false but found '{value}'"
            ))
        })
    }

    fn reader_invalid_int(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_INT, |value| {
            LiteralMessage::new(format!("Invalid integer '{value}'"))
        })
    }

    fn reader_expected_int(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_INT, "Expected integer")
    }

    fn reader_invalid_long(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_LONG, |value| {
            LiteralMessage::new(format!("Invalid long '{value}'"))
        })
    }

    fn reader_expected_long(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_LONG, "Expected long")
    }

    fn reader_invalid_double(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_DOUBLE, |value| {
            LiteralMessage::new(format!("Invalid double '{value}'"))
        })
    }

    fn reader_expected_double(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_DOUBLE, "Expected double")
    }

    fn reader_invalid_float(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_INVALID_FLOAT, |value| {
            LiteralMessage::new(format!("Invalid float '{value}'"))
        })
    }

    fn reader_expected_float(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_FLOAT, "Expected float")
    }

    fn reader_expected_bool(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(READER_EXPECTED_BOOL, "Expected bool")
    }

    fn reader_expected_symbol(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(READER_EXPECTED_SYMBOL, |symbol| {
            LiteralMessage::new(format!("Expected '{symbol}'"))
        })
    }

    fn dispatcher_unknown_command(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(DISPATCHER_UNKNOWN_COMMAND, "Unknown command")
    }

    fn dispatcher_unknown_argument(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(
            DISPATCHER_UNKNOWN_ARGUMENT,
            "Incorrect argument for command",
        )
    }

    fn dispatcher_expected_argument_separator(&self) -> SimpleCommandExceptionType {
        SimpleCommandExceptionType::built_in(
            DISPATCHER_EXPECTED_ARGUMENT_SEPARATOR,
            "Expected whitespace to end one argument, but found trailing data",
        )
    }

    fn dispatcher_parse_exception(&self) -> DynamicCommandExceptionType<String> {
        DynamicCommandExceptionType::built_in(DISPATCHER_PARSE_EXCEPTION, |message| {
            LiteralMessage::new(format!("Could not parse command: {message}"))
        })
    }
}

pub(crate) fn java_f32(value: f32) -> String {
    java_floating(value as f64, value.to_string(), |candidate| {
        candidate
            .parse::<f32>()
            .is_ok_and(|parsed| parsed.to_bits() == value.abs().to_bits())
    })
}

pub(crate) fn java_f64(value: f64) -> String {
    java_floating(value, value.to_string(), |candidate| {
        candidate
            .parse::<f64>()
            .is_ok_and(|parsed| parsed.to_bits() == value.abs().to_bits())
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DecimalCandidate {
    significand: u64,
    exponent: i32,
}

fn java_floating(value: f64, rust: String, parses_as_value: impl Fn(&str) -> bool) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value == f64::INFINITY {
        return "Infinity".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        };
    }

    let negative = value.is_sign_negative();
    let unsigned = rust.strip_prefix('-').unwrap_or(&rust);
    let (mantissa, parsed_exponent) =
        unsigned
            .split_once(['e', 'E'])
            .map_or((unsigned, 0_i32), |(mantissa, exponent)| {
                (
                    mantissa,
                    exponent.parse::<i32>().expect("Rust emitted an exponent"),
                )
            });
    let decimal_position = mantissa
        .find('.')
        .map_or(mantissa.len() as i32, |position| position as i32)
        + parsed_exponent;
    let mut digits = mantissa
        .chars()
        .filter(|character| *character != '.')
        .collect::<String>();
    let leading_zeroes = digits.chars().take_while(|digit| *digit == '0').count();
    digits.drain(..leading_zeroes);
    let scientific_exponent = decimal_position - leading_zeroes as i32 - 1;
    while digits.ends_with('0') {
        digits.pop();
    }

    let shortest = DecimalCandidate {
        significand: digits
            .parse()
            .expect("Rust emitted at most 17 significant decimal digits"),
        exponent: scientific_exponent + 1 - digits.len() as i32,
    };
    let candidate = closest_java_candidate(value.abs(), shortest, parses_as_value);
    format_java_decimal(negative, candidate)
}

fn closest_java_candidate(
    value: f64,
    shortest: DecimalCandidate,
    parses_as_value: impl Fn(&str) -> bool,
) -> DecimalCandidate {
    let digit_count = decimal_digit_count(shortest.significand);
    let mut candidates = Vec::new();
    if digit_count == 1 {
        add_candidates(
            shortest.significand,
            shortest.exponent,
            1,
            9,
            &parses_as_value,
            &mut candidates,
        );
        add_candidates(
            shortest.significand * 10,
            shortest.exponent - 1,
            10,
            99,
            &parses_as_value,
            &mut candidates,
        );
    } else {
        let minimum = 10_u64.pow(digit_count - 1);
        let maximum = 10_u64.pow(digit_count) - 1;
        add_candidates(
            shortest.significand,
            shortest.exponent,
            minimum,
            maximum,
            &parses_as_value,
            &mut candidates,
        );
    }
    assert!(
        !candidates.is_empty(),
        "the Rust shortest decimal must parse back to its source float"
    );
    if candidates.len() == 1 {
        return candidates[0];
    }

    let (exact_digits, exact_exponent) = exact_decimal(value);
    let mut best = candidates[0];
    let mut best_distance = decimal_distance(exact_digits.as_slice(), exact_exponent, best);
    for candidate in candidates.into_iter().skip(1) {
        let distance = decimal_distance(exact_digits.as_slice(), exact_exponent, candidate);
        match compare_decimal_integers(&distance, &best_distance) {
            std::cmp::Ordering::Less => {
                best = candidate;
                best_distance = distance;
            }
            std::cmp::Ordering::Equal
                if candidate.significand % 2 == 0 && best.significand % 2 != 0 =>
            {
                best = candidate;
                best_distance = distance;
            }
            _ => {}
        }
    }
    best
}

fn add_candidates(
    center: u64,
    exponent: i32,
    minimum: u64,
    maximum: u64,
    parses_as_value: &impl Fn(&str) -> bool,
    candidates: &mut Vec<DecimalCandidate>,
) {
    let start = center.saturating_sub(20).max(minimum);
    let end = center.saturating_add(20).min(maximum);
    for significand in start..=end {
        let parseable = format!("{significand}e{exponent}");
        if parses_as_value(&parseable) {
            let candidate = normalize_candidate(DecimalCandidate {
                significand,
                exponent,
            });
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
}

fn normalize_candidate(mut candidate: DecimalCandidate) -> DecimalCandidate {
    while candidate.significand.is_multiple_of(10) {
        candidate.significand /= 10;
        candidate.exponent += 1;
    }
    candidate
}

fn decimal_digit_count(value: u64) -> u32 {
    value.ilog10() + 1
}

fn exact_decimal(value: f64) -> (Vec<u8>, i32) {
    let bits = value.to_bits();
    let biased_exponent = ((bits >> 52) & 0x7ff) as i32;
    let mut significand = bits & ((1_u64 << 52) - 1);
    let mut binary_exponent;
    if biased_exponent == 0 {
        binary_exponent = -1074;
    } else {
        significand |= 1_u64 << 52;
        binary_exponent = biased_exponent - 1023 - 52;
    }
    if binary_exponent < 0 {
        binary_exponent += i32::min(significand.trailing_zeros() as i32, -binary_exponent);
    }
    let scale = (-binary_exponent).max(0) as usize;
    let exact = format!("{value:.scale$}");
    let digits = exact
        .bytes()
        .filter(|byte| byte.is_ascii_digit())
        .skip_while(|byte| *byte == b'0')
        .map(|byte| byte - b'0')
        .collect::<Vec<_>>();
    assert!(
        !digits.is_empty(),
        "nonzero float has a nonzero exact decimal"
    );
    (digits, -(scale as i32))
}

fn decimal_distance(
    exact_digits: &[u8],
    exact_exponent: i32,
    candidate: DecimalCandidate,
) -> Vec<u8> {
    let common_exponent = i32::min(exact_exponent, candidate.exponent);
    let mut exact = exact_digits.to_vec();
    exact.extend(std::iter::repeat_n(
        0,
        (exact_exponent - common_exponent) as usize,
    ));
    let mut decimal = candidate
        .significand
        .to_string()
        .bytes()
        .map(|byte| byte - b'0')
        .collect::<Vec<_>>();
    decimal.extend(std::iter::repeat_n(
        0,
        (candidate.exponent - common_exponent) as usize,
    ));
    subtract_decimal_integers(&exact, &decimal)
}

fn subtract_decimal_integers(left: &[u8], right: &[u8]) -> Vec<u8> {
    let width = usize::max(left.len(), right.len());
    let left_offset = width - left.len();
    let right_offset = width - right.len();
    let left_is_larger = compare_decimal_integers(left, right) != std::cmp::Ordering::Less;
    let (larger, larger_offset, smaller, smaller_offset) = if left_is_larger {
        (left, left_offset, right, right_offset)
    } else {
        (right, right_offset, left, left_offset)
    };
    let mut result = vec![0; width];
    let mut borrow = 0_i16;
    for position in (0..width).rev() {
        let large_digit = position
            .checked_sub(larger_offset)
            .and_then(|index| larger.get(index))
            .copied()
            .unwrap_or(0) as i16;
        let small_digit = position
            .checked_sub(smaller_offset)
            .and_then(|index| smaller.get(index))
            .copied()
            .unwrap_or(0) as i16;
        let mut difference = large_digit - small_digit - borrow;
        if difference < 0 {
            difference += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        result[position] = difference as u8;
    }
    let first_nonzero = result.iter().position(|digit| *digit != 0);
    first_nonzero.map_or_else(|| vec![0], |start| result[start..].to_vec())
}

fn compare_decimal_integers(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let left = &left[left
        .iter()
        .position(|digit| *digit != 0)
        .unwrap_or(left.len())..];
    let right = &right[right
        .iter()
        .position(|digit| *digit != 0)
        .unwrap_or(right.len())..];
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn format_java_decimal(negative: bool, candidate: DecimalCandidate) -> String {
    let digits = candidate.significand.to_string();
    let exponent = digits.len() as i32 + candidate.exponent - 1;

    let mut result = String::new();
    if negative {
        result.push('-');
    }
    if (-3..7).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            result.push_str("0.");
            result.extend(std::iter::repeat_n('0', (-point) as usize));
            result.push_str(&digits);
        } else if point as usize >= digits.len() {
            result.push_str(&digits);
            result.extend(std::iter::repeat_n('0', point as usize - digits.len()));
            result.push_str(".0");
        } else {
            result.push_str(&digits[..point as usize]);
            result.push('.');
            result.push_str(&digits[point as usize..]);
        }
    } else {
        result.push(digits.as_bytes()[0] as char);
        result.push('.');
        if digits.len() == 1 {
            result.push('0');
        } else {
            result.push_str(&digits[1..]);
        }
        result.push('E');
        result.push_str(&exponent.to_string());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        arguments::{ArgumentType, IntegerArgumentType},
        dispatcher::CommandDispatcher,
        reader::StringReader,
    };

    struct ReplacementProvider;

    macro_rules! delegate_to_default {
        ($(fn $name:ident() -> $return:ty;)+) => {
            $(
                fn $name(&self) -> $return {
                    BuiltInExceptions.$name()
                }
            )+
        };
    }

    impl BuiltInExceptionProvider for ReplacementProvider {
        delegate_to_default! {
            fn double_too_low() -> Dynamic2CommandExceptionType<f64, f64>;
            fn double_too_high() -> Dynamic2CommandExceptionType<f64, f64>;
            fn float_too_low() -> Dynamic2CommandExceptionType<f32, f32>;
            fn float_too_high() -> Dynamic2CommandExceptionType<f32, f32>;
            fn integer_too_low() -> Dynamic2CommandExceptionType<i32, i32>;
            fn integer_too_high() -> Dynamic2CommandExceptionType<i32, i32>;
            fn long_too_low() -> Dynamic2CommandExceptionType<i64, i64>;
            fn long_too_high() -> Dynamic2CommandExceptionType<i64, i64>;
            fn literal_incorrect() -> DynamicCommandExceptionType<String>;
            fn reader_expected_start_of_quote() -> SimpleCommandExceptionType;
            fn reader_expected_end_of_quote() -> SimpleCommandExceptionType;
            fn reader_invalid_escape() -> DynamicCommandExceptionType<String>;
            fn reader_invalid_bool() -> DynamicCommandExceptionType<String>;
            fn reader_invalid_int() -> DynamicCommandExceptionType<String>;
            fn reader_invalid_long() -> DynamicCommandExceptionType<String>;
            fn reader_expected_long() -> SimpleCommandExceptionType;
            fn reader_invalid_double() -> DynamicCommandExceptionType<String>;
            fn reader_expected_double() -> SimpleCommandExceptionType;
            fn reader_invalid_float() -> DynamicCommandExceptionType<String>;
            fn reader_expected_float() -> SimpleCommandExceptionType;
            fn reader_expected_bool() -> SimpleCommandExceptionType;
            fn reader_expected_symbol() -> DynamicCommandExceptionType<String>;
            fn dispatcher_unknown_argument() -> SimpleCommandExceptionType;
            fn dispatcher_expected_argument_separator() -> SimpleCommandExceptionType;
            fn dispatcher_parse_exception() -> DynamicCommandExceptionType<String>;
        }

        fn reader_expected_int(&self) -> SimpleCommandExceptionType {
            SimpleCommandExceptionType::new(LiteralMessage::new("Replacement expected integer"))
        }

        fn dispatcher_unknown_command(&self) -> SimpleCommandExceptionType {
            SimpleCommandExceptionType::new(LiteralMessage::new("Replacement unknown command"))
        }
    }

    struct RestoreProvider(Option<Rc<dyn BuiltInExceptionProvider>>);

    impl Drop for RestoreProvider {
        fn drop(&mut self) {
            set_built_in_exceptions(self.0.take().expect("provider is restored once"));
        }
    }

    #[test]
    fn simple_create_with_context() {
        let exception_type = SimpleCommandExceptionType::new(LiteralMessage::new("error"));
        let mut reader = StringReader::new("Foo bar");
        reader.set_cursor(5);
        let exception = exception_type.create_with_context(&reader);
        assert!(exception.is_type(&exception_type));
        assert_eq!(exception.input().as_deref(), Some("Foo bar"));
        assert_eq!(exception.cursor(), 5);
    }

    #[test]
    fn context_none() {
        let exception_type = SimpleCommandExceptionType::new(LiteralMessage::new("error"));
        assert_eq!(exception_type.create().context(), None);
    }

    #[test]
    fn context_short() {
        let exception_type = SimpleCommandExceptionType::new(LiteralMessage::new("error"));
        let mut reader = StringReader::new("Hello world!");
        reader.set_cursor(5);
        assert_eq!(
            exception_type
                .create_with_context(&reader)
                .context()
                .as_deref(),
            Some("Hello<--[HERE]")
        );
    }

    #[test]
    fn context_long() {
        let exception_type = SimpleCommandExceptionType::new(LiteralMessage::new("error"));
        let mut reader = StringReader::new("Hello world! This has an error in it. Oh dear!");
        reader.set_cursor(20);
        assert_eq!(
            exception_type
                .create_with_context(&reader)
                .context()
                .as_deref(),
            Some("...d! This ha<--[HERE]")
        );
    }

    #[test]
    fn dynamic_create_with_context() {
        let exception_type = DynamicCommandExceptionType::new(|name: String| {
            LiteralMessage::new(format!("Hello, {name}!"))
        });
        let mut reader = StringReader::new("Foo bar");
        reader.set_cursor(5);
        let exception = exception_type.create_with_context(&reader, "World".to_owned());
        assert!(exception.is_type(&exception_type));
        assert_eq!(exception.input().as_deref(), Some("Foo bar"));
        assert_eq!(exception.cursor(), 5);
    }

    #[test]
    fn dynamic_n_create_discards_its_required_first_argument() {
        let exception_type = DynamicNCommandExceptionType::new(|arguments| {
            LiteralMessage::new(arguments.len().to_string())
        });
        let exception = exception_type.create(
            Rc::new("discarded") as Rc<dyn Any>,
            vec![Rc::new(1_i32), Rc::new(2_i32)],
        );
        assert_eq!(exception.raw_message().string(), "2");
    }

    #[test]
    fn built_in_identity_is_stable_across_accesses() {
        let first = BUILT_IN_EXCEPTIONS.reader_invalid_int();
        let second = BUILT_IN_EXCEPTIONS.reader_invalid_int();
        let different = BUILT_IN_EXCEPTIONS.reader_invalid_long();
        assert_eq!(first.identity(), second.identity());
        assert_ne!(first.identity(), different.identity());
    }

    #[test]
    fn built_in_provider_replacement_reaches_all_exception_call_sites() {
        let _restore = RestoreProvider(Some(set_built_in_exceptions(Rc::new(ReplacementProvider))));

        let reader_error = StringReader::new("").read_int().unwrap_err();
        assert_eq!(
            reader_error.raw_message().string(),
            "Replacement expected integer"
        );

        let argument_error = <IntegerArgumentType as ArgumentType<()>>::parse(
            &IntegerArgumentType::integer(),
            &mut StringReader::new(""),
        )
        .unwrap_err();
        assert_eq!(
            argument_error.raw_message().string(),
            "Replacement expected integer"
        );

        let dispatcher_error = CommandDispatcher::<()>::new()
            .execute("missing", ())
            .unwrap_err();
        assert_eq!(
            dispatcher_error.raw_message().string(),
            "Replacement unknown command"
        );
    }

    #[test]
    fn java_25_float_rendering() {
        assert_eq!(java_f32(i32::MIN as f32), "-2.1474836E9");
        assert_eq!(java_f32(f32::from_bits(0x49ee_577a)), "1952495.2");
        assert_eq!(java_f64(i32::MIN as f64), "-2.147483648E9");
        assert_eq!(java_f32(-100.0), "-100.0");
        assert_eq!(java_f64(100.0), "100.0");
        assert_eq!(java_f32(f32::MIN_POSITIVE), "1.1754944E-38");
        assert_eq!(java_f32(f32::from_bits(1)), "1.4E-45");
        assert_eq!(java_f32(f32::MAX), "3.4028235E38");
        assert_eq!(java_f32(0.001), "0.001");
        assert_eq!(java_f32(9_999_999.0), "9999999.0");
        assert_eq!(java_f32(10_000_000.0), "1.0E7");
        assert_eq!(java_f64(f64::MIN_POSITIVE), "2.2250738585072014E-308");
        assert_eq!(java_f64(f64::from_bits(1)), "4.9E-324");
        assert_eq!(java_f64(f64::MAX), "1.7976931348623157E308");
        assert_eq!(java_f64(0.001), "0.001");
        assert_eq!(java_f64(9_999_999.0), "9999999.0");
        assert_eq!(java_f64(10_000_000.0), "1.0E7");
    }
}
