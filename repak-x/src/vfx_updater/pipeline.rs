//! VFX Updater - Pipeline Step Orchestration
//!
//! High-level pipeline functions that coordinate the VFX update workflow.

use std::path::Path;

use super::file_ops::{cleanup_vfx_temp_directories, create_step_directory};
use super::logging::vfx_info;
use super::models::{AssetClassInfo, VfxPipelineProgress, VfxTempDirectories};
use super::progress::VfxProgressSink;
use super::uasset_tool::{
    convert_json_to_uassets, convert_uassets_to_json, extract_mod_assets, extract_vanilla_assets,
    pack_to_iostore, run_vfx_uat_request, VfxUatRequest,
};

/// Create all temp directories for a pipeline run
pub fn create_pipeline_directories() -> Result<VfxTempDirectories, String> {
    let base = super::file_ops::get_vfx_temp_base()?;

    Ok(VfxTempDirectories {
        base: base.to_string_lossy().to_string(),
        mod_extract: create_step_directory("mod_extract")?
            .to_string_lossy()
            .to_string(),
        mod_json: create_step_directory("mod_json")?
            .to_string_lossy()
            .to_string(),
        vanilla_extract: create_step_directory("vanilla_extract")?
            .to_string_lossy()
            .to_string(),
        vanilla_json: create_step_directory("vanilla_json")?
            .to_string_lossy()
            .to_string(),
        edited_json: create_step_directory("edited_json")?
            .to_string_lossy()
            .to_string(),
        final_uassets: create_step_directory("final_uassets")?
            .to_string_lossy()
            .to_string(),
    })
}

/// Clean up all pipeline temp directories
pub fn cleanup_pipeline() -> Result<(), String> {
    cleanup_vfx_temp_directories()
}

/// Step 1: Extract mod assets from IOStore
pub async fn step_extract_mod_assets(
    tool_path: &Path,
    game_paks: &str,
    mod_path: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    vfx_info("=== Step 1: Extract Mod Assets ===");
    extract_mod_assets(tool_path, game_paks, mod_path, output_dir, progress).await
}

/// Step 2: Convert mod UAssets to JSON
pub async fn step_convert_mod_to_json(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    vfx_info("=== Step 2: Convert Mod UAssets to JSON ===");
    convert_uassets_to_json(tool_path, usmap_path, input_dir, output_dir, progress).await
}

/// Step 4: Extract vanilla assets matching mod asset paths
pub async fn step_extract_vanilla_assets(
    tool_path: &Path,
    game_paks: &str,
    output_dir: &str,
    filter_patterns: &[String],
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    vfx_info("=== Step 4: Extract Vanilla Assets ===");
    extract_vanilla_assets(tool_path, game_paks, output_dir, filter_patterns, progress).await
}

/// Step 7: Convert edited JSON back to UAssets
pub async fn step_convert_json_to_uassets(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    vfx_info("=== Step 7: Convert JSON to UAssets ===");
    convert_json_to_uassets(tool_path, usmap_path, input_dir, output_dir, progress).await
}

/// Step 8: Pack to IOStore bundle
pub async fn step_pack_to_iostore(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_base: &str,
    progress: &dyn VfxProgressSink,
) -> Result<String, String> {
    vfx_info("=== Step 8: Pack to IOStore ===");
    pack_to_iostore(tool_path, usmap_path, input_dir, output_base, progress).await
}

/// Detect asset class for a single asset
pub async fn detect_asset_class(
    tool_path: &Path,
    usmap_path: &str,
    file_path: &str,
) -> Result<AssetClassInfo, String> {
    vfx_info(&format!("Detecting asset class for: {}", file_path));

    let request = VfxUatRequest {
        action: "detect_class",
        file_path: Some(file_path.to_string()),
        file_paths: None,
        usmap_path: Some(usmap_path),
        output_path: None,
        filter: None,
        mount_point: None,
        base_path: None,
    };

    let response = run_vfx_uat_request(tool_path, &request).await?;

    if !response.success {
        return Err(format!("[VFX] detect_class failed: {}", response.message));
    }

    // Parse the class info from response data
    let class_name = response
        .data
        .as_ref()
        .and_then(|d| d.get("class_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let is_material_instance = class_name
        .as_ref()
        .map(|c| c.contains("MaterialInstance"))
        .unwrap_or(false);

    let is_niagara = class_name
        .as_ref()
        .map(|c| c.contains("Niagara") || c.starts_with("NS_"))
        .unwrap_or(false);

    let is_widget = class_name
        .as_ref()
        .map(|c| c.contains("Widget") || c.starts_with("WBP_"))
        .unwrap_or(false);

    Ok(AssetClassInfo {
        file_path: file_path.to_string(),
        class_name,
        is_material_instance,
        is_niagara,
        is_widget,
    })
}
