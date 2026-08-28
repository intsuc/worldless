use std::{
    collections::HashSet,
    error::Error,
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use flate2::read::MultiGzDecoder;

use crate::{
    nbt::{CompoundTag, JavaString, Tag, parse_binary_compound},
    resource::Identifier,
};

const COMMAND_STORAGE_DATA_VERSION: i32 = 5015;

/// An error produced while loading Minecraft command-storage files.
#[derive(Debug)]
pub enum CommandStorageLoadError {
    /// An explicit namespace is not a valid non-empty Minecraft namespace.
    InvalidNamespace { namespace: String },
    /// More than one input file was assigned to the same namespace.
    DuplicateNamespace { namespace: String },
    /// The file could not be read.
    Io {
        namespace: String,
        path: PathBuf,
        source: io::Error,
    },
    /// A file with the gzip signature could not be decompressed.
    InvalidGzip {
        namespace: String,
        path: PathBuf,
        source: io::Error,
    },
    /// The decompressed bytes are not valid binary NBT.
    InvalidNbt {
        namespace: String,
        path: PathBuf,
        reason: String,
    },
    /// The root compound does not contain `DataVersion`.
    MissingDataVersion { namespace: String, path: PathBuf },
    /// `DataVersion` is present but is not an integer tag.
    InvalidDataVersionType { namespace: String, path: PathBuf },
    /// The file belongs to a Minecraft data version other than the target.
    UnsupportedDataVersion {
        namespace: String,
        path: PathBuf,
        found: i32,
    },
    /// The NBT envelope or one of its storage entries has an invalid shape.
    InvalidSchema {
        namespace: String,
        path: PathBuf,
        reason: String,
    },
}

impl CommandStorageLoadError {
    /// Returns the namespace associated with the failed input.
    pub fn namespace(&self) -> &str {
        match self {
            Self::InvalidNamespace { namespace }
            | Self::DuplicateNamespace { namespace }
            | Self::Io { namespace, .. }
            | Self::InvalidGzip { namespace, .. }
            | Self::InvalidNbt { namespace, .. }
            | Self::MissingDataVersion { namespace, .. }
            | Self::InvalidDataVersionType { namespace, .. }
            | Self::UnsupportedDataVersion { namespace, .. }
            | Self::InvalidSchema { namespace, .. } => namespace,
        }
    }

    /// Returns the failed file path, or `None` when validation stopped before I/O.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::InvalidNamespace { .. } | Self::DuplicateNamespace { .. } => None,
            Self::Io { path, .. }
            | Self::InvalidGzip { path, .. }
            | Self::InvalidNbt { path, .. }
            | Self::MissingDataVersion { path, .. }
            | Self::InvalidDataVersionType { path, .. }
            | Self::UnsupportedDataVersion { path, .. }
            | Self::InvalidSchema { path, .. } => Some(path),
        }
    }
}

impl fmt::Display for CommandStorageLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNamespace { namespace } => {
                write!(formatter, "invalid command-storage namespace {namespace:?}")
            }
            Self::DuplicateNamespace { namespace } => {
                write!(
                    formatter,
                    "duplicate command-storage namespace {namespace:?}"
                )
            }
            Self::Io {
                namespace,
                path,
                source,
            } => write!(
                formatter,
                "failed to read command-storage file {} for namespace {namespace:?}: {source}",
                path.display()
            ),
            Self::InvalidGzip {
                namespace,
                path,
                source,
            } => write!(
                formatter,
                "invalid gzip command-storage file {} for namespace {namespace:?}: {source}",
                path.display()
            ),
            Self::InvalidNbt {
                namespace,
                path,
                reason,
            } => write!(
                formatter,
                "invalid binary NBT in command-storage file {} for namespace {namespace:?}: {reason}",
                path.display()
            ),
            Self::MissingDataVersion { namespace, path } => write!(
                formatter,
                "command-storage file {} for namespace {namespace:?} is missing DataVersion",
                path.display()
            ),
            Self::InvalidDataVersionType { namespace, path } => write!(
                formatter,
                "DataVersion in command-storage file {} for namespace {namespace:?} must be an int tag",
                path.display()
            ),
            Self::UnsupportedDataVersion {
                namespace,
                path,
                found,
            } => write!(
                formatter,
                "command-storage file {} for namespace {namespace:?} has DataVersion {found}, expected {COMMAND_STORAGE_DATA_VERSION}",
                path.display()
            ),
            Self::InvalidSchema {
                namespace,
                path,
                reason,
            } => write!(
                formatter,
                "invalid command-storage schema in {} for namespace {namespace:?}: {reason}",
                path.display()
            ),
        }
    }
}

