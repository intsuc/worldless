use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::{
    artifacts,
    download::{Http, Integrity, file_sha256},
};

const LIBRARIES_LIST: &str = "META-INF/libraries.list";
const CENTRAL_REPOSITORY: Repository = Repository {
    cache_name: "central",
    url: "https://repo.maven.apache.org/maven2/",
};
const MOJANG_REPOSITORY: Repository = Repository {
    cache_name: "mojang",
    url: "https://libraries.minecraft.net/",
};

#[derive(Debug)]
pub struct Prepared {
    pub manifest_sha256: String,
    pub sources: Vec<Source>,
}

#[derive(Debug)]
pub struct Source {
    pub id: String,
    pub artifact_path: PathBuf,
    pub binaries: Vec<BinaryIdentity>,
    pub input: Input,
}

#[derive(Debug)]
pub enum Input {
    Published {
        jar: PathBuf,
        repository: String,
        url: String,
        checksum_algorithm: String,
        checksum: String,
        sha256: String,
    },
    Decompiled {
        jar: PathBuf,
    },
}

#[derive(Debug)]
pub struct BinaryIdentity {
    pub coordinate: String,
    pub sha256: String,
    pub jar: PathBuf,
}

pub fn prepare(
    http: &Http,
    server_bundle: &Path,
    minecraft_cache: &Path,
    cache_root: &Path,
) -> Result<Prepared> {
    let file = File::open(server_bundle)
        .with_context(|| format!("failed to open server jar {}", server_bundle.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("invalid server jar {}", server_bundle.display()))?;
    let manifest_index = unique_entry_index(&mut archive, LIBRARIES_LIST)?;
    let mut manifest_bytes = Vec::new();
    {
        let mut manifest = archive
            .by_index(manifest_index)
            .context("failed to open the server library manifest")?;
        if !manifest.is_file() {
            bail!("{LIBRARIES_LIST} is not a regular file");
        }
        manifest
            .read_to_end(&mut manifest_bytes)
            .context("failed to read the server library manifest")?;
    }
    let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));
    let manifest = std::str::from_utf8(&manifest_bytes)
        .with_context(|| format!("{LIBRARIES_LIST} is not valid UTF-8"))?;
    let entries = parse_manifest(manifest)?;
    eprintln!("Preparing {} Minecraft server libraries", entries.len());

    let nested_root = minecraft_cache.join("libraries");
    artifacts::ensure_directory(cache_root, &nested_root)?;
    let mut groups = BTreeMap::<String, LibraryGroup>::new();
    for entry in entries {
        let jar = extract_nested_jar(&mut archive, &entry, &nested_root, cache_root)?;
        let id = entry.coordinate.id();
        let group = groups.entry(id.clone()).or_insert_with(|| LibraryGroup {
            id,
            coordinate: entry.coordinate.clone(),
            binaries: Vec::new(),
        });
        group.binaries.push(PreparedBinary {
            identity: BinaryIdentity {
                coordinate: entry.coordinate.full.clone(),
                sha256: entry.sha256,
                jar,
            },
            coordinate: entry.coordinate,
            artifact_path: entry.artifact_path,
        });
    }

    let mut sources = Vec::with_capacity(groups.len());
    for (_, mut group) in groups {
        group.binaries.sort_unstable_by(|left, right| {
            left.identity.coordinate.cmp(&right.identity.coordinate)
        });
        sources.extend(prepare_sources(http, group, cache_root)?);
    }
    let published = sources
        .iter()
        .filter(|source| matches!(&source.input, Input::Published { .. }))
        .count();
    let decompiled = sources.len() - published;
    eprintln!(
        "Prepared {published} published library source archives; {decompiled} library jars require decompilation"
    );
    Ok(Prepared {
        manifest_sha256,
        sources,
    })
}

