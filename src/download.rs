#![allow(clippy::missing_errors_doc)]

use std::io;

use balatro_mod_index::mods::Mod;
use zip::{ZipArchive, read::root_dir_common_filter};

pub async fn install_mod(client: &reqwest::Client, m: &Mod<'_>) -> Result<(), String> {
    let basename = m
        .meta
        .folder_name
        .clone()
        .unwrap_or_else(|| m.meta.title.chars().filter(char::is_ascii).collect());
    let outdir = dirs::config_dir()
        .ok_or("couldn't find config directory, your env so cooked")?
        .join("Balatro")
        .join("Mods")
        .join(&basename);

    let data = client
        .get(&m.meta.download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let mut zip = ZipArchive::new(io::Cursor::new(data)).map_err(|e| e.to_string())?;
    log::debug!(
        "downloaded zip file {}, will install it to {}",
        m.meta.download_url,
        outdir.display()
    );

    zip.extract_unwrapped_root_dir(outdir, root_dir_common_filter)
        .map_err(|e| e.to_string())
}
