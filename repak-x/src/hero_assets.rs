//! Hero portraits, synced from the rivals-resources repo at runtime.
//!
//! These used to be bundled into the binary via `import.meta.glob`, which meant
//! shipping a new build every time a hero was added. Instead the app now keeps
//! them in `%APPDATA%/Repak-X/hero` and reconciles that folder against
//! <https://github.com/SpaceDepot/rivals-resources> on startup: the first run
//! downloads the set, later runs only pull portraits that are new (or that
//! changed upstream).
//!
//! File naming matches the old bundled assets — `<character id>.png`, plus
//! `9999.png` as the unknown/multiple-heroes placeholder.

use crate::ui_log;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

/// Contents API listing for the repo folder holding the portraits.
const HERO_REPO_API: &str =
    "https://api.github.com/repos/SpaceDepot/rivals-resources/contents/hero?ref=main";

/// App data directory matching the rest of Repak-X: %APPDATA%/Repak-X
fn app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Repak-X")
}

fn hero_cache_dir() -> PathBuf {
    app_dir().join("hero")
}

fn hero_index_path() -> PathBuf {
    app_dir().join("hero_cache.json")
}

/// What we remember between runs so a check costs one conditional request.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HeroCacheIndex {
    /// ETag of the last Contents API listing.
    etag: Option<String>,
    /// Cached file name -> GitHub blob SHA it was downloaded from.
    files: BTreeMap<String, String>,
}

fn read_index() -> HeroCacheIndex {
    match std::fs::read_to_string(hero_index_path()) {
        Ok(c) => serde_json::from_str(&c).unwrap_or_default(),
        Err(_) => HeroCacheIndex::default(),
    }
}

fn write_index(index: &HeroCacheIndex) -> Result<(), String> {
    let dir = app_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("[Hero] Failed to create app data dir: {}", e))?;
    let content = serde_json::to_string_pretty(index)
        .map_err(|e| format!("[Hero] Failed to serialize hero cache index: {}", e))?;
    std::fs::write(hero_index_path(), content)
        .map_err(|e| format!("[Hero] Failed to write hero cache index: {}", e))
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeroSyncResult {
    /// Portraits available in the cache after this sync.
    pub cached: usize,
    /// Portraits downloaded for the first time.
    pub added: Vec<String>,
    /// Portraits re-downloaded because the repo copy changed.
    pub updated: Vec<String>,
    /// False when the repo could not be reached — the cache is still usable.
    pub checked: bool,
    /// Human readable status for the UI/log.
    pub message: String,
}

/// Download progress for the log drawer's progress bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeroSyncProgress {
    /// Portraits fetched so far.
    current: usize,
    /// Portraits this sync needs to fetch.
    total: usize,
    /// 0-100, ready to drop straight into the drawer.
    percent: u8,
    /// Status line for the drawer header.
    message: String,
    /// True on the final event, so the UI can hand the bar back.
    done: bool,
}

fn emit_progress(app: &AppHandle, progress: HeroSyncProgress) {
    if let Err(e) = app.emit("hero_sync_progress", progress) {
        warn!("[Hero] Failed to emit sync progress: {}", e);
    }
}

/// Scope tag for this module's log drawer entries.
const SCOPE: &str = "Heroes";

