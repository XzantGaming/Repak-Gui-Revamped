use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{fs, io};

use log::info;
use regex_lite::Regex;

// Use the runtime character_data module instead of compile-time embedded data
use crate::character_data;

static SKIN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Matches patterns like 1033/1033503 or 1033\1033503 (forward or backslash)
    Regex::new(r"[0-9]{4}[/\\][0-9]{7}").unwrap()
});

// Regex to extract just the character ID (4 digits) from paths like /Characters/1021/ or /Hero_ST/1048/
// Also matches directory mod paths like Marvel/Characters/1065/... (without leading slash)
static CHAR_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:Characters|Hero_ST|Hero)[/\\](\d{4})").unwrap());

// Regex to extract character ID from filenames (e.g., bnk_vo_1044001.bnk -> 1044)
// More strict pattern: requires the 7-digit skin ID to start with valid character ID range (10xx)
// This avoids false positives from random 7-digit numbers in filenames
static FILENAME_CHAR_ID_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // Matches patterns like _1044001, vo_1044001, where 1044 is a character ID in 10xx range
    Regex::new(r"[_/](10[1-6]\d)(\d{3})").unwrap()
});

/// Result of mod characteristics detection, includes mod type and detected heroes
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModCharacteristics {
    pub mod_type: String,
    pub heroes: Vec<String>,
    /// Character name for display (e.g., "Blade" or "Hawkeye - Default")
    /// Empty if no specific character or multiple characters
    pub character_name: String,
    /// Pure mod category (e.g., "Audio", "Mesh", "VFX")
    /// Without character name prefix
    pub category: String,
    /// Additional categories that can appear alongside the main category
    /// e.g., Blueprint, Text - these are additive and don't override the main category
    pub additional_categories: Vec<String>,
    /// 4-digit character ID (e.g., "1011" for Hulk)
    /// Empty if no character or multiple characters detected
    pub character_id: String,
}

impl ModCharacteristics {
    /// Format the mod type with hero info for display
    #[allow(dead_code)]
    pub fn display_type(&self) -> String {
        if self.heroes.is_empty() {
            self.mod_type.clone()
        } else if self.heroes.len() == 1 {
            format!("{} ({})", self.heroes[0], self.mod_type)
        } else {
            format!(
                "Multiple Heroes ({}) ({})",
                self.heroes.len(),
                self.mod_type
            )
        }
    }
}

pub fn collect_files(paths: &mut Vec<PathBuf>, dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(paths, &path)?;
        } else {
            paths.push(entry.path());
        }
    }
    Ok(())
}

pub enum ModType {
    Default(String),
    Custom(String),
}

