use crate::MessageRef;
use crate::context::{CommandContext, StringRange};
use crate::exceptions::CommandSyntaxException;
use crate::java_case::{java_root_lowercase, java_se_25_preserves_case};
use std::cmp::Ordering;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub type SuggestionsFuture =
    Pin<Box<dyn Future<Output = Result<Suggestions, CommandSyntaxException>>>>;

pub type SuggestionProvider<S> = Rc<
    dyn Fn(
        &CommandContext<S>,
        SuggestionsBuilder,
    ) -> Result<SuggestionsFuture, CommandSyntaxException>,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SuggestionKind {
    Text,
    Integer(i32),
}

#[derive(Clone)]
pub struct Suggestion {
    range: StringRange,
    text: Vec<u16>,
    tooltip: Option<MessageRef>,
    kind: SuggestionKind,
}

impl Suggestion {
    pub fn new(range: StringRange, text: impl AsRef<str>) -> Self {
        Self::from_utf16(range, text.as_ref().encode_utf16().collect())
    }

    pub fn with_tooltip(range: StringRange, text: impl AsRef<str>, tooltip: MessageRef) -> Self {
        Self::from_utf16_with_tooltip(range, text.as_ref().encode_utf16().collect(), Some(tooltip))
    }

    pub fn from_utf16(range: StringRange, text: Vec<u16>) -> Self {
        Self::from_utf16_with_tooltip(range, text, None)
    }

    pub fn from_utf16_with_tooltip(
        range: StringRange,
        text: Vec<u16>,
        tooltip: Option<MessageRef>,
    ) -> Self {
        Self {
            range,
            text,
            tooltip,
            kind: SuggestionKind::Text,
        }
    }

    fn integer(range: StringRange, value: i32, tooltip: Option<MessageRef>) -> Self {
        Self {
            range,
            text: value.to_string().encode_utf16().collect(),
            tooltip,
            kind: SuggestionKind::Integer(value),
        }
    }

    pub fn range(&self) -> &StringRange {
        &self.range
    }

    pub fn text(&self) -> String {
        utf16_to_string(&self.text, "Suggestion::text")
    }

    pub fn text_utf16(&self) -> &[u16] {
        &self.text
    }

    pub fn tooltip(&self) -> Option<&MessageRef> {
        self.tooltip.as_ref()
    }

    pub fn integer_value(&self) -> Option<i32> {
        match self.kind {
            SuggestionKind::Text => None,
            SuggestionKind::Integer(value) => Some(value),
        }
    }

    pub fn apply(&self, input: &str) -> String {
        utf16_to_string(
            &self.apply_utf16(&input.encode_utf16().collect::<Vec<_>>()),
            "Suggestion::apply",
        )
    }

    pub fn apply_utf16(&self, input: &[u16]) -> Vec<u16> {
        let start = self.range.start();
        let end = self.range.end();

        if start == 0 && end == input.len() {
            return self.text.clone();
        }

        let prefix = if start > 0 { &input[..start] } else { &[] };
        let suffix = if end < input.len() {
            &input[end..]
        } else {
            &[]
        };
        let mut result = Vec::with_capacity(prefix.len() + self.text.len() + suffix.len());
        result.extend_from_slice(prefix);
        result.extend_from_slice(&self.text);
        result.extend_from_slice(suffix);
        result
    }

    pub fn expand(&self, command: &str, range: StringRange) -> Self {
        self.expand_utf16(&command.encode_utf16().collect::<Vec<_>>(), range)
    }

    pub fn expand_utf16(&self, command: &[u16], range: StringRange) -> Self {
        if range == self.range {
            return self.clone();
        }

        let outer_start = range.start();
        let outer_end = range.end();
        let inner_start = self.range.start();
        let inner_end = self.range.end();
        let left = if outer_start < inner_start {
            &command[outer_start..inner_start]
        } else {
            &[]
        };
        let right = if outer_end > inner_end {
            &command[inner_end..outer_end]
        } else {
            &[]
        };
        let mut text = Vec::with_capacity(left.len() + self.text.len() + right.len());
        text.extend_from_slice(left);
        text.extend_from_slice(&self.text);
        text.extend_from_slice(right);

        Self {
            range,
            text,
            tooltip: self.tooltip.clone(),
            kind: SuggestionKind::Text,
        }
    }

    pub fn compare_to(&self, other: &Self) -> Ordering {
        match (self.kind, other.kind) {
            (SuggestionKind::Integer(left), SuggestionKind::Integer(right)) => left.cmp(&right),
            _ => self.text.cmp(&other.text),
        }
    }

