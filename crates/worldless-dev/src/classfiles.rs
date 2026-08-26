use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter, write::SimpleFileOptions};

const MANIFEST_PATH: &[u8] = b"META-INF/MANIFEST.MF";
const VERSIONED_PREFIX: &str = "META-INF/versions/";

#[derive(Debug)]
pub struct Analysis {
    classes: Vec<SelectedClass>,
    expected_sources: BTreeSet<String>,
    runtime_overridden_sources: BTreeSet<String>,
    source_to_decompiled: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug)]
struct SelectedClass {
    logical_path: String,
    source_path: String,
    bytes: Vec<u8>,
}

impl Analysis {
    pub fn expected_sources(&self) -> &BTreeSet<String> {
        &self.expected_sources
    }

    pub fn runtime_overridden_sources(&self) -> &BTreeSet<String> {
        &self.runtime_overridden_sources
    }

    pub fn expected_decompiled_paths(
        &self,
        source_paths: Option<&BTreeSet<String>>,
    ) -> Result<BTreeSet<String>> {
        self.validate_source_filter(source_paths)?;
        Ok(self
            .source_to_decompiled
            .iter()
            .filter(|(source, _)| source_paths.is_none_or(|filter| filter.contains(*source)))
            .flat_map(|(_, decompiled)| decompiled.iter().cloned())
            .collect())
    }

