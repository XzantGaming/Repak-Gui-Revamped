//! VFX Updater - Isolated UAssetTool Interactive Session
//!
//! This module manages a completely separate UAssetTool process from Repak-X's
//! existing uasset_toolkit. It provides async functions for VFX pipeline operations.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::logging::{vfx_debug, vfx_error, vfx_info, vfx_warn};
use super::models::VfxPipelineProgress;
use super::progress::VfxProgressSink;

#[derive(Serialize, Debug, Default)]
pub struct VfxUatRequest<'a> {
    pub action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usmap_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_path: Option<String>,
    /// Base path for preserving relative directory structure in batch output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,

    // --- Container-op fields (extract_iostore_legacy / create_mod_iostore). The C#
    //     tool reads each from these exact JSON keys via its `ProcessRequest` dispatch. ---
    /// Game `Paks` directory, for `extract_iostore_legacy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_paks: Option<String>,
    /// Mod `.utoc`/directory to overlay, for `extract_iostore_legacy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_path: Option<String>,
    /// Asset path filters, for `extract_iostore_legacy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_patterns: Option<Vec<String>>,
    /// Input asset directory, for `create_mod_iostore`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_dir: Option<String>,
    /// Enable Oodle compression, for `create_mod_iostore` (default-on in the legacy CLI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compress: Option<bool>,
    /// Extract dependencies recursively, for `extract_iostore_legacy`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_deps: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct VfxUatResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

// The VFX pipeline used to drive an isolated child `UAssetTool.exe` over a stdin/stdout
// JSON line protocol. The tool now ships as an in-process NativeAOT library, so requests
// go through the shared `uasset_toolkit` FFI (`invoke_json`). The JSON request/response
// envelope is unchanged — only the transport differs (no child process, no exe on disk).

/// Ensure the in-process UAssetTool library is loaded. Cheap and idempotent: the global
/// toolkit is initialized once and kept for the process lifetime. The `_tool_path`
/// argument is retained for call-site compatibility but is unused — the native library
/// resolves itself next to the executable.
pub async fn ensure_vfx_uat_session(_tool_path: &Path) -> Result<(), String> {
    tokio::task::spawn_blocking(uasset_toolkit::init_global_toolkit)
        .await
        .map_err(|e| format!("[VFX] UAT init task failed: {}", e))?
        .map_err(|e| format!("[VFX] Failed to load UAssetTool library: {}", e))?;
    vfx_debug("UAT in-process library ready");
    Ok(())
}

/// No-op retained for API symmetry with the old child-process session. There is no
/// process to tear down — the in-process library lives for the process lifetime.
pub async fn close_vfx_uat_session() {
    vfx_debug("UAT session close requested (in-process library; nothing to tear down)");
}

/// Send one JSON request through the in-process UAssetTool FFI and parse the response.
/// The native call is synchronous, so it runs on a blocking thread to avoid stalling the
/// async runtime. The `_tool_path` argument is retained for call-site compatibility.
pub async fn run_vfx_uat_request(
    _tool_path: &Path,
    request: &VfxUatRequest<'_>,
) -> Result<VfxUatResponse, String> {
    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("[VFX] Failed to serialize request: {}", e))?;
    vfx_debug(&format!("UAT request: {}", request_json));

    let response_json =
        tokio::task::spawn_blocking(move || uasset_toolkit::invoke_json(&request_json))
            .await
            .map_err(|e| format!("[VFX] UAT request task failed: {}", e))?
            .map_err(|e| format!("[VFX] UAT invoke failed: {}", e))?;

    let response: VfxUatResponse = serde_json::from_str(&response_json).map_err(|e| {
        format!(
            "[VFX] Failed to parse UAT response: {} (response: {})",
            e, response_json
        )
    })?;

    vfx_debug(&format!(
        "UAT response: success={}, message={}",
        response.success, response.message
    ));
    if let Some(data) = &response.data {
        vfx_debug(&format!("UAT response data: {}", data));
    }
    Ok(response)
}

