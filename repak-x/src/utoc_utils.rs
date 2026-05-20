use std::path::Path;

// Simplified file entry for mod file table display
#[derive(Clone, Debug)]
#[allow(dead_code)] // Fields are read via file_table.rs
pub struct UtocFileEntry {
    pub file_path: String,
    pub bulkdata_chunks: usize,
    pub packagedata_chunks: usize,
}

/// Read UTOC file list using UAssetTool (replaces retoc crate)
pub fn read_utoc(utoc_path: &Path) -> Vec<UtocFileEntry> {
    match try_read_utoc(utoc_path) {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!("Failed to read utoc {}: {}", utoc_path.display(), e);
            Vec::new()
        }
    }
}

pub fn try_read_utoc(utoc_path: &Path) -> Result<Vec<UtocFileEntry>, String> {
    // Use UAssetTool via uasset_toolkit to list IoStore files
    // Pass the Marvel Rivals AES key explicitly so obfuscated (encrypted) containers can be read
    let result = uasset_toolkit::list_iostore_files(
        utoc_path.to_string_lossy().as_ref(),
        Some("0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74"),
    )
    .map_err(|e| format!("Failed to read utoc: {}", e))?;

    // Convert to UtocFileEntry format, normalizing paths to remove /../ patterns
    let entries = result
        .files
        .iter()
        .map(|file_path| {
            let normalized = normalize_iostore_path(file_path);
            UtocFileEntry {
                file_path: normalized,
                bulkdata_chunks: 0,    // Not available from new API
                packagedata_chunks: 0, // Not available from new API
            }
        })
        .collect();

    Ok(entries)
}

/// Normalize IoStore paths to remove /../ patterns and convert to clean /Game/ paths
fn normalize_iostore_path(path: &str) -> String {
    let mut result = path.to_string();

    // Handle paths like "../../../Marvel_New/Content/Marvel/Characters/..."
    // These should become "/Game/Marvel/Characters/..."
    if result.contains("/../") {
        // Split by / and resolve .. segments
        let parts: Vec<&str> = result.split('/').collect();
        let mut resolved: Vec<&str> = Vec::new();

        for part in parts {
            if part == ".." {
                resolved.pop();
            } else if !part.is_empty() && part != "." {
                resolved.push(part);
            }
        }

        result = format!("/{}", resolved.join("/"));

        // Convert /Marvel_New/Content/... or /Marvel/Content/... to /Game/...
        if let Some(content_idx) = result.find("/Content/") {
            result = format!("/Game{}", &result[content_idx + "/Content".len()..]);
        }
    }

    result
}
