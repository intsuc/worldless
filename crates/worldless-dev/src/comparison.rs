use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    artifacts,
    config::validate_minecraft_version_id,
    download,
    sources::{self, FileKind, OriginFile, VerifiedSources},
};

const COMPARISON_SCHEMA_VERSION: u32 = 1;
const INPUT_FINGERPRINT_VERSION: &str = "worldless-dev-source-comparison-input-v1";
const TREE_FINGERPRINT_VERSION: &str = "worldless-dev-source-comparison-tree-v1";
const INVENTORY_FILE: &str = "comparison.json";
const BEFORE_DIRECTORY: &str = "before";
const AFTER_DIRECTORY: &str = "after";

type SparseDirectories = BTreeSet<String>;
type SparseFiles = BTreeMap<String, (String, u64)>;
type SparseEntries = (SparseDirectories, SparseFiles);

pub fn generate(
    before: &VerifiedSources,
    after: &VerifiedSources,
    generated_root: &Path,
) -> Result<PathBuf> {
    let before_version = before.inventory().minecraft_version();
    let after_version = after.inventory().minecraft_version();
    validate_minecraft_version_id("from-version-id", before_version)?;
    validate_minecraft_version_id("to-version-id", after_version)?;
    if before_version == after_version {
        bail!("source comparison requires two different Minecraft version ids");
    }

    if before.inventory().vineflower_fingerprint() != after.inventory().vineflower_fingerprint() {
        bail!(
            "cannot compare source trees produced by different Vineflower configurations; regenerate both Minecraft versions with the current worldless-dev"
        );
    }

    let comparison = build_comparison(before, after);
    let input_sha256 = input_fingerprint(&comparison);
    let comparisons_root = generated_root.join("minecraft-diffs");
    let output_parent = comparisons_root.join(before_version);
    let output = output_parent.join(after_version);

    match fs::symlink_metadata(&output) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() {
                bail!(
                    "source comparison output is not a regular directory: {}",
                    output.display()
                );
            }
            verify_output(&output, &comparison, &input_sha256)?;
            eprintln!("Using existing Minecraft source comparison");
            return Ok(output);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect source comparison output {}",
                    output.display()
                )
            });
        }
    }

    artifacts::ensure_directory(generated_root, &output_parent)?;
    let work_root = generated_root.join("minecraft-diffs-work");
    artifacts::ensure_directory(generated_root, &work_root)?;
    let temporary = work_root.join(&input_sha256);
    artifacts::ensure_absent(&temporary, "temporary source comparison")?;
    fs::create_dir(&temporary).with_context(|| {
        format!(
            "failed to create temporary source comparison {}",
            temporary.display()
        )
    })?;

    let result = (|| -> Result<()> {
        create_fixed_directories(&temporary)?;
        write_inventory(&temporary, &comparison)?;
        materialize_changes(&comparison, before.path(), after.path(), &temporary)?;
        let tree_sha256 = comparison_tree_sha256(&temporary)?;
        artifacts::write_completion(&temporary, &input_sha256, &tree_sha256)?;
        verify_output(&temporary, &comparison, &input_sha256)?;
        artifacts::ensure_absent(&output, "source comparison output")?;
        fs::rename(&temporary, &output).with_context(|| {
            format!(
                "failed to move completed source comparison {} to {}",
                temporary.display(),
                output.display()
            )
        })
    })();

    if let Err(mut error) = result {
        if let Err(cleanup) = remove_owned_work_directory(&work_root, &temporary) {
            error = error.context(format!(
                "also failed to remove temporary source comparison {}: {cleanup}",
                temporary.display()
            ));
        }
        return Err(error);
    }

    Ok(output)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    schema_version: u32,
    before: SourceIdentity,
    after: SourceIdentity,
    changes: Vec<FileChange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceIdentity {
    minecraft_version: String,
    input_sha256: String,
    tree_sha256: String,
    vineflower_fingerprint: String,
}

