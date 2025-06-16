use std::{env, path::PathBuf};

#[derive(Clone, Debug)]
pub struct LoveGame<'string> {
    name: &'string str,
    path: Option<&'string str>,
    #[cfg(target_os = "linux")]
    steamid: Option<&'string str>,
    #[cfg(target_os = "linux")]
    is_wine: bool,
}
#[cfg(target_os = "linux")]
pub const BALATRO_STEAMID: &str = "2379780";
impl<'strings> LoveGame<'strings> {
    #[must_use]
    pub fn default_balatro() -> Self {
        Self {
            name: "Balatro",
            path: None,
            #[cfg(target_os = "linux")]
            steamid: Some(BALATRO_STEAMID),
            #[cfg(target_os = "linux")]
            is_wine: true,
        }
    }

    #[must_use]
    pub fn new(name: &'strings str) -> Self {
        Self {
            name,
            path: None,
            #[cfg(target_os = "linux")]
            steamid: None,
            #[cfg(target_os = "linux")]
            is_wine: false,
        }
    }
    #[must_use]
    pub fn with_path(mut self, path: &'strings str) -> Self {
        self.path = Some(path);
        self
    }
    #[must_use]
    #[cfg(target_os = "linux")]
    pub fn with_steamid(mut self, steamid: &'strings str) -> Self {
        self.steamid = Some(steamid);
        self.is_wine = true;
        self
    }

    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.path
    }
    #[must_use]
    pub fn name(&self) -> &str {
        self.name
    }
    #[must_use]
    #[cfg(target_os = "linux")]
    pub fn is_wine(&self) -> bool {
        self.is_wine
    }
    #[must_use]
    #[cfg(target_os = "linux")]
    pub fn steamid(&self) -> Option<&str> {
        self.steamid
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn get_mods_dir(&self) -> Result<PathBuf, String> {
        if let Some(p) = env::var_os("LOVELY_MOD_DIR") {
            let p = PathBuf::from(p);
            if !p.exists() {
                std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
            }
            return Ok(p);
        }

        let mods_dir = dirs::config_dir()
            .ok_or("couldn't find config directory, your env is so cooked")?
            .join(self.name)
            .join("Mods");

        #[cfg(not(target_os = "linux"))]
        {
            if !mods_dir.exists() {
                std::fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
            }
            Ok(mods_dir)
        }

        #[cfg(target_os = "linux")]
        {
            if !self.is_wine {
                if !mods_dir.exists() {
                    std::fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
                }
                return Ok(mods_dir);
            }

            let wine_mods_dir = {
                let prefix = {
                    let p = self.path.map_or(PathBuf::new(), PathBuf::from);
                    if p.ends_with(String::from("steamapps/common/") + self.name) {
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
                    .join(self.steamid.unwrap_or(if self.name == "Balatro" {
                        BALATRO_STEAMID
                    } else {
                        panic!("steamid not provided for game `{}`", self.name)
                    }))
                    .join("pfx/drive_c/users/steamuser/AppData/Roaming/")
                    .join(self.name)
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
                    "dir `{}` already exists will not overwrite it",
                    mods_dir.display()
                );
            } else {
                std::os::unix::fs::symlink(&wine_mods_dir, mods_dir).unwrap_or(());
            }

            Ok(wine_mods_dir)
        }
    }
}
