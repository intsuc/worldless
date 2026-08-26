use std::{
    any::{Any, TypeId, type_name},
    fmt,
    rc::Rc,
};

use crate::{
    context::{CommandContext, ContextError},
    exceptions::{
        BUILT_IN_EXCEPTIONS, BuiltInExceptionProvider, CommandSyntaxException, java_f32, java_f64,
    },
    reader::StringReader,
    suggestion::{Suggestions, SuggestionsBuilder, SuggestionsFuture},
};

pub trait ArgumentType<S: 'static>: Any {
    type Value: Any;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException>;

    fn parse_with_source(
        &self,
        reader: &mut StringReader,
        _source: &S,
    ) -> Result<Self::Value, CommandSyntaxException> {
        self.parse(reader)
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        _builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        Suggestions::empty_future()
    }

    fn examples(&self) -> Vec<String> {
        Vec::new()
    }

    fn display(&self) -> String {
        type_name::<Self>().to_owned()
    }

    fn equals(&self, _other: &Self) -> bool {
        false
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        std::ptr::eq(left, right)
    }
}

pub type ArgumentValueComparator = Rc<dyn Fn(&dyn Any, &dyn Any) -> bool>;

fn owned_examples(examples: &[&str]) -> Vec<String> {
    examples
        .iter()
        .map(|example| (*example).to_owned())
        .collect()
}

trait ErasedArgumentType<S: 'static>: Any {
    fn parse_erased(
        &self,
        reader: &mut StringReader,
    ) -> Result<Rc<dyn Any>, CommandSyntaxException>;

    fn parse_with_source_erased(
        &self,
        reader: &mut StringReader,
        source: &S,
    ) -> Result<Rc<dyn Any>, CommandSyntaxException>;

    fn list_suggestions_erased(
        &self,
        context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture;

    fn examples_erased(&self) -> Vec<String>;
    fn display_erased(&self) -> String;
    fn equals_erased(&self, other: &dyn ErasedArgumentType<S>) -> bool;
    fn value_equals_erased(&self, left: &dyn Any, right: &dyn Any) -> bool;
    fn as_any(&self) -> &dyn Any;
    fn argument_type_name(&self) -> &'static str;
    fn value_type_name(&self) -> &'static str;
    fn value_type_id(&self) -> TypeId;
}

impl<S, A> ErasedArgumentType<S> for A
where
    S: 'static,
    A: ArgumentType<S>,
{
    fn parse_erased(
        &self,
        reader: &mut StringReader,
    ) -> Result<Rc<dyn Any>, CommandSyntaxException> {
        self.parse(reader)
            .map(|value| Rc::new(value) as Rc<dyn Any>)
    }

    fn parse_with_source_erased(
        &self,
        reader: &mut StringReader,
        source: &S,
    ) -> Result<Rc<dyn Any>, CommandSyntaxException> {
        self.parse_with_source(reader, source)
            .map(|value| Rc::new(value) as Rc<dyn Any>)
    }

    fn list_suggestions_erased(
        &self,
        context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        self.list_suggestions(context, builder)
    }

    fn examples_erased(&self) -> Vec<String> {
        self.examples()
    }

    fn display_erased(&self) -> String {
        self.display()
    }

    fn equals_erased(&self, other: &dyn ErasedArgumentType<S>) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.equals(other))
    }

    fn value_equals_erased(&self, left: &dyn Any, right: &dyn Any) -> bool {
        let (Some(left), Some(right)) = (
            left.downcast_ref::<<A as ArgumentType<S>>::Value>(),
            right.downcast_ref::<<A as ArgumentType<S>>::Value>(),
        ) else {
            return false;
        };
        self.value_equals(left, right)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn argument_type_name(&self) -> &'static str {
        type_name::<Self>()
    }

    fn value_type_name(&self) -> &'static str {
        type_name::<<A as ArgumentType<S>>::Value>()
    }

    fn value_type_id(&self) -> TypeId {
        TypeId::of::<<A as ArgumentType<S>>::Value>()
    }
}

pub struct ArgumentTypeRef<S: 'static> {
    inner: Rc<dyn ErasedArgumentType<S>>,
}

