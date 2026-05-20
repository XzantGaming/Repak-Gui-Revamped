pub mod install_mod_logic;

use crate::install_mod::install_mod_logic::archives::{extract_7z, extract_rar, extract_zip};
use crate::utils::{collect_files, get_current_pak_characteristics};
use crate::utoc_utils::read_utoc;
use log::{debug, error};
use std::path::PathBuf;
use tempfile::tempdir;
use walkdir::WalkDir;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallableMod {
    pub mod_name: String,
    pub mod_type: String,
    pub custom_tags: Vec<String>,
    pub custom_tag_input: String,
    pub repak: bool,
    pub is_dir: bool,
    pub editing: bool,
    pub path_hash_seed: String,
    pub mount_point: String,
    #[serde(skip)]
    pub compression: String,
    #[serde(skip)]
    pub reader: Option<Vec<String>>,
    pub mod_path: PathBuf,
    pub total_files: usize,
    pub iostore: bool,
    // the only reason we keep this is to filter out the archives during collection
    pub is_archived: bool,
    pub enabled: bool,
    // pub audio_mod: bool,
    /// Whether the mod contains any .uasset/.uexp/.ubulk/.umap files
    /// Used by frontend to lock/unlock certain toggles (e.g., fix texture only applies to uasset mods)
    pub contains_uassets: bool,
    /// Force legacy PAK format instead of IoStore conversion
    /// Used for Audio/Config mods that don't need IoStore processing
    pub force_legacy_pak: bool,
    /// Subfolder within the mods directory to install into (empty = root)
    pub install_subfolder: String,
    /// Enable parallel processing for batch operations (texture stripping, etc.)
    #[serde(default)]
    pub parallel_processing: bool,
    /// Enable obfuscation (encrypts IoStore with game's AES key to block extraction tools like FModel)
    #[serde(default)]
    pub obfuscate: bool,
}

impl Default for InstallableMod {
    fn default() -> Self {
        InstallableMod {
            mod_name: "".to_string(),
            mod_type: "".to_string(),
            custom_tags: Vec::new(),
            custom_tag_input: String::new(),
            repak: false,
            is_dir: false,
            editing: false,
            path_hash_seed: "".to_string(),
            mount_point: "".to_string(),
            compression: "Oodle".to_string(),
            reader: None,
            mod_path: Default::default(),
            total_files: 0,
            iostore: false,
            is_archived: false,
            enabled: true,
            contains_uassets: true, // Default to true for safety
            force_legacy_pak: false,
            install_subfolder: String::new(),
            parallel_processing: false,
            obfuscate: false,
        }
    }
}

/// Returns true if the file list contains any UAsset-related files
/// (.uasset, .uexp, .ubulk, .umap)
pub fn contains_uasset_files(files: &[String]) -> bool {
    files.iter().any(|f| {
        let lower = f.to_lowercase();
        lower.ends_with(".uasset")
            || lower.ends_with(".uexp")
            || lower.ends_with(".ubulk")
            || lower.ends_with(".umap")
    })
}

pub const AES_KEY_HEX: &str = "0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74";

