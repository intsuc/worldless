use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{HashMap, hash_map::DefaultHasher},
    error::Error,
    fmt,
    hash::{BuildHasherDefault, Hash, Hasher},
    sync::Arc,
};

use worldless_brigadier::{
    StringReader,
    exceptions::{java_f32, java_f64},
};

use crate::resource::{Identifier, IdentifierPart};

const MAX_DEPTH: usize = 512;

#[derive(Clone)]
pub(crate) struct JavaString {
    units: Arc<[u16]>,
    hash: u64,
}

impl JavaString {
    pub(crate) fn from_units(units: Vec<u16>) -> Self {
        let mut hasher = DefaultHasher::new();
        units.hash(&mut hasher);
        Self {
            units: units.into(),
            hash: hasher.finish(),
        }
    }

    pub(crate) fn units(&self) -> &[u16] {
        &self.units
    }

    pub(crate) fn len(&self) -> usize {
        self.units.len()
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
        Ok(Self::from_units(
            self.units[start as usize..end as usize].to_vec(),
        ))
    }

    pub(crate) fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.units)
    }

    fn eq_ascii_ignore_case(&self, value: &[u8]) -> bool {
        self.units.len() == value.len()
            && self
                .units
                .iter()
                .zip(value)
                .all(|(&left, &right)| left <= 0x7f && (left as u8).eq_ignore_ascii_case(&right))
    }

    fn eq_ascii(&self, value: &[u8]) -> bool {
        self.units.len() == value.len()
            && self
                .units
                .iter()
                .zip(value)
                .all(|(&left, &right)| left == u16::from(right))
    }
}

impl Default for JavaString {
    fn default() -> Self {
        Self::from_units(Vec::new())
    }
}

impl PartialEq for JavaString {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.units, &other.units) || self.units == other.units
    }
}

impl Eq for JavaString {}

impl Hash for JavaString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl PartialOrd for JavaString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JavaString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.units.cmp(&other.units)
    }
}

impl From<&str> for JavaString {
    fn from(value: &str) -> Self {
        Self::from_units(value.encode_utf16().collect())
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

type CompoundMap = HashMap<JavaString, Tag, BuildHasherDefault<DefaultHasher>>;

#[derive(Debug)]
pub struct CompoundTag(CompoundMap);

/// An error produced while parsing a compound SNBT value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundTagParseError {
    reason: String,
}

impl CompoundTagParseError {
    /// Returns the parser diagnostic without the public error context.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for CompoundTagParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid compound tag: {}", self.reason)
    }
}

impl Error for CompoundTagParseError {}

impl CompoundTag {
    /// Parses one complete compound SNBT value.
    pub fn from_snbt(input: &str) -> Result<Self, CompoundTagParseError> {
        parse_compound_fully(input).map_err(|reason| CompoundTagParseError { reason })
    }

    /// Returns compact SNBT preserving Java UTF-16 code units exactly.
    pub fn to_compact_snbt_utf16(&self) -> Vec<u16> {
        Tag::Compound(self.clone())
            .compact_stringify()
            .units()
            .to_vec()
    }

    pub(crate) fn new() -> Self {
        Self(HashMap::default())
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

    pub(crate) fn is_too_deep(&self, depth: usize) -> bool {
        depth >= MAX_DEPTH || self.values().any(|child| child.is_too_deep(depth + 1))
    }

    pub(crate) fn pretty_stringify(&self) -> JavaString {
        let mut output = Vec::new();
        write_pretty_compound(self, 0, &mut output);
        JavaString::from_units(output)
    }
}

impl Default for CompoundTag {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CompoundTag {
    fn clone(&self) -> Self {
        let mut values = CompoundMap::with_capacity_and_hasher(
            self.0.len(),
            BuildHasherDefault::<DefaultHasher>::default(),
        );
        values.extend(
            self.0
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        Self(values)
    }
}

impl PartialEq for CompoundTag {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other) || self.0 == other.0
    }
}

impl Eq for CompoundTag {}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BinaryNbtParseError {
    reason: String,
}

impl BinaryNbtParseError {
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

pub(crate) fn parse_binary_compound(input: &[u8]) -> Result<CompoundTag, BinaryNbtParseError> {
    BinaryNbtReader::new(input).read_root_compound()
}

struct BinaryNbtReader<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> BinaryNbtReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn read_root_compound(mut self) -> Result<CompoundTag, BinaryNbtParseError> {
        let tag_type = self.read_u8("root tag type")?;
        if tag_type != 10 {
            return self.fail(format!(
                "root tag must be a compound, found {}",
                tag_type_name(tag_type)
            ));
        }
        let name_length = usize::from(self.read_u16("root name length")?);
        self.take(name_length, "root name")?;
        let Tag::Compound(root) = self.read_payload(tag_type, 0)? else {
            unreachable!("tag type 10 always produces a compound")
        };
        Ok(root)
    }

    fn read_payload(
        &mut self,
        tag_type: u8,
        container_depth: usize,
    ) -> Result<Tag, BinaryNbtParseError> {
        match tag_type {
            1 => Ok(Tag::Byte(self.read_u8("byte tag")? as i8)),
            2 => Ok(Tag::Short(self.read_i16("short tag")?)),
            3 => Ok(Tag::Int(self.read_i32("int tag")?)),
            4 => Ok(Tag::Long(self.read_i64("long tag")?)),
            5 => Ok(Tag::float(f32::from_bits(self.read_u32("float tag")?))),
            6 => Ok(Tag::double(f64::from_bits(self.read_u64("double tag")?))),
            7 => self.read_byte_array().map(Tag::ByteArray),
            8 => self.read_modified_utf().map(Tag::String),
            9 => {
                let depth = self.enter_container(container_depth)?;
                self.read_list(depth).map(Tag::List)
            }
            10 => {
                let depth = self.enter_container(container_depth)?;
                self.read_compound(depth).map(Tag::Compound)
            }
            11 => self.read_int_array().map(Tag::IntArray),
            12 => self.read_long_array().map(Tag::LongArray),
            _ => self.fail(format!("invalid tag type {tag_type}")),
        }
    }

    fn enter_container(&self, depth: usize) -> Result<usize, BinaryNbtParseError> {
        let depth = depth
            .checked_add(1)
            .ok_or_else(|| self.error("NBT container depth overflow"))?;
        if depth > MAX_DEPTH {
            Err(self.error(format!("NBT exceeds the maximum depth of {MAX_DEPTH}")))
        } else {
            Ok(depth)
        }
    }

