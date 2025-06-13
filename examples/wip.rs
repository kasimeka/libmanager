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
        .uninstall_mod(&typist)
        .or_else(|e| match e.as_str() {
            "mod not installed" => {
                log::warn!("didn't uninstall `kasimeka@typist` as it wasn't installed");
                Ok(())
            }
            _ => Err(format!("unexpected error while uninstalling mod: {e}")),
        })?;
    manager.installed_mods.iter().for_each(|(id, version)| {
        log::info!("found managed mod: `{id}@{version}`");
    });

    manager
        .install_mod(&reqwest, &typist)
        .await
        .map_err(|e| format!("failed to install mod: {e}"))?;
    log::info!("installed mod: `{}`", typist.0);
    manager.installed_mods.iter().for_each(|(id, version)| {
        log::info!("found managed mod: `{id}@{version}`");
    });

    Ok(())
}
