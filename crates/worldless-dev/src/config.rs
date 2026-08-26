use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::artifacts;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub target_minecraft_version_id: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        validate_minecraft_version_id(
            "target_minecraft_version_id",
            &config.target_minecraft_version_id,
        )?;
        Ok(config)
    }
}

pub(crate) fn validate_minecraft_version_id(name: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{name} must not be empty");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!("{name} must contain only ASCII letters, digits, '.', '_' or '-': {value:?}");
    }
    artifacts::validate_portable_component(value, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minecraft_version_ids_are_portable_path_components() {
        for value in ["1.21.8", "26.3-snapshot-10", "version_id"] {
            assert!(validate_minecraft_version_id("version", value).is_ok());
        }
        for value in [
            "",
            ".",
            "..",
            "26/3",
            "26\\3",
            "26:3",
            "snapshot 10",
            "版本",
            "CON",
            "NUL.txt",
            "COM1",
            "version.",
        ] {
            assert!(
                validate_minecraft_version_id("version", value).is_err(),
                "accepted {value:?}"
            );
        }
    }

    #[test]
    fn config_accepts_only_the_target_field_name() {
        let config: Config =
            toml::from_str("target_minecraft_version_id = \"26.3-snapshot-10\"\n").unwrap();
        assert_eq!(config.target_minecraft_version_id, "26.3-snapshot-10");
        assert!(toml::from_str::<Config>("minecraft_version_id = \"1.0\"\n").is_err());
    }
}
