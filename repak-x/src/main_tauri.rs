// Tauri-based main.rs - React + Tauri implementation
// Original egui implementation backed up in src/egui_backup_original/

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod character_data;
mod crash_monitor;
mod discord_presence;
mod hero_assets;
mod install_mod;
mod ip_obfuscation;
mod merge_command;
mod p2p_libp2p;
mod p2p_manager;
mod p2p_protocol;
mod p2p_security;
mod p2p_sharing;
mod p2p_stream;
mod toast_events;
mod uasset_api_integration;
mod uasset_detection;
mod utils;
mod utoc_utils;
mod vfx_updater;

use log::{error, info, warn};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Listener, Manager, State, Window};
use uasset_detection::detect_texture_files_async;
use utils::find_marvel_rivals;
use walkdir::WalkDir;

// ============================================================================
// STATE MANAGEMENT
// ============================================================================

struct WatcherState {
    watcher: Mutex<Option<RecommendedWatcher>>,
    #[allow(dead_code)]
    last_event_time: Mutex<std::time::Instant>,
    /// When true, the file watcher suppresses events (e.g. during P2P transfer)
    paused: Arc<AtomicBool>,
}

/// P2P Sharing state management
struct P2PState {
    manager: Arc<p2p_manager::UnifiedP2PManager>,
}

/// Crash monitoring state
struct CrashMonitorState {
    game_start_time: Mutex<Option<std::time::SystemTime>>,
    last_checked_crash: Mutex<Option<std::time::SystemTime>>,
}

/// Discord Rich Presence state
struct DiscordState {
    manager: discord_presence::SharedDiscordPresence,
}

fn default_theme() -> String {
    "dark".to_string()
}
fn default_accent() -> String {
    "red".to_string()
}
fn default_view_mode() -> String {
    "list".to_string()
}
fn default_true() -> bool {
    true
}

fn default_launcher_type() -> String {
    "steam".to_string()
}

const ACCENT_PRESETS: &[(&str, &str)] = &[
    ("red", "#be1c1c"),
    ("blue", "#4a9eff"),
    ("purple", "#9c27b0"),
    ("green", "#4CAF50"),
    ("orange", "#ff9800"),
    ("pink", "#FF96BC"),
];

fn resolve_accent_name(color_str: &str) -> String {
    let lower = color_str.to_lowercase();
    match lower.as_str() {
        "red" | "#be1c1c" | "#d11f1f" | "repakred" => "red".to_string(),
        "blue" | "#4a9eff" => "blue".to_string(),
        "purple" | "#9c27b0" => "purple".to_string(),
        "green" | "#4caf50" => "green".to_string(),
        "orange" | "#ff9800" => "orange".to_string(),
        "pink" | "#ff96bc" => "pink".to_string(),
        _ => "red".to_string(), // Fallback to RepakRed
    }
}

fn deserialize_accent_color<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt: Option<String> = Option::deserialize(deserializer)?;
    let val = opt.unwrap_or_else(default_accent);
    Ok(resolve_accent_name(&val))
}

#[derive(Serialize, Deserialize)]
struct AppState {
    game_path: PathBuf,

    // Settings
    #[serde(default = "default_theme")]
    theme: String,
    #[serde(
        deserialize_with = "deserialize_accent_color",
        default = "default_accent"
    )]
    accent_color: String,
    #[serde(default = "default_view_mode")]
    view_mode: String,
    #[serde(default)]
    hide_suffix: bool,
    #[serde(default)]
    auto_open_details: bool,
    #[serde(default)]
    show_hero_icons: bool,
    #[serde(default)]
    show_hero_bg: bool,
    #[serde(default)]
    show_mod_type: bool,
    #[serde(default)]
    show_experimental: bool,
    #[serde(default = "default_true")]
    auto_check_updates: bool,
    #[serde(default)]
    parallel_processing: bool,
    #[serde(default = "default_true")]
    hold_to_delete: bool,
    #[serde(default = "default_true")]
    show_subfolder_mods: bool,
    #[serde(default)]
    bypass_game_running_lock: bool,
    #[serde(default = "default_true")]
    enable_drp: bool,
    #[serde(default = "default_launcher_type")]
    launcher_type: String,

    custom_tag_catalog: Vec<String>,
    #[serde(default)]
    mod_tags: std::collections::HashMap<String, Vec<String>>,
    /// Last known crash folder name for detecting crashes from previous sessions
    #[serde(default)]
    last_known_crash_folder: Option<String>,
    /// Cache for mod details to avoid redundant PAK opens (path -> (mtime, details))
    #[serde(skip)]
    mod_details_cache: std::collections::HashMap<PathBuf, (std::time::SystemTime, ModDetails)>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            game_path: PathBuf::new(),
            theme: default_theme(),
            accent_color: default_accent(),
            view_mode: default_view_mode(),
            hide_suffix: false,
            auto_open_details: false,
            show_hero_icons: false,
            show_hero_bg: false,
            show_mod_type: false,
            show_experimental: false,
            auto_check_updates: default_true(),
            parallel_processing: false,
            hold_to_delete: default_true(),
            show_subfolder_mods: default_true(),
            bypass_game_running_lock: false,
            enable_drp: default_true(),
            launcher_type: default_launcher_type(),
            custom_tag_catalog: Vec::new(),
            mod_tags: std::collections::HashMap::new(),
            last_known_crash_folder: None,
            mod_details_cache: std::collections::HashMap::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ModFolder {
    id: String,
    name: String,
    enabled: bool,
    expanded: bool,
    color: Option<[u8; 3]>,
    /// Depth in folder hierarchy (0 = root, 1 = direct child, etc.)
    #[serde(default)]
    depth: usize,
    /// Parent folder ID (None = root folder, "_root" for root's direct children)
    #[serde(default)]
    parent_id: Option<String>,
    /// Is this the root folder (the ~mods directory itself)
    #[serde(default)]
    is_root: bool,
    /// Number of mods directly in this folder
    #[serde(default)]
    mod_count: usize,
}