fn prepare_sources(http: &Http, group: LibraryGroup, cache_root: &Path) -> Result<Vec<Source>> {
    let repository = repository_for(&group.coordinate);
    let source_relative = group.coordinate.source_artifact_path();
    let source_artifact_path = PathBuf::from(group.coordinate.directory());
    if group.binaries.is_empty() {
        bail!("library source group has no binaries");
    }

    let mut binaries_match = true;
    for binary in &group.binaries {
        let remote_url = repository_url(repository, &binary.artifact_path)?;
        let Some(checksum) = published_checksum(http, &remote_url)? else {
            binaries_match = false;
            break;
        };
        let remote =
            repository_cache_path(cache_root, repository, Path::new(&binary.artifact_path))?;
        http.ensure_file(&remote_url, &remote, checksum.integrity())?;
        validate_jar(&remote)?;
        let remote_sha256 = file_sha256(&remote)?;
        if remote_sha256 != binary.identity.sha256 {
            binaries_match = false;
            break;
        }
    }

    if binaries_match {
        let source_url = repository_url(repository, &source_relative)?;
        if let Some(checksum) = published_checksum(http, &source_url)? {
            let jar = repository_cache_path(cache_root, repository, Path::new(&source_relative))?;
            http.ensure_file(&source_url, &jar, checksum.integrity())?;
            validate_jar(&jar)?;
            let sha256 = file_sha256(&jar)?;
            return Ok(vec![Source {
                id: group.id,
                artifact_path: source_artifact_path,
                binaries: group
                    .binaries
                    .into_iter()
                    .map(|binary| binary.identity)
                    .collect(),
                input: Input::Published {
                    jar,
                    repository: repository.url.to_owned(),
                    url: source_url,
                    checksum_algorithm: checksum.algorithm.name().to_owned(),
                    checksum: checksum.value,
                    sha256,
                },
            }]);
        }
    }

    Ok(group
        .binaries
        .into_iter()
        .map(|binary| {
            let mut artifact_path = PathBuf::from(binary.coordinate.directory());
            if let Some(classifier) = &binary.coordinate.classifier {
                artifact_path.push(classifier);
            }
            let jar = binary.identity.jar.clone();
            Source {
                id: binary.identity.coordinate.clone(),
                artifact_path,
                binaries: vec![binary.identity],
                input: Input::Decompiled { jar },
            }
        })
        .collect())
}

fn extract_nested_jar(
    archive: &mut ZipArchive<File>,
    entry: &ManifestEntry,
    nested_root: &Path,
    cache_root: &Path,
) -> Result<PathBuf> {
    let destination = nested_root.join(&entry.artifact_path);
    let parent = destination.parent().with_context(|| {
        format!(
            "nested library cache file has no parent: {}",
            destination.display()
        )
    })?;
    artifacts::ensure_directory(cache_root, parent)?;

    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                bail!(
                    "nested library cache is not a regular file: {}",
                    destination.display()
                );
            }
            verify_sha256(&destination, &entry.sha256).with_context(|| {
                format!(
                    "cached nested library failed integrity verification: {}; remove it before retrying",
                    destination.display()
                )
            })?;
            validate_jar(&destination)?;
            return Ok(destination);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect nested library cache {}",
                    destination.display()
                )
            });
        }
    }

    let archive_name = format!("META-INF/libraries/{}", entry.artifact_path);
    let archive_index = unique_entry_index(archive, &archive_name)?;
    let temporary = destination.with_file_name(format!(
        "{}.part",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("invalid library file name: {}", destination.display()))?
    ));
    match fs::symlink_metadata(&temporary) {
        Ok(_) => bail!(
            "incomplete or concurrent library extraction exists: {}; remove it after confirming no other worldless-dev process is running",
            temporary.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect temporary library extraction {}",
                    temporary.display()
                )
            });
        }
    }

    let temporary_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("failed to claim library extraction {}", temporary.display()))?;
    let extraction = (|| -> Result<()> {
        let mut nested = archive
            .by_index(archive_index)
            .with_context(|| format!("failed to open declared library {archive_name:?}"))?;
        if !nested.is_file() {
            bail!("declared library is not a regular file: {archive_name:?}");
        }
        let mut output = temporary_file;
        std::io::copy(&mut nested, &mut output)
            .with_context(|| format!("failed to extract {archive_name:?}"))?;
        output
            .flush()
            .with_context(|| format!("failed to flush {}", temporary.display()))?;
        drop(output);
        verify_sha256(&temporary, &entry.sha256)?;
        validate_jar(&temporary)
    })();
    if let Err(error) = extraction {
        return match fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => Err(error.context(format!(
                "also failed to remove incomplete library extraction {}: {cleanup}",
                temporary.display()
            ))),
        };
    }
    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "failed to move extracted library to {}",
            destination.display()
        )
    })?;
    Ok(destination)
}

