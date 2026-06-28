#![allow(dead_code)]
use crate::install_mod::install_mod_logic::pak_files::repak_dir;
#[allow(unused_imports)]
use crate::install_mod::InstallableMod;
use crate::utils::collect_files;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicI32;

// Windows-specific: Hide CMD windows when spawning processes
#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn convert_to_iostore_directory(
    pak: &InstallableMod,
    mod_dir: PathBuf,
    to_pak_dir: PathBuf,
    packed_files_count: &AtomicI32,
) -> Result<(), String> {
    let mod_type = pak.mod_type.clone();

    // Check for force_legacy_pak flag - skip IoStore conversion entirely
    if pak.force_legacy_pak {
        info!(
            "Force Legacy PAK enabled for '{}'. Skipping IoStore conversion.",
            pak.mod_name
        );
        repak_dir(pak, to_pak_dir, mod_dir, packed_files_count)
            .map_err(|e| format!("repak_dir failed: {}", e))?;
        return Ok(());
    }

    if mod_type == "Audio" || mod_type == "Movies" {
        debug!("{} mod detected. Not creating iostore packages", mod_type);
        repak_dir(pak, to_pak_dir, mod_dir, packed_files_count)
            .map_err(|e| format!("repak_dir failed: {}", e))?;
        return Ok(());
    }

    let mut pak_name = pak.mod_name.clone();
    pak_name.push_str(".pak");

    let mut utoc_name = pak.mod_name.clone();
    utoc_name.push_str(".utoc");

    let mut paths = vec![];
    collect_files(&mut paths, &to_pak_dir)
        .map_err(|e| format!("Failed to collect files: {}", e))?;

    // SerializeSize fixing, Skeletal Mesh patching, and texture processing are all
    // handled automatically by UAssetTool during IoStore conversion (ZenConverter.BuildExportMapWithRecalculatedSizes).
    let processed_textures: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Filter out temporary/backup files that should NOT be included in the IoStore package
    // This includes: .bak files (mesh patch backups), .temp files, patched_files cache,
    // and .ubulk files for textures that have been processed with NoMipmaps
    let original_count = paths.len();
    paths.retain(|p| {
        let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Check if this is a .ubulk file for a processed NoMipmaps texture
        // NOTE: Disabled for now - only exclude .ubulk if we're 100% sure conversion worked
        // Otherwise the texture will be broken (missing bulk data)
        let is_processed_ubulk = if ext == "ubulk" && !processed_textures.is_empty() {
            if let Some(stem) = p.file_stem() {
                let texture_base = stem.to_string_lossy().to_string();
                if processed_textures.contains(&texture_base) {
                    info!(
                        "Excluding .ubulk from packing for NoMipmaps texture: {}",
                        file_name
                    );
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        // Exclude backup files, temp files, patched_files cache, and .ubulk for NoMipmaps textures
        let should_exclude =
            ext == "bak" || ext == "temp" || file_name == "patched_files" || is_processed_ubulk;

        if should_exclude {
            debug!("Excluding from IoStore: {}", p.display());
        }

        !should_exclude
    });

    if paths.len() != original_count {
        info!(
            "Filtered {} files from IoStore conversion (temp/backup/.ubulk)",
            original_count - paths.len()
        );
    }

    // Log which files are being converted to IoStore
    info!("Converting {} files to IoStore", paths.len());
    for path in paths.iter().take(10) {
        if let Some(filename) = path.file_name() {
            let filename_str = filename.to_string_lossy();
            if filename_str.ends_with(".uexp")
                && processed_textures.iter().any(|t| filename_str.contains(t))
            {
                info!("  Including converted texture: {}", filename_str);
            }
        }
    }

    // Log file sizes before IoStore conversion
    if !processed_textures.is_empty() {
        info!("Checking converted texture files before IoStore conversion:");
        for texture_name in processed_textures.iter().take(5) {
            for path in paths.iter() {
                if let Some(filename) = path.file_name() {
                    if filename.to_string_lossy().contains(texture_name)
                        && path.extension() == Some(std::ffi::OsStr::new("uexp"))
                    {
                        if let Ok(metadata) = std::fs::metadata(path) {
                            info!(
                                "  {} - size: {} bytes",
                                filename.to_string_lossy(),
                                metadata.len()
                            );
                        }
                    }
                }
            }
        }
    }

    // Use UAssetTool to convert legacy assets to Zen format and create IoStore bundle
    // This replaces retoc's action_to_zen function
    let output_base = mod_dir.join(&pak.mod_name);

    info!("Converting to IoStore using UAssetTool...");
    info!("  Input directory: {}", to_pak_dir.display());
    info!("  Output base: {}", output_base.display());

    // parallel_processing toggle: false=50% threads, true=75% threads
    let result = uasset_toolkit::create_mod_iostore(
        &output_base.to_string_lossy(),
        &to_pak_dir.to_string_lossy(),
        Some(&pak.mount_point),
        Some(true),              // Enable compression
        None,                    // Use default AES key
        pak.parallel_processing, // Toggle: false=50%, true=75% CPU threads
        pak.obfuscate,           // Encrypt with game's AES key to block FModel extraction
        pak.hybrid,              // Hybrid: keep non-Unreal files in the companion PAK
    )
    .map_err(|e| format!("IoStore conversion failed: {}", e))?;

    info!("IoStore conversion complete:");
    info!("  UTOC: {}", result.utoc_path);
    info!("  UCAS: {}", result.ucas_path);
    info!("  PAK:  {}", result.pak_path);
    info!(
        "  Converted {} assets ({} files)",
        result.converted_count, result.file_count
    );

    packed_files_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Process texture files for NoMipmaps fix.
/// NOTE: Texture conversion is currently disabled - needs complete rewrite.
/// Returns an empty set since no textures are processed.
#[allow(dead_code)]
pub fn process_texture_files(
    _paths: &Vec<PathBuf>,
) -> Result<std::collections::HashSet<String>, Box<dyn std::error::Error>> {
    // Texture conversion disabled - return empty set
    info!("Texture conversion is currently disabled - needs complete rewrite");
    Ok(std::collections::HashSet::new())
}

// NOTE: All texture conversion functions removed - needs complete rewrite with different approach
// The previous implementation attempted manual binary patching which was unreliable
// Future implementation should consider:
// 1. Using Unreal Engine's UAT for proper cooking (requires UE installation)
// 2. Or finding a more robust binary format understanding
// 3. Or requiring mod creators to export textures with NoMipMaps setting

/// Asset type detection result from UAssetAPI tool
#[derive(Debug, Deserialize, Serialize)]
struct AssetDetectionResult {
    path: String,
    asset_type: String,
    export_count: usize,
    import_count: usize,
}

/// SerializeSize fix result from UAssetAPI tool
#[derive(Debug, Deserialize, Serialize)]
struct SerializeSizeFixResult {
    success: bool,
    message: String,
    fixed_count: Option<usize>,
    asset_type: Option<String>,
}

/// Find the UAssetTool - searches multiple locations
pub fn find_uasset_tool() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Try to find the tool relative to the executable first
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check in uassettool subdirectory (standard location from build.rs)
            let in_uassettool = exe_dir.join("uassettool").join("UAssetTool.exe");
            if in_uassettool.exists() {
                info!("Found UAssetTool: {}", in_uassettool.display());
                return Ok(in_uassettool);
            }

            // Check next to executable (for release builds)
            let next_to_exe = exe_dir.join("UAssetTool.exe");
            if next_to_exe.exists() {
                info!("Found UAssetTool next to exe: {}", next_to_exe.display());
                return Ok(next_to_exe);
            }
        }
    }

    // Relative paths for development (from workspace root during tauri dev)
    let relative_paths = [
        // From target directory (built by uasset_app)
        "target/release/uassettool/UAssetTool.exe",
        "target/debug/uassettool/UAssetTool.exe",
        // From workspace root
        "Repak_Gui-Revamped-TauriUpdate/uasset_toolkit/tools/UAssetTool/bin/Release/net8.0/win-x64/publish/UAssetTool.exe",
        "Repak_Gui-Revamped-TauriUpdate/uasset_toolkit/tools/UAssetTool/bin/Release/net8.0/win-x64/UAssetTool.exe",
        // From RepakX directory
        "../uasset_toolkit/tools/UAssetTool/bin/Release/net8.0/win-x64/publish/UAssetTool.exe",
        "../uasset_toolkit/tools/UAssetTool/bin/Release/net8.0/win-x64/UAssetTool.exe",
    ];

    for path in &relative_paths {
        let p = Path::new(path);
        if p.exists() {
            info!("Found UAssetTool at: {}", path);
            return Ok(p.to_path_buf());
        }
    }

    // Log current working directory to help debug
    if let Ok(cwd) = std::env::current_dir() {
        warn!("Current working directory: {}", cwd.display());
    }

    Err(
        "UAssetTool.exe not found in any search path. Make sure it's built with 'dotnet publish'."
            .into(),
    )
}

/// Detect asset type using UAssetAPI (no heuristics!)
fn detect_asset_type_with_uasset_api(
    uasset_path: &Path,
    usmap_path: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    // Get the tool path - try multiple locations
    let tool_path = find_uasset_tool()?;

    let mut cmd = Command::new(&tool_path);

    // Hide CMD window on Windows (CREATE_NO_WINDOW flag)
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    cmd.arg("detect").arg(uasset_path);

    // Add usmap path if provided
    if let Some(usmap) = usmap_path {
        cmd.arg(usmap);
        debug!(
            "Running: {} detect {:?} {}",
            tool_path.display(),
            uasset_path,
            usmap
        );
    } else {
        debug!("Running: {} detect {:?}", tool_path.display(), uasset_path);
    }

    let output = cmd.output()?;

    // ALWAYS log stderr for debugging
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        for line in stderr.lines() {
            info!("[C# Tool] {}", line);
        }
    }

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        error!("Tool failed:");
        error!("stdout: {}", stdout);
        error!("stderr: {}", stderr);
        return Err(format!("Asset detection failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    debug!("Tool output: {}", stdout);

    let result: AssetDetectionResult = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse tool output: {}. Output was: {}", e, stdout))?;

    Ok(result.asset_type)
}

/// Fix SerializeSize for Static Meshes using UAssetAPI (calculation only) + binary patching
fn fix_static_mesh_serializesize(
    uasset_path: &Path,
    usmap_path: Option<&str>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let tool_path = find_uasset_tool()?;

    let mut cmd = Command::new(&tool_path);

    // Hide CMD window on Windows (CREATE_NO_WINDOW flag)
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    cmd.arg("fix").arg(uasset_path);

    // Add usmap path if provided (REQUIRED for unversioned assets)
    if let Some(usmap) = usmap_path {
        cmd.arg(usmap);
    }

    let output = cmd.output()?;

    // Always log stderr for debugging
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        for line in stderr.lines() {
            debug!("[C# Tool] {}", line);
        }
    }

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        error!("Fix tool failed:");
        error!("stdout: {}", stdout);
        return Err(format!("SerializeSize fix failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let result: SerializeSizeFixResult = serde_json::from_str(&stdout).map_err(|e| {
        format!(
            "Failed to parse fix tool output: {}. Output was: {}",
            e, stdout
        )
    })?;

    if !result.success {
        return Ok(0);
    }

    let fixed_count = result.fixed_count.unwrap_or(0);
    if fixed_count == 0 {
        return Ok(0);
    }

    // Apply binary patches to the .uasset header ONLY
    // The C# tool calculates correct sizes from .uexp, Rust patches the .uasset header
    info!("Applying binary patches to .uasset header...");
    let fixes_json: serde_json::Value = serde_json::from_str(&stdout)?;
    if let Some(fixes_array) = fixes_json.get("fixes").and_then(|f| f.as_array()) {
        for fix in fixes_array {
            let old_size = fix
                .get("old_size")
                .and_then(|v| v.as_i64())
                .ok_or("Missing old_size")?;
            let new_size = fix
                .get("new_size")
                .and_then(|v| v.as_i64())
                .ok_or("Missing new_size")?;
            apply_binary_patch(uasset_path, old_size, new_size)?;
        }
    }

    Ok(fixed_count)
}

/// Apply a binary patch to replace old SerializeSize with new SerializeSize
/// This preserves the exact file structure that retoc expects
fn apply_binary_patch(
    uasset_path: &Path,
    old_size: i64,
    new_size: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Read the entire file
    let mut uasset_data = std::fs::read(uasset_path)?;

    // Search for the old SerializeSize value in the binary
    let search_bytes = old_size.to_le_bytes();
    let mut found_offset = None;

    for i in 0..uasset_data.len().saturating_sub(8) {
        if &uasset_data[i..i + 8] == &search_bytes {
            found_offset = Some(i);
            info!("Found old SerializeSize {} at offset {}", old_size, i);
            break;
        }
    }

    let offset = found_offset.ok_or(format!(
        "Could not find old SerializeSize {} in file",
        old_size
    ))?;

    // Patch the 8 bytes at that offset with the new value
    let new_bytes = new_size.to_le_bytes();
    uasset_data[offset..offset + 8].copy_from_slice(&new_bytes);

    // Write the file back
    std::fs::write(uasset_path, &uasset_data)?;

    info!(
        "Patched SerializeSize: {} → {} at offset {}",
        old_size, new_size, offset
    );

    Ok(())
}

/// Process Static Mesh .uasset files in a directory - fix SerializeSize for Static Meshes ONLY
/// Uses UAssetAPI to detect asset type before processing
fn process_static_mesh_serializesize(
    dir: &Path,
    usmap_path: Option<&str>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut total_fixed = 0;
    let mut uasset_files = Vec::new();

    // Collect all .uasset files
    collect_uasset_files(dir, &mut uasset_files)?;

    info!(
        "Found {} .uasset files to scan in: {:?}",
        uasset_files.len(),
        dir
    );

    if let Some(usmap) = usmap_path {
        info!("Using USmap file: {}", usmap);
    } else {
        warn!("No USmap file provided - processing may fail for unversioned assets!");
        return Ok(0);
    }

    // Filter to only Static Mesh files using UAssetAPI detection
    let mut static_mesh_files = Vec::new();
    for uasset_file in &uasset_files {
        let filename = uasset_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        // Detect asset type using UAssetAPI
        match detect_asset_type_with_uasset_api(uasset_file, usmap_path) {
            Ok(asset_type) => {
                if asset_type == "static_mesh" {
                    info!("Detected Static Mesh: {}", filename);
                    static_mesh_files.push(uasset_file.clone());
                } else {
                    debug!("Skipping {} (type: {})", filename, asset_type);
                }
            }
            Err(e) => {
                warn!("Failed to detect type for {}: {}", filename, e);
            }
        }
    }

    info!(
        "Found {} Static Mesh files to process",
        static_mesh_files.len()
    );

    if static_mesh_files.is_empty() {
        info!("✓ No Static Mesh files found - nothing to fix");
        return Ok(0);
    }

    // Process only Static Mesh files
    for uasset_file in &static_mesh_files {
        let filename = uasset_file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        info!("Processing Static Mesh: {}", filename);

        match fix_static_mesh_serializesize(uasset_file, usmap_path) {
            Ok(count) => {
                total_fixed += count;
                info!("Re-serialized ({} exports)", count);
            }
            Err(e) => {
                warn!("Failed to process: {}", e);
            }
        }
    }

    info!("Total Static Meshes processed: {}", total_fixed);

    Ok(total_fixed)
}

/// Recursively collect all .uasset files in a directory
fn collect_uasset_files(
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                collect_uasset_files(&path, files)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("uasset") {
                files.push(path);
            }
        }
    }
    Ok(())
}
