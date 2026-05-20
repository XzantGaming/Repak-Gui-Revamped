//! VFX Updater - Tauri Commands
//!
//! Exposes VFX pipeline operations to the frontend via Tauri IPC.

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Window};

use super::file_ops::{
    cleanup_vfx_temp_directories, create_step_directory, get_uasset_tool_path, get_vfx_temp_base,
    list_json_files, read_json_file, write_json_file,
};
use super::logging::{close_vfx_log, init_vfx_log, vfx_error, vfx_info, vfx_warn};
use super::models::{AssetClassInfo, VfxPipelineResult, VfxSettings, VfxTempDirectories};
use super::pipeline::{
    cleanup_pipeline, create_pipeline_directories,
    detect_asset_class as pipeline_detect_asset_class, step_convert_json_to_uassets,
    step_convert_mod_to_json, step_extract_mod_assets, step_extract_vanilla_assets,
    step_pack_to_iostore,
};
use super::progress::TauriVfxProgressSink;
use super::uasset_tool::{close_vfx_uat_session, ensure_vfx_uat_session};
use serde::Serialize;

/// App data directory matching the rest of Repak-X: %APPDATA%/Repak-X
fn vfx_app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Repak-X")
}

// ============================================================================
// Session Management
// ============================================================================

#[tauri::command]
pub async fn vfx_start_session(app: AppHandle) -> Result<(), String> {
    // Initialize logging
    let app_data_dir = vfx_app_dir();
    let _ = std::fs::create_dir_all(&app_data_dir);
    let _ = init_vfx_log(&app_data_dir);

    let tool_path = get_uasset_tool_path(&app)?;
    ensure_vfx_uat_session(&tool_path).await?;
    vfx_info("VFX session started");
    Ok(())
}

#[tauri::command]
pub async fn vfx_stop_session() -> Result<(), String> {
    vfx_info("Stopping VFX session");
    close_vfx_uat_session().await;
    close_vfx_log();
    Ok(())
}

// ============================================================================
// Pipeline Steps
// ============================================================================

#[tauri::command]
pub async fn vfx_extract_mod_assets(
    window: Window,
    app: AppHandle,
    game_paks: String,
    mod_path: String,
    output_dir: String,
) -> Result<Vec<String>, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    let progress = TauriVfxProgressSink::new(&window);

    step_extract_mod_assets(&tool_path, &game_paks, &mod_path, &output_dir, &progress).await
}

#[tauri::command]
pub async fn vfx_convert_uassets_to_json(
    window: Window,
    app: AppHandle,
    usmap_path: String,
    input_dir: String,
    output_dir: String,
) -> Result<Vec<String>, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    let progress = TauriVfxProgressSink::new(&window);

    step_convert_mod_to_json(&tool_path, &usmap_path, &input_dir, &output_dir, &progress).await
}

#[tauri::command]
pub async fn vfx_extract_vanilla_assets(
    window: Window,
    app: AppHandle,
    game_paks: String,
    output_dir: String,
    filter_patterns: Vec<String>,
) -> Result<Vec<String>, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    let progress = TauriVfxProgressSink::new(&window);

    step_extract_vanilla_assets(
        &tool_path,
        &game_paks,
        &output_dir,
        &filter_patterns,
        &progress,
    )
    .await
}

#[tauri::command]
pub async fn vfx_convert_json_to_uassets(
    window: Window,
    app: AppHandle,
    usmap_path: String,
    input_dir: String,
    output_dir: String,
) -> Result<Vec<String>, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    let progress = TauriVfxProgressSink::new(&window);

    step_convert_json_to_uassets(&tool_path, &usmap_path, &input_dir, &output_dir, &progress).await
}

#[tauri::command]
pub async fn vfx_pack_to_iostore(
    window: Window,
    app: AppHandle,
    usmap_path: String,
    input_dir: String,
    output_base: String,
) -> Result<String, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    let progress = TauriVfxProgressSink::new(&window);

    step_pack_to_iostore(&tool_path, &usmap_path, &input_dir, &output_base, &progress).await
}

// ============================================================================
// Asset Class Detection
// ============================================================================

#[tauri::command]
pub async fn vfx_detect_asset_class(
    app: AppHandle,
    usmap_path: String,
    file_path: String,
) -> Result<AssetClassInfo, String> {
    let tool_path = get_uasset_tool_path(&app)?;
    pipeline_detect_asset_class(&tool_path, &usmap_path, &file_path).await
}