fn repository_cache_path(
    cache_root: &Path,
    repository: Repository,
    artifact_path: &Path,
) -> Result<PathBuf> {
    let destination = cache_root
        .join("maven")
        .join(repository.cache_name)
        .join(artifact_path);
    let parent = destination.parent().with_context(|| {
        format!(
            "repository cache file has no parent: {}",
            destination.display()
        )
    })?;
    artifacts::ensure_directory(cache_root, parent)?;
    Ok(destination)
}

fn repository_url(repository: Repository, artifact_path: &str) -> Result<String> {
    if artifact_path.is_empty()
        || artifact_path.starts_with('/')
        || artifact_path.contains('\\')
        || artifact_path
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        bail!("invalid Maven artifact path: {artifact_path:?}");
    }
    Ok(format!("{}{artifact_path}", repository.url))
}

fn published_checksum(http: &Http, artifact_url: &str) -> Result<Option<PublishedChecksum>> {
    for algorithm in [ChecksumAlgorithm::Sha256, ChecksumAlgorithm::Sha1] {
        let checksum_url = format!("{artifact_url}.{}", algorithm.name());
        let Some(value) = http.get_optional_text(&checksum_url)? else {
            continue;
        };
        let value = value.trim();
        validate_checksum(algorithm, value)
            .with_context(|| format!("invalid published checksum from {checksum_url}"))?;
        return Ok(Some(PublishedChecksum {
            algorithm,
            value: value.to_ascii_lowercase(),
        }));
    }
    Ok(None)
}

fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let actual = file_sha256(path)?;
    if actual != expected {
        bail!(
            "SHA-256 mismatch for {}: expected {expected}, got {actual}",
            path.display()
        );
    }
    Ok(())
}

fn validate_jar(path: &Path) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("invalid JAR {}", path.display()))?;
    let mut names = HashSet::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect entry {index} in {}", path.display()))?;
        validate_zip_entry(entry.name())
            .with_context(|| format!("unsafe entry in {}", path.display()))?;
        if !names.insert(entry.name().to_owned()) {
            bail!("duplicate entry {:?} in {}", entry.name(), path.display());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!(
                "symbolic link entry {:?} in {}",
                entry.name(),
                path.display()
            );
        }
    }
    Ok(())
}

fn validate_zip_entry(name: &str) -> Result<()> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains('\0') {
        bail!("invalid ZIP entry path {name:?}");
    }
    let path = name.strip_suffix('/').unwrap_or(name);
    if path.is_empty() {
        bail!("invalid ZIP entry path {name:?}");
    }
    for component in path.split('/') {
        validate_coordinate_component("ZIP entry component", component)
            .with_context(|| format!("invalid ZIP entry path {name:?}"))?;
    }
    Ok(())
}

fn unique_entry_index(archive: &mut ZipArchive<File>, name: &str) -> Result<usize> {
    let mut matching = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect server jar entry {index}"))?;
        if entry.name() == name {
            matching.push(index);
        }
    }
    match matching.as_slice() {
        [index] => Ok(*index),
        [] => bail!("server bundler is missing declared entry {name:?}"),
        _ => bail!("server bundler contains multiple entries named {name:?}"),
    }
}