    pub fn write_normalized_jar(
        &self,
        output: &Path,
        source_paths: Option<&BTreeSet<String>>,
    ) -> Result<()> {
        self.validate_source_filter(source_paths)?;
        let parent = output
            .parent()
            .with_context(|| format!("normalized JAR has no parent: {}", output.display()))?;
        let metadata = fs::symlink_metadata(parent).with_context(|| {
            format!(
                "failed to inspect normalized JAR parent {}",
                parent.display()
            )
        })?;
        if !metadata.file_type().is_dir() {
            bail!(
                "normalized JAR parent is not a regular directory: {}",
                parent.display()
            );
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .with_context(|| format!("failed to create normalized JAR {}", output.display()))?;
        let result = (|| -> Result<()> {
            let mut file = self.write_normalized_to(file, source_paths)?;
            file.flush()
                .with_context(|| format!("failed to flush normalized JAR {}", output.display()))?;
            file.sync_all()
                .with_context(|| format!("failed to sync normalized JAR {}", output.display()))
        })();
        match result {
            Ok(()) => Ok(()),
            Err(error) => match fs::remove_file(output) {
                Ok(()) => Err(error),
                Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
                Err(cleanup) => Err(error.context(format!(
                    "also failed to remove incomplete normalized JAR {}: {cleanup}",
                    output.display()
                ))),
            },
        }
    }

    fn validate_source_filter(&self, source_paths: Option<&BTreeSet<String>>) -> Result<()> {
        let Some(source_paths) = source_paths else {
            return Ok(());
        };
        let missing = source_paths
            .difference(&self.expected_sources)
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!(
                "normalized JAR source filter contains paths absent from the input: {}",
                missing
                    .iter()
                    .map(|path| format!("{path:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        Ok(())
    }

    fn write_normalized_to<W: Write + Seek>(
        &self,
        output: W,
        source_paths: Option<&BTreeSet<String>>,
    ) -> Result<W> {
        self.validate_source_filter(source_paths)?;
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644);
        let mut archive = ZipWriter::new(output);
        for class in &self.classes {
            if source_paths.is_some_and(|filter| !filter.contains(&class.source_path)) {
                continue;
            }
            archive
                .start_file(
                    &class.logical_path,
                    options.large_file(class.bytes.len() > u32::MAX as usize),
                )
                .with_context(|| {
                    format!(
                        "failed to create normalized class entry {:?}",
                        class.logical_path
                    )
                })?;
            archive.write_all(&class.bytes).with_context(|| {
                format!(
                    "failed to write normalized class entry {:?}",
                    class.logical_path
                )
            })?;
        }
        archive.finish().context("failed to finish normalized JAR")
    }
}

pub fn analyze_jar(path: &Path, java_major: u32) -> Result<Analysis> {
    if java_major == 0 {
        bail!("Java major version must be positive");
    }
    let file =
        File::open(path).with_context(|| format!("failed to open JAR {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("invalid JAR {}", path.display()))?;
    analyze_archive(&mut archive, java_major, &path.display().to_string())
}

struct ArchiveClass {
    index: usize,
    archive_path: String,
}

struct Candidate {
    index: usize,
    archive_path: String,
    version: u32,
}

fn analyze_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    java_major: u32,
    label: &str,
) -> Result<Analysis> {
    if java_major == 0 {
        bail!("Java major version must be positive");
    }
    let mut manifest_index = None;
    let mut class_entries = Vec::new();
    let mut class_paths = BTreeSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect entry {index} in {label}"))?;
        let raw_name = entry.name_raw();
        if raw_name == MANIFEST_PATH {
            if manifest_index.replace(index).is_some() {
                bail!("JAR {label} contains multiple manifests");
            }
            require_regular_zip_file(&entry, "manifest", label)?;
        }
        if !raw_name.ends_with(b".class") {
            continue;
        }
        let archive_path = std::str::from_utf8(raw_name)
            .with_context(|| format!("class entry {index} in {label} has a non-UTF-8 path"))?;
        validate_archive_path(archive_path)
            .with_context(|| format!("unsafe class entry in {label}"))?;
        require_regular_zip_file(&entry, archive_path, label)?;
        if !class_paths.insert(archive_path.to_owned()) {
            bail!("JAR {label} repeats class entry {archive_path:?}");
        }
        class_entries.push(ArchiveClass {
            index,
            archive_path: archive_path.to_owned(),
        });
    }

    let multi_release = if let Some(index) = manifest_index {
        let mut manifest = archive
            .by_index(index)
            .with_context(|| format!("failed to open manifest in {label}"))?;
        let mut bytes = Vec::new();
        manifest
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read manifest in {label}"))?;
        manifest_is_multi_release(&bytes).with_context(|| format!("invalid manifest in {label}"))?
    } else {
        false
    };

    let mut candidates = BTreeMap::<String, Candidate>::new();
    for entry in class_entries {
        let Some((logical_path, version)) =
            runtime_class_path(&entry.archive_path, multi_release, java_major)?
        else {
            continue;
        };
        let candidate = Candidate {
            index: entry.index,
            archive_path: entry.archive_path,
            version,
        };
        match candidates.get(&logical_path) {
            Some(previous) if previous.version == version => {
                bail!("JAR {label} has multiple version {version} definitions for {logical_path:?}")
            }
            Some(previous) if previous.version > version => {}
            _ => {
                candidates.insert(logical_path, candidate);
            }
        }
    }

    let mut classes = Vec::with_capacity(candidates.len());
    let mut expected_sources = BTreeSet::new();
    let mut runtime_overridden_sources = BTreeSet::new();
    let mut source_to_decompiled = BTreeMap::new();
    for (logical_path, candidate) in candidates {
        let mut entry = archive.by_index(candidate.index).with_context(|| {
            format!(
                "failed to open selected class {:?} in {label}",
                candidate.archive_path
            )
        })?;
        if entry.name_raw() != candidate.archive_path.as_bytes() || !entry.is_file() {
            bail!(
                "selected class entry changed while reading {:?} in {label}",
                candidate.archive_path
            );
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).with_context(|| {
            format!(
                "failed to read selected class {:?} in {label}",
                candidate.archive_path
            )
        })?;
        let parsed = parse_class(&bytes, &candidate.archive_path)
            .with_context(|| format!("invalid class in JAR {label}"))?;
        let expected_internal = logical_path
            .strip_suffix(".class")
            .context("internal error: selected class lacks .class suffix")?;
        if parsed.internal_name != expected_internal {
            bail!(
                "class entry {:?} in {label} declares internal name {:?}, expected {:?}",
                candidate.archive_path,
                parsed.internal_name,
                expected_internal
            );
        }
        let source_file = match parsed.source_file {
            Some(source_file) => {
                validate_source_file(&source_file).with_context(|| {
                    format!(
                        "invalid SourceFile in class {:?} from {label}",
                        candidate.archive_path
                    )
                })?;
                source_file
            }
            None => fallback_source_file(&parsed.internal_name)?,
        };
        let package = parsed
            .internal_name
            .rsplit_once('/')
            .map(|(package, _)| package);
        let source_path = match package {
            Some(package) => format!("{package}/{source_file}"),
            None => source_file,
        };
        let extension = if source_path.ends_with(".kt") {
            "kt"
        } else {
            "java"
        };
        let decompiled_path = decompiled_class_path(&parsed.internal_name, extension)?;
        expected_sources.insert(source_path.clone());
        if candidate.version != 0 {
            runtime_overridden_sources.insert(source_path.clone());
        }
        source_to_decompiled
            .entry(source_path.clone())
            .or_insert_with(BTreeSet::new)
            .insert(decompiled_path);
        classes.push(SelectedClass {
            logical_path,
            source_path,
            bytes,
        });
    }

    Ok(Analysis {
        classes,
        expected_sources,
        runtime_overridden_sources,
        source_to_decompiled,
    })
}

fn require_regular_zip_file<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    name: &str,
    label: &str,
) -> Result<()> {
    if entry.is_symlink() || !entry.is_file() {
        bail!("JAR {label} contains non-regular file entry {name:?}");
    }
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        if file_type != 0 && file_type != 0o100000 {
            bail!("JAR {label} contains special entry {name:?} with Unix mode {mode:#o}");
        }
    }
    Ok(())
}

fn runtime_class_path(
    archive_path: &str,
    multi_release: bool,
    java_major: u32,
) -> Result<Option<(String, u32)>> {
    let (logical_path, version) = if let Some(rest) = archive_path.strip_prefix(VERSIONED_PREFIX) {
        if !multi_release {
            return Ok(None);
        }
        let Some((raw_version, logical_path)) = rest.split_once('/') else {
            return Ok(None);
        };
        if raw_version.is_empty()
            || !raw_version.bytes().all(|byte| byte.is_ascii_digit())
            || raw_version.starts_with('0')
        {
            return Ok(None);
        }
        let version = raw_version
            .parse::<u32>()
            .with_context(|| format!("invalid multi-release version in {archive_path:?}"))?;
        if version < 9 || version > java_major {
            return Ok(None);
        }
        (logical_path, version)
    } else {
        if archive_path.starts_with("META-INF/") {
            return Ok(None);
        }
        (archive_path, 0)
    };
    validate_archive_path(logical_path)?;
    if logical_path.starts_with("META-INF/")
        || logical_path
            .rsplit('/')
            .next()
            .is_some_and(|name| name == "module-info.class")
    {
        return Ok(None);
    }
    Ok(Some((logical_path.to_owned(), version)))
}

fn validate_archive_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains('\0')
        || path
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        bail!("unsafe archive path {path:?}");
    }
    Ok(())
}