// ============================================================================
// Temp Directory Management
// ============================================================================

#[tauri::command]
pub fn vfx_get_temp_dir() -> Result<String, String> {
    let base = get_vfx_temp_base()?;
    Ok(base.to_string_lossy().to_string())
}

#[tauri::command]
pub fn vfx_create_step_directory(step_name: String) -> Result<String, String> {
    let dir = create_step_directory(&step_name)?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn vfx_create_pipeline_directories() -> Result<VfxTempDirectories, String> {
    create_pipeline_directories()
}

#[tauri::command]
pub fn vfx_cleanup_temp_directories() -> Result<(), String> {
    cleanup_vfx_temp_directories()
}

#[tauri::command]
pub fn vfx_cleanup_pipeline() -> Result<(), String> {
    cleanup_pipeline()
}

// ============================================================================
// UAssetTool Path
// ============================================================================

#[tauri::command]
pub fn vfx_get_uasset_tool_path() -> Result<String, String> {
    // Use the same logic as install_mod for finding UAssetTool
    use crate::install_mod::install_mod_logic::iotoc::find_uasset_tool;

    let tool_path = find_uasset_tool().map_err(|e| format!("[VFX] {}", e))?;

    vfx_info(&format!("UAssetTool path: {}", tool_path.display()));
    Ok(tool_path.to_string_lossy().to_string())
}

// ============================================================================
// Asset Class Scanning
// ============================================================================

#[tauri::command]
pub async fn vfx_get_asset_classes(
    uat_path: String,
    usmap_path: String,
    uasset_paths: Vec<String>,
    window: Window,
) -> Result<std::collections::HashMap<String, String>, String> {
    use super::uasset_tool::batch_detect_asset_types;

    let progress = TauriVfxProgressSink::new(&window);
    batch_detect_asset_types(
        std::path::Path::new(&uat_path),
        &usmap_path,
        &uasset_paths,
        &progress,
    )
    .await
}

#[tauri::command]
pub fn vfx_is_updatable_class(class_name: String) -> bool {
    super::uasset_tool::is_updatable_class(&class_name)
}

#[tauri::command]
pub async fn vfx_copy_uasset_files(
    source_paths: Vec<String>,
    source_base_dir: String,
    dest_base_dir: String,
) -> Result<Vec<String>, String> {
    use std::path::{Path, PathBuf};

    let mut copied = Vec::new();

    // Normalize base dir for comparison
    let base_normalized: PathBuf = PathBuf::from(&source_base_dir).components().collect();

    for source_path in &source_paths {
        let source = Path::new(source_path);
        let source_normalized: PathBuf = source.components().collect();

        // Calculate relative path from source base (with normalized paths)
        let rel_path = source_normalized
            .strip_prefix(&base_normalized)
            .map_err(|e| {
                vfx_error(&format!(
                    "strip_prefix failed: source={}, base={}",
                    source_path, source_base_dir
                ));
                format!("[VFX] Failed to get relative path: {}", e)
            })?;

        let dest_path = Path::new(&dest_base_dir).join(rel_path);

        // Create parent directory
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("[VFX] Failed to create dir: {}", e))?;
        }

        // Copy the .uasset file
        std::fs::copy(source, &dest_path)
            .map_err(|e| format!("[VFX] Failed to copy {}: {}", source_path, e))?;

        // Copy companion files (.uexp, .ubulk, .uptnl) if they exist
        let stem = source.with_extension("");
        for ext in &["uexp", "ubulk", "uptnl"] {
            let companion = stem.with_extension(ext);
            if companion.exists() {
                let dest_companion = Path::new(&dest_base_dir).join(rel_path.with_extension(ext));
                std::fs::copy(&companion, &dest_companion).ok();
            }
        }

        copied.push(dest_path.to_string_lossy().to_string());
    }

    vfx_info(&format!(
        "Copied {} non-updatable assets to output",
        copied.len()
    ));
    Ok(copied)
}

// ============================================================================
// JSON File Operations
// ============================================================================

#[tauri::command]
pub async fn vfx_read_json_file(path: String) -> Result<String, String> {
    read_json_file(&path)
}

