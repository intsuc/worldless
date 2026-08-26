use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

pub const COMPLETION_FILE: &str = ".worldless-complete";

pub fn ensure_root(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => bail!(
            "artifact root is not a regular directory: {}",
            root.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(root)
            .with_context(|| format!("failed to create artifact root {}", root.display())),
        Err(error) => Err(error)
            .with_context(|| format!("failed to inspect artifact root {}", root.display())),
    }
}

pub fn ensure_directory(root: &Path, directory: &Path) -> Result<()> {
    ensure_root(root)?;
    let relative = directory.strip_prefix(root).with_context(|| {
        format!(
            "artifact directory {} is outside {}",
            directory.display(),
            root.display()
        )
    })?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            bail!("invalid artifact directory: {}", directory.display());
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => bail!(
                "artifact path component is not a regular directory: {}",
                current.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).with_context(|| {
                    format!("failed to create artifact directory {}", current.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect artifact directory {}", current.display())
                });
            }
        }
    }
    Ok(())
}

pub fn validate_portable_component(component: &str, label: &str) -> Result<()> {
    if component.is_empty() || component == "." || component == ".." {
        bail!("unsafe {label} component: {component:?}");
    }
    if component.ends_with('.') || component.ends_with(' ') {
        bail!("Windows-unsafe {label} component: {component:?}");
    }
    if component.chars().any(|character| {
        character <= '\u{1f}' || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
    }) {
        bail!("Windows-unsafe {label} component: {component:?}");
    }
    let base = component.split('.').next().unwrap_or(component);
    let upper = base.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || upper.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
    {
        bail!("Windows-reserved {label} component: {component:?}");
    }
    Ok(())
}

pub fn hash_field(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

pub fn copy_and_hash(
    input: &mut impl Read,
    output: &mut impl Write,
    destination: &Path,
    label: &str,
) -> Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .with_context(|| format!("failed to read {label} for {}", destination.display()))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .with_context(|| format!("failed to write {label} {}", destination.display()))?;
        hasher.update(&buffer[..read]);
        size = size
            .checked_add(read as u64)
            .with_context(|| format!("{label} size overflow"))?;
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

pub fn write_completion(output: &Path, input_sha256: &str, tree_sha256: &str) -> Result<()> {
    let path = output.join(COMPLETION_FILE);
    let record = completion_record(input_sha256, tree_sha256);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| format!("failed to create completion record {}", path.display()))?;
    file.write_all(record.as_bytes())
        .with_context(|| format!("failed to write completion record {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush completion record {}", path.display()))
}

pub fn completion_record(input_sha256: &str, tree_sha256: &str) -> String {
    format!("input-sha256={input_sha256}\ntree-sha256={tree_sha256}\n")
}

pub fn ensure_absent(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => bail!(
            "{label} already exists: {}; remove it after confirming no other worldless-dev process is running",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect {label} {}", path.display()))
        }
    }
}

pub fn require_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("required {label} is missing: {}", path.display()))?;
    if !metadata.file_type().is_dir() {
        bail!("{label} is not a regular directory: {}", path.display());
    }
    Ok(())
}

pub fn require_file(path: &Path, label: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("required {label} is missing: {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("{label} is not a regular file: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_record_is_exact_and_deterministic() {
        let input = "1".repeat(64);
        let tree = "2".repeat(64);
        assert_eq!(
            completion_record(&input, &tree),
            format!("input-sha256={input}\ntree-sha256={tree}\n")
        );
    }
}