fn validate_source_file(source_file: &str) -> Result<()> {
    if source_file.is_empty()
        || matches!(source_file, "." | "..")
        || source_file.contains(['/', '\\', '\0'])
    {
        bail!("SourceFile is not an unqualified file name: {source_file:?}");
    }
    Ok(())
}

fn fallback_source_file(internal_name: &str) -> Result<String> {
    let simple_name = internal_name.rsplit('/').next().unwrap_or(internal_name);
    let outer_name = simple_name.split('$').next().unwrap_or(simple_name);
    if outer_name.is_empty() {
        bail!("cannot derive an outer source name from internal class name {internal_name:?}");
    }
    Ok(format!("{outer_name}.java"))
}

fn decompiled_class_path(internal_name: &str, extension: &str) -> Result<String> {
    let (package, simple_name) = internal_name
        .rsplit_once('/')
        .map_or((None, internal_name), |(package, simple)| {
            (Some(package), simple)
        });
    let outer_name = simple_name.split('$').next().unwrap_or(simple_name);
    if outer_name.is_empty() {
        bail!("cannot derive a decompiled path from internal class name {internal_name:?}");
    }
    let file_name = format!("{outer_name}.{extension}");
    Ok(package.map_or(file_name.clone(), |package| {
        format!("{package}/{file_name}")
    }))
}