#[tauri::command]
pub async fn vfx_write_json_file(path: String, content: String) -> Result<(), String> {
    write_json_file(&path, &content)
}

#[tauri::command]
pub async fn vfx_list_json_files(dir: String) -> Result<Vec<String>, String> {
    list_json_files(&dir)
}

// ============================================================================
// Settings (VFX-specific)
// ============================================================================

#[tauri::command]
pub fn vfx_get_settings(_app: AppHandle) -> Result<VfxSettings, String> {
    let settings_path = vfx_app_dir().join("vfx_settings.json");

    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .map_err(|e| format!("[VFX] Failed to read settings: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("[VFX] Failed to parse settings: {}", e))
    } else {
        Ok(VfxSettings::default())
    }
}

#[tauri::command]
pub fn vfx_save_settings(_app: AppHandle, settings: VfxSettings) -> Result<(), String> {
    let app_dir = vfx_app_dir();
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("[VFX] Failed to create app data dir: {}", e))?;

    let settings_path = app_dir.join("vfx_settings.json");
    let content = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("[VFX] Failed to serialize settings: {}", e))?;

    std::fs::write(&settings_path, content)
        .map_err(|e| format!("[VFX] Failed to write settings: {}", e))?;

    vfx_info(&format!("Settings saved to {:?}", settings_path));
    Ok(())
}

// ============================================================================
// USMAP Auto-Update from rivals-depot
// ============================================================================

const USMAP_REPO_API: &str =
    "https://api.github.com/repos/SpaceDepot/rivals-depot/contents/usmap?ref=main";
const USMAP_COMMITS_API_BASE: &str = "https://api.github.com/repos/SpaceDepot/rivals-depot/commits";

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct VfxUsmapUpdateResult {
    /// True when a new file was downloaded
    pub updated: bool,
    /// True when the local copy already matches the latest available
    pub up_to_date: bool,
    /// True when no remote check was performed (custom user file in use)
    pub skipped: bool,
    /// Absolute path of the auto-managed local USMAP (if any)
    pub local_path: Option<String>,
    /// Filename of the latest USMAP
    pub filename: Option<String>,
    /// Friendly version label (e.g. "S7.5 / CL3484986")
    pub version: Option<String>,
    /// Build/changelist number parsed from filename
    pub build_number: Option<u64>,
    /// Latest commit date for this usmap file (YYYY-MM-DD)
    pub commit_date: Option<String>,
    /// Human readable status for the UI/log
    pub message: String,
}

fn read_vfx_settings_internal() -> VfxSettings {
    let p = vfx_app_dir().join("vfx_settings.json");
    if !p.exists() {
        return VfxSettings::default();
    }
    match std::fs::read_to_string(&p) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => VfxSettings::default(),
    }
}

fn write_vfx_settings_internal(settings: &VfxSettings) -> Result<(), String> {
    let dir = vfx_app_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("[VFX] Failed to create app data dir: {}", e))?;
    let p = dir.join("vfx_settings.json");
    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("[VFX] Failed to serialize settings: {}", e))?;
    std::fs::write(&p, content).map_err(|e| format!("[VFX] Failed to write settings: {}", e))?;
    Ok(())
}

/// Extract the build/changelist number from a depot usmap filename like
/// `5.3.2-3484986+++depot_marvel+S7.5_release-Marvel.usmap`. Returns None if
/// the pattern does not match.
fn extract_usmap_build_number(name: &str) -> Option<u64> {
    let after_dash = name.split_once('-')?.1;
    let cl_part = after_dash.split_once('+')?.0;
    cl_part.parse::<u64>().ok()
}

/// Pull a friendly version tag like `S7.5_release` out of the filename, if
/// present. Falls back to None.
fn extract_usmap_version_label(name: &str) -> Option<String> {
    // Pattern: `<engine>-<cl>+++depot_marvel+<RELEASE>-Marvel[+PY[_N]].usmap`
    let after_plusses = name.split("+++").nth(1)?;
    let after_marvel = after_plusses.split_once('+')?.1;
    let release = after_marvel.split('-').next()?;
    if release.is_empty() {
        None
    } else {
        Some(release.to_string())
    }
}

