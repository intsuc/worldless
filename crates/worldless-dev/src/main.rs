mod artifacts;
mod classfiles;
mod config;
mod download;
mod libraries;
mod mojang;
mod server;
mod sources;
mod vineflower;

use std::path::Path;

use anyhow::{Context, Result};

use config::Config;
use download::Http;

fn main() -> Result<()> {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .context("worldless-dev must remain under <repository>/crates/worldless-dev")?;
    let config = Config::load(&repository_root.join("worldless.toml"))?;
    let artifacts = repository_root.join(".worldless");
    let cache = artifacts.join("cache");
    let generated = artifacts.join("generated");
    artifacts::ensure_root(&artifacts)?;
    artifacts::ensure_directory(&artifacts, &cache)?;
    artifacts::ensure_directory(&artifacts, &generated)?;
    let http = Http::new()?;
    let vineflower = vineflower::download(&http, &cache)?;

    let minecraft = mojang::prepare(
        &http,
        &config.minecraft_version_id,
        vineflower::MINIMUM_JAVA,
        &cache,
    )?;
    let server_input = server::decompiler_input(
        &minecraft.server_jar,
        &config.minecraft_version_id,
        &cache.join("minecraft").join(&config.minecraft_version_id),
    )?;
    let libraries = libraries::prepare(
        &http,
        &minecraft.server_jar,
        &cache.join("minecraft").join(&config.minecraft_version_id),
        &cache,
    )?;
    let sources = sources::generate(
        &minecraft,
        &vineflower,
        &server_input,
        &libraries,
        &config.minecraft_version_id,
        &generated,
    )?;
    println!("Minecraft sources: {}", sources.display());
    Ok(())
}
