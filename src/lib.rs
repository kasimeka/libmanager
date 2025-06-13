#![allow(clippy::missing_errors_doc)]

pub const CRATE_NAME: &str = "libmanager";

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

use balatro_mod_index::mods::{Mod, ModId, ModIndex};
use zip::{ZipArchive, read::root_dir_common_filter};

pub type ModEntry<'index> = (ModId, Mod<'index>);
#[derive(Clone, Debug, Default)]
pub struct ModManager<'index> {
    pub index: ModIndex<'index>,
    pub installed_mods: HashMap<ModId, String>,
}
impl ModManager<'_> {
    pub fn mut_detect_installed_mods(&mut self) -> Result<(), String> {
        self.installed_mods = detect_installed_mods()?;
        Ok(())
    }

    pub fn uninstall_mod(&mut self, (id, m): &ModEntry) -> Result<(), String> {
        _ = self.installed_mods.remove(id).ok_or("mod not installed")?;
        uninstall_mod(m)?;
        Ok(())
    }

    pub async fn install_mod(
        &mut self,
        client: &reqwest::Client,
        entry: &ModEntry<'_>,
    ) -> Result<(), String> {
        install_mod(self, client, entry, false).await
    }
    pub async fn reinstall_mod(
        &mut self,
        client: &reqwest::Client,
        entry: &ModEntry<'_>,
    ) -> Result<(), String> {
        install_mod(self, client, entry, true).await
    }
}

fn uninstall_mod(m: &Mod<'_>) -> Result<(), String> {
    let mod_dir = mod_path(m)?;

    if !mod_dir.exists() {
        return Ok(());
    }

    std::fs::remove_dir_all(mod_dir).map_err(|e| e.to_string())
}

async fn install_mod(
    manager: &mut ModManager<'_>,
    client: &reqwest::Client,
    (id, m): &ModEntry<'_>,
    reinstall: bool,
) -> Result<(), String> {
    let outdir = mod_path(m)?;

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

    if outdir.exists() {
        if !reinstall {
            return Err(format!("mod `{}` is already installed", id.0));
        }
        std::fs::remove_dir_all(&outdir).map_err(|e| e.to_string())?;
    }
    zip.extract_unwrapped_root_dir(&outdir, root_dir_common_filter)
        .map_err(|e| e.to_string())?;

    let mut statefile =
        File::create(outdir.join(format!(".{}", crate::CRATE_NAME))).map_err(|e| e.to_string())?;
    write!(statefile, "id {id}\nversion {}", m.meta.version).map_err(|e| e.to_string())?;

    manager
        .installed_mods
        .insert(id.clone(), m.meta.version.clone());
    Ok(())
}

fn mod_path(m: &Mod<'_>) -> Result<PathBuf, String> {
    let basename = m
        .meta
        .folder_name
        .clone()
        .unwrap_or_else(|| m.meta.title.chars().filter(char::is_ascii).collect());
    let outdir = mods_dir()?.join(&basename);

    Ok(outdir)
}

fn detect_installed_mods() -> Result<HashMap<ModId, String>, String> {
    let mods_dir = mods_dir()?;

    if !mods_dir.exists() {
        return Err("Mods directory does not exist".to_string());
    }

    let mut mods = HashMap::new();
    for entry in std::fs::read_dir(&mods_dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !path.is_dir() {
            continue;
        }

        let statefile = path.join(format!(".{}", crate::CRATE_NAME));
        if !statefile.exists() {
            continue;
        }

        let (id, version) =
            parse_status(&std::fs::read_to_string(statefile).map_err(|e| e.to_string())?)?;
        mods.insert(id, version);
    }

    Ok(mods)
}

fn mods_dir() -> Result<PathBuf, String> {
    let mods_dir = dirs::config_dir()
        .ok_or("couldn't find config directory, your env is so cooked")?
        .join("Balatro")
        .join("Mods");

    if !mods_dir.exists() {
        std::fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    }

    Ok(mods_dir)
}

pub fn parse_status(meta: &str) -> Result<(ModId, String), String> {
    let mut id = ModId::default();
    let mut version = String::new();
    for line in meta.lines() {
        let (key, value) = line
            .trim()
            .split_once(' ')
            .ok_or("line is not a key-value pair")?;
        match key {
            "id" => {
                id = ModId(value.to_owned());
                if id.0.is_empty() {
                    return Err("id can't be empty".to_string());
                }
            }
            "version" => {
                if value.is_empty() {
                    return Err("version can't be empty".to_string());
                }
                value.clone_into(&mut version);
            }
            _ => {}
        }
    }

    Ok((id, version))
}
