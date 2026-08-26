use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::{
    artifacts,
    download::{Http, Integrity},
    mojang::PreparedMinecraft,
};

pub const VERSION: &str = "1.12.0";
pub const MINIMUM_JAVA: u32 = 17;
const OPTIONS: &[&str] = &[
    "--folder",
    "--include-runtime=current",
    "--log-level=warn",
    "--skip-extra-files=1",
];

pub struct Downloaded {
    pub jar: PathBuf,
    sha256: String,
}

pub fn download(http: &Http, cache_root: &Path) -> Result<Downloaded> {
    eprintln!("Preparing Vineflower {VERSION}");
    let artifact_url = format!(
        "https://repo.maven.apache.org/maven2/org/vineflower/vineflower/{VERSION}/vineflower-{VERSION}.jar"
    );
    let checksum_url = format!("{artifact_url}.sha256");
    let sha256 = http.get_text(&checksum_url)?;
    let sha256 = sha256.trim();
    validate_sha256(sha256)?;
    let destination = cache_root
        .join("vineflower")
        .join(VERSION)
        .join("vineflower.jar");
    artifacts::ensure_directory(
        cache_root,
        destination
            .parent()
            .context("Vineflower cache file has no parent")?,
    )?;
    http.ensure_file(&artifact_url, &destination, Integrity::sha256(sha256))?;
    Ok(Downloaded {
        jar: destination,
        sha256: sha256.to_owned(),
    })
}

pub fn fingerprint(vineflower: &Downloaded) -> String {
    let mut hasher = Sha256::new();
    for value in ["worldless-dev-vineflower-v1", VERSION, &vineflower.sha256] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    for option in OPTIONS {
        hasher.update(option.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn decompile_to(
    minecraft: &PreparedMinecraft,
    vineflower: &Downloaded,
    input_jar: &Path,
    output: &Path,
    expected_sources: usize,
    label: &str,
) -> Result<()> {
    let parent = output
        .parent()
        .context("decompiler output directory has no parent")?;
    let parent_metadata = fs::symlink_metadata(parent).with_context(|| {
        format!(
            "failed to inspect decompiler output parent {}",
            parent.display()
        )
    })?;
    if !parent_metadata.file_type().is_dir() {
        bail!(
            "decompiler output parent is not a regular directory: {}",
            parent.display()
        );
    }
    match fs::symlink_metadata(output) {
        Ok(_) => bail!("decompiler output already exists: {}", output.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to inspect decompiler output {}", output.display())
            });
        }
    }
    fs::create_dir(output)
        .with_context(|| format!("failed to create decompiler output {}", output.display()))?;
    if expected_sources == 0 {
        return Ok(());
    }
    eprintln!("Decompiling {label}");
    let mut command = Command::new(&minecraft.java);
    command.arg("-jar").arg(&vineflower.jar);
    for option in OPTIONS {
        command.arg(option);
    }
    let status = command
        .arg(input_jar)
        .arg(output)
        .status()
        .with_context(|| {
            format!(
                "failed to start Mojang Java at {}",
                minecraft.java.display()
            )
        })?;
    if !status.success() {
        bail!(
            "Vineflower {VERSION} failed with {status} while decompiling {label} using {}",
            minecraft.java.display()
        );
    }
    let actual = count_sources(output)?;
    if actual != expected_sources {
        bail!(
            "Vineflower produced {actual} Java/Kotlin files while decompiling {label}; expected {expected_sources} from {}",
            input_jar.display()
        );
    }
    Ok(())
}

fn count_sources(directory: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read decompiler output {}", directory.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read decompiler output {}", directory.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if file_type.is_dir() {
            count += count_sources(&entry.path())?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "java" || extension == "kt")
            && entry.file_name() != "module-info.java"
        {
            count += 1;
        } else if !file_type.is_file() {
            bail!(
                "decompiler output is not a regular file or directory: {}",
                entry.path().display()
            );
        }
    }
    Ok(count)
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid SHA-256 in Vineflower release metadata: {value:?}");
    }
    Ok(())
}
