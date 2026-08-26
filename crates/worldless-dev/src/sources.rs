use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::{
    artifacts, classfiles, download,
    libraries::{self, Input},
    mojang::PreparedMinecraft,
    server,
    vineflower::{self, Downloaded},
};

const INVENTORY_SCHEMA_VERSION: u32 = 1;
const INPUT_FINGERPRINT_VERSION: &str = "worldless-dev-sources-input-v3";
const TREE_FINGERPRINT_VERSION: &str = "worldless-dev-sources-tree-v1";
const INVENTORY_FILE: &str = "origins.json";
const CODE_DIRECTORY: &str = "code";
const ARTIFACTS_DIRECTORY: &str = "artifacts";

pub fn generate(
    minecraft: &PreparedMinecraft,
    vineflower: &Downloaded,
    server_jar: &Path,
    libraries: &libraries::Prepared,
    minecraft_version: &str,
    generated_root: &Path,
) -> Result<VerifiedSources> {
    validate_single_component("Minecraft version", minecraft_version)?;
    validate_sha256("library manifest", &libraries.manifest_sha256)?;

    let vineflower_fingerprint = vineflower::fingerprint(vineflower);
    validate_sha256("Vineflower fingerprint", &vineflower_fingerprint)?;
    let server_sha256 = download::file_sha256(server_jar)
        .with_context(|| format!("failed to hash server input {}", server_jar.display()))?;
    let resolved_libraries = resolve_libraries(libraries)?;
    let input_sha256 = input_fingerprint(
        minecraft,
        minecraft_version,
        &server_sha256,
        &libraries.manifest_sha256,
        &vineflower_fingerprint,
        &resolved_libraries,
    );

    let parent = generated_root.join("minecraft").join(minecraft_version);
    artifacts::ensure_directory(generated_root, &parent)?;
    let output = parent.join("sources");

    match fs::symlink_metadata(&output) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() {
                bail!(
                    "source output is not a regular directory: {}",
                    output.display()
                );
            }
            let verified = verify_output(&output, minecraft_version, &input_sha256)?;
            eprintln!("Using existing combined sources");
            return Ok(verified);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect source output {}", output.display()));
        }
    }

    let temporary = parent.join("sources.part");
    let work = parent.join("sources.work");
    artifacts::ensure_absent(&temporary, "temporary source output")?;
    artifacts::ensure_absent(&work, "source work directory")?;
    fs::create_dir(&work)
        .with_context(|| format!("failed to create source work directory {}", work.display()))?;
    let mut work_created = true;
    let mut temporary_created = false;

    let result = (|| -> Result<VerifiedSources> {
        let mut plans = Vec::with_capacity(resolved_libraries.len() + 1);

        let server_output = work.join("server");
        let server_source_count = server::expected_source_count(server_jar)?;
        vineflower::decompile_to(
            minecraft,
            vineflower,
            server_jar,
            &server_output,
            server_source_count,
            &format!("Minecraft {minecraft_version}"),
        )?;
        plans.push(scan_directory(
            OriginArtifact {
                id: format!("minecraft-server:{minecraft_version}"),
                artifact_path: "minecraft-server".to_owned(),
                binaries: vec![OriginBinary {
                    coordinate: format!("minecraft-server:{minecraft_version}"),
                    sha256: server_sha256.clone(),
                }],
                input: OriginInput::Decompiled {
                    sha256: server_sha256.clone(),
                },
            },
            SafePath::from_validated("minecraft-server"),
            &server_output,
        )?);

        for (index, resolved) in resolved_libraries.iter().enumerate() {
            match &resolved.source.input {
                Input::Published { jar, .. } => {
                    let analyses = resolved
                        .binaries
                        .iter()
                        .map(|binary| {
                            classfiles::analyze_jar(&binary.jar, minecraft.java_major_version)
                                .with_context(|| {
                                    format!(
                                        "failed to inspect binary coverage for {}",
                                        binary.origin.coordinate
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let runtime_overridden_sources = analyses
                        .iter()
                        .flat_map(|analysis| analysis.runtime_overridden_sources().iter().cloned())
                        .collect::<BTreeSet<_>>();
                    let published = scan_zip(
                        resolved.origin(),
                        resolved.artifact_path.clone(),
                        jar,
                        &resolved.input_sha256,
                        &runtime_overridden_sources,
                    )?;
                    let published_sources = package_source_paths(&published);
                    plans.push(published);

                    for (binary_index, (binary, analysis)) in
                        resolved.binaries.iter().zip(&analyses).enumerate()
                    {
                        let mut replacements = analysis
                            .expected_sources()
                            .difference(&published_sources)
                            .cloned()
                            .collect::<BTreeSet<_>>();
                        replacements.extend(analysis.runtime_overridden_sources().iter().cloned());
                        if replacements.is_empty() {
                            continue;
                        }

                        let normalized = work.join(format!(
                            "library-{index:04}-{binary_index:04}-supplement.jar"
                        ));
                        let expected = analysis.expected_decompiled_paths(Some(&replacements))?;
                        if expected.is_empty() {
                            bail!(
                                "binary {} reports missing sources but no decompiler outputs",
                                binary.origin.coordinate
                            );
                        }
                        validate_decompiled_expectations(&expected, &binary.origin.coordinate)?;
                        analysis
                            .write_normalized_jar(&normalized, Some(&replacements))
                            .with_context(|| {
                                format!(
                                    "failed to prepare missing classes from {}",
                                    binary.origin.coordinate
                                )
                            })?;
                        let decompiled =
                            work.join(format!("library-{index:04}-{binary_index:04}-supplement"));
                        let label = format!(
                            "runtime-selected or missing sources for {}",
                            binary.origin.coordinate
                        );
                        vineflower::decompile_to(
                            minecraft,
                            vineflower,
                            &normalized,
                            &decompiled,
                            expected.len(),
                            &label,
                        )?;
                        let supplement = scan_directory(
                            resolved.supplement_origin(binary, binary_index),
                            resolved.supplement_artifact_path(binary_index),
                            &decompiled,
                        )?;
                        verify_decompiled_paths(&supplement, &expected, &label)?;
                        plans.push(supplement);
                    }
                }
                Input::Decompiled { jar } => {
                    let [binary] = resolved.binaries.as_slice() else {
                        bail!(
                            "decompiled library source {} must have exactly one binary, got {}",
                            resolved.source.id,
                            resolved.binaries.len()
                        );
                    };
                    if binary.jar != *jar {
                        bail!(
                            "decompiled library input does not match its binary identity for {}",
                            resolved.source.id
                        );
                    }
                    let analysis = classfiles::analyze_jar(jar, minecraft.java_major_version)
                        .with_context(|| {
                            format!(
                                "failed to inspect binary classes for {}",
                                resolved.source.id
                            )
                        })?;
                    let expected = analysis.expected_decompiled_paths(None)?;
                    if expected.is_empty() {
                        plans.push(ArtifactPlan {
                            origin: resolved.origin(),
                            storage: Storage::Empty,
                            files: Vec::new(),
                        });
                        continue;
                    }
                    validate_decompiled_expectations(&expected, &resolved.source.id)?;
                    let normalized = work.join(format!("library-{index:04}-normalized.jar"));
                    analysis.write_normalized_jar(&normalized, None)?;
                    let decompiled = work.join(format!("library-{index:04}"));
                    vineflower::decompile_to(
                        minecraft,
                        vineflower,
                        &normalized,
                        &decompiled,
                        expected.len(),
                        &resolved.source.id,
                    )?;
                    let plan = scan_directory(
                        resolved.origin(),
                        resolved.artifact_path.clone(),
                        &decompiled,
                    )?;
                    verify_decompiled_paths(&plan, &expected, &resolved.source.id)?;
                    plans.push(plan);
                }
            }
        }

        plans.sort_by(|left, right| left.origin.id.cmp(&right.origin.id));
        preflight_outputs(&plans)?;

        fs::create_dir(&temporary).with_context(|| {
            format!(
                "failed to create temporary source output {}",
                temporary.display()
            )
        })?;
        temporary_created = true;
        create_stage_directory(&temporary, Path::new(CODE_DIRECTORY))?;
        create_stage_directory(&temporary, Path::new(ARTIFACTS_DIRECTORY))?;

        for plan in &plans {
            materialize_plan(plan, &temporary)?;
        }

        let inventory = build_inventory(
            minecraft_version,
            &input_sha256,
            &libraries.manifest_sha256,
            &vineflower_fingerprint,
            &plans,
        );
        write_inventory(&temporary, &inventory)?;
        let tree_sha256 = payload_tree_sha256(&temporary)?;
        artifacts::write_completion(&temporary, &input_sha256, &tree_sha256)?;
        let mut verified = verify_output(&temporary, minecraft_version, &input_sha256)?;

        remove_owned_directory(&parent, &work)?;
        work_created = false;
        artifacts::ensure_absent(&output, "source output")?;
        fs::rename(&temporary, &output).with_context(|| {
            format!(
                "failed to move completed sources {} to {}",
                temporary.display(),
                output.display()
            )
        })?;
        temporary_created = false;
        verified.path = output;
        Ok(verified)
    })();

    match result {
        Ok(verified) => Ok(verified),
        Err(mut error) => {
            if temporary_created && let Err(cleanup) = remove_owned_directory(&parent, &temporary) {
                error = error.context(format!(
                    "also failed to remove temporary source output {}: {cleanup}",
                    temporary.display()
                ));
            }
            if work_created && let Err(cleanup) = remove_owned_directory(&parent, &work) {
                error = error.context(format!(
                    "also failed to remove source work directory {}: {cleanup}",
                    work.display()
                ));
            }
            Err(error)
        }
    }
}

struct ResolvedLibrary<'a> {
    source: &'a libraries::Source,
    artifact_path: SafePath,
    input_sha256: String,
    binaries: Vec<ResolvedBinary>,
}

struct ResolvedBinary {
    origin: OriginBinary,
    jar: PathBuf,
}

impl ResolvedLibrary<'_> {
    fn origin(&self) -> OriginArtifact {
        let input = match &self.source.input {
            Input::Published {
                repository,
                url,
                checksum_algorithm,
                checksum,
                sha256,
                ..
            } => OriginInput::Published {
                repository: repository.clone(),
                url: url.clone(),
                checksum_algorithm: checksum_algorithm.clone(),
                checksum: checksum.clone(),
                sha256: sha256.clone(),
            },
            Input::Decompiled { .. } => OriginInput::Decompiled {
                sha256: self.input_sha256.clone(),
            },
        };
        OriginArtifact {
            id: self.source.id.clone(),
            artifact_path: self.artifact_path.text.clone(),
            binaries: self
                .binaries
                .iter()
                .map(|binary| binary.origin.clone())
                .collect(),
            input,
        }
    }

    fn supplement_origin(&self, binary: &ResolvedBinary, index: usize) -> OriginArtifact {
        OriginArtifact {
            id: format!("{}:vineflower-supplement:{index}", binary.origin.coordinate),
            artifact_path: self.supplement_artifact_path(index).text,
            binaries: vec![binary.origin.clone()],
            input: OriginInput::Decompiled {
                sha256: binary.origin.sha256.clone(),
            },
        }
    }

    fn supplement_artifact_path(&self, index: usize) -> SafePath {
        self.artifact_path.join(&SafePath::from_validated(&format!(
            "vineflower-supplement-{index:04}"
        )))
    }
}

fn resolve_libraries(prepared: &libraries::Prepared) -> Result<Vec<ResolvedLibrary<'_>>> {
    let mut sources = prepared.sources.iter().collect::<Vec<_>>();
    sources.sort_by(|left, right| left.id.cmp(&right.id));

    let mut ids = BTreeMap::new();
    let mut artifact_paths = BTreeMap::new();
    artifact_paths.insert(
        portable_key("minecraft-server"),
        "minecraft-server".to_owned(),
    );
    let mut resolved = Vec::with_capacity(sources.len());
    for source in sources {
        if source.id.is_empty() {
            bail!("library source has an empty id");
        }
        let id_key = portable_key(&source.id);
        if let Some(previous) = ids.insert(id_key, source.id.clone()) {
            bail!(
                "library source ids collide case-insensitively: {previous:?} and {:?}",
                source.id
            );
        }
        let artifact_path = safe_path_from_fs(&source.artifact_path, "library artifact path")?;
        if let Some(previous) =
            artifact_paths.insert(artifact_path.key.clone(), artifact_path.text.clone())
        {
            bail!(
                "library artifact paths collide case-insensitively: {previous:?} and {:?}",
                artifact_path.text
            );
        }

        if source.binaries.is_empty() {
            bail!("library source {:?} has no binary identities", source.id);
        }
        let mut binaries = source
            .binaries
            .iter()
            .map(|binary| -> Result<ResolvedBinary> {
                if binary.coordinate.is_empty() {
                    bail!(
                        "library source {:?} has an empty binary coordinate",
                        source.id
                    );
                }
                validate_sha256(&format!("binary {}", binary.coordinate), &binary.sha256)?;
                let actual_sha256 = download::file_sha256(&binary.jar).with_context(|| {
                    format!("failed to hash library binary {}", binary.jar.display())
                })?;
                if !actual_sha256.eq_ignore_ascii_case(&binary.sha256) {
                    bail!(
                        "library binary hash mismatch for {}: expected {}, got {actual_sha256}",
                        binary.coordinate,
                        binary.sha256
                    );
                }
                Ok(ResolvedBinary {
                    origin: OriginBinary {
                        coordinate: binary.coordinate.clone(),
                        sha256: binary.sha256.clone(),
                    },
                    jar: binary.jar.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        binaries.sort_by(|left, right| left.origin.coordinate.cmp(&right.origin.coordinate));
        for pair in binaries.windows(2) {
            if pair[0].origin.coordinate == pair[1].origin.coordinate {
                bail!(
                    "library source {:?} repeats binary coordinate {:?}",
                    source.id,
                    pair[0].origin.coordinate
                );
            }
        }

        let (input, expected_sha256) = match &source.input {
            Input::Published { jar, sha256, .. } => {
                validate_sha256(&format!("published source {}", source.id), sha256)?;
                (jar, Some(sha256.as_str()))
            }
            Input::Decompiled { jar } => (jar, None),
        };
        let input_sha256 = download::file_sha256(input)
            .with_context(|| format!("failed to hash library input {}", input.display()))?;
        if let Some(expected) = expected_sha256
            && !input_sha256.eq_ignore_ascii_case(expected)
        {
            bail!(
                "published source hash mismatch for {}: expected {expected}, got {input_sha256}",
                source.id
            );
        }
        resolved.push(ResolvedLibrary {
            source,
            artifact_path,
            input_sha256,
            binaries,
        });
    }
    Ok(resolved)
}

fn input_fingerprint(
    minecraft: &PreparedMinecraft,
    minecraft_version: &str,
    server_sha256: &str,
    manifest_sha256: &str,
    vineflower_fingerprint: &str,
    libraries: &[ResolvedLibrary<'_>],
) -> String {
    let mut hasher = Sha256::new();
    artifacts::hash_field(&mut hasher, INPUT_FINGERPRINT_VERSION);
    artifacts::hash_field(&mut hasher, minecraft_version);
    artifacts::hash_field(&mut hasher, &minecraft.java_major_version.to_string());
    artifacts::hash_field(&mut hasher, &minecraft.server_sha1);
    artifacts::hash_field(&mut hasher, &minecraft.runtime_manifest_sha1);
    artifacts::hash_field(&mut hasher, server_sha256);
    artifacts::hash_field(&mut hasher, manifest_sha256);
    artifacts::hash_field(&mut hasher, vineflower_fingerprint);
    for library in libraries {
        artifacts::hash_field(&mut hasher, &library.source.id);
        artifacts::hash_field(&mut hasher, &library.artifact_path.text);
        artifacts::hash_field(&mut hasher, &library.input_sha256);
        for binary in &library.binaries {
            artifacts::hash_field(&mut hasher, &binary.origin.coordinate);
            artifacts::hash_field(&mut hasher, &binary.origin.sha256);
        }
        match &library.source.input {
            Input::Published {
                repository,
                url,
                checksum_algorithm,
                checksum,
                sha256,
                ..
            } => {
                artifacts::hash_field(&mut hasher, "published");
                artifacts::hash_field(&mut hasher, repository);
                artifacts::hash_field(&mut hasher, url);
                artifacts::hash_field(&mut hasher, checksum_algorithm);
                artifacts::hash_field(&mut hasher, checksum);
                artifacts::hash_field(&mut hasher, sha256);
            }
            Input::Decompiled { .. } => artifacts::hash_field(&mut hasher, "decompiled"),
        }
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Clone)]
struct SafePath {
    text: String,
    path: PathBuf,
    key: String,
}

impl SafePath {
    fn from_validated(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            path: text.split('/').collect(),
            key: portable_key(text),
        }
    }

    fn join(&self, child: &SafePath) -> Self {
        let text = format!("{}/{}", self.text, child.text);
        let mut path = self.path.clone();
        path.push(&child.path);
        Self {
            key: portable_key(&text),
            text,
            path,
        }
    }
}

fn safe_archive_path(raw: &[u8], directory: bool, label: &str) -> Result<SafePath> {
    let raw =
        std::str::from_utf8(raw).with_context(|| format!("{label} is not a UTF-8 ZIP path"))?;
    let text = if directory {
        raw.strip_suffix('/').with_context(|| {
            format!("{label} is marked as a directory but does not end in '/': {raw:?}")
        })?
    } else {
        raw
    };
    safe_path_from_slashes(text, label)
}

fn safe_path_from_slashes(text: &str, label: &str) -> Result<SafePath> {
    if text.is_empty() || text.starts_with('/') || text.contains('\\') || text.contains('\0') {
        bail!("unsafe {label}: {text:?}");
    }
    let mut path = PathBuf::new();
    for component in text.split('/') {
        artifacts::validate_portable_component(component, label)?;
        path.push(component);
    }
    Ok(SafePath {
        text: text.to_owned(),
        path,
        key: portable_key(text),
    })
}

fn safe_payload_path(text: &str, label: &str) -> Result<SafePath> {
    let path = safe_path_from_slashes(text, label)?;
    if !(path.text.starts_with("code/") || path.text.starts_with("artifacts/")) {
        bail!("{label} is outside the source payload trees: {text:?}");
    }
    Ok(path)
}

pub(crate) fn payload_relative_path(text: &str, label: &str) -> Result<PathBuf> {
    Ok(safe_payload_path(text, label)?.path)
}

fn safe_path_from_fs(path: &Path, label: &str) -> Result<SafePath> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        bail!("unsafe {label}: {}", path.display());
    }
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(component) = component else {
            bail!("unsafe {label}: {}", path.display());
        };
        let component = component
            .to_str()
            .with_context(|| format!("{label} is not UTF-8: {}", path.display()))?;
        artifacts::validate_portable_component(component, label)?;
        components.push(component);
    }
    safe_path_from_slashes(&components.join("/"), label)
}

pub(crate) fn portable_relative_path(path: &Path, label: &str) -> Result<String> {
    Ok(safe_path_from_fs(path, label)?.text)
}

fn validate_single_component(label: &str, value: &str) -> Result<()> {
    let safe = safe_path_from_slashes(value, label)?;
    if safe.text.contains('/') {
        bail!("{label} must be a single path component: {value:?}");
    }
    Ok(())
}

pub(crate) fn portable_key(path: &str) -> String {
    path.chars().flat_map(char::to_lowercase).collect()
}

struct ArtifactPlan {
    origin: OriginArtifact,
    storage: Storage,
    files: Vec<PlannedFile>,
}

enum Storage {
    Zip { jar: PathBuf, sha256: String },
    Directory,
    Empty,
}

struct PlannedFile {
    output: SafePath,
    archive_path: SafePath,
    kind: FileKind,
    sha256: String,
    size: u64,
    content: Content,
}

enum Content {
    ZipEntry(usize),
    File(PathBuf),
}

fn scan_zip(
    origin: OriginArtifact,
    artifact_path: SafePath,
    jar: &Path,
    jar_sha256: &str,
    force_artifact_paths: &BTreeSet<String>,
) -> Result<ArtifactPlan> {
    let file = File::open(jar).with_context(|| format!("failed to open {}", jar.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("invalid published source jar {}", jar.display()))?;
    let mut seen = BTreeMap::new();
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect entry {index} in {}", jar.display()))?;
        let label = format!("entry {index} in {}", jar.display());
        let relative = safe_archive_path(entry.name_raw(), entry.is_dir(), &label)?;
        if let Some(previous) = seen.insert(relative.key.clone(), relative.text.clone()) {
            bail!(
                "published source jar {} contains duplicate or case-colliding paths {previous:?} and {:?}",
                jar.display(),
                relative.text
            );
        }
        validate_zip_entry_type(&entry, &relative.text, jar)?;
        if entry.is_dir() {
            continue;
        }

        let kind = classify(&relative);
        let package_source =
            kind.is_package_source() && !force_artifact_paths.contains(&relative.text);
        let output = output_path(&artifact_path, &relative, package_source);
        let (sha256, size) = hash_reader(&mut entry, &label)?;
        files.push(PlannedFile {
            output,
            archive_path: relative,
            kind,
            sha256,
            size,
            content: Content::ZipEntry(index),
        });
    }
    Ok(ArtifactPlan {
        origin,
        storage: Storage::Zip {
            jar: jar.to_path_buf(),
            sha256: jar_sha256.to_owned(),
        },
        files,
    })
}

fn validate_zip_entry_type<R: Read>(
    entry: &zip::read::ZipFile<'_, R>,
    relative: &str,
    jar: &Path,
) -> Result<()> {
    if entry.is_symlink() {
        bail!(
            "published source jar {} contains symbolic link {relative:?}",
            jar.display()
        );
    }
    if let Some(mode) = entry.unix_mode() {
        let file_type = mode & 0o170000;
        let expected = if entry.is_dir() { 0o040000 } else { 0o100000 };
        if file_type != 0 && file_type != expected {
            bail!(
                "published source jar {} contains special entry {relative:?} with Unix mode {mode:#o}",
                jar.display()
            );
        }
    }
    if !entry.is_dir() && !entry.is_file() {
        bail!(
            "published source jar {} contains non-file entry {relative:?}",
            jar.display()
        );
    }
    Ok(())
}

fn scan_directory(
    origin: OriginArtifact,
    artifact_path: SafePath,
    root: &Path,
) -> Result<ArtifactPlan> {
    let metadata = fs::symlink_metadata(root)
        .with_context(|| format!("failed to inspect source directory {}", root.display()))?;
    if !metadata.file_type().is_dir() {
        bail!(
            "source directory is not a regular directory: {}",
            root.display()
        );
    }
    let mut files = Vec::new();
    let mut seen = BTreeMap::new();
    scan_directory_entries(root, root, &artifact_path, &mut seen, &mut files)?;
    Ok(ArtifactPlan {
        origin,
        storage: Storage::Directory,
        files,
    })
}

fn package_source_paths(plan: &ArtifactPlan) -> BTreeSet<String> {
    plan.files
        .iter()
        .filter(|file| file.kind.is_package_source())
        .map(|file| file.archive_path.text.clone())
        .collect()
}

fn validate_decompiled_expectations(expected: &BTreeSet<String>, label: &str) -> Result<()> {
    let mut portable_paths = BTreeMap::new();
    for path in expected {
        let safe = safe_path_from_slashes(path, "expected decompiled source path")?;
        let file_name = safe.text.rsplit('/').next().unwrap_or_default();
        if !(file_name.ends_with(".java") || file_name.ends_with(".kt"))
            || file_name.eq_ignore_ascii_case("module-info.java")
        {
            bail!("invalid expected Vineflower output for {label}: {path:?}");
        }
        if let Some(previous) = portable_paths.insert(safe.key, safe.text.clone()) {
            bail!(
                "expected Vineflower outputs for {label} collide case-insensitively: {previous:?} and {:?}",
                safe.text
            );
        }
    }
    Ok(())
}

fn verify_decompiled_paths(
    plan: &ArtifactPlan,
    expected: &BTreeSet<String>,
    label: &str,
) -> Result<()> {
    let actual = plan
        .files
        .iter()
        .filter(|file| {
            let file_name = file
                .archive_path
                .text
                .rsplit('/')
                .next()
                .unwrap_or_default();
            (file_name.ends_with(".java") || file_name.ends_with(".kt"))
                && !file_name.eq_ignore_ascii_case("module-info.java")
        })
        .map(|file| file.archive_path.text.clone())
        .collect::<BTreeSet<_>>();
    if actual != *expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unexpected = actual.difference(expected).cloned().collect::<Vec<_>>();
        bail!(
            "Vineflower output paths do not match binary SourceFile metadata for {label}; missing: {missing:?}; unexpected: {unexpected:?}"
        );
    }
    Ok(())
}

fn scan_directory_entries(
    root: &Path,
    directory: &Path,
    artifact_path: &SafePath,
    seen: &mut BTreeMap<String, String>,
    files: &mut Vec<PlannedFile>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read source directory {}", directory.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read source directory {}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).with_context(|| {
            format!(
                "source path {} is outside {}",
                path.display(),
                root.display()
            )
        })?;
        let relative = safe_path_from_fs(relative, "decompiled source path")?;
        if let Some(previous) = seen.insert(relative.key.clone(), relative.text.clone()) {
            bail!(
                "decompiled source directory {} contains duplicate or case-colliding paths {previous:?} and {:?}",
                root.display(),
                relative.text
            );
        }
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_symlink() {
            bail!(
                "decompiled source contains symbolic link: {}",
                path.display()
            );
        }
        if file_type.is_dir() {
            scan_directory_entries(root, &path, artifact_path, seen, files)?;
        } else if file_type.is_file() {
            let kind = classify(&relative);
            let output = output_path(artifact_path, &relative, kind.is_package_source());
            let size = entry
                .metadata()
                .with_context(|| format!("failed to inspect {}", path.display()))?
                .len();
            let sha256 = download::file_sha256(&path)
                .with_context(|| format!("failed to hash {}", path.display()))?;
            files.push(PlannedFile {
                output,
                archive_path: relative,
                kind,
                sha256,
                size,
                content: Content::File(path),
            });
        } else {
            bail!(
                "decompiled source contains special file: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn output_path(artifact: &SafePath, relative: &SafePath, package_source: bool) -> SafePath {
    if package_source {
        SafePath::from_validated(CODE_DIRECTORY).join(relative)
    } else {
        SafePath::from_validated(ARTIFACTS_DIRECTORY)
            .join(artifact)
            .join(relative)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FileKind {
    Java,
    Kotlin,
    PackageInfo,
    ModuleInfo,
    License,
    Artifact,
}

impl FileKind {
    fn is_package_source(self) -> bool {
        matches!(self, Self::Java | Self::Kotlin | Self::PackageInfo)
    }
}

fn classify(relative: &SafePath) -> FileKind {
    let mut components = relative.text.split('/');
    let first = components.next().unwrap_or_default();
    let file_name = relative.text.rsplit('/').next().unwrap_or_default();
    let has_parent = relative.text.contains('/');
    let in_meta_inf = first.eq_ignore_ascii_case("META-INF");

    if file_name.eq_ignore_ascii_case("module-info.java") {
        FileKind::ModuleInfo
    } else if file_name.eq_ignore_ascii_case("package-info.java") && has_parent && !in_meta_inf {
        FileKind::PackageInfo
    } else if file_name.ends_with(".java") && has_parent && !in_meta_inf {
        FileKind::Java
    } else if file_name.ends_with(".kt") && has_parent && !in_meta_inf {
        FileKind::Kotlin
    } else if is_license_file(file_name) {
        FileKind::License
    } else {
        FileKind::Artifact
    }
}

fn is_license_file(file_name: &str) -> bool {
    let upper = file_name.to_ascii_uppercase();
    ["LICENSE", "NOTICE", "COPYING", "COPYRIGHT", "DEPENDENCIES"]
        .iter()
        .any(|prefix| upper == *prefix || upper.starts_with(&format!("{prefix}.")))
}

fn preflight_outputs(plans: &[ArtifactPlan]) -> Result<()> {
    let mut paths = OutputPaths::default();
    for plan in plans {
        for file in &plan.files {
            paths.insert(&file.output, &plan.origin.id)?;
        }
    }
    Ok(())
}

#[derive(Default)]
struct OutputPaths {
    files: BTreeMap<String, PathOwner>,
    directories: BTreeMap<String, PathOwner>,
}

#[derive(Clone)]
struct PathOwner {
    path: String,
    owner: String,
}

impl OutputPaths {
    fn insert(&mut self, path: &SafePath, owner: &str) -> Result<()> {
        if let Some(previous) = self.files.get(&path.key) {
            bail!(
                "source output collision between {} path {:?} and {owner} path {:?}",
                previous.owner,
                previous.path,
                path.text
            );
        }
        if let Some(previous) = self.directories.get(&path.key) {
            bail!(
                "source output file {:?} from {owner} conflicts with directory {:?} from {}",
                path.text,
                previous.path,
                previous.owner
            );
        }

        let components = path.text.split('/').collect::<Vec<_>>();
        for length in 1..components.len() {
            let ancestor = components[..length].join("/");
            let key = portable_key(&ancestor);
            if let Some(previous) = self.files.get(&key) {
                bail!(
                    "source output {:?} from {owner} is nested beneath file {:?} from {}",
                    path.text,
                    previous.path,
                    previous.owner
                );
            }
            if let Some(previous) = self.directories.get(&key) {
                if previous.path != ancestor {
                    bail!(
                        "source output directories collide case-insensitively: {:?} from {} and {ancestor:?} from {owner}",
                        previous.path,
                        previous.owner
                    );
                }
            } else {
                self.directories.insert(
                    key,
                    PathOwner {
                        path: ancestor,
                        owner: owner.to_owned(),
                    },
                );
            }
        }
        self.files.insert(
            path.key.clone(),
            PathOwner {
                path: path.text.clone(),
                owner: owner.to_owned(),
            },
        );
        Ok(())
    }
}

fn materialize_plan(plan: &ArtifactPlan, stage: &Path) -> Result<()> {
    match &plan.storage {
        Storage::Zip { jar, sha256 } => {
            let actual = download::file_sha256(jar)
                .with_context(|| format!("failed to hash {} before extraction", jar.display()))?;
            if actual != *sha256 {
                bail!(
                    "source jar changed after preflight: {} (expected {sha256}, got {actual})",
                    jar.display()
                );
            }
            let file =
                File::open(jar).with_context(|| format!("failed to open {}", jar.display()))?;
            let mut archive = ZipArchive::new(file)
                .with_context(|| format!("invalid published source jar {}", jar.display()))?;
            for planned in &plan.files {
                let Content::ZipEntry(index) = planned.content else {
                    bail!("internal source plan mismatch for {}", plan.origin.id);
                };
                let mut entry = archive.by_index(index).with_context(|| {
                    format!("failed to reopen entry {index} in {}", jar.display())
                })?;
                let relative = safe_archive_path(
                    entry.name_raw(),
                    entry.is_dir(),
                    &format!("entry {index} in {}", jar.display()),
                )?;
                if relative.text != planned.archive_path.text || entry.is_dir() {
                    bail!(
                        "source jar changed after preflight at entry {index}: {}",
                        jar.display()
                    );
                }
                write_planned_file(&mut entry, planned, stage)?;
            }
        }
        Storage::Directory => {
            for planned in &plan.files {
                let Content::File(input) = &planned.content else {
                    bail!("internal source plan mismatch for {}", plan.origin.id);
                };
                let metadata = fs::symlink_metadata(input)
                    .with_context(|| format!("failed to inspect {}", input.display()))?;
                if !metadata.file_type().is_file() {
                    bail!("source input changed after preflight: {}", input.display());
                }
                let mut file = File::open(input)
                    .with_context(|| format!("failed to open {}", input.display()))?;
                write_planned_file(&mut file, planned, stage)?;
            }
        }
        Storage::Empty => {
            if !plan.files.is_empty() {
                bail!(
                    "internal empty source plan contains files for {}",
                    plan.origin.id
                );
            }
        }
    }
    Ok(())
}

fn write_planned_file(input: &mut impl Read, planned: &PlannedFile, stage: &Path) -> Result<()> {
    let destination = stage.join(&planned.output.path);
    let parent = destination
        .parent()
        .with_context(|| format!("source output has no parent: {}", destination.display()))?;
    let relative_parent = parent.strip_prefix(stage).with_context(|| {
        format!(
            "source output {} is outside {}",
            destination.display(),
            stage.display()
        )
    })?;
    create_stage_directory(stage, relative_parent)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .with_context(|| format!("failed to create source file {}", destination.display()))?;
    let (sha256, size) = artifacts::copy_and_hash(input, &mut output, &destination, "source file")?;
    output
        .flush()
        .with_context(|| format!("failed to flush source file {}", destination.display()))?;
    if sha256 != planned.sha256 || size != planned.size {
        bail!(
            "source input changed after preflight for {}: expected {} bytes/{}, got {size} bytes/{sha256}",
            planned.archive_path.text,
            planned.size,
            planned.sha256
        );
    }
    Ok(())
}

fn create_stage_directory(stage: &Path, relative: &Path) -> Result<()> {
    let relative = safe_path_from_fs(relative, "source output directory")?;
    let mut current = stage.to_path_buf();
    for component in relative.path.components() {
        let Component::Normal(component) = component else {
            bail!("invalid source output directory: {}", relative.text);
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => bail!(
                "source output path component is not a regular directory: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!(
                        "failed to create source output directory {}",
                        current.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect source output directory {}",
                        current.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn hash_reader(reader: &mut impl Read, label: &str) -> Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {label}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size = size
            .checked_add(read as u64)
            .context("source file size overflow")?;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origins {
    schema_version: u32,
    minecraft_version: String,
    input_sha256: String,
    library_manifest_sha256: String,
    vineflower_fingerprint: String,
    artifacts: Vec<OriginArtifact>,
    files: Vec<OriginFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OriginArtifact {
    id: String,
    artifact_path: String,
    binaries: Vec<OriginBinary>,
    input: OriginInput,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OriginBinary {
    coordinate: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OriginInput {
    Published {
        repository: String,
        url: String,
        checksum_algorithm: String,
        checksum: String,
        sha256: String,
    },
    Decompiled {
        sha256: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginFile {
    path: String,
    artifact_id: String,
    archive_path: String,
    kind: FileKind,
    sha256: String,
    size: u64,
}

impl Origins {
    pub(crate) fn minecraft_version(&self) -> &str {
        &self.minecraft_version
    }

    pub(crate) fn input_sha256(&self) -> &str {
        &self.input_sha256
    }

    pub(crate) fn vineflower_fingerprint(&self) -> &str {
        &self.vineflower_fingerprint
    }

    pub(crate) fn files(&self) -> &[OriginFile] {
        &self.files
    }
}

impl OriginFile {
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn kind(&self) -> FileKind {
        self.kind
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    #[cfg(test)]
    pub(crate) fn for_test(path: &str, kind: FileKind, sha256: String, size: u64) -> Self {
        Self {
            path: path.to_owned(),
            artifact_id: "artifact:1".to_owned(),
            archive_path: path.strip_prefix("code/").unwrap_or(path).to_owned(),
            kind,
            sha256,
            size,
        }
    }
}

fn build_inventory(
    minecraft_version: &str,
    input_sha256: &str,
    manifest_sha256: &str,
    vineflower_fingerprint: &str,
    plans: &[ArtifactPlan],
) -> Origins {
    let mut artifacts = plans
        .iter()
        .map(|plan| plan.origin.clone())
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.id.cmp(&right.id));
    let mut files = plans
        .iter()
        .flat_map(|plan| {
            plan.files.iter().map(|file| OriginFile {
                path: file.output.text.clone(),
                artifact_id: plan.origin.id.clone(),
                archive_path: file.archive_path.text.clone(),
                kind: file.kind,
                sha256: file.sha256.clone(),
                size: file.size,
            })
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Origins {
        schema_version: INVENTORY_SCHEMA_VERSION,
        minecraft_version: minecraft_version.to_owned(),
        input_sha256: input_sha256.to_owned(),
        library_manifest_sha256: manifest_sha256.to_owned(),
        vineflower_fingerprint: vineflower_fingerprint.to_owned(),
        artifacts,
        files,
    }
}

fn write_inventory(output: &Path, inventory: &Origins) -> Result<()> {
    let path = output.join(INVENTORY_FILE);
    let mut bytes = serde_json::to_vec_pretty(inventory).context("failed to serialize origins")?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create source inventory {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("failed to write source inventory {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush source inventory {}", path.display()))
}

pub(crate) struct VerifiedSources {
    path: PathBuf,
    inventory: Origins,
    tree_sha256: String,
}

impl VerifiedSources {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn inventory(&self) -> &Origins {
        &self.inventory
    }

    pub(crate) fn tree_sha256(&self) -> &str {
        &self.tree_sha256
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        path: PathBuf,
        minecraft_version: &str,
        input_sha256: String,
        tree_sha256: String,
        vineflower_fingerprint: String,
        files: Vec<OriginFile>,
    ) -> Self {
        Self {
            path,
            inventory: Origins {
                schema_version: INVENTORY_SCHEMA_VERSION,
                minecraft_version: minecraft_version.to_owned(),
                input_sha256,
                library_manifest_sha256: "0".repeat(64),
                vineflower_fingerprint,
                artifacts: vec![OriginArtifact {
                    id: "artifact:1".to_owned(),
                    artifact_path: "artifact".to_owned(),
                    binaries: vec![OriginBinary {
                        coordinate: "artifact:1".to_owned(),
                        sha256: "0".repeat(64),
                    }],
                    input: OriginInput::Decompiled {
                        sha256: "0".repeat(64),
                    },
                }],
                files,
            },
            tree_sha256,
        }
    }
}

fn verify_output(
    output: &Path,
    minecraft_version: &str,
    input_sha256: &str,
) -> Result<VerifiedSources> {
    verify_output_shape(output)?;
    let inventory_path = output.join(INVENTORY_FILE);
    let inventory_file = File::open(&inventory_path).with_context(|| {
        format!(
            "failed to open source inventory {}",
            inventory_path.display()
        )
    })?;
    let inventory: Origins = serde_json::from_reader(BufReader::new(inventory_file))
        .with_context(|| format!("invalid source inventory {}", inventory_path.display()))?;
    verify_inventory(&inventory, minecraft_version, input_sha256)?;

    let actual_files = payload_files(output)?;
    let expected_files = inventory
        .files
        .iter()
        .map(|file| (file.path.clone(), (file.sha256.clone(), file.size)))
        .collect::<BTreeMap<_, _>>();
    if actual_files != expected_files {
        bail!(
            "source output files do not match {}: remove {} before retrying",
            inventory_path.display(),
            output.display()
        );
    }

    let tree_sha256 = payload_tree_sha256(output)?;
    let completion_path = output.join(artifacts::COMPLETION_FILE);
    let actual_completion = fs::read_to_string(&completion_path).with_context(|| {
        format!(
            "failed to read completion record {}",
            completion_path.display()
        )
    })?;
    let expected_completion = artifacts::completion_record(input_sha256, &tree_sha256);
    if actual_completion != expected_completion {
        bail!(
            "source output does not match its completion record: {}; remove it before retrying",
            output.display()
        );
    }
    Ok(VerifiedSources {
        path: output.to_owned(),
        inventory,
        tree_sha256,
    })
}

fn verify_output_shape(output: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(output)
        .with_context(|| format!("failed to inspect source output {}", output.display()))?;
    if !metadata.file_type().is_dir() {
        bail!(
            "source output is not a regular directory: {}",
            output.display()
        );
    }
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(output)
        .with_context(|| format!("failed to read source output {}", output.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read source output {}", output.display()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("source output contains a non-UTF-8 root entry"))?;
        names.insert(name);
    }
    let expected = [
        artifacts::COMPLETION_FILE.to_owned(),
        INVENTORY_FILE.to_owned(),
        ARTIFACTS_DIRECTORY.to_owned(),
        CODE_DIRECTORY.to_owned(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if names != expected {
        bail!(
            "source output has unexpected root entries: {}; remove it before retrying",
            output.display()
        );
    }
    artifacts::require_directory(&output.join(CODE_DIRECTORY), "source directory")?;
    artifacts::require_directory(&output.join(ARTIFACTS_DIRECTORY), "source directory")?;
    artifacts::require_file(&output.join(INVENTORY_FILE), "source file")?;
    artifacts::require_file(&output.join(artifacts::COMPLETION_FILE), "source file")
}

fn verify_inventory(
    inventory: &Origins,
    minecraft_version: &str,
    input_sha256: &str,
) -> Result<()> {
    if inventory.schema_version != INVENTORY_SCHEMA_VERSION {
        bail!(
            "unsupported source inventory schema version {}",
            inventory.schema_version
        );
    }
    if inventory.minecraft_version != minecraft_version {
        bail!(
            "source inventory Minecraft version mismatch: expected {minecraft_version:?}, got {:?}",
            inventory.minecraft_version
        );
    }
    if inventory.input_sha256 != input_sha256 {
        bail!("source inventory does not match the current inputs");
    }
    validate_sha256("inventory input", &inventory.input_sha256)?;
    validate_sha256(
        "inventory library manifest",
        &inventory.library_manifest_sha256,
    )?;
    validate_sha256(
        "inventory Vineflower fingerprint",
        &inventory.vineflower_fingerprint,
    )?;

    let mut previous_artifact = None;
    let mut artifact_ids = BTreeSet::new();
    let mut artifact_paths = BTreeMap::new();
    for artifact in &inventory.artifacts {
        if previous_artifact.is_some_and(|previous: &str| previous >= artifact.id.as_str()) {
            bail!("source inventory artifacts are not strictly sorted");
        }
        previous_artifact = Some(&artifact.id);
        if !artifact_ids.insert(artifact.id.as_str()) {
            bail!("source inventory repeats artifact id {:?}", artifact.id);
        }
        let path = safe_path_from_slashes(&artifact.artifact_path, "inventory artifact path")?;
        if let Some(previous) = artifact_paths.insert(path.key, path.text.clone()) {
            bail!(
                "source inventory artifact paths collide case-insensitively: {previous:?} and {:?}",
                path.text
            );
        }
        let mut previous_binary = None;
        for binary in &artifact.binaries {
            if previous_binary.is_some_and(|previous: &str| previous >= binary.coordinate.as_str())
            {
                bail!("source inventory binary identities are not strictly sorted");
            }
            previous_binary = Some(&binary.coordinate);
            validate_sha256(&format!("binary {}", binary.coordinate), &binary.sha256)?;
        }
        match &artifact.input {
            OriginInput::Published { sha256, .. } | OriginInput::Decompiled { sha256 } => {
                validate_sha256(&format!("artifact {} input", artifact.id), sha256)?;
            }
        }
    }

    let mut previous_file = None;
    let mut paths = OutputPaths::default();
    for file in &inventory.files {
        if previous_file.is_some_and(|previous: &str| previous >= file.path.as_str()) {
            bail!("source inventory files are not strictly sorted");
        }
        previous_file = Some(&file.path);
        if !artifact_ids.contains(file.artifact_id.as_str()) {
            bail!(
                "source inventory file {:?} refers to unknown artifact {:?}",
                file.path,
                file.artifact_id
            );
        }
        let path = safe_payload_path(&file.path, "inventory output path")?;
        safe_path_from_slashes(&file.archive_path, "inventory archive path")?;
        validate_sha256(&format!("source file {}", file.path), &file.sha256)?;
        paths.insert(&path, &file.artifact_id)?;
    }
    Ok(())
}

fn payload_files(output: &Path) -> Result<BTreeMap<String, (String, u64)>> {
    let mut files = BTreeMap::new();
    for root_name in [CODE_DIRECTORY, ARTIFACTS_DIRECTORY] {
        let root = output.join(root_name);
        collect_payload_files(output, &root, &mut files)?;
    }
    Ok(files)
}

fn collect_payload_files(
    output: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, (String, u64)>,
) -> Result<()> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read source output {}", directory.display()))?;
    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read source output {}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_symlink() {
            bail!("source output contains symbolic link: {}", path.display());
        }
        if file_type.is_dir() {
            collect_payload_files(output, &path, files)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(output).with_context(|| {
                format!(
                    "source output path {} is outside {}",
                    path.display(),
                    output.display()
                )
            })?;
            let relative = safe_path_from_fs(relative, "source output path")?;
            let size = entry
                .metadata()
                .with_context(|| format!("failed to inspect {}", path.display()))?
                .len();
            let sha256 = download::file_sha256(&path)
                .with_context(|| format!("failed to hash {}", path.display()))?;
            if files
                .insert(relative.text.clone(), (sha256, size))
                .is_some()
            {
                bail!("source output repeats path {:?}", relative.text);
            }
        } else {
            bail!("source output contains special file: {}", path.display());
        }
    }
    Ok(())
}

enum TreeEntry {
    Directory(SafePath),
    File(SafePath, String, u64),
}

fn payload_tree_sha256(output: &Path) -> Result<String> {
    let mut entries = Vec::new();
    collect_tree_entries(output, output, &mut entries)?;
    entries.sort_by(|left, right| tree_entry_path(left).cmp(tree_entry_path(right)));
    let mut hasher = Sha256::new();
    artifacts::hash_field(&mut hasher, TREE_FINGERPRINT_VERSION);
    for entry in entries {
        match entry {
            TreeEntry::Directory(path) => {
                artifacts::hash_field(&mut hasher, "directory");
                artifacts::hash_field(&mut hasher, &path.text);
            }
            TreeEntry::File(path, sha256, size) => {
                artifacts::hash_field(&mut hasher, "file");
                artifacts::hash_field(&mut hasher, &path.text);
                hasher.update(size.to_be_bytes());
                artifacts::hash_field(&mut hasher, &sha256);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn tree_entry_path(entry: &TreeEntry) -> &str {
    match entry {
        TreeEntry::Directory(path) | TreeEntry::File(path, ..) => &path.text,
    }
}

fn collect_tree_entries(root: &Path, directory: &Path, entries: &mut Vec<TreeEntry>) -> Result<()> {
    let children = fs::read_dir(directory)
        .with_context(|| format!("failed to read source tree {}", directory.display()))?;
    for child in children {
        let child =
            child.with_context(|| format!("failed to read source tree {}", directory.display()))?;
        let path = child.path();
        let relative = path.strip_prefix(root).with_context(|| {
            format!(
                "source tree path {} is outside {}",
                path.display(),
                root.display()
            )
        })?;
        let relative = safe_path_from_fs(relative, "source tree path")?;
        if relative.text == artifacts::COMPLETION_FILE {
            continue;
        }
        let file_type = child
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_symlink() {
            bail!("source tree contains symbolic link: {}", path.display());
        }
        if file_type.is_dir() {
            entries.push(TreeEntry::Directory(relative));
            collect_tree_entries(root, &path, entries)?;
        } else if file_type.is_file() {
            let size = child
                .metadata()
                .with_context(|| format!("failed to inspect {}", path.display()))?
                .len();
            let sha256 = download::file_sha256(&path)
                .with_context(|| format!("failed to hash {}", path.display()))?;
            entries.push(TreeEntry::File(relative, sha256, size));
        } else {
            bail!("source tree contains special file: {}", path.display());
        }
    }
    Ok(())
}

fn remove_owned_directory(parent: &Path, directory: &Path) -> Result<()> {
    if directory.parent() != Some(parent)
        || !matches!(directory.file_name(), Some(name) if name == "sources.part" || name == "sources.work")
    {
        bail!(
            "refusing to remove unowned source directory {}",
            directory.display()
        );
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_dir() => fs::remove_dir_all(directory)
            .with_context(|| format!("failed to remove source directory {}", directory.display())),
        Ok(_) => bail!(
            "refusing to remove non-directory source path {}",
            directory.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .with_context(|| format!("failed to inspect source directory {}", directory.display())),
    }
}

fn validate_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid SHA-256 for {label}: {value:?}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_paths_reject_unsafe_and_windows_specific_names() {
        for path in [
            "",
            "/absolute",
            "../outside",
            "a/../outside",
            "a/./b",
            "a//b",
            "a\\b",
            "C:/outside",
            "file:stream",
            "a/trailing.",
            "a/trailing ",
            "a/NUL.java",
            "a/COM1.txt",
        ] {
            assert!(
                safe_path_from_slashes(path, "test path").is_err(),
                "accepted {path:?}"
            );
        }
        assert!(safe_path_from_slashes("org/example/Valid.java", "test path").is_ok());
    }

    #[test]
    fn package_sources_and_artifacts_are_classified_without_parsing_source_text() {
        let java = safe_path_from_slashes("org/example/Main.java", "test").unwrap();
        let kotlin = safe_path_from_slashes("org/example/Main.kt", "test").unwrap();
        let package_info = safe_path_from_slashes("org/example/package-info.java", "test").unwrap();
        let module_info = safe_path_from_slashes("module-info.java", "test").unwrap();
        let versioned_module =
            safe_path_from_slashes("META-INF/versions/9/module-info.java", "test").unwrap();
        let root_source = safe_path_from_slashes("Root.java", "test").unwrap();

        assert_eq!(classify(&java), FileKind::Java);
        assert_eq!(classify(&kotlin), FileKind::Kotlin);
        assert_eq!(classify(&package_info), FileKind::PackageInfo);
        assert!(classify(&java).is_package_source());
        assert!(classify(&kotlin).is_package_source());
        assert!(classify(&package_info).is_package_source());
        assert_eq!(classify(&module_info), FileKind::ModuleInfo);
        assert_eq!(classify(&versioned_module), FileKind::ModuleInfo);
        assert!(!classify(&module_info).is_package_source());
        assert!(!classify(&versioned_module).is_package_source());
        assert_eq!(classify(&root_source), FileKind::Artifact);
        assert!(!classify(&root_source).is_package_source());
    }

    #[test]
    fn output_preflight_rejects_duplicates_case_collisions_and_file_prefixes() {
        let mut paths = OutputPaths::default();
        let first = safe_path_from_slashes("code/org/example/Main.java", "test").unwrap();
        paths.insert(&first, "first").unwrap();

        let duplicate = safe_path_from_slashes("code/org/example/Main.java", "test").unwrap();
        assert!(paths.insert(&duplicate, "second").is_err());

        let case_collision = safe_path_from_slashes("code/org/example/main.java", "test").unwrap();
        assert!(paths.insert(&case_collision, "second").is_err());

        let directory_case_collision =
            safe_path_from_slashes("code/ORG/other/Other.java", "test").unwrap();
        assert!(paths.insert(&directory_case_collision, "second").is_err());

        let mut prefix_paths = OutputPaths::default();
        let child = safe_path_from_slashes("artifacts/a/b/file", "test").unwrap();
        prefix_paths.insert(&child, "child").unwrap();
        let parent_file = safe_path_from_slashes("artifacts/a/b", "test").unwrap();
        assert!(prefix_paths.insert(&parent_file, "parent").is_err());
    }
}
