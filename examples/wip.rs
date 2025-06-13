// cargo run --example wip

use balatro_mod_index::{forge::Tree, mods::ModIndex};
use balatro_mod_manager::{ModManager, reinstall_mod, uninstall_mod};
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

    let typist = manager
        .index
        .mods
        .iter()
        .find(|(id, _)| id == "kasimeka@typist")
        .ok_or("Mod `kasimeka@typist` not found in index")?
        .clone();

    reinstall_mod(&reqwest, &typist)
        .await
        .map_err(|e| format!("failed to install mod: {e}"))?;

    manager.mut_detect_installed_mods()?;
    manager.installed_mods.iter().for_each(|(id, version)| {
        log::info!("found installed mod: `{id}@{version}`");
    });

    uninstall_mod(&typist).map_err(|e| format!("failed to uninstall mod: {e}"))?;

    Ok(())
}
