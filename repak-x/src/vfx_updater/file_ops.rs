//! VFX Updater - File Operations Utilities

use std::fs;
use std::path::{Path, PathBuf};

use super::logging::{vfx_debug, vfx_info};

/// Get the base temp directory for VFX operations: %TEMP%/repak-x
pub fn get_vfx_temp_base() -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let vfx_base = temp_dir.join("repak-x");

    fs::create_dir_all(&vfx_base)
        .map_err(|e| format!("[VFX] Failed to create temp base directory: {}", e))?;

    vfx_debug(&format!("Temp base directory: {}", vfx_base.display()));
    Ok(vfx_base)
}

/// Create a step-specific temp directory: %TEMP%/repak-x/rvfx_{step_name}
pub fn create_step_directory(step_name: &str) -> Result<PathBuf, String> {
    let base = get_vfx_temp_base()?;
    let step_dir = base.join(format!("rvfx_{}", step_name));

    // Clean up if it already exists
    if step_dir.exists() {
        fs::remove_dir_all(&step_dir)
            .map_err(|e| format!("[VFX] Failed to clean existing step directory: {}", e))?;
    }

    fs::create_dir_all(&step_dir)
        .map_err(|e| format!("[VFX] Failed to create step directory: {}", e))?;

    vfx_info(&format!("Created step directory: {}", step_dir.display()));
    Ok(step_dir)
}

/// Clean up all rvfx_* directories inside repak-x temp folder
pub fn cleanup_vfx_temp_directories() -> Result<(), String> {
    let base = get_vfx_temp_base()?;

    let entries = fs::read_dir(&base)
        .map_err(|e| format!("[VFX] Failed to read temp base directory: {}", e))?;

    let mut cleaned = 0;
    for entry in entries {
        let entry = entry.map_err(|e| format!("[VFX] Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("rvfx_") {
                    fs::remove_dir_all(&path)
                        .map_err(|e| format!("[VFX] Failed to remove {}: {}", path.display(), e))?;
                    vfx_debug(&format!("Cleaned up: {}", path.display()));
                    cleaned += 1;
                }
            }
        }
    }

    vfx_debug(&format!("Cleaned up {} temp directories", cleaned));
    Ok(())
}

/// Read a JSON file and return its contents as a string
/// Strips UTF-8 BOM if present (UAssetTool sometimes outputs files with BOM)
pub fn read_json_file(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("[VFX] Failed to read JSON file {}: {}", path, e))?;

    // Strip UTF-8 BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    Ok(content.to_string())
}

/// Write content to a JSON file
pub fn write_json_file(path: &str, content: &str) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("[VFX] Failed to create parent directory: {}", e))?;
    }

    fs::write(path, content).map_err(|e| format!("[VFX] Failed to write JSON file {}: {}", path, e))
}

/// List all JSON files in a directory recursively
pub fn list_json_files(dir: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    list_json_files_recursive(Path::new(dir), &mut files)?;
    Ok(files)
}

fn list_json_files_recursive(dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                list_json_files_recursive(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "json") {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}

/// Copy a file along with its companion files (.uexp, .ubulk, .uptnl)
pub fn copy_uasset_with_companions(src: &Path, dst_dir: &Path) -> Result<(), String> {
    let stem = src
        .file_stem()
        .ok_or_else(|| format!("[VFX] No file stem for: {}", src.display()))?;
    let parent = src
        .parent()
        .ok_or_else(|| format!("[VFX] No parent directory for: {}", src.display()))?;

    fs::create_dir_all(dst_dir)
        .map_err(|e| format!("[VFX] Failed to create destination directory: {}", e))?;

    let extensions = ["uasset", "uexp", "ubulk", "uptnl"];

    for ext in &extensions {
        let src_file = parent.join(format!("{}.{}", stem.to_string_lossy(), ext));
        if src_file.exists() {
            let dst_file = dst_dir.join(format!("{}.{}", stem.to_string_lossy(), ext));
            fs::copy(&src_file, &dst_file)
                .map_err(|e| format!("[VFX] Failed to copy {}: {}", src_file.display(), e))?;
            vfx_debug(&format!(
                "Copied: {} -> {}",
                src_file.display(),
                dst_file.display()
            ));
        }
    }

    Ok(())
}

/// Get the UAssetTool executable path (bundled with app)
/// Uses the same search logic as install_mod to work in both dev and release builds
pub fn get_uasset_tool_path(_app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    use crate::install_mod::install_mod_logic::iotoc::find_uasset_tool;

    let tool_path = find_uasset_tool().map_err(|e| format!("[VFX] {}", e))?;

    super::logging::vfx_info(&format!("UAssetTool path: {}", tool_path.display()));
    Ok(tool_path)
}
