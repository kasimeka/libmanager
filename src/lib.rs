#![allow(clippy::missing_errors_doc)]

pub const PACKAGE_NAME: &str = "lovely_mod_manager";

use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, Write},
    path::PathBuf,
};

use balatro_mod_index::mods::{Mod, ModId, ModIndex};
use zip::{ZipArchive, read::root_dir_common_filter};

#[derive(Clone, Debug, Default)]
pub struct ModManager<'index, 'game> {
    pub index: ModIndex<'index>,
    pub game: Game<'game>,
    pub installed_mods: HashMap<ModId, (bool, String)>,
}
#[derive(Clone, Debug)]
pub struct Game<'a> {
    pub name: &'a str,
    pub path: Option<&'a str>,
    pub steamid: Option<&'a str>,
}
pub const BALATRO_STEAMID: &str = "2379780";
impl Default for Game<'_> {
    fn default() -> Self {
        Self {
            name: "Balatro",
            path: None,
            steamid: Some(BALATRO_STEAMID), // only relevant for linux
        }
    }
}

pub type ModEntry<'index> = (ModId, Mod<'index>);
impl ModManager<'_, '_> {
    pub fn rebuild_expectfile(&mut self) -> Result<(), String> {
        self.installed_mods = detect_installed_mods(self)?;
        self.write_expectfile()
    }
    pub fn read_expectfile(&mut self) -> Result<(), String> {
        self.installed_mods = read_expectfile(&self.game)?;
        Ok(())
    }
    pub fn write_expectfile(&self) -> Result<(), String> {
        let p = get_mods_dir(&self.game)?.join(format!(".{}", crate::PACKAGE_NAME));
        let mut expectfile = File::create(p).map_err(|e| e.to_string())?;
        self.installed_mods
            .iter()
            .try_for_each(|s| write_modspec(&mut expectfile, s))
            .map_err(|e| format!("failed to write expectfile: {e}"))
    }

    pub fn uninstall_mod(&mut self, (id, m): &ModEntry) -> Result<(), String> {
        _ = self.installed_mods.get(id).ok_or("mod not installed")?;
        let mod_dir = get_mod_path(&self.game, m)?;
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
        File::create(get_mod_path(&self.game, m)?.join(".lovelyignore"))
            .map_err(|e| e.to_string())?;
        self.installed_mods
            .insert(id.clone(), (false, version.clone()));
        self.write_expectfile()
    }
    pub fn enable_mod(&mut self, (id, m): &ModEntry<'_>) -> Result<(), String> {
        let (_, version) = self.installed_mods.get(id).ok_or("mod not installed")?;
        let disablefile = get_mod_path(&self.game, m)?.join(".lovelyignore");
        if !disablefile.exists() {
            return Ok(());
        }
        std::fs::remove_file(disablefile).map_err(|e| e.to_string())?;
        self.installed_mods
            .insert(id.clone(), (true, version.clone()));
        self.write_expectfile()
    }
}

fn read_expectfile(game: &Game<'_>) -> Result<HashMap<ModId, (bool, String)>, String> {
    let expectfile = get_mods_dir(game)?.join(format!(".{}", crate::PACKAGE_NAME));
    if !expectfile.exists() {
        return Ok(HashMap::new());
    }
    Ok(std::fs::read_to_string(expectfile)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            if let [s, id, version] = l.split('/').collect::<Vec<_>>()[..] {
                Some((ModId(id.into()), (s.is_empty(), version.to_string())))
            } else {
                log::warn!("invalid modspec format in `{l}`, ignored it");
                None
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

async fn install_mod(
    manager: &mut ModManager<'_, '_>,
    client: &reqwest::Client,
    entry @ (id, m): &ModEntry<'_>,
    reinstall: bool,
) -> Result<(), String> {
    let outdir = get_mod_path(&manager.game, m)?;

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
    let mods_dir = get_mods_dir(&manager.game)?;

    if !mods_dir.exists() {
        return Err("Mods directory does not exist".to_string());
    }

    let mut mods = HashMap::new();
    for entry in std::fs::read_dir(&mods_dir).map_err(|e| e.to_string())? {
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

fn get_mod_path(game: &Game<'_>, m: &Mod<'_>) -> Result<PathBuf, String> {
    let basename = m
        .meta
        .folder_name
        .clone()
        .unwrap_or_else(|| m.meta.title.chars().filter(char::is_ascii).collect());
    Ok(get_mods_dir(game)?.join(&basename))
}
fn get_mods_dir(game: &Game<'_>) -> Result<PathBuf, String> {
    if let Some(p) = env::var_os("LOVELY_MOD_DIR") {
        let p = PathBuf::from(p);
        if !p.exists() {
            std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        }
        return Ok(p);
    }

    let mut mods_dir = dirs::config_dir()
        .ok_or("couldn't find config directory, your env is so cooked")?
        .join(game.name)
        .join("Mods");

    // implicit support for proton and wine
    #[cfg(target_os = "linux")]
    {
        let wine_mods_dir = {
            let prefix = {
                let p = game.path.map_or(PathBuf::new(), PathBuf::from);
                if p.ends_with(String::from("steamapps/common/") + game.name) {
                    p.parent().unwrap().parent().unwrap().to_path_buf()
                } else {
                    dirs::home_dir()
                        .ok_or("couldn't find home directory, your env is so cooked")?
                        .join(".steam/steam/steamapps/")
                }
            };
            log::debug!("assumed steam wineprefix `{}`", prefix.to_string_lossy());

            prefix
                .join("compatdata/")
                .join(game.steamid.unwrap_or(if game.name == "Balatro" {
                    BALATRO_STEAMID
                } else {
                    panic!("steamid not provided for game `{}`", game.name)
                }))
                .join("pfx/drive_c/users/steamuser/AppData/Roaming/")
                .join(game.name)
                .join("Mods")
        };

        if !wine_mods_dir.exists() {
            std::fs::create_dir_all(&wine_mods_dir).map_err(|e| e.to_string())?;
        }

        if mods_dir.read_link().is_ok() {
            std::fs::remove_file(&mods_dir).unwrap_or(());
            std::os::unix::fs::symlink(&wine_mods_dir, mods_dir).unwrap_or(());
        } else if mods_dir.exists() {
            log::warn!(
                "mods dir `{}` already exists will not overwrite it",
                mods_dir.display()
            );
        } else {
            std::os::unix::fs::symlink(&wine_mods_dir, mods_dir).unwrap_or(());
        }

        mods_dir = wine_mods_dir;
    }

    Ok(mods_dir)
}