async fn fetch_usmap_commit_date(client: &reqwest::Client, file_name: &str) -> Option<String> {
    let path = format!("usmap/{}", file_name);
    let mut url = reqwest::Url::parse(USMAP_COMMITS_API_BASE).ok()?;
    url.query_pairs_mut()
        .append_pair("path", &path)
        .append_pair("per_page", "1");

    let resp = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;
    let date = json
        .as_array()?
        .first()?
        .get("commit")?
        .get("committer")?
        .get("date")?
        .as_str()?;

    Some(date.split('T').next().unwrap_or(date).to_string())
}

fn usmap_dir() -> PathBuf {
    vfx_app_dir().join("usmap")
}

fn is_in_managed_dir(path: &str) -> bool {
    let managed = usmap_dir();
    Path::new(path).starts_with(&managed)
}

/// Check rivals-depot on GitHub for a newer USMAP and apply it.
///
/// Behavior:
/// - Uses GitHub Contents API with conditional `If-None-Match` (ETag) so a
///   cheap 304 response is the typical case after the first call.
/// - Picks the depot file with the highest changelist number as "latest".
/// - Downloads into `%APPDATA%/Repak-X/usmap/<filename>` and updates
///   `VfxSettings` (path/sha/etag/filename) on success.
/// - If the user picked a custom file outside the managed directory, the
///   command does not overwrite the selection — it only reports availability.
#[tauri::command]
pub async fn vfx_check_usmap_update(force: bool) -> Result<VfxUsmapUpdateResult, String> {
    let mut settings = read_vfx_settings_internal();

    // Build HTTP client with a short timeout so the UI is never blocked.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("RepakX-VFX-Updater")
        .build()
        .map_err(|e| format!("[VFX] HTTP client error: {}", e))?;

    let mut req = client
        .get(USMAP_REPO_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if !force {
        if let Some(etag) = settings.usmap_etag.as_ref() {
            req = req.header("If-None-Match", etag.clone());
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            vfx_warn(&format!("USMAP check failed: {}", e));
            return Ok(VfxUsmapUpdateResult {
                message: format!("USMAP check failed: {}", e),
                local_path: settings.usmap_path.clone(),
                filename: settings.usmap_filename.clone(),
                ..Default::default()
            });
        }
    };

    let status = resp.status();

    // 304 Not Modified -> we already have the latest (or at least the same
    // remote listing as last time). Only treat as "up to date" if the local
    // file actually exists on disk.
    if status.as_u16() == 304 {
        let local_ok = settings
            .usmap_path
            .as_deref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        if local_ok {
            let build_number = settings
                .usmap_filename
                .as_deref()
                .and_then(extract_usmap_build_number);
            let commit_date = if let Some(file_name) = settings.usmap_filename.as_deref() {
                fetch_usmap_commit_date(&client, file_name).await
            } else {
                None
            };

            vfx_info("USMAP already up to date (304 Not Modified)");
            return Ok(VfxUsmapUpdateResult {
                up_to_date: true,
                local_path: settings.usmap_path.clone(),
                filename: settings.usmap_filename.clone(),
                version: settings
                    .usmap_filename
                    .as_deref()
                    .and_then(extract_usmap_version_label),
                build_number,
                commit_date,
                message: "USMAP is up to date".to_string(),
                ..Default::default()
            });
        }
        // Local file missing — fall through and force a fresh listing.
        let resp2 = client
            .get(USMAP_REPO_API)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| format!("[VFX] Refetch failed after 304: {}", e))?;
        return apply_listing(client.clone(), resp2, &mut settings).await;
    }

    if !status.is_success() {
        let msg = format!("GitHub returned status {}", status);
        vfx_warn(&format!("USMAP check: {}", msg));
        return Ok(VfxUsmapUpdateResult {
            message: msg,
            local_path: settings.usmap_path.clone(),
            filename: settings.usmap_filename.clone(),
            ..Default::default()
        });
    }

    apply_listing(client, resp, &mut settings).await
}