fn find_uassets_recursive(
    dir: &Path,
    base_dir: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                find_uassets_recursive(&path, base_dir, files)?;
            } else if path.extension().map_or(false, |ext| ext == "uasset") {
                // Return full absolute path, not relative
                let path_str = path.to_string_lossy().replace("\\", "/");
                files.push(path_str);
            }
        }
    }
    Ok(())
}

fn find_uasset_paths(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                find_uasset_paths(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "uasset") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn find_json_paths(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                find_json_paths(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "json") {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn find_json_strings(dir: &Path, files: &mut Vec<String>) -> Result<(), String> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                find_json_strings(&path, files)?;
            } else if path.extension().map_or(false, |ext| ext == "json") {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}

pub async fn extract_mod_assets(
    tool_path: &Path,
    game_paks: &str,
    mod_path: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    progress.emit(VfxPipelineProgress {
        stage: "Extract Mod Assets".to_string(),
        step: 1,
        current: 0,
        total: 1,
        message: "Extracting mod assets from IOStore...".to_string(),
    });

    vfx_debug(&format!(
        "extract_mod_assets\n  game_paks: {}\n  mod_path: {}\n  output_dir: {}",
        game_paks, mod_path, output_dir
    ));

    // No deps: this pass defines what "the mod" is, and imports resolve in memory anyway.
    let request = VfxUatRequest {
        action: "extract_iostore_legacy",
        game_paks: Some(game_paks.to_string()),
        mod_path: Some(mod_path.to_string()),
        output_path: Some(output_dir.to_string()),
        with_deps: Some(false),
        ..Default::default()
    };

    let response = run_vfx_uat_request(tool_path, &request).await?;
    if !response.success {
        return Err(format!(
            "[VFX] Extract mod assets failed: {}",
            response.message
        ));
    }

    let mut assets = Vec::new();
    let mut list_lines = Vec::new();

    if let Some(data) = response.data {
        if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
            for file in files {
                if let Some(s) = file.as_str() {
                    if s.ends_with(".uasset") {
                        // For uasset_list.txt (relative, no extension)
                        let list_entry = s[..s.len() - 7].to_string();
                        list_lines.push(list_entry);

                        // For the return value (absolute, with extension)
                        let abs_path = Path::new(output_dir).join(s).to_string_lossy().replace("\\", "/");
                        assets.push(abs_path);
                    }
                }
            }
        }
    }

    // Sort to ensure deterministic order
    assets.sort();
    list_lines.sort();

    let list_path = Path::new(output_dir).join("uasset_list.txt");
    let list_content = list_lines.join("\n");
    let _ = fs::write(&list_path, &list_content);
    vfx_debug(&format!(
        "Wrote uasset_list.txt with {} entries",
        assets.len()
    ));

    progress.emit(VfxPipelineProgress {
        stage: "Extract Mod Assets".to_string(),
        step: 1,
        current: 1,
        total: 1,
        message: format!("Extracted {} assets", assets.len()),
    });

    Ok(assets)
}

pub async fn convert_uassets_to_json(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    progress.emit(VfxPipelineProgress {
        stage: "Converting UAssets to JSON".to_string(),
        step: 2,
        current: 0,
        total: 1,
        message: "Converting assets...".to_string(),
    });

    let list_path = Path::new(input_dir).join("uasset_list.txt");
    let mut file_paths = Vec::new();
    if list_path.exists() {
        if let Ok(content) = fs::read_to_string(&list_path) {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    let mut path = Path::new(input_dir).join(line);
                    path.set_extension("uasset");
                    file_paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    } else {
        vfx_warn("uasset_list.txt not found, falling back to scanning directory");
        let mut uasset_paths = Vec::new();
        find_uasset_paths(Path::new(input_dir), &mut uasset_paths)?;
        file_paths = uasset_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
    }

    vfx_debug(&format!(
        "convert_uassets_to_json (single batch)\n  input_dir: {}\n  output_dir: {}\n  usmap: {}\n  discovered_uasset_files: {}",
        input_dir, output_dir, usmap_path, file_paths.len()
    ));

    if file_paths.is_empty() {
        vfx_warn("No .uasset files found to convert");
        return Ok(Vec::new());
    }

    vfx_info(&format!(
        "Converting {} files in single batch using base_path preservation",
        file_paths.len()
    ));

    // Single request with all files - base_path preserves directory structure
    let request = VfxUatRequest {
        action: "batch_to_json",
        file_paths: Some(file_paths.clone()),
        usmap_path: Some(usmap_path),
        output_path: Some(output_dir.to_string()),
        base_path: Some(input_dir.to_string()), // Preserves relative structure
        ..Default::default()
    };

    let mut converted_files = Vec::new();

    match run_vfx_uat_request(tool_path, &request).await {
        Ok(response) => {
            if response.success {
                // Extract converted file list from response if available
                if let Some(data) = &response.data {
                    if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
                        for f in files {
                            if let Some(s) = f.as_str() {
                                converted_files.push(s.to_string());
                            }
                        }
                    }
                }
                vfx_info(&format!(
                    "to_json summary: success={}, total={}",
                    file_paths.len(),
                    file_paths.len()
                ));
            } else {
                vfx_error(&format!("Batch to_json failed: {}", response.message));
                return Err(format!("Batch conversion failed: {}", response.message));
            }
        }
        Err(e) => {
            vfx_error(&format!("Batch to_json request failed: {}", e));
            return Err(format!("Batch conversion failed: {}", e));
        }
    }

    // Fallback: if the UAT response did not include a `files` list, enumerate
    // the produced JSON files from disk so downstream pipeline steps receive
    // the real conversion output instead of an empty list.
    if converted_files.is_empty() {
        let mut json_paths = Vec::new();
        find_json_paths(Path::new(output_dir), &mut json_paths)?;
        converted_files = json_paths
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        vfx_debug(&format!(
            "to_json response had no `files` field; enumerated {} JSON files from {}",
            converted_files.len(),
            output_dir
        ));
    }

    Ok(converted_files)
}

pub async fn convert_json_to_uassets(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_dir: &str,
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    let mut json_files = Vec::new();
    find_json_paths(Path::new(input_dir), &mut json_files)?;

    if json_files.is_empty() {
        return Ok(Vec::new());
    }

    progress.emit(VfxPipelineProgress {
        stage: "Converting JSON to UAssets".to_string(),
        step: 7,
        current: 0,
        total: 1,
        message: format!(
            "Converting {} JSON files in single batch...",
            json_files.len()
        ),
    });

    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    vfx_debug(&format!(
        "convert_json_to_uassets (single batch)\n  input_dir: {}\n  output_dir: {}\n  usmap: {}\n  discovered_json_files: {}",
        input_dir, output_dir, usmap_path, json_files.len()
    ));

    // Flatten all file paths to strings
    let file_paths: Vec<String> = json_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    vfx_info(&format!(
        "Converting {} JSON files in single batch using base_path preservation",
        file_paths.len()
    ));

    // Single request with all files - base_path preserves directory structure
    let request = VfxUatRequest {
        action: "batch_from_json",
        file_paths: Some(file_paths.clone()),
        usmap_path: Some(usmap_path),
        output_path: Some(output_dir.to_string()),
        base_path: Some(input_dir.to_string()), // Preserves relative structure
        ..Default::default()
    };

    match run_vfx_uat_request(tool_path, &request).await {
        Ok(response) => {
            if response.success {
                vfx_info(&format!(
                    "from_json summary: success={}, total={}",
                    file_paths.len(),
                    file_paths.len()
                ));
            } else {
                // The tool reports each file's failure in `data.errors`; log them all and
                // surface the first few so the real cause is visible (not just "0/N").
                let sample = response
                    .data
                    .as_ref()
                    .and_then(|d| d.get("errors"))
                    .and_then(|e| e.as_array())
                    .map(|arr| {
                        for e in arr.iter().filter_map(|e| e.as_str()) {
                            vfx_error(&format!("from_json: {}", e));
                        }
                        arr.iter()
                            .filter_map(|e| e.as_str())
                            .take(3)
                            .collect::<Vec<_>>()
                            .join(" || ")
                    })
                    .unwrap_or_default();
                vfx_error(&format!("Batch from_json failed: {}", response.message));
                return Err(format!(
                    "Batch conversion failed: {} — first errors: {}",
                    response.message, sample
                ));
            }
        }
        Err(e) => {
            vfx_error(&format!("Batch from_json request failed: {}", e));
            return Err(format!("Batch conversion failed: {}", e));
        }
    }

    let mut uasset_paths = Vec::new();
    find_uasset_paths(Path::new(output_dir), &mut uasset_paths)?;
    let uasset_files = uasset_paths
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    progress.emit(VfxPipelineProgress {
        stage: "Converting JSON to UAssets".to_string(),
        step: 7,
        current: 1,
        total: 1,
        message: format!("Converted {} files", uasset_files.len()),
    });

    Ok(uasset_files)
}

pub async fn extract_vanilla_assets(
    tool_path: &Path,
    game_paks: &str,
    output_dir: &str,
    filter_patterns: &[String],
    progress: &dyn VfxProgressSink,
) -> Result<Vec<String>, String> {
    fs::create_dir_all(output_dir).map_err(|e| e.to_string())?;

    if filter_patterns.is_empty() {
        return Err("[VFX] No filter patterns provided for vanilla extraction".to_string());
    }

    progress.emit(VfxPipelineProgress {
        stage: "Extract Vanilla Assets".to_string(),
        step: 4,
        current: 0,
        total: 1,
        message: format!("Extracting {} vanilla assets...", filter_patterns.len()),
    });

    let normalized_patterns: Vec<String> = filter_patterns
        .iter()
        .map(|pattern| {
            let mut p = pattern.replace("\\", "/");
            if p.ends_with(".uasset") {
                p = p[..p.len() - 7].to_string();
            } else if p.ends_with(".uexp") {
                p = p[..p.len() - 5].to_string();
            }
            p
        })
        .collect();

    // The caller's patterns are extract-dir paths; the extractor filters on package paths.
    let package_patterns: Vec<String> = normalized_patterns
        .iter()
        .map(|p| to_package_path(p))
        .collect();

    vfx_debug(&format!(
        "extract_vanilla_assets\n  game_paks: {}\n  output_dir: {}\n  filter_patterns ({}):",
        game_paks,
        output_dir,
        package_patterns.len()
    ));
    for (i, p) in package_patterns.iter().enumerate().take(20) {
        vfx_debug(&format!("    [{}] {}", i, p));
    }
    if package_patterns.len() > 20 {
        vfx_debug(&format!(
            "    ... and {} more",
            package_patterns.len() - 20
        ));
    }

    // Mirrors the legacy CLI `extract_iostore_legacy <game_paks> <output> --filter ...`,
    // passing the patterns inline (the JSON handler takes a `filter_patterns` list, so the
    // old temp-filter-file round-trip is no longer needed).
    // Deps on: to_json pulls property schemas off referenced assets on disk (UAsset
    // PullSchemasFromAnotherAsset), so imports missing from the tree degrade the JSON.
    let request = VfxUatRequest {
        action: "extract_iostore_legacy",
        game_paks: Some(game_paks.to_string()),
        output_path: Some(output_dir.to_string()),
        filter_patterns: Some(package_patterns),
        with_deps: Some(true),
        ..Default::default()
    };

    let response = run_vfx_uat_request(tool_path, &request).await?;
    if !response.success {
        return Err(format!(
            "[VFX] Vanilla extraction failed: {}",
            response.message
        ));
    }

    let mut assets = Vec::new();
    find_uassets_recursive(Path::new(output_dir), Path::new(output_dir), &mut assets)?;

    if assets.is_empty() {
        vfx_warn(&format!(
            "Vanilla extraction matched none of the {} requested assets — the game has no counterpart for them, or the filter paths no longer match the tool's package paths",
            normalized_patterns.len()
        ));
        return Ok(assets);
    }

    // The deps stay on disk for schema resolution but are not themselves converted:
    // uasset_list.txt narrows the next step back down to the assets that were asked for.
    let base = Path::new(output_dir);
    let lowered_patterns: Vec<String> = normalized_patterns
        .iter()
        .map(|p| p.replace('\\', "/").to_lowercase())
        .collect();

    let mut requested = Vec::new();
    let mut list_lines = Vec::new();
    for asset in &assets {
        let path = Path::new(asset);
        let relative = path
            .strip_prefix(base)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| asset.clone());

        let lowered = relative.to_lowercase();
        if !lowered_patterns.iter().any(|p| lowered.contains(p)) {
            continue;
        }

        requested.push(asset.clone());
        list_lines.push(
            relative
                .strip_suffix(".uasset")
                .unwrap_or(&relative)
                .to_string(),
        );
    }

    let dependencies = assets.len() - requested.len();

    if requested.is_empty() {
        vfx_warn(&format!(
            "No extracted vanilla asset matched a filter pattern ({} files extracted); converting all of them",
            assets.len()
        ));
    } else {
        list_lines.sort();
        let list_path = Path::new(output_dir).join("uasset_list.txt");
        let _ = fs::write(&list_path, list_lines.join("\n"));
        vfx_info(&format!(
            "Vanilla extract: {} requested assets, {} dependencies kept on disk for schema resolution",
            requested.len(),
            dependencies
        ));
    }

    let assets = if requested.is_empty() { assets } else { requested };

    progress.emit(VfxPipelineProgress {
        stage: "Extract Vanilla Assets".to_string(),
        step: 4,
        current: 1,
        total: 1,
        message: format!("Extracted {} vanilla assets", assets.len()),
    });

    Ok(assets)
}

