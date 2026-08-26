use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

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