    pub fn compare_to_ignore_case(&self, other: &Self) -> Ordering {
        if matches!(self.kind, SuggestionKind::Integer(_)) {
            return self.compare_to(other);
        }

        java_compare_utf16_ignore_case(&self.text, &other.text)
    }

    pub fn java_equals(&self, other: &Self) -> bool {
        if matches!(self.kind, SuggestionKind::Integer(_)) && self.kind != other.kind {
            return false;
        }

        self.range == other.range
            && self.text == other.text
            && tooltip_equals(self.tooltip.as_ref(), other.tooltip.as_ref())
    }

    pub fn java_hash_code(&self) -> i32 {
        let range_hash = java_objects_hash(&[self.range.start() as i32, self.range.end() as i32]);
        let text_hash = java_utf16_hash_code(self.text.iter().copied());
        let tooltip_hash = self
            .tooltip
            .as_ref()
            .map_or(0, |tooltip| tooltip.hash_code());
        let base_hash = java_objects_hash(&[range_hash, text_hash, tooltip_hash]);
        match self.kind {
            SuggestionKind::Text => base_hash,
            SuggestionKind::Integer(value) => java_objects_hash(&[base_hash, value]),
        }
    }
}

impl PartialEq for Suggestion {
    fn eq(&self, other: &Self) -> bool {
        self.java_equals(other)
    }
}

impl fmt::Debug for Suggestion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct(match self.kind {
            SuggestionKind::Text => "Suggestion",
            SuggestionKind::Integer(_) => "IntegerSuggestion",
        });
        debug.field("range", &self.range);
        match String::from_utf16(&self.text) {
            Ok(text) => debug.field("text", &text),
            Err(_) => debug.field("text_utf16", &self.text),
        };
        debug
            .field(
                "tooltip",
                &self.tooltip.as_ref().map(|tooltip| tooltip.string()),
            )
            .field("value", &self.integer_value())
            .finish()
    }
}

#[derive(Clone)]
pub struct IntegerSuggestion(Suggestion);

impl IntegerSuggestion {
    pub fn new(range: StringRange, value: i32) -> Self {
        Self(Suggestion::integer(range, value, None))
    }

    pub fn with_tooltip(range: StringRange, value: i32, tooltip: MessageRef) -> Self {
        Self(Suggestion::integer(range, value, Some(tooltip)))
    }

    pub fn value(&self) -> i32 {
        self.0
            .integer_value()
            .expect("IntegerSuggestion always stores an integer")
    }

    pub fn as_suggestion(&self) -> &Suggestion {
        &self.0
    }

    pub fn into_suggestion(self) -> Suggestion {
        self.0
    }
}

impl std::ops::Deref for IntegerSuggestion {
    type Target = Suggestion;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<IntegerSuggestion> for Suggestion {
    fn from(value: IntegerSuggestion) -> Self {
        value.into_suggestion()
    }
}

impl PartialEq for IntegerSuggestion {
    fn eq(&self, other: &Self) -> bool {
        self.0.java_equals(&other.0)
    }
}

impl PartialEq<IntegerSuggestion> for Suggestion {
    fn eq(&self, other: &IntegerSuggestion) -> bool {
        self.java_equals(other.as_suggestion())
    }
}

impl PartialEq<Suggestion> for IntegerSuggestion {
    fn eq(&self, other: &Suggestion) -> bool {
        self.0.java_equals(other)
    }
}

impl fmt::Debug for IntegerSuggestion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
pub struct Suggestions {
    range: StringRange,
    suggestions: Vec<Suggestion>,
}

impl Suggestions {
    pub fn new(range: StringRange, suggestions: Vec<Suggestion>) -> Self {
        Self { range, suggestions }
    }

    pub fn empty() -> Self {
        Self::new(StringRange::at(0), Vec::new())
    }

    pub fn empty_future() -> SuggestionsFuture {
        Box::pin(std::future::ready(Ok(Self::empty())))
    }

    pub fn range(&self) -> &StringRange {
        &self.range
    }

    pub fn list(&self) -> &[Suggestion] {
        &self.suggestions
    }

    pub fn is_empty(&self) -> bool {
        self.suggestions.is_empty()
    }

    pub fn java_hash_code(&self) -> i32 {
        let list_hash = self.suggestions.iter().fold(1_i32, |hash, suggestion| {
            hash.wrapping_mul(31)
                .wrapping_add(suggestion.java_hash_code())
        });
        java_objects_hash(&[self.range.java_hash_code(), list_hash])
    }

