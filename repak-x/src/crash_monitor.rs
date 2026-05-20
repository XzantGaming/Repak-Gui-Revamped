// Crash Monitor - Detects Marvel Rivals crashes and provides crash information
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashInfo {
    pub crash_folder: PathBuf,
    pub timestamp: SystemTime,
    pub error_message: Option<String>,
    pub crash_type: Option<String>,
    pub seconds_since_start: Option<u64>,
    pub process_id: Option<u32>,
    pub enabled_mods: Vec<String>,
}

/// Get the path to Marvel Rivals crash logs directory
pub fn get_crash_log_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let local_appdata = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            // Fallback to default Windows path
            let userprofile = std::env::var("USERPROFILE").unwrap_or_default();
            format!("{}\\AppData\\Local", userprofile)
        });

        PathBuf::from(local_appdata)
            .join("Marvel")
            .join("Saved")
            .join("Crashes")
    }

    #[cfg(target_os = "linux")]
    {
        // Marvel Rivals on Linux runs via Steam Proton
        // Crash logs are in the Proton prefix for the game (AppID: 2767030)
        if let Some(home) = dirs::home_dir() {
            // Try common Steam library locations
            let steam_paths = [
                home.join(".steam/steam"),
                home.join(".local/share/Steam"),
                home.join("SteamLibrary"),
            ];

            for steam_path in &steam_paths {
                let crash_path = steam_path
                    .join("steamapps/compatdata/2767030/pfx/drive_c/users/steamuser/AppData/Local/Marvel/Saved/Crashes");
                if crash_path.exists() {
                    return crash_path;
                }
            }

            // Default to first path even if it doesn't exist yet
            return steam_paths[0]
                .join("steamapps/compatdata/2767030/pfx/drive_c/users/steamuser/AppData/Local/Marvel/Saved/Crashes");
        }

        // Fallback if home dir not found
        PathBuf::from("/home")
            .join(std::env::var("USER").unwrap_or_else(|_| "user".to_string()))
            .join(".steam/steam/steamapps/compatdata/2767030/pfx/drive_c/users/steamuser/AppData/Local/Marvel/Saved/Crashes")
    }

    #[cfg(target_os = "macos")]
    {
        // macOS - Marvel Rivals may run via CrossOver or similar
        // TODO: Determine actual path when macOS support is added
        if let Some(home) = dirs::home_dir() {
            home.join("Library/Application Support/Marvel/Saved/Crashes")
        } else {
            PathBuf::from("/Users")
                .join(std::env::var("USER").unwrap_or_else(|_| "user".to_string()))
                .join("Library/Application Support/Marvel/Saved/Crashes")
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        // Fallback for other platforms
        PathBuf::new()
    }
}

/// Check for new crash folders created after a specific time
pub fn check_for_new_crashes(since: SystemTime) -> Vec<PathBuf> {
    let crash_dir = get_crash_log_path();

    if !crash_dir.exists() {
        debug!("Crash directory does not exist: {:?}", crash_dir);
        return Vec::new();
    }

    let mut new_crashes = Vec::new();

    match fs::read_dir(&crash_dir) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();

                // Only check directories
                if !path.is_dir() {
                    continue;
                }

                // Check if folder was created after the specified time
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(created) = metadata.created() {
                        if created > since {
                            info!("New crash detected: {:?}", path);
                            new_crashes.push(path);
                        }
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to read crash directory: {}", e);
        }
    }

    new_crashes
}

