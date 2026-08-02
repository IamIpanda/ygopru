pub mod archive_manager {
    use std::fs;
    use std::io::Read;
    use std::sync::Arc;

    use arc_swap::ArcSwapOption;
    use parking_lot::Mutex;
    use zip::ZipArchive;
    use log::warn;

    struct ExpansionArchive {
        path: String,
        archive_reader: Mutex<ZipArchive<fs::File>>,
    }

    static GLOBAL_ARCHIVES: ArcSwapOption<Vec<ExpansionArchive>> = ArcSwapOption::const_empty();

    pub fn init() {
        let mut expansion_archives = Vec::new();
        let Ok(entries) = fs::read_dir("./expansions") else {
            warn!("Failed to read directory ./expansions");
            GLOBAL_ARCHIVES.store(Some(Arc::new(expansion_archives)));
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_expansion_archive(&path) {
                continue;
            }
            let Ok(file) = fs::File::open(&path) else {
                warn!("Failed to open archive {}", path.display());
                continue;
            };
            match ZipArchive::new(file) {
                Ok(archive_reader) => expansion_archives.push(ExpansionArchive {
                    path: path.display().to_string(),
                    archive_reader: Mutex::new(archive_reader),
                }),
                Err(error) => warn!("Failed to open archive {}: {}", path.display(), error),
            }
        }
        GLOBAL_ARCHIVES.store(Some(Arc::new(expansion_archives)));
    }

    pub fn read_from_archives(name: &str) -> Option<Vec<u8>> {
        let guard = GLOBAL_ARCHIVES.load();
        for expansion_archive in (*guard).as_ref()?.iter() {
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

    pub fn read_file(name: &str) -> Option<Vec<u8>> {
        match fs::read(name) {
            Ok(data) => Some(data),
            Err(_) => read_from_archives(name),
        }
    }

    pub fn cdb_names() -> Vec<String> {
        let guard = GLOBAL_ARCHIVES.load();
        let mut names = Vec::new();
        if let Some(archives) = (*guard).as_ref() {
            for expansion_archive in archives.iter() {
                let mut archive_reader = expansion_archive.archive_reader.lock();
                for index in 0..archive_reader.len() {
                    if let Ok(file) = archive_reader.by_index(index) {
                        if file.name().ends_with(".cdb") {
                            names.push(file.name().to_string());
                        }
                    }
                }
            }
        }
        names
    }

    fn is_expansion_archive(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension == "zip" || extension == "ypk")
            .unwrap_or(false)
    }
}
