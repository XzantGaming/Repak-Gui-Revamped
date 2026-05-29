//! VFX Updater - Isolated UAssetTool Interactive Session
//!
//! This module manages a completely separate UAssetTool process from Repak-X's
//! existing uasset_toolkit. It provides async functions for VFX pipeline operations.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::logging::{vfx_debug, vfx_error, vfx_info, vfx_warn};
use super::models::VfxPipelineProgress;
use super::progress::VfxProgressSink;

#[derive(Serialize, Debug)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point: Option<String>,
    /// Base path for preserving relative directory structure in batch output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct VfxUatResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

struct VfxUatSession {
    child: Child,
    stdin: ChildStdin,
    stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
    stderr_task: JoinHandle<()>,
}

impl VfxUatSession {
    async fn start(tool_path: &Path) -> Result<Self, String> {
        vfx_info(&format!(
            "Starting isolated UAssetTool session: {}",
            tool_path.display()
        ));

        let mut cmd = Command::new(tool_path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Hide console window on Windows release builds
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("[VFX] Failed to start UAssetTool: {}", e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "[VFX] Failed to open stdin for UAssetTool".to_string())?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "[VFX] Failed to open stdout for UAssetTool".to_string())?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "[VFX] Failed to open stderr for UAssetTool".to_string())?;

