// cargo run --example wip

use balatro_mod_index::{forge::Tree, mods::ModIndex};

use balatro_mod_manager::download::install_mod;
use env_logger::Env;

#[tokio::main]
async fn main() -> Result<(), String> {
    env_logger::Builder::from_env(Env::new().default_filter_or("info")).init();

    let reqwest = reqwest::Client::new();
    let index_repo = Tree::default();

    log::info!("fetching index...");
    let mut index = ModIndex::from_reqwest(&reqwest, &index_repo).await?;
    let mods = &mut index.mods;
    mods.sort_by(|(_, a), (_, b)| a.meta.title.cmp(&b.meta.title));
    mods.sort_by(|(_, a), (_, b)| b.meta.last_updated.cmp(&a.meta.last_updated));

    let (_, typist) = mods
        .iter()
        .find(|(id, _)| id == "kasimeka@typist")
        .ok_or("Mod `kasimeka@typist` not found in index")?;

    install_mod(&reqwest, typist)
        .await
        .map_err(|e| format!("Failed to install mod: {e}"))?;

    Ok(())
}
