#![allow(clippy::missing_errors_doc)]

pub mod game;
pub use game::LoveGame as Game;

pub const PACKAGE_NAME: &str = "lovely_mod_manager";

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

use balatro_mod_index::mods::{Mod, ModId, ModIndex};
use zip::{ZipArchive, read::root_dir_common_filter};

#[derive(Clone, Debug)]
pub struct ModManager<'index, 'game> {
    pub index: ModIndex<'index>,
    pub game: Game<'game>, // it's your fault if you change this without `replace_game`
    installed_mods: HashMap<ModId, (bool, String)>,
    mods_dir: PathBuf,
}

pub type ModEntry<'index> = (ModId, Mod<'index>);
impl<'index, 'game> ModManager<'index, 'game> {
    pub fn new(index: ModIndex<'index>, game: Game<'game>) -> Result<Self, String> {
        let mods_dir = game
            .detect_and_init_mods_dir()
            .map_err(|e| format!("failed to detect mods dir: {e}"))?;
        let installed_mods = read_expectfile(&mods_dir)?;
        Ok(Self {
            index,
            game,
            installed_mods,
            mods_dir,
        })
    }
    pub fn replace_game(&mut self, game: Game<'game>) -> Result<(), String> {
        let mods_dir = game.detect_and_init_mods_dir()?;
        let installed_mods = read_expectfile(&mods_dir)?;
        self.game = game;
        self.mods_dir = mods_dir;
        self.installed_mods = installed_mods;
        Ok(())
    }

    pub async fn refetch_index(&mut self, client: &reqwest::Client) -> Result<(), String> {
        self.index = ModIndex::from_reqwest(client, self.index.repo)
            .await
            .map_err(|e| format!("failed to refetch index: {e}"))?;
        Ok(())
    }

    #[must_use]
    pub fn mods_dir(&self) -> &Path {
        &self.mods_dir
    }
    #[must_use]
    pub fn installed_mods(&self) -> &HashMap<ModId, (bool, String)> {
        &self.installed_mods
    }

    pub fn rebuild_expectfile(&mut self) -> Result<(), String> {
        self.installed_mods = detect_installed_mods(self)?;
        self.write_expectfile()
    }
    pub fn load_expectfile(&mut self) -> Result<(), String> {
        self.installed_mods = read_expectfile(&self.mods_dir)?;
        Ok(())
    }
    pub fn write_expectfile(&self) -> Result<(), String> {
        let p = self.mods_dir.join(format!(".{}", crate::PACKAGE_NAME));
        let mut expectfile = File::create(p).map_err(|e| e.to_string())?;
        self.installed_mods
            .iter()
            .try_for_each(|s| write_modspec(&mut expectfile, s))
            .map_err(|e| format!("failed to write expectfile: {e}"))
    }