    pub fn merge(command: &str, input: impl AsRef<[Self]>) -> Self {
        Self::merge_utf16(&command.encode_utf16().collect::<Vec<_>>(), input.as_ref())
    }

    pub fn merge_utf16(command: &[u16], input: &[Self]) -> Self {
        match input {
            [] => Self::empty(),
            [only] => only.clone(),
            _ => {
                let texts = collect_unique_suggestions(
                    input
                        .iter()
                        .flat_map(|suggestions| suggestions.suggestions.iter().cloned()),
                );
                Self::create_utf16(command, texts)
            }
        }
    }

    pub fn create(command: &str, suggestions: impl IntoIterator<Item = Suggestion>) -> Self {
        Self::create_utf16(&command.encode_utf16().collect::<Vec<_>>(), suggestions)
    }

    pub fn create_utf16(
        command: &[u16],
        suggestions: impl IntoIterator<Item = Suggestion>,
    ) -> Self {
        let suggestions: Vec<_> = suggestions.into_iter().collect();
        if suggestions.is_empty() {
            return Self::empty();
        }

        let start = suggestions
            .iter()
            .map(|suggestion| suggestion.range.start())
            .min()
            .expect("non-empty suggestion collection has a start");
        let end = suggestions
            .iter()
            .map(|suggestion| suggestion.range.end())
            .max()
            .expect("non-empty suggestion collection has an end");
        let range = StringRange::between(start, end);

        let mut expanded = collect_unique_suggestions(
            suggestions
                .into_iter()
                .map(|suggestion| suggestion.expand_utf16(command, range)),
        );
        stable_sort(&mut expanded);

        Self::new(range, expanded)
    }
}

impl PartialEq for Suggestions {
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && self.suggestions == other.suggestions
    }
}

#[derive(Clone, Debug)]
pub struct SuggestionsBuilder {
    input: String,
    input_utf16: Vec<u16>,
    input_lower_case: String,
    input_lower_case_utf16: Vec<u16>,
    start: usize,
    result: Vec<Suggestion>,
}

impl SuggestionsBuilder {
    pub fn new(input: impl Into<String>, start: usize) -> Self {
        let input = input.into();
        Self::from_utf16(input.encode_utf16().collect(), start)
    }

    pub fn from_utf16(input: Vec<u16>, start: usize) -> Self {
        let input_lower_case_utf16 = java_root_lowercase(&input);
        Self::from_utf16_with_lower_case(input, input_lower_case_utf16, start)
    }

    pub fn with_lower_case(
        input: impl Into<String>,
        input_lower_case: impl Into<String>,
        start: usize,
    ) -> Self {
        let input_utf16: Vec<_> = input.into().encode_utf16().collect();
        let input_lower_case_utf16: Vec<_> = input_lower_case.into().encode_utf16().collect();
        Self::from_utf16_with_lower_case(input_utf16, input_lower_case_utf16, start)
    }