/// Root folder info for hierarchy display
#[derive(Clone, Serialize, Deserialize)]
struct RootFolderInfo {
    /// The actual folder name (e.g., "~mods")
    name: String,
    /// Full path to the root folder
    path: String,
    /// Total number of mods in root (not in subfolders)
    direct_mod_count: usize,
    /// Total number of subfolders
    subfolder_count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
struct ModEntry {
    path: PathBuf,
    utoc_path: Option<PathBuf>,
    enabled: bool,
    custom_name: Option<String>,
    folder_id: Option<String>,
    custom_tags: Vec<String>,
    file_size: u64,
    modified_date: u64,
    priority: usize,
    // Character/skin info from character_data (dynamically looked up)
    character_name: Option<String>,
    skin_name: Option<String>,
}

// ============================================================================
// TAURI COMMANDS
// ============================================================================

fn get_mod_key(path: &std::path::Path) -> String {
    let mut name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if name.ends_with(".bak_repak") {
        name = name.strip_suffix(".bak_repak").unwrap().to_string();
    }
    if name.ends_with("_disabled") {
        name = name.strip_suffix("_disabled").unwrap().to_string();
    }
    if name.ends_with("_P") {
        name = name.strip_suffix("_P").unwrap().to_string();
    }

    if let Some(idx) = name.rfind('_') {
        let suffix = &name[idx + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c == '9') {
            name.truncate(idx);
        }
    }

    name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccentColorPreset {
    name: String,
    hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    theme: String,
    accent_color: String,
    view_mode: String,
    hide_suffix: bool,
    auto_open_details: bool,
    show_hero_icons: bool,
    show_hero_bg: bool,
    show_mod_type: bool,
    show_experimental: bool,
    auto_check_updates: bool,
    parallel_processing: bool,
    hold_to_delete: bool,
    show_subfolder_mods: bool,
    bypass_game_running_lock: bool,
    enable_drp: bool,
    launcher_type: String,
}

#[tauri::command]
async fn get_accent_presets() -> Result<Vec<AccentColorPreset>, String> {
    Ok(ACCENT_PRESETS
        .iter()
        .map(|(name, hex)| AccentColorPreset {
            name: name.to_string(),
            hex: hex.to_string(),
        })
        .collect())
}

#[tauri::command]
async fn get_app_settings(state: State<'_, Arc<Mutex<AppState>>>) -> Result<AppSettings, String> {
    let state = state.lock().unwrap();
    Ok(AppSettings {
        theme: state.theme.clone(),
        accent_color: state.accent_color.clone(),
        view_mode: state.view_mode.clone(),
        hide_suffix: state.hide_suffix,
        auto_open_details: state.auto_open_details,
        show_hero_icons: state.show_hero_icons,
        show_hero_bg: state.show_hero_bg,
        show_mod_type: state.show_mod_type,
        show_experimental: state.show_experimental,
        auto_check_updates: state.auto_check_updates,
        parallel_processing: state.parallel_processing,
        hold_to_delete: state.hold_to_delete,
        show_subfolder_mods: state.show_subfolder_mods,
        bypass_game_running_lock: state.bypass_game_running_lock,
        enable_drp: state.enable_drp,
        launcher_type: state.launcher_type.clone(),
    })
}

#[tauri::command]
async fn save_app_settings(
    settings: AppSettings,
    state: State<'_, Arc<Mutex<AppState>>>,
    discord: State<'_, DiscordState>,
    window: Window,
) -> Result<(), String> {
    let settings_clone = settings.clone();
    let mut state = state.lock().unwrap();

    state.theme = settings.theme;
    state.accent_color = resolve_accent_name(&settings.accent_color);
    state.view_mode = settings.view_mode;
    state.hide_suffix = settings.hide_suffix;
    state.auto_open_details = settings.auto_open_details;
    state.show_hero_icons = settings.show_hero_icons;
    state.show_hero_bg = settings.show_hero_bg;
    state.show_mod_type = settings.show_mod_type;
    state.show_experimental = settings.show_experimental;
    state.auto_check_updates = settings.auto_check_updates;
    state.parallel_processing = settings.parallel_processing;
    state.hold_to_delete = settings.hold_to_delete;
    state.show_subfolder_mods = settings.show_subfolder_mods;
    state.bypass_game_running_lock = settings.bypass_game_running_lock;
    state.enable_drp = settings.enable_drp;
    state.launcher_type = settings.launcher_type;

    // Apply DRP immediately
    if state.enable_drp {
        if !discord.manager.is_connected() {
            if let Err(e) = discord.manager.connect() {
                warn!("Failed to connect Discord RPC: {}", e);
            }
        }
        let theme_name = state.accent_color.as_str();
        discord.manager.set_theme(theme_name);
    } else {
        if discord.manager.is_connected() {
            if let Err(e) = discord.manager.disconnect() {
                warn!("Failed to disconnect Discord RPC: {}", e);
            }
        }
    }

    save_state(&state).map_err(|e| e.to_string())?;

    // Emit settings changed event to all windows
    let _ = window.emit("settings_changed", &settings_clone);

    Ok(())
}

/// Set parallel processing mode for batch operations
#[tauri::command]
async fn set_parallel_processing(
    enabled: bool,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    info!("set_parallel_processing called: enabled={}", enabled);
    let mut state = state.lock().unwrap();
    state.parallel_processing = enabled;
    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

/// Get current parallel processing setting
#[tauri::command]
async fn get_parallel_processing(state: State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    let state = state.lock().unwrap();
    Ok(state.parallel_processing)
}

#[tauri::command]
async fn get_game_path(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let state = state.lock().unwrap();
    Ok(state.game_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn set_game_path(path: String, state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    let mods_path = PathBuf::from(&path);

    // Auto-deploy bundled LOD Disabler mod if path exists
    if mods_path.exists() {
        match deploy_bundled_lod_mod(&mods_path) {
            Ok(true) => info!("Auto-deployed bundled LOD Disabler mod"),
            Ok(false) => info!("Bundled LOD Disabler mod already present or not bundled"),
            Err(e) => warn!("Failed to auto-deploy LOD Disabler mod: {}", e),
        }
    }

    let mut state = state.lock().unwrap();
    state.game_path = mods_path;
    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn auto_detect_game_path(
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<String, String> {
    match find_marvel_rivals() {
        Some(game_root) => {
            // game_path should be the ~mods directory (matching egui behavior)
            let mods_path = game_root.join("~mods");

            // Create ~mods directory if it doesn't exist
            if !mods_path.exists() {
                if let Err(e) = std::fs::create_dir_all(&mods_path) {
                    let error_msg = format!("Failed to create ~mods directory: {}", e);
                    toast_events::emit_game_path_failed(&window, &error_msg);
                    return Err(error_msg);
                }
            }

            // Auto-deploy bundled LOD Disabler mod
            match deploy_bundled_lod_mod(&mods_path) {
                Ok(true) => info!("Auto-deployed bundled LOD Disabler mod"),
                Ok(false) => info!("Bundled LOD Disabler mod already present or not bundled"),
                Err(e) => warn!("Failed to auto-deploy LOD Disabler mod: {}", e),
            }

            let mut state = state.lock().unwrap();
            state.game_path = mods_path.clone();
            save_state(&state).map_err(|e| e.to_string())?;
            Ok(mods_path.to_string_lossy().to_string())
        }
        None => {
            let error_msg = "Could not auto-detect Marvel Rivals installation".to_string();
            toast_events::emit_game_path_failed(&window, &error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
async fn start_file_watcher(
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
    watcher_state: State<'_, WatcherState>,
) -> Result<(), String> {
    let state_guard = state.lock().unwrap();
    let game_path = state_guard.game_path.clone();
    drop(state_guard);

    if !game_path.exists() {
        return Ok(()); // Can't watch non-existent path
    }

    let mut watcher_guard = watcher_state.watcher.lock().unwrap();

    // Create a new watcher with debouncing
    let window_clone = window.clone();
    let last_event_time = Arc::new(Mutex::new(std::time::Instant::now()));

    let paused = watcher_state.paused.clone();
    let watcher_result = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                // Skip events while paused (e.g. during P2P transfer)
                if paused.load(Ordering::Relaxed) {
                    return;
                }
                // We only care about Create, Remove, Rename, and Modify events (files and directories)
                match event.kind {
                    EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                        // Debounce: only emit if 500ms have passed since last event
                        let mut last_time = last_event_time.lock().unwrap();
                        let now = std::time::Instant::now();
                        let elapsed = now.duration_since(*last_time);

                        if elapsed.as_millis() >= 500 {
                            *last_time = now;

                            // Collect affected paths
                            let changed_paths: Vec<String> = event
                                .paths
                                .iter()
                                .map(|p| p.to_string_lossy().to_string())
                                .collect();

                            // Determine which folders were affected
                            let changed_folders: Vec<String> = event
                                .paths
                                .iter()
                                .filter_map(|p| p.parent())
                                .map(|p| p.to_string_lossy().to_string())
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();

                            window_clone
                                .emit(
                                    "mods_dir_changed",
                                    serde_json::json!({
                                        "paths": changed_paths,
                                        "folders": changed_folders,
                                    }),
                                )
                                .unwrap_or_else(|e| {
                                    error!("Failed to emit mods_dir_changed: {}", e);
                                });
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => error!("Watch error: {:?}", e),
        }
    });

    match watcher_result {
        Ok(mut watcher) => {
            if let Err(e) = watcher.watch(&game_path, RecursiveMode::Recursive) {
                error!("Failed to watch game path: {}", e);
                return Err(e.to_string());
            }
            info!("Started watching game path: {:?}", game_path);
            *watcher_guard = Some(watcher);
            Ok(())
        }
        Err(e) => {
            error!("Failed to create watcher: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
async fn get_pak_files(state: State<'_, Arc<Mutex<AppState>>>) -> Result<Vec<ModEntry>, String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    info!("Loading mods from: {}", game_path.display());

    if !game_path.exists() {
        info!("Game path does not exist: {}", game_path.display());
        return Err(format!("Game path does not exist: {}", game_path.display()));
    }

    // game_path IS the ~mods directory (matching egui behavior)
    let mut mods = Vec::new();

    // Scan root ~mods directory and all subdirectories recursively (no depth limit)
    for entry in WalkDir::new(&game_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Skip directories themselves
        if path.is_dir() {
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str());

        // Check for .pak, .bak_repak, and .pak_disabled files
        if ext == Some("pak") || ext == Some("bak_repak") || ext == Some("pak_disabled") {
            let is_enabled = ext == Some("pak");
            let utoc_path = if ext == Some("pak") || ext == Some("pak_disabled") {
                let candidate = path.with_extension("utoc");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            } else {
                let enabled_pak_path = PathBuf::from(
                    path.to_string_lossy()
                        .trim_end_matches(".bak_repak")
                        .to_string(),
                );
                let candidate = enabled_pak_path.with_extension("utoc");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            };

            // Determine which folder this mod is in
            let root_folder_name = game_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("~mods")
                .to_string();

            // Determine folder_id based on relative path from game_path
            let folder_id = if let Some(parent) = path.parent() {
                if parent == game_path {
                    // Mod is directly in root - use root folder name (e.g., "~mods")
                    Some(root_folder_name)
                } else {
                    // Mod is in a subfolder - use relative path from game_path as ID
                    parent
                        .strip_prefix(game_path)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .ok()
                }
            } else {
                Some(root_folder_name)
            };

            info!(
                "Found PAK file: {} (enabled: {}, folder: {:?})",
                path.display(),
                is_enabled,
                folder_id
            );

            let ucas_path = path.with_extension("ucas");
            let file_size = if ucas_path.exists() {
                std::fs::metadata(&ucas_path).map(|m| m.len()).unwrap_or(0)
            } else {
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
            };

            let modified_date = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                })
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Calculate priority
            // Priority 0 = "!" prefix (highest priority)
            // Priority 1-N = 7-N+6 nines displayed as 1-based (7 nines → Priority 1, 8 nines → Priority 2, etc.)
            let mut priority = 0;
            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Check for "!" prefix (highest priority)
            if file_stem.starts_with("!") {
                priority = 0; // Highest priority
            } else if file_stem.ends_with("_P") {
                let base_no_p = file_stem.strip_suffix("_P").unwrap();
                // Check for _999... suffix
                let re_nums = Regex::new(r"_(\d+)$").unwrap();
                if let Some(caps) = re_nums.captures(base_no_p) {
                    let nums = &caps[1];
                    // Verify they are all 9s
                    if nums.chars().all(|c| c == '9') {
                        let actual_nines = nums.len();
                        // Convert actual nines count to UI priority (1-based)
                        // 7 nines → Priority 1, 8 nines → Priority 2, etc.
                        if actual_nines >= 7 {
                            priority = actual_nines - 6;
                        }
                    }
                }
            }

            let key = get_mod_key(&path);
            let custom_tags = state.mod_tags.get(&key).cloned().unwrap_or_default();

            mods.push(ModEntry {
                path: path.to_path_buf(),
                utoc_path,
                enabled: is_enabled,
                custom_name: None,
                folder_id,
                custom_tags,
                file_size,
                modified_date,
                priority,
                character_name: None,
                skin_name: None,
            });
        }
    }

    info!("Found {} mod(s)", mods.len());
    Ok(mods)
}

#[tauri::command]
async fn get_pak_files_in_folder(
    folder_path: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ModEntry>, String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;
    let folder = PathBuf::from(&folder_path);

    info!("Loading mods from folder: {}", folder.display());

    if !folder.exists() {
        return Err(format!("Folder does not exist: {}", folder.display()));
    }

    let mut mods = Vec::new();

    // Scan only the specified folder (non-recursive for performance)
    for entry in WalkDir::new(&folder)
        .max_depth(1) // Only scan this folder, not subdirectories
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip directories themselves
        if path.is_dir() {
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str());

        // Check for .pak, .bak_repak, and .pak_disabled files
        if ext == Some("pak") || ext == Some("bak_repak") || ext == Some("pak_disabled") {
            let is_enabled = ext == Some("pak");
            let utoc_path = if ext == Some("pak") || ext == Some("pak_disabled") {
                let candidate = path.with_extension("utoc");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            } else {
                let enabled_pak_path = PathBuf::from(
                    path.to_string_lossy()
                        .trim_end_matches(".bak_repak")
                        .to_string(),
                );
                let candidate = enabled_pak_path.with_extension("utoc");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            };

            // Determine which folder this mod is in
            let root_folder_name = game_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("~mods")
                .to_string();

            // Determine folder_id based on relative path from game_path
            let folder_id = if let Some(parent) = path.parent() {
                if parent == game_path {
                    Some(root_folder_name)
                } else {
                    parent
                        .strip_prefix(game_path)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .ok()
                }
            } else {
                Some(root_folder_name)
            };

            let ucas_path = path.with_extension("ucas");
            let file_size = if ucas_path.exists() {
                std::fs::metadata(&ucas_path).map(|m| m.len()).unwrap_or(0)
            } else {
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
            };

            let modified_date = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                })
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Calculate priority
            let mut priority = 0;
            let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            if file_stem.starts_with("!") {
                priority = 0;
            } else if file_stem.ends_with("_P") {
                let base_no_p = file_stem.strip_suffix("_P").unwrap();
                let re_nums = Regex::new(r"_(\d+)$").unwrap();
                if let Some(caps) = re_nums.captures(base_no_p) {
                    let nums = &caps[1];
                    if nums.chars().all(|c| c == '9') {
                        let actual_nines = nums.len();
                        if actual_nines >= 7 {
                            priority = actual_nines - 6;
                        }
                    }
                }
            }

            let key = get_mod_key(&path);
            let custom_tags = state.mod_tags.get(&key).cloned().unwrap_or_default();

            mods.push(ModEntry {
                path: path.to_path_buf(),
                utoc_path,
                enabled: is_enabled,
                custom_name: None,
                folder_id,
                custom_tags,
                file_size,
                modified_date,
                priority,
                character_name: None,
                skin_name: None,
            });
        }
    }

    info!("Found {} mod(s) in folder", mods.len());
    Ok(mods)
}

#[tauri::command]
async fn set_mod_priority(
    mod_path: String,
    priority: usize,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let path = PathBuf::from(&mod_path);
    if !path.exists() {
        return Err("Mod file does not exist".to_string());
    }

    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid filename")?;

    // Strip leading "!" if present (highest priority marker)
    let stem_no_exclaim = stem.strip_prefix("!").unwrap_or(stem);

    // 1. Strip _P if present
    let base_no_p = if stem_no_exclaim.ends_with("_P") {
        stem_no_exclaim.strip_suffix("_P").unwrap()
    } else {
        stem_no_exclaim
    };

    // 2. Strip _999... if present
    let re = Regex::new(r"^(.*)_(\d+)$").unwrap();
    let clean_base = if let Some(caps) = re.captures(base_no_p) {
        let prefix = &caps[1];
        let numbers = &caps[2];
        if numbers.chars().all(|c| c == '9') {
            prefix.to_string()
        } else {
            base_no_p.to_string()
        }
    } else {
        base_no_p.to_string()
    };

    // 3. Construct new name with new priority
    // Priority 0 = "!" prefix (highest priority) with minimum 7 nines
    // Priority 1-N = 7-N+6 nines (1→7 nines, 2→8 nines, etc.)
    let new_stem = if priority == 0 {
        // Highest priority: use "!" prefix with minimum 7 nines
        let min_nines = "9".repeat(7);
        format!("!{}_{}_P", clean_base, min_nines)
    } else {
        // Convert UI priority (1-based) to actual nines count (7-based)
        // Remove "!" prefix if present (since priority > 0)
        let actual_nines = priority + 6; // Priority 1 → 7 nines, Priority 2 → 8 nines, etc.
        let new_nines = "9".repeat(actual_nines);
        format!("{}_{}_P", clean_base, new_nines)
    };

    let new_filename = format!("{}.{}", new_stem, extension);
    let new_path = path.with_file_name(&new_filename);

    if new_path == path {
        return Ok(()); // No change
    }

    if new_path.exists() {
        return Err("A mod with this priority already exists".to_string());
    }

    // Rename main file
    std::fs::rename(&path, &new_path).map_err(|e| format!("Failed to rename mod: {}", e))?;

    // Rename associated files (.utoc, .ucas)
    let exts = ["utoc", "ucas"];
    for ext in exts {
        let old_f = path.with_extension(ext);
        if old_f.exists() {
            let new_f = new_path.with_extension(ext);
            let _ = std::fs::rename(old_f, new_f);
        }
    }

    // Invalidate cache for old path
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.mod_details_cache.remove(&path);
    }

    Ok(())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct InstallableModInfo {
    mod_name: String,
    mod_type: String,
    is_dir: bool,
    path: String,
    auto_to_repak: bool,
    /// Whether the mod contains any .uasset/.uexp/.ubulk/.umap files
    /// Used by frontend to lock/unlock certain toggles (e.g., fix texture only applies to uasset mods)
    contains_uassets: bool,
    /// Whether the mod contains any non-Unreal assets (audio, config, video)
    /// Used by frontend to identify Hybrid IOStore drop scenarios
    contains_raw_assets: bool,
}

#[tauri::command]
async fn parse_dropped_files(
    paths: Vec<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<Vec<InstallableModInfo>, String> {
    use crate::utils::get_current_pak_characteristics;
    use std::fs::File;

    // Emit start detection log
    let _ = window.emit("install_log", "[Detection] Starting UAssetAPI detection...");

    let mut mods = Vec::new();

    // Filter out .utoc and .ucas files - they will be handled with their .pak file
    let filtered_paths: Vec<String> = paths
        .into_iter()
        .filter(|p| {
            let path = PathBuf::from(p);
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                ext != "utoc" && ext != "ucas"
            } else {
                true
            }
        })
        .collect();

    for path_str in filtered_paths {
        let path = PathBuf::from(&path_str);

        if !path.exists() {
            continue;
        }

        let mod_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Determine mod type, contains_uassets and contains_raw_assets flag
        let (mod_type, contains_uassets, contains_raw_assets) = if path.is_dir() {
            // First check if directory contains multiple PAK files - if so, process each PAK separately
            use walkdir::WalkDir;
            let mut pak_files = Vec::new();

            for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                if let Some(ext) = entry_path.extension().and_then(|s| s.to_str()) {
                    if ext == "pak" {
                        pak_files.push(entry_path.to_path_buf());
                    }
                }
            }

            if pak_files.len() > 1 {
                // Multiple PAK files - process each separately
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[Detection] Found {} PAK files in directory, processing each separately",
                        pak_files.len()
                    ),
                );

                for pak_file in pak_files {
                    let pak_mods = Box::pin(parse_dropped_files(
                        vec![pak_file.to_string_lossy().to_string()],
                        state.clone(),
                        window.clone(),
                    ))
                    .await?;
                    for pak_mod in pak_mods {
                        mods.push(pak_mod);
                    }
                }

                continue;
            } else if pak_files.len() == 1 {
                // Single PAK file in directory - process it directly (handles IoStore if present)
                let pak_file = &pak_files[0];
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[Detection] Found single PAK in directory: {}",
                        pak_file.display()
                    ),
                );

                let pak_mods = Box::pin(parse_dropped_files(
                    vec![pak_file.to_string_lossy().to_string()],
                    state.clone(),
                    window.clone(),
                ))
                .await?;
                for pak_mod in pak_mods {
                    mods.push(pak_mod);
                }

                continue;
            }

            // No PAK files - analyze directory contents for loose assets
            let _ = window.emit(
                "install_log",
                "[Detection] No PAK files found, analyzing directory for loose assets...",
            );

            use crate::utils::collect_files;
            let mut all_files = Vec::new();
            if collect_files(&mut all_files, &path).is_ok() {
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[Detection] Collected {} files from directory",
                        all_files.len()
                    ),
                );

                // Convert absolute paths to relative paths for proper classification
                // Strip the base directory path to get relative paths
                let content_files_relative: Vec<String> = all_files
                    .iter()
                    .filter_map(|p| {
                        p.strip_prefix(&path)
                            .ok()
                            .map(|rel| rel.to_string_lossy().to_string().replace('\\', "/"))
                    })
                    .collect();

                if !content_files_relative.is_empty() {
                    let _ = window.emit(
                        "install_log",
                        format!(
                            "[Detection] Processing {} files for classification",
                            content_files_relative.len()
                        ),
                    );

                    // Use detailed characteristics for proper classification (needs relative paths)
                    use crate::utils::get_pak_characteristics_detailed;
                    let characteristics =
                        get_pak_characteristics_detailed(content_files_relative.clone());
                    let mod_type = characteristics.mod_type.clone();

                    let _ = window.emit(
                        "install_log",
                        format!("[Detection] Detected mod type: {}", mod_type),
                    );

                    // Get uasset files for detection (needs absolute paths for UAssetAPI to read files)
                    // Prioritize skeletal mesh files (SK_*), static mesh (SM_*), and textures (T_*) over materials
                    // Limit to 100 total to prevent hangs on large directories
                    let mut uasset_files_absolute: Vec<String> = Vec::new();
                    let mut priority_files: Vec<String> = Vec::new();
                    let mut other_files: Vec<String> = Vec::new();

                    for file in all_files.iter() {
                        if file.extension().and_then(|s| s.to_str()) == Some("uasset") {
                            let filename = file.file_name().and_then(|s| s.to_str()).unwrap_or("");
                            let filename_lower = filename.to_lowercase();

                            // Prioritize SK_, SM_, T_ files (skeletal mesh, static mesh, textures)
                            if filename_lower.starts_with("sk_")
                                || filename_lower.starts_with("sm_")
                                || filename_lower.starts_with("t_")
                            {
                                priority_files.push(file.to_string_lossy().to_string());
                            } else {
                                other_files.push(file.to_string_lossy().to_string());
                            }
                        }
                    }

                    // Add priority files first, then fill up to 100 with other files
                    uasset_files_absolute.extend(priority_files);
                    let remaining = 100usize.saturating_sub(uasset_files_absolute.len());
                    uasset_files_absolute.extend(other_files.into_iter().take(remaining));

                    // Only scan for textures - SkeletalMesh and StaticMesh are auto-fixed by ZenConverter
                    // This significantly speeds up detection by skipping unnecessary UAssetAPI calls
                    let _ = window.emit("install_log", "[Detection] Checking for textures with .ubulk (mesh fixes are automatic)...");

                    // For texture detection, we need ALL files (including .ubulk) to check for bulk data
                    let all_files_absolute: Vec<String> = all_files
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    let has_texture = detect_texture_files_async(&all_files_absolute).await;
                    let _ = window.emit(
                        "install_log",
                        format!("[Detection] Texture result: {}", has_texture),
                    );

                    let summary = format!(
                        "[Detection] Directory results: texture={} (mesh fixes automatic)",
                        has_texture
                    );
                    info!("{}", summary);
                    let _ = window.emit("install_log", &summary);

                    // Check if directory contains uasset files or raw assets
                    use crate::install_mod::{contains_raw_files, contains_uasset_files};
                    let has_uassets = contains_uasset_files(&all_files_absolute);
                    let has_raw_assets = contains_raw_files(&all_files_absolute);
                    let _ = window.emit(
                        "install_log",
                        format!(
                            "[Detection] Contains UAssets: {}, Raw: {}",
                            has_uassets, has_raw_assets
                        ),
                    );

                    (mod_type, has_uassets, has_raw_assets)
                } else {
                    ("Directory".to_string(), true, false) // Default to true for safety
                }
            } else {
                ("Directory".to_string(), true, false) // Default to true for safety
            }
        } else {
            // Get file extension
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

            // Check if it's an archive file (zip, rar, 7z)
            if ext == "zip" || ext == "rar" || ext == "7z" {
                use crate::install_mod::install_mod_logic::archives::{
                    extract_7z, extract_rar, extract_zip,
                };
                use walkdir::WalkDir;

                let _ = window.emit(
                    "install_log",
                    format!("[Detection] Archive detected: {} ({})", mod_name, ext),
                );

                // Extract archive to temp directory for analysis
                let temp_dir = tempfile::tempdir().ok();
                if let Some(ref temp) = temp_dir {
                    let temp_path = temp.path().to_str().unwrap();

                    // Extract based on type
                    let extract_result = if ext == "zip" {
                        extract_zip(path.to_str().unwrap(), temp_path)
                    } else if ext == "rar" {
                        extract_rar(path.to_str().unwrap(), temp_path)
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    } else {
                        extract_7z(path.to_str().unwrap(), temp_path)
                    };

                    if extract_result.is_ok() {
                        let _ = window
                            .emit("install_log", "[Detection] Archive extracted successfully");

                        // Look for PAK files in extracted content
                        let mut pak_files_in_archive = Vec::new();
                        for entry in WalkDir::new(temp_path).into_iter().filter_map(|e| e.ok()) {
                            let entry_path = entry.path();
                            if let Some(entry_ext) = entry_path.extension().and_then(|s| s.to_str())
                            {
                                if entry_ext == "pak" {
                                    pak_files_in_archive.push(entry_path.to_path_buf());
                                }
                            }
                        }

                        if pak_files_in_archive.len() > 1 {
                            // Multiple PAK files found in archive - analyze each separately
                            // All entries use the ORIGINAL ARCHIVE path so install side can re-extract
                            let _ = window.emit(
                                "install_log",
                                format!(
                                    "[Detection] Found {} PAK files in archive, analyzing each...",
                                    pak_files_in_archive.len()
                                ),
                            );

                            for pak_file_path in &pak_files_in_archive {
                                let pak_name = pak_file_path
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or("Unknown")
                                    .to_string();
                                let _ = window.emit(
                                    "install_log",
                                    format!("[Detection] Analyzing PAK: {}", pak_name),
                                );

                                // Check for IoStore companions
                                let utoc = pak_file_path.with_extension("utoc");
                                let ucas = pak_file_path.with_extension("ucas");
                                let is_ios = utoc.exists() && ucas.exists();

                                let files_opt: Option<Vec<String>> = if is_ios {
                                    use crate::utoc_utils::read_utoc;
                                    let utoc_files: Vec<String> = read_utoc(&utoc)
                                        .iter()
                                        .map(|e| e.file_path.clone())
                                        .collect();
                                    if utoc_files.is_empty() {
                                        None
                                    } else {
                                        Some(utoc_files)
                                    }
                                } else {
                                    uasset_toolkit::list_pak_files(
                                        pak_file_path.to_str().unwrap_or_default(),
                                        Some(crate::install_mod::AES_KEY_HEX),
                                    )
                                    .ok()
                                    .filter(|f| !f.is_empty())
                                };

                                let mut pak_mod_type = "Archive".to_string();
                                let mut pak_has_uassets = false;
                                let mut pak_has_raw_assets = false;

                                if let Some(files) = files_opt {
                                    use crate::install_mod::{
                                        contains_raw_files, contains_uasset_files,
                                    };
                                    use crate::utils::get_pak_characteristics_detailed;
                                    let chars = get_pak_characteristics_detailed(files.clone());
                                    pak_mod_type = chars.mod_type.clone();
                                    pak_has_uassets = contains_uasset_files(&files);
                                    pak_has_raw_assets = contains_raw_files(&files);
                                }

                                let _ = window.emit(
                                    "install_log",
                                    format!(
                                        "[Detection] PAK {}: type={}, uassets={}",
                                        pak_name, pak_mod_type, pak_has_uassets
                                    ),
                                );

                                // Use original archive path - map_to_mods_internal will re-extract all PAKs
                                // Standalone PAKs (not IoStore) need repak/IoStore conversion
                                mods.push(InstallableModInfo {
                                    mod_name: pak_name,
                                    mod_type: pak_mod_type,
                                    is_dir: false,
                                    path: path_str.clone(),
                                    auto_to_repak: !is_ios,
                                    contains_uassets: pak_has_uassets,
                                    contains_raw_assets: pak_has_raw_assets,
                                });
                            }
                            continue;
                        }

                        // Single PAK file or no PAK files - continue with existing logic
                        let found_pak = !pak_files_in_archive.is_empty();
                        if found_pak {
                            let entry_path = &pak_files_in_archive[0];

                            // Check if this is an IoStore package (has .utoc and .ucas companions)
                            let utoc_path = entry_path.with_extension("utoc");
                            let ucas_path = entry_path.with_extension("ucas");
                            let is_iostore = utoc_path.exists() && ucas_path.exists();

                            if is_iostore {
                                let _ = window.emit(
                                    "install_log",
                                    format!(
                                        "[Detection] IoStore package detected in archive: {}",
                                        mod_name
                                    ),
                                );
                            }

                            // Get file list - for IoStore, read from utoc directly (works with obfuscated mods);
                            // otherwise open PAK with AES key
                            let files: Option<Vec<String>> = if is_iostore {
                                use crate::utoc_utils::read_utoc;
                                let _ = window.emit(
                                    "install_log",
                                    "[Detection] Reading IoStore .utoc file for accurate file list",
                                );
                                let utoc_files: Vec<String> = read_utoc(&utoc_path)
                                    .iter()
                                    .map(|entry| entry.file_path.clone())
                                    .collect();
                                if utoc_files.is_empty() {
                                    None
                                } else {
                                    Some(utoc_files)
                                }
                            } else {
                                uasset_toolkit::list_pak_files(
                                    entry_path.to_str().unwrap_or_default(),
                                    Some(crate::install_mod::AES_KEY_HEX),
                                )
                                .ok()
                                .filter(|f| !f.is_empty())
                            };

                            if let Some(files) = files {
                                // Use detailed characteristics (same as get_mod_details)
                                use crate::utils::get_pak_characteristics_detailed;
                                let characteristics =
                                    get_pak_characteristics_detailed(files.clone());
                                let mod_type = characteristics.mod_type.clone();

                                let _ = window.emit(
                                    "install_log",
                                    format!("[Detection] Detected mod type: {}", mod_type),
                                );

                                // Get files to extract (both .uasset and .uexp needed by UAssetAPI)
                                // Prioritize SK_, SM_, T_ files for detection
                                let files_to_extract: Vec<&String> = files
                                    .iter()
                                    .filter(|f| {
                                        let lower = f.to_lowercase();
                                        (lower.ends_with(".uasset") || lower.ends_with(".uexp"))
                                            && if let Some(filename) = std::path::Path::new(f)
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                            {
                                                let fname_lower = filename.to_lowercase();
                                                fname_lower.starts_with("sk_")
                                                    || fname_lower.starts_with("sm_")
                                                    || fname_lower.starts_with("t_")
                                            } else {
                                                false
                                            }
                                    })
                                    .take(40) // Limit to 40 files (20 uasset + 20 uexp pairs)
                                    .collect();

                                let _ = window.emit("install_log", format!("[Detection] Extracting {} files from archive PAK for analysis...", files_to_extract.len()));

                                // Extract to temp directory for UAssetAPI analysis
                                let mut extracted_paths: Vec<String> = Vec::new();
                                let uasset_temp_dir = tempfile::tempdir().ok();

                                if let Some(ref uasset_temp) = uasset_temp_dir {
                                    // Extract PAK to temp dir using UAssetTool
                                    let _ = uasset_toolkit::extract_pak_all(
                                        entry_path.to_str().unwrap_or_default(),
                                        uasset_temp.path().to_str().unwrap_or_default(),
                                        Some(crate::install_mod::AES_KEY_HEX),
                                    );
                                    // Collect only extracted .uasset files matching our filter
                                    use walkdir::WalkDir;
                                    for entry in WalkDir::new(uasset_temp.path())
                                        .into_iter()
                                        .filter_map(|e| e.ok())
                                    {
                                        let p = entry.path();
                                        if p.extension().and_then(|e| e.to_str()) == Some("uasset")
                                        {
                                            if let Some(fname) =
                                                p.file_name().and_then(|n| n.to_str())
                                            {
                                                let fl = fname.to_lowercase();
                                                if fl.starts_with("sk_")
                                                    || fl.starts_with("sm_")
                                                    || fl.starts_with("t_")
                                                {
                                                    extracted_paths
                                                        .push(p.to_string_lossy().to_string());
                                                }
                                            }
                                        }
                                    }

                                    let _ = window.emit(
                                        "install_log",
                                        format!(
                                            "[Detection] Extracted {} uasset files for UAssetAPI",
                                            extracted_paths.len()
                                        ),
                                    );
                                }

                                // Only scan for textures - mesh fixes are automatic in ZenConverter
                                let _ = window.emit("install_log", "[Detection] Checking for textures with .ubulk (mesh fixes automatic)...");

                                // Texture detection - use extracted files but also check for .ubulk in original file list
                                let has_ubulk =
                                    files.iter().any(|f| f.to_lowercase().ends_with(".ubulk"));
                                let has_texture = if has_ubulk && !extracted_paths.is_empty() {
                                    // Add .ubulk indicator to detection files so detect_texture_files_async knows there's bulk data
                                    let mut texture_detection_files = extracted_paths.clone();
                                    texture_detection_files.push("dummy.ubulk".to_string()); // Signal that .ubulk exists
                                    detect_texture_files_async(&texture_detection_files).await
                                } else {
                                    false
                                };
                                let _ = window.emit(
                                    "install_log",
                                    format!("[Detection] Texture result: {}", has_texture),
                                );

                                let summary = format!("[Detection] Archive PAK results: texture={} (mesh fixes automatic)", has_texture);
                                info!("{}", summary);
                                let _ = window.emit("install_log", &summary);

                                // Clean up temp dir
                                drop(temp_dir);

                                // Check if files contain uassets or raw assets
                                use crate::install_mod::{
                                    contains_raw_files, contains_uasset_files,
                                };
                                let has_uassets = contains_uasset_files(&files);
                                let has_raw_assets = contains_raw_files(&files);

                                mods.push(InstallableModInfo {
                                    mod_name,
                                    mod_type,
                                    is_dir: false,
                                    path: path_str,
                                    auto_to_repak: !is_iostore,
                                    contains_uassets: has_uassets,
                                    contains_raw_assets: has_raw_assets,
                                });
                                continue;
                            }
                        }

                        // If no .pak files found, look for content folders with loose assets
                        if !found_pak {
                            let _ = window.emit("install_log", "[Detection] No PAK files found in archive, looking for content folders...");

                            use crate::utils::collect_files;

                            // Collect all files from the extracted archive
                            let mut all_files = Vec::new();
                            let temp_path_buf = PathBuf::from(temp_path);
                            if collect_files(&mut all_files, &temp_path_buf).is_ok() {
                                // Check if there are content files (.uasset, .uexp, .ubulk, etc.)
                                let content_files: Vec<String> = all_files
                                    .iter()
                                    .filter(|f| {
                                        if let Some(ext) = f.extension().and_then(|s| s.to_str()) {
                                            matches!(
                                                ext,
                                                "uasset" | "uexp" | "ubulk" | "bnk" | "wem"
                                            )
                                        } else {
                                            false
                                        }
                                    })
                                    .map(|p| p.to_string_lossy().to_string())
                                    .collect();

                                if !content_files.is_empty() {
                                    let _ = window.emit(
                                        "install_log",
                                        format!(
                                            "[Detection] Found {} content files in archive folder",
                                            content_files.len()
                                        ),
                                    );

                                    // Get mod type from content
                                    let mod_type =
                                        get_current_pak_characteristics(content_files.clone());

                                    // Only scan for textures - mesh fixes are automatic in ZenConverter
                                    let _ = window.emit("install_log", "[Detection] Checking for textures with .ubulk (mesh fixes automatic)...");
                                    let has_texture =
                                        detect_texture_files_async(&content_files).await;
                                    let _ = window.emit(
                                        "install_log",
                                        format!("[Detection] Texture result: {}", has_texture),
                                    );

                                    let summary = format!("[Detection] Archive folder results: texture={} (mesh fixes automatic)", has_texture);
                                    info!("{}", summary);
                                    let _ = window.emit("install_log", &summary);

                                    // Clean up temp dir
                                    drop(temp_dir);

                                    // Check if content files contain uassets or raw assets
                                    use crate::install_mod::{
                                        contains_raw_files, contains_uasset_files,
                                    };
                                    let has_uassets = contains_uasset_files(&content_files);
                                    let has_raw_assets = contains_raw_files(&content_files);

                                    // Add as a directory mod (will be converted to IoStore)
                                    mods.push(InstallableModInfo {
                                        mod_name,
                                        mod_type,
                                        is_dir: true,
                                        path: path_str,
                                        auto_to_repak: false,
                                        contains_uassets: has_uassets,
                                        contains_raw_assets: has_raw_assets,
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                }

                // Fallback if extraction/analysis failed
                ("Archive".to_string(), true, false) // Default to true for safety
            } else if ext == "pak" {
                // Check if this is an IoStore package (has .utoc and .ucas companions)
                let utoc_path = path.with_extension("utoc");
                let ucas_path = path.with_extension("ucas");
                let is_iostore = utoc_path.exists() && ucas_path.exists();

                if is_iostore {
                    let _ = window.emit(
                        "install_log",
                        format!("[Detection] IoStore package detected: {}", mod_name),
                    );
                }

                // Read file list for mod type detection
                // For IoStore, read from utoc directly (works with obfuscated mods);
                // otherwise open PAK with AES key
                let mod_type = {
                    let files_and_key: Option<Vec<String>> = if is_iostore {
                        use crate::utoc_utils::read_utoc;
                        let _ = window.emit(
                            "install_log",
                            "[Detection] Reading IoStore .utoc file for accurate file list",
                        );
                        let utoc_files: Vec<String> = read_utoc(&utoc_path)
                            .iter()
                            .map(|entry| entry.file_path.clone())
                            .collect();
                        if utoc_files.is_empty() {
                            None
                        } else {
                            Some(utoc_files)
                        }
                    } else {
                        uasset_toolkit::list_pak_files(
                            path.to_str().unwrap_or_default(),
                            Some(crate::install_mod::AES_KEY_HEX),
                        )
                        .ok()
                        .filter(|f| !f.is_empty())
                    };

                    if let Some(files) = files_and_key {
                        // Use detailed characteristics (same as get_mod_details)
                        use crate::utils::get_pak_characteristics_detailed;
                        let characteristics = get_pak_characteristics_detailed(files.clone());
                        let mod_type = characteristics.mod_type.clone();

                        let _ = window.emit(
                            "install_log",
                            format!("[Detection] Detected mod type: {}", mod_type),
                        );

                        // Get files to extract (both .uasset and .uexp needed by UAssetAPI)
                        let files_to_extract: Vec<&String> = files.iter()
                                .filter(|f| {
                                    let lower = f.to_lowercase();
                                    (lower.ends_with(".uasset") || lower.ends_with(".uexp")) &&
                                    // Prioritize SK_, SM_, T_ files
                                    if let Some(filename) = std::path::Path::new(f).file_name().and_then(|n| n.to_str()) {
                                        let fname_lower = filename.to_lowercase();
                                        fname_lower.starts_with("sk_") || fname_lower.starts_with("sm_") || fname_lower.starts_with("t_")
                                    } else {
                                        false
                                    }
                                })
                                .take(40)  // Limit to 40 files (20 uasset + 20 uexp pairs)
                                .collect();

                        let _ = window.emit(
                            "install_log",
                            format!(
                                "[Detection] Extracting {} files from PAK for analysis...",
                                files_to_extract.len()
                            ),
                        );

                        // Extract to temp directory
                        let mut extracted_paths: Vec<String> = Vec::new();
                        let uasset_temp_dir = tempfile::tempdir().ok();

                        if let Some(ref uasset_temp) = uasset_temp_dir {
                            let _ = uasset_toolkit::extract_pak_all(
                                path.to_str().unwrap_or_default(),
                                uasset_temp.path().to_str().unwrap_or_default(),
                                Some(crate::install_mod::AES_KEY_HEX),
                            );
                            use walkdir::WalkDir;
                            for entry in WalkDir::new(uasset_temp.path())
                                .into_iter()
                                .filter_map(|e| e.ok())
                            {
                                let p = entry.path();
                                if p.extension().and_then(|e| e.to_str()) == Some("uasset") {
                                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                                        let fl = fname.to_lowercase();
                                        if fl.starts_with("sk_")
                                            || fl.starts_with("sm_")
                                            || fl.starts_with("t_")
                                        {
                                            extracted_paths.push(p.to_string_lossy().to_string());
                                        }
                                    }
                                }
                            }
                            let _ = window.emit(
                                "install_log",
                                format!(
                                    "[Detection] Extracted {} uasset files for UAssetAPI",
                                    extracted_paths.len()
                                ),
                            );
                        }

                        // Only scan for textures - mesh fixes are automatic in ZenConverter
                        let _ = window.emit("install_log", "[Detection] Checking for textures with .ubulk (mesh fixes automatic)...");

                        // Texture detection - use extracted files but also check for .ubulk in original file list
                        let has_ubulk = files.iter().any(|f| f.to_lowercase().ends_with(".ubulk"));
                        let has_texture = if has_ubulk && !extracted_paths.is_empty() {
                            // Add .ubulk indicator to detection files so detect_texture_files_async knows there's bulk data
                            let mut texture_detection_files = extracted_paths.clone();
                            texture_detection_files.push("dummy.ubulk".to_string()); // Signal that .ubulk exists
                            detect_texture_files_async(&texture_detection_files).await
                        } else {
                            false
                        };
                        let _ = window.emit(
                            "install_log",
                            format!("[Detection] Texture result: {}", has_texture),
                        );

                        let summary = format!(
                            "[Detection] PAK file results: texture={} (mesh fixes automatic)",
                            has_texture
                        );
                        info!("{}", summary);
                        let _ = window.emit("install_log", &summary);

                        // Check if files contain uassets or raw assets
                        use crate::install_mod::{contains_raw_files, contains_uasset_files};
                        let has_uassets = contains_uasset_files(&files);
                        let has_raw_assets = contains_raw_files(&files);

                        // Push this PAK mod and continue processing other files
                        mods.push(InstallableModInfo {
                            mod_name,
                            mod_type,
                            is_dir: false,
                            path: path_str,
                            auto_to_repak: !is_iostore,
                            contains_uassets: has_uassets,
                            contains_raw_assets: has_raw_assets,
                        });
                        continue; // Continue to next file instead of returning
                    }

                    "PAK".to_string()
                };

                (mod_type, true, false) // Default to true for safety
            } else {
                ("Unknown".to_string(), true, false) // Default to true for safety
            }
        };

        // For .pak files, auto-enable repak UNLESS it's an IoStore package
        let is_pak = path.extension().and_then(|s| s.to_str()) == Some("pak");
        let is_iostore_pkg =
            is_pak && path.with_extension("utoc").exists() && path.with_extension("ucas").exists();
        let auto_to_repak = is_pak && !is_iostore_pkg;

        mods.push(InstallableModInfo {
            mod_name,
            mod_type,
            is_dir: path.is_dir(),
            path: path_str,
            auto_to_repak,
            contains_uassets,
            contains_raw_assets,
        });
    }

    Ok(mods)
}

#[derive(serde::Deserialize)]
struct ModToInstall {
    path: String,
    #[serde(rename = "mod_name", default)]
    mod_name: String,
    #[serde(rename = "customName")]
    custom_name: Option<String>,
    #[serde(rename = "toRepak")]
    to_repak: bool,
    #[serde(rename = "forceLegacy")]
    force_legacy: bool,
    /// Subfolder within the mods directory to install into (empty = root)
    #[serde(rename = "installSubfolder", default)]
    install_subfolder: String,
    /// Whether to include this mod in the installation (for multi-PAK selection)
    #[serde(default = "default_true")]
    enabled: bool,
    /// Enable IoStore obfuscation for this install (per-mod, not global)
    #[serde(default)]
    obfuscate: bool,
    /// Whether to generate a hybrid IOStore bundle (keeping raw assets)
    #[serde(default)]
    hybrid: bool,
}

/// Helper function to copy an IoStore bundle (.utoc/.ucas and .pak or .bak_repak) and recompress if needed
fn copy_iostore_with_compression_check(
    utoc_src: &Path,
    output_dir: &Path,
    window: &Window,
) -> Result<u32, String> {
    let utoc_name = utoc_src.file_name().unwrap();
    let ucas_src = utoc_src.with_extension("ucas");
    let utoc_dest = output_dir.join(utoc_name);
    let ucas_dest = output_dir.join(ucas_src.file_name().unwrap());

    let mut file_count = 0u32;

    // Also check for .pak or .bak_repak file (part of IoStore bundle)
    let pak_src = utoc_src.with_extension("pak");
    let bak_repak_src = utoc_src.with_extension("bak_repak");

    // Copy .pak if it exists
    if pak_src.exists() {
        let pak_dest = output_dir.join(pak_src.file_name().unwrap());
        if let Err(e) = std::fs::copy(&pak_src, &pak_dest) {
            warn!(
                "[QuickOrganize] Failed to copy {}: {}",
                pak_src.file_name().unwrap().to_string_lossy(),
                e
            );
        } else {
            info!(
                "[QuickOrganize] Copied: {}",
                pak_src.file_name().unwrap().to_string_lossy()
            );
            let _ = window.emit(
                "install_log",
                format!(
                    "[QuickOrganize] Copied: {}",
                    pak_src.file_name().unwrap().to_string_lossy()
                ),
            );
            file_count += 1;
        }
    }

    // Copy .bak_repak if it exists (disabled pak file)
    if bak_repak_src.exists() {
        let bak_repak_dest = output_dir.join(bak_repak_src.file_name().unwrap());
        if let Err(e) = std::fs::copy(&bak_repak_src, &bak_repak_dest) {
            warn!(
                "[QuickOrganize] Failed to copy {}: {}",
                bak_repak_src.file_name().unwrap().to_string_lossy(),
                e
            );
        } else {
            info!(
                "[QuickOrganize] Copied: {}",
                bak_repak_src.file_name().unwrap().to_string_lossy()
            );
            let _ = window.emit(
                "install_log",
                format!(
                    "[QuickOrganize] Copied: {}",
                    bak_repak_src.file_name().unwrap().to_string_lossy()
                ),
            );
            file_count += 1;
        }
    }

    // Check if the IoStore is compressed
    let is_compressed = match uasset_toolkit::is_iostore_compressed(&utoc_src.to_string_lossy()) {
        Ok(compressed) => compressed,
        Err(e) => {
            warn!(
                "[QuickOrganize] Failed to check IoStore compression for {}: {}",
                utoc_name.to_string_lossy(),
                e
            );
            // Assume compressed if we can't check, just copy
            true
        }
    };

    if is_compressed {
        // Already compressed, just copy
        info!(
            "[QuickOrganize] IoStore {} is already compressed, copying directly",
            utoc_name.to_string_lossy()
        );
        let _ = window.emit(
            "install_log",
            format!(
                "[QuickOrganize] Copying compressed IoStore: {}",
                utoc_name.to_string_lossy()
            ),
        );

        std::fs::copy(utoc_src, &utoc_dest)
            .map_err(|e| format!("Failed to copy {}: {}", utoc_name.to_string_lossy(), e))?;
        std::fs::copy(&ucas_src, &ucas_dest).map_err(|e| {
            format!(
                "Failed to copy {}: {}",
                ucas_src.file_name().unwrap().to_string_lossy(),
                e
            )
        })?;

        file_count += 2; // Copied utoc + ucas
    } else {
        // Not compressed, need to recompress with Oodle
        info!(
            "[QuickOrganize] IoStore {} is NOT compressed, recompressing with Oodle...",
            utoc_name.to_string_lossy()
        );
        let _ = window.emit(
            "install_log",
            format!(
                "[QuickOrganize] Recompressing uncompressed IoStore: {}",
                utoc_name.to_string_lossy()
            ),
        );

        // First copy to destination
        std::fs::copy(utoc_src, &utoc_dest)
            .map_err(|e| format!("Failed to copy {}: {}", utoc_name.to_string_lossy(), e))?;
        std::fs::copy(&ucas_src, &ucas_dest).map_err(|e| {
            format!(
                "Failed to copy {}: {}",
                ucas_src.file_name().unwrap().to_string_lossy(),
                e
            )
        })?;

        file_count += 2; // Copied utoc + ucas

        // Now recompress in place
        match uasset_toolkit::recompress_iostore(&utoc_dest.to_string_lossy()) {
            Ok(_) => {
                info!(
                    "[QuickOrganize] Successfully recompressed IoStore: {}",
                    utoc_name.to_string_lossy()
                );
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[QuickOrganize] ✓ Recompressed: {}",
                        utoc_name.to_string_lossy()
                    ),
                );
            }
            Err(e) => {
                warn!(
                    "[QuickOrganize] Failed to recompress IoStore {}: {}",
                    utoc_name.to_string_lossy(),
                    e
                );
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[QuickOrganize] Warning: Could not recompress {}: {}",
                        utoc_name.to_string_lossy(),
                        e
                    ),
                );
                // Files are still copied, just not recompressed
            }
        }
    }

    Ok(file_count)
}

fn get_clean_base_name(filename: &str) -> String {
    let mut stem = filename;

    loop {
        let prev_stem = stem;
        for ext in &[".pak", ".pak_disabled", ".bak_repak", ".utoc", ".ucas"] {
            if stem.ends_with(ext) {
                stem = stem.strip_suffix(ext).unwrap();
            }
        }
        if stem == prev_stem {
            break;
        }
    }

    let stem_no_exclaim = stem.strip_prefix("!").unwrap_or(stem);

    let base_no_p = if stem_no_exclaim.ends_with("_P") {
        stem_no_exclaim.strip_suffix("_P").unwrap()
    } else {
        stem_no_exclaim
    };

    let re = Regex::new(r"^(.*)_(\d+)$").unwrap();
    if let Some(caps) = re.captures(base_no_p) {
        let prefix = &caps[1];
        let numbers = &caps[2];
        if numbers.chars().all(|c| c == '9') {
            return prefix.to_string();
        }
    }
    base_no_p.to_string()
}

/// Quick Organize: Simply copy/move files to a target folder without any repak processing
/// This is for organizing existing mod files into subfolders
/// Now also detects uncompressed IoStore bundles and recompresses them with Oodle
/// Preserves subfolder structure from archives and directories
#[tauri::command]
async fn quick_organize(
    paths: Vec<String>,
    target_folder: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<i32, String> {
    use crate::install_mod::install_mod_logic::archives::{extract_7z, extract_rar, extract_zip};
    use walkdir::WalkDir;

    let state_guard = state.lock().unwrap();
    let mod_directory = state_guard.game_path.clone();
    drop(state_guard);

    // Determine the output directory
    let output_dir = if target_folder.is_empty() || target_folder == "~mods" {
        mod_directory.clone()
    } else {
        mod_directory.join(&target_folder)
    };

    // Create output directory if it doesn't exist (for "New Folder" drops and subfolder preservation)
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| format!("Failed to create target folder '{}': {}", target_folder, e))?;
        info!(
            "[QuickOrganize] Created target folder: {}",
            output_dir.display()
        );
    }

    let mut cleaned_bases = std::collections::HashSet::new();

    let mut cleanup_conflicting_mods = |dest_dir: &Path, incoming_filename: &str| {
        let clean_base = get_clean_base_name(incoming_filename);
        if clean_base.is_empty() {
            return;
        }

        // Only run cleanup once per clean base name
        if !cleaned_bases.insert(clean_base.clone()) {
            return;
        }

        if !dest_dir.exists() {
            return;
        }

        info!(
            "[QuickOrganize] Cleaning up existing mod variants for clean base '{}' in '{}'",
            clean_base,
            dest_dir.display()
        );

        if let Ok(entries) = std::fs::read_dir(dest_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if let Some(name_str) = entry_path.file_name().and_then(|n| n.to_str()) {
                        let entry_clean_base = get_clean_base_name(name_str);
                        if entry_clean_base == clean_base {
                            info!(
                                "[QuickOrganize] Removing conflicting pre-existing file: {}",
                                entry_path.display()
                            );
                            if let Err(e) = std::fs::remove_file(&entry_path) {
                                warn!(
                                    "[QuickOrganize] Failed to remove conflicting file {}: {}",
                                    entry_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
    };

    info!(
        "[QuickOrganize] Copying {} file(s) to '{}'",
        paths.len(),
        output_dir.display()
    );
    let _ = window.emit(
        "install_log",
        format!(
            "[QuickOrganize] Copying to folder: {}",
            if target_folder.is_empty() {
                "~mods (root)".to_string()
            } else {
                target_folder.clone()
            }
        ),
    );

    let mut copied_count = 0;

    /// Helper to compute relative path from a base directory to preserve subfolder structure
    fn get_relative_subpath(entry_path: &Path, base_path: &Path) -> Option<PathBuf> {
        entry_path
            .parent()
            .and_then(|parent| parent.strip_prefix(base_path).ok())
            .map(|rel| rel.to_path_buf())
            .filter(|rel| !rel.as_os_str().is_empty())
    }

    /// Helper to ensure destination directory exists and return the full destination path
    fn prepare_dest_with_subfolders(
        entry_path: &Path,
        base_path: &Path,
        output_dir: &Path,
        window: &Window,
    ) -> Result<PathBuf, String> {
        let file_name = entry_path.file_name().unwrap();

        if let Some(rel_subpath) = get_relative_subpath(entry_path, base_path) {
            let dest_subdir = output_dir.join(&rel_subpath);
            if !dest_subdir.exists() {
                std::fs::create_dir_all(&dest_subdir).map_err(|e| {
                    format!(
                        "Failed to create subfolder '{}': {}",
                        rel_subpath.display(),
                        e
                    )
                })?;
                info!(
                    "[QuickOrganize] Created subfolder: {}",
                    rel_subpath.display()
                );
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[QuickOrganize] Created subfolder: {}",
                        rel_subpath.display()
                    ),
                );
            }
            Ok(dest_subdir.join(file_name))
        } else {
            Ok(output_dir.join(file_name))
        }
    }

    /// Helper to get the destination directory for IoStore bundles with subfolder preservation
    fn get_iostore_dest_dir(
        entry_path: &Path,
        base_path: &Path,
        output_dir: &Path,
        window: &Window,
    ) -> Result<PathBuf, String> {
        if let Some(rel_subpath) = get_relative_subpath(entry_path, base_path) {
            let dest_subdir = output_dir.join(&rel_subpath);
            if !dest_subdir.exists() {
                std::fs::create_dir_all(&dest_subdir).map_err(|e| {
                    format!(
                        "Failed to create subfolder '{}': {}",
                        rel_subpath.display(),
                        e
                    )
                })?;
                info!(
                    "[QuickOrganize] Created subfolder: {}",
                    rel_subpath.display()
                );
                let _ = window.emit(
                    "install_log",
                    format!(
                        "[QuickOrganize] Created subfolder: {}",
                        rel_subpath.display()
                    ),
                );
            }
            Ok(dest_subdir)
        } else {
            Ok(output_dir.to_path_buf())
        }
    }

    for path_str in paths {
        let path = PathBuf::from(&path_str);

        if !path.exists() {
            warn!("[QuickOrganize] Path does not exist: {}", path_str);
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        // Handle archives - extract and copy contents preserving subfolder structure
        if ext == "zip" || ext == "rar" || ext == "7z" {
            let _ = window.emit(
                "install_log",
                format!(
                    "[QuickOrganize] Extracting archive: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            );

            let temp_dir =
                tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
            let temp_path = temp_dir.path();
            let temp_path_str = temp_path.to_str().unwrap();

            // Extract archive
            let extract_result = if ext == "zip" {
                extract_zip(path.to_str().unwrap(), temp_path_str)
            } else if ext == "rar" {
                extract_rar(path.to_str().unwrap(), temp_path_str)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            } else {
                extract_7z(path.to_str().unwrap(), temp_path_str)
            };

            if let Err(e) = extract_result {
                error!("[QuickOrganize] Failed to extract archive: {}", e);
                let _ = window.emit(
                    "install_log",
                    format!("[QuickOrganize] ERROR: Failed to extract archive: {}", e),
                );
                continue;
            }

            // Find and copy all pak/utoc/ucas files from extracted content with subfolder preservation
            let mut processed_utocs: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();

            for entry in WalkDir::new(temp_path).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                if let Some(entry_ext) = entry_path.extension().and_then(|s| s.to_str()) {
                    if entry_ext == "pak" {
                        // Prepare destination with subfolder structure
                        let dest = match prepare_dest_with_subfolders(
                            entry_path,
                            temp_path,
                            &output_dir,
                            &window,
                        ) {
                            Ok(d) => d,
                            Err(e) => {
                                error!("[QuickOrganize] {}", e);
                                let _ = window
                                    .emit("install_log", format!("[QuickOrganize] ERROR: {}", e));
                                continue;
                            }
                        };

                        let file_name_str = entry_path
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or("");
                        cleanup_conflicting_mods(
                            dest.parent().unwrap_or(&output_dir),
                            file_name_str,
                        );

                        if let Err(e) = std::fs::copy(entry_path, &dest) {
                            error!(
                                "[QuickOrganize] Failed to copy {}: {}",
                                entry_path.file_name().unwrap().to_string_lossy(),
                                e
                            );
                        } else {
                            let rel_dest = dest.strip_prefix(&output_dir).unwrap_or(&dest);
                            info!("[QuickOrganize] Copied: {}", rel_dest.display());
                            let _ = window.emit(
                                "install_log",
                                format!("[QuickOrganize] Copied: {}", rel_dest.display()),
                            );
                            copied_count += 1;
                        }
                    } else if entry_ext == "utoc" {
                        // Process IoStore with compression check, preserving subfolder structure
                        let ucas_path = entry_path.with_extension("ucas");
                        if ucas_path.exists() && !processed_utocs.contains(entry_path) {
                            processed_utocs.insert(entry_path.to_path_buf());

                            // Determine destination directory with subfolder preservation
                            let dest_dir = match get_iostore_dest_dir(
                                entry_path,
                                temp_path,
                                &output_dir,
                                &window,
                            ) {
                                Ok(d) => d,
                                Err(e) => {
                                    error!("[QuickOrganize] {}", e);
                                    let _ = window.emit(
                                        "install_log",
                                        format!("[QuickOrganize] ERROR: {}", e),
                                    );
                                    continue;
                                }
                            };

                            let file_name_str = entry_path
                                .file_name()
                                .unwrap_or_default()
                                .to_str()
                                .unwrap_or("");
                            cleanup_conflicting_mods(&dest_dir, file_name_str);

                            match copy_iostore_with_compression_check(
                                entry_path, &dest_dir, &window,
                            ) {
                                Ok(count) => copied_count += count as i32,
                                Err(e) => {
                                    error!("[QuickOrganize] Failed to process IoStore: {}", e);
                                    let _ = window.emit(
                                        "install_log",
                                        format!("[QuickOrganize] ERROR: {}", e),
                                    );
                                }
                            }
                        }
                    }
                    // Skip .ucas files - they're handled together with .utoc
                }
            }
        }
        // Handle pak files (and their iostore companions) - no subfolder structure for single files
        else if ext == "pak" {
            let file_name = path.file_name().unwrap();
            let dest = output_dir.join(file_name);

            let file_name_str = file_name.to_str().unwrap_or("");
            cleanup_conflicting_mods(&output_dir, file_name_str);

            // Copy the pak file
            if let Err(e) = std::fs::copy(&path, &dest) {
                error!(
                    "[QuickOrganize] Failed to copy {}: {}",
                    file_name.to_string_lossy(),
                    e
                );
                continue;
            }

            info!("[QuickOrganize] Copied: {}", file_name.to_string_lossy());
            let _ = window.emit(
                "install_log",
                format!("[QuickOrganize] Copied: {}", file_name.to_string_lossy()),
            );
            copied_count += 1;

            // Also handle utoc and ucas if they exist (IoStore package)
            let utoc_path = path.with_extension("utoc");
            let ucas_path = path.with_extension("ucas");

            if utoc_path.exists() && ucas_path.exists() {
                match copy_iostore_with_compression_check(&utoc_path, &output_dir, &window) {
                    Ok(count) => copied_count += count as i32,
                    Err(e) => {
                        error!("[QuickOrganize] Failed to process IoStore: {}", e);
                        let _ = window.emit("install_log", format!("[QuickOrganize] ERROR: {}", e));
                    }
                }
            } else if utoc_path.exists() {
                let utoc_name = utoc_path.file_name().unwrap();
                if let Err(e) = std::fs::copy(&utoc_path, output_dir.join(utoc_name)) {
                    error!(
                        "[QuickOrganize] Failed to copy {}: {}",
                        utoc_name.to_string_lossy(),
                        e
                    );
                } else {
                    copied_count += 1;
                }
            }
        }
        // Handle utoc files directly (IoStore bundle without .pak or with .bak_repak)
        else if ext == "utoc" {
            let ucas_path = path.with_extension("ucas");
            if ucas_path.exists() {
                let file_name_str = path.file_name().unwrap_or_default().to_str().unwrap_or("");
                cleanup_conflicting_mods(&output_dir, file_name_str);

                match copy_iostore_with_compression_check(&path, &output_dir, &window) {
                    Ok(count) => copied_count += count as i32,
                    Err(e) => {
                        error!("[QuickOrganize] Failed to process IoStore: {}", e);
                        let _ = window.emit("install_log", format!("[QuickOrganize] ERROR: {}", e));
                    }
                }
            }
        }
        // Handle directories - copy all pak/utoc/ucas files preserving subfolder structure
        else if path.is_dir() {
            let mut processed_utocs: std::collections::HashSet<PathBuf> =
                std::collections::HashSet::new();

            for entry in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                let entry_path = entry.path();
                if let Some(entry_ext) = entry_path.extension().and_then(|s| s.to_str()) {
                    if entry_ext == "pak" {
                        // Prepare destination with subfolder structure
                        let dest = match prepare_dest_with_subfolders(
                            entry_path,
                            &path,
                            &output_dir,
                            &window,
                        ) {
                            Ok(d) => d,
                            Err(e) => {
                                error!("[QuickOrganize] {}", e);
                                let _ = window
                                    .emit("install_log", format!("[QuickOrganize] ERROR: {}", e));
                                continue;
                            }
                        };

                        let file_name_str = entry_path
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or("");
                        cleanup_conflicting_mods(
                            dest.parent().unwrap_or(&output_dir),
                            file_name_str,
                        );

                        if let Err(e) = std::fs::copy(entry_path, &dest) {
                            error!(
                                "[QuickOrganize] Failed to copy {}: {}",
                                entry_path.file_name().unwrap().to_string_lossy(),
                                e
                            );
                        } else {
                            let rel_dest = dest.strip_prefix(&output_dir).unwrap_or(&dest);
                            info!("[QuickOrganize] Copied: {}", rel_dest.display());
                            let _ = window.emit(
                                "install_log",
                                format!("[QuickOrganize] Copied: {}", rel_dest.display()),
                            );
                            copied_count += 1;
                        }
                    } else if entry_ext == "utoc" {
                        // Process IoStore with compression check, preserving subfolder structure
                        let ucas_path = entry_path.with_extension("ucas");
                        if ucas_path.exists() && !processed_utocs.contains(entry_path) {
                            processed_utocs.insert(entry_path.to_path_buf());

                            // Determine destination directory with subfolder preservation
                            let dest_dir =
                                match get_iostore_dest_dir(entry_path, &path, &output_dir, &window)
                                {
                                    Ok(d) => d,
                                    Err(e) => {
                                        error!("[QuickOrganize] {}", e);
                                        let _ = window.emit(
                                            "install_log",
                                            format!("[QuickOrganize] ERROR: {}", e),
                                        );
                                        continue;
                                    }
                                };

                            let file_name_str = entry_path
                                .file_name()
                                .unwrap_or_default()
                                .to_str()
                                .unwrap_or("");
                            cleanup_conflicting_mods(&dest_dir, file_name_str);

                            match copy_iostore_with_compression_check(
                                entry_path, &dest_dir, &window,
                            ) {
                                Ok(count) => copied_count += count as i32,
                                Err(e) => {
                                    error!("[QuickOrganize] Failed to process IoStore: {}", e);
                                    let _ = window.emit(
                                        "install_log",
                                        format!("[QuickOrganize] ERROR: {}", e),
                                    );
                                }
                            }
                        }
                    }
                    // Skip .ucas files - they're handled together with .utoc
                }
            }
        }
    }

    let _ = window.emit(
        "install_log",
        format!("[QuickOrganize] Done! Copied {} file(s)", copied_count),
    );
    info!(
        "[QuickOrganize] Completed: {} files copied to {}",
        copied_count,
        output_dir.display()
    );

    Ok(copied_count)
}

#[tauri::command]
async fn install_mods(
    mods: Vec<ModToInstall>,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, AtomicI32};
    use std::sync::Arc as StdArc;

    let state_guard = state.lock().unwrap();
    let mod_directory = state_guard.game_path.clone();
    let parallel_processing = state_guard.parallel_processing;
    drop(state_guard);

    if !mod_directory.exists() {
        std::fs::create_dir_all(&mod_directory)
            .map_err(|e| format!("Failed to create mods directory: {}", e))?;
    }

    // Convert paths to properly initialized InstallableMods
    use crate::install_mod::map_paths_to_mods;

    // Deduplicate paths (multi-PAK archives send the same archive path multiple times)
    let all_paths: Vec<PathBuf> = mods.iter().map(|m| PathBuf::from(&m.path)).collect();
    let mut seen_paths = std::collections::HashSet::new();
    let mut unique_paths: Vec<PathBuf> = Vec::new();
    for p in &all_paths {
        if seen_paths.insert(p.clone()) {
            unique_paths.push(p.clone());
        }
    }

    // Log the paths we're trying to install
    for p in &unique_paths {
        info!("[Install] Processing path: {}", p.display());
        let _ = window.emit(
            "install_log",
            format!("[Install] Processing path: {}", p.display()),
        );
    }

    let mut installable_mods = map_paths_to_mods(&unique_paths);

    // Filter out mods the user disabled in the frontend (multi-PAK selection)
    // Build a set of enabled mod_names; only keep installable_mods that match
    let enabled_names: std::collections::HashSet<String> = mods
        .iter()
        .filter(|m| m.enabled)
        .map(|m| m.mod_name.clone())
        .collect();
    let disabled_names: Vec<String> = mods
        .iter()
        .filter(|m| !m.enabled)
        .map(|m| m.mod_name.clone())
        .collect();
    if !disabled_names.is_empty() {
        info!(
            "[Install] User disabled {} mod(s): {:?}",
            disabled_names.len(),
            disabled_names
        );
        let _ = window.emit(
            "install_log",
            format!(
                "[Install] Skipping {} disabled mod(s): {}",
                disabled_names.len(),
                disabled_names.join(", ")
            ),
        );
        installable_mods.retain(|m| enabled_names.contains(&m.mod_name));
    }

    // Check if we actually have mods to install
    if installable_mods.is_empty() {
        error!(
            "[Install] No valid mods found from {} input path(s)",
            unique_paths.len()
        );
        let _ = window.emit("install_log", "ERROR: No valid mods found to install!");
        let _ = window.emit("install_log", "Possible causes:");
        let _ = window.emit(
            "install_log",
            "  - PAK file couldn't be read (wrong AES key or corrupted)",
        );
        let _ = window.emit(
            "install_log",
            "  - Archive contains no .pak files or content folders",
        );
        let _ = window.emit("install_log", "  - Directory contains no valid content");
        let error_msg = "No valid mods found to install. Check the install logs for details.";
        toast_events::emit_installation_failed(&window, error_msg);
        return Err(error_msg.to_string());
    }

    // Apply user settings to each installable mod
    // Match by mod_name since archives expand to multiple mods with the same path
    // Build a lookup map keyed by mod_name (unique per PAK even in multi-PAK archives)
    crate::install_mod::write_install_debug(&format!(
        "=== Building user_settings from {} frontend entries ===",
        mods.len()
    ));
    for (i, m) in mods.iter().enumerate() {
        crate::install_mod::write_install_debug(&format!(
            "  frontend[{}]: mod_name='{}', path='{}', to_repak={}",
            i, m.mod_name, m.path, m.to_repak
        ));
    }
    let user_settings: std::collections::HashMap<String, &ModToInstall> = mods
        .iter()
        .map(|m| {
            // Use mod_name as primary key; fallback to path stem if mod_name is empty
            let name = if !m.mod_name.is_empty() {
                m.mod_name.clone()
            } else {
                PathBuf::from(&m.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            };
            (name, m)
        })
        .collect();
    crate::install_mod::write_install_debug(&format!(
        "  HashMap keys: {:?}",
        user_settings.keys().collect::<Vec<_>>()
    ));

    for installable in installable_mods.iter_mut() {
        // Try to find matching user settings by mod name, then fallback to path match
        crate::install_mod::write_install_debug(&format!(
            "  Matching installable '{}' (repak={})...",
            installable.mod_name, installable.repak
        ));
        let by_name = user_settings.get(&installable.mod_name).copied();
        let matched = by_name.or_else(|| {
            mods.iter().find(|m| {
                let m_path = PathBuf::from(&m.path);
                m_path == installable.mod_path
            })
        });

        if let Some(mod_to_install) = matched {
            // Apply custom name if provided
            if let Some(ref custom) = mod_to_install.custom_name {
                if !custom.is_empty() {
                    installable.mod_name = custom.clone();
                }
            }

            installable.repak = mod_to_install.to_repak;
            installable.force_legacy_pak = mod_to_install.force_legacy;
            installable.install_subfolder = mod_to_install.install_subfolder.clone();
            installable.obfuscate = mod_to_install.obfuscate;
            installable.hybrid = mod_to_install.hybrid;
        } else {
            // No matching user settings found (common for archive-expanded mods)
            // IMPORTANT: Do NOT override repak or force_legacy_pak here!
            // These flags were already correctly set by find_mods_from_archive
            // based on the actual PAK contents. Overriding them breaks IoStore conversion.
            if let Some(first_mod) = mods.first() {
                installable.install_subfolder = first_mod.install_subfolder.clone();
                installable.obfuscate = first_mod.obfuscate;
                installable.hybrid = first_mod.hybrid;
                // repak and force_legacy_pak are intentionally NOT overridden
            }
        }

        installable.parallel_processing = parallel_processing;
    }

    // Use existing installation logic
    let installed_counter = StdArc::new(AtomicI32::new(0));
    let stop_flag = StdArc::new(AtomicBool::new(false));

    let total = installable_mods.len() as i32;
    let counter_clone = installed_counter.clone();
    let _stop_clone = stop_flag.clone();
    let window_clone = window.clone();

    // Spawn installation thread
    let window_for_logs = window.clone();
    std::thread::spawn(move || {
        use std::panic;

        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            window_for_logs
                .emit("install_log", "Starting installation...")
                .ok();
            window_for_logs
                .emit(
                    "install_log",
                    format!("Installing {} mod(s)", installable_mods.len()),
                )
                .ok();

            for (idx, imod) in installable_mods.iter().enumerate() {
                window_for_logs
                    .emit(
                        "install_log",
                        format!(
                            "[{}/{}] Mod: {}",
                            idx + 1,
                            installable_mods.len(),
                            imod.mod_name
                        ),
                    )
                    .ok();
                window_for_logs
                    .emit("install_log", format!("  - Repak: {}", imod.repak))
                    .ok();
                window_for_logs
                    .emit(
                        "install_log",
                        format!("  - Force Legacy PAK: {}", imod.force_legacy_pak),
                    )
                    .ok();
            }

            window_for_logs
                .emit("install_log", "Calling installation logic...")
                .ok();
            window_for_logs
                .emit(
                    "install_log",
                    format!("Mod directory: {}", mod_directory.display()),
                )
                .ok();

            use crate::install_mod::install_mod_logic::install_mods_in_viewport;

            window_for_logs
                .emit("install_log", "Entering install_mods_in_viewport...")
                .ok();

            // Log each mod's path before processing
            for (idx, m) in installable_mods.iter().enumerate() {
                window_for_logs
                    .emit(
                        "install_log",
                        format!("  Mod {} path exists: {}", idx, m.mod_path.exists()),
                    )
                    .ok();
                window_for_logs
                    .emit(
                        "install_log",
                        format!("  Mod {} path: {}", idx, m.mod_path.display()),
                    )
                    .ok();
            }

            let install_results = install_mods_in_viewport(
                &mut installable_mods,
                &mod_directory,
                &installed_counter,
                &stop_flag,
            );
            window_for_logs
                .emit("install_log", "Exited install_mods_in_viewport")
                .ok();

            // Report per-mod results
            let mut failed: Vec<(String, String)> = Vec::new();
            let mut succeeded: Vec<String> = Vec::new();
            for r in &install_results {
                if r.success {
                    succeeded.push(r.mod_name.clone());
                } else {
                    failed.push((
                        r.mod_name.clone(),
                        r.error
                            .clone()
                            .unwrap_or_else(|| "unknown error".to_string()),
                    ));
                }
            }
            for (name, err) in &failed {
                window_for_logs
                    .emit("install_log", format!("[FAIL] {}: {}", name, err))
                    .ok();
            }
            window_for_logs
                .emit(
                    "install_log",
                    format!(
                        "Summary: {} succeeded, {} failed",
                        succeeded.len(),
                        failed.len()
                    ),
                )
                .ok();
            install_results
        }));

        match result {
            Ok(install_results) => {
                let total = install_results.len();
                let failed: Vec<&crate::install_mod::install_mod_logic::ModInstallResult> =
                    install_results.iter().filter(|r| !r.success).collect();
                let failed_count = failed.len();
                if total == 0 {
                    let msg = "No mods were processed.".to_string();
                    window_for_logs.emit("install_log", &msg).ok();
                    toast_events::emit_installation_failed(&window_for_logs, &msg);
                } else if failed_count == total {
                    // All failed
                    let first_err = failed
                        .first()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "Unknown error".to_string());
                    let msg = format!(
                        "Installation failed for all {} mod(s). First error: {}",
                        total, first_err
                    );
                    window_for_logs.emit("install_log", &msg).ok();
                    toast_events::emit_installation_failed(&window_for_logs, &msg);
                } else if failed_count > 0 {
                    // Partial
                    let names: Vec<String> = failed.iter().map(|r| r.mod_name.clone()).collect();
                    let msg = format!(
                        "Partial install: {} of {} mod(s) failed: {}",
                        failed_count,
                        total,
                        names.join(", ")
                    );
                    window_for_logs.emit("install_log", &msg).ok();
                    toast_events::emit_installation_failed(&window_for_logs, &msg);
                } else {
                    window_for_logs
                        .emit("install_log", "Installation completed successfully!")
                        .ok();
                }
            }
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    format!("PANIC: {}", s)
                } else if let Some(s) = e.downcast_ref::<String>() {
                    format!("PANIC: {}", s)
                } else {
                    "PANIC: Unknown error".to_string()
                };
                window_for_logs.emit("install_log", &msg).ok();
                toast_events::emit_installation_failed(&window_for_logs, &msg);
                error!("Installation thread panicked!");
            }
        }

        // --- CLEANUP TEMP HYBRID FOLDERS ---
        for path in unique_paths {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with("hybrid_merge_") {
                    if let Err(e) = std::fs::remove_dir_all(&path) {
                        let _ = window_for_logs.emit(
                            "install_log",
                            format!(
                                "Failed to clean up temp hybrid folder {}: {}",
                                path.display(),
                                e
                            ),
                        );
                    } else {
                        let _ = window_for_logs.emit(
                            "install_log",
                            format!("Cleaned up temp hybrid folder {}", path.display()),
                        );
                    }
                }
            }
        }
    });

    // Monitor progress
    std::thread::spawn(move || loop {
        let current = counter_clone.load(std::sync::atomic::Ordering::SeqCst);
        if current == -255 {
            window_clone.emit("install_complete", ()).ok();
            break;
        }
        let progress = (current as f32 / total as f32) * 100.0;
        window_clone.emit("install_progress", progress).ok();
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    Ok(())
}

#[tauri::command]
async fn delete_mod(
    path: String,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    log::info!("delete_mod called with path: {}", path);

    // Determine the actual file to delete - check both .pak and .bak_repak variants
    let (actual_path, is_disabled) = if path_buf.exists() {
        (path_buf.clone(), path.ends_with(".bak_repak"))
    } else if path.ends_with(".pak") {
        // The .pak file doesn't exist, check if there's a disabled version (.bak_repak)
        let disabled_path = PathBuf::from(format!("{}.bak_repak", path.trim_end_matches(".pak")));
        if disabled_path.exists() {
            log::info!("Found disabled mod at: {:?}", disabled_path);
            (disabled_path, true)
        } else {
            (path_buf.clone(), false)
        }
    } else if path.ends_with(".bak_repak") {
        // The .bak_repak doesn't exist, check if there's an enabled version (.pak)
        let enabled_path =
            PathBuf::from(format!("{}.pak", path.trim_end_matches(".pak.bak_repak")));
        if enabled_path.exists() {
            log::info!("Found enabled mod at: {:?}", enabled_path);
            (enabled_path, false)
        } else {
            (path_buf.clone(), true)
        }
    } else {
        (path_buf.clone(), false)
    };

    log::info!(
        "Attempting to delete: {:?} (is_disabled: {})",
        actual_path,
        is_disabled
    );

    // Try to delete the main file
    if actual_path.exists() {
        if let Err(e) = std::fs::remove_file(&actual_path) {
            let error_msg = format!("Failed to delete mod file: {}", e);
            toast_events::emit_delete_failed(&window, &error_msg);
            return Err(error_msg);
        }
        log::info!("Deleted main mod file: {:?}", actual_path);
    } else {
        log::warn!("Main mod file does not exist: {:?}", actual_path);
    }

    // Invalidate cache for deleted path
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.mod_details_cache.remove(&actual_path);
    }

    // Determine the base path for IoStore files (always based on .pak name, not .bak_repak)
    let base_pak_path = if is_disabled || path.ends_with(".bak_repak") {
        // For disabled mods, derive the .pak base path
        let path_str = if actual_path.to_string_lossy().ends_with(".bak_repak") {
            actual_path
                .to_string_lossy()
                .trim_end_matches(".bak_repak")
                .to_string()
        } else {
            path.trim_end_matches(".pak.bak_repak").to_string()
        };
        PathBuf::from(format!("{}.pak", path_str))
    } else {
        actual_path.clone()
    };

    log::info!("Base path for IoStore files: {:?}", base_pak_path);

    // Delete associated IoStore files (.ucas and .utoc)
    let ucas_path = base_pak_path.with_extension("ucas");
    if ucas_path.exists() {
        if let Err(e) = std::fs::remove_file(&ucas_path) {
            log::warn!("Failed to delete .ucas file: {}", e);
        } else {
            log::info!("Deleted associated .ucas file: {:?}", ucas_path);
        }
    }

    let utoc_path = base_pak_path.with_extension("utoc");
    if utoc_path.exists() {
        if let Err(e) = std::fs::remove_file(&utoc_path) {
            log::warn!("Failed to delete .utoc file: {}", e);
        } else {
            log::info!("Deleted associated .utoc file: {:?}", utoc_path);
        }
    }

    Ok(())
}

