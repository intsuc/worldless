use std::{
    collections::{BTreeMap, HashMap, hash_map::Entry},
    fmt,
};

use worldless_brigadier::{
    StringReader,
    exceptions::{java_f32, java_f64},
};

use crate::resource::Identifier;

const MAX_DEPTH: usize = 512;

#[derive(Clone, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct JavaString(Vec<u16>);

impl JavaString {
    pub(crate) fn from_units(units: Vec<u16>) -> Self {
        Self(units)
    }

    pub(crate) fn units(&self) -> &[u16] {
        &self.0
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn substring(&self, start: i32, end: Option<i32>) -> Result<Self, String> {
        let length = i32::try_from(self.len()).unwrap_or(i32::MAX);
        let start = if start < 0 {
            length.saturating_add(start)
        } else {
            start
        };
        let end = end.map_or(length, |end| {
            if end < 0 {
                length.saturating_add(end)
            } else {
                end
            }
        });
        if start < 0 || end > length || start > end {
            return Err(format!("invalid substring range {start}..{end}"));
        }
        Ok(Self(self.0[start as usize..end as usize].to_vec()))
    }

    pub(crate) fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }

    fn eq_ascii_ignore_case(&self, value: &[u8]) -> bool {
        self.0.len() == value.len()
            && self
                .0
                .iter()
                .zip(value)
                .all(|(&left, &right)| left <= 0x7f && (left as u8).eq_ignore_ascii_case(&right))
    }

    fn eq_ascii(&self, value: &[u8]) -> bool {
        self.0.len() == value.len()
            && self
                .0
                .iter()
                .zip(value)
                .all(|(&left, &right)| left == u16::from(right))
    }
}

impl From<&str> for JavaString {
    fn from(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
}

impl PartialEq<str> for JavaString {
    fn eq(&self, other: &str) -> bool {
        self.units().iter().copied().eq(other.encode_utf16())
    }
}

impl fmt::Debug for JavaString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string_lossy().fmt(formatter)
    }
}

impl fmt::Display for JavaString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_string_lossy())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompoundTag(BTreeMap<JavaString, Tag>);

impl CompoundTag {
    pub(crate) fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn get(&self, name: &JavaString) -> Option<&Tag> {
        self.0.get(name)
    }

    pub(crate) fn get_mut(&mut self, name: &JavaString) -> Option<&mut Tag> {
        self.0.get_mut(name)
    }

    pub(crate) fn contains_key(&self, name: &JavaString) -> bool {
        self.0.contains_key(name)
    }

    pub(crate) fn insert(&mut self, name: JavaString, tag: Tag) -> Option<Tag> {
        self.0.insert(name, tag)
    }

    pub(crate) fn remove(&mut self, name: &JavaString) -> Option<Tag> {
        self.0.remove(name)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Tag> {
        self.0.values()
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&JavaString, &Tag)> {
        self.0.iter()
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        for (name, other_tag) in &other.0 {
            match (self.0.get_mut(name), other_tag) {
                (Some(Tag::Compound(current)), Tag::Compound(other)) => current.merge(other),
                _ => {
                    self.0.insert(name.clone(), other_tag.clone());
                }
            }
        }
    }
}

impl Default for CompoundTag {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(u32),
    Double(u64),
    ByteArray(Vec<i8>),
    String(JavaString),
    List(Vec<Tag>),
    Compound(CompoundTag),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub(crate) fn float(value: f32) -> Self {
        Self::Float(if value == 0.0 {
            0.0_f32.to_bits()
        } else {
            value.to_bits()
        })
    }

    pub(crate) fn double(value: f64) -> Self {
        Self::Double(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        })
    }

