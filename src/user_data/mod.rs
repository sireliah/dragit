use std::fs;
use std::io::{Error, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories_next::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use toml;

// Unassigned in IANA
const DEFAULT_LISTEN_PORT: u16 = 36571;
const DEFAULT_FIREWALL_CHECKED: bool = false;

fn get_timestamp() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH)
        .expect("Time failed")
        .as_secs()
}

fn extend_dir(path: &Path, dir_name: &str, time: u64) -> PathBuf {
    path.join(format!("{}_{}", dir_name, time))
    // match path.parent() {
    //     Some(parent_path) => parent_path.join(format!("{}_{}", dir_name, time)),
    //     // Probably not best idea to use this application to move your whole root dir (｡•̀ᴗ-)
    //     None => Path::new(&format!("/directory_{}", time)).to_path_buf(),
    // }
}

fn extend_file(path: &Path, name: &str, time: u64) -> PathBuf {
    let file_name_path = Path::new(name);
    let extension: String = match file_name_path.extension() {
        Some(v) => v.to_string_lossy().to_string(),
        None => "".to_string(),
    };
    let basename = match file_name_path.file_stem() {
        Some(v) => v.to_string_lossy().to_string(),
        None => "file".to_string(),
    };
    let new_name = format!("{}_{}", basename, time);
    let mut path = path.join(&new_name);
    path.set_extension(extension);
    path
}

fn generate_full_path<F>(path: &Path, name: &str, timestamp: F) -> Result<String, Error>
where
    F: Fn() -> u64,
{
    info!("PATHS: {:?}, {:?}", path, name);
    // If file or dir already exists in the target directory, create a path extended with a timestamp
    let joined = path.join(&name);
    let time = timestamp();

    let joined = if joined.exists() {
        info!("File already exists: {:?}", joined);
        if joined.is_file() {
            let outpath = extend_file(path, name, time);
            info!("File out path: {:?}", outpath);
            outpath
        } else {
            extend_dir(path, name, time)
        }
    } else {
        joined
    };

    joined.into_os_string().into_string().or_else(|_| {
        Err(Error::new(
            ErrorKind::InvalidData,
            "Could not return target path as string",
        ))
    })
}

pub fn get_target_path(name: &str, target_path: Option<&String>) -> Result<String, Error> {
    match target_path {
        Some(path) => {
            let path = Path::new(path);
            generate_full_path(path, name, get_timestamp)
        }
        None => {
            let config = UserConfig::new()?;
            let dir = config.get_downloads_dir();
            generate_full_path(dir.as_path(), name, get_timestamp)
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    downloads: String,

    #[serde(default = "default_port")]
    port: u16,

    #[serde(default = "default_firewall_checked")]
    firewall_checked: bool,
}

fn default_port() -> u16 {
    DEFAULT_LISTEN_PORT
}

fn default_firewall_checked() -> bool {
    DEFAULT_FIREWALL_CHECKED
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
                port: DEFAULT_LISTEN_PORT,
                firewall_checked: DEFAULT_FIREWALL_CHECKED,
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

    pub fn get_port(&self) -> u16 {
        self.conf.port
    }

    pub fn get_firewall_checked(&self) -> bool {
        self.conf.firewall_checked
    }

    pub fn set_downloads_dir(&self, path: &Path) -> Result<(), Error> {
        // Watch out, this ::create will truncate the file
        let mut file = fs::File::create(&self.conf_path.as_path())?;

        let config: Config = Config {
            downloads: path.to_string_lossy().to_string(),
            port: self.conf.port,
            firewall_checked: self.conf.firewall_checked,
        };
        let toml = Self::serialize_config(config)?;
        file.write_all(&toml.as_bytes())?;
        Ok(())
    }

    pub fn set_firewall_checked(&self, value: bool) -> Result<(), Error> {
        // Watch out, this ::create will truncate the file
        let mut file = fs::File::create(&self.conf_path.as_path())?;

        let config: Config = Config {
            downloads: self.conf.downloads.to_owned(),
            port: self.conf.port,
            firewall_checked: value,
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
    use crate::user_data::{extend_dir, generate_full_path};
    use std::path::Path;
    use tempfile::tempdir;
    use std::fs::{create_dir_all, File};

    #[test]
    fn test_extend_dir_should_extend_name_with_timestamp() {
        let result = extend_dir(Path::new("/home/user/"), "directory", 1111);

        assert_eq!(result, Path::new("/home/user/directory_1111"))
    }

    #[test]
    fn test_generate_full_file_path() {
        let result = generate_full_path(Path::new("/home/user/"), "a-file.txt", || 1111).unwrap();

        assert_eq!(result, "/home/user/a-file.txt");
    }

    #[test]
    fn test_generate_full_file_path_when_file_exists() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let received_file_name = "a-file.txt";
        File::create(path.join(received_file_name)).unwrap();

        let result = generate_full_path(path, received_file_name, || 1111).unwrap();

        assert_eq!(result, path.join("a-file_1111.txt").to_string_lossy());
    }

    #[test]
    fn test_generate_full_dir_path() {
        let result =
            generate_full_path(Path::new("/home/user/"), "some_directory", || 1111).unwrap();

        assert_eq!(result, "/home/user/some_directory");
    }
    #[test]
    fn test_generate_full_dir_path_when_dir_exists() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let received_dir_name = "some_directory";
        create_dir_all(path.join(received_dir_name)).unwrap();
        let result =
            generate_full_path(path, received_dir_name, || 1111).unwrap();

        assert_eq!(result, path.join("some_directory_1111").to_string_lossy());
    }
}