/// Result of an update_mod operation
#[derive(Clone, Serialize, Deserialize)]
struct UpdateModResult {
    /// Path to the newly installed mod
    new_mod_path: String,
    /// Whether the old mod was successfully deleted
    old_mod_deleted: bool,
    /// The preserved metadata that was applied
    preserved_enabled_state: bool,
    preserved_folder: Option<String>,
}

/// Update (replace) an existing mod with new mod files.
/// This preserves the mod's metadata (folder location, enabled state, custom name, tags)
/// and replaces the old mod files with the new ones.
///
/// # Arguments
/// * `old_mod_path` - Path to the existing mod to be replaced
/// * `new_mod_source` - Path to the new mod files (can be .pak, .zip, .rar, .7z, or directory)
/// * `preserve_name` - If true, keeps the old mod's name; if false, uses the new mod's name
/// * `window` - Tauri window for emitting events
/// * `state` - Application state
#[tauri::command]
async fn update_mod(
    old_mod_path: String,
    new_mod_source: String,
    preserve_name: bool,
    #[allow(non_snake_case)] obfuscate: Option<bool>,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<UpdateModResult, String> {
    info!(
        "update_mod called: old={}, new={}, preserve_name={}, obfuscate={:?}",
        old_mod_path, new_mod_source, preserve_name, obfuscate
    );

    let old_path = PathBuf::from(&old_mod_path);
    let new_source = PathBuf::from(&new_mod_source);

    // Validate new source exists
    if !new_source.exists() {
        let err = format!("New mod source does not exist: {}", new_mod_source);
        toast_events::emit_installation_failed(&window, &err);
        return Err(err);
    }

    // ========================================================================
    // Step 1: Gather metadata from the old mod
    // ========================================================================

    // Determine the actual old mod path (handle .pak vs .bak_repak)
    let (actual_old_path, was_disabled) = if old_path.exists() {
        (
            old_path.clone(),
            old_mod_path.ends_with(".bak_repak") || old_mod_path.ends_with(".pak_disabled"),
        )
    } else if old_mod_path.ends_with(".pak") {
        // Check for disabled versions
        let bak_repak_path = PathBuf::from(format!(
            "{}.bak_repak",
            old_mod_path.trim_end_matches(".pak")
        ));
        let pak_disabled_path = PathBuf::from(format!(
            "{}_disabled",
            old_mod_path.trim_end_matches(".pak")
        ));
        if bak_repak_path.exists() {
            (bak_repak_path, true)
        } else if pak_disabled_path.exists() {
            (pak_disabled_path, true)
        } else {
            return Err(format!("Old mod not found: {}", old_mod_path));
        }
    } else {
        return Err(format!("Old mod not found: {}", old_mod_path));
    };

    info!(
        "Actual old mod path: {:?}, was_disabled: {}",
        actual_old_path, was_disabled
    );

    // Get the old mod's folder (subfolder within mods directory)
    let game_path = {
        let state_guard = state.lock().unwrap();
        state_guard.game_path.clone()
    };

    let install_subfolder = if let Some(parent) = actual_old_path.parent() {
        if parent == game_path {
            String::new() // Root folder
        } else {
            parent
                .strip_prefix(&game_path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default()
        }
    } else {
        String::new()
    };

    info!("Preserved install subfolder: {}", install_subfolder);

    // Migrate the old mod's custom tags
    let old_custom_tags = {
        let mut state_guard = state.lock().unwrap();
        let old_key = get_mod_key(&actual_old_path);
        state_guard.mod_tags.remove(&old_key).unwrap_or_default()
    };

    // (Deprecated old metadata)
    let (old_custom_name, old_folder_id) = (None::<String>, None::<String>);

    info!(
        "Preserved metadata - custom_name: {:?}, tags: {:?}, folder_id: {:?}",
        old_custom_name, old_custom_tags, old_folder_id
    );

    // Get the old mod's base name (for naming the new mod if preserve_name is true)
    let old_mod_name = actual_old_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            // Strip .bak_repak or _disabled suffix if present
            s.trim_end_matches(".bak_repak")
                .trim_end_matches("_disabled")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown".to_string());

    // ========================================================================
    // Step 2: Delete the old mod files
    // ========================================================================

    info!("Deleting old mod files...");

    // Delete main file
    let mut old_deleted = false;
    if actual_old_path.exists() {
        if let Err(e) = std::fs::remove_file(&actual_old_path) {
            warn!("Failed to delete old mod file: {}", e);
        } else {
            old_deleted = true;
            info!("Deleted old mod file: {:?}", actual_old_path);
        }
    }

    // Delete associated IoStore files (.ucas and .utoc)
    // Base path is always the .pak version
    let base_pak_path = if was_disabled {
        let path_str = actual_old_path.to_string_lossy();
        let clean = path_str
            .trim_end_matches(".bak_repak")
            .trim_end_matches("_disabled");
        if clean.ends_with(".pak") {
            PathBuf::from(clean)
        } else {
            PathBuf::from(format!("{}.pak", clean))
        }
    } else {
        actual_old_path.clone()
    };

    for ext in &["ucas", "utoc"] {
        let companion_path = base_pak_path.with_extension(ext);
        if companion_path.exists() {
            if let Err(e) = std::fs::remove_file(&companion_path) {
                warn!("Failed to delete .{} file: {}", ext, e);
            } else {
                info!("Deleted associated .{} file: {:?}", ext, companion_path);
            }
        }
    }

    // ========================================================================
    // Step 3: Install the new mod
    // ========================================================================

    info!("Installing new mod from: {:?}", new_source);

    // Determine the mod name to use
    let mod_name = if preserve_name {
        old_custom_name.clone().unwrap_or(old_mod_name.clone())
    } else {
        new_source
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "NewMod".to_string())
    };

    // Use the existing install_mods logic
    use crate::install_mod::map_paths_to_mods;
    use std::sync::atomic::{AtomicBool, AtomicI32};
    use std::sync::Arc as StdArc;

    let state_guard = state.lock().unwrap();
    let mod_directory = state_guard.game_path.clone();
    // Use per-update obfuscate param if provided; fallback to false
    let obfuscate = obfuscate.unwrap_or(false);
    drop(state_guard);

    let paths = vec![new_source.clone()];
    let mut installable_mods = map_paths_to_mods(&paths);

    if installable_mods.is_empty() {
        let err = "Failed to parse new mod source - no valid mods found";
        toast_events::emit_installation_failed(&window, err);
        return Err(err.to_string());
    }

    // Apply settings to the installable mod
    if let Some(installable) = installable_mods.get_mut(0) {
        installable.mod_name = mod_name.clone();
        installable.install_subfolder = install_subfolder.clone();
        installable.obfuscate = obfuscate;
    }

    // Install synchronously for update operation (we need to know the result)
    let installed_counter = StdArc::new(AtomicI32::new(0));
    let stop_flag = StdArc::new(AtomicBool::new(false));

    let window_clone = window.clone();
    window_clone
        .emit(
            "install_log",
            format!("[Update] Replacing mod: {}", old_mod_name),
        )
        .ok();
    window_clone
        .emit(
            "install_log",
            format!("[Update] New source: {}", new_mod_source),
        )
        .ok();

    use crate::install_mod::install_mod_logic::install_mods_in_viewport;

    let update_results = install_mods_in_viewport(
        &mut installable_mods,
        &mod_directory,
        &installed_counter,
        &stop_flag,
    );

    // If any mod failed in the update, surface the error instead of pretending it worked
    if let Some(failed) = update_results.iter().find(|r| !r.success) {
        let err_msg = failed
            .error
            .clone()
            .unwrap_or_else(|| "Unknown error".to_string());
        let msg = format!("Update failed for '{}': {}", failed.mod_name, err_msg);
        window_clone.emit("install_log", &msg).ok();
        toast_events::emit_installation_failed(&window_clone, &msg);
        return Err(msg);
    }

    // ========================================================================
    // Step 4: Apply preserved metadata to the new mod
    // ========================================================================

    // Determine the new mod's path
    let new_mod_filename = format!("{}_9999999_P.pak", mod_name);
    let new_mod_path = if install_subfolder.is_empty() {
        mod_directory.join(&new_mod_filename)
    } else {
        mod_directory
            .join(&install_subfolder)
            .join(&new_mod_filename)
    };

    {
        let mut state_guard = state.lock().unwrap();
        let new_key = get_mod_key(&new_mod_path);
        if !old_custom_tags.is_empty() {
            state_guard.mod_tags.insert(new_key, old_custom_tags);
            let _ = save_state(&state_guard);
        }
    }

    info!("Expected new mod path: {:?}", new_mod_path);

    // If the old mod was disabled, disable the new one too
    if was_disabled && new_mod_path.exists() {
        let disabled_path = PathBuf::from(format!(
            "{}.bak_repak",
            new_mod_path.to_string_lossy().trim_end_matches(".pak")
        ));
        if let Err(e) = std::fs::rename(&new_mod_path, &disabled_path) {
            warn!("Failed to disable new mod to match old state: {}", e);
        } else {
            info!("Disabled new mod to match old mod's state");
        }
    }

    // Metadata updates removed (deprecated)

    // Emit success event
    toast_events::emit_success(
        &window,
        "Mod Updated",
        format!("Successfully replaced mod: {}", mod_name),
    );
    window.emit("install_complete", ()).ok();

    info!("update_mod completed successfully");

    Ok(UpdateModResult {
        new_mod_path: new_mod_path.to_string_lossy().to_string(),
        old_mod_deleted: old_deleted,
        preserved_enabled_state: was_disabled,
        preserved_folder: if install_subfolder.is_empty() {
            None
        } else {
            Some(install_subfolder)
        },
    })
}

