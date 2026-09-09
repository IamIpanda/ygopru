//! ypk file manager.

/// Extra manager for processing zip (`.ypk`) files.
///
/// It scans the `./expansions` folder and lets cards/scripts be read from the archives.
pub mod archive_manager {
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::LazyLock;

    use arc_swap::ArcSwap;
    use parking_lot::Mutex;
    use walkdir::WalkDir;
    use zip::ZipArchive;

    struct ExpansionArchive {
        _path: PathBuf,
        archive_reader: Mutex<ZipArchive<fs::File>>,
    }

    static GLOBAL_ARCHIVES: LazyLock<ArcSwap<Vec<ExpansionArchive>>> = LazyLock::new(|| ArcSwap::from_pointee(Vec::new()));

    // Scan `./expansions` folder and remember them.
    /// Scan the `./expansions` folder and remember the archives.
    pub fn init() {
        let mut expansion_archives: Vec<ExpansionArchive> = Vec::new();
        let config_manager = crate::managers::config_manager::load();
        let path = Path::new(config_manager.get_or("path", "./"));
        for entry in WalkDir::new(path.join("expansions")) {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if !is_expansion_archive(path) {
                continue;
            }
            let Ok(file) = fs::File::open(path) else {
                log::debug!("Failed to open archive {}", path.display());
                continue;
            };
            match ZipArchive::new(file) {
                Ok(archive_reader) => expansion_archives.push(ExpansionArchive {
                    _path: path.to_path_buf(),
                    archive_reader: Mutex::new(archive_reader),
                }),
                Err(error) => log::debug!("Failed to open archive {}: {}", path.display(), error),
            }
        }
        let scripts_zip = path.join("scripts.zip");
        if let Ok(file) = fs::File::open(&scripts_zip) {
            match ZipArchive::new(file) {
                Ok(archive_reader) => expansion_archives.push(ExpansionArchive {
                    _path: scripts_zip,
                    archive_reader: Mutex::new(archive_reader),
                }),
                Err(error) => {
                    log::debug!("Failed to open archive {}: {}", scripts_zip.display(), error)
                }
            }
        } else {
            log::debug!("Failed to open archive {}", scripts_zip.display());
        }
        GLOBAL_ARCHIVES.store(Arc::new(expansion_archives));
    }

    /// Read a file by name from the remembered archives.
    pub fn read_from_archives(name: &str) -> Option<Vec<u8>> {
        let guard = GLOBAL_ARCHIVES.load();
        for expansion_archive in guard.iter() {
            let mut archive_reader = expansion_archive.archive_reader.lock();
            if let Ok(mut file) = archive_reader.by_name(name) {
                let mut buffer = Vec::new();
                if file.read_to_end(&mut buffer).is_ok() {
                    return Some(buffer);
                }
            }
        }
        None
    }

    /// Read a file from disk, falling back to the archives.
    #[cfg(feature = "card")]
    pub fn read_file(name: &str) -> Option<Vec<u8>> {
        match fs::read(name) {
            Ok(data) => Some(data),
            Err(_) => read_from_archives(name),
        }
    }

    /// List the `.cdb` file names across the archives.
    #[cfg(feature = "card")]
    pub fn cdb_names() -> Vec<String> {
        let guard = GLOBAL_ARCHIVES.load();
        let mut names = Vec::new();
        for expansion_archive in guard.iter() {
            let mut archive_reader = expansion_archive.archive_reader.lock();
            for index in 0..archive_reader.len() {
                if let Ok(file) = archive_reader.by_index(index) {
                    if file.name().ends_with(".cdb") {
                        names.push(file.name().to_string());
                    }
                }
            }
        }
        names
    }

    fn is_expansion_archive(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension == "zip" || extension == "ypk")
            .unwrap_or(false)
    }
}