impl Error for CommandStorageLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::InvalidGzip { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct LoadedCommandStorageNamespace {
    pub(crate) namespace: String,
    pub(crate) values: Vec<(Identifier, CompoundTag)>,
}

pub(crate) fn load<N, P>(
    files: impl IntoIterator<Item = (N, P)>,
) -> Result<Vec<LoadedCommandStorageNamespace>, CommandStorageLoadError>
where
    N: AsRef<str>,
    P: AsRef<Path>,
{
    let files = files
        .into_iter()
        .map(|(namespace, path)| (namespace.as_ref().to_owned(), path.as_ref().to_owned()))
        .collect::<Vec<_>>();
    validate_namespaces(&files)?;

    files
        .into_iter()
        .map(|(namespace, path)| load_one(namespace, path))
        .collect()
}

fn validate_namespaces(files: &[(String, PathBuf)]) -> Result<(), CommandStorageLoadError> {
    let mut seen = HashSet::new();
    for (namespace, _) in files {
        if namespace.is_empty() || Identifier::from_parts(namespace, "").is_none() {
            return Err(CommandStorageLoadError::InvalidNamespace {
                namespace: namespace.clone(),
            });
        }
        if !seen.insert(namespace.as_str()) {
            return Err(CommandStorageLoadError::DuplicateNamespace {
                namespace: namespace.clone(),
            });
        }
    }
    Ok(())
}

fn load_one(
    namespace: String,
    path: PathBuf,
) -> Result<LoadedCommandStorageNamespace, CommandStorageLoadError> {
    let encoded = fs::read(&path).map_err(|source| CommandStorageLoadError::Io {
        namespace: namespace.clone(),
        path: path.clone(),
        source,
    })?;
    let decoded = if encoded.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = MultiGzDecoder::new(encoded.as_slice());
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).map_err(|source| {
            CommandStorageLoadError::InvalidGzip {
                namespace: namespace.clone(),
                path: path.clone(),
                source,
            }
        })?;
        decoded
    } else {
        encoded
    };
    let root =
        parse_binary_compound(&decoded).map_err(|error| CommandStorageLoadError::InvalidNbt {
            namespace: namespace.clone(),
            path: path.clone(),
            reason: error.reason().to_owned(),
        })?;
    let values = parse_envelope(&namespace, &path, &root)?;
    Ok(LoadedCommandStorageNamespace { namespace, values })
}

fn parse_envelope(
    namespace: &str,
    path: &Path,
    root: &CompoundTag,
) -> Result<Vec<(Identifier, CompoundTag)>, CommandStorageLoadError> {
    let data_version_name = JavaString::from("DataVersion");
    let Some(data_version) = root.get(&data_version_name) else {
        return Err(CommandStorageLoadError::MissingDataVersion {
            namespace: namespace.to_owned(),
            path: path.to_owned(),
        });
    };
    let Tag::Int(data_version) = data_version else {
        return Err(CommandStorageLoadError::InvalidDataVersionType {
            namespace: namespace.to_owned(),
            path: path.to_owned(),
        });
    };
    if *data_version != COMMAND_STORAGE_DATA_VERSION {
        return Err(CommandStorageLoadError::UnsupportedDataVersion {
            namespace: namespace.to_owned(),
            path: path.to_owned(),
            found: *data_version,
        });
    }

    let data_name = JavaString::from("data");
    let Some(Tag::Compound(data)) = root.get(&data_name) else {
        return invalid_schema(namespace, path, "root field `data` must be a compound");
    };
    if root.len() != 2 {
        return invalid_schema(
            namespace,
            path,
            "root must contain exactly `DataVersion` and `data`",
        );
    }
    let contents_name = JavaString::from("contents");
    let Some(Tag::Compound(contents)) = data.get(&contents_name) else {
        return invalid_schema(namespace, path, "field `data.contents` must be a compound");
    };
    if data.len() != 1 {
        return invalid_schema(namespace, path, "`data` must contain exactly `contents`");
    }

    let mut values = Vec::new();
    values.try_reserve_exact(contents.len()).map_err(|error| {
        CommandStorageLoadError::InvalidSchema {
            namespace: namespace.to_owned(),
            path: path.to_owned(),
            reason: format!(
                "cannot allocate space for {} storage entries: {error}",
                contents.len()
            ),
        }
    })?;
    for (raw_path, value) in contents.entries() {
        let Some(resource_path) = ascii_string(raw_path) else {
            return invalid_schema(namespace, path, "storage path is not valid ASCII");
        };
        let Some(id) = Identifier::from_parts(namespace, &resource_path) else {
            return invalid_schema(
                namespace,
                path,
                format!("invalid storage path {resource_path:?}"),
            );
        };
        let Tag::Compound(value) = value else {
            return invalid_schema(
                namespace,
                path,
                format!("storage {id} must contain a compound tag"),
            );
        };
        values.push((id, value.clone()));
    }
    Ok(values)
}