#[tauri::command]
async fn open_in_explorer(path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);

    info!("open_in_explorer called: path={}", path);

    if !path_buf.exists() {
        return Err(format!("Path does not exist: {}", path_buf.display()));
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // On Windows, use explorer.exe with /select, to highlight the file
        // The path must be quoted if it contains spaces, and use backslashes
        let canonical_path = path_buf.canonicalize().unwrap_or_else(|_| path_buf.clone());

        // Remove the \\?\ prefix that canonicalize adds on Windows
        let path_str = canonical_path.to_string_lossy();
        let clean_path = if path_str.starts_with(r"\\?\") {
            path_str[4..].to_string()
        } else {
            path_str.to_string()
        };

        info!("open_in_explorer: using path={}", clean_path);

        // Use /select, with the path - explorer handles the quoting
        let select_arg = format!("/select,\"{}\"", clean_path);
        std::process::Command::new("explorer.exe")
            .raw_arg(&select_arg)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, use open -R to reveal the file in Finder
        std::process::Command::new("open")
            .args(["-R", &path_buf.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, open the parent directory
        let dir_to_open = if path_buf.is_file() {
            path_buf
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or(path_buf.clone())
        } else {
            path_buf.clone()
        };
        std::process::Command::new("xdg-open")
            .arg(&dir_to_open)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
async fn copy_to_clipboard(text: String, window: Window) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut child = Command::new("powershell")
            .args([
                "-Command",
                "Set-Clipboard",
                "-Value",
                &format!("'{}'", text.replace("'", "''")),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW - prevents PowerShell window from showing
            .spawn()
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

        child
            .wait()
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
        }

        child
            .wait()
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Try xclip first, then xsel
        let result = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn();

        match result {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
                }
                child
                    .wait()
                    .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
            }
            Err(_) => {
                let mut child = Command::new("xsel")
                    .args(["--clipboard", "--input"])
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| {
                        format!(
                            "Failed to copy to clipboard (neither xclip nor xsel available): {}",
                            e
                        )
                    })?;

                if let Some(mut stdin) = child.stdin.take() {
                    stdin
                        .write_all(text.as_bytes())
                        .map_err(|e| format!("Failed to write to clipboard: {}", e))?;
                }
                child
                    .wait()
                    .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
            }
        }
    }

    // Emit an event to notify the frontend that the copy was successful
    let _ = window.emit("clipboard-copied", text);

    Ok(())
}

#[tauri::command]
async fn rename_mod(
    mod_path: String,
    new_name: String,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<String, String> {
    let old_path_buf = PathBuf::from(&mod_path);

    info!(
        "rename_mod called: mod_path={}, new_name={}",
        mod_path, new_name
    );

    if !old_path_buf.exists() {
        let error_msg = format!("File does not exist: {}", mod_path);
        toast_events::emit_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Get the parent directory
    let parent = old_path_buf
        .parent()
        .ok_or_else(|| "Cannot get parent directory".to_string())?;

    // Get the full filename to detect extension properly
    let filename = old_path_buf
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Detect extension - handle .bak_repak as disabled .pak
    let (extension, is_pak_type) = if filename.ends_with(".bak_repak") {
        ("bak_repak".to_string(), true)
    } else if filename.ends_with(".pak") {
        ("pak".to_string(), true)
    } else {
        let ext = old_path_buf
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        (ext, false)
    };

    // Get the old stem (filename without extension) BEFORE renaming
    // For .bak_repak files, we need to strip that suffix manually
    let old_stem = if filename.ends_with(".bak_repak") {
        filename.trim_end_matches(".bak_repak").to_string()
    } else {
        old_path_buf
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    };

    // Extract the priority suffix (e.g., _9999999_P) from the old stem
    let priority_suffix_regex = regex_lite::Regex::new(r"(_\d+_P)+$").unwrap();
    let old_priority_suffix = priority_suffix_regex
        .find(&old_stem)
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();

    // Check if new_name already has a priority suffix
    let new_name_has_suffix = priority_suffix_regex.is_match(&new_name);

    // Build the final new stem: new_name + priority suffix (if not already present)
    let new_stem = if new_name_has_suffix {
        new_name.clone()
    } else if !old_priority_suffix.is_empty() {
        // Preserve the old priority suffix
        format!("{}{}", new_name, old_priority_suffix)
    } else {
        // Add default priority suffix if none existed
        format!("{}_9999999_P", new_name)
    };

    info!(
        "rename_mod: old_stem={}, extension={}, is_pak_type={}, new_stem={}",
        old_stem, extension, is_pak_type, new_stem
    );

    // Build the new path with the new stem and same extension
    let new_file_name = if extension.is_empty() {
        new_stem.clone()
    } else {
        format!("{}.{}", new_stem, extension)
    };
    let new_path = parent.join(&new_file_name);

    if new_path.exists() {
        let error_msg = format!("A file with name '{}' already exists", new_file_name);
        toast_events::emit_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // If it's a .pak or .bak_repak file, rename associated IoStore files (.ucas, .utoc) FIRST
    // Do this before renaming the main file to ensure we have the correct old stem
    if is_pak_type {
        // Rename .ucas file if it exists
        let old_ucas = parent.join(format!("{}.ucas", old_stem));
        let new_ucas = parent.join(format!("{}.ucas", new_stem));
        info!(
            "rename_mod: checking ucas: {} exists={}",
            old_ucas.display(),
            old_ucas.exists()
        );
        if old_ucas.exists() {
            match std::fs::rename(&old_ucas, &new_ucas) {
                Ok(_) => info!("rename_mod: renamed ucas to {}", new_ucas.display()),
                Err(e) => warn!("rename_mod: failed to rename ucas: {}", e),
            }
        }

        // Rename .utoc file if it exists
        let old_utoc = parent.join(format!("{}.utoc", old_stem));
        let new_utoc = parent.join(format!("{}.utoc", new_stem));
        info!(
            "rename_mod: checking utoc: {} exists={}",
            old_utoc.display(),
            old_utoc.exists()
        );
        if old_utoc.exists() {
            match std::fs::rename(&old_utoc, &new_utoc) {
                Ok(_) => info!("rename_mod: renamed utoc to {}", new_utoc.display()),
                Err(e) => warn!("rename_mod: failed to rename utoc: {}", e),
            }
        }
    }

    // Rename the main file
    if let Err(e) = std::fs::rename(&old_path_buf, &new_path) {
        let error_msg = format!("Failed to rename file: {}", e);
        toast_events::emit_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Invalidate cache for old path and migrate tags
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.mod_details_cache.remove(&old_path_buf);

        let old_key = get_mod_key(&old_path_buf);
        let new_key = get_mod_key(&new_path);
        if old_key != new_key {
            if let Some(tags) = state_guard.mod_tags.remove(&old_key) {
                state_guard.mod_tags.insert(new_key, tags);
                let _ = save_state(&state_guard);
            }
        }
    }

    info!(
        "rename_mod: successfully renamed {} to {}",
        mod_path,
        new_path.display()
    );

    Ok(new_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn create_folder(
    name: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<String, String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    // Create physical directory in ~mods
    let folder_path = game_path.join(&name);

    if folder_path.exists() {
        let error_msg = "Folder already exists".to_string();
        toast_events::emit_folder_create_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Use create_dir_all to support nested paths like "Category/Subcategory"
    if let Err(e) = std::fs::create_dir_all(&folder_path) {
        let error_msg = format!("Failed to create folder: {}", e);
        toast_events::emit_folder_create_failed(&window, &error_msg);
        return Err(error_msg);
    }

    Ok(name)
}

#[tauri::command]
async fn get_folders(state: State<'_, Arc<Mutex<AppState>>>) -> Result<Vec<ModFolder>, String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    if !game_path.exists() {
        return Ok(Vec::new());
    }

    let mut folders = Vec::new();

    // Get root folder name (e.g., "~mods")
    let root_name = game_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Mods")
        .to_string();

    // Count mods directly in root (not in subfolders)
    let root_mod_count = std::fs::read_dir(game_path)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    if path.is_file() {
                        let ext = path.extension().and_then(|s| s.to_str());
                        ext == Some("pak")
                            || ext == Some("bak_repak")
                            || ext == Some("pak_disabled")
                    } else {
                        false
                    }
                })
                .count()
        })
        .unwrap_or(0);

    // Add root folder first (depth 0) - use actual folder name as ID
    folders.push(ModFolder {
        id: root_name.clone(), // Use actual name like "~mods" as ID
        name: root_name.clone(),
        enabled: true,
        expanded: true,
        color: None,
        depth: 0,
        parent_id: None,
        is_root: true,
        mod_count: root_mod_count,
    });

    // Recursively scan for subdirectories using WalkDir
    for entry in WalkDir::new(game_path)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_dir() {
            // Calculate relative path from game_path to get ID
            let relative_path = path
                .strip_prefix(game_path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| "Unknown".to_string());

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string();

            // Calculate depth (number of path segments)
            let depth = relative_path.split('/').count();

            // Calculate parent ID
            let parent_id = if depth > 1 {
                // If depth > 1, parent is the directory containing this one
                // e.g. "A/B" -> parent is "A"
                let parent_rel = std::path::Path::new(&relative_path)
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"));
                parent_rel
            } else {
                // If depth is 1, parent is the root folder
                Some(root_name.clone())
            };

            // Count mods in this folder (only direct children)
            let mod_count = std::fs::read_dir(&path)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            let p = e.path();
                            if p.is_file() {
                                let ext = p.extension().and_then(|s| s.to_str());
                                ext == Some("pak")
                                    || ext == Some("bak_repak")
                                    || ext == Some("pak_disabled")
                            } else {
                                false
                            }
                        })
                        .count()
                })
                .unwrap_or(0);

            folders.push(ModFolder {
                id: relative_path, // ID is the relative path (e.g. "Category/Subcategory")
                name,
                enabled: true,
                expanded: true,
                color: None,
                depth,
                parent_id,
                is_root: false,
                mod_count,
            });
        }
    }

    Ok(folders)
}

/// Get detailed info about the root mods folder
#[tauri::command]
async fn get_root_folder_info(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<RootFolderInfo, String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    if !game_path.exists() {
        return Err("Game path does not exist".to_string());
    }

    let root_name = game_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Mods")
        .to_string();

    let mut direct_mod_count = 0;
    let mut subfolder_count = 0;

    for entry in std::fs::read_dir(game_path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        if path.is_dir() {
            subfolder_count += 1;
        } else if path.is_file() {
            let ext = path.extension().and_then(|s| s.to_str());
            if ext == Some("pak") || ext == Some("bak_repak") || ext == Some("pak_disabled") {
                direct_mod_count += 1;
            }
        }
    }

    Ok(RootFolderInfo {
        name: root_name,
        path: game_path.to_string_lossy().to_string(),
        direct_mod_count,
        subfolder_count,
    })
}

#[tauri::command]
async fn update_folder(
    _folder: ModFolder,
    _state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
async fn delete_folder(
    id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<(), String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    let root_name = game_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if id == root_name {
        let error_msg = "Cannot delete the root folder".to_string();
        toast_events::emit_folder_delete_failed(&window, &error_msg);
        return Err(error_msg);
    }

    let folder_path = game_path.join(&id);

    if !folder_path.exists() {
        let error_msg = "Folder does not exist".to_string();
        toast_events::emit_folder_delete_failed(&window, &error_msg);
        return Err(error_msg);
    }

    let canonical_game_path = match game_path.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            let error_msg = format!("Failed to resolve game path: {}", e);
            toast_events::emit_folder_delete_failed(&window, &error_msg);
            return Err(error_msg);
        }
    };

    let canonical_folder_path = match folder_path.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            let error_msg = format!("Failed to resolve folder path: {}", e);
            toast_events::emit_folder_delete_failed(&window, &error_msg);
            return Err(error_msg);
        }
    };

    if !canonical_folder_path.starts_with(&canonical_game_path)
        || canonical_folder_path == canonical_game_path
    {
        let error_msg = "Refusing to delete outside the mods directory".to_string();
        toast_events::emit_folder_delete_failed(&window, &error_msg);
        return Err(error_msg);
    }

    if !canonical_folder_path.is_dir() {
        let error_msg = "Target path is not a folder".to_string();
        toast_events::emit_folder_delete_failed(&window, &error_msg);
        return Err(error_msg);
    }

    if let Err(e) = std::fs::remove_dir_all(&canonical_folder_path) {
        let error_msg = format!("Failed to delete folder: {}", e);
        toast_events::emit_folder_delete_failed(&window, &error_msg);
        return Err(error_msg);
    }

    Ok(())
}