    fn from_utf16_with_lower_case(
        input_utf16: Vec<u16>,
        input_lower_case_utf16: Vec<u16>,
        start: usize,
    ) -> Self {
        assert!(
            start <= input_utf16.len(),
            "suggestion start exceeds the input"
        );
        assert!(
            start <= input_lower_case_utf16.len(),
            "suggestion start exceeds the lowercase input"
        );

        Self {
            input: String::from_utf16_lossy(&input_utf16),
            input_lower_case: String::from_utf16_lossy(&input_lower_case_utf16),
            input_utf16,
            input_lower_case_utf16,
            start,
            result: Vec::new(),
        }
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn input_utf16(&self) -> &[u16] {
        &self.input_utf16
    }

    pub fn input_lower_case(&self) -> &str {
        &self.input_lower_case
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn remaining(&self) -> String {
        utf16_to_string(
            &self.input_utf16[self.start..],
            "SuggestionsBuilder::remaining",
        )
    }

    pub fn remaining_utf16(&self) -> &[u16] {
        &self.input_utf16[self.start..]
    }

    pub fn remaining_lower_case(&self) -> String {
        utf16_to_string(
            &self.input_lower_case_utf16[self.start..],
            "SuggestionsBuilder::remaining_lower_case",
        )
    }

    pub fn remaining_lower_case_utf16(&self) -> &[u16] {
        &self.input_lower_case_utf16[self.start..]
    }

    pub fn build(&self) -> Suggestions {
        Suggestions::create_utf16(&self.input_utf16, self.result.clone())
    }

    pub fn build_future(&self) -> SuggestionsFuture {
        Box::pin(std::future::ready(Ok(self.build())))
    }

    pub fn suggest(&mut self, text: impl AsRef<str>) -> &mut Self {
        let text: Vec<_> = text.as_ref().encode_utf16().collect();
        self.suggest_utf16(text)
    }

    pub fn suggest_with_tooltip(
        &mut self,
        text: impl AsRef<str>,
        tooltip: MessageRef,
    ) -> &mut Self {
        let text: Vec<_> = text.as_ref().encode_utf16().collect();
        if text != self.input_utf16[self.start..] {
            self.result.push(Suggestion::from_utf16_with_tooltip(
                StringRange::between(self.start, self.input_utf16.len()),
                text,
                Some(tooltip),
            ));
        }
        self
    }

    pub fn suggest_utf16(&mut self, text: Vec<u16>) -> &mut Self {
        if text != self.input_utf16[self.start..] {
            self.result.push(Suggestion::from_utf16(
                StringRange::between(self.start, self.input_utf16.len()),
                text,
            ));
        }
        self
    }

    pub fn suggest_integer(&mut self, value: i32) -> &mut Self {
        self.result.push(Suggestion::integer(
            StringRange::between(self.start, self.input_utf16.len()),
            value,
            None,
        ));
        self
    }

    pub fn suggest_integer_with_tooltip(&mut self, value: i32, tooltip: MessageRef) -> &mut Self {
        self.result.push(Suggestion::integer(
            StringRange::between(self.start, self.input_utf16.len()),
            value,
            Some(tooltip),
        ));
        self
    }

    pub fn add(&mut self, other: &Self) -> &mut Self {
        self.result.extend(other.result.iter().cloned());
        self
    }

    pub fn create_offset(&self, start: usize) -> Self {
        Self::from_utf16_with_lower_case(
            self.input_utf16.clone(),
            self.input_lower_case_utf16.clone(),
            start,
        )
    }

    pub fn restart(&self) -> Self {
        self.create_offset(self.start)
    }
}

fn collect_unique_suggestions(input: impl IntoIterator<Item = Suggestion>) -> Vec<Suggestion> {
    let mut result = Vec::new();
    for suggestion in input {
        if !result
            .iter()
            .any(|existing| suggestion.java_equals(existing))
        {
            result.push(suggestion);
        }
    }
    result
}

fn java_objects_hash(values: &[i32]) -> i32 {
    values.iter().fold(1_i32, |hash, value| {
        hash.wrapping_mul(31).wrapping_add(*value)
    })
}

fn java_utf16_hash_code(input: impl IntoIterator<Item = u16>) -> i32 {
    input.into_iter().fold(0_i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    })
}

fn tooltip_equals(left: Option<&MessageRef>, right: Option<&MessageRef>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right) || left.equals(right.as_ref()),
        _ => false,
    }
}

fn stable_sort(suggestions: &mut [Suggestion]) {
    let mut order: Vec<_> = (0..suggestions.len()).collect();
    let mut buffer = order.clone();
    stable_sort_indices(&mut order, &mut buffer, suggestions);

    let mut destination = vec![0; order.len()];
    for (new_index, old_index) in order.into_iter().enumerate() {
        destination[old_index] = new_index;
    }
    for index in 0..destination.len() {
        while destination[index] != index {
            let target = destination[index];
            suggestions.swap(index, target);
            destination.swap(index, target);
        }
    }
}

fn stable_sort_indices(order: &mut [usize], buffer: &mut [usize], suggestions: &[Suggestion]) {
    if order.len() < 2 {
        return;
    }

    let middle = order.len() / 2;
    let (left_order, right_order) = order.split_at_mut(middle);
    let (left_buffer, right_buffer) = buffer.split_at_mut(middle);
    stable_sort_indices(left_order, left_buffer, suggestions);
    stable_sort_indices(right_order, right_buffer, suggestions);

    buffer.copy_from_slice(order);
    let (left, right) = buffer.split_at(middle);
    let mut left_index = 0;
    let mut right_index = 0;
    for output in order {
        if right_index == right.len()
            || (left_index < left.len()
                && suggestions[left[left_index]]
                    .compare_to_ignore_case(&suggestions[right[right_index]])
                    != Ordering::Greater)
        {
            *output = left[left_index];
            left_index += 1;
        } else {
            *output = right[right_index];
            right_index += 1;
        }
    }
}

fn utf16_to_string(units: &[u16], operation: &str) -> String {
    String::from_utf16(units).unwrap_or_else(|_| {
        panic!("{operation} produced an unpaired UTF-16 surrogate; use the UTF-16 API")
    })
}

