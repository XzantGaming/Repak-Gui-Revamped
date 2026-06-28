use std::path::PathBuf;
use tauri::{State, Window, Emitter};
use crate::AppState;
use std::sync::{Arc, Mutex};

#[tauri::command]
pub async fn merge_mods_for_hybrid(
    path1: String,
    path2: String,
    state: State<'_, Arc<Mutex<AppState>>>,
    window: Window,
) -> Result<Vec<crate::InstallableModInfo>, String> {
    let _ = window.emit("install_log", "[Merge] Starting hybrid merge process...");
    
    // Create a temp directory inside the app's temp folder
    let temp_base = crate::app_dir().join("temp");
    let _ = std::fs::create_dir_all(&temp_base);
    let temp_dir = tempfile::Builder::new()
        .prefix("hybrid_merge_")
        .tempdir_in(&temp_base)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;
        
    let temp_path = temp_dir.path().to_path_buf();
    // We must persist the temp dir since we'll return its path for installation later
    let persisted_temp_path = temp_dir.into_path();
    
    let paths = vec![PathBuf::from(path1), PathBuf::from(path2)];
    
    for p in paths {
        let _ = window.emit("install_log", format!("[Merge] Processing: {}", p.display()));
        if p.is_dir() {
            // Copy directory contents recursively to ensure we merge instead of replacing
            if let Err(e) = copy_dir_recursively(&p, &persisted_temp_path) {
                return Err(format!("Failed to merge folder contents: {}", e));
            }
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) {
            if ext == "pak" {
                // Extract pak
                let msg = format!("[Merge] Extracting PAK file: {} to {}", p.display(), persisted_temp_path.display());
                let _ = window.emit("install_log", &msg);
                match uasset_toolkit::extract_pak_all(
                    p.to_str().unwrap_or_default(),
                    persisted_temp_path.to_str().unwrap_or_default(),
                    Some(crate::install_mod::AES_KEY_HEX)
                ) {
                    Ok(count) => {
                        let msg = format!("[Merge] Extracted {} files from PAK.", count);
                        let _ = window.emit("install_log", &msg);
                    }
                    Err(e) => {
                        let msg = format!("[Merge] Error extracting PAK: {}", e);
                        let _ = window.emit("install_log", &msg);
                        return Err(format!("Failed to extract PAK: {}", e));
                    }
                }
                
                // Verify files exist after extraction
                let extracted_count = walkdir::WalkDir::new(&persisted_temp_path).into_iter().count();
                let _ = window.emit("install_log", format!("[Merge] Files in temp dir after extraction: {}", extracted_count));
            } else if ext == "zip" || ext == "rar" || ext == "7z" {
                // Return error since we only support merging uncompressed folders and .pak files for now
                return Err("Cannot merge compressed archives directly. Extract them first.".to_string());
            }
        }
    }
    
    let _ = window.emit("install_log", "[Merge] Merge complete, parsing new folder...");
    
    // Parse the merged directory
    let parse_result = crate::parse_dropped_files(
        vec![persisted_temp_path.to_string_lossy().to_string()],
        state,
        window
    ).await?;
    
    Ok(parse_result)
}

fn copy_dir_recursively(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type();
        let relative_path = entry.path().strip_prefix(src).map_err(|e| e.to_string())?;
        let dest_path = dst.join(relative_path);

        if ty.is_dir() {
            std::fs::create_dir_all(&dest_path).map_err(|e| e.to_string())?;
        } else if ty.is_file() {
            if let Some(parent) = dest_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(entry.path(), &dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
