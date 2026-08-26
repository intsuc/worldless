use crate::MessageRef;
use crate::context::{CommandContext, StringRange};
use crate::exceptions::CommandSyntaxException;
use crate::java_case::{java_root_lowercase, java25_preserves_case};
use crate::java_hash_set::{java_hash_set_order, java_utf16_hash_code};
use std::cmp::Ordering;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicI32, Ordering as AtomicOrdering};

static NEXT_SUGGESTION_IDENTITY_HASH: AtomicI32 = AtomicI32::new(1);

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
    identity_hash: i32,
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
            identity_hash: next_suggestion_identity_hash(),
        }
    }

    fn integer(range: StringRange, value: i32, tooltip: Option<MessageRef>) -> Self {
        Self {
            range,
            text: value.to_string().encode_utf16().collect(),
            tooltip,
            kind: SuggestionKind::Integer(value),
            identity_hash: next_suggestion_identity_hash(),
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
            identity_hash: next_suggestion_identity_hash(),
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

    fn java_hash_map_comparable_order(&self, other: &Self) -> Option<Ordering> {
        match (self.kind, other.kind) {
            (SuggestionKind::Text, SuggestionKind::Text) => Some(self.compare_to(other)),
            _ => None,
        }
    }

    fn java_hash_map_tie_break_order(&self, other: &Self) -> Ordering {
        match (self.kind, other.kind) {
            (SuggestionKind::Text, SuggestionKind::Integer(_)) => Ordering::Greater,
            (SuggestionKind::Integer(_), SuggestionKind::Text) => Ordering::Less,
            _ if self.identity_hash <= other.identity_hash => Ordering::Less,
            _ => Ordering::Greater,
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
                let texts = collect_java_hash_set(
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

        let mut expanded = collect_java_hash_set(
            suggestions
                .into_iter()
                .map(|suggestion| suggestion.expand_utf16(command, range)),
        );
        java_stable_sort(&mut expanded);

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

fn collect_java_hash_set(input: impl IntoIterator<Item = Suggestion>) -> Vec<Suggestion> {
    java_hash_set_order(
        input,
        Suggestion::java_hash_code,
        Suggestion::java_equals,
        Suggestion::java_hash_map_comparable_order,
        Suggestion::java_hash_map_tie_break_order,
    )
}

fn java_objects_hash(values: &[i32]) -> i32 {
    values.iter().fold(1_i32, |hash, value| {
        hash.wrapping_mul(31).wrapping_add(*value)
    })
}

fn next_suggestion_identity_hash() -> i32 {
    NEXT_SUGGESTION_IDENTITY_HASH.fetch_add(1, AtomicOrdering::Relaxed)
}

fn tooltip_equals(left: Option<&MessageRef>, right: Option<&MessageRef>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => Rc::ptr_eq(left, right) || left.equals(right.as_ref()),
        _ => false,
    }
}

const JAVA_TIMSORT_MIN_MERGE: usize = 32;

fn java_stable_sort(suggestions: &mut [Suggestion]) {
    let mut remaining = suggestions.len();
    if remaining < 2 {
        return;
    }

    if remaining < JAVA_TIMSORT_MIN_MERGE {
        let initial_run = count_run_and_make_ascending(suggestions, 0, remaining);
        binary_sort(suggestions, 0, remaining, initial_run);
        return;
    }

    let min_run = min_run_length(remaining);
    let mut lo = 0;
    let mut sorter = JavaTimSort::new(suggestions);
    loop {
        let mut run_len = count_run_and_make_ascending(sorter.values, lo, lo + remaining);
        if run_len < min_run {
            let forced = remaining.min(min_run);
            binary_sort(sorter.values, lo, lo + forced, lo + run_len);
            run_len = forced;
        }

        sorter.push_run(lo, run_len);
        sorter.merge_collapse();
        lo += run_len;
        remaining -= run_len;
        if remaining == 0 {
            break;
        }
    }
    sorter.merge_force_collapse();
}

fn binary_sort(values: &mut [Suggestion], lo: usize, hi: usize, mut start: usize) {
    if start == lo {
        start += 1;
    }
    while start < hi {
        let pivot = values[start].clone();
        let mut left = lo;
        let mut right = start;
        while left < right {
            let middle = (left + right) >> 1;
            if pivot.compare_to_ignore_case(&values[middle]) == Ordering::Less {
                right = middle;
            } else {
                left = middle + 1;
            }
        }
        values[left..=start].rotate_right(1);
        values[left] = pivot;
        start += 1;
    }
}

fn count_run_and_make_ascending(values: &mut [Suggestion], lo: usize, hi: usize) -> usize {
    let mut run_hi = lo + 1;
    if run_hi == hi {
        return 1;
    }

    if values[run_hi].compare_to_ignore_case(&values[lo]) == Ordering::Less {
        run_hi += 1;
        while run_hi < hi
            && values[run_hi].compare_to_ignore_case(&values[run_hi - 1]) == Ordering::Less
        {
            run_hi += 1;
        }
        values[lo..run_hi].reverse();
    } else {
        run_hi += 1;
        while run_hi < hi
            && values[run_hi].compare_to_ignore_case(&values[run_hi - 1]) != Ordering::Less
        {
            run_hi += 1;
        }
    }
    run_hi - lo
}

fn min_run_length(mut length: usize) -> usize {
    let mut shifted_off = 0;
    while length >= JAVA_TIMSORT_MIN_MERGE {
        shifted_off |= length & 1;
        length >>= 1;
    }
    length + shifted_off
}

struct JavaTimSort<'a> {
    values: &'a mut [Suggestion],
    min_gallop: i32,
    temporary: Vec<Suggestion>,
    run_base: Vec<usize>,
    run_len: Vec<usize>,
}

impl<'a> JavaTimSort<'a> {
    const MIN_GALLOP: usize = 7;
    const INITIAL_TEMPORARY_LENGTH: usize = 256;

    fn new(values: &'a mut [Suggestion]) -> Self {
        let temporary_length = if values.len() < 2 * Self::INITIAL_TEMPORARY_LENGTH {
            values.len() >> 1
        } else {
            Self::INITIAL_TEMPORARY_LENGTH
        };
        let stack_length = if values.len() < 120 {
            5
        } else if values.len() < 1_542 {
            10
        } else if values.len() < 119_151 {
            24
        } else {
            49
        };
        Self {
            values,
            min_gallop: Self::MIN_GALLOP as i32,
            temporary: Vec::with_capacity(temporary_length),
            run_base: Vec::with_capacity(stack_length),
            run_len: Vec::with_capacity(stack_length),
        }
    }

    fn push_run(&mut self, base: usize, length: usize) {
        self.run_base.push(base);
        self.run_len.push(length);
    }

    fn merge_collapse(&mut self) {
        while self.run_len.len() > 1 {
            let mut index = self.run_len.len() - 2;
            if (index > 0
                && self.run_len[index - 1] <= self.run_len[index] + self.run_len[index + 1])
                || (index > 1
                    && self.run_len[index - 2] <= self.run_len[index] + self.run_len[index - 1])
            {
                if self.run_len[index - 1] < self.run_len[index + 1] {
                    index -= 1;
                }
            } else if self.run_len[index] > self.run_len[index + 1] {
                break;
            }
            self.merge_at(index);
        }
    }

    fn merge_force_collapse(&mut self) {
        while self.run_len.len() > 1 {
            let mut index = self.run_len.len() - 2;
            if index > 0 && self.run_len[index - 1] < self.run_len[index + 1] {
                index -= 1;
            }
            self.merge_at(index);
        }
    }

    fn merge_at(&mut self, index: usize) {
        let mut base1 = self.run_base[index];
        let mut len1 = self.run_len[index];
        let base2 = self.run_base[index + 1];
        let mut len2 = self.run_len[index + 1];

        self.run_len[index] = len1 + len2;
        if self.run_len.len() >= 3 && index == self.run_len.len() - 3 {
            self.run_base[index + 1] = self.run_base[index + 2];
            self.run_len[index + 1] = self.run_len[index + 2];
        }
        self.run_base.pop();
        self.run_len.pop();

        let first_of_second = self.values[base2].clone();
        let skipped = gallop_right(&first_of_second, self.values, base1, len1, 0);
        base1 += skipped;
        len1 -= skipped;
        if len1 == 0 {
            return;
        }

        let last_of_first = self.values[base1 + len1 - 1].clone();
        len2 = gallop_left(&last_of_first, self.values, base2, len2, len2 - 1);
        if len2 == 0 {
            return;
        }

        if len1 <= len2 {
            self.merge_lo(base1, len1, base2, len2);
        } else {
            self.merge_hi(base1, len1, base2, len2);
        }
    }

    fn prepare_temporary(&mut self, base: usize, length: usize) {
        self.ensure_capacity(length);
        self.temporary.clear();
        self.temporary
            .extend(self.values[base..base + length].iter().cloned());
    }

    fn ensure_capacity(&mut self, minimum: usize) {
        if self.temporary.capacity() >= minimum {
            return;
        }

        let next_power = minimum
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two)
            .unwrap_or(minimum);
        let new_capacity = next_power.min(self.values.len() >> 1).max(minimum);
        self.temporary
            .reserve_exact(new_capacity - self.temporary.capacity());
    }

    fn merge_lo(&mut self, base1: usize, mut len1: usize, base2: usize, mut len2: usize) {
        self.prepare_temporary(base1, len1);
        let mut cursor1 = 0;
        let mut cursor2 = base2;
        let mut destination = base1;

        self.values[destination] = self.values[cursor2].clone();
        destination += 1;
        cursor2 += 1;
        len2 -= 1;
        if len2 == 0 {
            copy_from_temporary(&self.temporary, cursor1, self.values, destination, len1);
            return;
        }
        if len1 == 1 {
            array_copy(self.values, cursor2, destination, len2);
            self.values[destination + len2] = self.temporary[cursor1].clone();
            return;
        }

        let mut min_gallop = self.min_gallop;
        'outer: loop {
            let mut count1 = 0;
            let mut count2 = 0;

            loop {
                if self.values[cursor2].compare_to_ignore_case(&self.temporary[cursor1])
                    == Ordering::Less
                {
                    self.values[destination] = self.values[cursor2].clone();
                    destination += 1;
                    cursor2 += 1;
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 0 {
                        break 'outer;
                    }
                } else {
                    self.values[destination] = self.temporary[cursor1].clone();
                    destination += 1;
                    cursor1 += 1;
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }

            loop {
                let key = self.values[cursor2].clone();
                count1 = gallop_right(&key, &self.temporary, cursor1, len1, 0) as i32;
                if count1 != 0 {
                    let count = count1 as usize;
                    copy_from_temporary(&self.temporary, cursor1, self.values, destination, count);
                    destination += count;
                    cursor1 += count;
                    len1 -= count;
                    if len1 <= 1 {
                        break 'outer;
                    }
                }
                self.values[destination] = self.values[cursor2].clone();
                destination += 1;
                cursor2 += 1;
                len2 -= 1;
                if len2 == 0 {
                    break 'outer;
                }

                let key = self.temporary[cursor1].clone();
                count2 = gallop_left(&key, self.values, cursor2, len2, 0) as i32;
                if count2 != 0 {
                    let count = count2 as usize;
                    array_copy(self.values, cursor2, destination, count);
                    destination += count;
                    cursor2 += count;
                    len2 -= count;
                    if len2 == 0 {
                        break 'outer;
                    }
                }
                self.values[destination] = self.temporary[cursor1].clone();
                destination += 1;
                cursor1 += 1;
                len1 -= 1;
                if len1 == 1 {
                    break 'outer;
                }
                min_gallop -= 1;
                if count1 < Self::MIN_GALLOP as i32 && count2 < Self::MIN_GALLOP as i32 {
                    break;
                }
            }
            min_gallop = min_gallop.max(0) + 2;
        }
        self.min_gallop = min_gallop.max(1);

        if len1 == 1 {
            array_copy(self.values, cursor2, destination, len2);
            self.values[destination + len2] = self.temporary[cursor1].clone();
        } else if len1 == 0 {
            panic!("Comparison method violates its general contract!");
        } else {
            copy_from_temporary(&self.temporary, cursor1, self.values, destination, len1);
        }
    }

    fn merge_hi(&mut self, base1: usize, mut len1: usize, base2: usize, mut len2: usize) {
        self.prepare_temporary(base2, len2);
        let mut cursor1 = (base1 + len1 - 1) as isize;
        let mut cursor2 = (len2 - 1) as isize;
        let mut destination = (base2 + len2 - 1) as isize;

        self.values[destination as usize] = self.values[cursor1 as usize].clone();
        destination -= 1;
        cursor1 -= 1;
        len1 -= 1;
        if len1 == 0 {
            copy_from_temporary(
                &self.temporary,
                0,
                self.values,
                (destination - (len2 - 1) as isize) as usize,
                len2,
            );
            return;
        }
        if len2 == 1 {
            destination -= len1 as isize;
            cursor1 -= len1 as isize;
            array_copy(
                self.values,
                (cursor1 + 1) as usize,
                (destination + 1) as usize,
                len1,
            );
            self.values[destination as usize] = self.temporary[cursor2 as usize].clone();
            return;
        }

        let mut min_gallop = self.min_gallop;
        'outer: loop {
            let mut count1 = 0;
            let mut count2 = 0;

            loop {
                if self.temporary[cursor2 as usize]
                    .compare_to_ignore_case(&self.values[cursor1 as usize])
                    == Ordering::Less
                {
                    self.values[destination as usize] = self.values[cursor1 as usize].clone();
                    destination -= 1;
                    cursor1 -= 1;
                    count1 += 1;
                    count2 = 0;
                    len1 -= 1;
                    if len1 == 0 {
                        break 'outer;
                    }
                } else {
                    self.values[destination as usize] = self.temporary[cursor2 as usize].clone();
                    destination -= 1;
                    cursor2 -= 1;
                    count2 += 1;
                    count1 = 0;
                    len2 -= 1;
                    if len2 == 1 {
                        break 'outer;
                    }
                }
                if (count1 | count2) >= min_gallop {
                    break;
                }
            }

            loop {
                let key = self.temporary[cursor2 as usize].clone();
                count1 = (len1 - gallop_right(&key, self.values, base1, len1, len1 - 1)) as i32;
                if count1 != 0 {
                    let count = count1 as usize;
                    destination -= count as isize;
                    cursor1 -= count as isize;
                    len1 -= count;
                    array_copy(
                        self.values,
                        (cursor1 + 1) as usize,
                        (destination + 1) as usize,
                        count,
                    );
                    if len1 == 0 {
                        break 'outer;
                    }
                }
                self.values[destination as usize] = self.temporary[cursor2 as usize].clone();
                destination -= 1;
                cursor2 -= 1;
                len2 -= 1;
                if len2 == 1 {
                    break 'outer;
                }

                let key = self.values[cursor1 as usize].clone();
                count2 = (len2 - gallop_left(&key, &self.temporary, 0, len2, len2 - 1)) as i32;
                if count2 != 0 {
                    let count = count2 as usize;
                    destination -= count as isize;
                    cursor2 -= count as isize;
                    len2 -= count;
                    copy_from_temporary(
                        &self.temporary,
                        (cursor2 + 1) as usize,
                        self.values,
                        (destination + 1) as usize,
                        count,
                    );
                    if len2 <= 1 {
                        break 'outer;
                    }
                }
                self.values[destination as usize] = self.values[cursor1 as usize].clone();
                destination -= 1;
                cursor1 -= 1;
                len1 -= 1;
                if len1 == 0 {
                    break 'outer;
                }
                min_gallop -= 1;
                if count1 < Self::MIN_GALLOP as i32 && count2 < Self::MIN_GALLOP as i32 {
                    break;
                }
            }
            min_gallop = min_gallop.max(0) + 2;
        }
        self.min_gallop = min_gallop.max(1);

        if len2 == 1 {
            destination -= len1 as isize;
            cursor1 -= len1 as isize;
            array_copy(
                self.values,
                (cursor1 + 1) as usize,
                (destination + 1) as usize,
                len1,
            );
            self.values[destination as usize] = self.temporary[cursor2 as usize].clone();
        } else if len2 == 0 {
            panic!("Comparison method violates its general contract!");
        } else {
            copy_from_temporary(
                &self.temporary,
                0,
                self.values,
                (destination - (len2 - 1) as isize) as usize,
                len2,
            );
        }
    }
}

fn gallop_left(
    key: &Suggestion,
    values: &[Suggestion],
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let mut last_offset = 0_isize;
    let mut offset = 1_isize;
    if key.compare_to_ignore_case(&values[base + hint]) == Ordering::Greater {
        let maximum = (length - hint) as isize;
        while offset < maximum
            && key.compare_to_ignore_case(&values[base + hint + offset as usize])
                == Ordering::Greater
        {
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        last_offset += hint as isize;
        offset += hint as isize;
    } else {
        let maximum = (hint + 1) as isize;
        while offset < maximum
            && key.compare_to_ignore_case(&values[base + hint - offset as usize])
                != Ordering::Greater
        {
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        let previous_last = last_offset;
        last_offset = hint as isize - offset;
        offset = hint as isize - previous_last;
    }

    last_offset += 1;
    while last_offset < offset {
        let middle = last_offset + ((offset - last_offset) >> 1);
        if key.compare_to_ignore_case(&values[base + middle as usize]) == Ordering::Greater {
            last_offset = middle + 1;
        } else {
            offset = middle;
        }
    }
    offset as usize
}

fn gallop_right(
    key: &Suggestion,
    values: &[Suggestion],
    base: usize,
    length: usize,
    hint: usize,
) -> usize {
    let mut offset = 1_isize;
    let mut last_offset = 0_isize;
    if key.compare_to_ignore_case(&values[base + hint]) == Ordering::Less {
        let maximum = (hint + 1) as isize;
        while offset < maximum
            && key.compare_to_ignore_case(&values[base + hint - offset as usize]) == Ordering::Less
        {
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        let previous_last = last_offset;
        last_offset = hint as isize - offset;
        offset = hint as isize - previous_last;
    } else {
        let maximum = (length - hint) as isize;
        while offset < maximum
            && key.compare_to_ignore_case(&values[base + hint + offset as usize]) != Ordering::Less
        {
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        last_offset += hint as isize;
        offset += hint as isize;
    }

    last_offset += 1;
    while last_offset < offset {
        let middle = last_offset + ((offset - last_offset) >> 1);
        if key.compare_to_ignore_case(&values[base + middle as usize]) == Ordering::Less {
            offset = middle;
        } else {
            last_offset = middle + 1;
        }
    }
    offset as usize
}

fn array_copy(values: &mut [Suggestion], source: usize, destination: usize, length: usize) {
    if destination > source && destination < source + length {
        for offset in (0..length).rev() {
            values[destination + offset] = values[source + offset].clone();
        }
    } else {
        for offset in 0..length {
            values[destination + offset] = values[source + offset].clone();
        }
    }
}

fn copy_from_temporary(
    temporary: &[Suggestion],
    source: usize,
    values: &mut [Suggestion],
    destination: usize,
    length: usize,
) {
    values[destination..destination + length].clone_from_slice(&temporary[source..source + length]);
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
    // Mojang Java 25's Character(char) tables differ from Rust's scalar mappings here.
    match unit {
        0x1f80..=0x1f87 | 0x1f90..=0x1f97 | 0x1fa0..=0x1fa7 => return unit + 8,
        0x1fb3 => return 0x1fbc,
        0x1fc3 => return 0x1fcc,
        0x1ff3 => return 0x1ffc,
        _ if java25_preserves_case(u32::from(unit)) => return unit,
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
    if java25_preserves_case(code_point) {
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
    // These mappings were added after the Unicode table used by Mojang Java 25.
    if java25_preserves_case(u32::from(unit)) {
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
    if java25_preserves_case(code_point) {
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

    fn timsort_item(id: usize, specification: &str) -> Suggestion {
        let range = at(id);
        match specification.strip_prefix('i') {
            Some(value) => IntegerSuggestion::new(range, value.parse().unwrap()).into_suggestion(),
            None => suggestion(range, specification),
        }
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
        assert_eq!(
            texts(&result),
            [
                "11", "2", "22", "33", "3a", "4", "6", "8", "30", "32", "a", "a3", "b", "c"
            ]
        );
    }

    #[test]
    fn create_uses_java_hash_set_iteration_before_sorting() {
        let case_tie = Suggestions::create("", [suggestion(at(0), "ab"), suggestion(at(0), "AB")]);
        assert_eq!(texts(&case_tie), ["AB", "ab"]);

        let mixed = Suggestions::create(
            "",
            [
                IntegerSuggestion::new(at(0), 4).into_suggestion(),
                IntegerSuggestion::new(at(0), 11).into_suggestion(),
                suggestion(at(0), "3a"),
            ],
        );
        assert_eq!(texts(&mixed), ["3a", "4", "11"]);
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
    fn simple_case_mapping_matches_mojang_java_25() {
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
    fn sort_mixed_long_matches_java_25_object_timsort() {
        let input = [
            "i22", "i8", "i30", "i22", "i2", "i3", "i30", "i4", "a", "A", "b", "a", "i2", "i8",
            "i2", "a3", "3a", "i3", "i32", "i4", "i22", "b", "i1", "i3", "i32", "i2", "3a", "i4",
            "c", "c", "b", "i2", "i2", "i1", "3a", "i2", "i30", "i30", "i32", "b", "b", "c", "A",
            "i22", "i32", "A", "i4", "b", "b", "i2", "i11", "i4", "A", "i22", "3a", "b", "i3",
            "i3", "i4", "i1", "i22", "c", "a3", "A", "a3", "i22", "i8", "i1", "b", "c", "b", "b",
            "i8", "i1", "A", "i32", "i1", "a", "c", "A",
        ];
        let mut values: Vec<_> = input
            .iter()
            .enumerate()
            .map(|(id, specification)| timsort_item(id, specification))
            .collect();

        java_stable_sort(&mut values);

        let ids: Vec<_> = values
            .iter()
            .map(|suggestion| suggestion.range().start())
            .collect();
        assert_eq!(
            ids,
            [
                22, 33, 59, 67, 73, 76, 4, 12, 14, 25, 31, 32, 35, 49, 5, 17, 23, 56, 57, 46, 51,
                50, 43, 53, 44, 54, 7, 19, 27, 58, 1, 13, 66, 72, 0, 3, 20, 60, 65, 2, 6, 36, 37,
                18, 24, 38, 75, 16, 26, 34, 8, 9, 11, 42, 45, 52, 63, 74, 77, 79, 15, 62, 64, 10,
                21, 30, 39, 40, 47, 48, 55, 68, 70, 71, 28, 29, 41, 61, 69, 78,
            ]
        );
    }

    #[test]
    fn randomized_sort_matches_java_25_object_timsort_digest() {
        const TEXTS: &[&str] = &[
            "1", "2", "3", "4", "8", "10", "11", "12", "20", "22", "30", "32", "3a", "a3", "a",
            "A", "b", "B", "c", "C", "00", "01", "9z",
        ];
        const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
        const INCREMENT: u64 = 1_442_695_040_888_963_407;
        const FNV_PRIME: u64 = 1_099_511_628_211;

        fn next(state: &mut u64) -> u64 {
            *state = state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
            *state
        }

        fn mix(hash: u64, value: u64) -> u64 {
            (hash ^ value).wrapping_mul(FNV_PRIME)
        }

        let mut state = 0x5eed_1234;
        let mut hash = 0xcbf2_9ce4_8422_2325;
        let mut failures = 0;
        for trial in 0..4_096 {
            let length = 32 + ((next(&mut state) >> 1) % 225) as usize;
            let mut values = Vec::with_capacity(length);
            for id in 0..length {
                let text = TEXTS[((next(&mut state) >> 1) % TEXTS.len() as u64) as usize];
                let integer = ((next(&mut state) >> 17) & 1) != 0
                    && text.bytes().all(|byte| byte.is_ascii_digit());
                let value = if integer {
                    IntegerSuggestion::new(at(id), text.parse().unwrap()).into_suggestion()
                } else {
                    suggestion(at(id), text)
                };
                values.push(value);
            }

            hash = mix(hash, trial);
            hash = mix(hash, length as u64);
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                java_stable_sort(&mut values);
            }))
            .is_err()
            {
                failures += 1;
                hash = mix(hash, u64::MAX);
            } else {
                hash = mix(hash, 0);
                for value in values {
                    hash = mix(hash, value.range().start() as u64 + 1);
                }
            }
        }

        assert_eq!(failures, 1);
        assert_eq!(hash, 5_921_293_081_399_281_092);
    }

    #[test]
    #[should_panic(expected = "Comparison method violates its general contract!")]
    fn non_transitive_comparison_fails_when_java_25_object_timsort_fails() {
        const TEXTS: &[&str] = &["2", "10", "11"];
        const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
        const INCREMENT: u64 = 1_442_695_040_888_963_407;

        let mut state = 0x19a5_c0de_u64;
        let mut values = Vec::with_capacity(128);
        for id in 0..128 {
            state = state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
            let text = TEXTS[((state >> 1) % TEXTS.len() as u64) as usize];
            state = state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
            let value = if ((state >> 17) & 1) != 0 {
                IntegerSuggestion::new(at(id), text.parse().unwrap()).into_suggestion()
            } else {
                suggestion(at(id), text)
            };
            values.push(value);
        }

        java_stable_sort(&mut values);
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
    fn lowercase_uses_mojang_java_character_data() {
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