    pub fn uninstall_mod(&mut self, (id, m): &ModEntry) -> Result<(), String> {
        _ = self.installed_mods.get(id).ok_or("mod not installed")?;
        let mod_dir = get_mod_path(&self.mods_dir, m);
        if !mod_dir.exists() {
            return Ok(());
        }
        std::fs::remove_dir_all(mod_dir).map_err(|e| e.to_string())?;
        self.installed_mods.remove(id);
        self.write_expectfile()
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

    pub fn disable_mod(&mut self, (id, m): &ModEntry<'_>) -> Result<(), String> {
        let (_, version) = self.installed_mods.get(id).ok_or("mod not installed")?;
        File::create(get_mod_path(&self.mods_dir, m).join(".lovelyignore"))
            .map_err(|e| e.to_string())?;
        self.installed_mods
            .insert(id.clone(), (false, version.clone()));
        self.write_expectfile()
    }
    pub fn enable_mod(&mut self, (id, m): &ModEntry<'_>) -> Result<(), String> {
        let (_, version) = self.installed_mods.get(id).ok_or("mod not installed")?;
        let disablefile = get_mod_path(&self.mods_dir, m).join(".lovelyignore");
        if !disablefile.exists() {
            return Ok(());
        }
        std::fs::remove_file(disablefile).map_err(|e| e.to_string())?;
        self.installed_mods
            .insert(id.clone(), (true, version.clone()));
        self.write_expectfile()
    }
}

fn read_expectfile(mods_dir: &Path) -> Result<HashMap<ModId, (bool, String)>, String> {
    let expectfile = mods_dir.join(format!(".{}", crate::PACKAGE_NAME));
    if !expectfile.exists() {
        return Ok(HashMap::new());
    }
    Ok(std::fs::read_to_string(expectfile)
        .map_err(|e| e.to_string())?
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            match parse_modspec(l) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::warn!("failed to parse modspec `{l}`: {e}, skipping it");
                    None
                }
            }
        })
        .collect::<HashMap<_, _>>())
}
fn write_modspec<T>(
    expectfile: &mut T,
    (id, (enabled, version)): (&ModId, &(bool, String)),
) -> Result<(), String>
where
    T: Write,
{
    writeln!(
        expectfile,
        "{}/{id}/{version}",
        if *enabled { "" } else { "-" }
    )
    .map_err(|e| e.to_string())
}
fn parse_modspec(line: &str) -> Result<(ModId, (bool, String)), String> {
    if let [s, id, version] = line.split('/').collect::<Vec<_>>()[..] {
        if id.is_empty() {
            return Err("id can't be empty".to_string());
        }
        if version.is_empty() {
            return Err("version can't be empty".to_string());
        }
        Ok((ModId(id.into()), (s.is_empty(), version.to_string())))
    } else {
        Err("invalid modspec format".to_string())
    }
}

async fn install_mod(
    manager: &mut ModManager<'_, '_>,
    client: &reqwest::Client,
    entry @ (id, m): &ModEntry<'_>,
    reinstall: bool,
) -> Result<(), String> {
    let outdir = get_mod_path(&manager.mods_dir, m);

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

    let statefile = &mut File::create(outdir.join(format!(".{}", crate::PACKAGE_NAME)))
        .map_err(|e| e.to_string())?;
    write_state(statefile, entry).map_err(|e| e.to_string())?;

    manager
        .installed_mods
        .insert(id.clone(), (true, m.meta.version.clone()));
    manager.write_expectfile()?;
    Ok(())
}

fn detect_installed_mods(
    manager: &ModManager<'_, '_>,
) -> Result<HashMap<ModId, (bool, String)>, String> {
    if !manager.mods_dir.exists() {
        return Err("Mods directory does not exist".to_string());
    }

    let mut mods = HashMap::new();
    for entry in std::fs::read_dir(&manager.mods_dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !path.is_dir() {
            continue;
        }

        let statefile = path.join(format!(".{}", crate::PACKAGE_NAME));
        if !statefile.exists() {
            continue;
        }
        let (id, version) =
            parse_state(&std::fs::read_to_string(statefile).map_err(|e| e.to_string())?)?;
        let enabled = !path.join(".lovelyignore").exists();

        mods.insert(id.clone(), (enabled, version.clone()));
    }

    Ok(mods)
}

fn write_state<T>(statefile: &mut T, (id, m): &ModEntry<'_>) -> Result<(), String>
where
    T: Write,
{
    write!(statefile, "{id}/{}", m.meta.version).map_err(|e| e.to_string())
}
fn parse_state(state: &str) -> Result<(ModId, String), String> {
    state
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or("statefile can't be empty")?
        .split_once('/')
        .map_or(Err("invalid statefile format".into()), |(id, version)| {
            if id.is_empty() {
                return Err("id can't be empty".to_string());
            }
            if version.is_empty() {
                return Err("version can't be empty".to_string());
            }
            Ok((ModId(id.into()), version.to_string()))
        })
}

fn get_mod_path(mods_dir: &Path, m: &Mod<'_>) -> std::path::PathBuf {
    let basename = m.meta.folder_name.as_ref().map_or_else(
        || m.meta.title.chars().filter(char::is_ascii).collect(),
        Clone::clone,
    );
    mods_dir.join(basename)
}