/// Get character/skin info from file path using runtime character data cache
/// This uses the updated data from roaming folder, not the compile-time embedded data
pub fn get_character_mod_skin(file: &str) -> Option<ModType> {
    let skin_id_match = SKIN_REGEX.captures(file);
    if let Some(caps) = skin_id_match {
        let full_match = caps[0].to_string();
        info!("SKIN_REGEX matched: {} in file: {}", full_match, file);
        // Extract just the 7-digit skin ID (skip the "1234/" or "1234\" prefix)
        // The match is like "1033/1033503" or "1033\1033503", we want the last 7 digits
        let skin_id = &full_match[full_match.len() - 7..];
        info!("Extracted skin ID: {}", skin_id);

        // Use the runtime character data lookup
        if let Some(skin) = character_data::get_character_by_skin_id(skin_id) {
            info!("Found skin in database: {} - {}", skin.name, skin.skin_name);
            if skin.skin_name == "Default" {
                return Some(ModType::Default(format!(
                    "{} - {}",
                    &skin.name, &skin.skin_name
                )));
            }
            return Some(ModType::Custom(format!(
                "{} - {}",
                &skin.name, &skin.skin_name
            )));
        } else {
            info!("Skin ID {} not found in database", skin_id);
        }
        None
    } else {
        // Log first 100 chars of file path for debugging
        let preview = if file.len() > 100 { &file[..100] } else { file };
        info!("SKIN_REGEX did not match file: {}", preview);
        None
    }
}
/// Get detailed mod characteristics including mod type and all detected heroes
pub fn get_pak_characteristics_detailed(mod_contents: Vec<String>) -> ModCharacteristics {
    let mut _fallback: Option<String> = None;

    // Track what content types we find
    let mut has_skeletal_mesh = false;
    let mut has_static_mesh = false;
    let mut has_texture = false;
    let mut has_material = false;
    let mut has_audio = false;
    let mut has_movies = false;
    let mut has_ui = false;
    let mut has_blueprint = false;
    let mut has_text = false;
    let mut character_name: Option<String> = None; // Full skin-specific name (e.g., "Hawkeye - Default")
    let mut hero_names: HashSet<String> = HashSet::new(); // All detected hero names
    let mut detected_char_id: Option<String> = None; // Track the 4-digit character ID

    for file in &mod_contents {
        let path = file
            .strip_prefix("Marvel/Content/Marvel/")
            .or_else(|| file.strip_prefix("/Game/Marvel/"))
            .unwrap_or(file);

        let filename = path.split('/').last().unwrap_or("");
        let filename_lower = filename.to_lowercase();
        let path_lower = path.to_lowercase();

        // Check for specific asset types by filename pattern
        // Note: Internal paths may or may not have .uasset extension
        let is_uasset = filename_lower.ends_with(".uasset") || !filename_lower.contains('.');

        if filename_lower.starts_with("sk_") && is_uasset {
            has_skeletal_mesh = true;
        }
        if filename_lower.starts_with("sm_") && is_uasset {
            has_static_mesh = true;
        }
        if filename_lower.starts_with("t_") && is_uasset {
            has_texture = true;
        }

        // VFX: MI_ files in VFX path (e.g. /Game/Marvel/VFX/Materials/...)
        // Check both original file path and stripped path
        let file_lower = file.to_lowercase();
        if filename_lower.starts_with("mi_")
            && (path_lower.contains("/vfx/")
                || path_lower.starts_with("vfx/")
                || file_lower.contains("/vfx/"))
        {
            has_material = true;
        }

        // Check path-based categories
        if path_lower.contains("wwiseaudio") || file_lower.contains("wwiseaudio") {
            has_audio = true;
        }

        // UI: Files in UI folder
        if path_lower.contains("/ui/")
            || path_lower.starts_with("ui/")
            || file_lower.contains("/ui/")
        {
            has_ui = true;
        }

        // Movies: Files in Movies folder (placeholder - user to research exact criteria)
        if path_lower.contains("/movies/")
            || path_lower.starts_with("movies/")
            || file_lower.contains("/movies/")
            || path_lower.ends_with(".bik")
            || path_lower.ends_with(".mp4")
        {
            has_movies = true;
        }

        // Text: StringTable files (localization/text mods)
        if path_lower.contains("/stringtable/")
            || path_lower.starts_with("stringtable/")
            || file_lower.contains("/stringtable/")
            || path_lower.contains("/data/stringtable/")
        {
            has_text = true;
        }

        // Blueprint: Common Blueprint patterns
        // 1. BP_Something (Blueprint prefix)
        // 2. Something_C (Blueprint class suffix)
        // 3. SomethingBP (Blueprint suffix)
        // 4. /Blueprints/ folder path
        if (filename_lower.starts_with("bp_")
            || filename_lower.contains("_c.")
            || filename_lower.contains("bp.")
            || filename_lower.ends_with("bp")
            || path_lower.contains("/blueprints/"))
            && is_uasset
        {
            has_blueprint = true;
        }

        // Try to get skin-specific name from any path containing the skin pattern
        match get_character_mod_skin(path) {
            Some(ModType::Custom(skin)) => {
                info!("Found custom skin: {} from path: {}", skin, path);
                character_name = Some(skin);
            }
            Some(ModType::Default(name)) => {
                info!("Found default skin: {} from path: {}", name, path);
                _fallback = Some(name);
            }
            None => {
                // No skin-specific match, will use character ID detection below
            }
        }

        // Extract hero names ONLY from primary character folder paths
        // This handles paths like /Characters/1032/, /Hero_ST/1048/, /Hero/1021/
        // We ONLY look at folder structure, NOT filenames, to avoid false positives from shared assets
        let path_matched = CHAR_ID_REGEX
            .captures(file)
            .and_then(|caps| {
                caps.get(1).and_then(|char_id| {
                    let id = char_id.as_str();
                    character_data::get_character_name_from_id(id).map(|name| {
                        info!(
                            "CHAR_ID_REGEX matched ID {} ({}) in path: {}",
                            id, name, file
                        );
                        hero_names.insert(name);
                        // Store the character ID if this is the first one detected
                        if detected_char_id.is_none() {
                            detected_char_id = Some(id.to_string());
                        }
                        true
                    })
                })
            })
            .unwrap_or(false);

        // Fallback: For audio/UI/texture mods WITHOUT character folders, check filenames
        // ONLY use this when path regex didn't match - prevents false positives from shared texture assets
        // (e.g., T_1018301 texture in a 1048 folder shouldn't count as hero 1018)
        if !path_matched && !filename_lower.starts_with("mi_") {
            if let Some(caps) = FILENAME_CHAR_ID_REGEX.captures(filename) {
                if let Some(char_id) = caps.get(1) {
                    let id = char_id.as_str();
                    if let Some(name) = character_data::get_character_name_from_id(id) {
                        info!(
                            "FILENAME_CHAR_ID_REGEX matched ID {} ({}) in filename: {}",
                            id, name, filename
                        );
                        hero_names.insert(name);
                        // Store the character ID if this is the first one detected
                        if detected_char_id.is_none() {
                            detected_char_id = Some(id.to_string());
                        }
                    }
                }
            }
        }
    }

    // Convert to sorted Vec for consistent ordering
    let mut heroes: Vec<String> = hero_names.into_iter().collect();
    heroes.sort();

    // Determine the pure category (without character name)
    // Priority order: Audio/Movies/UI (pure) > Mesh > Static Mesh > VFX > Audio (mixed) > Texture
    // Note: Blueprint and Text are now additive categories and handled separately
    let category = if has_audio
        && !has_skeletal_mesh
        && !has_static_mesh
        && !has_texture
        && !has_material
    {
        "Audio"
    } else if has_movies && !has_skeletal_mesh && !has_static_mesh && !has_texture && !has_material
    {
        "Movies"
    } else if has_ui && !has_skeletal_mesh && !has_static_mesh && !has_texture && !has_material {
        "UI"
    } else if has_skeletal_mesh {
        "Mesh"
    } else if has_static_mesh {
        "Static Mesh"
    } else if has_material {
        "VFX"
    } else if has_audio {
        "Audio"
    } else if has_texture {
        "Texture"
    } else if has_blueprint {
        // Blueprint-only mod (no other primary category detected)
        "Blueprint"
    } else if has_text {
        // Text-only mod (no other primary category detected)
        "Text"
    } else {
        "Unknown"
    };

    // Determine character_name for display
    // Priority: multiple heroes > skin-specific name > single hero > empty
    let display_character_name = if heroes.len() > 1 {
        // Multiple heroes detected - leave empty so "Multiple Heroes" is shown
        String::new()
    } else if let Some(ref char_name) = character_name {
        // Single hero with skin-specific name
        char_name.clone()
    } else if heroes.len() == 1 {
        // Single hero without skin info
        heroes[0].clone()
    } else {
        String::new()
    };

    // Build additional categories list (Additive types)
    let mut additional_categories = Vec::new();
    if has_skeletal_mesh && category != "Mesh" {
        additional_categories.push("Mesh".to_string());
    }
    if has_static_mesh && category != "Static Mesh" {
        additional_categories.push("Static Mesh".to_string());
    }
    if has_texture && category != "Texture" {
        additional_categories.push("Texture".to_string());
    }
    if has_material && category != "VFX" {
        additional_categories.push("VFX".to_string());
    }
    if has_audio && category != "Audio" {
        additional_categories.push("Audio".to_string());
    }
    if has_ui && category != "UI" {
        additional_categories.push("UI".to_string());
    }
    if has_blueprint && category != "Blueprint" {
        additional_categories.push("Blueprint".to_string());
    }
    if has_text && category != "Text" {
        additional_categories.push("Text".to_string());
    }

    // Build the combined mod_type string with " - " separator for easy splitting
    // Include additional categories in the display string
    let base_type = if !display_character_name.is_empty() {
        // Character detected - combine with " - " separator
        format!("{} - {}", display_character_name, category)
    } else if heroes.len() > 1 {
        // Multiple heroes
        format!("Multiple Heroes ({}) - {}", heroes.len(), category)
    } else {
        // No heroes detected - just category
        category.to_string()
    };

    // Append additional categories to the mod_type string
    let mod_type = if !additional_categories.is_empty() {
        format!("{} [{}]", base_type, additional_categories.join(", "))
    } else {
        base_type
    };

    // Determine final character_id: only set if exactly one hero detected
    let final_character_id = if heroes.len() == 1 {
        detected_char_id.unwrap_or_default()
    } else {
        String::new()
    };

    ModCharacteristics {
        mod_type,
        heroes,
        character_name: display_character_name,
        category: category.to_string(),
        additional_categories,
        character_id: final_character_id,
    }
}

