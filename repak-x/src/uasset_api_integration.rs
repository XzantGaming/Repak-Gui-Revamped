use log::{error, info};
use std::path::Path;

// ============================================================================
// TEXTURE MIPMAP STRIPPING IMPLEMENTATION
// ============================================================================
// The batch_convert_textures_to_inline function is the primary entry point.
// It processes multiple textures in a single UAssetTool call for performance.
//
// The single-file functions below are kept for compatibility but are not used
// in the main workflow anymore.
// ============================================================================

/// Integration module for texture processing
/// Supports both native Rust and Python (UE4-DDS-Tools) implementations

/// Convert texture to inline format by stripping mipmaps.
/// This removes all mipmaps except the first one and embeds the data in .uexp,
/// eliminating the need for .ubulk files.
///
/// NOTE: This single-file function is deprecated. Use batch_convert_textures_to_inline instead.
#[allow(dead_code)]
const TEXTURE_IMPLEMENTATION: &str = "csharp";

#[allow(dead_code)]
pub fn convert_texture_to_inline(uasset_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    match TEXTURE_IMPLEMENTATION {
        "csharp" => convert_texture_to_inline_csharp(uasset_path),
        "rust" => convert_texture_to_inline_rust(uasset_path),
        _ => convert_texture_to_inline_python(uasset_path), // default to python
    }
}

/// Native C# implementation using UAssetAPI TextureExport via UAssetTool
#[allow(dead_code)]
fn convert_texture_to_inline_csharp(
    uasset_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    use uasset_toolkit::UAssetToolkitSync;

    info!(
        "[C#] Stripping mipmaps using UAssetAPI TextureExport: {:?}",
        uasset_path
    );

    // Get USMAP path from environment variable
    let usmap_path = std::env::var("USMAP_PATH").ok();

    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            let path_str = uasset_path.to_string_lossy();

            // Use the new strip_mipmaps_native action with usmap_path
            match toolkit.strip_mipmaps_native(&path_str, usmap_path.as_deref()) {
                Ok(true) => {
                    info!("[C#] Successfully stripped mipmaps: {:?}", uasset_path);
                    Ok(true)
                }
                Ok(false) => {
                    info!(
                        "[C#] Texture already has 1 mipmap or not a texture: {:?}",
                        uasset_path
                    );
                    Ok(false)
                }
                Err(e) => {
                    error!("[C#] Failed to strip mipmaps from {:?}: {}", uasset_path, e);
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("[C#] Failed to initialize UAssetToolkit: {}", e);
            Err(e.into())
        }
    }
}

/// Native C# implementation with explicit usmap_path parameter
#[allow(dead_code)]
fn convert_texture_to_inline_csharp_with_usmap(
    uasset_path: &Path,
    usmap_path: Option<&str>,
) -> Result<bool, Box<dyn std::error::Error>> {
    use uasset_toolkit::UAssetToolkitSync;

    info!(
        "[C#] Stripping mipmaps using UAssetAPI TextureExport: {:?}",
        uasset_path
    );

    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            let path_str = uasset_path.to_string_lossy();

            // Use the strip_mipmaps_native action with usmap_path
            match toolkit.strip_mipmaps_native(&path_str, usmap_path) {
                Ok(true) => {
                    info!("[C#] Successfully stripped mipmaps: {:?}", uasset_path);
                    Ok(true)
                }
                Ok(false) => {
                    info!(
                        "[C#] Texture already has 1 mipmap or not a texture: {:?}",
                        uasset_path
                    );
                    Ok(false)
                }
                Err(e) => {
                    error!("[C#] Failed to strip mipmaps from {:?}: {}", uasset_path, e);
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("[C#] Failed to initialize UAssetToolkit: {}", e);
            Err(e.into())
        }
    }
}

/// Native Rust implementation - REMOVED (uasset-texture-patch crate had issues)
#[allow(dead_code)]
fn convert_texture_to_inline_rust(_uasset_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    error!(
        "[Rust] Rust texture implementation has been removed. Use 'csharp' or 'python' instead."
    );
    Err("Rust texture implementation removed".into())
}