fn manifest_is_multi_release(bytes: &[u8]) -> Result<bool> {
    let mut attributes = BTreeMap::<String, Vec<u8>>::new();
    let mut current: Option<(String, Vec<u8>)> = None;
    for raw_line in bytes.split(|byte| *byte == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        if line.is_empty() {
            finish_manifest_attribute(&mut attributes, current.take())?;
            break;
        }
        if let Some(continuation) = line.strip_prefix(b" ") {
            let (_, value) = current
                .as_mut()
                .context("manifest continuation has no preceding main attribute")?;
            value.extend_from_slice(continuation);
            continue;
        }
        finish_manifest_attribute(&mut attributes, current.take())?;
        let Some(separator) = line.windows(2).position(|window| window == b": ") else {
            bail!("malformed main manifest attribute");
        };
        let name = &line[..separator];
        if name.is_empty()
            || !name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            bail!("invalid main manifest attribute name");
        }
        let name = String::from_utf8(name.to_ascii_lowercase())
            .context("manifest attribute name is not ASCII")?;
        current = Some((name, line[separator + 2..].to_vec()));
    }
    if current.is_some() {
        finish_manifest_attribute(&mut attributes, current.take())?;
    }
    Ok(attributes
        .get("multi-release")
        .is_some_and(|value| value.eq_ignore_ascii_case(b"true")))
}

fn finish_manifest_attribute(
    attributes: &mut BTreeMap<String, Vec<u8>>,
    attribute: Option<(String, Vec<u8>)>,
) -> Result<()> {
    let Some((name, value)) = attribute else {
        return Ok(());
    };
    if attributes.insert(name.clone(), value).is_some() {
        bail!("main manifest repeats attribute {name:?}");
    }
    Ok(())
}

struct ParsedClass {
    internal_name: String,
    source_file: Option<String>,
}

#[derive(Clone)]
enum ConstantPoolEntry {
    Unusable,
    Utf8(Vec<u8>),
    Class(u16),
    Other,
}

struct ClassReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    label: &'a str,
}

