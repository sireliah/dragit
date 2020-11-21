use std::fs;
use std::io::{Error, ErrorKind, Read, Write};
use std::path::Path;
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
    let name = format!("{}_{}", timestamp(), name);
    let path = Path::new(path);
    let joined = path.join(name);
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
        None => match get_downloads_dir() {
            Ok(path) => generate_full_path(name, Path::new(&path), get_timestamp),
            Err(_) => Err(Error::new(
                ErrorKind::NotFound,
                "Downloads directory could not be found",
            )),
        },
    }
}

pub fn get_downloads_dir() -> Result<String, Error> {
    let config = UserConfig::new()?;

    info!("Config struct: {}", config.get_downloads());

    Ok(config.get_downloads())
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    downloads: String,
}

struct UserConfig {
    conf: Config,
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
            let toml = toml::to_string(&config).expect("Failed creating config file");
            let mut file = fs::File::create(&joined_path)?;
            file.write_all(&toml.as_bytes())?;
        }
        let mut file = fs::File::open(joined_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        info!("Contents: {:?}", contents);

        let conf: Config = toml::from_str(&contents)?;

        Ok(UserConfig { conf })
    }

    pub fn get_downloads(&self) -> String {
        self.conf.downloads.to_owned()
    }
}

pub fn get_user_config() {
    let base_dir = BaseDirs::new().unwrap();
    let config_dir = base_dir.config_dir();
    info!("Config dir: {:?}", config_dir);
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