fn java_compare_utf16_ignore_case(left: &[u16], right: &[u16]) -> Ordering {
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        let mut left_code_point = u32::from(left[left_index]);
        let mut right_code_point = u32::from(right[right_index]);
        if left_code_point != right_code_point
            && java_compare_code_point_ignore_case(left_code_point, right_code_point)
                != Ordering::Equal
        {
            let (combined_left, left_consumes_next) = java_code_point_including(left, left_index);
            let (combined_right, right_consumes_next) =
                java_code_point_including(right, right_index);
            left_code_point = combined_left;
            right_code_point = combined_right;
            if left_consumes_next {
                left_index += 1;
            }
            if right_consumes_next {
                right_index += 1;
            }

            let comparison = java_compare_code_point_ignore_case(left_code_point, right_code_point);
            if comparison != Ordering::Equal {
                return comparison;
            }
        }
        left_index += 1;
        right_index += 1;
    }

    left.len().cmp(&right.len())
}

fn java_code_point_including(input: &[u16], index: usize) -> (u32, bool) {
    let unit = input[index];
    if (0xdc00..=0xdfff).contains(&unit)
        && index > 0
        && (0xd800..=0xdbff).contains(&input[index - 1])
    {
        return (java_surrogate_pair(input[index - 1], unit), false);
    }
    if (0xd800..=0xdbff).contains(&unit)
        && input
            .get(index + 1)
            .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
    {
        return (java_surrogate_pair(unit, input[index + 1]), true);
    }
    (u32::from(unit), false)
}

fn java_surrogate_pair(high: u16, low: u16) -> u32 {
    0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
}

fn java_compare_code_point_ignore_case(left: u32, right: u32) -> Ordering {
    let left_upper = java_simple_uppercase_code_point(left);
    let right_upper = java_simple_uppercase_code_point(right);
    if left_upper == right_upper {
        return Ordering::Equal;
    }

    java_simple_lowercase_code_point(left_upper).cmp(&java_simple_lowercase_code_point(right_upper))
}

fn java_simple_uppercase(unit: u16) -> u16 {
    // Java SE 25 uses Unicode 16.0, while Rust may use a newer Unicode release.
    match unit {
        0x1f80..=0x1f87 | 0x1f90..=0x1f97 | 0x1fa0..=0x1fa7 => return unit + 8,
        0x1fb3 => return 0x1fbc,
        0x1fc3 => return 0x1fcc,
        0x1ff3 => return 0x1ffc,
        _ if java_se_25_preserves_case(u32::from(unit)) => return unit,
        _ => {}
    }
    let Some(character) = char::from_u32(unit.into()) else {
        return unit;
    };
    let mut uppercase = character.to_uppercase();
    let Some(mapped) = uppercase.next() else {
        return unit;
    };
    if uppercase.next().is_some() || mapped.len_utf16() != 1 {
        unit
    } else {
        mapped as u16
    }
}

fn java_simple_uppercase_code_point(code_point: u32) -> u32 {
    if let Ok(unit) = u16::try_from(code_point) {
        return u32::from(java_simple_uppercase(unit));
    }
    if java_se_25_preserves_case(code_point) {
        return code_point;
    }
    let Some(character) = char::from_u32(code_point) else {
        return code_point;
    };
    let mut uppercase = character.to_uppercase();
    let Some(mapped) = uppercase.next() else {
        return code_point;
    };
    if uppercase.next().is_none() {
        mapped as u32
    } else {
        code_point
    }
}

fn java_simple_lowercase(unit: u16) -> u16 {
    // These mappings were added after Unicode 16.0.
    if java_se_25_preserves_case(u32::from(unit)) {
        return unit;
    }
    let Some(character) = char::from_u32(unit.into()) else {
        return unit;
    };
    let mut lowercase = character.to_lowercase();
    let Some(mapped) = lowercase.next() else {
        return unit;
    };
    if mapped.len_utf16() != 1 {
        return unit;
    }
    if lowercase.next().is_none() || character == '\u{0130}' {
        mapped as u16
    } else {
        unit
    }
}