impl<'a> ClassReader<'a> {
    fn new(bytes: &'a [u8], label: &'a str) -> Self {
        Self {
            bytes,
            offset: 0,
            label,
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .with_context(|| format!("offset overflow in {}", self.label))?;
        if end > self.bytes.len() {
            bail!(
                "truncated class {} at byte {} while reading {length} bytes",
                self.label,
                self.offset
            );
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn skip_u32_length(&mut self) -> Result<()> {
        let length = usize::try_from(self.u32()?).context("class attribute is too large")?;
        self.take(length)?;
        Ok(())
    }
}

fn parse_class(bytes: &[u8], label: &str) -> Result<ParsedClass> {
    let mut reader = ClassReader::new(bytes, label);
    if reader.u32()? != 0xcafebabe {
        bail!("class {label} has an invalid magic value");
    }
    reader.u16()?;
    let major = reader.u16()?;
    if major < 45 {
        bail!("class {label} has unsupported classfile major version {major}");
    }
    let constant_pool_count = usize::from(reader.u16()?);
    if constant_pool_count == 0 {
        bail!("class {label} has a zero constant-pool count");
    }
    let mut pool = vec![ConstantPoolEntry::Unusable; constant_pool_count];
    let mut index = 1;
    while index < constant_pool_count {
        let tag = reader.u8()?;
        pool[index] = match tag {
            1 => {
                let length = usize::from(reader.u16()?);
                ConstantPoolEntry::Utf8(reader.take(length)?.to_vec())
            }
            3 | 4 => {
                reader.take(4)?;
                ConstantPoolEntry::Other
            }
            5 | 6 => {
                reader.take(8)?;
                if index + 1 >= constant_pool_count {
                    bail!("class {label} has a long or double in the final constant-pool slot");
                }
                index += 1;
                ConstantPoolEntry::Other
            }
            7 => ConstantPoolEntry::Class(reader.u16()?),
            8 | 16 | 19 | 20 => {
                reader.take(2)?;
                ConstantPoolEntry::Other
            }
            9 | 10 | 11 | 12 | 17 | 18 => {
                reader.take(4)?;
                ConstantPoolEntry::Other
            }
            15 => {
                reader.take(3)?;
                ConstantPoolEntry::Other
            }
            _ => bail!("class {label} has unknown constant-pool tag {tag} at index {index}"),
        };
        index += 1;
    }

    reader.u16()?;
    let this_class = reader.u16()?;
    let super_class = reader.u16()?;
    let internal_name = class_name(&pool, this_class, label)?;
    if super_class != 0 {
        require_class_entry(&pool, super_class, label)?;
    }
    let interface_count = usize::from(reader.u16()?);
    for _ in 0..interface_count {
        require_class_entry(&pool, reader.u16()?, label)?;
    }
    skip_members(&mut reader, &pool)?;
    skip_members(&mut reader, &pool)?;

    let attribute_count = usize::from(reader.u16()?);
    let mut source_file = None;
    for _ in 0..attribute_count {
        let name_index = reader.u16()?;
        let name = modified_utf8(constant_utf8(&pool, name_index, label)?)
            .with_context(|| format!("invalid class attribute name in {label}"))?;
        let length = usize::try_from(reader.u32()?).context("class attribute is too large")?;
        let body = reader.take(length)?;
        if name == "SourceFile" {
            if source_file.is_some() {
                bail!("class {label} repeats its SourceFile attribute");
            }
            if body.len() != 2 {
                bail!(
                    "class {label} has a SourceFile attribute of {} bytes, expected 2",
                    body.len()
                );
            }
            let source_index = u16::from_be_bytes(body.try_into().unwrap());
            source_file = Some(
                modified_utf8(constant_utf8(&pool, source_index, label)?)
                    .with_context(|| format!("invalid SourceFile value in {label}"))?,
            );
        }
    }
    if reader.offset != bytes.len() {
        bail!(
            "class {label} has {} trailing bytes",
            bytes.len() - reader.offset
        );
    }
    Ok(ParsedClass {
        internal_name,
        source_file,
    })
}

fn skip_members(reader: &mut ClassReader<'_>, pool: &[ConstantPoolEntry]) -> Result<()> {
    let count = usize::from(reader.u16()?);
    for _ in 0..count {
        reader.u16()?;
        let name_index = reader.u16()?;
        let descriptor_index = reader.u16()?;
        constant_utf8(pool, name_index, reader.label)?;
        constant_utf8(pool, descriptor_index, reader.label)?;
        let attribute_count = usize::from(reader.u16()?);
        for _ in 0..attribute_count {
            let attribute_name = reader.u16()?;
            modified_utf8(constant_utf8(pool, attribute_name, reader.label)?)
                .with_context(|| format!("invalid member attribute name in {}", reader.label))?;
            reader.skip_u32_length()?;
        }
    }
    Ok(())
}

fn constant_utf8<'a>(pool: &'a [ConstantPoolEntry], index: u16, label: &str) -> Result<&'a [u8]> {
    match pool.get(usize::from(index)) {
        Some(ConstantPoolEntry::Utf8(value)) => Ok(value),
        Some(_) => bail!("class {label} constant-pool index {index} is not UTF-8"),
        None => bail!("class {label} has out-of-range constant-pool index {index}"),
    }
}

fn require_class_entry(pool: &[ConstantPoolEntry], index: u16, label: &str) -> Result<u16> {
    match pool.get(usize::from(index)) {
        Some(ConstantPoolEntry::Class(name_index)) => {
            constant_utf8(pool, *name_index, label)?;
            Ok(*name_index)
        }
        Some(_) => bail!("class {label} constant-pool index {index} is not a class"),
        None => bail!("class {label} has out-of-range constant-pool index {index}"),
    }
}

fn class_name(pool: &[ConstantPoolEntry], index: u16, label: &str) -> Result<String> {
    let name_index = require_class_entry(pool, index, label)?;
    let name = modified_utf8(constant_utf8(pool, name_index, label)?)
        .with_context(|| format!("invalid internal class name in {label}"))?;
    validate_internal_name(&name).with_context(|| format!("invalid internal name in {label}"))?;
    Ok(name)
}

fn validate_internal_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('/')
        || name.ends_with('/')
        || name.contains(['.', ';', '[', '\\', '\0'])
        || name.split('/').any(str::is_empty)
    {
        bail!("invalid internal class name {name:?}");
    }
    Ok(())
}