/// Extract-dir path back to the UE package path the extractor filters on — the inverse of
/// the tool's ResolveGamePathToContent (`/Game/X` <-> `Marvel/Content/X`, other mounts as-is).
fn to_package_path(relative_path: &str) -> String {
    let p = relative_path.replace('\\', "/");
    let p = p.trim_start_matches('/');
    match p.strip_prefix("Marvel/Content/") {
        Some(rest) => format!("/Game/{}", rest),
        None => format!("/{}", p),
    }
}

/// Comparison key for an asset: forward slashes, no extension, lowercased.
fn asset_key(relative_path: &str) -> String {
    let p = relative_path.replace('\\', "/");
    let p = p.strip_suffix(".uasset").unwrap_or(&p);
    p.trim_start_matches('/').to_lowercase()
}

/// Stage only `allowed` into a fresh dir — the packer packs whatever directory it is given.
/// Returns (stage dir, kept, dropped).
fn stage_allowed_assets(input_dir: &Path, allowed: &[String]) -> Result<(PathBuf, usize, usize), String> {
    use std::collections::HashSet;

    let wanted: HashSet<String> = allowed.iter().map(|p| asset_key(p)).collect();
    let stage_dir = super::file_ops::create_step_directory("pack_stage")?;

    let mut uasset_paths = Vec::new();
    find_uasset_paths(input_dir, &mut uasset_paths)?;

    let mut staged = 0usize;
    let mut dropped = 0usize;

    for asset in &uasset_paths {
        let relative = asset
            .strip_prefix(input_dir)
            .map_err(|e| format!("[VFX] Failed to relativize {}: {}", asset.display(), e))?;

        if !wanted.contains(&asset_key(&relative.to_string_lossy())) {
            vfx_debug(&format!("Pack filter dropped: {}", relative.display()));
            dropped += 1;
            continue;
        }

        let dest = stage_dir.join(relative);
        let dest_dir = dest
            .parent()
            .ok_or_else(|| format!("[VFX] No parent for {}", dest.display()))?;
        fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

        // Stem plus a dot: keeps `Foo.m.ubulk`, leaves `Foo_01.uasset` alone.
        let stem = asset
            .file_stem()
            .ok_or_else(|| format!("[VFX] No file stem for {}", asset.display()))?
            .to_string_lossy()
            .to_string();
        let source_dir = asset
            .parent()
            .ok_or_else(|| format!("[VFX] No parent for {}", asset.display()))?;
        let prefix = format!("{}.", stem);

        for entry in fs::read_dir(source_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&prefix) {
                continue;
            }
            fs::copy(&path, dest_dir.join(&name))
                .map_err(|e| format!("[VFX] Failed to stage {}: {}", path.display(), e))?;
        }

        staged += 1;
    }

    Ok((stage_dir, staged, dropped))
}

