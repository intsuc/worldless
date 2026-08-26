mod artifacts;
mod classfiles;
mod comparison;
mod config;
mod download;
mod libraries;
mod mojang;
mod server;
mod sources;
mod vineflower;

use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result, bail};

use config::{Config, validate_minecraft_version_id};
use download::Http;

const USAGE: &str = "usage: worldless-dev generate-target\n       worldless-dev compare <from-version-id> <to-version-id>";

#[derive(Debug, Eq, PartialEq)]
enum CliCommand {
    GenerateTarget,
    Compare { from: String, to: String },
}

enum Operation {
    Generate { version: String },
    Compare { from: String, to: String },
}

fn main() -> Result<()> {
    let command = parse_args(std::env::args_os().skip(1))?;
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .context("worldless-dev must remain under <repository>/crates/worldless-dev")?;
    let operation = match command {
        CliCommand::GenerateTarget => Operation::Generate {
            version: Config::load(&repository_root.join("worldless.toml"))?
                .target_minecraft_version_id,
        },
        CliCommand::Compare { from, to } => Operation::Compare { from, to },
    };
    let artifacts = repository_root.join(".worldless");
    let cache = artifacts.join("cache");
    let generated = artifacts.join("generated");
    artifacts::ensure_root(&artifacts)?;
    artifacts::ensure_directory(&artifacts, &cache)?;
    artifacts::ensure_directory(&artifacts, &generated)?;
    let http = Http::new()?;
    let vineflower = vineflower::download(&http, &cache)?;

    match operation {
        Operation::Generate { version } => {
            let sources = prepare_version(&http, &vineflower, &version, &cache, &generated)?;
            println!("Minecraft sources: {}", sources.path().display());
        }
        Operation::Compare { from, to } => {
            let from_sources = prepare_version(&http, &vineflower, &from, &cache, &generated)?;
            let to_sources = prepare_version(&http, &vineflower, &to, &cache, &generated)?;
            let comparison = comparison::generate(&from_sources, &to_sources, &generated)?;
            println!("Minecraft source comparison: {}", comparison.display());
        }
    }
    Ok(())
}

fn prepare_version(
    http: &Http,
    vineflower: &vineflower::Downloaded,
    minecraft_version_id: &str,
    cache: &Path,
    generated: &Path,
) -> Result<sources::VerifiedSources> {
    let minecraft = mojang::prepare(http, minecraft_version_id, vineflower::MINIMUM_JAVA, cache)?;
    let minecraft_cache = cache.join("minecraft").join(minecraft_version_id);
    let server_input = server::decompiler_input(
        &minecraft.server_jar,
        minecraft_version_id,
        &minecraft_cache,
    )?;
    let libraries = libraries::prepare(http, &minecraft.server_jar, &minecraft_cache, cache)?;
    sources::generate(
        &minecraft,
        vineflower,
        &server_input,
        &libraries,
        minecraft_version_id,
        generated,
    )
}

fn parse_args(arguments: impl IntoIterator<Item = OsString>) -> Result<CliCommand> {
    let arguments = arguments
        .into_iter()
        .enumerate()
        .map(|(index, argument)| {
            argument
                .into_string()
                .map_err(|_| anyhow::anyhow!("argument {} is not valid UTF-8\n{USAGE}", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;

    match arguments.as_slice() {
        [command] if command == "generate-target" => Ok(CliCommand::GenerateTarget),
        [command, from, to] if command == "compare" => {
            validate_minecraft_version_id("from-version-id", from)?;
            validate_minecraft_version_id("to-version-id", to)?;
            if from == to {
                bail!("compare requires two different version ids\n{USAGE}");
            }
            Ok(CliCommand::Compare {
                from: from.clone(),
                to: to.clone(),
            })
        }
        [] => bail!("missing command\n{USAGE}"),
        [command, ..] if command == "generate-target" => {
            bail!("generate-target takes no arguments\n{USAGE}")
        }
        [command, ..] if command == "compare" => {
            bail!("compare requires exactly two version ids\n{USAGE}")
        }
        [command, ..] => bail!("unknown command {command:?}\n{USAGE}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<CliCommand> {
        parse_args(arguments.iter().copied().map(OsString::from))
    }

    #[test]
    fn cli_accepts_only_the_exact_commands() {
        assert_eq!(
            parse(&["generate-target"]).unwrap(),
            CliCommand::GenerateTarget
        );
        assert_eq!(
            parse(&["compare", "26.3-snapshot-9", "26.3-snapshot-10"]).unwrap(),
            CliCommand::Compare {
                from: "26.3-snapshot-9".to_owned(),
                to: "26.3-snapshot-10".to_owned(),
            }
        );
    }

    #[test]
    fn cli_rejects_unknown_commands_and_wrong_arity() {
        for arguments in [
            &[][..],
            &["generate-target", "extra"],
            &["compare"],
            &["compare", "from"],
            &["compare", "from", "to", "extra"],
            &["generate"],
        ] {
            assert!(parse(arguments).is_err(), "accepted {arguments:?}");
        }
    }

    #[test]
    fn compare_rejects_invalid_version_ids() {
        assert!(parse(&["compare", "../from", "to"]).is_err());
        assert!(parse(&["compare", "from", "to/version"]).is_err());
        assert!(parse(&["compare", "same", "same"]).is_err());
    }
}
