use std::io::{Error, ErrorKind};

use directories::UserDirs;

pub fn get_target_path(name: &str) -> Result<String, Error> {
    match UserDirs::new() {
        Some(dirs) => match dirs.download_dir() {
            Some(path) => {
                let p = path.join(name);
                let result = p.into_os_string().into_string();
                match result {
                    Ok(value) => Ok(value),
                    Err(_) => Err(Error::new(
                        ErrorKind::InvalidData,
                        "Could not return Downloads path as string",
                    )),
                }
            },
            None => Err(Error::new(
                ErrorKind::NotFound,
                "Downloads directory could not be found",
            )),
        },
        None => Err(Error::new(ErrorKind::NotFound, "Could not check user dirs")),
    }
}
