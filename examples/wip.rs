// cargo run --example wip

use balatro_mod_index::{forge::Tree, mods::ModIndex};
use env_logger::Env;
use lovely_mod_manager::{self as lmm, ModManager};

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();

    log::info!("fetching index...");

    let mut manager = ModManager::new(
        ModIndex::from_reqwest(&reqwest, <&Tree>::default()).await?,
        lmm::Game::default_balatro(),
    )?;

    let target_mods = manager
        .index
        .mods
        .iter()
        .filter(|(id, _)| id == "kasimeka@typist" || id == "Breezebuilder@SystemClock")
        .cloned()
        .collect::<Vec<_>>();
    assert!(target_mods.len() == 2, "couldn't find expected mods");

    for m in &target_mods {
        manager.uninstall_mod(m).or_else(|e| match e.as_str() {
            "mod not installed" => {
                log::warn!("didn't uninstall `{}` as it wasn't installed", m.0);
                Ok(())
            }
            _ => Err(format!("unexpected error while uninstalling mod: {e}")),
        })?;
    }
    manager
        .installed_mods()
        .iter()
        .for_each(|(id, (enabled, version))| {
            log::info!(
                "found managed mod: `{}/{id}/{version}`",
                if *enabled { "" } else { "-" }
            );
        });

    for m in &target_mods {
        manager
            .install_mod(&reqwest, m)
            .await
            .map_err(|e| format!("failed to install mod: {e}"))?;
        log::info!("installed mod: `{}`", m.0);
    }
    let m = target_mods.first().unwrap();
    manager.disable_mod(m)?;
    log::info!("disabled mod: `{}`", m.0);
    manager
        .installed_mods()
        .iter()
        .for_each(|(id, (enabled, version))| {
            log::info!(
                "found managed mod: `{}/{id}/{version}`",
                if *enabled { "" } else { "-" }
            );
        });

    Ok(())
}
