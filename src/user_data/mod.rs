use std::fs;
use std::io::{Error, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use toml;

fn get_timestamp() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .expect("Time failed")
        .as_secs()
}

fn generate_full_path<F>(name: &str, path: &Path, timestamp: F) -> Result<String, Error>
where
    F: Fn() -> u64,
{
    let path = Path::new(&path);
    let joined = path.join(&name);

    // Add timestamp to the file name if the file already exists in target directory
    let joined = if joined.exists() {
        let extension: String = match joined.extension() {
            Some(v) => v.to_string_lossy().to_string(),
            None => "".to_string(),
        };
        let basename = match joined.file_stem() {
            Some(v) => v.to_string_lossy().to_string(),
            None => "file".to_string(),
        };
        let name = format!("{}_{}", basename, timestamp());
        let mut path = path.join(&name);
        path.set_extension(extension);
        path
    } else {
        joined
    };

    let result = joined.into_os_string().into_string().or_else(|_| {
        Err(Error::new(
            ErrorKind::InvalidData,
            "Could not return target path as string",
        ))
    });
    result
}

pub fn get_target_path(name: &str, target_path: Option<&String>) -> Result<String, Error> {
    match target_path {
        Some(path) => {
            let path = Path::new(path);
            generate_full_path(name, path, get_timestamp)
        }
        None => {
            let config = UserConfig::new()?;
            let dir = config.get_downloads_dir();
            generate_full_path(name, dir.as_path(), get_timestamp)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    downloads: String,
}

pub struct UserConfig {
    conf: Config,
    conf_path: PathBuf,
}

impl UserConfig {
    pub fn new() -> Result<UserConfig, Error> {
        let base_dirs = BaseDirs::new()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Problem opening base dirs"))?;
        let base_config_path = base_dirs.config_dir();

        let path = Path::new(base_config_path);
        let mut joined_path = path.join("dragit");

        if !joined_path.exists() {
            info!("Creating {:?} directory", joined_path);
            fs::create_dir(&joined_path)?;
        }

        joined_path.push("config.toml");
        if !joined_path.exists() {
            info!("Creating default {:?} file", joined_path);

            let user_dirs = UserDirs::new()
                .ok_or_else(|| Error::new(ErrorKind::Other, "Problem opening user dirs"))?;
            let config = Config {
                downloads: match user_dirs.download_dir() {
                    Some(v) => v.to_string_lossy().to_string(),
                    None => base_dirs.home_dir().to_string_lossy().to_string(),
                },
            };
            let toml = Self::serialize_config(config)?;
            let mut file = fs::File::create(&joined_path)?;
            file.write_all(&toml.as_bytes())?;
        }
        let mut file = fs::File::open(&joined_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let conf: Config = toml::from_str(&contents)?;

        Ok(UserConfig {
            conf,
            conf_path: joined_path.to_owned(),
        })
    }

    pub fn get_downloads_dir(&self) -> PathBuf {
        Path::new(&self.conf.downloads).to_owned()
    }

    pub fn set_downloads_dir(&self, path: &Path) -> Result<(), Error> {
        // Watch out, this ::create will truncate the file
        let mut file = fs::File::create(&self.conf_path.as_path())?;

        let config: Config = Config {
            downloads: path.to_string_lossy().to_string(),
        };
        let toml = Self::serialize_config(config)?;
        file.write_all(&toml.as_bytes())?;
        Ok(())
    }

    fn serialize_config(config: Config) -> Result<String, Error> {
        match toml::to_string(&config) {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("Problem parsing toml: {:?}", e);
                return Err(Error::new(ErrorKind::Other, "Problem parsing toml"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::user_data::generate_full_path;
    use std::path::Path;

    #[test]
    fn test_generate_full_path() {
        let result = generate_full_path("a-file.txt", Path::new("/home/user/"), || 1111).unwrap();

        assert_eq!(result, "/home/user/1111_a-file.txt");
    }
}