/// Python implementation using UE4-DDS-Tools via UAssetToolkit
#[allow(dead_code)]
fn convert_texture_to_inline_python(
    uasset_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    use uasset_toolkit::UAssetToolkitSync;

    info!(
        "[Python] Converting texture using UE4-DDS-Tools: {:?}",
        uasset_path
    );

    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            let path_str = uasset_path.to_string_lossy();

            // Use the convert_texture method which calls UE4-DDS-Tools
            match toolkit.convert_texture(&path_str) {
                Ok(true) => {
                    info!("[Python] Successfully converted texture: {:?}", uasset_path);
                    Ok(true)
                }
                Ok(false) => {
                    info!(
                        "[Python] Texture conversion returned false for: {:?}",
                        uasset_path
                    );
                    Ok(false)
                }
                Err(e) => {
                    error!(
                        "[Python] Failed to convert texture {:?}: {}",
                        uasset_path, e
                    );
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            error!("[Python] Failed to initialize UAssetToolkit: {}", e);
            Err(e.into())
        }
    }
}

/// Batch convert multiple textures to inline format by stripping mipmaps.
/// Uses the global UAssetToolkit singleton and sends ALL files in a single batch request
/// for optimal performance - no repeated process spawning.
///
/// Returns (success_count, skip_count, error_count, processed_file_names)
pub fn batch_convert_textures_to_inline(
    uasset_paths: &[std::path::PathBuf],
) -> Result<(usize, usize, usize, Vec<String>), Box<dyn std::error::Error>> {
    batch_convert_textures_to_inline_with_parallel(uasset_paths, false)
}

/// Batch convert multiple textures to inline format with optional parallel processing.
/// When parallel=true, uses multi-threaded processing in UAssetTool for faster batch operations.
///
/// Returns (success_count, skip_count, error_count, processed_file_names)
pub fn batch_convert_textures_to_inline_with_parallel(
    uasset_paths: &[std::path::PathBuf],
    parallel: bool,
) -> Result<(usize, usize, usize, Vec<String>), Box<dyn std::error::Error>> {
    if uasset_paths.is_empty() {
        info!("[C#] No textures to process, returning early");
        return Ok((0, 0, 0, Vec::new()));
    }

    // Get USMAP path from environment variable
    let usmap_path = std::env::var("USMAP_PATH").ok();
    info!("[C#] Batch stripping mipmaps for {} textures using global UAssetTool singleton (parallel={}, USMAP: {:?})", 
          uasset_paths.len(), parallel, usmap_path);

    // Convert PathBuf to String for the batch API
    let file_paths: Vec<String> = uasset_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    info!(
        "[C#] Calling batch_strip_mipmaps_native_parallel with {} files...",
        file_paths.len()
    );

    // Use the global singleton batch function with parallel option
    // This is much faster than creating a new process for each file
    match uasset_toolkit::batch_strip_mipmaps_native_parallel(
        &file_paths,
        usmap_path.as_deref(),
        parallel,
    ) {
        Ok((success_count, skip_count, error_count, processed_files)) => {
            info!(
                "[C#] Batch complete: {} stripped, {} skipped, {} errors",
                success_count, skip_count, error_count
            );
            Ok((success_count, skip_count, error_count, processed_files))
        }
        Err(e) => {
            error!("[C#] Batch strip mipmaps failed: {}", e);
            Err(e.into())
        }
    }
}

/// Processes texture files using UAssetAPI toolkit for MipGenSettings modification
/// (Legacy function - kept for compatibility but prefer convert_texture_to_inline)
#[allow(dead_code)]
pub fn process_texture_with_uasset_api(
    uasset_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    use uasset_toolkit::UAssetToolkitSync;

    info!(
        "Processing texture with UAssetAPI toolkit: {:?}",
        uasset_path
    );

    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            let path_str = uasset_path.to_string_lossy();

            match toolkit.is_texture_uasset(&path_str) {
                Ok(is_texture) => {
                    if is_texture {
                        match toolkit.set_no_mipmaps(&path_str) {
                            Ok(()) => {
                                info!(
                                    "UAssetAPI toolkit successfully set NoMipmaps: {:?}",
                                    uasset_path
                                );
                                return Ok(true);
                            }
                            Err(e) => {
                                error!("Failed to set NoMipmaps for {:?}: {}", uasset_path, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "UAssetAPI is_texture_uasset failed for {:?}: {}",
                        uasset_path, e
                    );
                }
            }
        }
        Err(e) => {
            error!("Failed to initialize UAssetAPI toolkit: {}", e);
        }
    }

    Ok(false)
}