fn parse_manifest(manifest: &str) -> Result<Vec<ManifestEntry>> {
    if manifest.is_empty() {
        bail!("{LIBRARIES_LIST} is empty");
    }
    let mut entries = Vec::new();
    let mut coordinates = HashSet::new();
    let mut artifact_paths = HashSet::new();
    for (line_index, raw_line) in manifest.split_terminator('\n').enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            bail!(
                "invalid {LIBRARIES_LIST} line {}: empty line",
                line_index + 1
            );
        }
        let columns = line.split('\t').collect::<Vec<_>>();
        let [sha256, coordinate, artifact_path] = columns.as_slice() else {
            bail!(
                "invalid {LIBRARIES_LIST} line {}: expected three tab-separated fields",
                line_index + 1
            );
        };
        validate_checksum(ChecksumAlgorithm::Sha256, sha256).with_context(|| {
            format!(
                "invalid SHA-256 in {LIBRARIES_LIST} line {}",
                line_index + 1
            )
        })?;
        let coordinate = Coordinate::parse(coordinate).with_context(|| {
            format!(
                "invalid coordinate in {LIBRARIES_LIST} line {}",
                line_index + 1
            )
        })?;
        let expected_path = coordinate.binary_artifact_path();
        if *artifact_path != expected_path {
            bail!(
                "invalid artifact path in {LIBRARIES_LIST} line {}: expected {expected_path:?}, got {artifact_path:?}",
                line_index + 1
            );
        }
        if !coordinates.insert(coordinate.full.clone()) {
            bail!(
                "duplicate coordinate in {LIBRARIES_LIST}: {:?}",
                coordinate.full
            );
        }
        if !artifact_paths.insert(expected_path.clone()) {
            bail!("duplicate artifact path in {LIBRARIES_LIST}: {expected_path:?}");
        }
        entries.push(ManifestEntry {
            coordinate,
            artifact_path: expected_path,
            sha256: sha256.to_ascii_lowercase(),
        });
    }
    if entries.is_empty() {
        bail!("{LIBRARIES_LIST} contains no libraries");
    }
    Ok(entries)
}

fn validate_checksum(algorithm: ChecksumAlgorithm, value: &str) -> Result<()> {
    let expected_length = match algorithm {
        ChecksumAlgorithm::Sha1 => 40,
        ChecksumAlgorithm::Sha256 => 64,
    };
    if value.len() != expected_length || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid {} checksum {value:?}", algorithm.name());
    }
    Ok(())
}

fn validate_coordinate_component(label: &str, value: &str) -> Result<()> {
    let mut components = Path::new(value).components();
    if value.is_empty()
        || value.ends_with('.')
        || value.ends_with(' ')
        || value.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
                )
        })
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || is_windows_device_name(value)
    {
        bail!("invalid Maven {label}: {value:?}");
    }
    Ok(())
}

fn is_windows_device_name(value: &str) -> bool {
    let stem = value.split('.').next().unwrap_or(value);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(|number| number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'9'))
}

#[derive(Clone, Debug)]
struct Coordinate {
    full: String,
    group: String,
    artifact: String,
    version: String,
    classifier: Option<String>,
}

impl Coordinate {
    fn parse(value: &str) -> Result<Self> {
        let components = value.split(':').collect::<Vec<_>>();
        let (group, artifact, version, classifier) = match components.as_slice() {
            [group, artifact, version] => (*group, *artifact, *version, None),
            [group, artifact, version, classifier] => {
                (*group, *artifact, *version, Some(*classifier))
            }
            _ => bail!(
                "expected group:artifact:version or group:artifact:version:classifier, got {value:?}"
            ),
        };
        for segment in group.split('.') {
            validate_coordinate_component("group component", segment)?;
        }
        validate_coordinate_component("artifact", artifact)?;
        validate_coordinate_component("version", version)?;
        if let Some(classifier) = classifier {
            validate_coordinate_component("classifier", classifier)?;
        }
        Ok(Self {
            full: value.to_owned(),
            group: group.to_owned(),
            artifact: artifact.to_owned(),
            version: version.to_owned(),
            classifier: classifier.map(str::to_owned),
        })
    }

    fn id(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.version)
    }

    fn directory(&self) -> String {
        format!(
            "{}/{}/{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version
        )
    }

    fn binary_artifact_path(&self) -> String {
        let classifier = self
            .classifier
            .as_deref()
            .map(|classifier| format!("-{classifier}"))
            .unwrap_or_default();
        format!(
            "{}/{}-{}{}.jar",
            self.directory(),
            self.artifact,
            self.version,
            classifier
        )
    }

    fn source_artifact_path(&self) -> String {
        format!(
            "{}/{}-{}-sources.jar",
            self.directory(),
            self.artifact,
            self.version
        )
    }
}

struct ManifestEntry {
    coordinate: Coordinate,
    artifact_path: String,
    sha256: String,
}

struct LibraryGroup {
    id: String,
    coordinate: Coordinate,
    binaries: Vec<PreparedBinary>,
}

struct PreparedBinary {
    identity: BinaryIdentity,
    coordinate: Coordinate,
    artifact_path: String,
}

#[derive(Clone, Copy)]
struct Repository {
    cache_name: &'static str,
    url: &'static str,
}

