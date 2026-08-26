use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::{StatusCode, blocking::Client};
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};
use sha2::Sha256;

#[derive(Clone, Copy)]
enum Checksum<'a> {
    Sha1(&'a str),
    Sha256(&'a str),
}

#[derive(Clone, Copy)]
pub struct Integrity<'a> {
    pub size: Option<u64>,
    checksum: Option<Checksum<'a>>,
}

impl<'a> Integrity<'a> {
    pub const fn sha1_and_size(sha1: &'a str, size: u64) -> Self {
        Self {
            size: Some(size),
            checksum: Some(Checksum::Sha1(sha1)),
        }
    }

    pub const fn sha1(sha1: &'a str) -> Self {
        Self {
            size: None,
            checksum: Some(Checksum::Sha1(sha1)),
        }
    }

    pub const fn sha256(sha256: &'a str) -> Self {
        Self {
            size: None,
            checksum: Some(Checksum::Sha256(sha256)),
        }
    }
}

#[derive(Clone)]
pub struct Http {
    client: Client,
}

impl Http {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent(concat!("worldless-dev/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self { client })
    }

    pub fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url)?;
        serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON from {url}"))
    }

    pub fn get_text(&self, url: &str) -> Result<String> {
        let bytes = self.get_bytes(url)?;
        String::from_utf8(bytes).with_context(|| format!("response is not UTF-8: {url}"))
    }

    pub fn get_optional_text(&self, url: &str) -> Result<Option<String>> {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("request failed: {url}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response
            .error_for_status()
            .with_context(|| format!("server returned an error: {url}"))?;
        let bytes = response
            .bytes()
            .with_context(|| format!("failed to read response: {url}"))?;
        String::from_utf8(bytes.to_vec())
            .with_context(|| format!("response is not UTF-8: {url}"))
            .map(Some)
    }

    pub fn get_verified_json<T: DeserializeOwned>(
        &self,
        url: &str,
        integrity: Integrity<'_>,
    ) -> Result<T> {
        let bytes = self.get_bytes(url)?;
        verify_bytes(&bytes, integrity, url)?;
        serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON from {url}"))
    }

    pub fn ensure_file(
        &self,
        url: &str,
        destination: &Path,
        integrity: Integrity<'_>,
    ) -> Result<()> {
        match fs::symlink_metadata(destination) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    bail!(
                        "download destination exists but is not a regular file: {}",
                        destination.display()
                    );
                }
                verify_file(destination, integrity).with_context(|| {
                    format!(
                        "cached download failed integrity verification: {}; remove it before retrying",
                        destination.display()
                    )
                })?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect download cache {}", destination.display())
                });
            }
        }

        let parent = destination.parent().with_context(|| {
            format!(
                "download destination has no parent: {}",
                destination.display()
            )
        })?;
        let parent_metadata = fs::symlink_metadata(parent).with_context(|| {
            format!("failed to inspect download directory {}", parent.display())
        })?;
        if !parent_metadata.file_type().is_dir() {
            bail!(
                "download directory is not a regular directory: {}",
                parent.display()
            );
        }
        let file_name = destination
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("invalid destination file name: {}", destination.display()))?;
        let temporary = destination.with_file_name(format!("{file_name}.part"));
        match fs::symlink_metadata(&temporary) {
            Ok(_) => {
                bail!(
                    "incomplete or concurrent download exists: {}; remove it after confirming no other worldless-dev process is running",
                    temporary.display()
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect temporary download {}",
                        temporary.display()
                    )
                });
            }
        }

        let temporary_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("failed to claim download {}", temporary.display()))?;
        let result = self.download_to(url, &temporary, temporary_file, integrity);
        if let Err(error) = result {
            return match fs::remove_file(&temporary) {
                Ok(()) => Err(error),
                Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => Err(error),
                Err(cleanup) => Err(error.context(format!(
                    "also failed to remove incomplete download {}: {cleanup}",
                    temporary.display()
                ))),
            };
        }

        fs::rename(&temporary, destination).with_context(|| {
            format!(
                "failed to move completed download {} to {}",
                temporary.display(),
                destination.display()
            )
        })?;
        Ok(())
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("request failed: {url}"))?
            .error_for_status()
            .with_context(|| format!("server returned an error: {url}"))?;
        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .with_context(|| format!("failed to read response: {url}"))
    }

    fn download_to(
        &self,
        url: &str,
        destination: &Path,
        mut file: File,
        integrity: Integrity<'_>,
    ) -> Result<()> {
        let mut response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("request failed: {url}"))?
            .error_for_status()
            .with_context(|| format!("server returned an error: {url}"))?;
        std::io::copy(&mut response, &mut file)
            .with_context(|| format!("failed to download {url}"))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", destination.display()))?;
        drop(file);
        verify_file(destination, integrity)
            .with_context(|| format!("downloaded file failed verification: {url}"))
    }
}

pub fn verify_file(path: &Path, integrity: Integrity<'_>) -> Result<()> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let size = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .len();
    if let Some(expected) = integrity.size
        && size != expected
    {
        bail!(
            "size mismatch for {}: expected {expected}, got {size}",
            path.display()
        );
    }
    if let Some(checksum) = integrity.checksum {
        let mut reader = BufReader::new(file);
        match checksum {
            Checksum::Sha1(expected) => {
                let actual = hash_reader::<Sha1>(&mut reader, path)?;
                if actual != expected {
                    bail!(
                        "SHA-1 mismatch for {}: expected {expected}, got {actual}",
                        path.display()
                    );
                }
            }
            Checksum::Sha256(expected) => {
                let actual = hash_reader::<Sha256>(&mut reader, path)?;
                if actual != expected {
                    bail!(
                        "SHA-256 mismatch for {}: expected {expected}, got {actual}",
                        path.display()
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = BufReader::new(
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    );
    hash_reader::<Sha256>(&mut file, path)
}

fn verify_bytes(bytes: &[u8], integrity: Integrity<'_>, source: &str) -> Result<()> {
    if let Some(expected) = integrity.size
        && bytes.len() as u64 != expected
    {
        bail!(
            "size mismatch for {source}: expected {expected}, got {}",
            bytes.len()
        );
    }
    if let Some(checksum) = integrity.checksum {
        match checksum {
            Checksum::Sha1(expected) => {
                let actual = format!("{:x}", Sha1::digest(bytes));
                if actual != expected {
                    bail!("SHA-1 mismatch for {source}: expected {expected}, got {actual}");
                }
            }
            Checksum::Sha256(expected) => {
                let actual = format!("{:x}", Sha256::digest(bytes));
                if actual != expected {
                    bail!("SHA-256 mismatch for {source}: expected {expected}, got {actual}");
                }
            }
        }
    }
    Ok(())
}

fn hash_reader<D: Digest + Default>(reader: &mut impl Read, path: &Path) -> Result<String> {
    let mut hasher = D::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(hex(digest.as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    result
}
