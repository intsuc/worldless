use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde::Deserialize;

use crate::{
    artifacts,
    download::{Http, Integrity},
};

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const JAVA_RUNTIME_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

pub struct PreparedMinecraft {
    pub server_jar: PathBuf,
    pub java: PathBuf,
    pub java_major_version: u32,
    pub server_sha1: String,
    pub runtime_manifest_sha1: String,
}

pub fn prepare(
    http: &Http,
    version_id: &str,
    required_java: u32,
    cache_root: &Path,
) -> Result<PreparedMinecraft> {
    eprintln!("Resolving Minecraft {version_id}");
    let version_manifest: VersionManifest = http.get_json(VERSION_MANIFEST_URL)?;
    let matches = version_manifest
        .versions
        .iter()
        .filter(|version| version.id == version_id)
        .collect::<Vec<_>>();
    let version = match matches.as_slice() {
        [version] => *version,
        [] => bail!("Minecraft version id {version_id:?} is not present in Mojang's manifest"),
        _ => bail!("Mojang's manifest contains duplicate version id {version_id:?}"),
    };
    validate_sha1("Minecraft metadata SHA-1", &version.sha1)?;
    validate_https_url("Minecraft metadata URL", &version.url)?;
    let details: VersionDetails =
        http.get_verified_json(&version.url, Integrity::sha1(&version.sha1))?;
    if details.id != version_id {
        bail!(
            "Minecraft metadata id mismatch: requested {version_id:?}, received {:?}",
            details.id
        );
    }
    let server = details.downloads.server.with_context(|| {
        format!("Minecraft {version_id} metadata does not provide a server jar")
    })?;
    validate_sha1("server jar SHA-1", &server.sha1)?;
    validate_https_url("server jar URL", &server.url)?;
    let java_version = details.java_version.with_context(|| {
        format!("Minecraft {version_id} metadata does not specify a Java runtime")
    })?;
    if java_version.major_version < required_java {
        bail!(
            "Minecraft {version_id} uses Java {}, but the fixed Vineflower requires Java {required_java} or newer",
            java_version.major_version
        );
    }

    let platform = RuntimePlatform::current()?;
    eprintln!(
        "Preparing Mojang Java {} ({}, {})",
        java_version.major_version, java_version.component, platform.key
    );
    let runtime_index: RuntimeIndex = http.get_json(JAVA_RUNTIME_INDEX_URL)?;
    let releases = runtime_index
        .get(platform.key)
        .with_context(|| format!("Mojang has no Java runtime metadata for {}", platform.key))?
        .get(&java_version.component)
        .with_context(|| {
            format!(
                "Mojang has no {} runtime metadata for {}",
                java_version.component, platform.key
            )
        })?;
    let release = match releases.as_slice() {
        [release] => release,
        [] => bail!(
            "Mojang does not provide {} for {}",
            java_version.component,
            platform.key
        ),
        _ => bail!(
            "Mojang provided multiple {} runtimes for {}; refusing to guess which is the launcher default",
            java_version.component,
            platform.key
        ),
    };
    validate_sha1("runtime manifest SHA-1", &release.manifest.sha1)?;
    validate_https_url("runtime manifest URL", &release.manifest.url)?;
    validate_single_component("runtime component", &java_version.component)?;
    let runtime_manifest: RuntimeManifest = http.get_verified_json(
        &release.manifest.url,
        Integrity::sha1_and_size(&release.manifest.sha1, release.manifest.size),
    )?;
    preflight_runtime_manifest(&runtime_manifest)?;

    let minecraft_cache = cache_root.join("minecraft").join(version_id);
    artifacts::ensure_directory(cache_root, &minecraft_cache)?;
    let server_jar = minecraft_cache.join("server.jar");
    eprintln!("Preparing Minecraft server jar");
    http.ensure_file(
        &server.url,
        &server_jar,
        Integrity::sha1_and_size(&server.sha1, server.size),
    )?;

    let runtime_root = cache_root
        .join("java")
        .join(&java_version.component)
        .join(platform.key)
        .join(&release.manifest.sha1);
    artifacts::ensure_directory(cache_root, &runtime_root)?;
    install_runtime(http, &runtime_manifest, &runtime_root)?;
    let java = runtime_root.join(platform.java_path);
    match fs::symlink_metadata(&java) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => bail!(
            "Mojang Java executable is not a regular file: {}",
            java.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => bail!(
            "Mojang runtime is missing its Java executable: {}",
            java.display()
        ),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect Mojang Java {}", java.display()));
        }
    }

    Ok(PreparedMinecraft {
        server_jar,
        java,
        java_major_version: java_version.major_version,
        server_sha1: server.sha1,
        runtime_manifest_sha1: release.manifest.sha1.clone(),
    })
}