fn repository_for(coordinate: &Coordinate) -> Repository {
    if coordinate.group == "com.mojang" {
        MOJANG_REPOSITORY
    } else {
        CENTRAL_REPOSITORY
    }
}

#[derive(Clone, Copy)]
enum ChecksumAlgorithm {
    Sha1,
    Sha256,
}

impl ChecksumAlgorithm {
    const fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
        }
    }
}

struct PublishedChecksum {
    algorithm: ChecksumAlgorithm,
    value: String,
}

impl PublishedChecksum {
    fn integrity(&self) -> Integrity<'_> {
        match self.algorithm {
            ChecksumAlgorithm::Sha1 => Integrity::sha1(&self.value),
            ChecksumAlgorithm::Sha256 => Integrity::sha256(&self.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA256_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA256_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn classifier_binaries_share_one_source_coordinate() {
        let manifest = format!(
            "{SHA256_A}\tio.netty:netty-transport-native-epoll:4.2.16.Final:linux-aarch_64\tio/netty/netty-transport-native-epoll/4.2.16.Final/netty-transport-native-epoll-4.2.16.Final-linux-aarch_64.jar\n{SHA256_B}\tio.netty:netty-transport-native-epoll:4.2.16.Final:linux-x86_64\tio/netty/netty-transport-native-epoll/4.2.16.Final/netty-transport-native-epoll-4.2.16.Final-linux-x86_64.jar\n"
        );
        let entries = parse_manifest(&manifest).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].coordinate.id(), entries[1].coordinate.id());
        assert_eq!(
            entries[0].coordinate.source_artifact_path(),
            "io/netty/netty-transport-native-epoll/4.2.16.Final/netty-transport-native-epoll-4.2.16.Final-sources.jar"
        );
        assert_eq!(
            entries[0].coordinate.source_artifact_path(),
            entries[1].coordinate.source_artifact_path()
        );
    }

    #[test]
    fn manifest_rejects_noncanonical_or_duplicate_entries() {
        let wrong_path =
            format!("{SHA256_A}\tcom.example:library:1.0\tcom/example/library/1.0/other.jar\n");
        assert!(parse_manifest(&wrong_path).is_err());

        let duplicate = format!(
            "{SHA256_A}\tcom.example:library:1.0\tcom/example/library/1.0/library-1.0.jar\n{SHA256_B}\tcom.example:library:1.0\tcom/example/library/1.0/library-1.0.jar\n"
        );
        assert!(parse_manifest(&duplicate).is_err());

        let blank_line = format!(
            "{SHA256_A}\tcom.example:library:1.0\tcom/example/library/1.0/library-1.0.jar\n\n"
        );
        assert!(parse_manifest(&blank_line).is_err());
    }

    #[test]
    fn repository_is_selected_only_by_group() {
        let mojang = Coordinate::parse("com.mojang:brigadier:1.3.11").unwrap();
        let central = Coordinate::parse("com.google.code.gson:gson:2.14.0").unwrap();
        assert_eq!(repository_for(&mojang).url, MOJANG_REPOSITORY.url);
        assert_eq!(repository_for(&central).url, CENTRAL_REPOSITORY.url);
    }

    #[test]
    fn checksum_and_zip_paths_are_strict() {
        assert!(validate_checksum(ChecksumAlgorithm::Sha1, &"a".repeat(40)).is_ok());
        assert!(validate_checksum(ChecksumAlgorithm::Sha256, &"a".repeat(64)).is_ok());
        assert!(validate_checksum(ChecksumAlgorithm::Sha256, &"a".repeat(40)).is_err());
        assert!(validate_zip_entry("com/example/Source.java").is_ok());
        assert!(validate_zip_entry("../Source.java").is_err());
        assert!(validate_zip_entry("com\\example\\Source.java").is_err());
    }

    #[test]
    fn coordinate_components_are_safe_on_windows() {
        assert!(Coordinate::parse("com.example:library:1.0").is_ok());
        assert!(Coordinate::parse("com.example:CON:1.0").is_err());
        assert!(Coordinate::parse("com.example:library:Lpt9.txt").is_err());
        assert!(Coordinate::parse("com.example:library:1.0.").is_err());
        assert!(Coordinate::parse("com.example:library:1.0 :classifier").is_err());
    }
}
