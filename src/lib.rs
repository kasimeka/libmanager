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
    pub expected_mods: HashMap<ModId, String>,
}
impl ModManager<'_> {
    pub fn detect_installed_mods(&mut self) -> Result<(), String> {
        self.installed_mods = detect_installed_mods()?;
        Ok(())
    }
    pub fn read_expected_mods(&mut self) -> Result<(), String> {
        self.expected_mods = read_expected_mods()?;
        Ok(())
    }
    pub fn write_expected_mods(&self) -> Result<(), String> {
        let p = mods_dir()?.join(format!(".{}", crate::CRATE_NAME));
        let mut expectfile = File::create(p).map_err(|e| e.to_string())?;
        self.expected_mods
            .iter()
            .try_for_each(|(id, version)| {
                writeln!(expectfile, "{id}/{version}").map_err(|e| e.to_string())
            })
            .map_err(|e| format!("failed to write expected mods: {e}"))
    }
    pub fn repopulate_expected_mods(&mut self) -> Result<(), String> {
        self.expected_mods = self.installed_mods.clone();
        self.write_expected_mods()
    }

    pub fn uninstall_mod(&mut self, (id, m): &ModEntry) -> Result<(), String> {
        _ = self.installed_mods.get(id).ok_or("mod not installed")?;
        let mod_dir = mod_path(m)?;
        if !mod_dir.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(mod_dir).map_err(|e| e.to_string())?;
        self.installed_mods.remove(id);
        self.expected_mods.remove(id);
        self.write_expected_mods()
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

async fn install_mod(
    manager: &mut ModManager<'_>,
    client: &reqwest::Client,
    e @ (id, m): &ModEntry<'_>,
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

    let statusfile = &mut File::create(outdir.join(format!(".{}", crate::CRATE_NAME)))
        .map_err(|e| e.to_string())?;
    write_status(e, statusfile).map_err(|e| e.to_string())?;

    manager
        .installed_mods
        .insert(id.clone(), m.meta.version.clone());
    manager
        .expected_mods
        .insert(id.clone(), m.meta.version.clone());
    manager.write_expected_mods()?;
    Ok(())
}

fn mod_path(m: &Mod<'_>) -> Result<PathBuf, String> {
    let basename = m
        .meta
        .folder_name
        .clone()
        .unwrap_or_else(|| m.meta.title.chars().filter(char::is_ascii).collect());
    Ok(mods_dir()?.join(&basename))
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
        mods.insert(id.clone(), version.clone());
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

fn write_status<T>((id, m): &ModEntry<'_>, statusfile: &mut T) -> Result<(), String>
where
    T: Write,
{
    write!(statusfile, "{id}/{}", m.meta.version).map_err(|e| e.to_string())
}
fn parse_status(status: &str) -> Result<(ModId, String), String> {
    status
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or("statusfile can't be empty")?
        .split_once('/')
        .map_or(Err("invalid statusfile format".into()), |(id, version)| {
            if id.is_empty() {
                return Err("id can't be empty".to_string());
            }
            if version.is_empty() {
                return Err("version can't be empty".to_string());
            }
            Ok((ModId(id.into()), version.to_string()))
        })
}

fn read_expected_mods() -> Result<HashMap<ModId, String>, String> {
    let p = mods_dir()?.join(format!(".{}", crate::CRATE_NAME));
    if !p.exists() {
        return Ok(HashMap::new());
    }
    Ok(std::fs::read_to_string(p)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            l.split_once('/')
                .map(|(id, version)| (ModId(id.into()), version.to_string()))
        })
        .collect::<HashMap<_, _>>())
}