/// Number of `.png` files currently cached.
fn count_cached() -> usize {
    std::fs::read_dir(hero_cache_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("png"))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// Reconcile the local portrait cache with the rivals-resources repo.
///
/// `force` skips the conditional request, so the UI can offer an explicit
/// "check now" that ignores the stored ETag. A network failure is not an error:
/// whatever is already cached stays usable offline.
#[tauri::command]
pub async fn sync_hero_images(app: AppHandle, force: bool) -> Result<HeroSyncResult, String> {
    ui_log::info(
        SCOPE,
        if force {
            "Re-checking hero icons (ignoring cached listing)...".to_string()
        } else {
            "Checking hero icons for updates...".to_string()
        },
    );

    let cache_dir = hero_cache_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("[Hero] Failed to create hero cache dir: {}", e))?;

    let mut index = read_index();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("RepakX-Hero-Sync")
        .build()
        .map_err(|e| format!("[Hero] HTTP client error: {}", e))?;

    let mut req = client
        .get(HERO_REPO_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if !force {
        if let Some(etag) = index.etag.as_ref() {
            req = req.header("If-None-Match", etag.clone());
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let cached = count_cached();
            ui_log::warn(
                SCOPE,
                format!(
                    "Hero icon check failed ({}) - using {} cached icon(s)",
                    e, cached
                ),
            );
            return Ok(HeroSyncResult {
                cached,
                checked: false,
                message: format!("Hero portrait check failed: {}", e),
                ..Default::default()
            });
        }
    };

    let status = resp.status();

    // 304 Not Modified — the listing is unchanged since the last run, which is
    // the common case. Trust it only while the cache still holds the files it
    // described; otherwise fall through and re-fetch.
    if status.as_u16() == 304 {
        let cached = count_cached();
        if cached >= index.files.len() && cached > 0 {
            ui_log::success(
                SCOPE,
                format!("Hero icons are up to date ({} cached)", cached),
            );
            return Ok(HeroSyncResult {
                cached,
                checked: true,
                message: "Hero portraits are up to date".to_string(),
                ..Default::default()
            });
        }
        index.etag = None;
        let resp2 = client
            .get(HERO_REPO_API)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| format!("[Hero] Refetch failed after 304: {}", e))?;
        return apply_listing(&app, &client, resp2, index).await;
    }

    if !status.is_success() {
        ui_log::warn(
            SCOPE,
            format!(
                "Hero icon check skipped - GitHub returned {} (using {} cached icon(s))",
                status,
                count_cached()
            ),
        );
        return Ok(HeroSyncResult {
            cached: count_cached(),
            checked: false,
            message: format!("GitHub returned status {}", status),
            ..Default::default()
        });
    }

    apply_listing(&app, &client, resp, index).await
}

async fn apply_listing(
    app: &AppHandle,
    client: &reqwest::Client,
    resp: reqwest::Response,
    mut index: HeroCacheIndex,
) -> Result<HeroSyncResult, String> {
    // Capture the ETag before the body is consumed.
    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("[Hero] Failed to parse listing: {}", e))?;
    let entries = json
        .as_array()
        .ok_or_else(|| "[Hero] Unexpected listing format".to_string())?;

    let cache_dir = hero_cache_dir();
    let mut added: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    // Work out everything that needs fetching before fetching any of it, so the
    // progress bar has a real total instead of counting up from an unknown end.
    struct Pending {
        name: String,
        sha: String,
        download_url: String,
        is_new: bool,
    }

    let mut pending: Vec<Pending> = Vec::new();
    for entry in entries {
        if entry.get("type").and_then(|t| t.as_str()) != Some("file") {
            continue;
        }
        let Some(name) = entry.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !name.to_lowercase().ends_with(".png") {
            continue;
        }
        let sha = entry
            .get("sha")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let Some(download_url) = entry.get("download_url").and_then(|v| v.as_str()) else {
            continue;
        };

        let is_new = !cache_dir.join(name).exists();
        // Re-download when the repo copy changed under a name we already hold,
        // so a corrected portrait reaches users without a reinstall.
        let changed = !is_new && !sha.is_empty() && index.files.get(name) != Some(&sha);
        if !is_new && !changed {
            continue;
        }

        pending.push(Pending {
            name: name.to_string(),
            sha,
            download_url: download_url.to_string(),
            is_new,
        });
    }

    let total = pending.len();
    if total > 0 {
        ui_log::info(SCOPE, format!("Downloading {} hero icon(s)...", total));
        emit_progress(
            app,
            HeroSyncProgress {
                current: 0,
                total,
                percent: 0,
                message: format!("Downloading hero icons... 0/{}", total),
                done: false,
            },
        );
    }

    for (i, item) in pending.iter().enumerate() {
        let target = cache_dir.join(&item.name);

        match download_portrait(client, &item.download_url).await {
            Ok(bytes) => match std::fs::write(&target, &bytes) {
                Ok(_) => {
                    index.files.insert(item.name.clone(), item.sha.clone());
                    if item.is_new {
                        added.push(item.name.clone());
                    } else {
                        updated.push(item.name.clone());
                    }
                }
                Err(e) => {
                    ui_log::warn(SCOPE, format!("Could not save {}: {}", item.name, e));
                    failures.push(format!("write {}: {}", item.name, e))
                }
            },
            Err(e) => {
                ui_log::warn(SCOPE, format!("Could not download {}: {}", item.name, e));
                failures.push(format!("{}: {}", item.name, e))
            }
        }

        let current = i + 1;
        emit_progress(
            app,
            HeroSyncProgress {
                current,
                total,
                percent: ((current * 100) / total.max(1)) as u8,
                message: format!("Downloading hero icons... {}/{}", current, total),
                done: false,
            },
        );
    }

    // Only store the ETag when every portrait in the listing landed; otherwise
    // the next run would get a 304 and never retry the ones that failed.
    index.etag = if failures.is_empty() { etag } else { None };
    let _ = write_index(&index);

    let cached = count_cached();
    let message = if !failures.is_empty() {
        format!(
            "Synced {} portrait(s), {} failed: {}",
            added.len() + updated.len(),
            failures.len(),
            failures.join("; ")
        )
    } else if added.is_empty() && updated.is_empty() {
        "Hero portraits are up to date".to_string()
    } else {
        format!(
            "Hero portraits synced: {} new, {} updated",
            added.len(),
            updated.len()
        )
    };
    if failures.is_empty() {
        ui_log::success(SCOPE, format!("{} ({} cached)", message, cached));
    } else {
        ui_log::warn(
            SCOPE,
            format!(
                "{} icon(s) failed to sync - {} cached icon(s) still available",
                failures.len(),
                cached
            ),
        );
    }

    // Always close out a run that opened the progress bar, success or not, so
    // the drawer never keeps showing a stalled download.
    if total > 0 {
        emit_progress(
            app,
            HeroSyncProgress {
                current: total,
                total,
                percent: 100,
                message: message.clone(),
                done: true,
            },
        );
    }

    Ok(HeroSyncResult {
        cached,
        added,
        updated,
        checked: true,
        message,
    })
}