/// `allowed_assets`: the source mod's own assets, relative to the extract dir. Only these get packed.
pub async fn pack_to_iostore(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_base: &str,
    allowed_assets: Option<&[String]>,
    progress: &dyn VfxProgressSink,
) -> Result<String, String> {
    let output_base = if output_base.ends_with(".pak") {
        output_base.replace(".pak", "")
    } else if output_base.ends_with(".utoc") {
        output_base.replace(".utoc", "")
    } else {
        output_base.to_string()
    };

    progress.emit(VfxPipelineProgress {
        stage: "Creating IOStore bundle".to_string(),
        step: 8,
        current: 0,
        total: 1,
        message: format!("Creating: {}.utoc/.ucas/.pak", output_base),
    });

    // Checked before the manifest, so an empty pipeline output is not reported as a
    // manifest mismatch.
    let mut input_assets = Vec::new();
    find_uasset_paths(Path::new(input_dir), &mut input_assets)?;
    if input_assets.is_empty() {
        return Err(format!(
            "[VFX] Nothing to pack: no .uasset files in {}",
            input_dir
        ));
    }

    // An empty manifest means "no manifest" — packing nothing is never the intent.
    let pack_dir = match allowed_assets {
        Some(allowed) if !allowed.is_empty() => {
            let (stage_dir, staged, dropped) = stage_allowed_assets(Path::new(input_dir), allowed)?;
            vfx_info(&format!(
                "Pack manifest: {} of {} assets kept, {} left out (not in the source mod)",
                staged,
                staged + dropped,
                dropped
            ));
            if staged == 0 {
                return Err(
                    "[VFX] No packable assets matched the source mod manifest".to_string()
                );
            }
            stage_dir.to_string_lossy().to_string()
        }
        _ => {
            vfx_warn("No source mod manifest supplied; packing the whole input directory");
            input_dir.to_string()
        }
    };

    let mut uasset_files = Vec::new();
    find_uasset_paths(Path::new(&pack_dir), &mut uasset_files)?;

    if uasset_files.is_empty() {
        return Err("[VFX] No .uasset files found in input directory".to_string());
    }

    vfx_debug(&format!(
        "pack_to_iostore\n  input_dir: {}\n  pack_dir: {}\n  output_base: {}\n  usmap: {}\n  uasset_files: {}",
        input_dir,
        pack_dir,
        output_base,
        usmap_path,
        uasset_files.len()
    ));

    progress.emit(VfxPipelineProgress {
        stage: "Creating IOStore bundle".to_string(),
        step: 8,
        current: 0,
        total: 1,
        message: format!("Packing {} assets...", uasset_files.len()),
    });

    // Mirrors the legacy CLI `create_mod_iostore <output_base> <input_dir>` (compression
    // on by default). The CLI's `--usmap` flag was a no-op for packing (it matched no
    // option and was ignored), and the JSON handler takes no usmap, so it is dropped here.
    let request = VfxUatRequest {
        action: "create_mod_iostore",
        output_path: Some(output_base.clone()),
        input_dir: Some(pack_dir.clone()),
        compress: Some(true),
        ..Default::default()
    };

    let response = run_vfx_uat_request(tool_path, &request).await?;
    if !response.success {
        return Err(format!(
            "[VFX] create_mod_iostore failed: {}",
            response.message
        ));
    }

    progress.emit(VfxPipelineProgress {
        stage: "Creating IOStore bundle".to_string(),
        step: 8,
        current: 1,
        total: 1,
        message: "IOStore bundle created successfully".to_string(),
    });

    Ok(format!("{}.utoc", output_base))
}