async fn apply_listing(
    client: reqwest::Client,
    resp: reqwest::Response,
    settings: &mut VfxSettings,
) -> Result<VfxUsmapUpdateResult, String> {
    // Capture ETag before consuming the body.
    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("[VFX] Failed to parse listing: {}", e))?;
    let entries = json
        .as_array()
        .ok_or_else(|| "[VFX] Unexpected listing format".to_string())?;

    // Find the .usmap entry with the highest build/CL number.
    let latest = entries
        .iter()
        .filter(|e| e.get("type").and_then(|t| t.as_str()) == Some("file"))
        .filter_map(|e| {
            let name = e.get("name")?.as_str()?;
            if !name.ends_with(".usmap") {
                return None;
            }
            let cl = extract_usmap_build_number(name)?;
            Some((cl, e))
        })
        .max_by_key(|(cl, _)| *cl)
        .map(|(_, e)| e);

    let latest = match latest {
        Some(e) => e,
        None => {
            return Ok(VfxUsmapUpdateResult {
                message: "No usmap files found in rivals-depot".to_string(),
                local_path: settings.usmap_path.clone(),
                filename: settings.usmap_filename.clone(),
                ..Default::default()
            });
        }
    };

    let latest_name = latest
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let latest_sha = latest
        .get("sha")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let download_url = latest
        .get("download_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version_label = extract_usmap_version_label(&latest_name);
    let build_number = extract_usmap_build_number(&latest_name);

    if latest_name.is_empty() || latest_sha.is_empty() || download_url.is_empty() {
        return Ok(VfxUsmapUpdateResult {
            message: "Latest usmap entry is missing fields".to_string(),
            ..Default::default()
        });
    }

    let commit_date = fetch_usmap_commit_date(&client, &latest_name).await;

    // Determine whether the user is currently using a managed path or a
    // custom one. Don't overwrite a custom file silently.
    let user_has_custom = match settings.usmap_path.as_deref() {
        Some(p) => !p.is_empty() && !is_in_managed_dir(p),
        None => false,
    };

    let managed_target = usmap_dir().join(&latest_name);
    let local_managed_ok =
        managed_target.exists() && settings.usmap_sha.as_deref() == Some(latest_sha.as_str());

    // Always persist the new etag (cheap), even when nothing else changes.
    if let Some(et) = etag.as_ref() {
        settings.usmap_etag = Some(et.clone());
    }

    if local_managed_ok {
        if !user_has_custom {
            settings.usmap_path = Some(managed_target.to_string_lossy().to_string());
            settings.usmap_filename = Some(latest_name.clone());
        }
        let _ = write_vfx_settings_internal(settings);
        vfx_info(&format!("USMAP up to date: {}", latest_name));
        return Ok(VfxUsmapUpdateResult {
            up_to_date: true,
            local_path: if user_has_custom {
                settings.usmap_path.clone()
            } else {
                Some(managed_target.to_string_lossy().to_string())
            },
            filename: Some(latest_name),
            version: version_label,
            build_number,
            commit_date,
            message: "USMAP is up to date".to_string(),
            ..Default::default()
        });
    }

    // Need to download. Stream raw bytes to disk.
    std::fs::create_dir_all(usmap_dir())
        .map_err(|e| format!("[VFX] Failed to create usmap dir: {}", e))?;

    vfx_info(&format!("Downloading latest USMAP: {}", latest_name));
    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("[VFX] Download failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("[VFX] Download HTTP error: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("[VFX] Download body error: {}", e))?;

    std::fs::write(&managed_target, &bytes)
        .map_err(|e| format!("[VFX] Failed to write usmap: {}", e))?;

    // Clean up the previous managed file if it has a different name.
    if let Some(old_name) = settings.usmap_filename.clone() {
        if old_name != latest_name {
            let old_path = usmap_dir().join(&old_name);
            if old_path.exists() {
                let _ = std::fs::remove_file(&old_path);
            }
        }
    }

    settings.usmap_sha = Some(latest_sha);
    settings.usmap_filename = Some(latest_name.clone());
    if !user_has_custom {
        settings.usmap_path = Some(managed_target.to_string_lossy().to_string());
    }
    write_vfx_settings_internal(settings)?;

    vfx_info(&format!(
        "USMAP updated to {} ({} bytes)",
        latest_name,
        bytes.len()
    ));

    Ok(VfxUsmapUpdateResult {
        updated: true,
        skipped: user_has_custom,
        local_path: Some(managed_target.to_string_lossy().to_string()),
        filename: Some(latest_name.clone()),
        version: version_label,
        build_number,
        commit_date,
        message: if user_has_custom {
            format!(
                "Latest USMAP downloaded ({}). Custom selection kept; click Browse to switch.",
                latest_name
            )
        } else {
            format!("USMAP updated to {}", latest_name)
        },
        ..Default::default()
    })
}