/// Parse crash information from a crash folder
pub fn parse_crash_info(crash_folder: &Path, enabled_mods: Vec<String>) -> Option<CrashInfo> {
    let crash_context_path = crash_folder.join("CrashContext.runtime-xml");

    if !crash_context_path.exists() {
        warn!("CrashContext.runtime-xml not found in {:?}", crash_folder);
        return None;
    }

    let timestamp = fs::metadata(crash_folder)
        .and_then(|m| m.created())
        .unwrap_or_else(|_| SystemTime::now());

    // Try to read and parse the XML file
    let xml_content = match fs::read_to_string(&crash_context_path) {
        Ok(content) => {
            // Validate XML content size to prevent memory issues
            if content.len() > 10_000_000 {
                // 10MB limit
                warn!(
                    "CrashContext.runtime-xml file too large ({}B), skipping detailed parsing",
                    content.len()
                );
                return Some(CrashInfo {
                    crash_folder: crash_folder.to_path_buf(),
                    timestamp,
                    error_message: Some("Crash file too large to parse".to_string()),
                    crash_type: None,
                    seconds_since_start: None,
                    process_id: None,
                    enabled_mods,
                });
            }
            content
        }
        Err(e) => {
            warn!("Failed to read CrashContext.runtime-xml: {}", e);
            return Some(CrashInfo {
                crash_folder: crash_folder.to_path_buf(),
                timestamp,
                error_message: None,
                crash_type: None,
                seconds_since_start: None,
                process_id: None,
                enabled_mods,
            });
        }
    };

    // Simple XML parsing (extract key values)
    let error_message = extract_xml_tag(&xml_content, "ErrorMessage");
    let crash_type = extract_xml_tag(&xml_content, "CrashType");
    let seconds_since_start =
        extract_xml_tag(&xml_content, "SecondsSinceStart").and_then(|s| s.parse::<u64>().ok());
    let process_id = extract_xml_tag(&xml_content, "ProcessId").and_then(|s| s.parse::<u32>().ok());

    info!(
        "Parsed crash info - Error: {:?}, Type: {:?}, Time: {:?}s",
        error_message, crash_type, seconds_since_start
    );

    Some(CrashInfo {
        crash_folder: crash_folder.to_path_buf(),
        timestamp,
        error_message,
        crash_type,
        seconds_since_start,
        process_id,
        enabled_mods,
    })
}