fn invalid_schema<T>(
    namespace: &str,
    path: &Path,
    reason: impl Into<String>,
) -> Result<T, CommandStorageLoadError> {
    Err(CommandStorageLoadError::InvalidSchema {
        namespace: namespace.to_owned(),
        path: path.to_owned(),
        reason: reason.into(),
    })
}

fn ascii_string(value: &JavaString) -> Option<String> {
    value
        .units()
        .iter()
        .map(|&unit| u8::try_from(unit).ok().filter(u8::is_ascii))
        .collect::<Option<Vec<_>>>()
        .map(|bytes| String::from_utf8(bytes).expect("ASCII is UTF-8"))
}

#[cfg(test)]
mod tests {
    use std::{
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };

    use flate2::{Compression, write::GzEncoder};

    use super::*;
    use crate::nbt::CommandStorage;

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    struct TestFile(PathBuf);

    impl TestFile {
        fn new(contents: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "worldless-command-storage-unit-{}-{}",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&path, contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_file(&self.0) {
                eprintln!("failed to remove {}: {error}", self.0.display());
            }
        }
    }

    fn push_string(output: &mut Vec<u8>, value: &[u8]) {
        output.extend_from_slice(&u16::try_from(value.len()).unwrap().to_be_bytes());
        output.extend_from_slice(value);
    }

    fn push_header(output: &mut Vec<u8>, tag_type: u8, name: &[u8]) {
        output.push(tag_type);
        push_string(output, name);
    }