/// Get mod characteristics as a display string (backward compatible)
/// Returns the mod_type string which uses " - " separator between character and category
/// Format: "Character - Category" or just "Category" if no character
pub fn get_current_pak_characteristics(mod_contents: Vec<String>) -> String {
    let chars = get_pak_characteristics_detailed(mod_contents);
    chars.mod_type
}

pub fn find_marvel_rivals() -> Option<PathBuf> {
    let shit = get_steam_library_paths();
    if shit.is_empty() {
        return None;
    }

    for lib in shit {
        let path = lib.join("steamapps/common/MarvelRivals/MarvelGame/Marvel/Content/Paks");
        if path.exists() {
            return Some(path);
        }
    }
    println!("Marvel Rivals not found.");
    None
}

/// Reads `libraryfolders.vdf` to find additional Steam libraries.
/// Enhanced to check registry and multiple common locations.
fn get_steam_library_paths() -> Vec<PathBuf> {
    let mut vdf_paths_to_check: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // Try to get Steam path from Windows registry first
        if let Some(steam_path) = get_steam_path_from_registry() {
            let vdf = steam_path.join("steamapps/libraryfolders.vdf");
            info!("Found Steam path from registry: {:?}", vdf);
            vdf_paths_to_check.push(vdf);
        }

        // Common Steam installation paths to check as fallbacks
        let common_paths = [
            "C:/Program Files (x86)/Steam",
            "C:/Program Files/Steam",
            "D:/Steam",
            "D:/Program Files (x86)/Steam",
            "D:/Program Files/Steam",
            "E:/Steam",
            "E:/SteamLibrary",
            "F:/Steam",
            "F:/SteamLibrary",
        ];

        for path in common_paths {
            let vdf = PathBuf::from(path).join("steamapps/libraryfolders.vdf");
            if !vdf_paths_to_check.contains(&vdf) {
                vdf_paths_to_check.push(vdf);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Expand home directory properly
        if let Some(home) = dirs::home_dir() {
            vdf_paths_to_check.push(home.join(".steam/steam/steamapps/libraryfolders.vdf"));
            vdf_paths_to_check.push(home.join(".local/share/Steam/steamapps/libraryfolders.vdf"));
        }
    }

    // Find first existing VDF file
    let vdf_path = vdf_paths_to_check.into_iter().find(|p| p.exists());

    let Some(vdf_path) = vdf_path else {
        info!("No Steam libraryfolders.vdf found");
        return vec![];
    };

    info!("Using Steam library config: {:?}", vdf_path);

    let content = fs::read_to_string(&vdf_path).ok().unwrap_or_default();
    let mut paths = Vec::new();

    for line in content.lines() {
        if line.trim().starts_with("\"path\"") {
            let path = line
                .split("\"")
                .nth(3)
                .map(|s| PathBuf::from(s.replace("\\\\", "\\")));
            info!("Found steam library path: {:?}", path);
            if let Some(p) = path {
                paths.push(p);
            }
        }
    }

    paths
}

/// Get Steam installation path from Windows registry
#[cfg(target_os = "windows")]
fn get_steam_path_from_registry() -> Option<PathBuf> {
    use std::process::Command;

    // Query registry for Steam install path
    // reg query "HKCU\Software\Valve\Steam" /v SteamPath
    let output = Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse output: "    SteamPath    REG_SZ    C:\Program Files (x86)\Steam"
    for line in stdout.lines() {
        if line.contains("SteamPath") && line.contains("REG_SZ") {
            // Split by REG_SZ and take the path part
            if let Some(path_part) = line.split("REG_SZ").nth(1) {
                let path = path_part.trim();
                if !path.is_empty() {
                    info!("Found Steam path in registry: {}", path);
                    return Some(PathBuf::from(path));
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn get_steam_path_from_registry() -> Option<PathBuf> {
    None
}