fn install_runtime(http: &Http, manifest: &RuntimeManifest, root: &Path) -> Result<()> {
    artifacts::ensure_root(root)?;

    for (relative, entry) in &manifest.files {
        if matches!(entry, RuntimeFile::Directory) {
            let path = manifest_path(root, relative)?;
            artifacts::ensure_directory(root, &path)?;
        }
    }

    let files = manifest
        .files
        .iter()
        .filter_map(|(relative, entry)| match entry {
            RuntimeFile::File {
                downloads,
                executable,
            } => Some((relative, &downloads.raw, *executable)),
            RuntimeFile::Directory | RuntimeFile::Link { .. } => None,
        })
        .collect::<Vec<_>>();
    for (relative, _, _) in &files {
        let destination = manifest_path(root, relative)?;
        let parent = destination.parent().with_context(|| {
            format!(
                "runtime file has no parent directory: {}",
                destination.display()
            )
        })?;
        artifacts::ensure_directory(root, parent)?;
    }
    files
        .par_iter()
        .try_for_each(|(relative, download, executable)| -> Result<()> {
            let destination = manifest_path(root, relative)?;
            http.ensure_file(
                &download.url,
                &destination,
                Integrity::sha1_and_size(&download.sha1, download.size),
            )?;
            set_executable(&destination, *executable)
        })?;

    for (relative, entry) in &manifest.files {
        if let RuntimeFile::Link { target } = entry {
            let link = manifest_path(root, relative)?;
            let parent = link.parent().with_context(|| {
                format!("runtime link has no parent directory: {}", link.display())
            })?;
            artifacts::ensure_directory(root, parent)?;
            let resolved = root.join(normalize_link_target(relative, target)?);
            let target_metadata = fs::symlink_metadata(&resolved).with_context(|| {
                format!(
                    "runtime link {} has no regular target at {}",
                    link.display(),
                    resolved.display()
                )
            })?;
            if !target_metadata.file_type().is_file() && !target_metadata.file_type().is_dir() {
                bail!(
                    "runtime link {} targets another link or special file: {}",
                    link.display(),
                    resolved.display()
                );
            }
            ensure_symlink(&link, Path::new(target))?;
        }
    }
    Ok(())
}

fn preflight_runtime_manifest(manifest: &RuntimeManifest) -> Result<()> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum PathKind {
        Directory,
        File,
        Link,
    }

    let mut manifest_paths = HashMap::with_capacity(manifest.files.len());
    for (relative, entry) in &manifest.files {
        let path = manifest_relative_path(relative)?;
        let kind = match entry {
            RuntimeFile::File { downloads, .. } => {
                validate_sha1("runtime file SHA-1", &downloads.raw.sha1)?;
                validate_https_url("runtime file URL", &downloads.raw.url)?;
                PathKind::File
            }
            RuntimeFile::Link { .. } => PathKind::Link,
            RuntimeFile::Directory => PathKind::Directory,
        };
        if manifest_paths.insert(path.clone(), kind).is_some() {
            bail!(
                "Mojang runtime manifest contains multiple entries resolving to {:?}",
                path
            );
        }
    }

    for path in manifest_paths.keys() {
        let mut ancestor = path.parent();
        while let Some(parent) = ancestor.filter(|parent| !parent.as_os_str().is_empty()) {
            if let Some(kind) = manifest_paths.get(parent)
                && *kind != PathKind::Directory
            {
                bail!(
                    "runtime manifest path {:?} is nested beneath non-directory {:?}",
                    path,
                    parent
                );
            }
            ancestor = parent.parent();
        }
    }

    for (relative, entry) in &manifest.files {
        if let RuntimeFile::Link { target } = entry {
            let resolved = normalize_link_target(relative, target)?;
            let target_kind = manifest_paths.get(&resolved).with_context(|| {
                format!(
                    "runtime link {relative:?} targets a path absent from the manifest: {target:?}"
                )
            })?;
            if *target_kind == PathKind::Link {
                bail!("runtime link {relative:?} targets another runtime link: {target:?}");
            }
        }
    }
    Ok(())
}

fn manifest_path(root: &Path, relative: &str) -> Result<PathBuf> {
    Ok(root.join(manifest_relative_path(relative)?))
}

fn manifest_relative_path(relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid path in Mojang runtime manifest: {relative:?}");
    }
    Ok(path.to_path_buf())
}

fn normalize_link_target(link: &str, target: &str) -> Result<PathBuf> {
    let target_path = Path::new(target);
    if target.is_empty() || target_path.is_absolute() {
        bail!("invalid target {target:?} for runtime link {link:?}");
    }
    let mut resolved = manifest_relative_path(link)?
        .parent()
        .context("runtime link has no parent")?
        .to_path_buf();
    for component in target_path.components() {
        match component {
            Component::Normal(component) => resolved.push(component),
            Component::ParentDir if resolved.pop() => {}
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                bail!("runtime link {link:?} points outside its runtime: {target:?}")
            }
        }
    }
    if resolved.as_os_str().is_empty() {
        bail!("runtime link {link:?} targets the runtime root");
    }
    Ok(resolved)
}