#[tauri::command]
async fn rename_folder(
    id: String,
    new_name: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<String, String> {
    let mut state = state.lock().unwrap();
    let game_path = state.game_path.clone();

    // Prevent renaming root folder
    if id == game_path.file_name().and_then(|n| n.to_str()).unwrap_or("") {
        let error_msg = "Cannot rename the root folder".to_string();
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Validate new name (no path separators allowed in the leaf name)
    if new_name.contains('/') || new_name.contains('\\') || new_name.is_empty() {
        let error_msg = "Invalid folder name".to_string();
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    let old_path = game_path.join(&id);
    if !old_path.exists() {
        let error_msg = "Folder does not exist".to_string();
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Build new path: replace only the last path segment with new_name
    let new_id = if let Some(parent) = std::path::Path::new(&id).parent() {
        let parent_str = parent.to_string_lossy().replace('\\', "/");
        if parent_str.is_empty() {
            new_name.clone()
        } else {
            format!("{}/{}", parent_str, new_name)
        }
    } else {
        new_name.clone()
    };

    let new_path = game_path.join(&new_id);

    if new_path.exists() {
        let error_msg = format!("A folder named \"{}\" already exists", new_name);
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Rename the physical directory
    if let Err(e) = std::fs::rename(&old_path, &new_path) {
        let error_msg = format!("Failed to rename folder: {}", e);
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    Ok(new_id)
}

/// Move a folder under a different parent (or to root).
/// - `id`: existing folder relative path (e.g. "A/B")
/// - `new_parent_id`: target parent's relative path, or None / root_name to
///   place the folder at the top level.
/// The leaf folder name is preserved; the operation is essentially a rename
/// of the full relative path. Cycles (moving a folder into itself or one of
/// its descendants) and name collisions are rejected.
#[tauri::command]
async fn move_folder(
    id: String,
    new_parent_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<String, String> {
    let mut state = state.lock().unwrap();
    let game_path = state.game_path.clone();

    let root_name = game_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    if id.is_empty() || id == root_name {
        let error_msg = "Cannot move the root folder".to_string();
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Normalize new_parent_id: empty string or root_name == root.
    let parent_rel: Option<String> = match new_parent_id.as_deref() {
        None => None,
        Some(s) if s.is_empty() || s == root_name => None,
        Some(s) => Some(s.replace('\\', "/")),
    };

    let leaf_name = std::path::Path::new(&id)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid folder id".to_string())?
        .to_string();

    let new_id = match &parent_rel {
        Some(p) => format!("{}/{}", p, leaf_name),
        None => leaf_name.clone(),
    };

    if new_id == id {
        return Ok(new_id);
    }

    // Reject moving a folder into itself or its descendants.
    if let Some(p) = &parent_rel {
        let descendant_prefix = format!("{}/", id);
        if p == &id || p.starts_with(&descendant_prefix) {
            let error_msg = "Cannot move a folder into itself or its own descendants".to_string();
            toast_events::emit_folder_rename_failed(&window, &error_msg);
            return Err(error_msg);
        }
    }

    let old_path = game_path.join(&id);
    if !old_path.exists() {
        let error_msg = "Folder does not exist".to_string();
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    if let Some(p) = &parent_rel {
        let parent_path = game_path.join(p);
        if !parent_path.exists() || !parent_path.is_dir() {
            let error_msg = "Target parent folder does not exist".to_string();
            toast_events::emit_folder_rename_failed(&window, &error_msg);
            return Err(error_msg);
        }
    }

    let new_path = game_path.join(&new_id);
    if new_path.exists() {
        let error_msg = format!(
            "A folder named \"{}\" already exists at the target location",
            leaf_name
        );
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    if let Err(e) = std::fs::rename(&old_path, &new_path) {
        let error_msg = format!("Failed to move folder: {}", e);
        toast_events::emit_folder_rename_failed(&window, &error_msg);
        return Err(error_msg);
    }

    Ok(new_id)
}

#[tauri::command]
async fn assign_mod_to_folder(
    mod_path: String,
    folder_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<(), String> {
    let state = state.lock().unwrap();
    let game_path = &state.game_path;
    let source_path = PathBuf::from(&mod_path);

    if !source_path.exists() {
        let error_msg = "Mod file does not exist".to_string();
        toast_events::emit_move_failed(&window, &error_msg);
        return Err(error_msg);
    }

    let filename = source_path.file_name().ok_or("Invalid file name")?;

    let dest_path = if let Some(folder_name) = folder_id {
        // Move to folder
        let folder_path = game_path.join(&folder_name);
        if !folder_path.exists() {
            let error_msg = "Folder does not exist".to_string();
            toast_events::emit_move_failed(&window, &error_msg);
            return Err(error_msg);
        }
        folder_path.join(filename)
    } else {
        // Move back to root ~mods directory
        game_path.join(filename)
    };

    // Move the main file
    if let Err(e) = std::fs::rename(&source_path, &dest_path) {
        let error_msg = format!("Failed to move mod: {}", e);
        toast_events::emit_move_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Also move .utoc and .ucas files if they exist (IoStore files)
    let utoc_source = source_path.with_extension("utoc");
    let ucas_source = source_path.with_extension("ucas");

    if utoc_source.exists() {
        let utoc_dest = dest_path.with_extension("utoc");
        let _ = std::fs::rename(&utoc_source, &utoc_dest);
    }

    if ucas_source.exists() {
        let ucas_dest = dest_path.with_extension("ucas");
        let _ = std::fs::rename(&ucas_source, &ucas_dest);
    }

    Ok(())
}

#[tauri::command]
async fn add_custom_tag(
    mod_path: String,
    tag: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().unwrap();

    if !state.custom_tag_catalog.contains(&tag) {
        state.custom_tag_catalog.push(tag.clone());
        state.custom_tag_catalog.sort();
    }

    let key = get_mod_key(std::path::Path::new(&mod_path));
    let tags = state.mod_tags.entry(key).or_insert_with(Vec::new);
    if !tags.contains(&tag) {
        tags.push(tag);
        tags.sort();
    }

    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn add_tag_to_catalog(
    tag: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().unwrap();

    if !state.custom_tag_catalog.contains(&tag) {
        state.custom_tag_catalog.push(tag);
        state.custom_tag_catalog.sort();
    }

    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn delete_tag_from_all_mods(
    tag: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().unwrap();

    state.custom_tag_catalog.retain(|t| t != &tag);

    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn remove_custom_tag(
    mod_path: String,
    tag: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().unwrap();

    let key = get_mod_key(std::path::Path::new(&mod_path));
    if let Some(tags) = state.mod_tags.get_mut(&key) {
        tags.retain(|t| t != &tag);
        if tags.is_empty() {
            state.mod_tags.remove(&key);
        }
    }

    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_all_tags(state: State<'_, Arc<Mutex<AppState>>>) -> Result<Vec<String>, String> {
    let state = state.lock().unwrap();
    let mut tags_vec = state.custom_tag_catalog.clone();
    tags_vec.sort();
    Ok(tags_vec)
}

#[tauri::command]
async fn toggle_mod(
    mod_path: String,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let path = PathBuf::from(&mod_path);

    if !path.exists() {
        let error_msg = "Mod file does not exist".to_string();
        toast_events::emit_toggle_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Check current state
    let is_enabled = path.extension().and_then(|s| s.to_str()) == Some("pak");

    // Toggle by renaming
    let new_path = if is_enabled {
        path.with_extension("bak_repak")
    } else {
        path.with_extension("pak")
    };

    if let Err(e) = std::fs::rename(&path, &new_path) {
        let error_msg = format!("Failed to toggle mod: {}", e);
        toast_events::emit_toggle_failed(&window, &error_msg);
        return Err(error_msg);
    }

    // Invalidate cache for old path
    {
        let mut state_guard = state.lock().unwrap();
        state_guard.mod_details_cache.remove(&path);
    }

    Ok(!is_enabled)
}

#[tauri::command]
async fn extract_pak_to_destination(mod_path: String, dest_path: String) -> Result<(), String> {
    let pak_path = PathBuf::from(&mod_path);
    if !pak_path.exists() {
        return Err("Pak file not found".to_string());
    }

    let dest_dir = PathBuf::from(&dest_path);
    let mod_name = pak_path.file_stem().unwrap().to_string_lossy().to_string();
    let to_create = dest_dir.join(&mod_name);

    std::fs::create_dir_all(&to_create).map_err(|e| e.to_string())?;

    uasset_toolkit::extract_pak_all(
        pak_path.to_str().unwrap_or_default(),
        to_create.to_str().unwrap_or_default(),
        Some(crate::install_mod::AES_KEY_HEX),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Cleanup .ubulk files for textures that have inline data.
/// This is called after extraction to remove unnecessary .ubulk files
/// that were pulled from the base game but aren't needed because the
/// mod's textures have been patched to use inline data.
async fn cleanup_ubulk_for_inline_textures(output_dir: &PathBuf) {
    use uasset_toolkit::get_global_toolkit;
    use walkdir::WalkDir;

    // Find all .uasset files - UAssetTool will detect which are textures
    let uasset_files: Vec<String> = WalkDir::new(output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            if let Some(ext) = path.extension() {
                return ext.to_string_lossy().to_lowercase() == "uasset";
            }
            false
        })
        .map(|e| e.path().to_string_lossy().to_string())
        .collect();

    if uasset_files.is_empty() {
        return;
    }

    log::info!(
        "[Extraction] Checking {} uasset files for textures with inline data...",
        uasset_files.len()
    );

    // Use UAssetToolkit to check which are textures with inline data
    // The batch_has_inline_texture_data function internally checks asset type
    match get_global_toolkit() {
        Ok(toolkit) => {
            match toolkit.batch_has_inline_texture_data(&uasset_files, None) {
                Ok(inline_files) => {
                    log::info!(
                        "[Extraction] Found {} textures with inline data",
                        inline_files.len()
                    );

                    // Delete .ubulk files for textures with inline data
                    let mut deleted_count = 0;
                    for uasset_path in inline_files {
                        let ubulk_path = uasset_path.replace(".uasset", ".ubulk");
                        if std::path::Path::new(&ubulk_path).exists() {
                            if let Ok(_) = std::fs::remove_file(&ubulk_path) {
                                deleted_count += 1;
                                log::debug!(
                                    "[Extraction] Deleted unnecessary .ubulk: {}",
                                    ubulk_path
                                );
                            }
                        }
                    }

                    if deleted_count > 0 {
                        log::info!(
                            "[Extraction] Cleaned up {} unnecessary .ubulk files",
                            deleted_count
                        );
                    }
                }
                Err(e) => {
                    log::warn!("[Extraction] Failed to check inline texture data: {}", e);
                }
            }
        }
        Err(e) => {
            log::warn!("[Extraction] UAssetToolkit unavailable for cleanup: {}", e);
        }
    }
}

/// Extract assets from a mod file (PAK or IoStore) to a destination directory.
/// Automatically detects the mod type and uses the appropriate extraction method.
/// Handles disabled mods (.bak_repak extension) by treating them as PAK files.
///
/// # Arguments
/// * `mod_path` - Path to the mod file (.pak, .utoc, .bak_repak)
/// * `dest_path` - Destination directory for extracted files
///
/// # Returns
/// Number of files extracted
#[tauri::command]
async fn extract_mod_assets(
    mod_path: String,
    dest_path: String,
    window: Window,
    _state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<usize, String> {
    extract_mod_assets_inner(mod_path, dest_path, window).await
}

async fn extract_mod_assets_inner(
    mod_path: String,
    dest_path: String,
    window: Window,
) -> Result<usize, String> {
    let mut path = PathBuf::from(&mod_path);
    if !path.exists() {
        return Err(format!("File not found: {}", mod_path));
    }

    // Check if this is a PAK file with a corresponding .utoc file (IoStore mod)
    // IoStore mods have both .pak (with just chunknames) and .utoc/.ucas (actual data)
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if extension == "pak" || extension == "bak_repak" {
        // Check if there's a .utoc file with the same name - if so, use IoStore extraction
        // Handle .bak_repak (disabled mod) by stripping .bak_repak then .pak to get base name
        let base_path = if extension == "bak_repak" {
            let name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let base_name = name.trim_end_matches(".bak_repak").trim_end_matches(".pak");
            path.parent()
                .map(|p| p.join(base_name))
                .unwrap_or_else(|| PathBuf::from(base_name))
        } else {
            path.with_extension("")
        };
        let utoc_path = base_path.with_extension("utoc");
        if utoc_path.exists() {
            log::info!("Detected IoStore mod (has .utoc alongside .pak/.bak_repak), using IoStore extraction");
            path = utoc_path;
        }
    }

    let dest_dir = PathBuf::from(&dest_path);

    // Get mod name - handle .bak_repak extension specially
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "extracted".to_string());

    // Strip .bak_repak or other extensions to get clean mod name
    let mod_name = if file_name.ends_with(".bak_repak") {
        // Remove .bak_repak and then .pak to get the base name
        file_name
            .trim_end_matches(".bak_repak")
            .trim_end_matches(".pak")
            .to_string()
    } else {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string())
    };

    let output_dir = dest_dir.join(&mod_name);

    // Create output directory
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    // Re-get extension after potential path change
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "utoc" => {
            // IoStore extraction using UAssetTool
            log::info!(
                "Starting IoStore extraction from {:?} to {:?}",
                path,
                output_dir
            );

            // Emit start progress
            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: 0,
                    total_files: 0,
                    percentage: 0.0,
                    status: "extracting".to_string(),
                },
            );

            let file_count = uasset_toolkit::extract_iostore(
                &path.to_string_lossy(),
                &output_dir.to_string_lossy(),
                None, // Use default AES key
            )
            .map_err(|e| {
                log::error!("IoStore extraction failed: {}", e);
                let _ = window.emit(
                    "extraction_progress",
                    ExtractionProgress {
                        current_file: mod_name.clone(),
                        files_extracted: 0,
                        total_files: 0,
                        percentage: 0.0,
                        status: "error".to_string(),
                    },
                );
                format!("Failed to extract IoStore: {}", e)
            })?;

            log::info!(
                "Extracted {} files from IoStore to {:?}",
                file_count,
                output_dir
            );

            // Post-extraction cleanup: Remove .ubulk files for textures with inline data
            cleanup_ubulk_for_inline_textures(&output_dir).await;

            // Emit completion progress
            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: file_count,
                    total_files: file_count,
                    percentage: 100.0,
                    status: "complete".to_string(),
                },
            );

            Ok(file_count)
        }
        "pak" => {
            // PAK extraction
            let file_list = uasset_toolkit::list_pak_files(
                path.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            )
            .unwrap_or_default();
            let file_count = file_list.len();

            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: 0,
                    total_files: file_count,
                    percentage: 0.0,
                    status: "extracting".to_string(),
                },
            );

            uasset_toolkit::extract_pak_all(
                path.to_str().unwrap_or_default(),
                output_dir.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            )
            .map_err(|e| {
                let _ = window.emit(
                    "extraction_progress",
                    ExtractionProgress {
                        current_file: mod_name.clone(),
                        files_extracted: 0,
                        total_files: file_count,
                        percentage: 0.0,
                        status: "error".to_string(),
                    },
                );
                e.to_string()
            })?;

            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: file_count,
                    total_files: file_count,
                    percentage: 100.0,
                    status: "complete".to_string(),
                },
            );

            log::info!(
                "Extracted {} files from PAK to {:?}",
                file_count,
                output_dir
            );
            Ok(file_count)
        }
        "ucas" => {
            // User selected .ucas, find the corresponding .utoc
            let utoc_path = path.with_extension("utoc");
            if !utoc_path.exists() {
                return Err(format!("Cannot find .utoc file for: {}", mod_path));
            }

            // Recursively call with the .utoc path
            let utoc_str = utoc_path.to_string_lossy().to_string();
            Box::pin(extract_mod_assets_inner(utoc_str, dest_path, window)).await
        }
        "bak_repak" => {
            // Disabled PAK file - extract it as a regular PAK
            let file_list = uasset_toolkit::list_pak_files(
                path.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            )
            .unwrap_or_default();
            let file_count = file_list.len();

            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: 0,
                    total_files: file_count,
                    percentage: 0.0,
                    status: "extracting".to_string(),
                },
            );

            uasset_toolkit::extract_pak_all(
                path.to_str().unwrap_or_default(),
                output_dir.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            )
            .map_err(|e| {
                let _ = window.emit(
                    "extraction_progress",
                    ExtractionProgress {
                        current_file: mod_name.clone(),
                        files_extracted: 0,
                        total_files: file_count,
                        percentage: 0.0,
                        status: "error".to_string(),
                    },
                );
                e.to_string()
            })?;

            let _ = window.emit(
                "extraction_progress",
                ExtractionProgress {
                    current_file: mod_name.clone(),
                    files_extracted: file_count,
                    total_files: file_count,
                    percentage: 100.0,
                    status: "complete".to_string(),
                },
            );

            log::info!(
                "Extracted {} files from disabled PAK to {:?}",
                file_count,
                output_dir
            );
            Ok(file_count)
        }
        _ => Err(format!(
            "Unsupported file type: .{}. Supported: .pak, .utoc, .ucas, .bak_repak",
            extension
        )),
    }
}

#[tauri::command]
async fn check_game_running() -> Result<bool, String> {
    Ok(is_game_process_running())
}

/// Reliable game process detection using multiple methods
/// Uses exe() path as primary method, falls back to name() matching
fn is_game_process_running() -> bool {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};

    // Create system with full process info including exe path
    // Use everything() to ensure exe path is fetched
    let s = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );

    let game_exe_name = "marvel-win64-shipping.exe";

    for (_pid, process) in s.processes() {
        // Primary method: Check exe() path (most reliable on Windows)
        if let Some(exe_path) = process.exe() {
            if let Some(file_name) = exe_path.file_name() {
                if file_name.to_string_lossy().to_lowercase() == game_exe_name {
                    return true;
                }
            }
        }

        #[cfg(target_os = "linux")]
        for arg in process.cmd() {
            let arg = arg.to_str().unwrap_or("").to_string();
            if arg.to_lowercase().contains(game_exe_name) {
                return true;
            }
        }
        // Fallback: Check process name() directly
        let process_name = process.name().to_string_lossy().to_lowercase();
        if process_name == game_exe_name {
            return true;
        }
    }

    false
}

/// Launch Marvel Rivals via Steam, temporarily skipping the launcher
///
/// This function:
/// 1. Backs up the current launch_record value
/// 2. DELETES the launch_record file
/// 3. RECREATES it with "0" to skip the launcher
/// 4. Launches the game via Steam protocol
/// 5. Restores the original launch_record after game starts
///
/// This ensures the game launches without the launcher when using our app,
/// but preserves the user's Steam launch settings for manual launches
#[tauri::command]
async fn launch_game(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), String> {
    use std::process::Command;

    // Get game path (this is the ~mods folder inside Paks)
    let (mods_path, launcher_type) = {
        let state = state.lock().unwrap();
        (state.game_path.clone(), state.launcher_type.clone())
    };

    // Go up 5 levels to get the actual game root
    // ~mods -> Paks -> Content -> Marvel -> MarvelGame -> MarvelRivals (game root)
    let game_root = mods_path
        .parent() // Paks
        .and_then(|p| p.parent()) // Content
        .and_then(|p| p.parent()) // Marvel
        .and_then(|p| p.parent()) // MarvelGame
        .and_then(|p| p.parent()) // MarvelRivals (game root)
        .ok_or_else(|| "Could not determine game root directory".to_string())?;

    // Path to launch_record file (in the game root, next to MarvelRivals_Launcher.exe)
    let launch_record_path = game_root.join("launch_record");

    // Backup original value
    let original_value = match std::fs::read_to_string(&launch_record_path) {
        Ok(content) => {
            info!("Backed up launch_record value: {}", content.trim());
            Some(content)
        }
        Err(e) => {
            warn!("Could not read launch_record (file may not exist): {}", e);
            None
        }
    };

    // DELETE the launch_record file
    if launch_record_path.exists() {
        if let Err(e) = std::fs::remove_file(&launch_record_path) {
            error!("Failed to delete launch_record: {}", e);
            return Err(format!("Failed to delete launch_record: {}", e));
        }
        info!("Deleted launch_record file");
    }

    // RECREATE it with "0" to skip launcher
    if let Err(e) = std::fs::write(&launch_record_path, "0") {
        error!("Failed to recreate launch_record: {}", e);
        return Err(format!("Failed to recreate launch_record: {}", e));
    }
    info!("Recreated launch_record with value 0 (skip launcher)");

    // Launch the game via selected launcher (running with parent admin rights)
    #[cfg(target_os = "windows")]
    let launch_result = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        if launcher_type == "epic" {
            let launcher_exe = game_root.join("MarvelRivals_Launcher.exe");
            Command::new("cmd")
                .arg("/C")
                .arg("start")
                .arg("")
                .arg(&launcher_exe)
                .current_dir(&game_root)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        } else {
            Command::new("cmd")
                .arg("/C")
                .arg("start")
                .arg("")
                .arg("steam://run/2767030")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        }
    };

    #[cfg(target_os = "linux")]
    let launch_result = {
        if launcher_type == "epic" {
            let launcher_exe = game_root.join("MarvelRivals_Launcher.exe");
            Command::new("wine")
                .arg(&launcher_exe)
                .current_dir(&game_root)
                .spawn()
        } else {
            Command::new("xdg-open").arg("steam://run/2767030").spawn()
        }
    };

    // Check launch result
    match launch_result {
        Ok(_) => {
            info!("Successfully launched Marvel Rivals via Steam");

            // Spawn a background task to restore the launch_record after the game starts
            let launch_record_path_clone = launch_record_path.clone();
            std::thread::spawn(move || {
                use sysinfo::{ProcessRefreshKind, RefreshKind, System};

                // Wait for the game process to start (up to 30 seconds)
                let mut waited = 0;
                let mut game_started = false;

                while waited < 30000 {
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                    waited += 1000;

                    // Check if game process is running
                    let process_refresh = {
                        // On Linux, we need full process info because cmdline is required for detection.
                        // On other platforms, minimal process info is sufficient.
                        #[cfg(target_os = "linux")]
                        {
                            ProcessRefreshKind::everything()
                        }

                        #[cfg(not(target_os = "linux"))]
                        {
                            ProcessRefreshKind::new()
                        }
                    };

                    let s = System::new_with_specifics(
                        RefreshKind::new().with_processes(process_refresh),
                    );

                    let mut found = false;
                    for (_pid, process) in s.processes() {
                        // CMD parsing for linux
                        #[cfg(target_os = "linux")]
                        for arg in process.cmd() {
                            let arg = arg.to_str().unwrap_or("").to_string();
                            if arg.to_lowercase().contains("marvel-win64-shipping.exe") {
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                found = true;
                                game_started = true;
                                break;
                            }
                        }

                        // for macos and windows
                        #[cfg(not(target_os = "linux"))]
                        {
                            let process_name = process.name().to_string_lossy().to_lowercase();
                            if process_name == "marvel-win64-shipping.exe" {
                                info!("Game process detected, waiting 2 more seconds before restoring launch_record");
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                found = true;
                                game_started = true;
                                break;
                            }
                        }
                    }

                    if found {
                        break;
                    }
                }

                if !game_started {
                    warn!("Timeout waiting for game to start, restoring launch_record anyway");
                }

                // DELETE and RECREATE with original value
                if let Some(original) = original_value {
                    if launch_record_path_clone.exists() {
                        if let Err(e) = std::fs::remove_file(&launch_record_path_clone) {
                            warn!("Failed to delete launch_record for restoration: {}", e);
                            return;
                        }
                    }

                    if let Err(e) = std::fs::write(&launch_record_path_clone, original.trim()) {
                        warn!(
                            "Failed to recreate launch_record with original value: {}",
                            e
                        );
                    } else {
                        info!(
                            "Restored launch_record to original value (game_started: {})",
                            game_started
                        );
                    }
                }
            });

            Ok(())
        }
        Err(e) => {
            error!("Failed to launch game: {}", e);
            Err(format!(
                "Failed to launch game. Please ensure Steam is installed. Error: {}",
                e
            ))
        }
    }
}

/// Toggle the skip launcher patch (manual control)
/// Returns true if skip launcher is now enabled (0), false if disabled (6)
#[tauri::command]
async fn skip_launcher_patch(state: State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    // Get game path (this is the ~mods folder inside Paks)
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    // Go up 5 levels to get the actual game root
    // ~mods -> Paks -> Content -> Marvel -> MarvelGame -> MarvelRivals (game root)
    let game_root = mods_path
        .parent() // Paks
        .and_then(|p| p.parent()) // Content
        .and_then(|p| p.parent()) // Marvel
        .and_then(|p| p.parent()) // MarvelGame
        .and_then(|p| p.parent()) // MarvelRivals (game root)
        .ok_or_else(|| "Could not determine game root directory".to_string())?;

    // Path to launch_record file
    let launch_record_path = game_root.join("launch_record");

    info!("Mods path: {:?}", mods_path);
    info!("Game root: {:?}", game_root);
    info!("Launch record path: {:?}", launch_record_path);

    // Read current value
    let current_value = match std::fs::read_to_string(&launch_record_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => {
            // If file doesn't exist, assume default (6)
            "6".to_string()
        }
    };

    // Determine new value (toggle between 0 and 6)
    let new_value = if current_value == "0" {
        "6" // Disable skip launcher (show launcher)
    } else {
        "0" // Enable skip launcher
    };

    // Delete and recreate the file with new value
    if launch_record_path.exists() {
        std::fs::remove_file(&launch_record_path)
            .map_err(|e| format!("Failed to delete launch_record: {}", e))?;
    }

    std::fs::write(&launch_record_path, &new_value)
        .map_err(|e| format!("Failed to write launch_record: {}", e))?;

    let skip_enabled = new_value == "0";
    info!(
        "Skip launcher patch toggled: {} (value: {})",
        skip_enabled, new_value
    );

    Ok(skip_enabled)
}

/// Check if skip launcher patch is currently enabled
#[tauri::command]
async fn get_skip_launcher_status(state: State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    // Get game path (this is the ~mods folder inside Paks)
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    // Go up 5 levels to get the actual game root
    // ~mods -> Paks -> Content -> Marvel -> MarvelGame -> MarvelRivals (game root)
    let game_root = mods_path
        .parent() // Paks
        .and_then(|p| p.parent()) // Content
        .and_then(|p| p.parent()) // Marvel
        .and_then(|p| p.parent()) // MarvelGame
        .and_then(|p| p.parent()) // MarvelRivals (game root)
        .ok_or_else(|| "Could not determine game root directory".to_string())?;

    // Path to launch_record file
    let launch_record_path = game_root.join("launch_record");

    // Read current value
    let current_value = match std::fs::read_to_string(&launch_record_path) {
        Ok(content) => content.trim().to_string(),
        Err(_) => "6".to_string(), // Default if file doesn't exist
    };

    Ok(current_value == "0")
}

// ============================================================================
// SIG BYPASSER TOGGLE
// ============================================================================

/// Check the status of the Sig Bypasser
/// Returns: "Enabled", "Disabled", or "NotInstalled"
#[tauri::command]
async fn get_sig_bypasser_status(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    let binaries_path = mods_path
        .parent() // Paks
        .and_then(|p| p.parent()) // Content
        .and_then(|p| p.parent()) // Marvel
        .map(|p| p.join("Binaries").join("Win64"))
        .ok_or_else(|| "Could not determine Binaries directory".to_string())?;

    let dsound_path = binaries_path.join("dsound.dll");
    let dsound_disabled_path = binaries_path.join("dsound.dll.disabled");

    if dsound_path.exists() {
        Ok("Enabled".to_string())
    } else if dsound_disabled_path.exists() {
        Ok("Disabled".to_string())
    } else {
        Ok("NotInstalled".to_string())
    }
}

/// Toggle the Sig Bypasser
/// Returns the new status: "Enabled", "Disabled", or "NotInstalled"
#[tauri::command]
async fn toggle_sig_bypasser(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    let binaries_path = mods_path
        .parent() // Paks
        .and_then(|p| p.parent()) // Content
        .and_then(|p| p.parent()) // Marvel
        .map(|p| p.join("Binaries").join("Win64"))
        .ok_or_else(|| "Could not determine Binaries directory".to_string())?;

    let dsound_path = binaries_path.join("dsound.dll");
    let dsound_disabled_path = binaries_path.join("dsound.dll.disabled");

    if dsound_path.exists() {
        // Currently enabled, disable it
        match std::fs::rename(&dsound_path, &dsound_disabled_path) {
            Ok(_) => {
                info!("Disabled Sig Bypasser (renamed to dsound.dll.disabled)");
                Ok("Disabled".to_string())
            }
            Err(e) => Err(format!("Failed to disable Sig Bypasser: {}", e)),
        }
    } else if dsound_disabled_path.exists() {
        // Currently disabled, enable it
        match std::fs::rename(&dsound_disabled_path, &dsound_path) {
            Ok(_) => {
                info!("Enabled Sig Bypasser (renamed back to dsound.dll)");
                Ok("Enabled".to_string())
            }
            Err(e) => Err(format!("Failed to enable Sig Bypasser: {}", e)),
        }
    } else {
        // Not installed
        Err("Sig Bypasser (dsound.dll) is not installed in the Binaries directory.".to_string())
    }
}

// ============================================================================
// BUNDLED LOD DISABLER MOD
// ============================================================================

/// The bundled LOD Disabler mod bytes (embedded at compile time)
/// This mod must stay as legacy PAK and NOT be converted to IoStore
///
/// To bundle the mod:
/// 1. Download from https://www.nexusmods.com/marvelrivals/mods/5303
/// 2. Place the .pak file at: repak-gui/src/bundled_mods/SK_LODs_Disabler_9999999_P.pak
/// 3. Rebuild the application with --features bundled_lod_mod
#[cfg(feature = "bundled_lod_mod")]
const BUNDLED_LOD_DISABLER_PAK: &[u8] =
    include_bytes!("bundled_mods/SK_LODs_Disabler_9999999_P.pak");

/// Folder name for the bundled LOD mod
const LOD_DISABLER_FOLDER: &str = "_LOD-Disabler (Built-in)";

/// Filename for the bundled LOD mod
const LOD_DISABLER_FILENAME: &str = "SK_LODs_Disabler_9999999_P.pak";

/// Get the bundled LOD mod bytes if available
fn get_bundled_lod_mod_bytes() -> Option<&'static [u8]> {
    #[cfg(feature = "bundled_lod_mod")]
    {
        Some(BUNDLED_LOD_DISABLER_PAK)
    }
    #[cfg(not(feature = "bundled_lod_mod"))]
    {
        None
    }
}

/// Deploy the bundled LOD Disabler mod to the game's mods folder
/// Creates a special folder and copies the pak file there
/// Returns Ok(true) if deployed, Ok(false) if already exists or not bundled, Err on failure
fn deploy_bundled_lod_mod(mods_path: &Path) -> Result<bool, String> {
    // Check if bundled mod is available
    let pak_bytes = match get_bundled_lod_mod_bytes() {
        Some(bytes) => bytes,
        None => {
            info!("Bundled LOD Disabler mod not included in this build");
            return Ok(false);
        }
    };

    let lod_folder = mods_path.join(LOD_DISABLER_FOLDER);
    let pak_path = lod_folder.join(LOD_DISABLER_FILENAME);

    // Check if already deployed
    if pak_path.exists() {
        info!(
            "Bundled LOD Disabler mod already deployed at: {}",
            pak_path.display()
        );
        return Ok(false);
    }

    // Create the folder
    std::fs::create_dir_all(&lod_folder)
        .map_err(|e| format!("Failed to create LOD Disabler folder: {}", e))?;

    // Write the bundled pak file
    std::fs::write(&pak_path, pak_bytes)
        .map_err(|e| format!("Failed to write LOD Disabler pak: {}", e))?;

    info!(
        "Deployed bundled LOD Disabler mod to: {}",
        pak_path.display()
    );
    Ok(true)
}

/// Check if the bundled LOD Disabler mod is deployed
#[tauri::command]
async fn check_lod_disabler_deployed(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, String> {
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    if !mods_path.exists() {
        return Ok(false);
    }

    let pak_path = mods_path
        .join(LOD_DISABLER_FOLDER)
        .join(LOD_DISABLER_FILENAME);
    Ok(pak_path.exists())
}

/// Get the path to the bundled LOD Disabler mod
#[tauri::command]
async fn get_lod_disabler_path(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, String> {
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    let pak_path = mods_path
        .join(LOD_DISABLER_FOLDER)
        .join(LOD_DISABLER_FILENAME);
    Ok(pak_path.to_string_lossy().to_string())
}

/// Manually deploy the bundled LOD Disabler mod
#[tauri::command]
async fn deploy_lod_disabler(state: State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    let mods_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    if !mods_path.exists() {
        return Err("Game path does not exist. Please set a valid mods folder first.".to_string());
    }

    deploy_bundled_lod_mod(&mods_path)
}

/// Result of recompression operation
#[derive(Clone, Serialize, Deserialize)]
struct RecompressResult {
    total_scanned: usize,
    already_oodle: usize,
    recompressed: usize,
    failed: usize,
    skipped_iostore: usize,
    details: Vec<RecompressDetail>,
}

#[derive(Clone, Serialize, Deserialize)]
struct RecompressDetail {
    mod_name: String,
    status: String, // "already_oodle", "recompressed", "failed", "skipped_iostore"
    original_size: u64,
    new_size: Option<u64>,
    error: Option<String>,
}

/// Scan all mods and recompress any that aren't using Oodle compression
#[tauri::command]
async fn recompress_mods(
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<RecompressResult, String> {
    let game_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    if !game_path.exists() {
        return Err("Game path does not exist".to_string());
    }

    info!("Starting recompression scan in: {}", game_path.display());

    let mut result = RecompressResult {
        total_scanned: 0,
        already_oodle: 0,
        recompressed: 0,
        failed: 0,
        skipped_iostore: 0,
        details: Vec::new(),
    };

    // Collect all .pak files
    let mut pak_files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(&game_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("pak") {
            pak_files.push(path.to_path_buf());
        }
    }

    result.total_scanned = pak_files.len();
    info!("Found {} PAK files to scan", pak_files.len());

    // Emit initial progress
    let _ = window.emit(
        "recompress_progress",
        serde_json::json!({
            "current": 0,
            "total": pak_files.len(),
            "status": "Scanning..."
        }),
    );

    for (idx, pak_path) in pak_files.iter().enumerate() {
        let mod_name = pak_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // Emit progress
        let _ = window.emit(
            "recompress_progress",
            serde_json::json!({
                "current": idx + 1,
                "total": pak_files.len(),
                "status": format!("Checking: {}", mod_name)
            }),
        );

        // Check if this is an IoStore mod (has .utoc/.ucas files)
        let utoc_path = pak_path.with_extension("utoc");
        let ucas_path = pak_path.with_extension("ucas");

        if utoc_path.exists() && ucas_path.exists() {
            // IoStore mod - check if it needs recompression
            let ucas_size = std::fs::metadata(&ucas_path).map(|m| m.len()).unwrap_or(0);

            // Check if IoStore is already compressed
            let is_compressed =
                match uasset_toolkit::is_iostore_compressed(&utoc_path.to_string_lossy()) {
                    Ok(compressed) => compressed,
                    Err(e) => {
                        warn!(
                            "Failed to check IoStore compression for {}: {}",
                            mod_name, e
                        );
                        result.failed += 1;
                        result.details.push(RecompressDetail {
                            mod_name,
                            status: "failed".to_string(),
                            original_size: ucas_size,
                            new_size: None,
                            error: Some(format!("Failed to check compression: {}", e)),
                        });
                        continue;
                    }
                };

            if is_compressed {
                // Already compressed
                info!("IoStore already compressed: {}", mod_name);
                result.already_oodle += 1;
                result.details.push(RecompressDetail {
                    mod_name,
                    status: "already_oodle".to_string(),
                    original_size: ucas_size,
                    new_size: None,
                    error: None,
                });
            } else {
                // Need to recompress IoStore
                info!("Recompressing IoStore: {}", mod_name);

                let _ = window.emit(
                    "recompress_progress",
                    serde_json::json!({
                        "current": idx + 1,
                        "total": pak_files.len(),
                        "status": format!("Recompressing IoStore: {}", mod_name)
                    }),
                );

                match uasset_toolkit::recompress_iostore(&utoc_path.to_string_lossy()) {
                    Ok(_) => {
                        let new_ucas_size =
                            std::fs::metadata(&ucas_path).map(|m| m.len()).unwrap_or(0);
                        info!(
                            "Successfully recompressed IoStore: {} ({} -> {} bytes)",
                            mod_name, ucas_size, new_ucas_size
                        );
                        result.recompressed += 1;
                        result.details.push(RecompressDetail {
                            mod_name,
                            status: "recompressed".to_string(),
                            original_size: ucas_size,
                            new_size: Some(new_ucas_size),
                            error: None,
                        });
                    }
                    Err(e) => {
                        error!("Failed to recompress IoStore {}: {}", mod_name, e);
                        result.failed += 1;
                        result.details.push(RecompressDetail {
                            mod_name,
                            status: "failed".to_string(),
                            original_size: ucas_size,
                            new_size: None,
                            error: Some(format!("Recompression failed: {}", e)),
                        });
                    }
                }
            }
            continue;
        }

        // Try to read the PAK file
        let file = match File::open(pak_path) {
            Ok(f) => f,
            Err(e) => {
                error!("Failed to open PAK file {}: {}", pak_path.display(), e);
                result.failed += 1;
                result.details.push(RecompressDetail {
                    mod_name,
                    status: "failed".to_string(),
                    original_size: 0,
                    new_size: None,
                    error: Some(format!("Failed to open: {}", e)),
                });
                continue;
            }
        };

        let original_size = std::fs::metadata(pak_path).map(|m| m.len()).unwrap_or(0);
        drop(file);

        // For PAK files without IoStore - use uasset_toolkit to extract and recreate with Oodle
        // First check if it already seems Oodle (heuristic: file size relative to content)
        info!("Recompressing PAK: {}", mod_name);

        let _ = window.emit(
            "recompress_progress",
            serde_json::json!({
                "current": idx + 1,
                "total": pak_files.len(),
                "status": format!("Recompressing: {}", mod_name)
            }),
        );

        match recompress_pak_file(pak_path) {
            Ok(new_size) => {
                info!(
                    "Successfully recompressed: {} ({} -> {} bytes)",
                    mod_name, original_size, new_size
                );
                result.recompressed += 1;
                result.details.push(RecompressDetail {
                    mod_name,
                    status: "recompressed".to_string(),
                    original_size,
                    new_size: Some(new_size),
                    error: None,
                });
            }
            Err(e) => {
                error!("Failed to recompress {}: {}", mod_name, e);
                result.failed += 1;
                result.details.push(RecompressDetail {
                    mod_name,
                    status: "failed".to_string(),
                    original_size,
                    new_size: None,
                    error: Some(e),
                });
            }
        }
    }

    // Emit completion
    let _ = window.emit(
        "recompress_progress",
        serde_json::json!({
            "current": pak_files.len(),
            "total": pak_files.len(),
            "status": "Complete"
        }),
    );

    info!(
        "Recompression complete: {} scanned, {} already Oodle, {} recompressed, {} failed",
        result.total_scanned, result.already_oodle, result.recompressed, result.failed
    );

    Ok(result)
}

/// Recompress a single PAK file by extracting to temp dir and repacking via UAssetTool
fn recompress_pak_file(pak_path: &Path) -> Result<u64, String> {
    use tempfile::tempdir;

    let temp_dir = tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let temp_path = temp_dir.path();

    // Extract PAK to temp dir
    uasset_toolkit::extract_pak_all(
        pak_path.to_str().unwrap_or_default(),
        temp_path.to_str().unwrap_or_default(),
        Some(crate::install_mod::AES_KEY_HEX),
    )
    .map_err(|e| format!("Failed to extract PAK for recompression: {}", e))?;

    // Collect extracted files
    let mut file_entries: Vec<(String, String)> = Vec::new();
    for entry in WalkDir::new(temp_path).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() {
            if let (Some(rel), Some(abs)) = (
                p.strip_prefix(temp_path)
                    .ok()
                    .and_then(|r| r.to_str())
                    .map(|s| s.replace('\\', "/")),
                p.to_str().map(|s| s.to_string()),
            ) {
                file_entries.push((rel, abs));
            }
        }
    }

    // Repack with UAssetTool (uses Oodle by default)
    uasset_toolkit::create_pak(
        pak_path.to_str().unwrap_or_default(),
        file_entries,
        Some("../../../"),
        None,
        Some(crate::install_mod::AES_KEY_HEX),
    )
    .map_err(|e| format!("Failed to repack: {}", e))?;

    let new_size = std::fs::metadata(pak_path).map(|m| m.len()).unwrap_or(0);
    Ok(new_size)
}

#[tauri::command]
async fn get_app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Check for application updates and emit update_available event when found
#[tauri::command]
async fn check_for_updates(window: Window) -> Result<Option<UpdateInfo>, String> {
    let client = reqwest::Client::new();
    let url = "https://api.github.com/repos/XzantGaming/Repak-X/releases/latest";

    let res = client
        .get(url)
        .header("User-Agent", "RepakX")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !res.status().is_success() {
        return Ok(None);
    }

    let json: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;

    let tag_name = json["tag_name"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");

    if let (Ok(remote_ver), Ok(current_ver)) = (
        semver::Version::parse(tag_name),
        semver::Version::parse(current),
    ) {
        if remote_ver > current_ver {
            let url = json["html_url"].as_str().unwrap_or("").to_string();
            let assets = json["assets"].as_array();
            let changelog = json["body"].as_str().map(|s| s.to_string());

            let mut asset_url = None;
            let mut asset_name = None;

            // Find the appropriate asset for the current platform using RUNTIME detection
            if let Some(assets) = assets {
                // Runtime OS detection - works correctly even when cross-compiled
                let platform_pattern = if cfg!(target_os = "windows") {
                    "Windows"
                } else if cfg!(target_os = "linux") {
                    "Linux"
                } else if cfg!(target_os = "macos") {
                    "macOS"
                } else {
                    ""
                };

                // First, try to find a platform-specific asset
                if let Some(asset) = assets.iter().find(|a| {
                    let name = a["name"].as_str().unwrap_or("");
                    name.contains(platform_pattern)
                        && (name.ends_with(".zip")
                            || name.ends_with(".tar.gz")
                            || name.ends_with(".exe")
                            || name.ends_with(".msi"))
                }) {
                    asset_url = asset["browser_download_url"]
                        .as_str()
                        .map(|s| s.to_string());
                    asset_name = asset["name"].as_str().map(|s| s.to_string());
                }

                // Fallback: if no platform-specific asset found, try generic patterns based on OS
                if asset_url.is_none() {
                    if cfg!(target_os = "windows") {
                        if let Some(asset) = assets.iter().find(|a| {
                            let name = a["name"].as_str().unwrap_or("");
                            name.ends_with(".zip")
                                || name.ends_with(".exe")
                                || name.ends_with(".msi")
                        }) {
                            asset_url = asset["browser_download_url"]
                                .as_str()
                                .map(|s| s.to_string());
                            asset_name = asset["name"].as_str().map(|s| s.to_string());
                        }
                    } else if cfg!(target_os = "linux") {
                        if let Some(asset) = assets.iter().find(|a| {
                            let name = a["name"].as_str().unwrap_or("");
                            name.ends_with(".tar.gz")
                                || name.ends_with(".AppImage")
                                || name.ends_with(".deb")
                        }) {
                            asset_url = asset["browser_download_url"]
                                .as_str()
                                .map(|s| s.to_string());
                            asset_name = asset["name"].as_str().map(|s| s.to_string());
                        }
                    } else if cfg!(target_os = "macos") {
                        if let Some(asset) = assets.iter().find(|a| {
                            let name = a["name"].as_str().unwrap_or("");
                            name.ends_with(".zip") || name.ends_with(".dmg")
                        }) {
                            asset_url = asset["browser_download_url"]
                                .as_str()
                                .map(|s| s.to_string());
                            asset_name = asset["name"].as_str().map(|s| s.to_string());
                        }
                    }
                }
            }

            let update_info = UpdateInfo {
                latest: tag_name.to_string(),
                url,
                asset_url,
                asset_name,
                changelog,
            };

            // Emit update_available event
            let _ = window.emit("update_available", &update_info);
            info!("Emitted update_available event for version {}", tag_name);

            return Ok(Some(update_info));
        }
    }

    Ok(None)
}

#[derive(Serialize, Deserialize, Clone)]
struct UpdateInfo {
    latest: String,
    url: String,
    asset_url: Option<String>,
    asset_name: Option<String>,
    changelog: Option<String>,
}

/// Progress information for update download
#[derive(Clone, Serialize, Deserialize)]
struct UpdateDownloadProgress {
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    percentage: f32,
    status: String, // "downloading", "extracting", "ready", "error"
}

/// Progress information for asset extraction
#[derive(Clone, Serialize, Deserialize)]
struct ExtractionProgress {
    current_file: String,
    files_extracted: usize,
    total_files: usize,
    percentage: f32,
    status: String, // "extracting", "complete", "error"
}

/// Download an update from the given URL
/// Returns the path to the downloaded file
#[tauri::command]
async fn download_update(
    asset_url: String,
    asset_name: String,
    window: Window,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    info!("Starting update download from: {}", asset_url);

    // Create temp directory for the update
    let temp_dir = std::env::temp_dir().join("repakx_update");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    let download_path = temp_dir.join(&asset_name);

    // Download the file with progress reporting and retry logic
    let client = reqwest::Client::new();
    let mut response = None;
    let max_retries = 3;
    for attempt in 1..=max_retries {
        let res = client
            .get(&asset_url)
            .header("User-Agent", "RepakX")
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => {
                response = Some(r);
                break;
            }
            Ok(r) => {
                let status = r.status();
                if attempt < max_retries
                    && (status.as_u16() == 502 || status.as_u16() == 503 || status.as_u16() == 429)
                {
                    info!(
                        "Download attempt {}/{} failed with status {}, retrying...",
                        attempt, max_retries, status
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2 * attempt as u64)).await;
                } else {
                    return Err(format!("Download failed with status: {}", status));
                }
            }
            Err(e) => {
                if attempt < max_retries {
                    info!(
                        "Download attempt {}/{} failed: {}, retrying...",
                        attempt, max_retries, e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2 * attempt as u64)).await;
                } else {
                    return Err(format!("Download request failed: {}", e));
                }
            }
        }
    }
    let response = response.ok_or("Download failed after all retries")?;

    let total_size = response.content_length();
    let mut downloaded: u64 = 0;
    let mut last_emitted_pct: i32 = -1;

    // Create file and stream the download
    let mut file = tokio::fs::File::create(&download_path)
        .await
        .map_err(|e| format!("Failed to create download file: {}", e))?;

    // Read in small fixed-size pieces (64 KB) for smooth progress updates
    let mut body = response;

    loop {
        let n = body
            .chunk()
            .await
            .map_err(|e| format!("Download stream error: {}", e))?;
        let chunk = match n {
            Some(c) => c,
            None => break,
        };

        // Write the network chunk in 64 KB slices to get granular progress
        let mut offset = 0;
        while offset < chunk.len() {
            let end = (offset + 65_536).min(chunk.len());
            file.write_all(&chunk[offset..end])
                .await
                .map_err(|e| format!("Failed to write chunk: {}", e))?;

            downloaded += (end - offset) as u64;
            offset = end;

            let percentage = if let Some(total) = total_size {
                (downloaded as f32 / total as f32) * 100.0
            } else {
                -1.0
            };

            // Only emit when percentage changes by >= 1% to avoid flooding
            let pct_int = percentage as i32;
            if pct_int > last_emitted_pct {
                last_emitted_pct = pct_int;
                let progress = UpdateDownloadProgress {
                    downloaded_bytes: downloaded,
                    total_bytes: total_size,
                    percentage,
                    status: "downloading".to_string(),
                };
                let _ = window.emit("update_download_progress", &progress);
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush file: {}", e))?;
    drop(file);

    info!("Update downloaded to: {:?}", download_path);

    // Emit completion progress
    let progress = UpdateDownloadProgress {
        downloaded_bytes: downloaded,
        total_bytes: total_size,
        percentage: 100.0,
        status: "ready".to_string(),
    };
    let _ = window.emit("update_download_progress", &progress);

    // Emit update_downloaded event with the downloaded file path
    let download_result = serde_json::json!({
        "path": download_path.to_string_lossy().to_string(),
        "size": downloaded,
    });
    let _ = window.emit("update_downloaded", &download_result);
    info!("Emitted update_downloaded event");

    Ok(download_path.to_string_lossy().to_string())
}

/// Apply a downloaded update
/// This creates an updater script and schedules it to run after the app closes
#[tauri::command]
async fn apply_update(downloaded_path: String, window: Window) -> Result<(), String> {
    info!("Applying update from: {}", downloaded_path);

    let download_path = PathBuf::from(&downloaded_path);
    if !download_path.exists() {
        return Err("Downloaded update file not found".to_string());
    }

    // Get the current exe path and directory
    let exe_path =
        std::env::current_exe().map_err(|e| format!("Failed to get current exe path: {}", e))?;
    let app_dir = exe_path.parent().ok_or("Failed to get app directory")?;

    // Get the exe filename for process matching
    let exe_name =
        exe_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(if cfg!(target_os = "windows") {
                "RepakX.exe"
            } else {
                "REPAK-X"
            });

    // Determine archive type
    let is_zip = downloaded_path.to_lowercase().ends_with(".zip");
    let is_tar_gz = downloaded_path.to_lowercase().ends_with(".tar.gz");

    // Use runtime OS detection to create the appropriate updater script
    if cfg!(target_os = "windows") {
        // Windows: Create .bat script
        let updater_script_path = std::env::temp_dir().join("repakx_updater.bat");

        let script_content = if is_zip {
            format!(
                r#"@echo off
title RepakX Updater
echo ============================================
echo RepakX Portable Update
echo ============================================
echo.
echo Waiting for RepakX to close...
timeout /t 2 /nobreak >nul

:waitloop
tasklist /FI "IMAGENAME eq {exe_name}" 2>NUL | find /I /N "{exe_name}">NUL
if "%ERRORLEVEL%"=="0" (
    echo Still running, waiting...
    timeout /t 1 /nobreak >nul
    goto waitloop
)

echo RepakX closed. Starting update...
echo.

echo Extracting update archive...
cd /d "{temp_dir}"

:: Ensure extracted directory exists
if not exist "{temp_dir}\extracted" mkdir "{temp_dir}\extracted"

:: Use PowerShell to extract the ZIP
powershell -Command "Expand-Archive -LiteralPath '{zip_path}' -DestinationPath '{temp_dir}\extracted' -Force" 2>nul
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to extract update archive!
    echo Please extract manually from: {zip_path}
    echo To: {app_dir}
    pause
    exit /b 1
)

:: Check if extraction created a single subfolder (common with GitHub releases)
set "EXTRACTED_DIR="
set "FOLDER_COUNT=0"
for /d %%i in ("{temp_dir}\extracted\*") do (
    set "EXTRACTED_DIR=%%i"
    set /a FOLDER_COUNT+=1
)

:: If exactly one subfolder exists and it contains an exe or dll, use that folder
if "%FOLDER_COUNT%"=="1" (
    if exist "%EXTRACTED_DIR%\*.exe" (
        echo Found nested folder: %EXTRACTED_DIR%
    ) else if exist "%EXTRACTED_DIR%\*.dll" (
        echo Found nested folder: %EXTRACTED_DIR%
    ) else (
        set "EXTRACTED_DIR={temp_dir}\extracted"
    )
) else (
    set "EXTRACTED_DIR={temp_dir}\extracted"
)

echo Source: %EXTRACTED_DIR%
echo Destination: {app_dir}
echo.

echo Copying new files...
xcopy /E /Y /I /Q "%EXTRACTED_DIR%\*" "{app_dir}\" >nul
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Failed to copy update files!
    echo Please copy manually from: %EXTRACTED_DIR%
    echo To: {app_dir}
    pause
    exit /b 1
)

:: Remove stale artifacts that should not ship (legacy ue4-dds-tools, debug symbols, stale executables)
if exist "{app_dir}\uassettool\ue4-dds-tools" rd /s /q "{app_dir}\uassettool\ue4-dds-tools" 2>nul
del /q "{app_dir}\uassettool\*.pdb" 2>nul

:: Flattening migration guard: the new layout places UAssetTool.dll at the root and removes the
:: uassettool folder. Remove the old uassettool folder ONLY when the incoming
:: release ships the exact marker (UAssetTool.dll) at the root AND no longer ships the folder.
:: Hardcode the marker name -- NEVER match *.dll, because Oodle (oo2core/oodle*) DLLs also live
:: in this folder and a wildcard would falsely "detect AOT" and delete a still-needed UAssetTool.exe.
if exist "%EXTRACTED_DIR%\UAssetTool.dll" (
    if not exist "%EXTRACTED_DIR%\uassettool" (
        if exist "{app_dir}\uassettool" rd /s /q "{app_dir}\uassettool" 2>nul
    )
)

:: These are always-wrong locations (never the resolved tool path) -- safe to clean unconditionally
if exist "{app_dir}\UAssetTool.exe" del /f /q "{app_dir}\UAssetTool.exe" 2>nul
if exist "{app_dir}\tools\UAssetTool.exe" del /f /q "{app_dir}\tools\UAssetTool.exe" 2>nul
if exist "{app_dir}\tools\uassettool\UAssetTool.exe" del /f /q "{app_dir}\tools\uassettool\UAssetTool.exe" 2>nul

echo Cleaning up temporary files...
rd /s /q "{temp_dir}" 2>nul

echo.
echo ============================================
echo Update complete!
echo ============================================
echo.
echo Launching RepakX...
timeout /t 2 /nobreak >nul
start "" "{exe_path}"

:: Self-delete and exit cleanly
(goto) 2>nul & del "%~f0" & exit
"#,
                exe_name = exe_name,
                temp_dir = download_path
                    .parent()
                    .unwrap_or(&std::env::temp_dir())
                    .to_string_lossy()
                    .replace('/', "\\"),
                zip_path = download_path.to_string_lossy().replace('/', "\\"),
                app_dir = app_dir.to_string_lossy().replace('/', "\\"),
                exe_path = exe_path.to_string_lossy().replace('/', "\\"),
            )
        } else {
            format!(
                r#"@echo off
title RepakX Updater
echo Waiting for RepakX to close...
timeout /t 2 /nobreak >nul

:waitloop
tasklist /FI "IMAGENAME eq {exe_name}" 2>NUL | find /I /N "{exe_name}">NUL
if "%ERRORLEVEL%"=="0" (
    timeout /t 1 /nobreak >nul
    goto waitloop
)

echo Running installer...
start /wait "" "{installer_path}"

echo Cleaning up...
del "{installer_path}"

:: Self-delete and exit cleanly
(goto) 2>nul & del "%~f0" & exit
"#,
                exe_name = exe_name,
                installer_path = download_path.to_string_lossy().replace('/', "\\"),
            )
        };

        std::fs::write(&updater_script_path, &script_content)
            .map_err(|e| format!("Failed to write updater script: {}", e))?;

        info!(
            "Created Windows updater script at: {:?}",
            updater_script_path
        );

        // Launch the updater script
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            std::process::Command::new("cmd")
                .args([
                    "/C",
                    "start",
                    "/MIN",
                    "RepakX Updater",
                    &updater_script_path.to_string_lossy(),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Failed to launch updater: {}", e))?;
        }
    } else if cfg!(target_os = "linux") {
        // Linux: Create .sh script
        let linux_script_path = std::env::temp_dir().join("repakx_updater.sh");

        let linux_script = if is_tar_gz {
            format!(
                r#"#!/bin/bash
echo "============================================"
echo "RepakX Portable Update"
echo "============================================"
echo ""
echo "Waiting for RepakX to close..."
sleep 2

# Wait for process to exit
while pgrep -f "{exe_name}" > /dev/null; do
    echo "Still running, waiting..."
    sleep 1
done

echo "RepakX closed. Starting update..."
echo ""

# Extract update
TEMP_DIR="{temp_dir}"
ARCHIVE_PATH="{archive_path}"
APP_DIR="{app_dir}"

echo "Extracting update archive..."
mkdir -p "$TEMP_DIR/extracted"
tar -xzf "$ARCHIVE_PATH" -C "$TEMP_DIR/extracted"

# Check for nested folder
EXTRACTED_DIR="$TEMP_DIR/extracted"
SUBDIR_COUNT=$(find "$EXTRACTED_DIR" -maxdepth 1 -type d | wc -l)
if [ "$SUBDIR_COUNT" -eq 2 ]; then
    SUBDIR=$(find "$EXTRACTED_DIR" -maxdepth 1 -type d ! -path "$EXTRACTED_DIR" | head -1)
    if [ -f "$SUBDIR/REPAK-X" ] || [ -f "$SUBDIR/repak-x" ]; then
        EXTRACTED_DIR="$SUBDIR"
    fi
fi

echo "Source: $EXTRACTED_DIR"
echo "Destination: $APP_DIR"
echo ""

echo "Copying new files..."
cp -rf "$EXTRACTED_DIR"/* "$APP_DIR/"
chmod +x "$APP_DIR/REPAK-X" 2>/dev/null || chmod +x "$APP_DIR/repak-x" 2>/dev/null || true

# Flattening migration guard: the new layout places UAssetTool.dll at the root and removes the
# uassettool folder. Remove the old uassettool folder ONLY when the incoming
# release ships the exact marker (UAssetTool.dll) at the root AND no longer ships the folder.
# Hardcode the marker name -- NEVER match *.dll, because Oodle DLLs also live in this folder and a
# wildcard would falsely "detect AOT" and delete a still-needed UAssetTool binary.
if [ -f "$EXTRACTED_DIR/UAssetTool.dll" ] && [ ! -d "$EXTRACTED_DIR/uassettool" ]; then
    rm -rf "$APP_DIR/uassettool" 2>/dev/null
fi
# Always-wrong locations (never the resolved tool path) -- safe to clean unconditionally
rm -f "$APP_DIR/UAssetTool" "$APP_DIR/tools/UAssetTool" "$APP_DIR/tools/uassettool/UAssetTool" 2>/dev/null

echo "Cleaning up..."
rm -rf "$TEMP_DIR"

echo ""
echo "============================================"
echo "Update complete!"
echo "============================================"
echo ""
echo "Launching RepakX..."
sleep 2
"{exe_path}" &

# Delete this script
rm -f "$0"
"#,
                exe_name = exe_name,
                temp_dir = download_path
                    .parent()
                    .unwrap_or(&std::env::temp_dir())
                    .to_string_lossy(),
                archive_path = download_path.to_string_lossy(),
                app_dir = app_dir.to_string_lossy(),
                exe_path = exe_path.to_string_lossy(),
            )
        } else if is_zip {
            format!(
                r#"#!/bin/bash
echo "============================================"
echo "RepakX Portable Update"
echo "============================================"
echo ""
echo "Waiting for RepakX to close..."
sleep 2

# Wait for process to exit
while pgrep -f "{exe_name}" > /dev/null; do
    echo "Still running, waiting..."
    sleep 1
done

echo "RepakX closed. Starting update..."
echo ""

# Extract update
TEMP_DIR="{temp_dir}"
ZIP_PATH="{zip_path}"
APP_DIR="{app_dir}"

echo "Extracting update archive..."
mkdir -p "$TEMP_DIR/extracted"
unzip -o "$ZIP_PATH" -d "$TEMP_DIR/extracted"

# Check for nested folder
EXTRACTED_DIR="$TEMP_DIR/extracted"
SUBDIR_COUNT=$(find "$EXTRACTED_DIR" -maxdepth 1 -type d | wc -l)
if [ "$SUBDIR_COUNT" -eq 2 ]; then
    SUBDIR=$(find "$EXTRACTED_DIR" -maxdepth 1 -type d ! -path "$EXTRACTED_DIR" | head -1)
    if [ -f "$SUBDIR/REPAK-X" ] || [ -f "$SUBDIR/repak-x" ]; then
        EXTRACTED_DIR="$SUBDIR"
    fi
fi

echo "Source: $EXTRACTED_DIR"
echo "Destination: $APP_DIR"
echo ""

echo "Copying new files..."
cp -rf "$EXTRACTED_DIR"/* "$APP_DIR/"
chmod +x "$APP_DIR/REPAK-X" 2>/dev/null || chmod +x "$APP_DIR/repak-x" 2>/dev/null || true

# Flattening migration guard: the new layout places UAssetTool.dll at the root and removes the
# uassettool folder. Remove the old uassettool folder ONLY when the incoming
# release ships the exact marker (UAssetTool.dll) at the root AND no longer ships the folder.
# Hardcode the marker name -- NEVER match *.dll, because Oodle DLLs also live in this folder and a
# wildcard would falsely "detect AOT" and delete a still-needed UAssetTool binary.
if [ -f "$EXTRACTED_DIR/UAssetTool.dll" ] && [ ! -d "$EXTRACTED_DIR/uassettool" ]; then
    rm -rf "$APP_DIR/uassettool" 2>/dev/null
fi
# Always-wrong locations (never the resolved tool path) -- safe to clean unconditionally
rm -f "$APP_DIR/UAssetTool" "$APP_DIR/tools/UAssetTool" "$APP_DIR/tools/uassettool/UAssetTool" 2>/dev/null

echo "Cleaning up..."
rm -rf "$TEMP_DIR"

echo ""
echo "============================================"
echo "Update complete!"
echo "============================================"
echo ""
echo "Launching RepakX..."
sleep 2
"{exe_path}" &

# Delete this script
rm -f "$0"
"#,
                exe_name = exe_name,
                temp_dir = download_path
                    .parent()
                    .unwrap_or(&std::env::temp_dir())
                    .to_string_lossy(),
                zip_path = download_path.to_string_lossy(),
                app_dir = app_dir.to_string_lossy(),
                exe_path = exe_path.to_string_lossy(),
            )
        } else {
            // For AppImage or other executables
            format!(
                r#"#!/bin/bash
echo "Waiting for RepakX to close..."
sleep 2

while pgrep -f "{exe_name}" > /dev/null; do
    sleep 1
done

echo "Installing update..."
chmod +x "{installer_path}"
"{installer_path}"

rm -f "{installer_path}"
rm -f "$0"
"#,
                exe_name = exe_name,
                installer_path = download_path.to_string_lossy(),
            )
        };

        std::fs::write(&linux_script_path, &linux_script)
            .map_err(|e| format!("Failed to write Linux updater script: {}", e))?;

        info!("Created Linux updater script at: {:?}", linux_script_path);

        // Make script executable and launch it
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&linux_script_path)
                .map_err(|e| format!("Failed to get script metadata: {}", e))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&linux_script_path, perms)
                .map_err(|e| format!("Failed to set script permissions: {}", e))?;

            std::process::Command::new("bash")
                .arg(&linux_script_path)
                .spawn()
                .map_err(|e| format!("Failed to launch updater: {}", e))?;
        }

        #[cfg(not(unix))]
        {
            return Err("Linux update not supported on this build".to_string());
        }
    } else {
        return Err("Unsupported operating system for auto-update".to_string());
    }

    info!("Updater script launched, app will update on close");

    // Emit event to notify frontend that update is ready
    let _ = window.emit("update_ready_to_apply", ());

    // Exit immediately so the updater script can continue without manual user action
    info!("Auto-closing app to continue update process");
    window.app_handle().exit(0);

    Ok(())
}

/// Get auto-update preference from settings
#[tauri::command]
async fn get_auto_update_enabled(state: State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    let state = state.lock().unwrap();
    Ok(state.auto_check_updates)
}

/// Set auto-update preference
#[tauri::command]
async fn set_auto_update_enabled(
    enabled: bool,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let mut state = state.lock().unwrap();
    state.auto_check_updates = enabled;
    save_state(&state).map_err(|e| e.to_string())?;
    Ok(())
}

/// Cancel an ongoing update download (cleanup temp files)
#[tauri::command]
async fn cancel_update_download() -> Result<(), String> {
    let temp_dir = std::env::temp_dir().join("repakx_update");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to cleanup temp directory: {}", e))?;
    }
    Ok(())
}

// ============================================================================
// DISCORD RICH PRESENCE COMMANDS
// ============================================================================

/// Enable Discord Rich Presence
#[tauri::command]
async fn discord_connect(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    discord_state.manager.connect()
}

/// Disable Discord Rich Presence
#[tauri::command]
async fn discord_disconnect(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    discord_state.manager.disconnect()
}

/// Check if Discord is connected
#[tauri::command]
async fn discord_is_connected(discord_state: State<'_, DiscordState>) -> Result<bool, String> {
    Ok(discord_state.manager.is_connected())
}

/// Set Discord activity to idle state
#[tauri::command]
async fn discord_set_idle(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    if discord_state.manager.is_connected() {
        discord_state.manager.set_idle()
    } else {
        Ok(())
    }
}

/// Set Discord activity to show mod count
#[tauri::command]
async fn discord_set_managing_mods(
    mod_count: usize,
    discord_state: State<'_, DiscordState>,
) -> Result<(), String> {
    if discord_state.manager.is_connected() {
        discord_state.manager.set_managing_mods(mod_count)
    } else {
        Ok(())
    }
}

/// Set Discord activity to show installing mod
#[tauri::command]
async fn discord_set_installing(
    mod_name: String,
    discord_state: State<'_, DiscordState>,
) -> Result<(), String> {
    if discord_state.manager.is_connected() {
        discord_state.manager.set_installing_mod(&mod_name)
    } else {
        Ok(())
    }
}

/// Set Discord activity to show sharing mods
#[tauri::command]
async fn discord_set_sharing(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    if discord_state.manager.is_connected() {
        discord_state.manager.set_sharing_mods()
    } else {
        Ok(())
    }
}

/// Set Discord activity to show receiving mods
#[tauri::command]
async fn discord_set_receiving(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    if discord_state.manager.is_connected() {
        discord_state.manager.set_receiving_mods()
    } else {
        Ok(())
    }
}

/// Clear Discord activity
#[tauri::command]
async fn discord_clear_activity(discord_state: State<'_, DiscordState>) -> Result<(), String> {
    discord_state.manager.clear_activity()
}

/// Set Discord theme (changes the logo based on app color palette)
/// Theme names: "blue", "red", "green", "purple", "orange", "pink", "cyan", "yellow", "teal", "default"
#[tauri::command]
async fn discord_set_theme(
    theme: String,
    discord_state: State<'_, DiscordState>,
) -> Result<(), String> {
    let resolved_theme = resolve_accent_name(&theme);
    discord_state.manager.set_theme(&resolved_theme);
    Ok(())
}

/// Get current Discord theme
#[tauri::command]
async fn discord_get_theme(discord_state: State<'_, DiscordState>) -> Result<String, String> {
    Ok(discord_state.manager.get_theme())
}

// ============================================================================
// CRASH MONITORING COMMANDS
// ============================================================================

/// Monitor game state and detect crashes
/// This should be called periodically (every 2-5 seconds) from the frontend
#[tauri::command]
async fn monitor_game_for_crashes(
    crash_state: State<'_, CrashMonitorState>,
    window: Window,
) -> Result<Option<crash_monitor::CrashInfo>, String> {
    // Use the shared reliable game detection function
    let game_running = is_game_process_running();

    let mut game_start_time = crash_state.game_start_time.lock().unwrap();
    let mut last_checked = crash_state.last_checked_crash.lock().unwrap();

    // Game just started - record the start time
    if game_running && game_start_time.is_none() {
        let now = std::time::SystemTime::now();
        *game_start_time = Some(now);
        *last_checked = Some(now);
        info!("Game started - monitoring for crashes from: {:?}", now);
        return Ok(None);
    }

    // Game just stopped - check for crashes that occurred during THIS session
    if !game_running && game_start_time.is_some() {
        let session_start = game_start_time.unwrap();
        info!(
            "Game stopped - checking for crashes since session start: {:?}",
            session_start
        );

        // Check for crashes created AFTER the game started
        let new_crashes = crash_monitor::check_for_new_crashes(session_start);

        // Reset state for next session
        *game_start_time = None;

        if !new_crashes.is_empty() {
            error!("⚠️ ═══════════════════════════════════════════════════════════════");
            error!("⚠️ CRASH DETECTED! Marvel Rivals crashed during this session!");
            error!("⚠️ ═══════════════════════════════════════════════════════════════");
            error!(
                "⚠️ Found {} crash folder(s) from this session",
                new_crashes.len()
            );

            // Parse the most recent crash
            if let Some(crash_folder) = new_crashes.first() {
                let crash_info = crash_monitor::parse_crash_info(crash_folder, Vec::new());

                if let Some(ref info) = crash_info {
                    let unknown_error = "Unknown error".to_string();
                    let error_msg = info.error_message.as_ref().unwrap_or(&unknown_error);

                    error!("⚠️ Crash Details:");
                    error!(
                        "⚠️   Type: {}",
                        info.crash_type.as_ref().unwrap_or(&"Unknown".to_string())
                    );

                    // Parse and display detailed error information
                    let (asset_path, error_type, details) =
                        crash_monitor::parse_error_details(error_msg);

                    if let Some(err_type) = error_type {
                        error!("⚠️   Error Type: {}", err_type);
                    }

                    if let Some(asset) = asset_path {
                        error!("⚠️   Affected Asset: {}", asset);

                        // Extract character ID if present
                        if let Some(char_id) = crash_monitor::extract_character_id(error_msg) {
                            error!("⚠️   Character ID: {}", char_id);
                        }
                    }

                    if let Some(detail) = details {
                        error!("⚠️   Details: {}", detail);
                    }

                    // Check if it's a mesh-related crash
                    if crash_monitor::is_mesh_related_crash(error_msg) {
                        error!("⚠️   ⚡ MESH LOADING ERROR detected");
                    }

                    if let Some(seconds) = info.seconds_since_start {
                        let minutes = seconds / 60;
                        let secs = seconds % 60;
                        error!("⚠️   Time in game: {}m {}s", minutes, secs);
                    }

                    error!("⚠️   Crash folder: {:?}", crash_folder);
                    error!("⚠️   Mods enabled: {} mod(s)", info.enabled_mods.len());

                    if !info.enabled_mods.is_empty() {
                        error!("⚠️   Active mods:");
                        for mod_name in &info.enabled_mods {
                            error!("⚠️     - {}", mod_name);
                        }
                    }

                    // Show full error message at the end for reference
                    error!("⚠️");
                    error!("⚠️   Full Error Message:");
                    error!("⚠️   {}", error_msg);
                    error!("⚠️ ═══════════════════════════════════════════════════════════════");

                    // Update last checked time to avoid re-reporting this crash
                    *last_checked = Some(info.timestamp);

                    // Emit toast notification with crash details
                    toast_events::emit_crash_from_info(&window, info);
                }

                return Ok(crash_info);
            }
        } else {
            info!("✓ ═══════════════════════════════════════════════════════════════");
            info!("✓ Game closed normally - no crashes detected this session");
            info!("✓ ═══════════════════════════════════════════════════════════════");
        }
    }

    Ok(None)
}

/// Check for crashes that occurred in previous sessions (when app wasn't running)
/// This should be called once on app startup to detect crashes from the last game session
#[tauri::command]
async fn check_for_previous_crash(
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<Option<crash_monitor::CrashInfo>, String> {
    let last_known = {
        let state_guard = state.lock().unwrap();
        state_guard.last_known_crash_folder.clone()
    };

    // Check for crashes since last known
    let crash_info = crash_monitor::check_for_previous_session_crash(last_known.as_deref());

    if let Some(ref info) = crash_info {
        error!("⚠️ ═══════════════════════════════════════════════════════════════");
        error!("⚠️ PREVIOUS SESSION CRASH DETECTED!");
        error!("⚠️ ═══════════════════════════════════════════════════════════════");
        error!("⚠️ Crash folder: {:?}", info.crash_folder);

        if let Some(ref err_msg) = info.error_message {
            error!("⚠️ Error: {}", err_msg);
        }

        // Emit toast notification
        toast_events::emit_crash_from_info(&window, info);
    }

    // Update last known crash folder to the newest one (whether crash detected or not)
    if let Some((newest_name, _)) = crash_monitor::get_newest_crash_folder() {
        let mut state_guard = state.lock().unwrap();
        state_guard.last_known_crash_folder = Some(newest_name);
        let _ = save_state(&state_guard);
    }

    Ok(crash_info)
}

#[tauri::command]
async fn get_crash_history() -> Result<Vec<PathBuf>, String> {
    let crash_dir = crash_monitor::get_crash_log_path();

    if !crash_dir.exists() {
        return Ok(Vec::new());
    }

    let mut crashes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&crash_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                crashes.push(path);
            }
        }
    }

    // Sort by creation time (newest first)
    crashes.sort_by(|a, b| {
        let a_time = std::fs::metadata(a).and_then(|m| m.created()).ok();
        let b_time = std::fs::metadata(b).and_then(|m| m.created()).ok();
        b_time.cmp(&a_time)
    });

    Ok(crashes)
}

#[tauri::command]
async fn get_total_crashes() -> Result<usize, String> {
    Ok(crash_monitor::count_total_crashes())
}

#[tauri::command]
async fn clear_crash_logs() -> Result<usize, String> {
    crash_monitor::clear_all_crashes()
}

#[tauri::command]
async fn dismiss_crash_dialog() -> Result<(), String> {
    // This is a no-op in Tauri version - frontend handles dialog state
    Ok(())
}

#[tauri::command]
async fn get_crash_log_path() -> Result<String, String> {
    Ok(crash_monitor::get_crash_log_path()
        .to_string_lossy()
        .to_string())
}

// ============================================================================
// CHARACTER DATA COMMANDS
// ============================================================================

#[tauri::command]
async fn get_character_data() -> Result<Vec<character_data::CharacterSkin>, String> {
    Ok(character_data::get_all_character_data())
}

#[tauri::command]
async fn get_character_by_skin_id(
    skin_id: String,
) -> Result<Option<character_data::CharacterSkin>, String> {
    Ok(character_data::get_character_by_skin_id(&skin_id))
}

#[tauri::command]
async fn refresh_character_cache() -> Result<String, String> {
    info!("Manually refreshing character data cache...");
    character_data::refresh_cache();
    info!("Character data cache refreshed successfully");
    Ok("Character data cache refreshed successfully".to_string())
}

/// Update character data from GitHub MarvelRivalsCharacterIDs with progress events
/// Supports cancellation via cancel_character_update command
#[tauri::command]
async fn update_character_data_from_github(window: Window) -> Result<usize, String> {
    let _ = window.emit(
        "install_log",
        "[Character Data] Starting GitHub data fetch...",
    );

    // Create progress callback that emits events
    let window_clone = window.clone();
    let on_progress = move |msg: &str| {
        let _ = window_clone.emit("install_log", format!("[Character Data] {}", msg));
    };

    match character_data::update_from_github_with_progress(on_progress).await {
        Ok(new_count) => {
            let msg = format!(
                "[Character Data] ✓ Complete! {} new skins added.",
                new_count
            );
            let _ = window.emit("install_log", &msg);
            // Trigger mod list refresh so new character names show up
            let _ = window.emit("character_data_updated", new_count);
            info!(
                "Successfully updated character data. {} new skins added.",
                new_count
            );
            Ok(new_count)
        }
        Err(e) if e == "Cancelled" => {
            let _ = window.emit("install_log", "[Character Data] ✗ Update cancelled by user");
            Err(e)
        }
        Err(e) => {
            let msg = format!("[Character Data] ✗ Error: {}", e);
            let _ = window.emit("install_log", &msg);
            error!("Failed to update character data: {}", e);
            Err(e)
        }
    }
}

/// Cancel an ongoing character data update
#[tauri::command]
async fn cancel_character_update() -> Result<(), String> {
    character_data::request_cancel_update();
    info!("Character data update cancellation requested");
    Ok(())
}

#[tauri::command]
async fn get_character_data_path() -> Result<String, String> {
    Ok(character_data::character_data_path()
        .to_string_lossy()
        .to_string())
}

#[tauri::command]
async fn identify_mod_character(
    file_paths: Vec<String>,
) -> Result<Option<(String, String)>, String> {
    Ok(character_data::identify_mod_from_paths(&file_paths))
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Repak-X")
}

/// Directory for log files - placed next to the executable for easy access
fn log_dir() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("Logs");
        }
    }
    // Fallback to config-based app_dir if current_exe fails
    app_dir()
}

fn save_state(state: &AppState) -> std::io::Result<()> {
    let dir = app_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("state.json");
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, state)?;
    Ok(())
}

fn load_state() -> AppState {
    let path = app_dir().join("state.json");
    let mut state = if let Ok(file) = File::open(path) {
        serde_json::from_reader(file).unwrap_or_default()
    } else {
        AppState::default()
    };

    state
}

fn setup_logging() {
    // Try exe-relative Logs folder first
    let log_dir = log_dir();
    let log_file = log_dir.join("repakx.log");

    // Attempt to create the log directory
    let log_file_result = std::fs::create_dir_all(&log_dir).and_then(|_| File::create(&log_file));

    let final_log_file = match log_file_result {
        Ok(file) => {
            // Successfully created log file at exe-relative location
            eprintln!("Logging to: {}", log_file.display());
            file
        }
        Err(e) => {
            // Fallback to temp directory if exe-relative fails
            eprintln!("Failed to create log at {}: {}", log_file.display(), e);
            let temp_log = std::env::temp_dir().join("repakx.log");
            eprintln!("Fallback logging to: {}", temp_log.display());
            File::create(&temp_log).expect("Failed to create log file even in temp directory")
        }
    };

    let _ = CombinedLogger::init(vec![
        TermLogger::new(
            log::LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(log::LevelFilter::Debug, Config::default(), final_log_file),
    ]);
}

#[derive(Debug, Clone, serde::Serialize)]
struct ModDetails {
    mod_name: String,
    mod_type: String,
    character_name: String,
    category: String,
    character_id: String,
    file_count: usize,
    total_size: u64,
    files: Vec<String>,
    is_iostore: bool,
    is_encrypted: bool,
    has_blueprint: bool,
    is_hybrid: bool,
}

#[tauri::command]
async fn get_mod_details(
    mod_path: String,
    _detect_blueprint: Option<bool>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ModDetails, String> {
    let path = PathBuf::from(&mod_path);

    info!("Getting details for mod: {}", path.display());

    if !path.exists() {
        return Err(format!("Mod file does not exist: {}", path.display()));
    }

    // --- Cache check ---
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .map_err(|e| format!("Failed to get file metadata: {}", e))?;

    {
        let state_guard = state.lock().unwrap();
        if let Some((cached_mtime, cached_details)) = state_guard.mod_details_cache.get(&path) {
            if *cached_mtime == mtime {
                info!("Cache hit for mod: {}", path.display());
                return Ok(cached_details.clone());
            } else {
                info!("Cache stale for mod: {} (mtime changed)", path.display());
            }
        }
    }
    info!("Cache miss for mod: {}, opening PAK...", path.display());
    // --- End cache check ---

    // Check if it's IoStore (has .utoc file) BEFORE trying to open the PAK
    // Obfuscated IoStore mods have encrypted PAK indexes with zeroed EncryptionKeyGuid,
    // which causes repak to fail. For IoStore mods we read the file list from .utoc instead.
    let mut utoc_path = path.clone();
    utoc_path.set_extension("utoc");
    let is_iostore = utoc_path.exists();

    // Get file list (plus, for PAK, extra metadata via the new list_pak JSON API)
    // `pak_list_info` carries through richer info — total content size and encrypted-index
    // status — so we don't have to make a second call later.
    let mut pak_list_info: Option<uasset_toolkit::PakListResult> = None;
    let mut is_hybrid = false;
    let files: Vec<String> = if is_iostore {
        // For IoStore, read from utoc (handles both normal and obfuscated containers)
        use crate::utoc_utils::read_utoc;
        let mut utoc_files: Vec<String> = read_utoc(&utoc_path)
            .iter()
            .map(|entry| entry.file_path.clone())
            .collect();

        // For Hybrid detection: Check if the .pak file also contains raw assets
        if path.exists() && path.extension().unwrap_or_default() == "pak" {
            if let Ok(listing) = uasset_toolkit::list_pak(
                path.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
                None,
            ) {
                let all_pak_files: Vec<String> =
                    listing.files.iter().map(|e| e.path.clone()).collect();

                let has_raw_assets = all_pak_files.iter().any(|f| {
                    let lower = f.to_lowercase();
                    !lower.contains("chunknames") && !lower.contains("patched_files")
                });

                if has_raw_assets {
                    info!("[Detection] Found legacy PAK alongside IoStore bundle (Hybrid!)");
                    is_hybrid = true;
                }

                if !all_pak_files.is_empty() {
                    // Always append legacy pak files to the total files list
                    utoc_files.extend(all_pak_files);
                    pak_list_info = Some(listing);
                }
            }
        }

        utoc_files
    } else {
        let listing = uasset_toolkit::list_pak(
            path.to_str().unwrap_or_default(),
            Some(crate::install_mod::AES_KEY_HEX),
            None,
        )
        .map_err(|e| format!("Failed to read PAK: {}", e))?;
        let paths: Vec<String> = listing.files.iter().map(|e| e.path.clone()).collect();
        pak_list_info = Some(listing);
        paths
    };

    let file_count = files.len();

    info!("PAK contains {} files", file_count);
    if file_count > 0 && file_count <= 10 {
        info!("Files: {:?}", files);
    } else if file_count > 10 {
        info!("First 10 files: {:?}", &files[..10]);
    }

    // Determine mod type using the detailed function
    use crate::utils::get_pak_characteristics_detailed;
    let characteristics = get_pak_characteristics_detailed(files.clone());
    info!("Detected mod type: {}", characteristics.mod_type);
    info!("Character name: {}", characteristics.character_name);
    info!("Category: {}", characteristics.category);

    // Run fast Blueprint detection using filename heuristics
    let has_blueprint = files.iter().any(|f| {
        let filename = f.split('/').last().unwrap_or("");
        let name_lower = filename.to_lowercase();
        let path_lower = f.to_lowercase();

        // Common Blueprint patterns:
        // 1. BP_Something (Blueprint prefix)
        // 2. Something_C (Blueprint class suffix)
        // 3. SomethingBP (Blueprint suffix without underscore)
        // 4. /Blueprints/ folder path
        name_lower.starts_with("bp_")
            || name_lower.contains("_c.")
            || name_lower.contains("bp.")
            || name_lower.ends_with("bp")
            || path_lower.contains("/blueprints/")
    });

    if has_blueprint {
        info!("Blueprint detected via filename patterns");
    }

    // Get total size:
    //   - IoStore: prefer .ucas size (the actual content blob)
    //   - PAK: prefer the summed uncompressed content size from list_pak (richer than the
    //          on-disk PAK file size, which includes compression + headers)
    //   - Fallback: on-disk file size
    let ucas_path_for_size = path.with_extension("ucas");
    let total_size = if is_iostore && ucas_path_for_size.exists() {
        std::fs::metadata(&ucas_path_for_size)
            .map(|m| m.len())
            .unwrap_or(0)
    } else if let Some(info) = pak_list_info.as_ref() {
        // Use the uncompressed content size reported by the PAK index
        info.total_uncompressed_bytes
    } else {
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    };

    let mod_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    // Check encryption:
    //   - IoStore: ask UAssetTool whether the container is AES-encrypted
    //   - PAK: list_pak already told us if the index is encrypted (obfuscated mods)
    let is_encrypted = if is_iostore {
        uasset_toolkit::is_iostore_encrypted(&utoc_path.to_string_lossy()).unwrap_or(false)
    } else {
        pak_list_info
            .as_ref()
            .map(|info| info.encrypted_index)
            .unwrap_or(false)
    };

    let details = ModDetails {
        mod_name,
        mod_type: characteristics.mod_type,
        character_name: characteristics.character_name,
        category: characteristics.category,
        character_id: characteristics.character_id,
        file_count,
        total_size,
        files,
        is_iostore,
        is_encrypted,
        has_blueprint,
        is_hybrid,
    };

    // --- Cache store ---
    {
        let mut state_guard = state.lock().unwrap();
        state_guard
            .mod_details_cache
            .insert(path.clone(), (mtime, details.clone()));
        info!("Cached details for mod: {}", path.display());
    }
    // --- End cache store ---

    Ok(details)
}

#[derive(Clone, Serialize, Deserialize)]
struct ModClash {
    file_path: String,
    mod_paths: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SingleModConflict {
    conflicting_mod_path: String,
    conflicting_mod_name: String,
    overlapping_files: Vec<String>,
    priority_comparison: String,
    affected_characters: Vec<String>,
}

#[tauri::command]
async fn check_mod_clashes(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<ModClash>, String> {
    use std::collections::HashMap;

    let state = state.lock().unwrap();
    let game_path = &state.game_path;

    info!("Checking for mod clashes...");

    if !game_path.exists() {
        return Err("Game path does not exist".to_string());
    }

    // Structure to hold mod info for clash detection
    #[derive(Clone)]
    struct ModInfo {
        path: PathBuf,
        priority: usize,
        files: Vec<String>, // List of files inside this mod
    }

    let mut mods_info: Vec<ModInfo> = Vec::new();

    // Scan all enabled mods
    for entry in WalkDir::new(&game_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str());

        // Only check enabled .pak files
        if ext != Some("pak") {
            continue;
        }

        // Calculate priority (same as get_pak_files)
        let mut priority = 0;
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        // Check for "!" prefix (highest priority)
        if file_stem.starts_with("!") {
            priority = 0; // Highest priority
        } else if file_stem.ends_with("_P") {
            let base_no_p = file_stem.strip_suffix("_P").unwrap();
            let re_nums = Regex::new(r"_(\d+)$").unwrap();
            if let Some(caps) = re_nums.captures(base_no_p) {
                let nums = &caps[1];
                if nums.chars().all(|c| c == '9') {
                    let actual_nines = nums.len();
                    // Convert actual nines count to UI priority (1-based)
                    if actual_nines >= 7 {
                        priority = actual_nines - 6;
                    }
                }
            }
        }

        // Check if it's IoStore
        let mut utoc_path = path.to_path_buf();
        utoc_path.set_extension("utoc");
        let is_iostore = utoc_path.exists();

        // Get file list
        let files: Vec<String> = if is_iostore {
            use crate::utoc_utils::read_utoc;
            read_utoc(&utoc_path)
                .iter()
                .map(|entry| entry.file_path.clone())
                .collect()
        } else {
            match uasset_toolkit::list_pak_files(
                path.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            ) {
                Ok(f) => f,
                Err(e) => {
                    warn!("Failed to read PAK {:?}: {}", path, e);
                    continue;
                }
            }
        };

        mods_info.push(ModInfo {
            path: path.to_path_buf(),
            priority,
            files,
        });
    }

    info!("Analyzed {} enabled mods", mods_info.len());

    // Don't group by character - instead, compare all mods at the same priority level
    // Group by priority first
    let mut by_priority: HashMap<usize, Vec<ModInfo>> = HashMap::new();

    for mod_info in mods_info {
        by_priority
            .entry(mod_info.priority)
            .or_insert_with(Vec::new)
            .push(mod_info);
    }

    // Find clashes: same priority and overlapping files
    let mut clashes: Vec<ModClash> = Vec::new();
    use std::collections::HashSet;

    for (priority, same_priority_mods) in by_priority {
        if same_priority_mods.len() < 2 {
            continue;
        }

        info!(
            "Checking priority {} with {} mods",
            priority,
            same_priority_mods.len()
        );

        // Compare each pair of mods at this priority level
        for i in 0..same_priority_mods.len() {
            for j in (i + 1)..same_priority_mods.len() {
                let mod1 = &same_priority_mods[i];
                let mod2 = &same_priority_mods[j];

                // Convert file lists to HashSets for efficient intersection
                let files1: HashSet<&String> = mod1.files.iter().collect();
                let files2: HashSet<&String> = mod2.files.iter().collect();

                // Find overlapping files, excluding metadata files like 'patched_files'
                let overlapping_files: Vec<String> = files1
                    .intersection(&files2)
                    .filter(|f| !f.ends_with("patched_files") && !f.contains("/patched_files"))
                    .map(|s| (*s).clone())
                    .collect();

                if !overlapping_files.is_empty() {
                    // Found a clash! These two mods modify the same files
                    let mod_paths = vec![
                        mod1.path.to_string_lossy().to_string(),
                        mod2.path.to_string_lossy().to_string(),
                    ];

                    // Build a description showing which characters are affected
                    let mut affected_characters = HashSet::new();

                    // Extract character IDs from overlapping file paths
                    for file_path in &overlapping_files {
                        // Look for pattern like "Characters/1050/" or "1050/1050800/"
                        if let Some(char_match) = file_path.split('/').find(|s| {
                            s.len() == 4
                                && s.chars().all(|c| c.is_ascii_digit())
                                && s.starts_with("10")
                        }) {
                            affected_characters.insert(char_match.to_string());
                        }
                    }

                    let character_info = if !affected_characters.is_empty() {
                        let char_ids: Vec<String> = affected_characters.iter().cloned().collect();
                        format!("Characters: {} - ", char_ids.join(", "))
                    } else {
                        String::new()
                    };

                    let clash_description = format!(
                        "{}Priority: {} - {} overlapping file(s)",
                        character_info,
                        priority,
                        overlapping_files.len()
                    );

                    info!(
                        "Found clash between {} and {} at priority {} ({} overlapping files, characters: {:?})",
                        mod1.path.file_name().unwrap_or_default().to_string_lossy(),
                        mod2.path.file_name().unwrap_or_default().to_string_lossy(),
                        priority,
                        overlapping_files.len(),
                        affected_characters
                    );

                    clashes.push(ModClash {
                        file_path: clash_description,
                        mod_paths,
                    });
                }
            }
        }
    }
    info!("Found {} clashes", clashes.len());
    Ok(clashes)
}

#[tauri::command]
async fn check_single_mod_conflicts(
    mod_path: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<SingleModConflict>, String> {
    use std::collections::HashSet;

    let target_path = PathBuf::from(&mod_path);

    if !target_path.exists() {
        return Err(format!("Mod file does not exist: {}", mod_path));
    }

    let game_path = {
        let state = state.lock().unwrap();
        state.game_path.clone()
    };

    if !game_path.exists() {
        return Err("Game path does not exist".to_string());
    }

    info!("Checking conflicts for mod: {}", target_path.display());

    // Helper to calculate priority from filename
    fn calculate_priority(path: &Path) -> usize {
        let mut priority = 0;
        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        if file_stem.starts_with("!") {
            priority = 0;
        } else if file_stem.ends_with("_P") {
            let base_no_p = file_stem.strip_suffix("_P").unwrap();
            let re_nums = Regex::new(r"_(\d+)$").unwrap();
            if let Some(caps) = re_nums.captures(base_no_p) {
                let nums = &caps[1];
                if nums.chars().all(|c| c == '9') {
                    let actual_nines = nums.len();
                    if actual_nines >= 7 {
                        priority = actual_nines - 6;
                    }
                }
            }
        }
        priority
    }

    // Helper to get files from a PAK or IoStore
    fn get_pak_files(path: &Path) -> Result<Vec<String>, String> {
        let mut utoc_path = path.to_path_buf();
        utoc_path.set_extension("utoc");

        if utoc_path.exists() {
            use crate::utoc_utils::read_utoc;
            Ok(read_utoc(&utoc_path)
                .iter()
                .map(|entry| entry.file_path.clone())
                .collect())
        } else {
            uasset_toolkit::list_pak_files(
                path.to_str().unwrap_or_default(),
                Some(crate::install_mod::AES_KEY_HEX),
            )
            .map_err(|e| format!("Failed to read PAK: {}", e))
        }
    }

    // Get target mod info
    let target_priority = calculate_priority(&target_path);
    let target_files: HashSet<String> = get_pak_files(&target_path)?.into_iter().collect();

    info!(
        "Target mod has {} files at priority {}",
        target_files.len(),
        target_priority
    );

    let mut conflicts: Vec<SingleModConflict> = Vec::new();

    // Scan all other enabled mods
    for entry in WalkDir::new(&game_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        // Skip non-pak files
        let ext = path.extension().and_then(|s| s.to_str());
        if ext != Some("pak") {
            continue;
        }

        // Skip the target mod itself
        if path == target_path {
            continue;
        }

        // Get this mod's files
        let other_files: HashSet<String> = match get_pak_files(path) {
            Ok(files) => files.into_iter().collect(),
            Err(e) => {
                warn!("Failed to read mod {:?}: {}", path, e);
                continue;
            }
        };

        // Find overlapping files, excluding metadata files like 'patched_files'
        let overlapping: Vec<String> = target_files
            .intersection(&other_files)
            .filter(|f| !f.ends_with("patched_files") && !f.contains("/patched_files"))
            .cloned()
            .collect();

        if overlapping.is_empty() {
            continue;
        }

        // Calculate priority comparison
        let other_priority = calculate_priority(path);
        let priority_comparison = if target_priority == other_priority {
            "Same priority (conflict!)".to_string()
        } else if target_priority < other_priority {
            format!(
                "Target has higher priority ({} vs {})",
                target_priority, other_priority
            )
        } else {
            format!(
                "Target has lower priority ({} vs {})",
                target_priority, other_priority
            )
        };

        // Extract affected characters from overlapping files
        let mut affected_characters: HashSet<String> = HashSet::new();
        for file_path in &overlapping {
            if let Some(char_match) = file_path.split('/').find(|s| {
                s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) && s.starts_with("10")
            }) {
                affected_characters.insert(char_match.to_string());
            }
        }

        let mod_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        info!(
            "Found conflict with {} ({} overlapping files)",
            mod_name,
            overlapping.len()
        );

        conflicts.push(SingleModConflict {
            conflicting_mod_path: path.to_string_lossy().to_string(),
            conflicting_mod_name: mod_name,
            overlapping_files: overlapping,
            priority_comparison,
            affected_characters: affected_characters.into_iter().collect(),
        });
    }

    info!(
        "Found {} conflicts for mod {}",
        conflicts.len(),
        target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    Ok(conflicts)
}

// ============================================================================
// P2P SHARING COMMANDS
// ============================================================================

/// Start sharing a mod pack
#[tauri::command]
async fn p2p_start_sharing(
    name: String,
    description: String,
    mod_paths: Vec<String>,
    creator: Option<String>,
    p2p_state: State<'_, P2PState>,
) -> Result<p2p_libp2p::ShareInfo, String> {
    let paths: Vec<PathBuf> = mod_paths.iter().map(PathBuf::from).collect();

    p2p_state
        .manager
        .start_sharing(name, description, paths, creator)
        .await
        .map_err(|e| e.to_string())
}

/// Stop sharing
#[tauri::command]
async fn p2p_stop_sharing(
    share_code: String,
    p2p_state: State<'_, P2PState>,
) -> Result<(), String> {
    p2p_state
        .manager
        .stop_sharing(&share_code)
        .map_err(|e| e.to_string())
}

/// Get current share session info
#[tauri::command]
async fn p2p_get_share_session(
    p2p_state: State<'_, P2PState>,
) -> Result<Option<p2p_libp2p::ShareInfo>, String> {
    // Return the first active share if any
    let shares = p2p_state.manager.active_shares.lock();
    Ok(shares
        .values()
        .next()
        .map(|s| s.session.clone())
        .and_then(|session| {
            // Convert ShareSession to ShareInfo
            p2p_libp2p::ShareInfo::decode(&session.connection_string).ok()
        }))
}

/// Check if currently sharing
#[tauri::command]
async fn p2p_is_sharing(p2p_state: State<'_, P2PState>) -> Result<bool, String> {
    Ok(!p2p_state.manager.active_shares.lock().is_empty())
}

/// Start receiving mods from a connection string
#[tauri::command]
async fn p2p_start_receiving(
    connection_string: String,
    client_name: Option<String>,
    folder_id: Option<String>,
    window: Window,
    state: State<'_, Arc<Mutex<AppState>>>,
    p2p_state: State<'_, P2PState>,
    watcher_state: State<'_, WatcherState>,
) -> Result<(), String> {
    // Pause file watcher so incoming files don't trigger mod-list spam
    watcher_state.paused.store(true, Ordering::Relaxed);
    info!("[P2P] File watcher paused for transfer");

    let game_path = {
        let state_guard = state.lock().unwrap();
        state_guard.game_path.clone()
    };

    let output_dir = match folder_id {
        Some(ref id) if !id.is_empty() => game_path.join(id),
        _ => game_path,
    };
    info!("[P2P] Receive destination: {}", output_dir.display());

    p2p_state
        .manager
        .start_receiving(&connection_string, output_dir, client_name, window)
        .await
        .map_err(|e| e.to_string())
}

/// Stop receiving
#[tauri::command]
async fn p2p_stop_receiving(
    p2p_state: State<'_, P2PState>,
    watcher_state: State<'_, WatcherState>,
    window: Window,
) -> Result<(), String> {
    // Signal all active clients to stop, then clear
    p2p_state.manager.stop_all_downloads();

    // Unpause file watcher and emit one refresh so the mod list picks up new files
    watcher_state.paused.store(false, Ordering::Relaxed);
    info!("[P2P] File watcher resumed after transfer");
    let _ = window.emit("mods_dir_changed", ());

    Ok(())
}

/// Get current transfer progress
#[tauri::command]
async fn p2p_get_receive_progress(
    p2p_state: State<'_, P2PState>,
) -> Result<Option<p2p_sharing::TransferProgress>, String> {
    let downloads = p2p_state.manager.active_downloads.lock();

    // Return the first active download's progress (typically only one at a time)
    if let Some((_, download)) = downloads.iter().next() {
        Ok(Some(download.progress.clone()))
    } else {
        Ok(None)
    }
}

/// Check if currently receiving
#[tauri::command]
async fn p2p_is_receiving(p2p_state: State<'_, P2PState>) -> Result<bool, String> {
    Ok(!p2p_state.manager.active_downloads.lock().is_empty())
}

/// Create a shareable mod pack preview (total size and file count)
#[tauri::command]
async fn p2p_create_mod_pack_preview(
    name: String,
    description: String,
    mod_paths: Vec<String>,
    creator: Option<String>,
) -> Result<p2p_sharing::PackPreview, String> {
    let paths: Vec<PathBuf> = mod_paths.iter().map(PathBuf::from).collect();
    p2p_sharing::create_mod_pack_preview(name, description, &paths, creator)
        .map_err(|e| e.to_string())
}

/// Validate a connection string without connecting
#[tauri::command]
async fn p2p_validate_connection_string(connection_string: String) -> Result<bool, String> {
    // Validate base64 ShareInfo format
    match p2p_libp2p::ShareInfo::decode(&connection_string) {
        Ok(_) => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

/// Calculate hash for a file (useful for verification)
#[tauri::command]
async fn p2p_hash_file(file_path: String) -> Result<String, String> {
    let path = PathBuf::from(file_path);
    p2p_sharing::hash_file(&path).map_err(|e| e.to_string())
}

// ============================================================================
// PROTOCOL REGISTRATION (Portable App Support)
// ============================================================================

/// Registers the repakx:// protocol handler in Windows Registry (HKCU)
/// This enables the browser extension to communicate with the app.
/// Safe to call on every startup - it will just update the path if needed.
#[cfg(target_os = "windows")]
fn register_protocol_handler() -> Result<(), Box<dyn std::error::Error>> {
    use winreg::enums::*;
    use winreg::RegKey;

    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Create or open the protocol key
    let (protocol_key, _) = hkcu.create_subkey(r"Software\Classes\repakx")?;
    protocol_key.set_value("", &"URL:Repak X Protocol")?;
    protocol_key.set_value("URL Protocol", &"")?;

    // Create the DefaultIcon key (optional, for nice icon in Windows)
    let (icon_key, _) = hkcu.create_subkey(r"Software\Classes\repakx\DefaultIcon")?;
    icon_key.set_value("", &format!("\"{}\",0", exe_path_str))?;

    // Create the shell\open\command key
    let (command_key, _) = hkcu.create_subkey(r"Software\Classes\repakx\shell\open\command")?;
    let command = format!("\"{}\" \"%1\"", exe_path_str);
    command_key.set_value("", &command)?;

    info!(
        "Registered repakx:// protocol handler for: {}",
        exe_path_str
    );
    Ok(())
}

#[cfg(target_os = "linux")]
fn register_protocol_handler() -> Result<(), Box<dyn std::error::Error>> {
    // On Linux, register the protocol handler via .desktop file
    // This creates a user-local .desktop file in ~/.local/share/applications/

    let exe_path = std::env::current_exe()?;
    let exe_path_str = exe_path.to_string_lossy();

    // Create the .desktop file content
    let desktop_content = format!(
        r#"[Desktop Entry]
Type=Application
Name=Repak X
Comment=Marvel Rivals Mod Manager
Exec="{}" %u
Icon=repakx
Terminal=false
Categories=Game;Utility;
MimeType=x-scheme-handler/repakx;
StartupNotify=true
"#,
        exe_path_str
    );

    // Get the applications directory
    if let Some(home) = dirs::home_dir() {
        let applications_dir = home.join(".local/share/applications");
        std::fs::create_dir_all(&applications_dir)?;

        let desktop_file = applications_dir.join("repakx.desktop");
        std::fs::write(&desktop_file, desktop_content)?;

        // Update the MIME database to register the handler
        // This is done via xdg-mime or update-desktop-database
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&applications_dir)
            .output();

        // Also try to set as default handler
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "repakx.desktop", "x-scheme-handler/repakx"])
            .output();

        info!(
            "Registered repakx:// protocol handler for Linux: {}",
            exe_path_str
        );
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn register_protocol_handler() -> Result<(), Box<dyn std::error::Error>> {
    // On macOS, protocol handlers are registered via Info.plist in the app bundle
    // This is typically done at build time, not runtime
    // For now, just log that it's not implemented
    info!("macOS protocol handler registration is handled via Info.plist at build time");
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn register_protocol_handler() -> Result<(), Box<dyn std::error::Error>> {
    // No-op on other platforms
    Ok(())
}

// ============================================================================
// DEEP LINK PROTOCOL HANDLER
// ============================================================================

fn handle_deep_link_url(url: &str, app_handle: &tauri::AppHandle) {
    info!("Processing deep link URL: {}", url);

    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.scheme() == "repakx" && parsed.host_str() == Some("install") {
            if let Some(file_path) = parsed
                .query_pairs()
                .find(|(key, _)| key == "file")
                .map(|(_, value)| value.to_string())
            {
                let decoded_path = urlencoding::decode(&file_path)
                    .unwrap_or(file_path.clone().into())
                    .to_string();

                info!("Received mod file from extension: {}", decoded_path);

                let path = std::path::Path::new(&decoded_path);
                if path.exists() {
                    if let Err(e) = app_handle.emit("extension-mod-received", &decoded_path) {
                        error!("Failed to emit extension-mod-received event: {}", e);
                    } else {
                        info!("Emitted extension-mod-received event for: {}", decoded_path);
                    }
                } else {
                    warn!("Deep link file does not exist: {}", decoded_path);
                    let _ = app_handle.emit(
                        "extension-mod-error",
                        format!("File not found: {}", decoded_path),
                    );
                }
            } else {
                warn!("Deep link URL missing 'file' parameter: {}", url);
            }
        } else {
            warn!(
                "Unknown deep link action: scheme={}, host={:?}",
                parsed.scheme(),
                parsed.host_str()
            );
        }
    } else {
        error!("Failed to parse deep link URL: {}", url);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Cleanup old uassettool folder from previous versions
            let old_uassettool_dir = exe_dir.join("uassettool");
            if old_uassettool_dir.is_dir() {
                if let Err(e) = std::fs::remove_dir_all(&old_uassettool_dir) {
                    eprintln!("Failed to remove old uassettool folder {:?}: {}", old_uassettool_dir, e);
                } else {
                    println!("Successfully removed old uassettool folder");
                }
            }

            let log_dir = exe_dir.join("Logs");
            if let Err(e) = std::fs::create_dir_all(&log_dir) {
                eprintln!("Failed to create log directory {:?}: {}", log_dir, e);
            } else {
                let startup_log = log_dir.join("startup.log");
                let _ = std::fs::write(
                    &startup_log,
                    format!(
                        "RepakX (Tauri) startup at {:?}\n",
                        std::time::SystemTime::now()
                    ),
                );
            }
        }
    }

    setup_logging();
    info!("Starting RepakX v{}", env!("CARGO_PKG_VERSION"));

    // Register protocol handler for portable app support (self-healing registry)
    if let Err(e) = register_protocol_handler() {
        warn!(
            "Failed to register repakx:// protocol handler: {} - browser extension may not work",
            e
        );
    }

    // Initialize UAssetToolkit global singleton on startup
    // This starts the UAssetTool process once and keeps it alive for the app lifetime
    info!("Initializing UAssetToolkit global singleton...");
    if let Err(e) = uasset_toolkit::init_global_toolkit() {
        warn!(
            "Failed to initialize UAssetToolkit singleton: {} - detection features may be slower",
            e
        );
    } else {
        info!("UAssetToolkit global singleton initialized successfully");
    }

    // Initialize character data cache on startup
    info!("Initializing character data cache...");
    character_data::refresh_cache();

    let state = Arc::new(Mutex::new(load_state()));
    let watcher_state = WatcherState {
        watcher: Mutex::new(None),
        last_event_time: Mutex::new(std::time::Instant::now()),
        paused: Arc::new(AtomicBool::new(false)),
    };
    let crash_state = CrashMonitorState {
        game_start_time: Mutex::new(None),
        last_checked_crash: Mutex::new(None),
    };
    let p2p_manager = tokio::runtime::Runtime::new()
        .expect("Failed to create tokio runtime")
        .block_on(p2p_manager::UnifiedP2PManager::new())
        .expect("Failed to initialize P2P network");
    let p2p_state = P2PState {
        manager: Arc::new(p2p_manager),
    };

    // Initialize Discord Rich Presence manager
    let discord_manager = discord_presence::create_discord_manager();

    // Check saved state to see if DRP should be enabled
    {
        let state_guard = state.lock().unwrap();
        if state_guard.enable_drp {
            if let Err(e) = discord_manager.connect() {
                warn!("Failed to auto-connect Discord RPC: {}", e);
            } else {
                info!("Auto-connected Discord RPC from saved settings");

                // Apply saved theme
                discord_manager.set_theme(state_guard.accent_color.as_str());

                // Set initial activity
                if let Err(e) = discord_manager.set_idle() {
                    warn!("Failed to set initial Discord RPC activity: {}", e);
                }
            }
        }
    }

    let discord_state = DiscordState {
        manager: discord_manager,
    };
    #[cfg(target_os = "linux")]
    {
        // Tauri and NVIDIA don't mix, due to Webkit compositing and DMABUF renderer issues so this env fixes that
        std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    }
    tauri::Builder::default()
        .manage(state)
        .manage(watcher_state)
        .manage(crash_state)
        .manage(p2p_state)
        .manage(discord_state)
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // This closure is called when a second instance is launched
            // `args` contains command line arguments including the deep-link URL
            info!("Single instance callback triggered with args: {:?}", args);

            // Focus the main window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
                let _ = window.unminimize();
            }

            // Check if args contains a repakx:// URL
            for arg in args.iter() {
                if arg.starts_with("repakx://") {
                    info!("Received deep link from second instance: {}", arg);
                    handle_deep_link_url(arg, app);
                }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(e) = app.deep_link().register("repakx") {
                    warn!("Failed to register repakx:// protocol: {}", e);
                } else {
                    info!("Successfully registered repakx:// protocol handler");
                }
            }

            let app_handle = app.handle().clone();
            app.listen("deep-link://new-url", move |event| {
                let payload = event.payload();
                info!("Received deep link URL: {}", payload);
                handle_deep_link_url(payload, &app_handle);
            });

            // ============================================================
            // COLD START DEEP LINK HANDLING
            // ============================================================
            // When the app is launched via repakx:// protocol (not already running),
            // the URL is passed as a command-line argument, not as an event.
            // We need to check for it here and emit the event to the frontend.
            //
            // Note: We use a small delay to ensure the frontend is ready to receive events.
            // ============================================================
            let startup_app_handle = app.handle().clone();
            std::thread::spawn(move || {
                // Wait for the frontend to be ready
                std::thread::sleep(std::time::Duration::from_millis(1000));

                // Check command-line arguments for repakx:// URL
                let args: Vec<String> = std::env::args().collect();
                info!("Startup command-line args: {:?}", args);

                for arg in args.iter().skip(1) {
                    // Skip the exe path itself
                    if arg.starts_with("repakx://") {
                        info!("Found cold-start deep link URL: {}", arg);
                        handle_deep_link_url(arg, &startup_app_handle);
                        break; // Only process the first repakx:// URL
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_game_path,
            set_game_path,
            auto_detect_game_path,
            start_file_watcher,
            get_pak_files,
            get_pak_files_in_folder,
            merge_command::merge_mods_for_hybrid,
            parse_dropped_files,
            install_mods,
            quick_organize,
            delete_mod,
            update_mod,
            rename_mod,
            open_in_explorer,
            copy_to_clipboard,
            create_folder,
            get_folders,
            get_root_folder_info,
            update_folder,
            delete_folder,
            rename_folder,
            move_folder,
            assign_mod_to_folder,
            add_custom_tag,
            remove_custom_tag,
            get_all_tags,
            add_tag_to_catalog,
            delete_tag_from_all_mods,
            toggle_mod,
            check_game_running,
            launch_game,
            skip_launcher_patch,
            get_skip_launcher_status,
            get_sig_bypasser_status,
            toggle_sig_bypasser,
            recompress_mods,
            get_app_version,
            check_for_updates,
            download_update,
            apply_update,
            get_auto_update_enabled,
            set_auto_update_enabled,
            cancel_update_download,
            monitor_game_for_crashes,
            check_for_previous_crash,
            get_crash_history,
            get_total_crashes,
            clear_crash_logs,
            dismiss_crash_dialog,
            get_crash_log_path,
            get_mod_details,
            set_mod_priority,
            check_mod_clashes,
            check_single_mod_conflicts,
            extract_pak_to_destination,
            extract_mod_assets,
            // Character data commands
            get_character_data,
            get_character_by_skin_id,
            update_character_data_from_github,
            cancel_character_update,
            identify_mod_character,
            get_character_data_path,
            refresh_character_cache,
            // P2P sharing commands
            p2p_start_sharing,
            p2p_stop_sharing,
            p2p_get_share_session,
            p2p_is_sharing,
            p2p_start_receiving,
            p2p_stop_receiving,
            p2p_get_receive_progress,
            p2p_is_receiving,
            p2p_create_mod_pack_preview,
            p2p_validate_connection_string,
            p2p_hash_file,
            // Bundled LOD Disabler commands
            check_lod_disabler_deployed,
            get_lod_disabler_path,
            deploy_lod_disabler,
            // Discord Rich Presence commands
            discord_connect,
            discord_disconnect,
            discord_is_connected,
            discord_set_idle,
            discord_set_managing_mods,
            discord_set_installing,
            discord_set_sharing,
            discord_set_receiving,
            discord_clear_activity,
            discord_set_theme,
            discord_get_theme,
            // App Settings
            get_accent_presets,
            get_app_settings,
            save_app_settings,
            // Parallel processing
            set_parallel_processing,
            get_parallel_processing,
            // VFX Updater commands
            vfx_updater::vfx_start_session,
            vfx_updater::vfx_stop_session,
            vfx_updater::vfx_extract_mod_assets,
            vfx_updater::vfx_convert_uassets_to_json,
            vfx_updater::vfx_extract_vanilla_assets,
            vfx_updater::vfx_convert_json_to_uassets,
            vfx_updater::vfx_pack_to_iostore,
            vfx_updater::vfx_detect_asset_class,
            vfx_updater::vfx_get_temp_dir,
            vfx_updater::vfx_create_step_directory,
            vfx_updater::vfx_create_pipeline_directories,
            vfx_updater::vfx_cleanup_temp_directories,
            vfx_updater::vfx_cleanup_pipeline,
            vfx_updater::vfx_get_uasset_tool_path,
            vfx_updater::vfx_get_asset_classes,
            vfx_updater::vfx_is_updatable_class,
            vfx_updater::vfx_copy_uasset_files,
            vfx_updater::vfx_read_json_file,
            vfx_updater::vfx_write_json_file,
            vfx_updater::vfx_list_json_files,
            vfx_updater::vfx_get_settings,
            vfx_updater::vfx_save_settings,
            vfx_updater::vfx_check_usmap_update,
            hero_assets::sync_hero_images,
            hero_assets::get_hero_image_ids,
            hero_assets::get_hero_image,
            hero_assets::get_hero_cache_dir
        ])
        .on_window_event(|window, event| {
            // When the main window is closed, also close any auxiliary
            // windows (e.g. the VFX Updater) so the app exits cleanly.
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    for (label, other) in app.webview_windows() {
                        if label != "main" {
                            let _ = other.close();
                        }
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Ensure Discord RPC is disconnected on exit
                let discord_state = app_handle.state::<DiscordState>();
                if discord_state.manager.is_connected() {
                    info!("App exiting, disconnecting Discord RPC...");
                    let _ = discord_state.manager.disconnect();
                }
            }
        });
}