impl<S: 'static> ArgumentTypeRef<S> {
    pub fn new<A: ArgumentType<S>>(argument_type: A) -> Self {
        Self {
            inner: Rc::new(argument_type),
        }
    }

    pub fn parse(&self, reader: &mut StringReader) -> Result<Rc<dyn Any>, CommandSyntaxException> {
        self.inner.parse_erased(reader)
    }

    pub fn parse_with_source(
        &self,
        reader: &mut StringReader,
        source: &S,
    ) -> Result<Rc<dyn Any>, CommandSyntaxException> {
        self.inner.parse_with_source_erased(reader, source)
    }

    pub fn list_suggestions(
        &self,
        context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        self.inner.list_suggestions_erased(context, builder)
    }

    pub fn examples(&self) -> Vec<String> {
        self.inner.examples_erased()
    }

    pub fn display(&self) -> String {
        self.inner.display_erased()
    }

    pub fn equals(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner) || self.inner.equals_erased(other.inner.as_ref())
    }

    pub fn values_equal(&self, left: &dyn Any, right: &dyn Any) -> bool {
        self.inner.value_equals_erased(left, right)
    }

    pub fn value_comparator(&self) -> ArgumentValueComparator {
        let argument_type = self.clone();
        Rc::new(move |left, right| argument_type.values_equal(left, right))
    }

    pub fn as_any(&self) -> &dyn Any {
        self.inner.as_any()
    }

    pub fn type_name(&self) -> &'static str {
        self.inner.argument_type_name()
    }

    pub fn value_type_name(&self) -> &'static str {
        self.inner.value_type_name()
    }

    pub fn value_type_id(&self) -> TypeId {
        self.inner.value_type_id()
    }
}

impl<S: 'static> Clone for ArgumentTypeRef<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<S: 'static> fmt::Debug for ArgumentTypeRef<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArgumentTypeRef")
            .field("type_name", &self.type_name())
            .field("value_type_name", &self.value_type_name())
            .finish()
    }
}

impl<S: 'static> PartialEq for ArgumentTypeRef<S> {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BoolArgumentType;

impl BoolArgumentType {
    pub const fn bool() -> Self {
        Self
    }

    pub fn get_bool<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<bool, ContextError> {
        context.argument::<bool>(name).map(|value| *value)
    }
}

impl<S: 'static> ArgumentType<S> for BoolArgumentType {
    type Value = bool;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        reader.read_boolean()
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        let (suggest_true, suggest_false) = {
            let remaining = builder.remaining_lower_case_utf16();
            (
                [b't' as u16, b'r' as u16, b'u' as u16, b'e' as u16].starts_with(remaining),
                [
                    b'f' as u16,
                    b'a' as u16,
                    b'l' as u16,
                    b's' as u16,
                    b'e' as u16,
                ]
                .starts_with(remaining),
            )
        };
        if suggest_true {
            builder.suggest("true");
        }
        if suggest_false {
            builder.suggest("false");
        }
        builder.build_future()
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(&["true", "false"])
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IntegerArgumentType {
    minimum: i32,
    maximum: i32,
}

impl IntegerArgumentType {
    pub const fn integer() -> Self {
        Self::new(i32::MIN, i32::MAX)
    }

    pub const fn integer_min(minimum: i32) -> Self {
        Self::new(minimum, i32::MAX)
    }

    pub const fn integer_range(minimum: i32, maximum: i32) -> Self {
        Self::new(minimum, maximum)
    }

    pub const fn new(minimum: i32, maximum: i32) -> Self {
        Self { minimum, maximum }
    }

    pub const fn minimum(self) -> i32 {
        self.minimum
    }

    pub const fn maximum(self) -> i32 {
        self.maximum
    }

    pub fn get_integer<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<i32, ContextError> {
        context.argument::<i32>(name).map(|value| *value)
    }
}

impl Default for IntegerArgumentType {
    fn default() -> Self {
        Self::integer()
    }
}