fn find_mods_from_archive(path: &str) -> Vec<InstallableMod> {
    write_install_debug(&format!(
        "=== find_mods_from_archive called: path={} ===",
        path
    ));
    write_install_debug(&format!(
        "  path exists: {}",
        std::path::Path::new(path).exists()
    ));
    let mut new_mods = Vec::<InstallableMod>::new();
    let mut processed_mods = std::collections::HashSet::new();
    let mut found_pak_files = false;

    // First pass: look for .pak files (existing behavior)
    for entry in WalkDir::new(path) {
        let entry = entry.expect("Failed to read directory entry");
        let file_path = entry.path();

        // Only process .pak files
        if file_path.is_file() && file_path.extension().and_then(|s| s.to_str()) == Some("pak") {
            found_pak_files = true;
            let mod_base_name = file_path.file_stem().unwrap().to_str().unwrap().to_string();
            write_install_debug(&format!(
                "  Found PAK: {} (base_name={})",
                file_path.display(),
                mod_base_name
            ));

            // Skip if we've already processed this mod
            if processed_mods.contains(&mod_base_name) {
                write_install_debug(&format!("  Skipping duplicate: {}", mod_base_name));
                continue;
            }
            processed_mods.insert(mod_base_name.clone());

            let utoc_path = file_path.with_extension("utoc");
            let ucas_path = file_path.with_extension("ucas");
            write_install_debug(&format!(
                "  utoc exists: {}, ucas exists: {}",
                utoc_path.exists(),
                ucas_path.exists()
            ));

            // Check if this is an iostore mod (has all three files: pak, utoc, ucas)
            if utoc_path.exists() && ucas_path.exists() {
                write_install_debug("  -> IoStore path (copy)");
                // This is an iostore mod - read file list from utoc (works with obfuscated mods)
                // Don't require PAK reading since obfuscated mods have encrypted PAK indexes
                let files = read_utoc(&utoc_path);
                let files = files
                    .iter()
                    .map(|x| x.file_path.clone())
                    .collect::<Vec<_>>();
                let len = files.len();
                let modtype = get_current_pak_characteristics(files.clone());
                let has_uassets = contains_uasset_files(&files);

                let installable_mod = InstallableMod {
                    mod_name: mod_base_name,
                    mod_type: modtype.to_string(),
                    repak: false, // Don't use repak workflow for iostore mods
                    is_dir: false,
                    reader: None,
                    mod_path: file_path.to_path_buf(),
                    mount_point: "../../../".to_string(),
                    path_hash_seed: "00000000".to_string(),
                    total_files: len,
                    iostore: true, // Mark as iostore so it gets copied directly
                    is_archived: false,
                    editing: false,
                    compression: "Oodle".to_string(),
                    contains_uassets: has_uassets,
                    ..Default::default()
                };

                new_mods.push(installable_mod);
            }
            // This is a standalone .pak file
            else {
                write_install_debug("  -> Standalone PAK path (repak)");
                let files = uasset_toolkit::list_pak_files(
                    file_path.to_str().unwrap_or_default(),
                    Some(AES_KEY_HEX),
                )
                .unwrap_or_default();
                let len = files.len();
                let modtype = get_current_pak_characteristics(files.clone());
                let has_uassets = contains_uasset_files(&files);
                let is_audio_or_movie = modtype.contains("Audio") || modtype.contains("Movies");

                let installable_mod = InstallableMod {
                    mod_name: mod_base_name,
                    mod_type: modtype.to_string(),
                    repak: !is_audio_or_movie,
                    is_dir: false,
                    reader: if files.is_empty() { None } else { Some(files) },
                    mod_path: file_path.to_path_buf(),
                    mount_point: "../../../".to_string(),
                    path_hash_seed: "00000000".to_string(),
                    total_files: len,
                    iostore: false,
                    is_archived: false,
                    editing: false,
                    compression: "Oodle".to_string(),
                    contains_uassets: has_uassets,
                    ..Default::default()
                };

                write_install_debug(&format!(
                    "  Created InstallableMod: name={}, repak={}, iostore={}, type={}, files={}",
                    installable_mod.mod_name,
                    installable_mod.repak,
                    installable_mod.iostore,
                    installable_mod.mod_type,
                    installable_mod.total_files
                ));
                new_mods.push(installable_mod);
            }
        }
    }
    write_install_debug(&format!(
        "find_mods_from_archive result: {} mods found, found_pak_files={}",
        new_mods.len(),
        found_pak_files
    ));

    // Second pass: if no .pak files found, look for content folders with .uasset files
    // This handles archives that contain loose mod files (folders) instead of pre-packed .pak files
    if !found_pak_files {
        debug!("No .pak files found in archive, looking for content folders...");

        // Find directories that contain .uasset files (these are mod content folders)
        let archive_root = std::path::Path::new(path);

        // Check immediate subdirectories of the archive root
        if let Ok(entries) = std::fs::read_dir(archive_root) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                // Check if this is a directory that contains content
                if entry_path.is_dir() {
                    let mut has_content = false;
                    let mut content_files = Vec::new();

                    // Recursively collect files and check for .uasset content
                    if collect_files(&mut content_files, &entry_path).is_ok() {
                        for file in &content_files {
                            if let Some(ext) = file.extension().and_then(|s| s.to_str()) {
                                if ext == "uasset"
                                    || ext == "uexp"
                                    || ext == "ubulk"
                                    || ext == "bnk"
                                    || ext == "wem"
                                {
                                    has_content = true;
                                    break;
                                }
                            }
                        }
                    }

                    if has_content {
                        let mod_name = entry_path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Unknown")
                            .to_string();

                        debug!(
                            "Found content folder in archive: {} ({} files)",
                            mod_name,
                            content_files.len()
                        );

                        // Get file paths as strings for mod type detection
                        let file_strings: Vec<String> = content_files
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect();

                        let modtype = get_current_pak_characteristics(file_strings.clone());
                        let has_uassets = contains_uasset_files(&file_strings);
                        let is_audio_or_movies =
                            modtype.contains("Audio") || modtype.contains("Movies");

                        let installable_mod = InstallableMod {
                            mod_name,
                            mod_type: modtype.to_string(),
                            repak: !is_audio_or_movies, // Will go through convert_to_iostore_directory
                            is_dir: true, // Mark as directory so it uses convert_to_iostore_directory
                            reader: None,
                            mod_path: entry_path,
                            mount_point: "../../../".to_string(),
                            path_hash_seed: "00000000".to_string(),
                            total_files: content_files.len(),
                            iostore: false,
                            is_archived: false,
                            editing: false,
                            compression: "Oodle".to_string(),
                            contains_uassets: has_uassets,
                            ..Default::default()
                        };

                        new_mods.push(installable_mod);
                    }
                }
            }
        }

        // If still no mods found, check if the archive root itself contains content files directly
        if new_mods.is_empty() {
            let mut content_files = Vec::new();
            if collect_files(&mut content_files, archive_root).is_ok() {
                let has_content = content_files.iter().any(|f| {
                    f.extension()
                        .and_then(|s| s.to_str())
                        .map(|ext| {
                            ext == "uasset"
                                || ext == "uexp"
                                || ext == "ubulk"
                                || ext == "bnk"
                                || ext == "wem"
                        })
                        .unwrap_or(false)
                });

                if has_content {
                    // Use the archive folder name as mod name
                    let mod_name = archive_root
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("ExtractedMod")
                        .to_string();

                    debug!(
                        "Archive root contains content files: {} ({} files)",
                        mod_name,
                        content_files.len()
                    );

                    let file_strings: Vec<String> = content_files
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();

                    let modtype = get_current_pak_characteristics(file_strings.clone());
                    let has_uassets = contains_uasset_files(&file_strings);
                    let is_audio_or_movies =
                        modtype.contains("Audio") || modtype.contains("Movies");
                    let installable_mod = InstallableMod {
                        mod_name,
                        mod_type: modtype.to_string(),
                        repak: !is_audio_or_movies,
                        is_dir: true,
                        reader: None,
                        mod_path: archive_root.to_path_buf(),
                        mount_point: "../../../".to_string(),
                        path_hash_seed: "00000000".to_string(),
                        total_files: content_files.len(),
                        iostore: false,
                        is_archived: false,
                        editing: false,
                        compression: "Oodle".to_string(),
                        contains_uassets: has_uassets,
                        ..Default::default()
                    };

                    new_mods.push(installable_mod);
                }
            }
        }
    }

    new_mods
}