#[cfg(unix)]
fn ensure_symlink(link: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    match fs::read_link(link) {
        Ok(existing) if existing == target => return Ok(()),
        Ok(existing) => bail!(
            "runtime link {} points to {:?}, expected {:?}",
            link.display(),
            existing,
            target
        ),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error)
                .with_context(|| format!("failed to inspect runtime link {}", link.display()));
        }
        Err(_) => {}
    }
    symlink(target, link)
        .with_context(|| format!("failed to create runtime link {}", link.display()))
}

#[cfg(windows)]
fn ensure_symlink(link: &Path, target: &Path) -> Result<()> {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    match fs::read_link(link) {
        Ok(existing) if existing == target => return Ok(()),
        Ok(existing) => bail!(
            "runtime link {} points to {:?}, expected {:?}",
            link.display(),
            existing,
            target
        ),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(error)
                .with_context(|| format!("failed to inspect runtime link {}", link.display()));
        }
        Err(_) => {}
    }
    let resolved_target = link
        .parent()
        .context("runtime link has no parent")?
        .join(target);
    if resolved_target.is_dir() {
        symlink_dir(target, link)
    } else if resolved_target.is_file() {
        symlink_file(target, link)
    } else {
        bail!(
            "runtime link target does not exist for {}: {}",
            link.display(),
            resolved_target.display()
        );
    }
    .with_context(|| format!("failed to create runtime link {}", link.display()))
}

#[cfg(not(any(unix, windows)))]
fn ensure_symlink(_link: &Path, _target: &Path) -> Result<()> {
    bail!("runtime links are not supported on this operating system")
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect runtime file {}", path.display()))?;
    let mut permissions = metadata.permissions();
    let current = permissions.mode();
    let desired = if executable {
        current | 0o111
    } else {
        current & !0o111
    };
    if desired != current {
        permissions.set_mode(desired);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("failed to set runtime permissions on {}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

fn validate_single_component(label: &str, value: &str) -> Result<()> {
    let mut components = Path::new(value).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        bail!("invalid {label} in Mojang metadata: {value:?}");
    }
    Ok(())
}

fn validate_sha1(label: &str, value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid {label} in Mojang metadata: {value:?}");
    }
    Ok(())
}

fn validate_https_url(label: &str, value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value)
        .with_context(|| format!("invalid {label} in Mojang metadata: {value:?}"))?;
    if url.scheme() != "https" {
        bail!("non-HTTPS {label} in Mojang metadata: {value:?}");
    }
    Ok(())
}

struct RuntimePlatform {
    key: &'static str,
    java_path: &'static str,
}

impl RuntimePlatform {
    fn current() -> Result<Self> {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Self {
                key: "windows-x64",
                java_path: "bin/java.exe",
            },
            ("windows", "x86") => Self {
                key: "windows-x86",
                java_path: "bin/java.exe",
            },
            ("windows", "aarch64") => Self {
                key: "windows-arm64",
                java_path: "bin/java.exe",
            },
            ("macos", "x86_64") => Self {
                key: "mac-os",
                java_path: "jre.bundle/Contents/Home/bin/java",
            },
            ("macos", "aarch64") => Self {
                key: "mac-os-arm64",
                java_path: "jre.bundle/Contents/Home/bin/java",
            },
            ("linux", "x86_64") => Self {
                key: "linux",
                java_path: "bin/java",
            },
            ("linux", "x86") => Self {
                key: "linux-i386",
                java_path: "bin/java",
            },
            (os, arch) => bail!(
                "Mojang does not publish a launcher Java runtime for operating system {os:?} and architecture {arch:?}"
            ),
        };
        Ok(platform)
    }
}

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<VersionReference>,
}

#[derive(Deserialize)]
struct VersionReference {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Deserialize)]
struct VersionDetails {
    id: String,
    downloads: MinecraftDownloads,
    #[serde(rename = "javaVersion")]
    java_version: Option<JavaVersion>,
}

#[derive(Deserialize)]
struct MinecraftDownloads {
    server: Option<Download>,
}

#[derive(Deserialize)]
struct JavaVersion {
    component: String,
    #[serde(rename = "majorVersion")]
    major_version: u32,
}

type RuntimeIndex = HashMap<String, HashMap<String, Vec<RuntimeRelease>>>;

#[derive(Deserialize)]
struct RuntimeRelease {
    manifest: Download,
}

#[derive(Deserialize)]
struct Download {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Deserialize)]
struct RuntimeManifest {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    Directory,
    File {
        downloads: RuntimeDownloads,
        executable: bool,
    },
    Link {
        target: String,
    },
}

#[derive(Deserialize)]
struct RuntimeDownloads {
    raw: Download,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_link_target_stays_within_runtime() {
        let resolved =
            normalize_link_target("legal/java.compiler/LICENSE", "../java.base/LICENSE").unwrap();
        assert_eq!(
            resolved,
            Path::new("legal").join("java.base").join("LICENSE")
        );
        assert!(normalize_link_target("bin/java", "../../outside").is_err());
    }
}