    fn storage_file() -> Vec<u8> {
        let mut output = vec![10, 0, 0];
        push_header(&mut output, 10, b"data");
        push_header(&mut output, 10, b"contents");
        push_header(&mut output, 10, b"state");
        push_header(&mut output, 3, b"value");
        output.extend_from_slice(&7_i32.to_be_bytes());
        output.push(0);
        push_header(&mut output, 10, b"");
        output.push(0);
        output.push(0);
        output.push(0);
        push_header(&mut output, 3, b"DataVersion");
        output.extend_from_slice(&COMMAND_STORAGE_DATA_VERSION.to_be_bytes());
        output.push(0);
        output
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn loader_reads_single_and_multiple_gzip_members() {
        let raw = storage_file();
        let split = raw.len() / 2;
        let single = TestFile::new(&gzip(&raw));
        let mut multiple_bytes = gzip(&raw[..split]);
        multiple_bytes.extend_from_slice(&gzip(&raw[split..]));
        let multiple = TestFile::new(&multiple_bytes);

        for file in [&single, &multiple] {
            let loaded = load_one("probe".to_owned(), file.0.clone()).unwrap();
            assert_eq!(loaded.values.len(), 2);
            let (_, state) = loaded
                .values
                .iter()
                .find(|(id, _)| id.to_string() == "probe:state")
                .unwrap();
            assert_eq!(state, &CompoundTag::from_snbt("{value:7}").unwrap());
        }
    }

    #[test]
    fn gzip_signature_with_a_corrupt_stream_has_its_own_error() {
        let file = TestFile::new(&[0x1f, 0x8b, 0]);
        assert!(matches!(
            load_one("probe".to_owned(), file.0.clone()),
            Err(CommandStorageLoadError::InvalidGzip { .. })
        ));
    }

    fn envelope(version: Tag, path: JavaString, value: Tag) -> CompoundTag {
        let mut contents = CompoundTag::new();
        contents.insert(path, value);
        let mut data = CompoundTag::new();
        data.insert(JavaString::from("contents"), Tag::Compound(contents));
        let mut root = CompoundTag::new();
        root.insert(JavaString::from("DataVersion"), version);
        root.insert(JavaString::from("data"), Tag::Compound(data));
        root
    }

    #[test]
    fn envelope_distinguishes_version_schema_path_and_value_errors() {
        let path = Path::new("storage.dat");
        let valid = envelope(
            Tag::Int(COMMAND_STORAGE_DATA_VERSION),
            JavaString::from("state"),
            Tag::Compound(CompoundTag::new()),
        );
        assert!(parse_envelope("probe", path, &valid).is_ok());

        let mut missing_version = valid.clone();
        missing_version.remove(&JavaString::from("DataVersion"));
        assert!(matches!(
            parse_envelope("probe", path, &missing_version),
            Err(CommandStorageLoadError::MissingDataVersion { .. })
        ));

        let wrong_type = envelope(
            Tag::Long(i64::from(COMMAND_STORAGE_DATA_VERSION)),
            JavaString::from("state"),
            Tag::Compound(CompoundTag::new()),
        );
        assert!(matches!(
            parse_envelope("probe", path, &wrong_type),
            Err(CommandStorageLoadError::InvalidDataVersionType { .. })
        ));

        let wrong_version = envelope(
            Tag::Int(COMMAND_STORAGE_DATA_VERSION - 1),
            JavaString::from("state"),
            Tag::Compound(CompoundTag::new()),
        );
        assert!(matches!(
            parse_envelope("probe", path, &wrong_version),
            Err(CommandStorageLoadError::UnsupportedDataVersion { .. })
        ));

        let mut unknown_root_field = valid.clone();
        unknown_root_field.insert(JavaString::from("future"), Tag::Byte(1));
        assert!(matches!(
            parse_envelope("probe", path, &unknown_root_field),
            Err(CommandStorageLoadError::InvalidSchema { .. })
        ));

        let invalid_path = envelope(
            Tag::Int(COMMAND_STORAGE_DATA_VERSION),
            JavaString::from("Upper"),
            Tag::Compound(CompoundTag::new()),
        );
        assert!(matches!(
            parse_envelope("probe", path, &invalid_path),
            Err(CommandStorageLoadError::InvalidSchema { .. })
        ));

        let scalar_value = envelope(
            Tag::Int(COMMAND_STORAGE_DATA_VERSION),
            JavaString::from("state"),
            Tag::Int(1),
        );
        assert!(matches!(
            parse_envelope("probe", path, &scalar_value),
            Err(CommandStorageLoadError::InvalidSchema { .. })
        ));
    }

    #[test]
    fn namespace_validation_precedes_io_and_loaded_empty_compounds_are_absent() {
        let missing = Path::new("definitely-missing-command-storage-file");
        for namespace in ["", "Upper"] {
            let error = load([(namespace, missing)]).unwrap_err();
            assert!(matches!(
                error,
                CommandStorageLoadError::InvalidNamespace { .. }
            ));
            assert_eq!(error.path(), None);
        }
        let error = load([("probe", missing), ("probe", missing)]).unwrap_err();
        assert!(matches!(
            error,
            CommandStorageLoadError::DuplicateNamespace { .. }
        ));

        let raw = TestFile::new(&storage_file());
        let loaded = load_one("probe".to_owned(), raw.0.clone()).unwrap();
        let mut storage = CommandStorage::default();
        storage.replace_namespace(&loaded.namespace, loaded.values);
        assert!(
            storage
                .get_ref(&Identifier::from_parts("probe", "").unwrap())
                .is_none()
        );
        assert!(
            storage
                .get_ref(&Identifier::from_parts("probe", "state").unwrap())
                .is_some()
        );
    }
}