/// Get asset class for multiple uasset files using UAssetTool's get_class action
/// Returns a map of file path -> class name (e.g., "MaterialInstanceConstant")
pub async fn get_asset_classes(
    tool_path: &Path,
    usmap_path: &str,
    uasset_paths: &[String],
    progress: &dyn VfxProgressSink,
) -> Result<std::collections::HashMap<String, String>, String> {
    use std::collections::HashMap;

    vfx_debug(&format!(
        "get_asset_classes\n  usmap: {}\n  files: {}",
        usmap_path,
        uasset_paths.len()
    ));

    progress.emit(VfxPipelineProgress {
        stage: "Scanning asset classes".to_string(),
        step: 0,
        current: 0,
        total: uasset_paths.len(),
        message: format!("Scanning {} assets...", uasset_paths.len()),
    });

    let mut class_map: HashMap<String, String> = HashMap::new();

    // Process in batches to avoid overwhelming the session
    let batch_size = 50;
    let batches: Vec<_> = uasset_paths.chunks(batch_size).collect();

    for (batch_idx, batch) in batches.iter().enumerate() {
        let request = VfxUatRequest {
            action: "get_class",
            file_paths: Some(batch.to_vec()),
            usmap_path: Some(usmap_path),
            ..Default::default()
        };

        if batch_idx % 5 == 0 || batch_idx == batches.len() - 1 {
            vfx_debug(&format!(
                "get_class batch {}/{}: {} files",
                batch_idx + 1,
                batches.len(),
                batch.len()
            ));
        }

        match run_vfx_uat_request(tool_path, &request).await {
            Ok(response) => {
                if response.success {
                    // Response data should be an object mapping file paths to class names
                    if let Some(data) = response.data {
                        if let Some(obj) = data.as_object() {
                            for (path, class_value) in obj {
                                if let Some(class_name) = class_value.as_str() {
                                    class_map.insert(path.clone(), class_name.to_string());
                                }
                            }
                        }
                    }
                } else {
                    vfx_warn(&format!(
                        "get_class batch {} warning: {}",
                        batch_idx + 1,
                        response.message
                    ));
                }
            }
            Err(e) => {
                vfx_error(&format!("get_class batch {} error: {}", batch_idx + 1, e));
            }
        }

        progress.emit(VfxPipelineProgress {
            stage: "Scanning asset classes".to_string(),
            step: 0,
            current: ((batch_idx + 1) * batch_size).min(uasset_paths.len()),
            total: uasset_paths.len(),
            message: format!(
                "Scanned {}/{} assets",
                (batch_idx + 1) * batch.len(),
                uasset_paths.len()
            ),
        });
    }

    vfx_info(&format!(
        "get_asset_classes complete: {} classes mapped",
        class_map.len()
    ));
    Ok(class_map)
}