    fn read_compound(
        &mut self,
        container_depth: usize,
    ) -> Result<CompoundTag, BinaryNbtParseError> {
        let mut result = CompoundTag::new();
        loop {
            let tag_type = self.read_u8("compound tag type")?;
            if tag_type == 0 {
                return Ok(result);
            }
            validate_tag_type(tag_type).map_err(|reason| self.error(reason))?;
            let name = self.read_modified_utf()?;
            let value = self.read_payload(tag_type, container_depth)?;
            if result.insert(name, value).is_some() {
                return self.fail("duplicate compound key");
            }
        }
    }

    fn read_list(&mut self, container_depth: usize) -> Result<Vec<Tag>, BinaryNbtParseError> {
        let element_type = self.read_u8("list element type")?;
        validate_tag_type_or_end(element_type).map_err(|reason| self.error(reason))?;
        let length = self.read_length("list length")?;
        if element_type == 0 && length != 0 {
            return self.fail("non-empty list has end-tag element type");
        }
        if let Some(minimum_size) = minimum_payload_size(element_type) {
            self.require_elements(length, minimum_size, "list elements")?;
        }
        let mut result = Vec::new();
        reserve_exact(&mut result, length).map_err(|reason| self.error(reason))?;
        for _ in 0..length {
            let value = self.read_payload(element_type, container_depth)?;
            result.push(if element_type == 10 {
                unwrap_heterogeneous_list_element(value)
            } else {
                value
            });
        }
        Ok(result)
    }

    fn read_byte_array(&mut self) -> Result<Vec<i8>, BinaryNbtParseError> {
        let length = self.read_length("byte-array length")?;
        self.require_elements(length, 1, "byte-array elements")?;
        let bytes = self.take(length, "byte-array elements")?;
        let mut result = Vec::new();
        reserve_exact(&mut result, length).map_err(|reason| self.error(reason))?;
        result.extend(bytes.iter().map(|&value| value as i8));
        Ok(result)
    }

    fn read_int_array(&mut self) -> Result<Vec<i32>, BinaryNbtParseError> {
        let length = self.read_length("int-array length")?;
        self.require_elements(length, 4, "int-array elements")?;
        let mut result = Vec::new();
        reserve_exact(&mut result, length).map_err(|reason| self.error(reason))?;
        for _ in 0..length {
            result.push(self.read_i32("int-array element")?);
        }
        Ok(result)
    }

    fn read_long_array(&mut self) -> Result<Vec<i64>, BinaryNbtParseError> {
        let length = self.read_length("long-array length")?;
        self.require_elements(length, 8, "long-array elements")?;
        let mut result = Vec::new();
        reserve_exact(&mut result, length).map_err(|reason| self.error(reason))?;
        for _ in 0..length {
            result.push(self.read_i64("long-array element")?);
        }
        Ok(result)
    }

    fn read_modified_utf(&mut self) -> Result<JavaString, BinaryNbtParseError> {
        let byte_length = usize::from(self.read_u16("modified UTF-8 length")?);
        let bytes = self.take(byte_length, "modified UTF-8 bytes")?;
        let mut units = Vec::new();
        reserve_exact(&mut units, byte_length).map_err(|reason| self.error(reason))?;
        let mut index = 0;
        while index < bytes.len() {
            let first = bytes[index];
            match first >> 4 {
                0..=7 => {
                    units.push(u16::from(first));
                    index += 1;
                }
                12 | 13 => {
                    let Some(&second) = bytes.get(index + 1) else {
                        return self.fail("truncated two-byte modified UTF-8 sequence");
                    };
                    if second & 0xc0 != 0x80 {
                        return self.fail("invalid modified UTF-8 continuation byte");
                    }
                    units.push((u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f));
                    index += 2;
                }
                14 => {
                    let (Some(&second), Some(&third)) =
                        (bytes.get(index + 1), bytes.get(index + 2))
                    else {
                        return self.fail("truncated three-byte modified UTF-8 sequence");
                    };
                    if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 {
                        return self.fail("invalid modified UTF-8 continuation byte");
                    }
                    units.push(
                        (u16::from(first & 0x0f) << 12)
                            | (u16::from(second & 0x3f) << 6)
                            | u16::from(third & 0x3f),
                    );
                    index += 3;
                }
                _ => return self.fail("invalid modified UTF-8 leading byte"),
            }
        }
        Ok(JavaString::from_units(units))
    }

    fn read_length(&mut self, description: &str) -> Result<usize, BinaryNbtParseError> {
        let length = self.read_i32(description)?;
        usize::try_from(length).map_err(|_| self.error(format!("negative {description}")))
    }

    fn require_elements(
        &self,
        length: usize,
        element_size: usize,
        description: &str,
    ) -> Result<(), BinaryNbtParseError> {
        let byte_length = length
            .checked_mul(element_size)
            .ok_or_else(|| self.error(format!("{description} length overflow")))?;
        if byte_length > self.input.len() - self.position {
            Err(self.error(format!("truncated {description}")))
        } else {
            Ok(())
        }
    }

    fn read_u8(&mut self, description: &str) -> Result<u8, BinaryNbtParseError> {
        Ok(self.take(1, description)?[0])
    }

    fn read_u16(&mut self, description: &str) -> Result<u16, BinaryNbtParseError> {
        let bytes = self.take(2, description)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_i16(&mut self, description: &str) -> Result<i16, BinaryNbtParseError> {
        let bytes = self.take(2, description)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self, description: &str) -> Result<u32, BinaryNbtParseError> {
        let bytes = self.take(4, description)?;
        Ok(u32::from_be_bytes(bytes.try_into().expect("four bytes")))
    }

    fn read_i32(&mut self, description: &str) -> Result<i32, BinaryNbtParseError> {
        let bytes = self.take(4, description)?;
        Ok(i32::from_be_bytes(bytes.try_into().expect("four bytes")))
    }

    fn read_u64(&mut self, description: &str) -> Result<u64, BinaryNbtParseError> {
        let bytes = self.take(8, description)?;
        Ok(u64::from_be_bytes(bytes.try_into().expect("eight bytes")))
    }

    fn read_i64(&mut self, description: &str) -> Result<i64, BinaryNbtParseError> {
        let bytes = self.take(8, description)?;
        Ok(i64::from_be_bytes(bytes.try_into().expect("eight bytes")))
    }

    fn take(&mut self, length: usize, description: &str) -> Result<&'a [u8], BinaryNbtParseError> {
        let Some(end) = self.position.checked_add(length) else {
            return self.fail(format!("{description} length overflow"));
        };
        let Some(bytes) = self.input.get(self.position..end) else {
            return self.fail(format!("truncated {description}"));
        };
        self.position = end;
        Ok(bytes)
    }

    fn error(&self, reason: impl fmt::Display) -> BinaryNbtParseError {
        BinaryNbtParseError {
            reason: format!("{reason} at byte {}", self.position),
        }
    }

