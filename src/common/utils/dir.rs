use std::{env, path::PathBuf};

use directories::ProjectDirs;
use tracing::warn;

use crate::Error;

pub(crate) fn config_dir_path(app_name: &str) -> Result<PathBuf, Error> {
    match ProjectDirs::from("", "", app_name) {
        Some(dir) => Ok(dir.config_dir().to_path_buf()),
        None => {
            warn!("Failed to get the path to the project's config directory, using the current working directory");
            Ok(env::current_dir()?)
        }
    }
}

pub(crate) fn data_dir_path(app_name: &str) -> Result<PathBuf, Error> {
    match ProjectDirs::from("", "", app_name) {
        Some(dir) => Ok(dir.data_local_dir().to_path_buf()),
        None => {
            warn!("Failed to get the path to the project's local data directory, using the current working directory");
            Ok(env::current_dir()?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_path() -> Result<(), Error> {
        let _ = super::config_dir_path("test-app")?;
        Ok(())
    }

    #[test]
    fn data_dir_path() -> Result<(), Error> {
        let _ = super::data_dir_path("test-app")?;
        Ok(())
    }
}
