use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use zip::{ZipArchive, result::ZipError};

use crate::download::{Integrity, verify_file};

const VERSIONS_LIST: &str = "META-INF/versions.list";

pub fn decompiler_input(
    server_jar: &Path,
    version_id: &str,
    minecraft_cache: &Path,
) -> Result<PathBuf> {
    let file = File::open(server_jar)
        .with_context(|| format!("failed to open server jar {}", server_jar.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("invalid server jar {}", server_jar.display()))?;
    let mut versions = String::new();
    match archive.by_name(VERSIONS_LIST) {
        Ok(mut entry) => entry
            .read_to_string(&mut versions)
            .with_context(|| format!("{VERSIONS_LIST} is not valid UTF-8"))?,
        Err(ZipError::FileNotFound) => return Ok(server_jar.to_path_buf()),
        Err(error) => return Err(error).context("failed to read the server bundler manifest"),
    };

    let mut matching = Vec::new();
    for (line_index, line) in versions.lines().filter(|line| !line.is_empty()).enumerate() {
        let columns = line.split('\t').collect::<Vec<_>>();
        let [sha256, id, relative] = columns.as_slice() else {
            bail!(
                "invalid {VERSIONS_LIST} line {}: expected three tab-separated fields",
                line_index + 1
            );
        };
        validate_sha256(sha256)?;
        validate_archive_path(relative)?;
        if *id == version_id {
            matching.push((*sha256, *relative));
        }
    }
    let (sha256, relative) = match matching.as_slice() {
        [entry] => *entry,
        [] => bail!("server bundler does not contain a runtime jar for Minecraft {version_id}"),
        _ => bail!("server bundler contains multiple runtime jars for Minecraft {version_id}"),
    };

    let destination = minecraft_cache.join("server-classes.jar");
    let integrity = Integrity::sha256(sha256);
    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                bail!(
                    "server runtime cache is not a regular file: {}",
                    destination.display()
                );
            }
            verify_file(&destination, integrity).with_context(|| {
                format!(
                    "cached server runtime jar failed integrity verification: {}; remove it before retrying",
                    destination.display()
                )
            })?;
            return Ok(destination);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect server runtime cache {}",
                    destination.display()
                )
            });
        }
    }

    eprintln!("Extracting Minecraft runtime jar");
    let cache_metadata = fs::symlink_metadata(minecraft_cache).with_context(|| {
        format!(
            "failed to inspect Minecraft cache {}",
            minecraft_cache.display()
        )
    })?;
    if !cache_metadata.file_type().is_dir() {
        bail!(
            "Minecraft cache is not a regular directory: {}",
            minecraft_cache.display()
        );
    }
    let temporary = minecraft_cache.join("server-classes.jar.part");
    match fs::symlink_metadata(&temporary) {
        Ok(_) => bail!(
            "incomplete or concurrent extraction exists: {}; remove it after confirming no other worldless-dev process is running",
            temporary.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect temporary extraction {}",
                    temporary.display()
                )
            });
        }
    }
    let temporary_output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("failed to claim extraction {}", temporary.display()))?;
    let archive_name = format!("META-INF/versions/{relative}");
    let extraction = (|| -> Result<()> {
        let mut entry = archive.by_name(&archive_name).with_context(|| {
            format!("server bundler is missing declared entry {archive_name:?}")
        })?;
        let mut output = temporary_output;
        std::io::copy(&mut entry, &mut output)
            .with_context(|| format!("failed to extract {archive_name:?}"))?;
        output
            .flush()
            .with_context(|| format!("failed to flush {}", temporary.display()))?;
        drop(output);
        verify_file(&temporary, integrity)
    })();
    if let Err(error) = extraction {
        return match fs::remove_file(&temporary) {
            Ok(()) => Err(error),
            Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
            Err(cleanup) => Err(error.context(format!(
                "also failed to remove incomplete extraction {}: {cleanup}",
                temporary.display()
            ))),
        };
    }
    fs::rename(&temporary, &destination).with_context(|| {
        format!(
            "failed to move extracted runtime jar to {}",
            destination.display()
        )
    })?;
    Ok(destination)
}

pub fn expected_source_count(server_jar: &Path) -> Result<usize> {
    let count = top_level_class_count(server_jar)?;
    if count == 0 {
        bail!(
            "server jar contains no top-level class files: {}",
            server_jar.display()
        );
    }
    Ok(count)
}

pub fn top_level_class_count(jar: &Path) -> Result<usize> {
    let file = File::open(jar).with_context(|| format!("failed to open jar {}", jar.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("invalid jar {}", jar.display()))?;
    let mut count = 0;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to inspect entry {index} in {}", jar.display()))?;
        let name = entry.name();
        let Some(file_name) = name.rsplit('/').next() else {
            continue;
        };
        if let Some(class_name) = file_name.strip_suffix(".class")
            && class_name != "module-info"
            && !class_name.contains('$')
        {
            count += 1;
        }
    }
    Ok(count)
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid SHA-256 in {VERSIONS_LIST}: {value:?}");
    }
    Ok(())
}

fn validate_archive_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid runtime jar path in {VERSIONS_LIST}: {value:?}");
    }
    Ok(())
}