/// Extract a value from an XML tag (simple parser, not full XML)
/// Includes validation to prevent crashes from malformed XML
fn extract_xml_tag(xml: &str, tag_name: &str) -> Option<String> {
    // Validate input parameters
    if xml.is_empty() || tag_name.is_empty() {
        return None;
    }

    let start_tag = format!("<{}>", tag_name);
    let end_tag = format!("</{}>", tag_name);

    if let Some(start_pos) = xml.find(&start_tag) {
        let content_start = start_pos + start_tag.len();

        // Validate bounds to prevent panics
        if content_start >= xml.len() {
            warn!(
                "XML parsing error: invalid content start position for tag '{}'",
                tag_name
            );
            return None;
        }

        if let Some(end_pos) = xml[content_start..].find(&end_tag) {
            let content_end = content_start + end_pos;

            // Additional bounds check
            if content_end > xml.len() {
                warn!(
                    "XML parsing error: invalid content end position for tag '{}'",
                    tag_name
                );
                return None;
            }

            let value = xml[content_start..content_end].trim();

            // Don't return empty values
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Extract specific crash details from error message
/// Returns (asset_path, error_type, details)
pub fn parse_error_details(
    error_message: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut asset_path = None;
    let mut error_type = None;
    let mut details = None;

    // Extract asset path (e.g., /Game/Marvel/Characters/1033/1033001/...)
    if let Some(game_pos) = error_message.find("/Game/") {
        if let Some(space_pos) = error_message[game_pos..].find(' ') {
            asset_path = Some(error_message[game_pos..game_pos + space_pos].to_string());
        }
    }

    // Extract error type (e.g., "ObjectSerializationError", "Serial size mismatch")
    if error_message.contains("ObjectSerializationError") {
        error_type = Some("ObjectSerializationError".to_string());
    } else if error_message.contains("EXCEPTION_ACCESS_VIOLATION") {
        error_type = Some("Access Violation".to_string());
    } else if error_message.contains("Assertion failed") {
        error_type = Some("Assertion Failed".to_string());
    }

    // Extract serial size mismatch details
    if let Some(mismatch_pos) = error_message.find("Serial size mismatch:") {
        if let Some(end_pos) = error_message[mismatch_pos..]
            .find('\n')
            .or(Some(error_message.len() - mismatch_pos))
        {
            details = Some(error_message[mismatch_pos..mismatch_pos + end_pos].to_string());
        }
    }

    (asset_path, error_type, details)
}

/// Check if crash is related to mesh/asset loading
pub fn is_mesh_related_crash(error_message: &str) -> bool {
    error_message.contains("StaticMesh")
        || error_message.contains("SM_")
        || error_message.contains("Serial size mismatch")
        || error_message.contains("ObjectSerializationError")
}

/// Extract character ID from error message (e.g., "1033" from path)
pub fn extract_character_id(error_message: &str) -> Option<String> {
    // Look for pattern like /Characters/1033/ or /1033/
    if let Some(chars_pos) = error_message.find("/Characters/") {
        let after_chars = &error_message[chars_pos + 12..]; // Skip "/Characters/"
        if let Some(slash_pos) = after_chars.find('/') {
            let potential_id = &after_chars[..slash_pos];
            // Check if it's a 4-digit number starting with 10
            if potential_id.len() == 4 && potential_id.starts_with("10") {
                return Some(potential_id.to_string());
            }
        }
    }
    None
}

/// Count total number of crash folders
pub fn count_total_crashes() -> usize {
    let crash_dir = get_crash_log_path();

    if !crash_dir.exists() {
        return 0;
    }

    match fs::read_dir(&crash_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .count(),
        Err(e) => {
            warn!("Failed to count crash folders: {}", e);
            0
        }
    }
}

/// Delete all crash folders (cleanup)
pub fn clear_all_crashes() -> Result<usize, String> {
    let crash_dir = get_crash_log_path();

    if !crash_dir.exists() {
        return Ok(0);
    }

    let mut deleted_count = 0;

    match fs::read_dir(&crash_dir) {
        Ok(entries) => {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();

                if path.is_dir() {
                    match fs::remove_dir_all(&path) {
                        Ok(_) => {
                            deleted_count += 1;
                            info!("Deleted crash folder: {:?}", path);
                        }
                        Err(e) => {
                            error!("Failed to delete crash folder {:?}: {}", path, e);
                        }
                    }
                }
            }
            Ok(deleted_count)
        }
        Err(e) => Err(format!("Failed to read crash directory: {}", e)),
    }
}

/// Get the newest crash folder in the crash directory
/// Returns (folder_name, folder_path) if found
pub fn get_newest_crash_folder() -> Option<(String, PathBuf)> {
    let crash_dir = get_crash_log_path();

    if !crash_dir.exists() {
        return None;
    }

    let mut newest: Option<(String, PathBuf, SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(&crash_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(created) = metadata.created() {
                    let folder_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();

                    match &newest {
                        Some((_, _, newest_time)) if created > *newest_time => {
                            newest = Some((folder_name, path, created));
                        }
                        None => {
                            newest = Some((folder_name, path, created));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    newest.map(|(name, path, _)| (name, path))
}

/// Check for crashes that occurred in previous sessions (when app wasn't running)
/// Returns crash info if a new crash is detected since last_known_crash_folder
pub fn check_for_previous_session_crash(
    last_known_crash_folder: Option<&str>,
) -> Option<CrashInfo> {
    let newest = get_newest_crash_folder();

    match (newest, last_known_crash_folder) {
        (Some((newest_name, newest_path)), Some(last_known)) => {
            // Compare folder names - if different, there's a new crash
            if newest_name != last_known {
                info!(
                    "New crash detected from previous session: {} (last known: {})",
                    newest_name, last_known
                );
                return parse_crash_info(&newest_path, Vec::new());
            }
            None
        }
        (Some((newest_name, newest_path)), None) => {
            // First time running - check if crash folder exists
            // Only report if the crash is recent (within last 24 hours)
            if let Ok(metadata) = fs::metadata(&newest_path) {
                if let Ok(created) = metadata.created() {
                    let age = SystemTime::now()
                        .duration_since(created)
                        .unwrap_or_default();
                    if age.as_secs() < 86400 {
                        // 24 hours
                        info!("Recent crash detected on first run: {}", newest_name);
                        return parse_crash_info(&newest_path, Vec::new());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_xml_tag() {
        let xml = r#"<Root><ErrorMessage>Test Error</ErrorMessage></Root>"#;
        assert_eq!(
            extract_xml_tag(xml, "ErrorMessage"),
            Some("Test Error".to_string())
        );
    }

    #[test]
    fn test_crash_log_path() {
        let path = get_crash_log_path();
        assert!(path.to_string_lossy().contains("Marvel"));
        assert!(path.to_string_lossy().contains("Crashes"));
    }
}
