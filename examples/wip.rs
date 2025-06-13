// cargo run --example wip

use balatro_mod_index::{forge::Tree, mods::ModIndex};
use balatro_mod_manager::ModManager;
use env_logger::Env;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();

    log::info!("fetching index...");
    let mut manager = ModManager {
        index: ModIndex::from_reqwest(&reqwest, <&Tree>::default()).await?,
        ..Default::default()
    };
    manager.mut_detect_installed_mods()?;

    let typist = manager
        .index
        .mods
        .iter()
        .find(|(id, _)| id == "kasimeka@typist")
        .ok_or("`kasimeka@typist` not found in the index")?
        .clone();

    manager
        .reinstall_mod(&reqwest, &typist)
        .await
        .map_err(|e| format!("failed to install mod: {e}"))?;
    manager.installed_mods.iter().for_each(|(id, version)| {
        log::info!("found managed mod: `{id}@{version}`");
    });

    manager
        .uninstall_mod(&typist)
        .map_err(|e| format!("failed to uninstall mod: {e}"))?;
    log::info!("uninstalled  `kasimeka@typist`");
    manager.installed_mods.iter().for_each(|(id, version)| {
        log::info!("found installed mod: `{id}@{version}`");
    });

    Ok(())
}