pub fn write_install_debug(msg: &str) {
    if let Some(config_dir) = dirs::config_dir() {
        let debug_log = config_dir.join("Repak-X").join("install_debug.log");
        let _ = std::fs::create_dir_all(debug_log.parent().unwrap());
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&debug_log)
        {
            let _ = writeln!(f, "{}", msg);
        }
    }
}

fn map_to_mods_internal(paths: &[PathBuf]) -> Vec<InstallableMod> {
    // Clear debug log
    if let Some(config_dir) = dirs::config_dir() {
        let debug_log = config_dir.join("Repak-X").join("install_debug.log");
        let _ = std::fs::write(&debug_log, "");
    }
    write_install_debug(&format!(
        "=== map_to_mods_internal called with {} paths ===",
        paths.len()
    ));
    for p in paths {
        write_install_debug(&format!(
            "  path: {} (exists={}, ext={:?})",
            p.display(),
            p.exists(),
            p.extension()
        ));
    }

    let mut extensible_vec: Vec<InstallableMod> = Vec::new();
    let mut installable_mods = paths
        .iter()
        .map(|path| {
            let is_dir = path.clone().is_dir();
            let extension = path.extension().unwrap_or_default();
            let is_archive = extension == "zip" || extension == "rar" || extension == "7z";
            write_install_debug(&format!(
                "Processing: {} is_dir={} is_archive={} ext={:?}",
                path.display(),
                is_dir,
                is_archive,
                extension
            ));

            // Check if this is an IoStore package (has .utoc and .ucas companions)
            let is_iostore = if extension == "pak" {
                let utoc_path = path.with_extension("utoc");
                let ucas_path = path.with_extension("ucas");
                utoc_path.exists() && ucas_path.exists()
            } else {
                false
            };

            let mut modtype = "Unknown".to_string();
            let mut pak = None;
            let mut len = 1;
            let mut has_uassets = true; // Default to true for safety

            if !is_dir && !is_archive {
                if is_iostore {
                    // For IoStore packages, read from .utoc file directly (works with obfuscated mods)
                    // Don't require PAK reading since obfuscated mods have encrypted PAK indexes
                    let utoc_path = path.with_extension("utoc");
                    let utoc_files = read_utoc(&utoc_path);
                    len = utoc_files.len();
                    let files: Vec<String> =
                        utoc_files.iter().map(|f| f.file_path.clone()).collect();

                    modtype = get_current_pak_characteristics(files.clone());
                    has_uassets = contains_uasset_files(&files);

                    pak = uasset_toolkit::list_pak_files(
                        path.to_str().unwrap_or_default(),
                        Some(AES_KEY_HEX),
                    )
                    .ok()
                    .filter(|f| !f.is_empty());
                } else {
                    match uasset_toolkit::list_pak_files(
                        path.to_str().unwrap_or_default(),
                        Some(AES_KEY_HEX),
                    ) {
                        Ok(files) => {
                            len = files.len();
                            modtype = get_current_pak_characteristics(files.clone());
                            has_uassets = contains_uasset_files(&files);
                            pak = if files.is_empty() { None } else { Some(files) };
                        }
                        Err(e) => {
                            error!("Error reading pak file: {}", e);
                            return None;
                        }
                    }
                }
            }

            if is_dir {
                let mut files = vec![];
                collect_files(&mut files, path).ok();
                let files = files
                    .iter()
                    .map(|s| s.to_str().unwrap().to_string())
                    .collect::<Vec<_>>();
                len = files.len();
                modtype = get_current_pak_characteristics(files.clone());
                has_uassets = contains_uasset_files(&files);
            }

            if is_archive {
                modtype = "Archive".to_string();
                // IMPORTANT: Keep the TempDir alive so the directory isn't deleted
                // before find_mods_from_archive reads the PAK files inside it.
                // We leak the TempDir intentionally so the temp directory persists
                // until the installation completes (OS cleans up on process exit).
                let temp_dir_obj = tempdir().unwrap();
                let tempdir = temp_dir_obj.path().to_str().unwrap().to_string();
                // Leak the TempDir to prevent cleanup - the extracted PAK files
                // must remain accessible during the entire installation flow
                std::mem::forget(temp_dir_obj);

                if extension == "zip" {
                    extract_zip(path.to_str().unwrap(), &tempdir)
                        .expect("Unable to extract zip archive")
                } else if extension == "rar" {
                    extract_rar(path.to_str().unwrap(), &tempdir)
                        .expect("Unable to extract rar archive")
                } else if extension == "7z" {
                    extract_7z(path.to_str().unwrap(), &tempdir)
                        .expect("Unable to extract 7z archive")
                }

                // Now find pak files / iostore mods and turn them into installable mods
                let mut new_mods = find_mods_from_archive(&tempdir);
                extensible_vec.append(&mut new_mods);
            }

            // Determine if we should repak this mod
            // Don't repak if: it's a directory, IoStore package, or Audio/Movies mod
            let is_audio_or_movies = modtype.contains("Audio") || modtype.contains("Movies");
            let should_repak = !is_dir && !is_iostore && !is_audio_or_movies;

            Some(InstallableMod {
                mod_name: path.file_stem().unwrap().to_str().unwrap().to_string(),
                mod_type: modtype,
                repak: should_repak,
                is_dir,
                reader: pak,
                mod_path: path.clone(),
                mount_point: "../../../".to_string(),
                path_hash_seed: "00000000".to_string(),
                total_files: len,
                iostore: is_iostore, // Mark as IoStore package
                is_archived: is_archive,
                contains_uassets: has_uassets,
                ..Default::default()
            })
        })
        .filter_map(|x| x)
        .filter(|x| !x.is_archived)
        .collect::<Vec<_>>();

    installable_mods.extend(extensible_vec);

    debug!("Install mods: {:?}", installable_mods);
    installable_mods
}

pub fn map_paths_to_mods(paths: &[PathBuf]) -> Vec<InstallableMod> {
    let installable_mods = map_to_mods_internal(paths);
    installable_mods
}

// Egui-specific function - stubbed out for Tauri
#[allow(dead_code)]
pub fn map_dropped_file_to_mods(_dropped_files: &[PathBuf]) -> Vec<InstallableMod> {
    // This function signature is changed for Tauri compatibility
    // Original egui version uses egui::DroppedFile
    unimplemented!("This function is only available in the egui version")
}