        let mut stderr_lines = BufReader::new(stderr).lines();
        let stderr_task = tokio::spawn(async move {
            loop {
                match stderr_lines.next_line().await {
                    Ok(Some(line)) => {
                        if !line.trim().is_empty() {
                            vfx_debug(&format!("UAT[stderr] {}", line));
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        vfx_error(&format!("UAT[stderr] read error: {}", e));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            stdout_lines: BufReader::new(stdout).lines(),
            stderr_task,
        })
    }

    async fn send_request(
        &mut self,
        request: &VfxUatRequest<'_>,
    ) -> Result<VfxUatResponse, String> {
        let request_json = serde_json::to_string(request)
            .map_err(|e| format!("[VFX] Failed to serialize request: {}", e))?;

        vfx_debug(&format!("UAT request: {}", request_json));

        self.stdin
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| format!("[VFX] Failed to write to UAT stdin: {}", e))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("[VFX] Failed to finalize request line: {}", e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("[VFX] Failed to flush UAT stdin: {}", e))?;

        loop {
            let line = self
                .stdout_lines
                .next_line()
                .await
                .map_err(|e| format!("[VFX] Failed to read UAT response: {}", e))?;

            let Some(line) = line else {
                return Err("[VFX] UAT stdout closed unexpectedly".to_string());
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<VfxUatResponse>(trimmed) {
                Ok(response) => {
                    vfx_debug(&format!(
                        "UAT response: success={}, message={}",
                        response.success, response.message
                    ));
                    if let Some(data) = &response.data {
                        vfx_debug(&format!("UAT response data: {}", data));
                    }
                    return Ok(response);
                }
                Err(e) => {
                    vfx_debug(&format!("UAT stdout non-JSON line ({}): {}", e, trimmed));
                }
            }
        }
    }

    async fn shutdown(&mut self) {
        vfx_info("Shutting down UAT session");

        if let Err(e) = self.stdin.shutdown().await {
            vfx_error(&format!("Failed to close UAT stdin: {}", e));
        }

        match self.child.try_wait() {
            Ok(Some(status)) => {
                vfx_info(&format!("UAT already exited with status: {}", status));
            }
            Ok(None) => {
                if let Err(e) = self.child.kill().await {
                    vfx_error(&format!("Failed to kill UAT process: {}", e));
                }
                if let Err(e) = self.child.wait().await {
                    vfx_error(&format!("Failed waiting for killed UAT process: {}", e));
                }
            }
            Err(e) => {
                vfx_error(&format!("Failed to query UAT process status: {}", e));
            }
        }

        self.stderr_task.abort();
    }
}

fn vfx_session_store() -> &'static Mutex<Option<VfxUatSession>> {
    static SESSION: OnceLock<Mutex<Option<VfxUatSession>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

pub async fn ensure_vfx_uat_session(tool_path: &Path) -> Result<(), String> {
    let store = vfx_session_store();
    let mut guard = store.lock().await;
    if guard.is_some() {
        vfx_debug("UAT session already active");
        return Ok(());
    }

    let session = VfxUatSession::start(tool_path).await?;
    *guard = Some(session);
    vfx_info("UAT session started successfully");
    Ok(())
}

pub async fn close_vfx_uat_session() {
    let store = vfx_session_store();
    let mut guard = store.lock().await;
    if let Some(mut session) = guard.take() {
        session.shutdown().await;
        vfx_info("UAT session closed");
    } else {
        vfx_debug("UAT session was not active");
    }
}

pub async fn run_vfx_uat_request(
    tool_path: &Path,
    request: &VfxUatRequest<'_>,
) -> Result<VfxUatResponse, String> {
    ensure_vfx_uat_session(tool_path).await?;

    let store = vfx_session_store();
    let mut guard = store.lock().await;
    let session = guard
        .as_mut()
        .ok_or_else(|| "[VFX] UAT session missing after initialization".to_string())?;

    match session.send_request(request).await {
        Ok(response) => Ok(response),
        Err(first_error) => {
            vfx_warn(&format!(
                "UAT request failed, restarting session and retrying: {}",
                first_error
            ));

            if let Some(mut old_session) = guard.take() {
                old_session.shutdown().await;
            }

            let mut new_session = VfxUatSession::start(tool_path).await?;
            let retry = new_session.send_request(request).await;
            *guard = Some(new_session);

            retry.map_err(|retry_error| {
                format!(
                    "[VFX] UAT request failed after restart. First: {}. Retry: {}",
                    first_error, retry_error
                )
            })
        }
    }
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

    let mut cmd = Command::new(tool_path);
    cmd.arg("extract_iostore_legacy")
        .arg(game_paks)
        .arg(output_dir)
        .arg("--mod")
        .arg(mod_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    vfx_debug(&format!(
        "extract_mod_assets\n  game_paks: {}\n  mod_path: {}\n  output_dir: {}",
        game_paks, mod_path, output_dir
    ));

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("[VFX] Failed to run UAssetTool: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        vfx_debug(&format!("stdout:\n{}", stdout));
    }
    if !stderr.is_empty() {
        vfx_debug(&format!("stderr:\n{}", stderr));
    }

    if !output.status.success() {
        return Err(format!("[VFX] Extract mod assets failed: {}", stderr));
    }

    let mut assets = Vec::new();
    find_uassets_recursive(Path::new(output_dir), Path::new(output_dir), &mut assets)?;

    let list_path = Path::new(output_dir).join("uasset_list.txt");
    let list_content = assets
        .iter()
        .map(|p| {
            let mut p = p.clone();
            if p.ends_with(".uasset") {
                p = p[..p.len() - 7].to_string();
            }
            p
        })
        .collect::<Vec<_>>()
        .join("\n");
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

    let mut uasset_paths = Vec::new();
    find_uasset_paths(Path::new(input_dir), &mut uasset_paths)?;

    vfx_debug(&format!(
        "convert_uassets_to_json (single batch)\n  input_dir: {}\n  output_dir: {}\n  usmap: {}\n  discovered_uasset_files: {}",
        input_dir, output_dir, usmap_path, uasset_paths.len()
    ));

    if uasset_paths.is_empty() {
        vfx_warn("No .uasset files found to convert");
        return Ok(Vec::new());
    }

    // Flatten all file paths to strings
    let file_paths: Vec<String> = uasset_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    vfx_info(&format!(
        "Converting {} files in single batch using base_path preservation",
        file_paths.len()
    ));

    // Single request with all files - base_path preserves directory structure
    let request = VfxUatRequest {
        action: "batch_to_json",
        file_path: None,
        file_paths: Some(file_paths.clone()),
        usmap_path: Some(usmap_path),
        output_path: Some(output_dir.to_string()),
        filter: None,
        mount_point: None,
        base_path: Some(input_dir.to_string()), // Preserves relative structure
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
        file_path: None,
        file_paths: Some(file_paths.clone()),
        usmap_path: Some(usmap_path),
        output_path: Some(output_dir.to_string()),
        filter: None,
        mount_point: None,
        base_path: Some(input_dir.to_string()), // Preserves relative structure
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
                vfx_error(&format!("Batch from_json failed: {}", response.message));
                return Err(format!("Batch conversion failed: {}", response.message));
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

    let filters_file_name = format!(
        "rvfx_extract_filters_{}.txt",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
    );
    let filters_file_path = std::env::temp_dir().join(filters_file_name);
    let filters_file_contents = normalized_patterns.join("\n");

    fs::write(&filters_file_path, filters_file_contents).map_err(|e| {
        format!(
            "[VFX] Failed to write filter file {}: {}",
            filters_file_path.display(),
            e
        )
    })?;

    let mut cmd = Command::new(tool_path);
    cmd.arg("extract_iostore_legacy")
        .arg(game_paks)
        .arg(output_dir)
        .arg("--filter")
        .arg(&filters_file_path);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    vfx_debug(&format!(
        "extract_vanilla_assets\n  game_paks: {}\n  output_dir: {}\n  filters_file: {}\n  filter_patterns ({}):",
        game_paks, output_dir, filters_file_path.display(), filter_patterns.len()
    ));
    for (i, p) in normalized_patterns.iter().enumerate().take(20) {
        vfx_debug(&format!("    [{}] {}", i, p));
    }
    if normalized_patterns.len() > 20 {
        vfx_debug(&format!(
            "    ... and {} more",
            normalized_patterns.len() - 20
        ));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("[VFX] Failed to run UAssetTool: {}", e))?;

    if let Err(e) = fs::remove_file(&filters_file_path) {
        vfx_warn(&format!(
            "Failed to remove filters file {}: {}",
            filters_file_path.display(),
            e
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        vfx_debug(&format!("stdout:\n{}", stdout));
    }
    if !stderr.is_empty() {
        vfx_debug(&format!("stderr:\n{}", stderr));
    }

    if !output.status.success() {
        return Err(format!("[VFX] Vanilla extraction failed: {}", stderr));
    }

    let mut assets = Vec::new();
    find_uassets_recursive(Path::new(output_dir), Path::new(output_dir), &mut assets)?;

    progress.emit(VfxPipelineProgress {
        stage: "Extract Vanilla Assets".to_string(),
        step: 4,
        current: 1,
        total: 1,
        message: format!("Extracted {} vanilla assets", assets.len()),
    });

    Ok(assets)
}

pub async fn pack_to_iostore(
    tool_path: &Path,
    usmap_path: &str,
    input_dir: &str,
    output_base: &str,
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

    let mut uasset_files = Vec::new();
    find_uasset_paths(Path::new(input_dir), &mut uasset_files)?;

    if uasset_files.is_empty() {
        return Err("[VFX] No .uasset files found in input directory".to_string());
    }

    let mut cmd = Command::new(tool_path);
    cmd.arg("create_mod_iostore")
        .arg(&output_base)
        .arg(input_dir)
        .arg("--usmap")
        .arg(usmap_path);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    vfx_debug(&format!(
        "pack_to_iostore\n  input_dir: {}\n  output_base: {}\n  usmap: {}\n  uasset_files: {}",
        input_dir,
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

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("[VFX] Failed to run UAssetTool: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        vfx_debug(&format!("stdout:\n{}", stdout));
    }
    if !stderr.is_empty() {
        vfx_debug(&format!("stderr:\n{}", stderr));
    }

    if !output.status.success() {
        return Err(format!("[VFX] create_mod_iostore failed: {}", stderr));
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
            file_path: None,
            file_paths: Some(batch.to_vec()),
            usmap_path: Some(usmap_path),
            output_path: None,
            filter: None,
            mount_point: None,
            base_path: None,
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
            file_path: None,
            file_paths: Some(batch.to_vec()),
            usmap_path: Some(usmap_path),
            output_path: None,
            filter: None,
            mount_point: None,
            base_path: None,
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