/// Batch-detect asset types for multiple uasset files using UAssetTool's batch_detect action.
/// Returns a map of file path -> asset type
/// (e.g. "material_instance", "blueprint", "texture", "other").
pub async fn batch_detect_asset_types(
    tool_path: &Path,
    usmap_path: &str,
    uasset_paths: &[String],
    progress: &dyn VfxProgressSink,
) -> Result<std::collections::HashMap<String, String>, String> {
    use std::collections::HashMap;

    fn normalize_detected_type(asset_type: Option<&str>) -> String {
        let trimmed = asset_type.map(str::trim).unwrap_or("");
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("Unknown") {
            return "other".to_string();
        }

        if trimmed.eq_ignore_ascii_case("material_instance")
            || trimmed.eq_ignore_ascii_case("MaterialInstance")
            || trimmed.eq_ignore_ascii_case("MaterialInstanceConstant")
            || trimmed.eq_ignore_ascii_case("MaterialInstanceDynamic")
        {
            return "material_instance".to_string();
        }

        if trimmed.eq_ignore_ascii_case("blueprint")
            || trimmed.eq_ignore_ascii_case("BlueprintGeneratedClass")
            || trimmed.eq_ignore_ascii_case("CanvasPanelSlot")
        {
            return "blueprint".to_string();
        }

        "other".to_string()
    }

    vfx_debug(&format!(
        "batch_detect_asset_types\n  usmap: {}\n  files: {}",
        usmap_path,
        uasset_paths.len()
    ));

    progress.emit(VfxPipelineProgress {
        stage: "Detecting asset types".to_string(),
        step: 0,
        current: 0,
        total: uasset_paths.len(),
        message: format!("Detecting {} assets...", uasset_paths.len()),
    });

    let mut type_map: HashMap<String, String> = HashMap::new();

    // Keep requests bounded to avoid oversized request payloads.
    let batch_size = 100;
    let batches: Vec<_> = uasset_paths.chunks(batch_size).collect();

    for (batch_idx, batch) in batches.iter().enumerate() {
        let request = VfxUatRequest {
            action: "detect_type",
            file_paths: Some(batch.to_vec()),
            usmap_path: Some(usmap_path),
            ..Default::default()
        };

        vfx_debug(&format!(
            "batch_detect batch {}/{}: {} files",
            batch_idx + 1,
            batches.len(),
            batch.len()
        ));

        match run_vfx_uat_request(tool_path, &request).await {
            Ok(response) => {
                if response.success {
                    if let Some(data) = response.data {
                        if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
                            for result in results {
                                let path = result
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .map(|p| p.to_string());

                                if let Some(path) = path {
                                    let raw_asset_type =
                                        result.get("asset_type").and_then(|v| v.as_str());
                                    let normalized = normalize_detected_type(raw_asset_type);
                                    if raw_asset_type.is_none() {
                                        vfx_warn(&format!(
                                            "detect_type missing asset_type for path '{}' (normalized=other)",
                                            path
                                        ));
                                    }
                                    type_map.insert(path, normalized);
                                }
                            }
                        } else {
                            vfx_warn(&format!(
                                "detect_type batch {} missing data.results array",
                                batch_idx + 1
                            ));
                            vfx_debug(&format!("detect_type raw data: {}", data));
                        }
                    } else {
                        vfx_warn(&format!(
                            "detect_type batch {} returned no data",
                            batch_idx + 1
                        ));
                    }
                } else {
                    vfx_warn(&format!(
                        "detect_type batch {} warning: {}",
                        batch_idx + 1,
                        response.message
                    ));
                }
            }
            Err(e) => {
                vfx_error(&format!("detect_type batch {} error: {}", batch_idx + 1, e));
            }
        }

        progress.emit(VfxPipelineProgress {
            stage: "Detecting asset types".to_string(),
            step: 0,
            current: ((batch_idx + 1) * batch_size).min(uasset_paths.len()),
            total: uasset_paths.len(),
            message: format!(
                "Detected types for {}/{} assets",
                ((batch_idx + 1) * batch_size).min(uasset_paths.len()),
                uasset_paths.len()
            ),
        });
    }

    vfx_info(&format!(
        "batch_detect_asset_types complete: {} type mappings",
        type_map.len()
    ));
    Ok(type_map)
}

/// Check if an asset class is updatable (contains color parameters)
pub fn is_updatable_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "MaterialInstance"
            | "MaterialInstanceConstant"
            | "MaterialInstanceDynamic"
            | "blueprint"
            | "CanvasPanelSlot"
            | "BlueprintGeneratedClass"
            | "WidgetBlueprintGeneratedClass"
    )
}