impl<S: 'static> ArgumentType<S> for IntegerArgumentType {
    type Value = i32;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let result = reader.read_int()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.integer_too_low().create_with_context(
                reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.integer_too_high().create_with_context(
                reader,
                result,
                self.maximum,
            ));
        }
        Ok(result)
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(&["0", "123", "-123"])
    }

    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }

    fn display(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for IntegerArgumentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::integer() {
            formatter.write_str("integer()")
        } else if self.maximum == i32::MAX {
            write!(formatter, "integer({})", self.minimum)
        } else {
            write!(formatter, "integer({}, {})", self.minimum, self.maximum)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LongArgumentType {
    minimum: i64,
    maximum: i64,
}

impl LongArgumentType {
    pub const fn long() -> Self {
        Self::new(i64::MIN, i64::MAX)
    }

    pub const fn long_min(minimum: i64) -> Self {
        Self::new(minimum, i64::MAX)
    }

    pub const fn long_range(minimum: i64, maximum: i64) -> Self {
        Self::new(minimum, maximum)
    }

    pub const fn new(minimum: i64, maximum: i64) -> Self {
        Self { minimum, maximum }
    }

    pub const fn minimum(self) -> i64 {
        self.minimum
    }

    pub const fn maximum(self) -> i64 {
        self.maximum
    }

    pub fn get_long<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<i64, ContextError> {
        context.argument::<i64>(name).map(|value| *value)
    }
}

impl Default for LongArgumentType {
    fn default() -> Self {
        Self::long()
    }
}

impl<S: 'static> ArgumentType<S> for LongArgumentType {
    type Value = i64;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let result = reader.read_long()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.long_too_low().create_with_context(
                reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.long_too_high().create_with_context(
                reader,
                result,
                self.maximum,
            ));
        }
        Ok(result)
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(&["0", "123", "-123"])
    }

    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }

    fn display(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for LongArgumentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::long() {
            formatter.write_str("longArg()")
        } else if self.maximum == i64::MAX {
            write!(formatter, "longArg({})", self.minimum)
        } else {
            write!(formatter, "longArg({}, {})", self.minimum, self.maximum)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatArgumentType {
    minimum: f32,
    maximum: f32,
}

impl FloatArgumentType {
    pub const fn float() -> Self {
        Self::new(-f32::MAX, f32::MAX)
    }

    pub const fn float_min(minimum: f32) -> Self {
        Self::new(minimum, f32::MAX)
    }

    pub const fn float_range(minimum: f32, maximum: f32) -> Self {
        Self::new(minimum, maximum)
    }

    pub const fn new(minimum: f32, maximum: f32) -> Self {
        Self { minimum, maximum }
    }

    pub const fn minimum(self) -> f32 {
        self.minimum
    }

    pub const fn maximum(self) -> f32 {
        self.maximum
    }

    pub fn get_float<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<f32, ContextError> {
        context.argument::<f32>(name).map(|value| *value)
    }
}

impl Default for FloatArgumentType {
    fn default() -> Self {
        Self::float()
    }
}

impl<S: 'static> ArgumentType<S> for FloatArgumentType {
    type Value = f32;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let result = reader.read_float()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.float_too_low().create_with_context(
                reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.float_too_high().create_with_context(
                reader,
                result,
                self.maximum,
            ));
        }
        Ok(result)
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(&["0", "1.2", ".5", "-1", "-.5", "-1234.56"])
    }

    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
    }

    fn display(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for FloatArgumentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::float() {
            formatter.write_str("float()")
        } else if self.maximum == f32::MAX {
            write!(formatter, "float({})", java_f32(self.minimum))
        } else {
            write!(
                formatter,
                "float({}, {})",
                java_f32(self.minimum),
                java_f32(self.maximum)
            )
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleArgumentType {
    minimum: f64,
    maximum: f64,
}

impl DoubleArgumentType {
    pub const fn double() -> Self {
        Self::new(-f64::MAX, f64::MAX)
    }

    pub const fn double_min(minimum: f64) -> Self {
        Self::new(minimum, f64::MAX)
    }

    pub const fn double_range(minimum: f64, maximum: f64) -> Self {
        Self::new(minimum, maximum)
    }

    pub const fn new(minimum: f64, maximum: f64) -> Self {
        Self { minimum, maximum }
    }

    pub const fn minimum(self) -> f64 {
        self.minimum
    }

    pub const fn maximum(self) -> f64 {
        self.maximum
    }

    pub fn get_double<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<f64, ContextError> {
        context.argument::<f64>(name).map(|value| *value)
    }
}

impl Default for DoubleArgumentType {
    fn default() -> Self {
        Self::double()
    }
}

impl<S: 'static> ArgumentType<S> for DoubleArgumentType {
    type Value = f64;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        let start = reader.cursor();
        let result = reader.read_double()?;
        if result < self.minimum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.double_too_low().create_with_context(
                reader,
                result,
                self.minimum,
            ));
        }
        if result > self.maximum {
            reader.set_cursor(start);
            return Err(BUILT_IN_EXCEPTIONS.double_too_high().create_with_context(
                reader,
                result,
                self.maximum,
            ));
        }
        Ok(result)
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(&["0", "1.2", ".5", "-1", "-.5", "-1234.56"])
    }

    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
    }

    fn display(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for DoubleArgumentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::double() {
            formatter.write_str("double()")
        } else if self.maximum == f64::MAX {
            write!(formatter, "double({})", java_f64(self.minimum))
        } else {
            write!(
                formatter,
                "double({}, {})",
                java_f64(self.minimum),
                java_f64(self.maximum)
            )
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StringType {
    SingleWord,
    QuotablePhrase,
    GreedyPhrase,
}

impl StringType {
    pub fn examples(self) -> &'static [&'static str] {
        match self {
            Self::SingleWord => &["word", "words_with_underscores"],
            Self::QuotablePhrase => &["\"quoted phrase\"", "word", "\"\""],
            Self::GreedyPhrase => &["word", "words with spaces", "\"and symbols\""],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StringArgumentType {
    string_type: StringType,
}

impl StringArgumentType {
    pub const fn word() -> Self {
        Self {
            string_type: StringType::SingleWord,
        }
    }

    pub const fn string() -> Self {
        Self {
            string_type: StringType::QuotablePhrase,
        }
    }

    pub const fn greedy_string() -> Self {
        Self {
            string_type: StringType::GreedyPhrase,
        }
    }

    pub const fn string_type(self) -> StringType {
        self.string_type
    }

    pub fn get_string<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<String, ContextError> {
        Self::get_string_utf16(context, name).map(|value| String::from_utf16_lossy(&value))
    }

    pub fn get_string_utf16<S: 'static>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<Vec<u16>, ContextError> {
        context
            .argument::<Vec<u16>>(name)
            .map(|value| (*value).clone())
    }

    pub fn escape_if_required(input: &str) -> String {
        if input
            .encode_utf16()
            .all(StringReader::is_allowed_in_unquoted_string)
        {
            input.to_owned()
        } else {
            Self::escape(input)
        }
    }

    fn escape(input: &str) -> String {
        let mut result = String::with_capacity(input.len() + 2);
        result.push('"');
        for character in input.chars() {
            if character == '\\' || character == '"' {
                result.push('\\');
            }
            result.push(character);
        }
        result.push('"');
        result
    }
}

impl<S: 'static> ArgumentType<S> for StringArgumentType {
    type Value = Vec<u16>;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        match self.string_type {
            StringType::SingleWord => Ok(reader.read_unquoted_string_utf16()),
            StringType::QuotablePhrase => reader.read_string_utf16(),
            StringType::GreedyPhrase => {
                let result = reader.remaining_utf16().to_vec();
                reader.set_cursor(reader.total_length());
                Ok(result)
            }
        }
    }

    fn examples(&self) -> Vec<String> {
        owned_examples(self.string_type.examples())
    }

    fn display(&self) -> String {
        self.to_string()
    }

    fn value_equals(&self, left: &Self::Value, right: &Self::Value) -> bool {
        left == right
    }
}

impl fmt::Display for StringArgumentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("string()")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse<A: ArgumentType<()>>(
        argument_type: &A,
        reader: &mut StringReader,
    ) -> Result<A::Value, CommandSyntaxException> {
        argument_type.parse(reader)
    }

    #[test]
    fn bool_parse() {
        let mut reader = StringReader::new("true");
        assert!(parse(&BoolArgumentType::bool(), &mut reader).unwrap());
        assert!(!reader.can_read());
    }

    #[test]
    fn integer_parse() {
        let mut reader = StringReader::new("15");
        assert_eq!(
            parse(&IntegerArgumentType::integer(), &mut reader).unwrap(),
            15
        );
        assert!(!reader.can_read());
    }

    #[test]
    fn integer_parse_too_small() {
        let mut reader = StringReader::new("-5");
        let error = parse(&IntegerArgumentType::integer_range(0, 100), &mut reader)
            .expect_err("out-of-range integer must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.integer_too_low()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn integer_parse_too_big() {
        let mut reader = StringReader::new("5");
        let error = parse(&IntegerArgumentType::integer_range(-100, 0), &mut reader)
            .expect_err("out-of-range integer must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.integer_too_high()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn integer_equals() {
        assert_eq!(
            IntegerArgumentType::integer(),
            IntegerArgumentType::integer()
        );
        assert_eq!(
            IntegerArgumentType::integer_range(-100, 100),
            IntegerArgumentType::integer_range(-100, 100)
        );
        assert_ne!(
            IntegerArgumentType::integer_range(-100, 100),
            IntegerArgumentType::integer_range(-100, 50)
        );
        assert_ne!(
            IntegerArgumentType::integer_range(-100, 100),
            IntegerArgumentType::integer_range(-50, 100)
        );
    }

    #[test]
    fn integer_to_string() {
        assert_eq!(IntegerArgumentType::integer().to_string(), "integer()");
        assert_eq!(
            IntegerArgumentType::integer_min(-100).to_string(),
            "integer(-100)"
        );
        assert_eq!(
            IntegerArgumentType::integer_range(-100, 100).to_string(),
            "integer(-100, 100)"
        );
        assert_eq!(
            IntegerArgumentType::integer_range(i32::MIN, 100).to_string(),
            "integer(-2147483648, 100)"
        );
    }

    #[test]
    fn long_parse() {
        let mut reader = StringReader::new("15");
        assert_eq!(parse(&LongArgumentType::long(), &mut reader).unwrap(), 15);
        assert!(!reader.can_read());
    }

    #[test]
    fn long_parse_too_small() {
        let mut reader = StringReader::new("-5");
        let error = parse(&LongArgumentType::long_range(0, 100), &mut reader)
            .expect_err("out-of-range long must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.long_too_low()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn long_parse_too_big() {
        let mut reader = StringReader::new("5");
        let error = parse(&LongArgumentType::long_range(-100, 0), &mut reader)
            .expect_err("out-of-range long must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.long_too_high()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn long_equals() {
        assert_eq!(LongArgumentType::long(), LongArgumentType::long());
        assert_eq!(
            LongArgumentType::long_range(-100, 100),
            LongArgumentType::long_range(-100, 100)
        );
        assert_ne!(
            LongArgumentType::long_range(-100, 100),
            LongArgumentType::long_range(-100, 50)
        );
        assert_ne!(
            LongArgumentType::long_range(-100, 100),
            LongArgumentType::long_range(-50, 100)
        );
    }

    #[test]
    fn long_to_string() {
        assert_eq!(LongArgumentType::long().to_string(), "longArg()");
        assert_eq!(
            LongArgumentType::long_min(-100).to_string(),
            "longArg(-100)"
        );
        assert_eq!(
            LongArgumentType::long_range(-100, 100).to_string(),
            "longArg(-100, 100)"
        );
        assert_eq!(
            LongArgumentType::long_range(i64::MIN, 100).to_string(),
            "longArg(-9223372036854775808, 100)"
        );
    }

    #[test]
    fn float_parse() {
        let mut reader = StringReader::new("15");
        assert_eq!(
            parse(&FloatArgumentType::float(), &mut reader).unwrap(),
            15.0
        );
        assert!(!reader.can_read());
    }

    #[test]
    fn float_parse_too_small() {
        let mut reader = StringReader::new("-5");
        let error = parse(&FloatArgumentType::float_range(0.0, 100.0), &mut reader)
            .expect_err("out-of-range float must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.float_too_low()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn float_parse_too_big() {
        let mut reader = StringReader::new("5");
        let error = parse(&FloatArgumentType::float_range(-100.0, 0.0), &mut reader)
            .expect_err("out-of-range float must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.float_too_high()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn float_equals() {
        assert_eq!(FloatArgumentType::float(), FloatArgumentType::float());
        assert_eq!(
            FloatArgumentType::float_range(-100.0, 100.0),
            FloatArgumentType::float_range(-100.0, 100.0)
        );
        assert_ne!(
            FloatArgumentType::float_range(-100.0, 100.0),
            FloatArgumentType::float_range(-100.0, 50.0)
        );
        assert_ne!(
            FloatArgumentType::float_range(-100.0, 100.0),
            FloatArgumentType::float_range(-50.0, 100.0)
        );
    }

    #[test]
    fn float_to_string() {
        assert_eq!(FloatArgumentType::float().to_string(), "float()");
        assert_eq!(
            FloatArgumentType::float_min(-100.0).to_string(),
            "float(-100.0)"
        );
        assert_eq!(
            FloatArgumentType::float_range(-100.0, 100.0).to_string(),
            "float(-100.0, 100.0)"
        );
        assert_eq!(
            FloatArgumentType::float_range(i32::MIN as f32, 100.0).to_string(),
            "float(-2.1474836E9, 100.0)"
        );
    }

    #[test]
    fn double_parse() {
        let mut reader = StringReader::new("15");
        assert_eq!(
            parse(&DoubleArgumentType::double(), &mut reader).unwrap(),
            15.0
        );
        assert!(!reader.can_read());
    }

    #[test]
    fn double_parse_too_small() {
        let mut reader = StringReader::new("-5");
        let error = parse(&DoubleArgumentType::double_range(0.0, 100.0), &mut reader)
            .expect_err("out-of-range double must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.double_too_low()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn double_parse_too_big() {
        let mut reader = StringReader::new("5");
        let error = parse(&DoubleArgumentType::double_range(-100.0, 0.0), &mut reader)
            .expect_err("out-of-range double must fail");
        assert!(error.is_type(&BUILT_IN_EXCEPTIONS.double_too_high()));
        assert_eq!(error.cursor(), 0);
    }

    #[test]
    fn double_equals() {
        assert_eq!(DoubleArgumentType::double(), DoubleArgumentType::double());
        assert_eq!(
            DoubleArgumentType::double_range(-100.0, 100.0),
            DoubleArgumentType::double_range(-100.0, 100.0)
        );
        assert_ne!(
            DoubleArgumentType::double_range(-100.0, 100.0),
            DoubleArgumentType::double_range(-100.0, 50.0)
        );
        assert_ne!(
            DoubleArgumentType::double_range(-100.0, 100.0),
            DoubleArgumentType::double_range(-50.0, 100.0)
        );
    }

    #[test]
    fn double_to_string() {
        assert_eq!(DoubleArgumentType::double().to_string(), "double()");
        assert_eq!(
            DoubleArgumentType::double_min(-100.0).to_string(),
            "double(-100.0)"
        );
        assert_eq!(
            DoubleArgumentType::double_range(-100.0, 100.0).to_string(),
            "double(-100.0, 100.0)"
        );
        assert_eq!(
            DoubleArgumentType::double_range(i32::MIN as f64, 100.0).to_string(),
            "double(-2.147483648E9, 100.0)"
        );
    }

    #[test]
    fn string_parse_word() {
        let mut reader = StringReader::new("hello world");
        assert_eq!(
            parse(&StringArgumentType::word(), &mut reader).unwrap(),
            "hello".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(reader.remaining(), " world");
    }

    #[test]
    fn string_parse_string() {
        let mut reader = StringReader::new("\"hello world\"");
        assert_eq!(
            parse(&StringArgumentType::string(), &mut reader).unwrap(),
            "hello world".encode_utf16().collect::<Vec<_>>()
        );
        assert!(!reader.can_read());
    }

    #[test]
    fn string_parse_greedy_string() {
        let mut reader = StringReader::new("Hello world! This is a test.");
        assert_eq!(
            parse(&StringArgumentType::greedy_string(), &mut reader).unwrap(),
            "Hello world! This is a test."
                .encode_utf16()
                .collect::<Vec<_>>()
        );
        assert!(!reader.can_read());
    }

    #[test]
    fn string_parse_preserves_java_utf16_values() {
        let mut reader = StringReader::from_utf16(vec![b'"' as u16, 0xd800, b'"' as u16]);
        assert_eq!(
            parse(&StringArgumentType::string(), &mut reader).unwrap(),
            [0xd800]
        );
    }

    #[test]
    fn string_to_string() {
        assert_eq!(StringArgumentType::string().to_string(), "string()");
    }

    #[test]
    fn string_escape_if_required_not_required() {
        assert_eq!(StringArgumentType::escape_if_required("hello"), "hello");
        assert_eq!(StringArgumentType::escape_if_required(""), "");
    }

    #[test]
    fn string_escape_if_required_multiple_words() {
        assert_eq!(
            StringArgumentType::escape_if_required("hello world"),
            "\"hello world\""
        );
    }

    #[test]
    fn string_escape_if_required_quote() {
        assert_eq!(
            StringArgumentType::escape_if_required("hello \"world\"!"),
            "\"hello \\\"world\\\"!\""
        );
    }

    #[test]
    fn string_escape_if_required_escapes() {
        assert_eq!(StringArgumentType::escape_if_required("\\"), "\"\\\\\"");
    }

    #[test]
    fn string_escape_if_required_single_quote() {
        assert_eq!(StringArgumentType::escape_if_required("\""), "\"\\\"\"");
    }

    #[derive(Clone, Copy)]
    struct SourceAware;

    impl ArgumentType<i32> for SourceAware {
        type Value = i32;

        fn parse(&self, _reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
            Ok(0)
        }

        fn parse_with_source(
            &self,
            _reader: &mut StringReader,
            source: &i32,
        ) -> Result<Self::Value, CommandSyntaxException> {
            Ok(*source)
        }
    }

    #[test]
    fn erased_argument_type_uses_source_aware_parse() {
        let argument_type = ArgumentTypeRef::<i32>::new(SourceAware);
        let mut reader = StringReader::new("");
        let result = argument_type
            .parse_with_source(&mut reader, &42)
            .unwrap()
            .downcast::<i32>()
            .unwrap();
        assert_eq!(*result, 42);
    }

    #[test]
    fn erased_argument_type_preserves_java_equality_rules() {
        let numeric = ArgumentTypeRef::<()>::new(IntegerArgumentType::integer_range(-1, 1));
        let equal_numeric = ArgumentTypeRef::<()>::new(IntegerArgumentType::integer_range(-1, 1));
        let boolean = ArgumentTypeRef::<()>::new(BoolArgumentType::bool());
        let other_boolean = ArgumentTypeRef::<()>::new(BoolArgumentType::bool());
        assert!(numeric.equals(&equal_numeric));
        assert!(!boolean.equals(&other_boolean));
        assert!(boolean.equals(&boolean.clone()));
    }

    #[test]
    fn floating_argument_bounds_use_java_primitive_equality() {
        let nan_float = ArgumentTypeRef::<()>::new(FloatArgumentType::float_range(f32::NAN, 1.0));
        let other_nan_float =
            ArgumentTypeRef::<()>::new(FloatArgumentType::float_range(f32::NAN, 1.0));
        let positive_zero_float =
            ArgumentTypeRef::<()>::new(FloatArgumentType::float_range(0.0, 1.0));
        let negative_zero_float =
            ArgumentTypeRef::<()>::new(FloatArgumentType::float_range(-0.0, 1.0));
        assert!(!nan_float.equals(&other_nan_float));
        assert!(positive_zero_float.equals(&negative_zero_float));

        let nan_double =
            ArgumentTypeRef::<()>::new(DoubleArgumentType::double_range(f64::NAN, 1.0));
        let other_nan_double =
            ArgumentTypeRef::<()>::new(DoubleArgumentType::double_range(f64::NAN, 1.0));
        let positive_zero_double =
            ArgumentTypeRef::<()>::new(DoubleArgumentType::double_range(0.0, 1.0));
        let negative_zero_double =
            ArgumentTypeRef::<()>::new(DoubleArgumentType::double_range(-0.0, 1.0));
        assert!(!nan_double.equals(&other_nan_double));
        assert!(positive_zero_double.equals(&negative_zero_double));
    }

    #[test]
    fn erased_value_comparator_preserves_java_wrapper_equality() {
        let float = ArgumentTypeRef::<()>::new(FloatArgumentType::float());
        let compare = float.value_comparator();
        assert!(compare(&f32::NAN, &f32::from_bits(0x7fc0_0001)));
        assert!(!compare(&0.0_f32, &-0.0_f32));
        assert!(!compare(&1.0_f32, &1.0_f64));

        let string = ArgumentTypeRef::<()>::new(StringArgumentType::string());
        let value = "value".encode_utf16().collect::<Vec<_>>();
        assert!(string.values_equal(&value, &value.clone()));
    }

    #[test]
    fn erased_value_comparator_defaults_to_identity() {
        #[derive(Clone, Copy)]
        struct IdentityValue;

        impl ArgumentType<()> for IdentityValue {
            type Value = Vec<i32>;

            fn parse(
                &self,
                _reader: &mut StringReader,
            ) -> Result<Self::Value, CommandSyntaxException> {
                Ok(Vec::new())
            }
        }

        let argument_type = ArgumentTypeRef::<()>::new(IdentityValue);
        let value = Rc::new(vec![1_i32]) as Rc<dyn Any>;
        let equal_value = Rc::new(vec![1_i32]) as Rc<dyn Any>;
        assert!(argument_type.values_equal(value.as_ref(), value.as_ref()));
        assert!(!argument_type.values_equal(value.as_ref(), equal_value.as_ref()));
    }
}