    fn fail<T>(&self, reason: impl fmt::Display) -> Result<T, BinaryNbtParseError> {
        Err(self.error(reason))
    }
}

fn validate_tag_type(tag_type: u8) -> Result<(), String> {
    if (1..=12).contains(&tag_type) {
        Ok(())
    } else {
        Err(format!("invalid tag type {tag_type}"))
    }
}

fn validate_tag_type_or_end(tag_type: u8) -> Result<(), String> {
    if tag_type <= 12 {
        Ok(())
    } else {
        Err(format!("invalid tag type {tag_type}"))
    }
}

fn tag_type_name(tag_type: u8) -> String {
    match tag_type {
        0 => "end".to_owned(),
        1 => "byte".to_owned(),
        2 => "short".to_owned(),
        3 => "int".to_owned(),
        4 => "long".to_owned(),
        5 => "float".to_owned(),
        6 => "double".to_owned(),
        7 => "byte array".to_owned(),
        8 => "string".to_owned(),
        9 => "list".to_owned(),
        10 => "compound".to_owned(),
        11 => "int array".to_owned(),
        12 => "long array".to_owned(),
        _ => format!("unknown type {tag_type}"),
    }
}

fn minimum_payload_size(tag_type: u8) -> Option<usize> {
    match tag_type {
        0 => None,
        1 => Some(1),
        2 => Some(2),
        3 | 5 => Some(4),
        4 | 6 => Some(8),
        7 | 11 | 12 => Some(4),
        8 => Some(2),
        9 => Some(5),
        10 => Some(1),
        _ => None,
    }
}

fn reserve_exact<T>(values: &mut Vec<T>, length: usize) -> Result<(), String> {
    values
        .try_reserve_exact(length)
        .map_err(|error| format!("cannot allocate space for {length} NBT values: {error}"))
}

fn unwrap_heterogeneous_list_element(value: Tag) -> Tag {
    let Tag::Compound(mut compound) = value else {
        return value;
    };
    if compound.len() == 1
        && let Some(value) = compound.remove(&JavaString::default())
    {
        return value;
    }
    Tag::Compound(compound)
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
        if depth >= MAX_DEPTH {
            return true;
        }
        match self {
            Self::Compound(compound) => compound.is_too_deep(depth),
            Self::List(list) => list.iter().any(|child| child.is_too_deep(depth + 1)),
            _ => false,
        }
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

    pub(crate) fn pretty_stringify(&self) -> JavaString {
        let mut output = Vec::new();
        write_pretty_tag(self, 0, &mut output);
        JavaString::from_units(output)
    }

    pub(crate) fn compact_stringify(&self) -> JavaString {
        compact_stringify(self)
    }

    fn collection_element_equals(&self, index: usize, value: &Tag) -> Option<bool> {
        match self {
            Self::ByteArray(values) => values
                .get(index)
                .map(|current| value == &Self::Byte(*current)),
            Self::List(values) => values.get(index).map(|current| current == value),
            Self::IntArray(values) => values
                .get(index)
                .map(|current| value == &Self::Int(*current)),
            Self::LongArray(values) => values
                .get(index)
                .map(|current| value == &Self::Long(*current)),
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

    fn collection_set(&mut self, index: usize, value: &Tag) -> bool {
        match self {
            Self::ByteArray(values) => value.byte_value().is_some_and(|value| {
                values[index] = value;
                true
            }),
            Self::List(values) => {
                values[index] = value.clone();
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

    fn collection_insert(&mut self, index: i32, value: &Tag) -> Result<bool, String> {
        Ok(match self {
            Self::ByteArray(values) => {
                let Some(value) = value.byte_value() else {
                    return Ok(false);
                };
                let index = validated_insertion_index(index, values.len())?;
                values.insert(index, value);
                true
            }
            Self::List(values) => {
                let index = validated_insertion_index(index, values.len())?;
                values.insert(index, value.clone());
                true
            }
            Self::IntArray(values) => {
                let Some(value) = value.int_value() else {
                    return Ok(false);
                };
                let index = validated_insertion_index(index, values.len())?;
                values.insert(index, value);
                true
            }
            Self::LongArray(values) => {
                let Some(value) = value.long_value() else {
                    return Ok(false);
                };
                let index = validated_insertion_index(index, values.len())?;
                values.insert(index, value);
                true
            }
            _ => return Err("expected a list".to_owned()),
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

fn validated_insertion_index(index: i32, length: usize) -> Result<usize, String> {
    let index = usize::try_from(index).map_err(|_| format!("invalid list index {index}"))?;
    if index > length {
        Err(format!("invalid list index {index}"))
    } else {
        Ok(index)
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
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

fn sorted_compound_entries(compound: &CompoundTag) -> Vec<(&JavaString, &Tag)> {
    let mut entries = compound.0.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|(name, _)| *name);
    entries
}

fn write_pretty_tag(tag: &Tag, depth: usize, output: &mut Vec<u16>) {
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
        Tag::ByteArray(values) => write_pretty_array(
            b'B',
            values,
            |value, output| {
                push_ascii(output, &value.to_string());
                output.push(u16::from(b'b'));
            },
            output,
        ),
        Tag::String(value) => write_quoted(value, output),
        Tag::List(values) if values.is_empty() => push_ascii(output, "[]"),
        Tag::List(_) if depth >= 64 => push_ascii(output, "[<...>]"),
        Tag::List(values) => {
            output.push(u16::from(b'['));
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    push_ascii(output, ", ");
                }
                write_pretty_tag(value, depth + 1, output);
            }
            output.push(u16::from(b']'));
        }
        Tag::Compound(compound) => write_pretty_compound(compound, depth, output),
        Tag::IntArray(values) => write_pretty_array(
            b'I',
            values,
            |value, output| push_ascii(output, &value.to_string()),
            output,
        ),
        Tag::LongArray(values) => write_pretty_array(
            b'L',
            values,
            |value, output| {
                push_ascii(output, &value.to_string());
                output.push(u16::from(b'L'));
            },
            output,
        ),
    }
}

fn write_pretty_compound(compound: &CompoundTag, depth: usize, output: &mut Vec<u16>) {
    if compound.is_empty() {
        push_ascii(output, "{}");
        return;
    }
    if depth >= 64 {
        push_ascii(output, "{<...>}");
        return;
    }

    output.push(u16::from(b'{'));
    for (index, (name, value)) in compound.0.iter().enumerate() {
        if index != 0 {
            push_ascii(output, ", ");
        }
        if is_pretty_unquoted_key(name) {
            output.extend_from_slice(name.units());
        } else {
            write_quoted(name, output);
        }
        push_ascii(output, ": ");
        write_pretty_tag(value, depth + 1, output);
    }
    output.push(u16::from(b'}'));
}

fn is_pretty_unquoted_key(value: &JavaString) -> bool {
    !value.units().is_empty()
        && value.units().iter().copied().all(|unit| {
            matches!(unit, 0x41..=0x5a | 0x61..=0x7a | 0x30..=0x39 | 0x2e | 0x5f | 0x2b | 0x2d)
        })
}

fn write_pretty_array<T>(
    prefix: u8,
    values: &[T],
    mut write_value: impl FnMut(&T, &mut Vec<u16>),
    output: &mut Vec<u16>,
) {
    output.push(u16::from(b'['));
    output.push(u16::from(prefix));
    output.push(u16::from(b';'));
    for (index, value) in values.iter().take(128).enumerate() {
        output.push(u16::from(b' '));
        write_value(value, output);
        if index != values.len() - 1 {
            output.push(u16::from(b','));
        }
    }
    if values.len() > 128 {
        push_ascii(output, "<...>");
    }
    output.push(u16::from(b']'));
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
            for (index, (name, value)) in sorted_compound_entries(compound).into_iter().enumerate()
            {
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
    nodes: Arc<[PathNode]>,
    original: JavaString,
    node_ends: Arc<[usize]>,
}

pub(crate) enum NbtSelection<'a> {
    Root(&'a CompoundTag),
    Tag(&'a Tag),
    ArrayElement(Tag),
}

impl NbtSelection<'_> {
    pub(crate) fn as_tag(&self) -> Option<&Tag> {
        match self {
            Self::Root(_) => None,
            Self::Tag(tag) => Some(tag),
            Self::ArrayElement(tag) => Some(tag),
        }
    }

    pub(crate) fn is_compound(&self) -> bool {
        matches!(self, Self::Root(_) | Self::Tag(Tag::Compound(_)))
    }

    pub(crate) fn is_too_deep(&self, depth: usize) -> bool {
        match self {
            Self::Root(root) => root.is_too_deep(depth),
            Self::Tag(tag) => tag.is_too_deep(depth),
            Self::ArrayElement(tag) => tag.is_too_deep(depth),
        }
    }

    pub(crate) fn into_owned(self) -> Tag {
        match self {
            Self::Root(root) => Tag::Compound(root.clone()),
            Self::Tag(tag) => tag.clone(),
            Self::ArrayElement(tag) => tag,
        }
    }

    pub(crate) fn pretty_stringify(&self) -> JavaString {
        match self {
            Self::Root(root) => root.pretty_stringify(),
            Self::Tag(tag) => tag.pretty_stringify(),
            Self::ArrayElement(tag) => tag.pretty_stringify(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum NbtEditError {
    DataTooDeep,
    NothingFound(JavaString),
    ExpectedList(Tag),
    ExpectedObject(Tag),
    InvalidListIndex(i32),
    Other(String),
}

impl NbtPath {
    pub(crate) fn parse(reader: &mut StringReader) -> Result<Self, String> {
        let start = reader.cursor();
        let mut nodes = Vec::new();
        let mut node_ends = Vec::new();
        let mut all_elements_end = None;
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
            let node_end = reader.cursor() - start;
            if matches!(node, PathNode::AllElements) {
                all_elements_end = Some(node_end);
            }
            nodes.push(node);
            node_ends.push(node_end);
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
            if let Some(all_elements_end) = all_elements_end {
                for (node, node_end) in nodes.iter().zip(&mut node_ends) {
                    if matches!(node, PathNode::AllElements) {
                        *node_end = all_elements_end;
                    }
                }
            }
            Ok(Self {
                nodes: nodes.into(),
                original: JavaString::from_units(reader.substring_utf16(start, reader.cursor())),
                node_ends: node_ends.into(),
            })
        }
    }

    pub(crate) fn parse_codec(reader: &mut StringReader) -> Result<Self, String> {
        if !reader.can_read() || reader.peek() == 0x20 {
            Ok(Self {
                nodes: Arc::from([]),
                original: JavaString::default(),
                node_ends: Arc::from([]),
            })
        } else {
            Self::parse(reader)
        }
    }

    pub(crate) fn original(&self) -> &JavaString {
        &self.original
    }

    pub(crate) fn depth(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn select_with_not_found<'a>(
        &self,
        root: &'a CompoundTag,
    ) -> Result<Vec<NbtSelection<'a>>, JavaString> {
        self.select(root).map_err(|index| {
            JavaString::from_units(self.original.units()[..self.node_ends[index]].to_vec())
        })
    }

    pub(crate) fn select<'a>(&self, root: &'a CompoundTag) -> Result<Vec<NbtSelection<'a>>, usize> {
        let mut current = vec![NbtSelection::Root(root)];
        for (index, node) in self.nodes.iter().enumerate() {
            let mut next = Vec::new();
            for tag in &current {
                node.collect_selection(tag, &mut next);
            }
            if next.is_empty() {
                return Err(index);
            }
            current = next;
        }
        Ok(current)
    }

    pub(crate) fn count_matching(&self, root: &CompoundTag) -> usize {
        self.select(root).map_or(0, |tags| tags.len())
    }

    pub(crate) fn set(&self, root: &mut CompoundTag, value: &Tag) -> Result<i32, NbtEditError> {
        if value.is_too_deep(self.nodes.len()) {
            return Err(NbtEditError::DataTooDeep);
        }
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            let last_index = self.nodes.len() - 1;
            let parents =
                self.resolve_for_edit(&mut root_tag, last_index, &Tag::List(Vec::new()))?;
            let last = &self.nodes[last_index];
            let mut changed = 0_i32;
            for parent in parents {
                changed = changed.wrapping_add(parent.apply(|parent| last.set(parent, value)));
            }
            Ok(changed)
        })();
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        result
    }

    pub(crate) fn insert(
        &self,
        index: i32,
        root: &mut CompoundTag,
        values: &[Tag],
    ) -> Result<i32, NbtEditError> {
        for value in values {
            if value.is_too_deep(self.nodes.len()) {
                return Err(NbtEditError::DataTooDeep);
            }
        }
        let mut modified = 0_i32;
        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            let targets =
                self.resolve_for_edit(&mut root_tag, self.nodes.len(), &Tag::List(Vec::new()))?;
            for target in targets {
                target.apply(|target| {
                    let Some(size) = target.collection_len() else {
                        return Err(NbtEditError::ExpectedList(target.clone()));
                    };
                    let size = i32::try_from(size)
                        .map_err(|_| NbtEditError::Other("NBT list is too large".to_owned()))?;
                    let mut actual_index = if index < 0 {
                        size.wrapping_add(index).wrapping_add(1)
                    } else {
                        index
                    };
                    let mut changed = false;
                    for value in values {
                        let inserted = target
                            .collection_insert(actual_index, value)
                            .map_err(|_| NbtEditError::InvalidListIndex(actual_index))?;
                        if inserted {
                            actual_index = actual_index.wrapping_add(1);
                            changed = true;
                        }
                    }
                    if changed {
                        modified = modified.wrapping_add(1);
                    }
                    Ok(())
                })?;
            }
            Ok(modified)
        })();
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot replace its root compound"),
        };
        result
    }

    pub(crate) fn merge(
        &self,
        root: &mut CompoundTag,
        sources: &[Tag],
    ) -> Result<i32, NbtEditError> {
        let mut combined = CompoundTag::new();
        for source in sources {
            if source.is_too_deep(0) {
                return Err(NbtEditError::DataTooDeep);
            }
            let Tag::Compound(source) = source else {
                return Err(NbtEditError::ExpectedObject(source.clone()));
            };
            combined.merge(source);
        }

        let mut root_tag = Tag::Compound(std::mem::take(root));
        let result = (|| {
            let targets = self.resolve_for_edit(
                &mut root_tag,
                self.nodes.len(),
                &Tag::Compound(CompoundTag::new()),
            )?;
            let mut changed = 0_i32;
            for target in targets {
                target.apply(|target| {
                    if !matches!(target, Tag::Compound(_)) {
                        return Err(NbtEditError::ExpectedObject(target.clone()));
                    };
                    let Tag::Compound(target) = target else {
                        unreachable!("the target tag type was checked")
                    };
                    let previous = target.clone();
                    target.merge(&combined);
                    if *target != previous {
                        changed = changed.wrapping_add(1);
                    }
                    Ok(())
                })?;
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
        let last = self.nodes.last().expect("NBT paths have at least one node");
        let mut removed = 0_i32;
        for parent in self.resolve_mutable(&mut root_tag, self.nodes.len() - 1) {
            removed = removed.wrapping_add(parent.apply(|parent| last.remove(parent)));
        }
        *root = match root_tag {
            Tag::Compound(root) => root,
            _ => unreachable!("an NBT path cannot remove its root compound"),
        };
        removed
    }

    fn resolve_for_edit<'a>(
        &self,
        root: &'a mut Tag,
        end: usize,
        final_preferred: &Tag,
    ) -> Result<Vec<MutableSelection<'a>>, NbtEditError> {
        let mut current = vec![MutableSelection::Tag(root)];
        for index in 0..end {
            let preferred = self
                .nodes
                .get(index + 1)
                .map_or_else(|| final_preferred.clone(), PathNode::preferred_parent);
            let mut next = Vec::new();
            for selection in current {
                if let MutableSelection::Tag(parent) = selection {
                    self.nodes[index].collect_or_create_mut(parent, &preferred, &mut next);
                }
            }
            if next.is_empty() && index < self.nodes.len() - 1 {
                return Err(NbtEditError::NothingFound(JavaString::from_units(
                    self.original.units()[..self.node_ends[index]].to_vec(),
                )));
            }
            current = next;
        }
        Ok(current)
    }

    fn resolve_mutable<'a>(&self, root: &'a mut Tag, end: usize) -> Vec<MutableSelection<'a>> {
        let mut current = vec![MutableSelection::Tag(root)];
        for node in &self.nodes[..end] {
            let mut next = Vec::new();
            for selection in current {
                if let MutableSelection::Tag(parent) = selection {
                    node.collect_mut(parent, &mut next);
                }
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }
        current
    }
}

enum MutableSelection<'a> {
    Tag(&'a mut Tag),
    ArrayElement(Tag),
}

impl MutableSelection<'_> {
    fn apply<R>(self, operation: impl FnOnce(&mut Tag) -> R) -> R {
        match self {
            Self::Tag(tag) => operation(tag),
            Self::ArrayElement(mut tag) => operation(&mut tag),
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

    fn collect_or_create_mut<'a>(
        &self,
        parent: &'a mut Tag,
        preferred: &Tag,
        output: &mut Vec<MutableSelection<'a>>,
    ) {
        match self {
            Self::AllElements => {
                if let Tag::List(values) = parent
                    && values.is_empty()
                {
                    values.push(preferred.clone());
                }
            }
            Self::CompoundChild(name) => {
                if let Some(parent) = parent.as_compound_mut()
                    && !parent.contains_key(name)
                {
                    parent.insert(name.clone(), preferred.clone());
                }
            }
            Self::MatchElement(pattern) => {
                if let Tag::List(values) = parent
                    && !values
                        .iter()
                        .any(|value| partial_matches_compound(pattern, value))
                {
                    values.push(Tag::Compound(pattern.clone()));
                }
            }
            Self::MatchObject(name, pattern) => {
                if let Some(parent) = parent.as_compound_mut()
                    && !parent.contains_key(name)
                {
                    parent.insert(name.clone(), Tag::Compound(pattern.clone()));
                }
            }
            Self::Indexed(_) | Self::MatchRoot(_) => {}
        }
        self.collect_mut(parent, output);
    }

    fn collect_mut<'a>(&self, parent: &'a mut Tag, output: &mut Vec<MutableSelection<'a>>) {
        match self {
            Self::AllElements => match parent {
                Tag::ByteArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Byte)
                        .map(MutableSelection::ArrayElement),
                ),
                Tag::List(values) => {
                    output.extend(values.iter_mut().map(MutableSelection::Tag));
                }
                Tag::IntArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Int)
                        .map(MutableSelection::ArrayElement),
                ),
                Tag::LongArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Long)
                        .map(MutableSelection::ArrayElement),
                ),
                _ => {}
            },
            Self::CompoundChild(name) => {
                if let Some(value) = parent
                    .as_compound_mut()
                    .and_then(|parent| parent.get_mut(name))
                {
                    output.push(MutableSelection::Tag(value));
                }
            }
            Self::Indexed(index) => {
                if let Some(index) = actual_index(*index, parent.collection_len()) {
                    match parent {
                        Tag::ByteArray(values) => {
                            output.push(MutableSelection::ArrayElement(Tag::Byte(values[index])))
                        }
                        Tag::List(values) => {
                            output.push(MutableSelection::Tag(&mut values[index]));
                        }
                        Tag::IntArray(values) => {
                            output.push(MutableSelection::ArrayElement(Tag::Int(values[index])))
                        }
                        Tag::LongArray(values) => {
                            output.push(MutableSelection::ArrayElement(Tag::Long(values[index])))
                        }
                        _ => {}
                    }
                }
            }
            Self::MatchElement(pattern) => {
                if let Tag::List(values) = parent {
                    output.extend(
                        values
                            .iter_mut()
                            .filter(|value| partial_matches_compound(pattern, value))
                            .map(MutableSelection::Tag),
                    );
                }
            }
            Self::MatchObject(name, pattern) => {
                if let Some(value) = parent
                    .as_compound_mut()
                    .and_then(|parent| parent.get_mut(name))
                    && partial_matches_compound(pattern, value)
                {
                    output.push(MutableSelection::Tag(value));
                }
            }
            Self::MatchRoot(pattern) => {
                if partial_matches_compound(pattern, parent) {
                    output.push(MutableSelection::Tag(parent));
                }
            }
        }
    }

    fn collect_selection<'a>(&self, parent: &NbtSelection<'a>, output: &mut Vec<NbtSelection<'a>>) {
        match parent {
            NbtSelection::Root(parent) => self.collect_root_selection(parent, output),
            NbtSelection::Tag(parent) => self.collect_tag_selection(parent, output),
            NbtSelection::ArrayElement(_) => {}
        }
    }

    fn collect_root_selection<'a>(
        &self,
        parent: &'a CompoundTag,
        output: &mut Vec<NbtSelection<'a>>,
    ) {
        match self {
            Self::CompoundChild(name) => {
                if let Some(tag) = parent.get(name) {
                    output.push(NbtSelection::Tag(tag));
                }
            }
            Self::MatchObject(name, pattern) => {
                if let Some(value) = parent.get(name)
                    && partial_matches_compound(pattern, value)
                {
                    output.push(NbtSelection::Tag(value));
                }
            }
            Self::MatchRoot(pattern) if compound_partial_matches(pattern, parent) => {
                output.push(NbtSelection::Root(parent));
            }
            Self::AllElements | Self::Indexed(_) | Self::MatchElement(_) | Self::MatchRoot(_) => {}
        }
    }

    fn collect_tag_selection<'a>(&self, parent: &'a Tag, output: &mut Vec<NbtSelection<'a>>) {
        match self {
            Self::AllElements => match parent {
                Tag::ByteArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Byte)
                        .map(NbtSelection::ArrayElement),
                ),
                Tag::List(values) => {
                    output.extend(values.iter().map(NbtSelection::Tag));
                }
                Tag::IntArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Int)
                        .map(NbtSelection::ArrayElement),
                ),
                Tag::LongArray(values) => output.extend(
                    values
                        .iter()
                        .copied()
                        .map(Tag::Long)
                        .map(NbtSelection::ArrayElement),
                ),
                _ => {}
            },
            Self::CompoundChild(name) => {
                if let Some(tag) = parent.as_compound().and_then(|parent| parent.get(name)) {
                    output.push(NbtSelection::Tag(tag));
                }
            }
            Self::Indexed(index) => {
                if let Some(index) = actual_index(*index, parent.collection_len()) {
                    match parent {
                        Tag::ByteArray(values) => {
                            output.push(NbtSelection::ArrayElement(Tag::Byte(values[index])))
                        }
                        Tag::List(values) => output.push(NbtSelection::Tag(&values[index])),
                        Tag::IntArray(values) => {
                            output.push(NbtSelection::ArrayElement(Tag::Int(values[index])))
                        }
                        Tag::LongArray(values) => {
                            output.push(NbtSelection::ArrayElement(Tag::Long(values[index])))
                        }
                        _ => {}
                    }
                }
            }
            Self::MatchElement(pattern) => {
                if let Tag::List(values) = parent {
                    output.extend(
                        values
                            .iter()
                            .filter(|value| partial_matches_compound(pattern, value))
                            .map(NbtSelection::Tag),
                    );
                }
            }
            Self::MatchObject(name, pattern) => {
                if let Some(value) = parent.as_compound().and_then(|parent| parent.get(name))
                    && partial_matches_compound(pattern, value)
                {
                    output.push(NbtSelection::Tag(value));
                }
            }
            Self::MatchRoot(pattern) => {
                if matches!(parent, Tag::Compound(_)) && partial_matches_compound(pattern, parent) {
                    output.push(NbtSelection::Tag(parent));
                }
            }
        }
    }

    fn set(&self, parent: &mut Tag, value: &Tag) -> i32 {
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
                    .filter(|&index| {
                        parent
                            .collection_element_equals(index, value)
                            .is_some_and(|equal| !equal)
                    })
                    .count();
                if changed == 0 {
                    return 0;
                }
                parent.collection_clear();
                if !parent.collection_insert(0, value).unwrap_or(false) {
                    return 0;
                }
                let size = i32::try_from(size)
                    .expect("an NBT collection cannot exceed the Java int range");
                for index in 1..size {
                    let _ = parent.collection_insert(index, value);
                }
                i32::try_from(changed).unwrap_or(i32::MAX)
            }
            Self::CompoundChild(name) => parent.as_compound_mut().map_or(0, |parent| {
                if parent.get(name) == Some(value) {
                    0
                } else {
                    parent.insert(name.clone(), value.clone());
                    1
                }
            }),
            Self::Indexed(index) => {
                let Some(index) = actual_index(*index, parent.collection_len()) else {
                    return 0;
                };
                if parent.collection_element_equals(index, value) == Some(true) {
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
                    values.push(value.clone());
                    return 1;
                }
                let mut changed = 0_i32;
                for current in values {
                    if partial_matches_compound(pattern, current) && current != value {
                        *current = value.clone();
                        changed = changed.wrapping_add(1);
                    }
                }
                changed
            }
            Self::MatchObject(name, pattern) => parent.as_compound_mut().map_or(0, |parent| {
                if parent.get(name).is_some_and(|current| {
                    partial_matches_compound(pattern, current) && current != value
                }) {
                    parent.insert(name.clone(), value.clone());
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
                values.retain(|value| !partial_matches_compound(pattern, value));
                i32::try_from(old - values.len()).unwrap_or(i32::MAX)
            }
            Self::MatchObject(name, pattern) => {
                i32::from(parent.as_compound_mut().is_some_and(|parent| {
                    if parent
                        .get(name)
                        .is_some_and(|current| partial_matches_compound(pattern, current))
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
            compound_partial_matches(expected, actual)
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

fn partial_matches_compound(expected: &CompoundTag, actual: &Tag) -> bool {
    let Tag::Compound(actual) = actual else {
        return false;
    };
    compound_partial_matches(expected, actual)
}

fn compound_partial_matches(expected: &CompoundTag, actual: &CompoundTag) -> bool {
    actual.len() >= expected.len()
        && expected.0.iter().all(|(name, expected)| {
            actual
                .get(name)
                .is_some_and(|actual| partial_matches(expected, actual))
        })
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
    values: StorageNamespaces,
}

type StorageNamespaces = HashMap<IdentifierPart, StorageValues, BuildHasherDefault<DefaultHasher>>;
type StorageValues = HashMap<IdentifierPart, CompoundTag, BuildHasherDefault<DefaultHasher>>;

impl CommandStorage {
    pub(crate) fn get_ref(&self, id: &Identifier) -> Option<&CompoundTag> {
        self.values
            .get(id.namespace_key())
            .and_then(|namespace| namespace.get(id.path_key()))
    }

    pub(crate) fn get(&self, id: &Identifier) -> Cow<'_, CompoundTag> {
        self.get_ref(id)
            .map_or_else(|| Cow::Owned(CompoundTag::new()), Cow::Borrowed)
    }

    pub(crate) fn set(&mut self, id: Identifier, value: CompoundTag) {
        let (namespace, path) = id.into_parts();
        if value.is_empty() {
            let remove_namespace = self.values.get_mut(&namespace).is_some_and(|values| {
                values.remove(&path);
                values.is_empty()
            });
            if remove_namespace {
                self.values.remove(&namespace);
            }
        } else {
            self.values
                .entry(namespace)
                .or_default()
                .insert(path, value);
        }
    }

    pub(crate) fn replace_namespace(
        &mut self,
        namespace: &str,
        values: impl IntoIterator<Item = (Identifier, CompoundTag)>,
    ) {
        let expected_namespace = IdentifierPart::new(namespace);
        let mut storage_namespace = None;
        let mut replacement = StorageValues::default();
        for (id, value) in values {
            let (id_namespace, path) = id.into_parts();
            let namespace = storage_namespace.get_or_insert_with(|| {
                assert_eq!(
                    id_namespace, expected_namespace,
                    "storage namespace replacement mismatch"
                );
                id_namespace.clone()
            });
            assert_eq!(
                id_namespace, *namespace,
                "storage namespace replacement mismatch"
            );
            if !value.is_empty() {
                replacement.insert(path, value);
            }
        }
        let namespace = storage_namespace.unwrap_or(expected_namespace);
        if replacement.is_empty() {
            self.values.remove(&namespace);
        } else {
            self.values.insert(namespace, replacement);
        }
    }

    pub(crate) fn edit<R, E>(
        &mut self,
        id: &Identifier,
        operation: impl FnOnce(&mut CompoundTag) -> Result<R, E>,
    ) -> Result<R, E> {
        if let Some(values) = self.values.get_mut(id.namespace_key()) {
            if values.contains_key(id.path_key()) {
                let result = operation(
                    values
                        .get_mut(id.path_key())
                        .expect("the storage path was found immediately before editing"),
                );
                if result.is_ok() && values.get(id.path_key()).is_some_and(CompoundTag::is_empty) {
                    values.remove(id.path_key());
                }
                let remove_namespace = result.is_ok() && values.is_empty();
                if remove_namespace {
                    self.values.remove(id.namespace_key());
                }
                result
            } else {
                let mut value = CompoundTag::new();
                let result = operation(&mut value);
                if result.is_ok() && !value.is_empty() {
                    values.insert(id.path_key().clone(), value);
                }
                result
            }
        } else {
            let mut value = CompoundTag::new();
            let result = operation(&mut value);
            if result.is_ok() && !value.is_empty() {
                self.values
                    .entry(id.namespace_key().clone())
                    .or_default()
                    .insert(id.path_key().clone(), value);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_binary_string(output: &mut Vec<u8>, bytes: &[u8]) {
        output.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
        output.extend_from_slice(bytes);
    }

    fn push_binary_header(output: &mut Vec<u8>, tag_type: u8, name: &[u8]) {
        output.push(tag_type);
        push_binary_string(output, name);
    }

    fn binary_root() -> Vec<u8> {
        vec![10, 0, 0]
    }

    fn binary_tag<'a>(root: &'a CompoundTag, name: &str) -> &'a Tag {
        root.get(&JavaString::from(name)).unwrap()
    }

    #[test]
    fn binary_nbt_reads_every_payload_type_and_modified_utf() {
        let mut input = binary_root();
        push_binary_header(&mut input, 1, b"byte");
        input.push(0xfe);
        push_binary_header(&mut input, 2, b"short");
        input.extend_from_slice(&(-1234_i16).to_be_bytes());
        push_binary_header(&mut input, 3, b"int");
        input.extend_from_slice(&(-123_456_i32).to_be_bytes());
        push_binary_header(&mut input, 4, b"long");
        input.extend_from_slice(&(-123_456_789_i64).to_be_bytes());
        push_binary_header(&mut input, 5, b"float");
        input.extend_from_slice(&(-0.0_f32).to_bits().to_be_bytes());
        push_binary_header(&mut input, 6, b"double");
        input.extend_from_slice(&(-0.0_f64).to_bits().to_be_bytes());
        push_binary_header(&mut input, 5, b"nan");
        input.extend_from_slice(&0x7fc0_1234_u32.to_be_bytes());
        push_binary_header(&mut input, 7, b"bytes");
        input.extend_from_slice(&3_i32.to_be_bytes());
        input.extend_from_slice(&[0xff, 0, 1]);
        push_binary_header(&mut input, 8, b"string");
        push_binary_string(&mut input, &[0, 0xc0, 0x80, 0xc1, 0x81, 0xed, 0xa0, 0x80]);
        push_binary_header(&mut input, 9, b"list");
        input.push(3);
        input.extend_from_slice(&2_i32.to_be_bytes());
        input.extend_from_slice(&7_i32.to_be_bytes());
        input.extend_from_slice(&8_i32.to_be_bytes());
        push_binary_header(&mut input, 10, b"compound");
        push_binary_header(&mut input, 1, b"value");
        input.push(9);
        input.push(0);
        push_binary_header(&mut input, 11, b"ints");
        input.extend_from_slice(&2_i32.to_be_bytes());
        input.extend_from_slice(&(-1_i32).to_be_bytes());
        input.extend_from_slice(&2_i32.to_be_bytes());
        push_binary_header(&mut input, 12, b"longs");
        input.extend_from_slice(&2_i32.to_be_bytes());
        input.extend_from_slice(&(-3_i64).to_be_bytes());
        input.extend_from_slice(&4_i64.to_be_bytes());
        input.push(0);
        input.extend_from_slice(b"ignored trailing bytes");

        let root = parse_binary_compound(&input).unwrap();
        assert_eq!(binary_tag(&root, "byte"), &Tag::Byte(-2));
        assert_eq!(binary_tag(&root, "short"), &Tag::Short(-1234));
        assert_eq!(binary_tag(&root, "int"), &Tag::Int(-123_456));
        assert_eq!(binary_tag(&root, "long"), &Tag::Long(-123_456_789));
        assert_eq!(binary_tag(&root, "float"), &Tag::Float(0.0_f32.to_bits()));
        assert_eq!(binary_tag(&root, "double"), &Tag::Double(0.0_f64.to_bits()));
        assert_eq!(binary_tag(&root, "nan"), &Tag::Float(0x7fc0_1234));
        assert_eq!(binary_tag(&root, "bytes"), &Tag::ByteArray(vec![-1, 0, 1]));
        assert_eq!(
            binary_tag(&root, "string"),
            &Tag::String(JavaString::from_units(vec![0, 0, 0x41, 0xd800]))
        );
        assert_eq!(
            binary_tag(&root, "list"),
            &Tag::List(vec![Tag::Int(7), Tag::Int(8)])
        );
        let mut compound = CompoundTag::new();
        compound.insert(JavaString::from("value"), Tag::Byte(9));
        assert_eq!(binary_tag(&root, "compound"), &Tag::Compound(compound));
        assert_eq!(binary_tag(&root, "ints"), &Tag::IntArray(vec![-1, 2]));
        assert_eq!(binary_tag(&root, "longs"), &Tag::LongArray(vec![-3, 4]));
        assert_eq!(
            binary_tag(&root, "float").compact_stringify(),
            JavaString::from("0.0f")
        );
        assert_eq!(
            binary_tag(&root, "double").compact_stringify(),
            JavaString::from("0.0d")
        );
    }

    #[test]
    fn binary_nbt_unwraps_compound_list_elements_once() {
        let mut input = binary_root();
        push_binary_header(&mut input, 9, b"values");
        input.push(10);
        input.extend_from_slice(&3_i32.to_be_bytes());

        push_binary_header(&mut input, 8, b"");
        push_binary_string(&mut input, b"a");
        input.push(0);

        push_binary_header(&mut input, 3, b"b");
        input.extend_from_slice(&3_i32.to_be_bytes());
        input.push(0);

        push_binary_header(&mut input, 10, b"");
        push_binary_header(&mut input, 1, b"");
        input.push(7);
        input.push(0);
        input.push(0);

        input.push(0);

        let root = parse_binary_compound(&input).unwrap();
        let mut ordinary = CompoundTag::new();
        ordinary.insert(JavaString::from("b"), Tag::Int(3));
        let mut empty_key_compound = CompoundTag::new();
        empty_key_compound.insert(JavaString::default(), Tag::Byte(7));
        assert_eq!(
            binary_tag(&root, "values"),
            &Tag::List(vec![
                Tag::String(JavaString::from("a")),
                Tag::Compound(ordinary),
                Tag::Compound(empty_key_compound),
            ])
        );
    }

    #[test]
    fn binary_nbt_skips_the_root_name_without_decoding_it() {
        let input = [10, 0, 4, 0xf0, 0x80, 0x80, 0x80, 0];
        assert_eq!(parse_binary_compound(&input).unwrap(), CompoundTag::new());
    }

    #[test]
    fn binary_nbt_rejects_invalid_modified_utf_and_decoded_duplicate_keys() {
        let mut invalid_string = binary_root();
        push_binary_header(&mut invalid_string, 8, b"value");
        push_binary_string(&mut invalid_string, &[0xf0, 0x80, 0x80, 0x80]);
        invalid_string.push(0);
        assert!(
            parse_binary_compound(&invalid_string)
                .unwrap_err()
                .reason()
                .contains("leading byte")
        );

        let mut duplicate = binary_root();
        push_binary_header(&mut duplicate, 1, b"A");
        duplicate.push(1);
        push_binary_header(&mut duplicate, 1, &[0xc1, 0x81]);
        duplicate.push(2);
        duplicate.push(0);
        assert!(
            parse_binary_compound(&duplicate)
                .unwrap_err()
                .reason()
                .contains("duplicate")
        );
    }

    #[test]
    fn binary_nbt_validates_list_types_and_declared_lengths_before_allocation() {
        let mut empty_end_list = binary_root();
        push_binary_header(&mut empty_end_list, 9, b"empty");
        empty_end_list.push(0);
        empty_end_list.extend_from_slice(&0_i32.to_be_bytes());
        empty_end_list.push(0);
        assert!(parse_binary_compound(&empty_end_list).is_ok());

        let mut nonempty_end_list = binary_root();
        push_binary_header(&mut nonempty_end_list, 9, b"invalid");
        nonempty_end_list.push(0);
        nonempty_end_list.extend_from_slice(&1_i32.to_be_bytes());
        nonempty_end_list.push(0);
        assert!(parse_binary_compound(&nonempty_end_list).is_err());

        let mut negative_array = binary_root();
        push_binary_header(&mut negative_array, 7, b"invalid");
        negative_array.extend_from_slice(&(-1_i32).to_be_bytes());
        negative_array.push(0);
        assert!(
            parse_binary_compound(&negative_array)
                .unwrap_err()
                .reason()
                .contains("negative")
        );

        let mut huge_array = binary_root();
        push_binary_header(&mut huge_array, 12, b"invalid");
        huge_array.extend_from_slice(&i32::MAX.to_be_bytes());
        huge_array.push(0);
        assert!(
            parse_binary_compound(&huge_array)
                .unwrap_err()
                .reason()
                .contains("truncated")
        );
    }

    fn nested_binary_compounds(container_count: usize) -> Vec<u8> {
        let mut input = binary_root();
        for _ in 1..container_count {
            push_binary_header(&mut input, 10, b"x");
        }
        input.extend(std::iter::repeat_n(0, container_count));
        input
    }

    #[test]
    fn binary_nbt_accepts_512_containers_and_rejects_the_513th() {
        assert!(parse_binary_compound(&nested_binary_compounds(512)).is_ok());
        assert!(
            parse_binary_compound(&nested_binary_compounds(513))
                .unwrap_err()
                .reason()
                .contains("maximum depth")
        );
    }

    #[test]
    fn replacing_a_storage_namespace_removes_empty_and_stale_entries_only_there() {
        let mut storage = CommandStorage::default();
        storage.set(
            Identifier::from_parts("probe", "stale").unwrap(),
            CompoundTag::from_snbt("{value:1}").unwrap(),
        );
        storage.set(
            Identifier::from_parts("keep", "state").unwrap(),
            CompoundTag::from_snbt("{value:2}").unwrap(),
        );

        storage.replace_namespace(
            "probe",
            [
                (
                    Identifier::from_parts("probe", "loaded").unwrap(),
                    CompoundTag::from_snbt("{value:3}").unwrap(),
                ),
                (
                    Identifier::from_parts("probe", "empty").unwrap(),
                    CompoundTag::new(),
                ),
            ],
        );

        assert!(
            storage
                .get_ref(&Identifier::from_parts("probe", "stale").unwrap())
                .is_none()
        );
        assert!(
            storage
                .get_ref(&Identifier::from_parts("probe", "loaded").unwrap())
                .is_some()
        );
        assert!(
            storage
                .get_ref(&Identifier::from_parts("probe", "empty").unwrap())
                .is_none()
        );
        assert!(
            storage
                .get_ref(&Identifier::from_parts("keep", "state").unwrap())
                .is_some()
        );
    }

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