fn java_simple_lowercase_code_point(code_point: u32) -> u32 {
    if let Ok(unit) = u16::try_from(code_point) {
        return u32::from(java_simple_lowercase(unit));
    }
    if java_se_25_preserves_case(code_point) {
        return code_point;
    }
    let Some(character) = char::from_u32(code_point) else {
        return code_point;
    };
    let mut lowercase = character.to_lowercase();
    let Some(mapped) = lowercase.next() else {
        return code_point;
    };
    if lowercase.next().is_none() {
        mapped as u32
    } else {
        code_point
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiteralMessage;

    fn at(position: usize) -> StringRange {
        StringRange::at(position)
    }

    fn between(start: usize, end: usize) -> StringRange {
        StringRange::between(start, end)
    }

    fn suggestion(range: StringRange, text: &str) -> Suggestion {
        Suggestion::new(range, text)
    }

    fn texts(suggestions: &Suggestions) -> Vec<String> {
        suggestions.list().iter().map(Suggestion::text).collect()
    }

    #[test]
    fn apply_insertation_start() {
        let suggestion = suggestion(at(0), "And so I said: ");
        assert_eq!(
            suggestion.apply("Hello world!"),
            "And so I said: Hello world!"
        );
    }

    #[test]
    fn apply_insertation_middle() {
        let suggestion = suggestion(at(6), "small ");
        assert_eq!(suggestion.apply("Hello world!"), "Hello small world!");
    }

    #[test]
    fn apply_insertation_end() {
        let suggestion = suggestion(at(5), " world!");
        assert_eq!(suggestion.apply("Hello"), "Hello world!");
    }

    #[test]
    fn apply_replacement_start() {
        let suggestion = suggestion(between(0, 5), "Goodbye");
        assert_eq!(suggestion.apply("Hello world!"), "Goodbye world!");
    }

    #[test]
    fn apply_replacement_middle() {
        let suggestion = suggestion(between(6, 11), "Alex");
        assert_eq!(suggestion.apply("Hello world!"), "Hello Alex!");
    }

    #[test]
    fn apply_replacement_end() {
        let suggestion = suggestion(between(6, 12), "Creeper!");
        assert_eq!(suggestion.apply("Hello world!"), "Hello Creeper!");
    }

    #[test]
    fn apply_replacement_everything() {
        let suggestion = suggestion(between(0, 12), "Oh dear.");
        assert_eq!(suggestion.apply("Hello world!"), "Oh dear.");
    }

    #[test]
    fn expand_unchanged() {
        let subject = suggestion(at(1), "oo");
        assert_eq!(subject.expand("f", at(1)), subject);
    }

    #[test]
    fn expand_left() {
        let subject = suggestion(at(1), "oo");
        assert_eq!(
            subject.expand("f", between(0, 1)),
            suggestion(between(0, 1), "foo")
        );
    }

    #[test]
    fn expand_right() {
        let subject = suggestion(at(0), "minecraft:");
        assert_eq!(
            subject.expand("fish", between(0, 4)),
            suggestion(between(0, 4), "minecraft:fish")
        );
    }

    #[test]
    fn expand_both() {
        let subject = suggestion(at(11), "minecraft:");
        assert_eq!(
            subject.expand("give Steve fish_block", between(5, 21)),
            suggestion(between(5, 21), "Steve minecraft:fish_block")
        );
    }

    #[test]
    fn expand_replacement() {
        let subject = suggestion(between(6, 11), "strangers");
        assert_eq!(
            subject.expand("Hello world!", between(0, 12)),
            suggestion(between(0, 12), "Hello strangers!")
        );
    }

    #[test]
    fn merge_empty() {
        assert!(Suggestions::merge("foo b", &[]).is_empty());
    }

    #[test]
    fn merge_single() {
        let suggestions = Suggestions::new(at(5), vec![suggestion(at(5), "ar")]);
        assert_eq!(
            Suggestions::merge("foo b", std::slice::from_ref(&suggestions)),
            suggestions
        );
    }

    #[test]
    fn merge_multiple() {
        let a = Suggestions::new(
            at(5),
            vec![
                suggestion(at(5), "ar"),
                suggestion(at(5), "az"),
                suggestion(at(5), "Az"),
            ],
        );
        let b = Suggestions::new(
            between(4, 5),
            vec![
                suggestion(between(4, 5), "foo"),
                suggestion(between(4, 5), "qux"),
                suggestion(between(4, 5), "apple"),
                suggestion(between(4, 5), "Bar"),
            ],
        );
        assert_eq!(
            Suggestions::merge("foo b", &[a, b]).list(),
            &[
                suggestion(between(4, 5), "apple"),
                suggestion(between(4, 5), "bar"),
                suggestion(between(4, 5), "Bar"),
                suggestion(between(4, 5), "baz"),
                suggestion(between(4, 5), "bAz"),
                suggestion(between(4, 5), "foo"),
                suggestion(between(4, 5), "qux"),
            ]
        );
    }

    #[test]
    fn suggest_appends() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder.suggest("world!").build();
        assert_eq!(result.list(), &[suggestion(between(6, 7), "world!")]);
        assert_eq!(result.range(), &between(6, 7));
        assert!(!result.is_empty());
    }

    #[test]
    fn suggest_replaces() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder.suggest("everybody").build();
        assert_eq!(result.list(), &[suggestion(between(6, 7), "everybody")]);
        assert_eq!(result.range(), &between(6, 7));
        assert!(!result.is_empty());
    }

    #[test]
    fn suggest_noop() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder.suggest("w").build();
        assert!(result.list().is_empty());
        assert!(result.is_empty());
    }

    #[test]
    fn suggest_multiple() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder
            .suggest("world!")
            .suggest("everybody")
            .suggest("weekend")
            .build();
        assert_eq!(texts(&result), ["everybody", "weekend", "world!"]);
        assert_eq!(result.range(), &between(6, 7));
        assert!(!result.is_empty());
    }

    #[test]
    fn restart() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        builder.suggest("won't be included in restart");
        let other = builder.restart();
        assert_eq!(other.input(), builder.input());
        assert_eq!(other.start(), builder.start());
        assert_eq!(other.remaining(), builder.remaining());
        assert!(other.result.is_empty());
    }

    #[test]
    fn sort_alphabetical() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder
            .suggest("2")
            .suggest("4")
            .suggest("6")
            .suggest("8")
            .suggest("30")
            .suggest("32")
            .build();
        assert_eq!(texts(&result), ["2", "30", "32", "4", "6", "8"]);
    }

    #[test]
    fn sort_numerical() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder
            .suggest_integer(2)
            .suggest_integer(4)
            .suggest_integer(6)
            .suggest_integer(8)
            .suggest_integer(30)
            .suggest_integer(32)
            .build();
        assert_eq!(texts(&result), ["2", "4", "6", "8", "30", "32"]);
    }

    #[test]
    fn sort_mixed() {
        let mut builder = SuggestionsBuilder::new("Hello w", 6);
        let result = builder
            .suggest("11")
            .suggest("22")
            .suggest("33")
            .suggest("a")
            .suggest("b")
            .suggest("c")
            .suggest_integer(2)
            .suggest_integer(4)
            .suggest_integer(6)
            .suggest_integer(8)
            .suggest_integer(30)
            .suggest_integer(32)
            .suggest("3a")
            .suggest("a3")
            .build();
        assert!(
            result
                .list()
                .windows(2)
                .all(|pair| { pair[0].compare_to_ignore_case(&pair[1]) != Ordering::Greater })
        );

        let mut actual = texts(&result);
        actual.sort();
        let mut expected = vec![
            "11", "22", "33", "a", "b", "c", "2", "4", "6", "8", "30", "32", "3a", "a3",
        ];
        expected.sort();
        assert_eq!(actual, expected);

        let integers: Vec<_> = result
            .list()
            .iter()
            .filter_map(Suggestion::integer_value)
            .collect();
        assert_eq!(integers, [2, 4, 6, 8, 30, 32]);
    }

    #[test]
    fn create_uses_deterministic_encounter_order_for_unconstrained_ties() {
        let case_tie = Suggestions::create("", [suggestion(at(0), "ab"), suggestion(at(0), "AB")]);
        assert_eq!(texts(&case_tie), ["ab", "AB"]);

        let mixed = Suggestions::create(
            "",
            [
                IntegerSuggestion::new(at(0), 4).into_suggestion(),
                IntegerSuggestion::new(at(0), 11).into_suggestion(),
                suggestion(at(0), "3a"),
            ],
        );
        assert_eq!(texts(&mixed), ["4", "11", "3a"]);
    }

    #[test]
    fn encounter_order_dedup_uses_incoming_equals() {
        let text_then_integer = Suggestions::create(
            "",
            [
                suggestion(at(0), "1"),
                IntegerSuggestion::new(at(0), 1).into_suggestion(),
            ],
        );
        assert_eq!(text_then_integer.list().len(), 2);

        let integer_then_text = Suggestions::create(
            "",
            [
                IntegerSuggestion::new(at(0), 1).into_suggestion(),
                suggestion(at(0), "1"),
            ],
        );
        assert_eq!(integer_then_text.list().len(), 1);
        assert_eq!(integer_then_text.list()[0].integer_value(), Some(1));
    }

    #[test]
    fn suggestion_hash_codes_match_java() {
        assert_eq!(suggestion(at(0), "ab").java_hash_code(), 1_049_567);
        assert_eq!(suggestion(between(2, 5), "😀").java_hash_code(), 55_977_568);
        assert_eq!(
            IntegerSuggestion::new(at(0), 4).java_hash_code(),
            29_603_609
        );
        assert_eq!(
            Suggestions::new(at(0), vec![suggestion(at(0), "ab")]).java_hash_code(),
            1_080_350
        );
    }

    #[test]
    fn ignore_case_comparison_recombines_surrogate_pairs() {
        let uppercase = suggestion(at(0), "𐐀");
        let lowercase = suggestion(at(0), "𐐨");
        assert_eq!(
            uppercase.compare_to_ignore_case(&lowercase),
            Ordering::Equal
        );

        let before = suggestion(at(0), "𐀀");
        assert_eq!(before.compare_to_ignore_case(&uppercase), Ordering::Less);
    }

    #[test]
    fn java_se_25_unicode_16_simple_case_mapping() {
        const FNV_PRIME: u64 = 1_099_511_628_211;

        fn mix(hash: u64, value: u32) -> u64 {
            (hash ^ u64::from(value)).wrapping_mul(FNV_PRIME)
        }

        let mut hash = 0xcbf2_9ce4_8422_2325;
        for code_point in 0..=0x10ffff {
            hash = mix(hash, code_point);
            hash = mix(hash, java_simple_uppercase_code_point(code_point));
            hash = mix(hash, java_simple_lowercase_code_point(code_point));
        }
        assert_eq!(hash, 14_675_530_581_949_953_150);
    }

    #[test]
    fn range_offsets_are_utf16_code_units() {
        let mut builder = SuggestionsBuilder::new("a😀x", 3);
        assert_eq!(builder.remaining(), "x");
        let suggestion = builder.suggest("y").build().list()[0].clone();
        assert_eq!(suggestion.range(), &between(3, 4));
        assert_eq!(suggestion.apply("a😀x"), "a😀y");
    }

    #[test]
    fn utf16_api_preserves_unpaired_surrogates() {
        let suggestion = Suggestion::from_utf16(at(1), vec![b'x' as u16]);
        assert_eq!(
            suggestion.apply_utf16(&[0xd83d, 0xde00]),
            [0xd83d, b'x' as u16, 0xde00]
        );

        let builder = SuggestionsBuilder::from_utf16(vec![0xd800, b'X' as u16], 1);
        assert_eq!(builder.input_utf16(), [0xd800, b'X' as u16]);
        assert_eq!(builder.remaining_utf16(), [b'X' as u16]);
        assert_eq!(
            builder.create_offset(0).input_utf16(),
            builder.input_utf16()
        );
    }

    #[test]
    fn tooltip_equality_uses_message_equality() {
        let tooltip: MessageRef = Rc::new(LiteralMessage::new("tip"));
        let same = Suggestion::with_tooltip(at(0), "x", tooltip.clone());
        let same_reference = Suggestion::with_tooltip(at(0), "x", tooltip);
        let other: MessageRef = Rc::new(LiteralMessage::new("tip"));
        let distinct_reference = Suggestion::with_tooltip(at(0), "x", other);
        assert_eq!(same, same_reference);
        assert_ne!(same, distinct_reference);
    }

    #[test]
    fn lowercase_uses_java_se_25_unicode_16_data() {
        let input = String::from_utf16(&[0xa7ce, b'A' as u16]).unwrap();
        let builder = SuggestionsBuilder::new(input, 0);
        assert_eq!(
            builder
                .input_lower_case()
                .encode_utf16()
                .collect::<Vec<_>>(),
            [0xa7ce, b'a' as u16]
        );
    }

    #[test]
    fn equality_preserves_java_subclass_asymmetry() {
        let text = suggestion(at(0), "1");
        let integer = IntegerSuggestion::new(at(0), 1);
        assert!(text.java_equals(integer.as_suggestion()));
        assert!(!integer.java_equals(&text));
        assert!(text == integer);
        assert!(!(integer == text));
    }

    #[test]
    fn comparison_uses_utf16_units_and_java_case_mapping() {
        let supplementary = suggestion(at(0), "\u{10000}");
        let bmp = suggestion(at(0), "\u{e000}");
        assert_eq!(supplementary.compare_to(&bmp), Ordering::Less);

        let lowercase_with_iota = suggestion(at(0), "\u{1f80}");
        let uppercase_with_iota = suggestion(at(0), "\u{1f88}");
        assert_eq!(
            lowercase_with_iota.compare_to_ignore_case(&uppercase_with_iota),
            Ordering::Equal
        );
    }
}