async fn download_portrait(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("download HTTP error: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("download body error: {}", e))?;
    Ok(bytes.to_vec())
}

/// Character ids of every cached portrait, e.g. `["1011", "9999"]`.
///
/// The frontend pairs this with `get_hero_image` to build one blob URL per
/// portrait: the bytes cross the IPC boundary raw, and the DOM only ever holds
/// a short `blob:` URL instead of a ~46 KB data URL per `<img>`.
#[tauri::command]
pub fn get_hero_image_ids() -> Result<Vec<String>, String> {
    Ok(read_ids_from(&hero_cache_dir()))
}

fn read_ids_from(cache_dir: &std::path::Path) -> Vec<String> {
    let entries = match std::fs::read_dir(cache_dir) {
        Ok(e) => e,
        // Nothing cached yet (first run, before the sync finishes).
        Err(_) => return Vec::new(),
    };

    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let is_png = path
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("png"))
                .unwrap_or(false);
            if !is_png {
                return None;
            }
            path.file_stem().and_then(|s| s.to_str()).map(String::from)
        })
        .collect();

    ids.sort();
    ids
}

/// Raw PNG bytes for one cached portrait.
///
/// Returned as an IPC `Response`, so the payload stays binary end to end and
/// arrives in the webview as an ArrayBuffer.
#[tauri::command]
pub fn get_hero_image(id: String) -> Result<tauri::ipc::Response, String> {
    // `id` comes from the frontend, so keep it to a bare file stem — never let
    // it walk out of the cache directory.
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("[Hero] Invalid portrait id: {}", id));
    }

    let path = hero_cache_dir().join(format!("{}.png", id));
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("[Hero] Failed to read portrait {}: {}", path.display(), e))?;

    Ok(tauri::ipc::Response::new(bytes))
}

/// Absolute path of the portrait cache, for the settings UI / troubleshooting.
#[tauri::command]
pub fn get_hero_cache_dir() -> Result<String, String> {
    Ok(hero_cache_dir().to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest valid PNG, enough to stand in for a portrait.
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn ids_are_the_character_ids_of_cached_portraits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("1011.png"), PNG).unwrap();
        std::fs::write(dir.path().join("9999.png"), PNG).unwrap();
        // Non-portrait files must be ignored.
        std::fs::write(dir.path().join("hero_cache.json"), b"{}").unwrap();

        let ids = read_ids_from(dir.path());

        assert_eq!(ids, vec!["1011".to_string(), "9999".to_string()]);
    }

    #[test]
    fn missing_cache_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(read_ids_from(&missing).is_empty());
    }

    #[test]
    fn portrait_ids_cannot_escape_the_cache_dir() {
        for bad in ["../vfx_settings", "..\\..\\secrets", "a/b", ""] {
            assert!(
                get_hero_image(bad.to_string()).is_err(),
                "id {:?} must be rejected",
                bad
            );
        }
    }

    #[test]
    fn index_round_trips() {
        let mut index = HeroCacheIndex::default();
        index.etag = Some("W/\"abc\"".to_string());
        index.files.insert("1011.png".to_string(), "sha1".to_string());

        let json = serde_json::to_string(&index).unwrap();
        let parsed: HeroCacheIndex = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.etag.as_deref(), Some("W/\"abc\""));
        assert_eq!(parsed.files.get("1011.png").map(String::as_str), Some("sha1"));
    }
}