impl SourceIdentity {
    fn from_verified(sources: &VerifiedSources) -> Self {
        Self {
            minecraft_version: sources.inventory().minecraft_version().to_owned(),
            input_sha256: sources.inventory().input_sha256().to_owned(),
            tree_sha256: sources.tree_sha256().to_owned(),
            vineflower_fingerprint: sources.inventory().vineflower_fingerprint().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
enum FileChange {
    Added {
        after: OriginFile,
    },
    Removed {
        before: OriginFile,
    },
    Modified {
        before: OriginFile,
        after: OriginFile,
    },
    Moved {
        before: OriginFile,
        after: OriginFile,
    },
}

impl FileChange {
    fn before(&self) -> Option<&OriginFile> {
        match self {
            Self::Added { .. } => None,
            Self::Removed { before }
            | Self::Modified { before, .. }
            | Self::Moved { before, .. } => Some(before),
        }
    }

    fn after(&self) -> Option<&OriginFile> {
        match self {
            Self::Removed { .. } => None,
            Self::Added { after } | Self::Modified { after, .. } | Self::Moved { after, .. } => {
                Some(after)
            }
        }
    }

    fn sort_key(&self) -> (&str, &str, u8) {
        let rank = match self {
            Self::Added { .. } => 0,
            Self::Removed { .. } => 1,
            Self::Modified { .. } => 2,
            Self::Moved { .. } => 3,
        };
        (
            self.before().map_or("", OriginFile::path),
            self.after().map_or("", OriginFile::path),
            rank,
        )
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ContentIdentity {
    kind: FileKind,
    size: u64,
    sha256: String,
}

impl ContentIdentity {
    fn from_file(file: &OriginFile) -> Self {
        Self {
            kind: file.kind(),
            size: file.size(),
            sha256: file.sha256().to_owned(),
        }
    }
}

fn build_comparison(before: &VerifiedSources, after: &VerifiedSources) -> Comparison {
    Comparison {
        schema_version: COMPARISON_SCHEMA_VERSION,
        before: SourceIdentity::from_verified(before),
        after: SourceIdentity::from_verified(after),
        changes: compare_files(before.inventory().files(), after.inventory().files()),
    }
}

fn compare_files(before: &[OriginFile], after: &[OriginFile]) -> Vec<FileChange> {
    let mut before_by_path = before
        .iter()
        .cloned()
        .map(|file| (file.path().to_owned(), file))
        .collect::<BTreeMap<_, _>>();
    let mut after_by_path = after
        .iter()
        .cloned()
        .map(|file| (file.path().to_owned(), file))
        .collect::<BTreeMap<_, _>>();
    let shared_paths = before_by_path
        .keys()
        .filter(|path| after_by_path.contains_key(*path))
        .cloned()
        .collect::<Vec<_>>();
    let mut changes = Vec::new();

    for path in shared_paths {
        let before = before_by_path
            .remove(&path)
            .expect("shared before path disappeared");
        let after = after_by_path
            .remove(&path)
            .expect("shared after path disappeared");
        if before.sha256() != after.sha256() || before.size() != after.size() {
            changes.push(FileChange::Modified { before, after });
        }
    }

    let before_content = group_paths_by_content(&before_by_path);
    let after_content = group_paths_by_content(&after_by_path);
    let moves = before_content
        .iter()
        .filter_map(|(identity, before_paths)| {
            let after_paths = after_content.get(identity)?;
            (before_paths.len() == 1 && after_paths.len() == 1)
                .then(|| (before_paths[0].clone(), after_paths[0].clone()))
        })
        .collect::<Vec<_>>();

    for (before_path, after_path) in moves {
        let before = before_by_path
            .remove(&before_path)
            .expect("move source disappeared");
        let after = after_by_path
            .remove(&after_path)
            .expect("move destination disappeared");
        changes.push(FileChange::Moved { before, after });
    }

    changes.extend(
        before_by_path
            .into_values()
            .map(|before| FileChange::Removed { before }),
    );
    changes.extend(
        after_by_path
            .into_values()
            .map(|after| FileChange::Added { after }),
    );
    changes.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
    changes
}

fn group_paths_by_content(
    files: &BTreeMap<String, OriginFile>,
) -> BTreeMap<ContentIdentity, Vec<String>> {
    let mut groups = BTreeMap::<ContentIdentity, Vec<String>>::new();
    for (path, file) in files {
        groups
            .entry(ContentIdentity::from_file(file))
            .or_default()
            .push(path.clone());
    }
    groups
}

fn input_fingerprint(comparison: &Comparison) -> String {
    let mut hasher = Sha256::new();
    artifacts::hash_field(&mut hasher, INPUT_FINGERPRINT_VERSION);
    artifacts::hash_field(&mut hasher, &comparison.schema_version.to_string());
    for identity in [&comparison.before, &comparison.after] {
        artifacts::hash_field(&mut hasher, &identity.minecraft_version);
        artifacts::hash_field(&mut hasher, &identity.input_sha256);
        artifacts::hash_field(&mut hasher, &identity.tree_sha256);
        artifacts::hash_field(&mut hasher, &identity.vineflower_fingerprint);
    }
    format!("{:x}", hasher.finalize())
}

fn create_fixed_directories(output: &Path) -> Result<()> {
    for relative in [
        BEFORE_DIRECTORY,
        "before/code",
        "before/artifacts",
        AFTER_DIRECTORY,
        "after/code",
        "after/artifacts",
    ] {
        let path = output.join(relative);
        fs::create_dir(&path).with_context(|| {
            format!(
                "failed to create source comparison directory {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn write_inventory(output: &Path, comparison: &Comparison) -> Result<()> {
    let path = output.join(INVENTORY_FILE);
    let mut bytes =
        serde_json::to_vec_pretty(comparison).context("failed to serialize source comparison")?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create source comparison {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("failed to write source comparison {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush source comparison {}", path.display()))
}

fn materialize_changes(
    comparison: &Comparison,
    before_sources: &Path,
    after_sources: &Path,
    output: &Path,
) -> Result<()> {
    for change in &comparison.changes {
        if let Some(file) = change.before() {
            materialize_file(before_sources, &output.join(BEFORE_DIRECTORY), file)?;
        }
        if let Some(file) = change.after() {
            materialize_file(after_sources, &output.join(AFTER_DIRECTORY), file)?;
        }
    }
    Ok(())
}

fn materialize_file(source_root: &Path, output_root: &Path, file: &OriginFile) -> Result<()> {
    let relative = sources::payload_relative_path(file.path(), "comparison source path")?;
    let source = source_root.join(&relative);
    let metadata = fs::symlink_metadata(&source)
        .with_context(|| format!("failed to inspect comparison source {}", source.display()))?;
    if !metadata.file_type().is_file() {
        bail!(
            "comparison source is not a regular file: {}",
            source.display()
        );
    }

    let destination = output_root.join(&relative);
    let parent = destination.parent().with_context(|| {
        format!(
            "source comparison output has no parent: {}",
            destination.display()
        )
    })?;
    create_output_directory(output_root, parent)?;
    let mut input =
        File::open(&source).with_context(|| format!("failed to open {}", source.display()))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let (sha256, size) =
        artifacts::copy_and_hash(&mut input, &mut output, &destination, "comparison source")?;
    output
        .flush()
        .with_context(|| format!("failed to flush {}", destination.display()))?;
    if sha256 != file.sha256() || size != file.size() {
        bail!(
            "comparison source changed after verification for {:?}: expected {} bytes/{}, got {size} bytes/{sha256}",
            file.path(),
            file.size(),
            file.sha256()
        );
    }
    Ok(())
}

fn create_output_directory(root: &Path, directory: &Path) -> Result<()> {
    let relative = directory.strip_prefix(root).with_context(|| {
        format!(
            "comparison directory {} is outside {}",
            directory.display(),
            root.display()
        )
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            bail!("invalid comparison directory: {}", directory.display());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => bail!(
                "comparison path component is not a regular directory: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!(
                        "failed to create comparison directory {}",
                        current.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect comparison directory {}",
                        current.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn verify_output(output: &Path, expected: &Comparison, input_sha256: &str) -> Result<()> {
    verify_root_shape(output)?;
    let inventory_path = output.join(INVENTORY_FILE);
    let inventory_file = File::open(&inventory_path).with_context(|| {
        format!(
            "failed to open source comparison {}",
            inventory_path.display()
        )
    })?;
    let actual: Comparison = serde_json::from_reader(BufReader::new(inventory_file))
        .with_context(|| format!("invalid source comparison {}", inventory_path.display()))?;
    if actual != *expected {
        bail!(
            "source comparison does not match its source trees: {}; remove {} before retrying",
            inventory_path.display(),
            output.display()
        );
    }

    let (expected_directories, expected_files) = expected_sparse_entries(expected)?;
    let (actual_directories, actual_files) = collect_sparse_entries(output)?;
    if actual_directories != expected_directories || actual_files != expected_files {
        bail!(
            "source comparison files do not match {}: remove {} before retrying",
            inventory_path.display(),
            output.display()
        );
    }

    let tree_sha256 = comparison_tree_sha256(output)?;
    let completion_path = output.join(artifacts::COMPLETION_FILE);
    let actual_completion = fs::read_to_string(&completion_path).with_context(|| {
        format!(
            "failed to read source comparison completion record {}",
            completion_path.display()
        )
    })?;
    let expected_completion = artifacts::completion_record(input_sha256, &tree_sha256);
    if actual_completion != expected_completion {
        bail!(
            "source comparison does not match its completion record: {}; remove it before retrying",
            output.display()
        );
    }
    Ok(())
}

fn verify_root_shape(output: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(output).with_context(|| {
        format!(
            "failed to inspect source comparison output {}",
            output.display()
        )
    })?;
    if !metadata.file_type().is_dir() {
        bail!(
            "source comparison output is not a regular directory: {}",
            output.display()
        );
    }
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(output).with_context(|| {
        format!(
            "failed to read source comparison output {}",
            output.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to read source comparison output {}",
                output.display()
            )
        })?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("source comparison contains a non-UTF-8 root entry"))?;
        names.insert(name);
    }
    let expected = [
        artifacts::COMPLETION_FILE.to_owned(),
        INVENTORY_FILE.to_owned(),
        BEFORE_DIRECTORY.to_owned(),
        AFTER_DIRECTORY.to_owned(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if names != expected {
        bail!(
            "source comparison has unexpected root entries: {}; remove it before retrying",
            output.display()
        );
    }
    artifacts::require_directory(&output.join(BEFORE_DIRECTORY), "comparison directory")?;
    artifacts::require_directory(&output.join(AFTER_DIRECTORY), "comparison directory")?;
    artifacts::require_file(&output.join(INVENTORY_FILE), "comparison file")?;
    artifacts::require_file(&output.join(artifacts::COMPLETION_FILE), "comparison file")
}

fn expected_sparse_entries(comparison: &Comparison) -> Result<SparseEntries> {
    let mut directories = [
        BEFORE_DIRECTORY.to_owned(),
        "before/code".to_owned(),
        "before/artifacts".to_owned(),
        AFTER_DIRECTORY.to_owned(),
        "after/code".to_owned(),
        "after/artifacts".to_owned(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut files = BTreeMap::new();
    let mut portable_paths = BTreeMap::new();
    for change in &comparison.changes {
        if let Some(file) = change.before() {
            insert_expected_entry(
                BEFORE_DIRECTORY,
                file,
                &mut directories,
                &mut files,
                &mut portable_paths,
            )?;
        }
        if let Some(file) = change.after() {
            insert_expected_entry(
                AFTER_DIRECTORY,
                file,
                &mut directories,
                &mut files,
                &mut portable_paths,
            )?;
        }
    }
    Ok((directories, files))
}

fn insert_expected_entry(
    side: &str,
    file: &OriginFile,
    directories: &mut BTreeSet<String>,
    files: &mut BTreeMap<String, (String, u64)>,
    portable_paths: &mut BTreeMap<String, String>,
) -> Result<()> {
    sources::payload_relative_path(file.path(), "comparison inventory path")?;
    let path = format!("{side}/{}", file.path());
    let portable = sources::portable_key(&path);
    if let Some(previous) = portable_paths.insert(portable, path.clone()) {
        bail!("source comparison paths collide case-insensitively: {previous:?} and {path:?}");
    }
    if files
        .insert(path.clone(), (file.sha256().to_owned(), file.size()))
        .is_some()
    {
        bail!("source comparison repeats path {path:?}");
    }
    let components = path.split('/').collect::<Vec<_>>();
    for length in 1..components.len() {
        directories.insert(components[..length].join("/"));
    }
    Ok(())
}

fn collect_sparse_entries(output: &Path) -> Result<SparseEntries> {
    let mut directories = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut portable_paths = BTreeMap::new();
    for side in [BEFORE_DIRECTORY, AFTER_DIRECTORY] {
        collect_sparse_directory(
            output,
            &output.join(side),
            &mut directories,
            &mut files,
            &mut portable_paths,
        )?;
    }
    Ok((directories, files))
}

fn collect_sparse_directory(
    root: &Path,
    directory: &Path,
    directories: &mut BTreeSet<String>,
    files: &mut BTreeMap<String, (String, u64)>,
    portable_paths: &mut BTreeMap<String, String>,
) -> Result<()> {
    let relative_directory = directory.strip_prefix(root).with_context(|| {
        format!(
            "source comparison directory {} is outside {}",
            directory.display(),
            root.display()
        )
    })?;
    let relative_directory =
        sources::portable_relative_path(relative_directory, "comparison directory path")?;
    directories.insert(relative_directory);
    let entries = fs::read_dir(directory).with_context(|| {
        format!(
            "failed to read comparison directory {}",
            directory.display()
        )
    })?;
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to read comparison directory {}",
                directory.display()
            )
        })?;
        let path = entry.path();
        let relative = path.strip_prefix(root).with_context(|| {
            format!(
                "source comparison path {} is outside {}",
                path.display(),
                root.display()
            )
        })?;
        let relative = sources::portable_relative_path(relative, "comparison output path")?;
        let portable = sources::portable_key(&relative);
        if let Some(previous) = portable_paths.insert(portable, relative.clone()) {
            bail!(
                "source comparison output paths collide case-insensitively: {previous:?} and {relative:?}"
            );
        }
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_symlink() {
            bail!(
                "source comparison contains symbolic link: {}",
                path.display()
            );
        }
        if file_type.is_dir() {
            collect_sparse_directory(root, &path, directories, files, portable_paths)?;
        } else if file_type.is_file() {
            let size = entry
                .metadata()
                .with_context(|| format!("failed to inspect {}", path.display()))?
                .len();
            let sha256 = download::file_sha256(&path)
                .with_context(|| format!("failed to hash {}", path.display()))?;
            if files.insert(relative.clone(), (sha256, size)).is_some() {
                bail!("source comparison repeats path {relative:?}");
            }
        } else {
            bail!(
                "source comparison contains special file: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn comparison_tree_sha256(output: &Path) -> Result<String> {
    let (directories, mut files) = collect_sparse_entries(output)?;
    let inventory_path = output.join(INVENTORY_FILE);
    let inventory_metadata = fs::symlink_metadata(&inventory_path).with_context(|| {
        format!(
            "failed to inspect source comparison {}",
            inventory_path.display()
        )
    })?;
    if !inventory_metadata.file_type().is_file() {
        bail!(
            "source comparison inventory is not a regular file: {}",
            inventory_path.display()
        );
    }
    let inventory_sha256 = download::file_sha256(&inventory_path)
        .with_context(|| format!("failed to hash {}", inventory_path.display()))?;
    files.insert(
        INVENTORY_FILE.to_owned(),
        (inventory_sha256, inventory_metadata.len()),
    );

    let mut entries = directories
        .into_iter()
        .map(|path| (path, None))
        .chain(
            files
                .into_iter()
                .map(|(path, (sha256, size))| (path, Some((sha256, size)))),
        )
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    artifacts::hash_field(&mut hasher, TREE_FINGERPRINT_VERSION);
    for (path, file) in entries {
        match file {
            None => {
                artifacts::hash_field(&mut hasher, "directory");
                artifacts::hash_field(&mut hasher, &path);
            }
            Some((sha256, size)) => {
                artifacts::hash_field(&mut hasher, "file");
                artifacts::hash_field(&mut hasher, &path);
                hasher.update(size.to_be_bytes());
                artifacts::hash_field(&mut hasher, &sha256);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn remove_owned_work_directory(work_root: &Path, directory: &Path) -> Result<()> {
    if directory.parent() != Some(work_root) {
        bail!(
            "refusing to remove unowned comparison directory {}",
            directory.display()
        );
    }
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", directory.display()));
        }
    };
    if !metadata.file_type().is_dir() {
        bail!(
            "refusing to remove non-directory comparison path {}",
            directory.display()
        );
    }
    let canonical_root = fs::canonicalize(work_root)
        .with_context(|| format!("failed to resolve {}", work_root.display()))?;
    let canonical_directory = fs::canonicalize(directory)
        .with_context(|| format!("failed to resolve {}", directory.display()))?;
    if canonical_directory.parent() != Some(canonical_root.as_path()) {
        bail!(
            "refusing to remove comparison directory outside {}: {}",
            canonical_root.display(),
            canonical_directory.display()
        );
    }
    fs::remove_dir_all(&canonical_directory).with_context(|| {
        format!(
            "failed to remove comparison directory {}",
            canonical_directory.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "worldless-dev-comparison-test-{}-{}",
                std::process::id(),
                NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            if self.0.exists() {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }

    fn file(path: &str, byte: char) -> OriginFile {
        OriginFile::for_test(
            path,
            if path.ends_with(".java") {
                FileKind::Java
            } else {
                FileKind::Artifact
            },
            byte.to_string().repeat(64),
            10,
        )
    }

    fn write_source(root: &Path, path: &str, contents: &[u8]) -> OriginFile {
        let destination = root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination, contents).unwrap();
        OriginFile::for_test(
            path,
            FileKind::Java,
            format!("{:x}", Sha256::digest(contents)),
            contents.len() as u64,
        )
    }

    #[test]
    fn file_comparison_omits_unchanged_and_detects_exact_changes() {
        let before = vec![
            file("code/p/Unchanged.java", '1'),
            file("code/p/Modified.java", '2'),
            file("code/p/Removed.java", '3'),
            file("code/p/OldName.java", '4'),
        ];
        let after = vec![
            file("code/p/Unchanged.java", '1'),
            file("code/p/Modified.java", '5'),
            file("code/p/Added.java", '6'),
            file("code/p/NewName.java", '4'),
        ];

        let changes = compare_files(&before, &after);
        assert_eq!(changes.len(), 4);
        assert!(changes.iter().any(|change| matches!(
            change,
            FileChange::Modified { before, after }
                if before.path() == "code/p/Modified.java"
                    && after.path() == "code/p/Modified.java"
        )));
        assert!(changes.iter().any(|change| matches!(
            change,
            FileChange::Removed { before } if before.path() == "code/p/Removed.java"
        )));
        assert!(changes.iter().any(|change| matches!(
            change,
            FileChange::Added { after } if after.path() == "code/p/Added.java"
        )));
        assert!(changes.iter().any(|change| matches!(
            change,
            FileChange::Moved { before, after }
                if before.path() == "code/p/OldName.java"
                    && after.path() == "code/p/NewName.java"
        )));
    }

    #[test]
    fn duplicate_content_is_not_guessed_as_a_move() {
        let before = vec![
            file("code/p/First.java", '7'),
            file("code/p/Second.java", '7'),
        ];
        let after = vec![file("code/p/Third.java", '7')];

        let changes = compare_files(&before, &after);
        assert_eq!(changes.len(), 3);
        assert!(
            !changes
                .iter()
                .any(|change| matches!(change, FileChange::Moved { .. }))
        );
        assert_eq!(
            changes
                .iter()
                .filter(|change| matches!(change, FileChange::Removed { .. }))
                .count(),
            2
        );
        assert_eq!(
            changes
                .iter()
                .filter(|change| matches!(change, FileChange::Added { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn generate_materializes_reuses_and_verifies_sparse_output() {
        let temporary = TestDirectory::new();
        let before_root = temporary.0.join("before-sources");
        let after_root = temporary.0.join("after-sources");
        fs::create_dir(&before_root).unwrap();
        fs::create_dir(&after_root).unwrap();
        let before_files = vec![
            write_source(&before_root, "code/p/Unchanged.java", b"unchanged"),
            write_source(&before_root, "code/p/Modified.java", b"before"),
            write_source(&before_root, "code/p/Removed.java", b"removed"),
            write_source(&before_root, "code/p/OldName.java", b"moved"),
        ];
        let after_files = vec![
            write_source(&after_root, "code/p/Unchanged.java", b"unchanged"),
            write_source(&after_root, "code/p/Modified.java", b"after"),
            write_source(&after_root, "code/p/Added.java", b"added"),
            write_source(&after_root, "code/p/NewName.java", b"moved"),
        ];
        let before = VerifiedSources::for_test(
            before_root,
            "before",
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            before_files,
        );
        let after = VerifiedSources::for_test(
            after_root,
            "after",
            "4".repeat(64),
            "5".repeat(64),
            "3".repeat(64),
            after_files,
        );
        let generated = temporary.0.join("generated");
        let output = generate(&before, &after, &generated).unwrap();

        assert!(
            !output
                .join("before/code/p/Unchanged.java")
                .try_exists()
                .unwrap()
        );
        assert!(
            !output
                .join("after/code/p/Unchanged.java")
                .try_exists()
                .unwrap()
        );
        assert_eq!(generate(&before, &after, &generated).unwrap(), output);
        fs::write(output.join("after/code/p/Modified.java"), b"tampered").unwrap();
        assert!(generate(&before, &after, &generated).is_err());
    }

    #[test]
    fn generate_supports_an_empty_change_set() {
        let temporary = TestDirectory::new();
        let before_root = temporary.0.join("before-sources");
        let after_root = temporary.0.join("after-sources");
        fs::create_dir(&before_root).unwrap();
        fs::create_dir(&after_root).unwrap();
        let before_file = write_source(&before_root, "code/p/Same.java", b"same");
        let after_file = write_source(&after_root, "code/p/Same.java", b"same");
        let before = VerifiedSources::for_test(
            before_root,
            "before",
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            vec![before_file],
        );
        let after = VerifiedSources::for_test(
            after_root,
            "after",
            "4".repeat(64),
            "5".repeat(64),
            "3".repeat(64),
            vec![after_file],
        );

        let output = generate(&before, &after, &temporary.0.join("generated")).unwrap();
        let inventory: Comparison =
            serde_json::from_reader(File::open(output.join(INVENTORY_FILE)).unwrap()).unwrap();
        assert!(inventory.changes.is_empty());
        for directory in [
            "before/code",
            "before/artifacts",
            "after/code",
            "after/artifacts",
        ] {
            assert!(
                fs::read_dir(output.join(directory))
                    .unwrap()
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn generate_rejects_different_vineflower_fingerprints_before_writing() {
        let temporary = TestDirectory::new();
        let before = VerifiedSources::for_test(
            temporary.0.join("before-sources"),
            "before",
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            Vec::new(),
        );
        let after = VerifiedSources::for_test(
            temporary.0.join("after-sources"),
            "after",
            "4".repeat(64),
            "5".repeat(64),
            "6".repeat(64),
            Vec::new(),
        );
        let generated = temporary.0.join("generated");

        assert!(generate(&before, &after, &generated).is_err());
        assert!(!generated.try_exists().unwrap());
    }

    #[test]
    fn generate_removes_its_temporary_output_after_failure() {
        let temporary = TestDirectory::new();
        let before_root = temporary.0.join("before-sources");
        let after_root = temporary.0.join("after-sources");
        fs::create_dir(&before_root).unwrap();
        fs::create_dir(&after_root).unwrap();
        let before = VerifiedSources::for_test(
            before_root,
            "before",
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
            Vec::new(),
        );
        let after = VerifiedSources::for_test(
            after_root,
            "after",
            "4".repeat(64),
            "5".repeat(64),
            "3".repeat(64),
            vec![file("code/p/Missing.java", '7')],
        );
        let comparison = build_comparison(&before, &after);
        let generated = temporary.0.join("generated");
        let work = generated
            .join("minecraft-diffs-work")
            .join(input_fingerprint(&comparison));
        let output = generated
            .join("minecraft-diffs")
            .join("before")
            .join("after");

        assert!(generate(&before, &after, &generated).is_err());
        assert!(!work.try_exists().unwrap());
        assert!(!output.try_exists().unwrap());
    }
}