    pub(crate) fn as_compound(&self) -> Option<&CompoundTag> {
        match self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_compound_mut(&mut self) -> Option<&mut CompoundTag> {
        match self {
            Self::Compound(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_string(&self) -> Option<&JavaString> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn double_value(&self) -> Option<f64> {
        match self {
            Self::Byte(value) => Some(f64::from(*value)),
            Self::Short(value) => Some(f64::from(*value)),
            Self::Int(value) => Some(f64::from(*value)),
            Self::Long(value) => Some(*value as f64),
            Self::Float(bits) => Some(f64::from(f32::from_bits(*bits))),
            Self::Double(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }

    pub(crate) fn byte_value(&self) -> Option<i8> {
        match self {
            Self::Byte(value) => Some(*value),
            Self::Short(value) => Some(*value as i8),
            Self::Int(value) => Some(*value as i8),
            Self::Long(value) => Some(*value as i8),
            Self::Float(bits) => Some(java_floor_f32_to_i32(f32::from_bits(*bits)) as i8),
            Self::Double(bits) => Some(java_floor_f64_to_i32(f64::from_bits(*bits)) as i8),
            _ => None,
        }
    }

    pub(crate) fn int_value(&self) -> Option<i32> {
        match self {
            Self::Byte(value) => Some(i32::from(*value)),
            Self::Short(value) => Some(i32::from(*value)),
            Self::Int(value) => Some(*value),
            Self::Long(value) => Some(*value as i32),
            Self::Float(bits) => Some(java_floor_f32_to_i32(f32::from_bits(*bits))),
            Self::Double(bits) => Some(java_floor_f64_to_i32(f64::from_bits(*bits))),
            _ => None,
        }
    }

    pub(crate) fn long_value(&self) -> Option<i64> {
        match self {
            Self::Byte(value) => Some(i64::from(*value)),
            Self::Short(value) => Some(i64::from(*value)),
            Self::Int(value) => Some(i64::from(*value)),
            Self::Long(value) => Some(*value),
            Self::Float(bits) => Some(f32::from_bits(*bits) as i64),
            Self::Double(bits) => Some(java_floor_f64_to_i64(f64::from_bits(*bits))),
            _ => None,
        }
    }

    pub(crate) fn collection_len(&self) -> Option<usize> {
        match self {
            Self::ByteArray(values) => Some(values.len()),
            Self::List(values) => Some(values.len()),
            Self::IntArray(values) => Some(values.len()),
            Self::LongArray(values) => Some(values.len()),
            _ => None,
        }
    }

    pub(crate) fn is_too_deep(&self, depth: usize) -> bool {
        let mut stack = vec![(self, depth)];
        while let Some((tag, depth)) = stack.pop() {
            if depth >= MAX_DEPTH {
                return true;
            }
            match tag {
                Self::Compound(compound) => {
                    stack.extend(compound.values().map(|child| (child, depth + 1)));
                }
                Self::List(list) => {
                    stack.extend(list.iter().map(|child| (child, depth + 1)));
                }
                _ => {}
            }
        }
        false
    }

    pub(crate) fn macro_stringify(&self) -> JavaString {
        match self {
            Self::Byte(value) => JavaString::from(value.to_string().as_str()),
            Self::Short(value) => JavaString::from(value.to_string().as_str()),
            Self::Long(value) => JavaString::from(value.to_string().as_str()),
            Self::Float(bits) => {
                JavaString::from(format_macro_decimal(f64::from(f32::from_bits(*bits))).as_str())
            }
            Self::Double(bits) => {
                JavaString::from(format_macro_decimal(f64::from_bits(*bits)).as_str())
            }
            Self::String(value) => value.clone(),
            _ => compact_stringify(self),
        }
    }

    pub(crate) fn primitive_text(&self) -> Option<JavaString> {
        match self {
            Self::Byte(_)
            | Self::Short(_)
            | Self::Int(_)
            | Self::Long(_)
            | Self::Float(_)
            | Self::Double(_) => Some(compact_stringify(self)),
            Self::String(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn collection_element(&self, index: usize) -> Option<Tag> {
        match self {
            Self::ByteArray(values) => values.get(index).copied().map(Self::Byte),
            Self::List(values) => values.get(index).cloned(),
            Self::IntArray(values) => values.get(index).copied().map(Self::Int),
            Self::LongArray(values) => values.get(index).copied().map(Self::Long),
            _ => None,
        }
    }

    fn collection_clear(&mut self) -> bool {
        match self {
            Self::ByteArray(values) => {
                values.clear();
                true
            }
            Self::List(values) => {
                values.clear();
                true
            }
            Self::IntArray(values) => {
                values.clear();
                true
            }
            Self::LongArray(values) => {
                values.clear();
                true
            }
            _ => false,
        }
    }

    fn collection_set(&mut self, index: usize, value: Tag) -> bool {
        match self {
            Self::ByteArray(values) => value.byte_value().is_some_and(|value| {
                values[index] = value;
                true
            }),
            Self::List(values) => {
                values[index] = value;
                true
            }
            Self::IntArray(values) => value.int_value().is_some_and(|value| {
                values[index] = value;
                true
            }),
            Self::LongArray(values) => value.long_value().is_some_and(|value| {
                values[index] = value;
                true
            }),
            _ => false,
        }
    }

    fn collection_insert(&mut self, index: usize, value: Tag) -> Result<bool, String> {
        let length = self
            .collection_len()
            .ok_or_else(|| "expected a list".to_owned())?;
        if index > length {
            return Err(format!("invalid list index {index}"));
        }
        Ok(match self {
            Self::ByteArray(values) => value.byte_value().is_some_and(|value| {
                values.insert(index, value);
                true
            }),
            Self::List(values) => {
                values.insert(index, value);
                true
            }
            Self::IntArray(values) => value.int_value().is_some_and(|value| {
                values.insert(index, value);
                true
            }),
            Self::LongArray(values) => value.long_value().is_some_and(|value| {
                values.insert(index, value);
                true
            }),
            _ => unreachable!("the collection length check resolved the tag type"),
        })
    }

    fn collection_remove(&mut self, index: usize) -> bool {
        match self {
            Self::ByteArray(values) => {
                values.remove(index);
                true
            }
            Self::List(values) => {
                values.remove(index);
                true
            }
            Self::IntArray(values) => {
                values.remove(index);
                true
            }
            Self::LongArray(values) => {
                values.remove(index);
                true
            }
            _ => false,
        }
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Byte(left), Self::Byte(right)) => left == right,
            (Self::Short(left), Self::Short(right)) => left == right,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Long(left), Self::Long(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => {
                canonical_f32_bits(*left) == canonical_f32_bits(*right)
            }
            (Self::Double(left), Self::Double(right)) => {
                canonical_f64_bits(*left) == canonical_f64_bits(*right)
            }
            (Self::ByteArray(left), Self::ByteArray(right)) => left == right,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Compound(left), Self::Compound(right)) => left == right,
            (Self::IntArray(left), Self::IntArray(right)) => left == right,
            (Self::LongArray(left), Self::LongArray(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for Tag {}

fn compact_stringify(tag: &Tag) -> JavaString {
    let mut output = Vec::new();
    write_compact_tag(tag, &mut output);
    JavaString::from_units(output)
}

fn write_compact_tag(tag: &Tag, output: &mut Vec<u16>) {
    match tag {
        Tag::Byte(value) => {
            push_ascii(output, &value.to_string());
            output.push(u16::from(b'b'));
        }
        Tag::Short(value) => {
            push_ascii(output, &value.to_string());
            output.push(u16::from(b's'));
        }
        Tag::Int(value) => push_ascii(output, &value.to_string()),
        Tag::Long(value) => {
            push_ascii(output, &value.to_string());
            output.push(u16::from(b'L'));
        }
        Tag::Float(bits) => {
            push_ascii(output, &java_f32(f32::from_bits(*bits)));
            output.push(u16::from(b'f'));
        }
        Tag::Double(bits) => {
            push_ascii(output, &java_f64(f64::from_bits(*bits)));
            output.push(u16::from(b'd'));
        }
        Tag::ByteArray(values) => {
            push_ascii(output, "[B;");
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                push_ascii(output, &value.to_string());
                output.push(u16::from(b'B'));
            }
            output.push(u16::from(b']'));
        }
        Tag::String(value) => write_quoted(value, output),
        Tag::List(values) => {
            output.push(u16::from(b'['));
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                write_compact_tag(value, output);
            }
            output.push(u16::from(b']'));
        }
        Tag::Compound(compound) => {
            output.push(u16::from(b'{'));
            for (index, (name, value)) in compound.0.iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                if is_unquoted_compound_key(name) {
                    output.extend_from_slice(name.units());
                } else {
                    write_quoted(name, output);
                }
                output.push(u16::from(b':'));
                write_compact_tag(value, output);
            }
            output.push(u16::from(b'}'));
        }
        Tag::IntArray(values) => {
            push_ascii(output, "[I;");
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                push_ascii(output, &value.to_string());
            }
            output.push(u16::from(b']'));
        }
        Tag::LongArray(values) => {
            push_ascii(output, "[L;");
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                push_ascii(output, &value.to_string());
                output.push(u16::from(b'L'));
            }
            output.push(u16::from(b']'));
        }
    }
}

fn push_ascii(output: &mut Vec<u16>, value: &str) {
    output.extend(value.bytes().map(u16::from));
}

fn is_unquoted_compound_key(value: &JavaString) -> bool {
    if value.eq_ascii_ignore_case(b"true") || value.eq_ascii_ignore_case(b"false") {
        return false;
    }
    let Some((&first, remaining)) = value.units().split_first() else {
        return false;
    };
    (is_ascii_letter(first) || first == u16::from(b'.') || first == u16::from(b'_'))
        && remaining.iter().copied().all(|unit| {
            is_ascii_letter(unit)
                || matches!(unit, 0x30..=0x39)
                || matches!(unit, 0x2b | 0x2d | 0x2e | 0x5f)
        })
}

fn is_ascii_letter(unit: u16) -> bool {
    matches!(unit, 0x41..=0x5a | 0x61..=0x7a)
}

fn write_quoted(value: &JavaString, output: &mut Vec<u16>) {
    let quote = value
        .units()
        .iter()
        .copied()
        .find_map(|unit| match unit {
            0x22 => Some(0x27),
            0x27 => Some(0x22),
            _ => None,
        })
        .unwrap_or(0x22);
    output.push(quote);
    for &unit in value.units() {
        match unit {
            0x5c => push_ascii(output, "\\\\"),
            0x08 => push_ascii(output, "\\b"),
            0x09 => push_ascii(output, "\\t"),
            0x0a => push_ascii(output, "\\n"),
            0x0c => push_ascii(output, "\\f"),
            0x0d => push_ascii(output, "\\r"),
            0x00..=0x1f => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                push_ascii(output, "\\x");
                output.push(u16::from(HEX[usize::from(unit >> 4)]));
                output.push(u16::from(HEX[usize::from(unit & 0x0f)]));
            }
            _ => {
                if unit == quote {
                    output.push(u16::from(b'\\'));
                }
                output.push(unit);
            }
        }
    }
    output.push(quote);
}

fn format_macro_decimal(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-∞".to_owned()
        } else {
            "∞".to_owned()
        };
    }

    // Apply the public Java SE rounding contract to the exact binary value;
    // JAVA_COMPATIBILITY.md excludes a particular JDK's private digit converter.
    let negative = value.is_sign_negative();
    let mut output = format!("{:.15}", value.abs());
    while output.ends_with('0') {
        output.pop();
    }
    if output.ends_with('.') {
        output.pop();
    }
    if output.starts_with("0.") {
        output.remove(0);
    }
    if negative {
        output.insert(0, '-');
    }
    output
}

fn canonical_f32_bits(bits: u32) -> u32 {
    let value = f32::from_bits(bits);
    if value.is_nan() {
        f32::NAN.to_bits()
    } else {
        bits
    }
}

fn canonical_f64_bits(bits: u64) -> u64 {
    let value = f64::from_bits(bits);
    if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        bits
    }
}

fn java_floor_f32_to_i32(value: f32) -> i32 {
    value.floor() as i32
}

fn java_floor_f64_to_i32(value: f64) -> i32 {
    value.floor() as i32
}

fn java_floor_f64_to_i64(value: f64) -> i64 {
    value.floor() as i64
}

pub(crate) fn parse_tag(reader: &mut StringReader) -> Result<Tag, String> {
    SnbtParser { reader }.parse_tag()
}

pub(crate) fn parse_compound(reader: &mut StringReader) -> Result<CompoundTag, String> {
    match parse_tag(reader)? {
        Tag::Compound(compound) => Ok(compound),
        _ => Err("expected a compound tag".to_owned()),
    }
}

pub(crate) fn parse_compound_fully(input: &str) -> Result<CompoundTag, String> {
    let mut reader = StringReader::new(input);
    let compound = parse_compound(&mut reader)?;
    reader.skip_whitespace();
    if reader.can_read() {
        Err(format!("trailing data at position {}", reader.cursor()))
    } else {
        Ok(compound)
    }
}

struct SnbtParser<'a> {
    reader: &'a mut StringReader,
}

impl SnbtParser<'_> {
    fn parse_tag(&mut self) -> Result<Tag, String> {
        self.reader.skip_whitespace();
        let Some(next) = self.peek() else {
            return Err("expected an NBT value".to_owned());
        };
        match next {
            0x7b => self.parse_compound().map(Tag::Compound),
            0x5b => self.parse_list_or_array(),
            0x22 | 0x27 => self.parse_quoted().map(Tag::String),
            0x2b | 0x2d | 0x2e | 0x30..=0x39 => self.parse_number(),
            _ => self.parse_unquoted_or_builtin(),
        }
    }

    fn parse_compound(&mut self) -> Result<CompoundTag, String> {
        self.expect(0x7b)?;
        let mut result = CompoundTag::new();
        if self.try_character(0x7d) {
            return Ok(result);
        }
        loop {
            let key = self.parse_key()?;
            if key.len() == 0 {
                return Err("empty compound key".to_owned());
            }
            self.expect(0x3a)?;
            let value = self.parse_tag()?;
            result.insert(key, value);
            if self.try_character(0x7d) {
                break;
            }
            self.expect(0x2c)?;
            if self.try_character(0x7d) {
                break;
            }
        }
        Ok(result)
    }

    fn parse_key(&mut self) -> Result<JavaString, String> {
        self.reader.skip_whitespace();
        match self.peek() {
            Some(0x22 | 0x27) => self.parse_quoted(),
            _ => self.parse_unquoted(),
        }
    }

    fn parse_list_or_array(&mut self) -> Result<Tag, String> {
        self.expect(0x5b)?;
        let after_open = self.reader.cursor();
        self.reader.skip_whitespace();
        let array_kind = match self.peek() {
            Some(0x42 | 0x49 | 0x4c) => {
                let kind = self.read();
                if self.try_character(0x3b) {
                    Some(kind)
                } else {
                    self.reader.set_cursor(after_open);
                    None
                }
            }
            _ => None,
        };
        if let Some(kind) = array_kind {
            return self.parse_array(kind);
        }

        let mut values = Vec::new();
        if self.try_character(0x5d) {
            return Ok(Tag::List(values));
        }
        loop {
            values.push(self.parse_tag()?);
            if self.try_character(0x5d) {
                break;
            }
            self.expect(0x2c)?;
            if self.try_character(0x5d) {
                break;
            }
        }
        Ok(Tag::List(values))
    }

    fn parse_array(&mut self, kind: u16) -> Result<Tag, String> {
        let (default_kind, allowed_kinds) = match kind {
            0x42 => (IntegerKind::Byte, &[IntegerKind::Byte][..]),
            0x49 => (
                IntegerKind::Int,
                &[IntegerKind::Byte, IntegerKind::Short, IntegerKind::Int][..],
            ),
            0x4c => (
                IntegerKind::Long,
                &[
                    IntegerKind::Byte,
                    IntegerKind::Short,
                    IntegerKind::Int,
                    IntegerKind::Long,
                ][..],
            ),
            _ => unreachable!("typed array prefixes are validated before parsing"),
        };
        let mut entries = Vec::new();
        if !self.try_character(0x5d) {
            loop {
                let value = self.parse_integer_as(default_kind, allowed_kinds)?;
                entries.push(value);
                if self.try_character(0x5d) {
                    break;
                }
                self.expect(0x2c)?;
                if self.try_character(0x5d) {
                    break;
                }
            }
        }
        match kind {
            0x42 => entries
                .into_iter()
                .map(|tag| match tag {
                    Tag::Byte(value) => Ok(value),
                    _ => Err("invalid byte-array element type".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Tag::ByteArray),
            0x49 => entries
                .into_iter()
                .map(|tag| match tag {
                    Tag::Byte(value) => Ok(i32::from(value)),
                    Tag::Short(value) => Ok(i32::from(value)),
                    Tag::Int(value) => Ok(value),
                    _ => Err("invalid int-array element type".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Tag::IntArray),
            0x4c => entries
                .into_iter()
                .map(|tag| match tag {
                    Tag::Byte(value) => Ok(i64::from(value)),
                    Tag::Short(value) => Ok(i64::from(value)),
                    Tag::Int(value) => Ok(i64::from(value)),
                    Tag::Long(value) => Ok(value),
                    _ => Err("invalid long-array element type".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Tag::LongArray),
            _ => unreachable!("typed array prefixes are validated before parsing"),
        }
    }

    fn parse_number(&mut self) -> Result<Tag, String> {
        let start = self.reader.cursor();
        if let Ok(value) = self.parse_float() {
            return Ok(value);
        }
        self.reader.set_cursor(start);
        self.parse_integer()
    }

    fn parse_float(&mut self) -> Result<Tag, String> {
        let start = self.reader.cursor();
        let sign = self.parse_sign();
        let whole = self.try_digit_run(10)?;
        let mut fraction = None;
        let mut exponent = None;
        let mut has_float_marker = false;

        if self.try_character(0x2e) {
            has_float_marker = true;
            fraction = self.try_digit_run(10)?;
            if whole.is_none() && fraction.is_none() {
                self.reader.set_cursor(start);
                return Err("invalid floating-point literal".to_owned());
            }
        } else if whole.is_none() {
            self.reader.set_cursor(start);
            return Err("invalid floating-point literal".to_owned());
        }

        if self.try_one_of(&[0x65, 0x45]).is_some() {
            has_float_marker = true;
            let exponent_sign = self.parse_sign();
            let digits = self
                .try_digit_run(10)?
                .ok_or_else(|| "missing floating-point exponent".to_owned())?;
            exponent = Some((exponent_sign, digits));
        }

        let suffix = self.try_one_of(&[0x66, 0x46, 0x64, 0x44]);
        has_float_marker |= suffix.is_some();
        if !has_float_marker {
            self.reader.set_cursor(start);
            return Err("not a floating-point literal".to_owned());
        }

        let mut text = String::new();
        if sign < 0 {
            text.push('-');
        }
        if let Some(whole) = whole {
            text.push_str(&remove_underscores(&whole));
        } else {
            text.push('0');
        }
        if fraction.is_some() || self.consumed_character_since(start, 0x2e) {
            text.push('.');
            if let Some(fraction) = fraction {
                text.push_str(&remove_underscores(&fraction));
            }
        }
        if let Some((exponent_sign, exponent)) = exponent {
            text.push('e');
            if exponent_sign < 0 {
                text.push('-');
            }
            text.push_str(&remove_underscores(&exponent));
        }

        match suffix {
            Some(0x66 | 0x46) => {
                let value = text
                    .parse::<f32>()
                    .map_err(|error| format!("invalid float: {error}"))?;
                if value.is_finite() {
                    Ok(Tag::float(value))
                } else {
                    Err("infinite floating-point values are not allowed".to_owned())
                }
            }
            _ => {
                let value = text
                    .parse::<f64>()
                    .map_err(|error| format!("invalid double: {error}"))?;
                if value.is_finite() {
                    Ok(Tag::double(value))
                } else {
                    Err("infinite floating-point values are not allowed".to_owned())
                }
            }
        }
    }

    fn parse_integer(&mut self) -> Result<Tag, String> {
        self.parse_integer_as(
            IntegerKind::Int,
            &[
                IntegerKind::Byte,
                IntegerKind::Short,
                IntegerKind::Int,
                IntegerKind::Long,
            ],
        )
    }

    fn parse_integer_as(
        &mut self,
        default_kind: IntegerKind,
        allowed_kinds: &[IntegerKind],
    ) -> Result<Tag, String> {
        self.reader.skip_whitespace();
        let sign = self.parse_sign();
        self.reader.skip_whitespace();
        let mut radix = 10;
        let mut default_unsigned = false;
        let digits = if self.peek() == Some(0x30) {
            self.read();
            let prefix_cursor = self.reader.cursor();
            if self.try_one_of(&[0x78, 0x58]).is_some() {
                radix = 16;
                default_unsigned = true;
                self.require_digit_run(radix)?
            } else if self.try_one_of(&[0x62, 0x42]).is_some() {
                match self.try_digit_run(2) {
                    Ok(Some(digits)) => {
                        radix = 2;
                        default_unsigned = true;
                        digits
                    }
                    Ok(None) | Err(_) => {
                        self.reader.set_cursor(prefix_cursor);
                        "0".to_owned()
                    }
                }
            } else {
                self.reader.set_cursor(prefix_cursor);
                if self
                    .peek()
                    .is_some_and(|unit| is_digit_for_radix(unit, 10) || unit == 0x5f)
                {
                    return Err("leading zeroes are not allowed in integer literals".to_owned());
                }
                "0".to_owned()
            }
        } else {
            self.require_digit_run(10)?
        };
        let (explicit_signed, explicit_kind) = self.parse_integer_suffix();
        let kind = explicit_kind.unwrap_or(default_kind);
        if !allowed_kinds.contains(&kind) {
            return Err("invalid typed-array element type".to_owned());
        }
        let signed = explicit_signed.unwrap_or(!default_unsigned);
        if !signed && sign < 0 {
            return Err("unsigned integers cannot be negative".to_owned());
        }
        let digits = remove_underscores(&digits);
        parse_integer_value(sign, radix, &digits, signed, kind)
    }

    fn parse_integer_suffix(&mut self) -> (Option<bool>, Option<IntegerKind>) {
        let start = self.reader.cursor();
        self.reader.skip_whitespace();
        let Some(first) = self.peek() else {
            self.reader.set_cursor(start);
            return (None, None);
        };
        if matches!(first, 0x75 | 0x55 | 0x73 | 0x53) {
            self.read();
            self.reader.skip_whitespace();
            if let Some(kind) = self.peek().and_then(IntegerKind::from_suffix) {
                self.read();
                return (Some(matches!(first, 0x73 | 0x53)), Some(kind));
            }
            self.reader.set_cursor(start);
            self.reader.skip_whitespace();
        }
        if let Some(kind) = self.peek().and_then(IntegerKind::from_suffix) {
            self.read();
            (None, Some(kind))
        } else {
            self.reader.set_cursor(start);
            (None, None)
        }
    }

    fn parse_unquoted_or_builtin(&mut self) -> Result<Tag, String> {
        let value = self.parse_unquoted()?;
        if value.eq_ascii_ignore_case(b"true") {
            return Ok(Tag::Byte(1));
        }
        if value.eq_ascii_ignore_case(b"false") {
            return Ok(Tag::Byte(0));
        }
        if self.try_character(0x28) {
            let mut arguments = Vec::new();
            if !self.try_character(0x29) {
                loop {
                    arguments.push(self.parse_tag()?);
                    if self.try_character(0x29) {
                        break;
                    }
                    self.expect(0x2c)?;
                    if self.try_character(0x29) {
                        break;
                    }
                }
            }
            return self.apply_builtin(value, arguments);
        }
        Ok(Tag::String(value))
    }

    fn apply_builtin(&self, name: JavaString, arguments: Vec<Tag>) -> Result<Tag, String> {
        if name.eq_ascii(b"bool") && arguments.len() == 1 {
            let value = arguments[0]
                .double_value()
                .ok_or_else(|| "bool expects a numeric or boolean argument".to_owned())?;
            return Ok(Tag::Byte(i8::from(value != 0.0)));
        }
        if name.eq_ascii(b"uuid") && arguments.len() == 1 {
            let value = arguments[0]
                .as_string()
                .ok_or_else(|| "uuid expects a string argument".to_owned())?;
            return parse_uuid(value).map(Tag::IntArray);
        }
        Err(format!(
            "unknown SNBT operation {}/{}",
            name.to_string_lossy(),
            arguments.len()
        ))
    }

    fn parse_quoted(&mut self) -> Result<JavaString, String> {
        self.reader.skip_whitespace();
        let quote = self.read();
        let mut result = Vec::new();
        while let Some(unit) = self.peek() {
            self.read();
            if unit == quote {
                return Ok(JavaString::from_units(result));
            }
            if unit != 0x5c {
                result.push(unit);
                continue;
            }
            let escape = self
                .peek()
                .ok_or_else(|| "unterminated escape sequence".to_owned())?;
            self.read();
            match escape {
                0x62 => result.push(0x0008),
                0x73 => result.push(0x0020),
                0x74 => result.push(0x0009),
                0x6e => result.push(0x000a),
                0x66 => result.push(0x000c),
                0x72 => result.push(0x000d),
                0x5c | 0x27 | 0x22 => result.push(escape),
                0x78 => result.extend(code_point_to_utf16(self.read_hex(2)?)?),
                0x75 => result.extend(code_point_to_utf16(self.read_hex(4)?)?),
                0x55 => result.extend(code_point_to_utf16(self.read_hex(8)?)?),
                0x4e => {
                    self.expect_raw(0x7b)?;
                    let name_start = self.reader.cursor();
                    while self.peek().is_some_and(is_unicode_name_unit) {
                        self.read();
                    }
                    let name = self.reader.substring(name_start, self.reader.cursor());
                    self.expect_raw(0x7d)?;
                    let code_point = unicode_name(&name)
                        .ok_or_else(|| format!("unknown Unicode character name {name:?}"))?;
                    result.extend(code_point_to_utf16(code_point)?);
                }
                _ => return Err("invalid quoted-string escape".to_owned()),
            }
        }
        Err("unterminated quoted string".to_owned())
    }

    fn parse_unquoted(&mut self) -> Result<JavaString, String> {
        self.reader.skip_whitespace();
        let start = self.reader.cursor();
        while self.peek().is_some_and(is_unquoted_snbt_unit) {
            self.read();
        }
        if start == self.reader.cursor() {
            return Err("expected an unquoted string".to_owned());
        }
        Ok(JavaString::from_units(
            self.reader.substring_utf16(start, self.reader.cursor()),
        ))
    }

    fn parse_sign(&mut self) -> i8 {
        match self.try_one_of(&[0x2b, 0x2d]) {
            Some(0x2d) => -1,
            _ => 1,
        }
    }

    fn try_digit_run(&mut self, radix: u32) -> Result<Option<String>, String> {
        let start_before_whitespace = self.reader.cursor();
        self.reader.skip_whitespace();
        let start = self.reader.cursor();
        while self
            .peek()
            .is_some_and(|unit| is_digit_for_radix(unit, radix) || unit == 0x5f)
        {
            self.read();
        }
        if start == self.reader.cursor() {
            self.reader.set_cursor(start_before_whitespace);
            return Ok(None);
        }
        let value = self.reader.substring(start, self.reader.cursor());
        if value.starts_with('_') || value.ends_with('_') {
            return Err("underscores cannot start or end a number".to_owned());
        }
        Ok(Some(value))
    }

    fn require_digit_run(&mut self, radix: u32) -> Result<String, String> {
        self.try_digit_run(radix)?
            .ok_or_else(|| "expected digits".to_owned())
    }

    fn read_hex(&mut self, length: usize) -> Result<u32, String> {
        if !self.reader.can_read_n(length) {
            return Err(format!("expected {length} hexadecimal digits"));
        }
        let start = self.reader.cursor();
        for _ in 0..length {
            if !self.peek().is_some_and(|unit| is_digit_for_radix(unit, 16)) {
                return Err(format!("expected {length} hexadecimal digits"));
            }
            self.read();
        }
        u32::from_str_radix(&self.reader.substring(start, self.reader.cursor()), 16)
            .map_err(|error| error.to_string())
    }

    fn consumed_character_since(&self, start: usize, character: u16) -> bool {
        self.reader.utf16()[start..self.reader.cursor()].contains(&character)
    }

    fn expect(&mut self, expected: u16) -> Result<(), String> {
        self.reader.skip_whitespace();
        self.expect_raw(expected)
    }

    fn expect_raw(&mut self, expected: u16) -> Result<(), String> {
        if self.peek() == Some(expected) {
            self.read();
            Ok(())
        } else {
            Err(format!(
                "expected {:?}",
                char::from_u32(u32::from(expected))
            ))
        }
    }

    fn try_character(&mut self, expected: u16) -> bool {
        let start = self.reader.cursor();
        self.reader.skip_whitespace();
        if self.peek() == Some(expected) {
            self.read();
            true
        } else {
            self.reader.set_cursor(start);
            false
        }
    }

    fn try_one_of(&mut self, expected: &[u16]) -> Option<u16> {
        let start = self.reader.cursor();
        self.reader.skip_whitespace();
        match self.peek() {
            Some(unit) if expected.contains(&unit) => {
                self.read();
                Some(unit)
            }
            _ => {
                self.reader.set_cursor(start);
                None
            }
        }
    }

    fn peek(&self) -> Option<u16> {
        self.reader.can_read().then(|| self.reader.peek())
    }

    fn read(&mut self) -> u16 {
        self.reader.read()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum IntegerKind {
    Byte,
    Short,
    Int,
    Long,
}

impl IntegerKind {
    fn from_suffix(value: u16) -> Option<Self> {
        match value {
            0x62 | 0x42 => Some(Self::Byte),
            0x73 | 0x53 => Some(Self::Short),
            0x69 | 0x49 => Some(Self::Int),
            0x6c | 0x4c => Some(Self::Long),
            _ => None,
        }
    }
}

fn parse_integer_value(
    sign: i8,
    radix: u32,
    digits: &str,
    signed: bool,
    kind: IntegerKind,
) -> Result<Tag, String> {
    if signed {
        let text = if sign < 0 {
            format!("-{digits}")
        } else {
            digits.to_owned()
        };
        return match kind {
            IntegerKind::Byte => i8::from_str_radix(&text, radix).map(Tag::Byte),
            IntegerKind::Short => i16::from_str_radix(&text, radix).map(Tag::Short),
            IntegerKind::Int => i32::from_str_radix(&text, radix).map(Tag::Int),
            IntegerKind::Long => i64::from_str_radix(&text, radix).map(Tag::Long),
        }
        .map_err(|error| format!("invalid integer: {error}"));
    }

    match kind {
        IntegerKind::Byte => u8::from_str_radix(digits, radix).map(|value| Tag::Byte(value as i8)),
        IntegerKind::Short => {
            u16::from_str_radix(digits, radix).map(|value| Tag::Short(value as i16))
        }
        IntegerKind::Int => u32::from_str_radix(digits, radix).map(|value| Tag::Int(value as i32)),
        IntegerKind::Long => {
            u64::from_str_radix(digits, radix).map(|value| Tag::Long(value as i64))
        }
    }
    .map_err(|error| format!("invalid unsigned integer: {error}"))
}

fn remove_underscores(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character != '_')
        .collect()
}

fn is_digit_for_radix(unit: u16, radix: u32) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(|character| character.is_digit(radix))
}

fn is_unquoted_snbt_unit(unit: u16) -> bool {
    matches!(
        unit,
        0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x5f | 0x2d | 0x2e | 0x2b
    )
}

fn is_unicode_name_unit(unit: u16) -> bool {
    matches!(unit, 0x2d | 0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x20)
}

fn code_point_to_utf16(code_point: u32) -> Result<Vec<u16>, String> {
    if code_point > 0x10ffff {
        return Err(format!("invalid Unicode code point U+{code_point:08X}"));
    }
    if code_point <= 0xffff {
        return Ok(vec![code_point as u16]);
    }
    let value = code_point - 0x10000;
    Ok(vec![
        0xd800 | ((value >> 10) as u16),
        0xdc00 | ((value & 0x3ff) as u16),
    ])
}

fn unicode_name(name: &str) -> Option<u32> {
    let name = name.trim_matches(' ').to_ascii_uppercase();
    if let Some(code_point) = java_control_code_point(&name) {
        return Some(code_point);
    }

    if let Some(character) = unicode_names2::character(&name) {
        let code_point = u32::from(character);
        let is_java_table_name = unicode_names2::name(character)
            .is_some_and(|canonical| canonical.to_string() == name)
            && java_fallback_block_name(code_point).is_none();
        if is_java_table_name {
            return Some(code_point);
        }
    }

    let (_, hexadecimal) = name.rsplit_once(' ')?;
    let code_point = u32::from_str_radix(hexadecimal, 16).ok()?;
    let block = java_fallback_block_name(code_point)?;
    (name == format!("{block} {code_point:X}")).then_some(code_point)
}

fn java_control_code_point(name: &str) -> Option<u32> {
    const NAMES: &[(u32, &str)] = &[
        (0x0000, "NULL"),
        (0x0001, "START OF HEADING"),
        (0x0002, "START OF TEXT"),
        (0x0003, "END OF TEXT"),
        (0x0004, "END OF TRANSMISSION"),
        (0x0005, "ENQUIRY"),
        (0x0006, "ACKNOWLEDGE"),
        (0x0007, "BEL"),
        (0x0008, "BACKSPACE"),
        (0x0009, "CHARACTER TABULATION"),
        (0x000b, "LINE TABULATION"),
        (0x000e, "SHIFT OUT"),
        (0x000f, "SHIFT IN"),
        (0x0010, "DATA LINK ESCAPE"),
        (0x0011, "DEVICE CONTROL ONE"),
        (0x0012, "DEVICE CONTROL TWO"),
        (0x0013, "DEVICE CONTROL THREE"),
        (0x0014, "DEVICE CONTROL FOUR"),
        (0x0015, "NEGATIVE ACKNOWLEDGE"),
        (0x0016, "SYNCHRONOUS IDLE"),
        (0x0017, "END OF TRANSMISSION BLOCK"),
        (0x0018, "CANCEL"),
        (0x0019, "END OF MEDIUM"),
        (0x001a, "SUBSTITUTE"),
        (0x001b, "ESCAPE"),
        (0x001c, "INFORMATION SEPARATOR FOUR"),
        (0x001d, "INFORMATION SEPARATOR THREE"),
        (0x001e, "INFORMATION SEPARATOR TWO"),
        (0x001f, "INFORMATION SEPARATOR ONE"),
        (0x007f, "DELETE"),
        (0x0080, "PADDING CHARACTER"),
        (0x0081, "HIGH OCTET PRESET"),
        (0x0082, "BREAK PERMITTED HERE"),
        (0x0083, "NO BREAK HERE"),
        (0x0086, "START OF SELECTED AREA"),
        (0x0087, "END OF SELECTED AREA"),
        (0x0088, "CHARACTER TABULATION SET"),
        (0x0089, "CHARACTER TABULATION WITH JUSTIFICATION"),
        (0x008a, "LINE TABULATION SET"),
        (0x008b, "PARTIAL LINE FORWARD"),
        (0x008c, "PARTIAL LINE BACKWARD"),
        (0x008d, "REVERSE LINE FEED"),
        (0x008e, "SINGLE SHIFT TWO"),
        (0x008f, "SINGLE SHIFT THREE"),
        (0x0090, "DEVICE CONTROL STRING"),
        (0x0091, "PRIVATE USE ONE"),
        (0x0092, "PRIVATE USE TWO"),
        (0x0093, "SET TRANSMIT STATE"),
        (0x0094, "CANCEL CHARACTER"),
        (0x0095, "MESSAGE WAITING"),
        (0x0096, "START OF GUARDED AREA"),
        (0x0097, "END OF GUARDED AREA"),
        (0x0098, "START OF STRING"),
        (0x0099, "SINGLE GRAPHIC CHARACTER INTRODUCER"),
        (0x009a, "SINGLE CHARACTER INTRODUCER"),
        (0x009b, "CONTROL SEQUENCE INTRODUCER"),
        (0x009c, "STRING TERMINATOR"),
        (0x009d, "OPERATING SYSTEM COMMAND"),
        (0x009e, "PRIVACY MESSAGE"),
        (0x009f, "APPLICATION PROGRAM COMMAND"),
    ];

    NAMES
        .iter()
        .find_map(|&(code_point, candidate)| (candidate == name).then_some(code_point))
}

fn java_fallback_block_name(code_point: u32) -> Option<&'static str> {
    match code_point {
        0x0084 => Some("LATIN 1 SUPPLEMENT"),
        0x3400..=0x4dbf => Some("CJK UNIFIED IDEOGRAPHS EXTENSION A"),
        0x4e00..=0x9fff => Some("CJK UNIFIED IDEOGRAPHS"),
        0xac00..=0xd7a3 => Some("HANGUL SYLLABLES"),
        0xd800..=0xdb7f => Some("HIGH SURROGATES"),
        0xdb80..=0xdbff => Some("HIGH PRIVATE USE SURROGATES"),
        0xdc00..=0xdfff => Some("LOW SURROGATES"),
        0xe000..=0xf8ff => Some("PRIVATE USE AREA"),
        0x17000..=0x187f7 => Some("TANGUT"),
        0x18d00..=0x18d08 => Some("TANGUT SUPPLEMENT"),
        0x20000..=0x2a6df => Some("CJK UNIFIED IDEOGRAPHS EXTENSION B"),
        0x2a700..=0x2b739 => Some("CJK UNIFIED IDEOGRAPHS EXTENSION C"),
        0x2b740..=0x2b81d => Some("CJK UNIFIED IDEOGRAPHS EXTENSION D"),
        0x2b820..=0x2cea1 => Some("CJK UNIFIED IDEOGRAPHS EXTENSION E"),
        0x2ceb0..=0x2ebe0 => Some("CJK UNIFIED IDEOGRAPHS EXTENSION F"),
        0x2ebf0..=0x2ee5d => Some("CJK UNIFIED IDEOGRAPHS EXTENSION I"),
        0x30000..=0x3134a => Some("CJK UNIFIED IDEOGRAPHS EXTENSION G"),
        0x31350..=0x323af => Some("CJK UNIFIED IDEOGRAPHS EXTENSION H"),
        0xf0000..=0xffffd => Some("SUPPLEMENTARY PRIVATE USE AREA A"),
        0x100000..=0x10fffd => Some("SUPPLEMENTARY PRIVATE USE AREA B"),
        _ => None,
    }
}

fn parse_uuid(value: &JavaString) -> Result<Vec<i32>, String> {
    let value = String::from_utf16(value.units()).map_err(|_| "invalid UUID string".to_owned())?;
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 5
        || parts.iter().zip([8, 4, 4, 4, 12]).any(|(part, width)| {
            part.len() != width || !part.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err("invalid UUID string".to_owned());
    }
    let parts = parts
        .into_iter()
        .map(|part| u64::from_str_radix(part, 16).expect("validated UUID group"))
        .collect::<Vec<_>>();
    let most = (parts[0] << 32) | (parts[1] << 16) | parts[2];
    let least = (parts[3] << 48) | parts[4];
    Ok(vec![
        (most >> 32) as i32,
        most as i32,
        (least >> 32) as i32,
        least as i32,
    ])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NbtPath {
    nodes: Vec<PathNode>,
}

impl NbtPath {
    pub(crate) fn parse(reader: &mut StringReader) -> Result<Self, String> {
        let mut nodes = Vec::new();
        let mut first = true;
        while reader.can_read() && reader.peek() != 0x20 {
            let node = match reader.peek() {
                0x22 | 0x27 => {
                    let name = read_path_string(reader)?;
                    parse_named_path_node(reader, name)?
                }
                0x5b => {
                    reader.skip();
                    match reader.can_read().then(|| reader.peek()) {
                        Some(0x7b) => {
                            let pattern = parse_compound(reader)?;
                            expect_path_character(reader, 0x5d)?;
                            PathNode::MatchElement(pattern)
                        }
                        Some(0x5d) => {
                            reader.skip();
                            PathNode::AllElements
                        }
                        _ => {
                            let index = reader.read_int().map_err(|error| error.to_string())?;
                            expect_path_character(reader, 0x5d)?;
                            PathNode::Indexed(index)
                        }
                    }
                }
                0x7b => {
                    if !first {
                        return Err("a root object match must be the first path node".to_owned());
                    }
                    PathNode::MatchRoot(parse_compound(reader)?)
                }
                _ => {
                    let name = read_unquoted_path_name(reader)?;
                    parse_named_path_node(reader, name)?
                }
            };
            nodes.push(node);
            first = false;
            if reader.can_read() {
                match reader.peek() {
                    0x20 | 0x5b | 0x7b => {}
                    _ => expect_path_character(reader, 0x2e)?,
                }
            }
        }
        if nodes.is_empty() {
            Err("expected an NBT path".to_owned())
        } else {
            Ok(Self { nodes })
        }
    }

    pub(crate) fn parse_codec(reader: &mut StringReader) -> Result<Self, String> {
        if !reader.can_read() || reader.peek() == 0x20 {
            Ok(Self { nodes: Vec::new() })
        } else {
            Self::parse(reader)
        }
    }

    pub(crate) fn get(&self, root: &CompoundTag) -> Result<Vec<Tag>, String> {
        let mut current = vec![Tag::Compound(root.clone())];
        for node in &self.nodes {
            let mut next = Vec::new();
            for tag in &current {
                node.collect(tag, &mut next);
            }
            if next.is_empty() {
                return Err("nothing found at NBT path".to_owned());
            }
            current = next;
        }
        Ok(current)
    }

    pub(crate) fn count_matching(&self, root: &CompoundTag) -> usize {
        self.get(root).map_or(0, |tags| tags.len())
    }

    pub(crate) fn set(&self, root: &mut CompoundTag, value: Tag) -> Result<i32, String> {
        if value.is_too_deep(self.nodes.len()) {
            return Err("NBT data is too deep".to_owned());
        }
        let (parents, last) = self.get_or_create_parents(root)?;
        let mut changed = 0_i32;
        let mut root_tag = Tag::Compound(std::mem::take(root));
        for location in parents {
            let parent = tag_at_mut(&mut root_tag, &location)
                .expect("resolved mutable NBT locations remain valid during set");
            changed = changed.wrapping_add(last.set(parent, value.clone()));
        }
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        Ok(changed)
    }

    pub(crate) fn insert(
        &self,
        index: i32,
        root: &mut CompoundTag,
        values: &[Tag],
    ) -> Result<i32, String> {
        for value in values {
            if value.is_too_deep(self.nodes.len()) {
                return Err("NBT data is too deep".to_owned());
            }
        }
        let (targets, _) = self.get_or_create_targets(root, Tag::List(Vec::new()))?;
        let mut modified = 0_i32;
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            for location in targets {
                let target = tag_at_mut(&mut root_tag, &location)
                    .expect("resolved mutable NBT locations remain valid during insertion");
                let size = target
                    .collection_len()
                    .ok_or_else(|| "expected a list at target path".to_owned())?;
                let size = i32::try_from(size).map_err(|_| "NBT list is too large".to_owned())?;
                let mut actual_index = if index < 0 {
                    size.wrapping_add(index).wrapping_add(1)
                } else {
                    index
                };
                let mut changed = false;
                for value in values {
                    let insert_index = usize::try_from(actual_index)
                        .map_err(|_| format!("invalid list index {actual_index}"))?;
                    if target.collection_insert(insert_index, value.clone())? {
                        actual_index = actual_index.wrapping_add(1);
                        changed = true;
                    }
                }
                if changed {
                    modified = modified.wrapping_add(1);
                }
            }
            Ok(modified)
        })();
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        result
    }

    pub(crate) fn merge(&self, root: &mut CompoundTag, sources: &[Tag]) -> Result<i32, String> {
        let mut combined = CompoundTag::new();
        for source in sources {
            if source.is_too_deep(0) {
                return Err("NBT data is too deep".to_owned());
            }
            let Tag::Compound(source) = source else {
                return Err("expected a compound tag".to_owned());
            };
            combined.merge(source);
        }

        let (targets, _) = self.get_or_create_targets(root, Tag::Compound(CompoundTag::new()))?;
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            let mut changed = 0_i32;
            for location in targets {
                let target = tag_at_mut(&mut root_tag, &location)
                    .expect("resolved mutable NBT locations remain valid during merge");
                let Tag::Compound(target) = target else {
                    return Err("expected a compound tag at target path".to_owned());
                };
                let previous = target.clone();
                target.merge(&combined);
                if *target != previous {
                    changed = changed.wrapping_add(1);
                }
            }
            Ok(changed)
        })();
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        result
    }

    pub(crate) fn remove(&self, root: &mut CompoundTag) -> i32 {
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let mut current = vec![Location::default()];
        for node in &self.nodes[..self.nodes.len() - 1] {
            let mut next = Vec::new();
            for location in &current {
                collect_locations(&root_tag, location, node, &mut next);
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }
        let last = self.nodes.last().expect("NBT paths have at least one node");
        let mut removed = 0_i32;
        for location in current {
            let Some(parent) = tag_at_mut(&mut root_tag, &location) else {
                continue;
            };
            removed = removed.wrapping_add(last.remove(parent));
        }
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot remove its root compound"),
        };
        removed
    }

    fn get_or_create_parents(
        &self,
        root: &mut CompoundTag,
    ) -> Result<(Vec<Location>, &PathNode), String> {
        let last = self.nodes.last().expect("NBT paths have at least one node");
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            let mut current = vec![Location::default()];
            for (index, node) in self.nodes[..self.nodes.len() - 1].iter().enumerate() {
                let preferred = self.nodes[index + 1].preferred_parent();
                let mut next = Vec::new();
                for location in &current {
                    collect_or_create_locations(
                        &mut root_tag,
                        location,
                        node,
                        &preferred,
                        &mut next,
                    );
                }
                if next.is_empty() {
                    return Err("nothing found at NBT path".to_owned());
                }
                current = next;
            }
            Ok(current)
        })();
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        result.map(|locations| (locations, last))
    }

    fn get_or_create_targets(
        &self,
        root: &mut CompoundTag,
        default: Tag,
    ) -> Result<(Vec<Location>, &PathNode), String> {
        let (parents, last) = self.get_or_create_parents(root)?;
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let mut targets = Vec::new();
        for parent in &parents {
            collect_or_create_locations(&mut root_tag, parent, last, &default, &mut targets);
        }
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        if targets.is_empty() {
            Err("nothing found at NBT path".to_owned())
        } else {
            Ok((targets, last))
        }
    }
}

pub(crate) fn parse_path(reader: &mut StringReader) -> Result<NbtPath, String> {
    NbtPath::parse(reader)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PathNode {
    AllElements,
    CompoundChild(JavaString),
    Indexed(i32),
    MatchElement(CompoundTag),
    MatchObject(JavaString, CompoundTag),
    MatchRoot(CompoundTag),
}

impl PathNode {
    fn preferred_parent(&self) -> Tag {
        match self {
            Self::AllElements | Self::Indexed(_) | Self::MatchElement(_) => Tag::List(Vec::new()),
            Self::CompoundChild(_) | Self::MatchObject(_, _) | Self::MatchRoot(_) => {
                Tag::Compound(CompoundTag::new())
            }
        }
    }

    fn collect(&self, parent: &Tag, output: &mut Vec<Tag>) {
        match self {
            Self::AllElements => {
                if let Some(length) = parent.collection_len() {
                    output.extend((0..length).filter_map(|index| parent.collection_element(index)));
                }
            }
            Self::CompoundChild(name) => {
                if let Some(tag) = parent.as_compound().and_then(|parent| parent.get(name)) {
                    output.push(tag.clone());
                }
            }
            Self::Indexed(index) => {
                if let Some(index) = actual_index(*index, parent.collection_len())
                    && let Some(tag) = parent.collection_element(index)
                {
                    output.push(tag);
                }
            }
            Self::MatchElement(pattern) => {
                if let Tag::List(values) = parent {
                    output.extend(
                        values
                            .iter()
                            .filter(|value| partial_matches(&Tag::Compound(pattern.clone()), value))
                            .cloned(),
                    );
                }
            }
            Self::MatchObject(name, pattern) => {
                if let Some(value) = parent.as_compound().and_then(|parent| parent.get(name))
                    && partial_matches(&Tag::Compound(pattern.clone()), value)
                {
                    output.push(value.clone());
                }
            }
            Self::MatchRoot(pattern) => {
                if matches!(parent, Tag::Compound(_))
                    && partial_matches(&Tag::Compound(pattern.clone()), parent)
                {
                    output.push(parent.clone());
                }
            }
        }
    }

    fn set(&self, parent: &mut Tag, value: Tag) -> i32 {
        match self {
            Self::AllElements => {
                let Some(size) = parent.collection_len() else {
                    return 0;
                };
                if size == 0 {
                    let _ = parent.collection_insert(0, value);
                    return 1;
                }
                let changed = (0..size)
                    .filter(|&index| parent.collection_element(index).as_ref() != Some(&value))
                    .count();
                if changed == 0 {
                    return 0;
                }
                parent.collection_clear();
                if !parent.collection_insert(0, value.clone()).unwrap_or(false) {
                    return 0;
                }
                for index in 1..size {
                    let _ = parent.collection_insert(index, value.clone());
                }
                i32::try_from(changed).unwrap_or(i32::MAX)
            }
            Self::CompoundChild(name) => parent.as_compound_mut().map_or(0, |parent| {
                let changed = parent.get(name) != Some(&value);
                parent.insert(name.clone(), value);
                i32::from(changed)
            }),
            Self::Indexed(index) => {
                let Some(index) = actual_index(*index, parent.collection_len()) else {
                    return 0;
                };
                if parent.collection_element(index).as_ref() == Some(&value) {
                    0
                } else {
                    i32::from(parent.collection_set(index, value))
                }
            }
            Self::MatchElement(pattern) => {
                let Tag::List(values) = parent else {
                    return 0;
                };
                if values.is_empty() {
                    values.push(value);
                    return 1;
                }
                let mut changed = 0_i32;
                for current in values {
                    if partial_matches(&Tag::Compound(pattern.clone()), current)
                        && *current != value
                    {
                        *current = value.clone();
                        changed = changed.wrapping_add(1);
                    }
                }
                changed
            }
            Self::MatchObject(name, pattern) => parent.as_compound_mut().map_or(0, |parent| {
                let pattern = Tag::Compound(pattern.clone());
                if parent
                    .get(name)
                    .is_some_and(|current| partial_matches(&pattern, current) && *current != value)
                {
                    parent.insert(name.clone(), value);
                    1
                } else {
                    0
                }
            }),
            Self::MatchRoot(_) => 0,
        }
    }

    fn remove(&self, parent: &mut Tag) -> i32 {
        match self {
            Self::AllElements => {
                let Some(size) = parent.collection_len() else {
                    return 0;
                };
                parent.collection_clear();
                i32::try_from(size).unwrap_or(i32::MAX)
            }
            Self::CompoundChild(name) => i32::from(
                parent
                    .as_compound_mut()
                    .is_some_and(|parent| parent.remove(name).is_some()),
            ),
            Self::Indexed(index) => {
                let Some(index) = actual_index(*index, parent.collection_len()) else {
                    return 0;
                };
                i32::from(parent.collection_remove(index))
            }
            Self::MatchElement(pattern) => {
                let Tag::List(values) = parent else {
                    return 0;
                };
                let old = values.len();
                let pattern = Tag::Compound(pattern.clone());
                values.retain(|value| !partial_matches(&pattern, value));
                i32::try_from(old - values.len()).unwrap_or(i32::MAX)
            }
            Self::MatchObject(name, pattern) => {
                i32::from(parent.as_compound_mut().is_some_and(|parent| {
                    let pattern = Tag::Compound(pattern.clone());
                    if parent
                        .get(name)
                        .is_some_and(|current| partial_matches(&pattern, current))
                    {
                        parent.remove(name);
                        true
                    } else {
                        false
                    }
                }))
            }
            Self::MatchRoot(_) => 0,
        }
    }
}

fn parse_named_path_node(reader: &mut StringReader, name: JavaString) -> Result<PathNode, String> {
    if name.len() == 0 {
        return Err("empty NBT path name".to_owned());
    }
    if reader.can_read() && reader.peek() == 0x7b {
        Ok(PathNode::MatchObject(name, parse_compound(reader)?))
    } else {
        Ok(PathNode::CompoundChild(name))
    }
}

fn read_path_string(reader: &mut StringReader) -> Result<JavaString, String> {
    let quote = reader.read();
    let mut result = Vec::new();
    let mut escaped = false;
    while reader.can_read() {
        let unit = reader.read();
        if escaped {
            if unit == quote || unit == 0x5c {
                result.push(unit);
                escaped = false;
            } else {
                return Err("invalid escape in quoted NBT path name".to_owned());
            }
        } else if unit == 0x5c {
            escaped = true;
        } else if unit == quote {
            return Ok(JavaString::from_units(result));
        } else {
            result.push(unit);
        }
    }
    Err("unterminated quoted NBT path name".to_owned())
}

fn read_unquoted_path_name(reader: &mut StringReader) -> Result<JavaString, String> {
    let start = reader.cursor();
    while reader.can_read() && is_unquoted_path_unit(reader.peek()) {
        reader.skip();
    }
    if start == reader.cursor() {
        Err("invalid NBT path node".to_owned())
    } else {
        Ok(JavaString::from_units(
            reader.substring_utf16(start, reader.cursor()),
        ))
    }
}

fn is_unquoted_path_unit(unit: u16) -> bool {
    !matches!(unit, 0x20 | 0x22 | 0x27 | 0x5b | 0x5d | 0x2e | 0x7b | 0x7d)
}

fn expect_path_character(reader: &mut StringReader, expected: u16) -> Result<(), String> {
    if reader.can_read() && reader.peek() == expected {
        reader.skip();
        Ok(())
    } else {
        Err(format!("expected NBT path delimiter {expected:?}"))
    }
}

fn partial_matches(expected: &Tag, actual: &Tag) -> bool {
    match (expected, actual) {
        (Tag::Compound(expected), Tag::Compound(actual)) => {
            actual.len() >= expected.len()
                && expected.0.iter().all(|(name, expected)| {
                    actual
                        .get(name)
                        .is_some_and(|actual| partial_matches(expected, actual))
                })
        }
        (Tag::List(expected), Tag::List(actual)) => {
            if expected.is_empty() {
                actual.is_empty()
            } else {
                actual.len() >= expected.len()
                    && expected.iter().all(|expected| {
                        actual
                            .iter()
                            .any(|actual| partial_matches(expected, actual))
                    })
            }
        }
        _ => expected == actual,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Location(Vec<LocationStep>);

impl Location {
    fn child(&self, step: LocationStep) -> Self {
        let mut result = self.clone();
        result.0.push(step);
        result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LocationStep {
    Key(JavaString),
    Index(usize),
}

fn tag_at<'a>(root: &'a Tag, location: &Location) -> Option<&'a Tag> {
    let mut current = root;
    for step in &location.0 {
        current = match step {
            LocationStep::Key(key) => current.as_compound()?.get(key)?,
            LocationStep::Index(index) => match current {
                Tag::List(values) => values.get(*index)?,
                _ => return None,
            },
        };
    }
    Some(current)
}

fn tag_at_mut<'a>(root: &'a mut Tag, location: &Location) -> Option<&'a mut Tag> {
    let mut current = root;
    for step in &location.0 {
        current = match step {
            LocationStep::Key(key) => current.as_compound_mut()?.get_mut(key)?,
            LocationStep::Index(index) => match current {
                Tag::List(values) => values.get_mut(*index)?,
                _ => return None,
            },
        };
    }
    Some(current)
}

fn collect_locations(
    root: &Tag,
    parent_location: &Location,
    node: &PathNode,
    output: &mut Vec<Location>,
) {
    let Some(parent) = tag_at(root, parent_location) else {
        return;
    };
    match node {
        PathNode::AllElements => {
            if let Tag::List(values) = parent {
                output.extend(
                    (0..values.len())
                        .map(|index| parent_location.child(LocationStep::Index(index))),
                );
            }
        }
        PathNode::CompoundChild(name) => {
            if parent
                .as_compound()
                .is_some_and(|parent| parent.contains_key(name))
            {
                output.push(parent_location.child(LocationStep::Key(name.clone())));
            }
        }
        PathNode::Indexed(index) => {
            if let Tag::List(values) = parent
                && let Some(index) = actual_index(*index, Some(values.len()))
            {
                output.push(parent_location.child(LocationStep::Index(index)));
            }
        }
        PathNode::MatchElement(pattern) => {
            if let Tag::List(values) = parent {
                let pattern = Tag::Compound(pattern.clone());
                output.extend(
                    values
                        .iter()
                        .enumerate()
                        .filter(|(_, value)| partial_matches(&pattern, value))
                        .map(|(index, _)| parent_location.child(LocationStep::Index(index))),
                );
            }
        }
        PathNode::MatchObject(name, pattern) => {
            if parent
                .as_compound()
                .and_then(|parent| parent.get(name))
                .is_some_and(|value| partial_matches(&Tag::Compound(pattern.clone()), value))
            {
                output.push(parent_location.child(LocationStep::Key(name.clone())));
            }
        }
        PathNode::MatchRoot(pattern) => {
            if partial_matches(&Tag::Compound(pattern.clone()), parent) {
                output.push(parent_location.clone());
            }
        }
    }
}

fn collect_or_create_locations(
    root: &mut Tag,
    parent_location: &Location,
    node: &PathNode,
    preferred: &Tag,
    output: &mut Vec<Location>,
) {
    let Some(parent) = tag_at_mut(root, parent_location) else {
        return;
    };
    match node {
        PathNode::AllElements => {
            if let Tag::List(values) = parent {
                if values.is_empty() {
                    values.push(preferred.clone());
                }
                output.extend(
                    (0..values.len())
                        .map(|index| parent_location.child(LocationStep::Index(index))),
                );
            }
        }
        PathNode::CompoundChild(name) => {
            if let Some(parent) = parent.as_compound_mut() {
                if !parent.contains_key(name) {
                    parent.insert(name.clone(), preferred.clone());
                }
                output.push(parent_location.child(LocationStep::Key(name.clone())));
            }
        }
        PathNode::Indexed(index) => {
            if let Tag::List(values) = parent
                && let Some(index) = actual_index(*index, Some(values.len()))
            {
                output.push(parent_location.child(LocationStep::Index(index)));
            }
        }
        PathNode::MatchElement(pattern) => {
            if let Tag::List(values) = parent {
                let pattern_tag = Tag::Compound(pattern.clone());
                let matches = values
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| {
                        partial_matches(&pattern_tag, value).then_some(index)
                    })
                    .collect::<Vec<_>>();
                if matches.is_empty() {
                    values.push(pattern_tag);
                    output.push(parent_location.child(LocationStep::Index(values.len() - 1)));
                } else {
                    output.extend(
                        matches
                            .into_iter()
                            .map(|index| parent_location.child(LocationStep::Index(index))),
                    );
                }
            }
        }
        PathNode::MatchObject(name, pattern) => {
            if let Some(parent) = parent.as_compound_mut() {
                if !parent.contains_key(name) {
                    parent.insert(name.clone(), Tag::Compound(pattern.clone()));
                    output.push(parent_location.child(LocationStep::Key(name.clone())));
                } else if parent
                    .get(name)
                    .is_some_and(|value| partial_matches(&Tag::Compound(pattern.clone()), value))
                {
                    output.push(parent_location.child(LocationStep::Key(name.clone())));
                }
            }
        }
        PathNode::MatchRoot(pattern) => {
            if partial_matches(&Tag::Compound(pattern.clone()), parent) {
                output.push(parent_location.clone());
            }
        }
    }
}

fn actual_index(index: i32, length: Option<usize>) -> Option<usize> {
    let length = i32::try_from(length?).ok()?;
    let index = if index < 0 {
        length.wrapping_add(index)
    } else {
        index
    };
    (0..length).contains(&index).then_some(index as usize)
}

#[derive(Debug, Default)]
pub(crate) struct CommandStorage {
    values: HashMap<Identifier, CompoundTag>,
}

impl CommandStorage {
    pub(crate) fn get_ref(&self, id: &Identifier) -> Option<&CompoundTag> {
        self.values.get(id)
    }

    pub(crate) fn get(&self, id: &Identifier) -> CompoundTag {
        self.values.get(id).cloned().unwrap_or_default()
    }

    pub(crate) fn set(&mut self, id: Identifier, value: CompoundTag) {
        if value.is_empty() {
            self.values.remove(&id);
        } else {
            self.values.insert(id, value);
        }
    }

    pub(crate) fn edit<R, E>(
        &mut self,
        id: &Identifier,
        operation: impl FnOnce(&mut CompoundTag) -> Result<R, E>,
    ) -> Result<R, E> {
        match self.values.entry(id.clone()) {
            Entry::Occupied(mut entry) => {
                let result = operation(entry.get_mut());
                if result.is_ok() && entry.get().is_empty() {
                    entry.remove();
                }
                result
            }
            Entry::Vacant(entry) => {
                let mut value = CompoundTag::new();
                let result = operation(&mut value);
                if result.is_ok() && !value.is_empty() {
                    entry.insert(value);
                }
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_java_string(actual: JavaString, expected: &str) {
        assert_eq!(actual.units(), expected.encode_utf16().collect::<Vec<_>>());
    }

    #[test]
    fn macro_stringify_uses_unadorned_primitive_values() {
        assert_java_string(Tag::Byte(-12).macro_stringify(), "-12");
        assert_java_string(Tag::Short(34).macro_stringify(), "34");
        assert_java_string(Tag::Int(-56).macro_stringify(), "-56");
        assert_java_string(Tag::Long(78).macro_stringify(), "78");
        assert_java_string(Tag::float(0.1).macro_stringify(), ".100000001490116");
        assert_java_string(Tag::double(0.1).macro_stringify(), ".1");

        let string = JavaString::from_units(vec![0x61, 0xd800, 0x62]);
        assert_eq!(
            Tag::String(string.clone()).macro_stringify().units(),
            string.units()
        );
    }

    #[test]
    fn macro_decimal_format_rounds_the_exact_value_to_fifteen_fraction_digits() {
        let cases: [(f64, &str); 15] = [
            (0.0, "0"),
            (-0.0, "-0"),
            (10.0, "10"),
            (1.234_567_890_123_456_7, "1.234567890123457"),
            (1_000_000_000_000_000.1, "1000000000000000.125"),
            (999_999_999_999_999.9, "999999999999999.875"),
            (5.0e-16, ".000000000000001"),
            (6.0e-16, ".000000000000001"),
            (1.5e-15, ".000000000000001"),
            (2.5e-15, ".000000000000002"),
            (3.5e-15, ".000000000000004"),
            (0.000_015_258_789_062_5, ".000015258789062"),
            (0.000_045_776_367_187_5, ".000045776367188"),
            (f64::from_bits(0x43ac_1109_368c_74bb), "1011203918777703808"),
            (1.0e20, "100000000000000000000"),
        ];
        for (value, expected) in cases {
            assert_java_string(Tag::Double(value.to_bits()).macro_stringify(), expected);
        }
        assert_java_string(
            Tag::Float(f32::MAX.to_bits()).macro_stringify(),
            "340282346638528859811704183484516925440",
        );
        assert_java_string(Tag::Double(f64::INFINITY.to_bits()).macro_stringify(), "∞");
        assert_java_string(
            Tag::Double(f64::NEG_INFINITY.to_bits()).macro_stringify(),
            "-∞",
        );
        assert_java_string(Tag::Double(f64::NAN.to_bits()).macro_stringify(), "NaN");
    }

    #[test]
    fn primitive_text_reuses_compact_numeric_rendering_and_raw_strings() {
        let cases = [
            (Tag::Byte(-1), "-1b"),
            (Tag::Short(2), "2s"),
            (Tag::Int(-3), "-3"),
            (Tag::Long(4), "4L"),
            (Tag::float(1.0), "1.0f"),
            (Tag::double(2.0), "2.0d"),
        ];
        for (value, expected) in cases {
            assert_java_string(value.primitive_text().expect("primitive"), expected);
        }

        let string = JavaString::from_units(vec![0x61, 0xd800, 0x62]);
        assert_eq!(
            Tag::String(string.clone())
                .primitive_text()
                .expect("string")
                .units(),
            string.units()
        );
        assert_eq!(Tag::List(Vec::new()).primitive_text(), None);
    }

    #[test]
    fn macro_stringify_falls_back_to_compact_sorted_snbt() {
        let mut compound = CompoundTag::new();
        compound.insert(JavaString::from("z"), Tag::LongArray(vec![-1, 2]));
        compound.insert(
            JavaString::from("alpha"),
            Tag::List(vec![
                Tag::Byte(1),
                Tag::Short(2),
                Tag::Int(3),
                Tag::Long(4),
                Tag::float(1.0),
                Tag::double(2.0),
            ]),
        );
        compound.insert(JavaString::from("true"), Tag::String(JavaString::from("x")));
        compound.insert(JavaString::from("bytes"), Tag::ByteArray(vec![-1, 2]));
        compound.insert(JavaString::from("ints"), Tag::IntArray(vec![-3, 4]));

        assert_java_string(
            Tag::Compound(compound).macro_stringify(),
            r#"{alpha:[1b,2s,3,4L,1.0f,2.0d],bytes:[B;-1B,2B],ints:[I;-3,4],"true":"x",z:[L;-1L,2L]}"#,
        );
    }

    #[test]
    fn compact_snbt_quoting_preserves_java_utf16() {
        let value = JavaString::from_units(vec![0x22, 0x27, 0x5c, 0x08, 0x00, 0xd800]);
        assert_eq!(
            compact_stringify(&Tag::String(value)).units(),
            &[
                0x27, 0x22, 0x5c, 0x27, 0x5c, 0x5c, 0x5c, 0x62, 0x5c, 0x78, 0x30, 0x30, 0xd800,
                0x27
            ]
        );
    }

    #[test]
    fn compact_snbt_key_rules_match_string_tag_visitor() {
        for key in ["alpha", ".path", "_name", "a+1", "a-1"] {
            assert!(is_unquoted_compound_key(&JavaString::from(key)), "{key}");
        }
        for key in ["", "+name", "-name", "1name", "true", "FALSE", "é"] {
            assert!(!is_unquoted_compound_key(&JavaString::from(key)), "{key}");
        }
    }
}