fn modified_utf8(bytes: &[u8]) -> Result<String> {
    let mut units = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        match first {
            0x01..=0x7f => {
                units.push(u16::from(first));
                index += 1;
            }
            0xc0..=0xdf => {
                let second = *bytes
                    .get(index + 1)
                    .context("truncated two-byte modified UTF-8 sequence")?;
                if second & 0xc0 != 0x80 {
                    bail!("invalid modified UTF-8 continuation byte");
                }
                let value = (u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f);
                if value == 0 {
                    if first != 0xc0 || second != 0x80 {
                        bail!("invalid modified UTF-8 null encoding");
                    }
                } else if value < 0x80 {
                    bail!("overlong modified UTF-8 sequence");
                }
                units.push(value);
                index += 2;
            }
            0xe0..=0xef => {
                let second = *bytes
                    .get(index + 1)
                    .context("truncated three-byte modified UTF-8 sequence")?;
                let third = *bytes
                    .get(index + 2)
                    .context("truncated three-byte modified UTF-8 sequence")?;
                if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 {
                    bail!("invalid modified UTF-8 continuation byte");
                }
                let value = (u16::from(first & 0x0f) << 12)
                    | (u16::from(second & 0x3f) << 6)
                    | u16::from(third & 0x3f);
                if value < 0x800 {
                    bail!("overlong modified UTF-8 sequence");
                }
                units.push(value);
                index += 3;
            }
            _ => bail!("invalid modified UTF-8 leading byte {first:#04x}"),
        }
    }
    String::from_utf16(&units).context("modified UTF-8 contains an unpaired surrogate")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn folded_multi_release_is_read_only_from_main_attributes() {
        let manifest = b"Manifest-Version: 1.0\r\nMulti-Release: tr\r\n ue\r\n\r\nName: ignored\r\nMulti-Release: false\r\n";
        assert!(manifest_is_multi_release(manifest).unwrap());
        assert!(manifest_is_multi_release(b"multi-release: TRUE\n\n").unwrap());
        assert!(!manifest_is_multi_release(b"Manifest-Version: 1.0\n\n").unwrap());
        assert!(manifest_is_multi_release(b" continuation\n\n").is_err());
        assert!(
            manifest_is_multi_release(b"Multi-Release: true\nMulti-Release: false\n\n").is_err()
        );
    }

    #[test]
    fn modified_utf8_supports_null_and_surrogate_pairs() {
        assert_eq!(modified_utf8(b"a\xc0\x80b").unwrap(), "a\0b");
        assert_eq!(
            modified_utf8(b"\xed\xa0\xbd\xed\xb8\x80").unwrap(),
            "\u{1f600}"
        );
        assert!(modified_utf8(b"\0").is_err());
        assert!(modified_utf8(b"\xf0\x9f\x98\x80").is_err());
        assert!(modified_utf8(b"\xed\xa0\x80").is_err());
    }

    #[test]
    fn class_parser_reads_source_file_and_rejects_duplicates() {
        let parsed = parse_class(
            &class_bytes("example/Thing", Some("Thing.kt"), false),
            "test",
        )
        .unwrap();
        assert_eq!(parsed.internal_name, "example/Thing");
        assert_eq!(parsed.source_file.as_deref(), Some("Thing.kt"));
        assert!(
            parse_class(
                &class_bytes("example/Thing", Some("Thing.java"), true),
                "test"
            )
            .is_err()
        );
        let mut truncated = class_bytes("example/Thing", Some("Thing.java"), false);
        truncated.pop();
        assert!(parse_class(&truncated, "test").is_err());
    }

    #[test]
    fn analysis_selects_runtime_version_and_derives_sources() {
        let jar = jar_bytes(vec![
            (
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\nMulti-Release: tr\n ue\n\n".to_vec(),
            ),
            (
                "example/Choice.class",
                class_bytes("example/Choice", Some("Base.java"), false),
            ),
            (
                "META-INF/versions/9/example/Choice.class",
                class_bytes("example/Choice", Some("Nine.java"), false),
            ),
            (
                "META-INF/versions/17/example/Choice.class",
                class_bytes("example/Choice", Some("Seventeen.kt"), false),
            ),
            (
                "META-INF/versions/21/example/Choice.class",
                class_bytes("example/Choice", Some("TwentyOne.java"), false),
            ),
            (
                "example/Outer$Inner.class",
                class_bytes("example/Outer$Inner", None, false),
            ),
            (
                "META-INF/Ignored.class",
                class_bytes("META-INF/Ignored", Some("Ignored.java"), false),
            ),
            (
                "module-info.class",
                class_bytes("module-info", Some("module-info.java"), false),
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(jar)).unwrap();
        let analysis = analyze_archive(&mut archive, 17, "test JAR").unwrap();
        assert_eq!(analysis.classes.len(), 2);
        assert_eq!(analysis.classes[0].logical_path, "example/Choice.class");
        assert_eq!(
            analysis.expected_sources,
            BTreeSet::from([
                "example/Outer.java".to_owned(),
                "example/Seventeen.kt".to_owned(),
            ])
        );
        assert_eq!(
            analysis.expected_decompiled_paths(None).unwrap(),
            BTreeSet::from([
                "example/Choice.kt".to_owned(),
                "example/Outer.java".to_owned(),
            ])
        );
        assert_eq!(
            analysis.runtime_overridden_sources(),
            &BTreeSet::from(["example/Seventeen.kt".to_owned()])
        );
    }

    #[test]
    fn non_multi_release_jar_ignores_versioned_classes() {
        let jar = jar_bytes(vec![
            (
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\n\n".to_vec(),
            ),
            (
                "example/Base.class",
                class_bytes("example/Base", Some("Base.java"), false),
            ),
            (
                "META-INF/versions/17/example/Only.class",
                class_bytes("example/Only", Some("Only.java"), false),
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(jar)).unwrap();
        let analysis = analyze_archive(&mut archive, 17, "test JAR").unwrap();
        assert_eq!(analysis.classes.len(), 1);
        assert_eq!(analysis.classes[0].logical_path, "example/Base.class");
    }

    #[test]
    fn analysis_rejects_logical_and_internal_name_mismatch() {
        let jar = jar_bytes(vec![(
            "example/Wrong.class",
            class_bytes("example/Right", Some("Right.java"), false),
        )]);
        let mut archive = ZipArchive::new(Cursor::new(jar)).unwrap();
        assert!(analyze_archive(&mut archive, 17, "test JAR").is_err());
    }

    #[test]
    fn normalized_jar_is_deterministic_and_filters_whole_sources() {
        let jar = jar_bytes(vec![
            (
                "example/B$Inner.class",
                class_bytes("example/B$Inner", Some("B.kt"), false),
            ),
            (
                "example/A.class",
                class_bytes("example/A", Some("A.java"), false),
            ),
            (
                "example/B.class",
                class_bytes("example/B", Some("B.kt"), false),
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(jar)).unwrap();
        let analysis = analyze_archive(&mut archive, 17, "test JAR").unwrap();
        let filter = BTreeSet::from(["example/B.kt".to_owned()]);
        assert_eq!(
            analysis.expected_decompiled_paths(Some(&filter)).unwrap(),
            BTreeSet::from(["example/B.kt".to_owned()])
        );

        let first = analysis
            .write_normalized_to(Cursor::new(Vec::new()), Some(&filter))
            .unwrap()
            .into_inner();
        let second = analysis
            .write_normalized_to(Cursor::new(Vec::new()), Some(&filter))
            .unwrap()
            .into_inner();
        assert_eq!(first, second);
        let mut normalized = ZipArchive::new(Cursor::new(first)).unwrap();
        let names = (0..normalized.len())
            .map(|index| normalized.by_index(index).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, ["example/B$Inner.class", "example/B.class"]);
    }

    #[test]
    fn decompiled_paths_use_logical_java_classes_and_kotlin_source_files() {
        let jar = jar_bytes(vec![
            (
                "example/JavaType.class",
                class_bytes("example/JavaType", Some("Shared.java"), false),
            ),
            (
                "example/KotlinType.class",
                class_bytes("example/KotlinType", Some("Shared.kt"), false),
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(jar)).unwrap();
        let analysis = analyze_archive(&mut archive, 17, "test JAR").unwrap();
        assert_eq!(analysis.expected_sources.len(), 2);
        assert_eq!(
            analysis.expected_decompiled_paths(None).unwrap(),
            BTreeSet::from([
                "example/JavaType.java".to_owned(),
                "example/KotlinType.kt".to_owned(),
            ])
        );
    }

    fn class_bytes(internal_name: &str, source_file: Option<&str>, duplicate: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_u32(&mut bytes, 0xcafebabe);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 61);
        push_u16(&mut bytes, if source_file.is_some() { 7 } else { 5 });
        push_utf8(&mut bytes, internal_name.as_bytes());
        bytes.push(7);
        push_u16(&mut bytes, 1);
        push_utf8(&mut bytes, b"java/lang/Object");
        bytes.push(7);
        push_u16(&mut bytes, 3);
        if let Some(source_file) = source_file {
            push_utf8(&mut bytes, b"SourceFile");
            push_utf8(&mut bytes, source_file.as_bytes());
        }
        push_u16(&mut bytes, 0x0021);
        push_u16(&mut bytes, 2);
        push_u16(&mut bytes, 4);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        push_u16(&mut bytes, 0);
        let attributes = if source_file.is_some() {
            if duplicate { 2 } else { 1 }
        } else {
            0
        };
        push_u16(&mut bytes, attributes);
        for _ in 0..attributes {
            push_u16(&mut bytes, 5);
            push_u32(&mut bytes, 2);
            push_u16(&mut bytes, 6);
        }
        bytes
    }

    fn push_utf8(bytes: &mut Vec<u8>, value: &[u8]) {
        bytes.push(1);
        push_u16(bytes, value.len().try_into().unwrap());
        bytes.extend_from_slice(value);
    }

    fn push_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn jar_bytes(entries: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(DateTime::default())
            .unix_permissions(0o644);
        for (name, contents) in entries {
            archive.start_file(name, options).unwrap();
            archive.write_all(&contents).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }
}
