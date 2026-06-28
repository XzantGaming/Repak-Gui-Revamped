use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::{Mutex as StdMutex, OnceLock};

// ============================================================================
// SYNCHRONOUS UASSETTOOL WRAPPER (in-process NativeAOT FFI)
// ============================================================================
// This module provides a synchronous interface to UAssetTool. It loads the
// NativeAOT-compiled UAssetTool library (UAssetTool.dll / .so / .dylib) and calls
// the C-exported `uat_invoke` / `uat_free` functions directly. No child process,
// no stdin/stdout pipe.
//
// The wire format is unchanged: requests/responses are the same JSON that the
// tool's interactive stdin/stdout mode speaks. Only the transport changed, so the
// request/response types and every public method below are identical to before.
//
// Thread-safety: a std::sync::Mutex serializes calls into the native tool, matching
// the previous single-process behavior (UAssetAPI / ProcessRequest keep static
// state and were never called concurrently).
// ============================================================================

/// `uat_invoke(const char* request_utf8) -> char*` — JSON in, JSON out. The returned
/// buffer is owned by the native side and must be released with `uat_free`.
type UatInvokeFn = unsafe extern "C" fn(*const c_char) -> *mut c_char;
/// `uat_free(char*)` — release a buffer returned by `uat_invoke`.
type UatFreeFn = unsafe extern "C" fn(*mut c_char);

/// Global singleton for the synchronous UAssetToolkit
static GLOBAL_TOOLKIT_SYNC: OnceLock<SyncToolkit> = OnceLock::new();

/// Synchronous toolkit that holds the loaded UAssetTool native library and calls
/// into it directly. The library is loaded once and kept for the process lifetime.
pub struct SyncToolkit {
    lib: Library,
    /// Serializes calls into the native tool (see module note on thread-safety).
    call_lock: StdMutex<()>,
}

impl SyncToolkit {
    /// Load the native toolkit. `dll_path` overrides auto-discovery when provided
    /// (the old `tool_path` argument; kept so existing call sites compile unchanged).
    pub fn new(dll_path: Option<String>) -> Result<Self> {
        let dll_path = match dll_path {
            Some(path) => path,
            None => Self::find_dll_path()?,
        };

        log::info!("[SyncToolkit] Loading UAssetTool native library: {}", dll_path);
        let lib = unsafe { Library::new(&dll_path) }
            .with_context(|| format!("Failed to load UAssetTool native library at: {}", dll_path))?;
        log::info!("[SyncToolkit] Native library loaded successfully");

        Ok(Self {
            lib,
            call_lock: StdMutex::new(()),
        })
    }

    /// Native library file name for the current platform.
    fn get_dll_name() -> &'static str {
        #[cfg(windows)]
        {
            "UAssetTool.dll"
        }
        #[cfg(target_os = "linux")]
        {
            "libUAssetTool.so"
        }
        #[cfg(target_os = "macos")]
        {
            "libUAssetTool.dylib"
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            "UAssetTool.dll"
        }
    }

    /// Locate the native library: beside the executable first (where build.rs copies
    /// it), then a legacy `uassettool/` subdir, then the dev publish output.
    fn find_dll_path() -> Result<String> {
        let dll_name = Self::get_dll_name();
        let exe_path = std::env::current_exe()?;
        let exe_dir = exe_path
            .parent()
            .context("Failed to get executable directory")?;

        // Primary: beside the executable.
        let beside = exe_dir.join(dll_name);
        if beside.exists() {
            return Ok(beside.to_string_lossy().to_string());
        }

        // Legacy: a `uassettool/` subdirectory next to the executable.
        let legacy = exe_dir.join("uassettool").join(dll_name);
        if legacy.exists() {
            return Ok(legacy.to_string_lossy().to_string());
        }

        // Dev: NativeAOT publish output inside the submodule.
        let runtime_id = Self::get_runtime_identifier();
        let dev_tool = Path::new("UAssetToolRivals/src/UAssetTool/bin_native/Release/net8.0")
            .join(runtime_id)
            .join("native")
            .join(dll_name);
        if dev_tool.exists() {
            return Ok(dev_tool.to_string_lossy().to_string());
        }

        // Default assumption: beside the executable (will surface a clear load error).
        Ok(beside.to_string_lossy().to_string())
    }

    /// Get the .NET runtime identifier for the current platform
    fn get_runtime_identifier() -> &'static str {
        #[cfg(target_os = "windows")]
        {
            "win-x64"
        }
        #[cfg(target_os = "linux")]
        {
            "linux-x64"
        }
        #[cfg(target_os = "macos")]
        {
            "osx-x64"
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            "win-x64"
        } // fallback
    }

    /// Low-level FFI call: send a raw JSON request envelope and return the raw JSON
    /// response string. Serializes concurrent calls into the native tool via `call_lock`.
    fn invoke_raw(&self, request_json: &str) -> Result<String> {
        log::info!(
            "[SyncToolkit] Invoking native: {}...",
            &request_json[..std::cmp::min(200, request_json.len())]
        );

        // JSON text never contains an interior NUL, but guard anyway rather than panic.
        let c_request = CString::new(request_json)
            .map_err(|e| anyhow::anyhow!("Request JSON contained an interior NUL byte: {}", e))?;

        // Serialize calls into the native tool (see module note on thread-safety).
        let _guard = self
            .call_lock
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to acquire call lock: {}", e))?;

        unsafe {
            let invoke: Symbol<UatInvokeFn> = self
                .lib
                .get(b"uat_invoke\0")
                .map_err(|e| anyhow::anyhow!("FFI symbol `uat_invoke` not found: {}", e))?;
            let free: Symbol<UatFreeFn> = self
                .lib
                .get(b"uat_free\0")
                .map_err(|e| anyhow::anyhow!("FFI symbol `uat_free` not found: {}", e))?;

            let response_ptr = invoke(c_request.as_ptr());
            if response_ptr.is_null() {
                anyhow::bail!("uat_invoke returned null (native fatal error)");
            }

            // Copy the response out, then hand the buffer back to the native side.
            let response_json = CStr::from_ptr(response_ptr).to_string_lossy().into_owned();
            free(response_ptr);

            log::info!("[SyncToolkit] Got response: {} bytes", response_json.len());
            Ok(response_json)
        }
    }

    fn send_request(&self, request: &UAssetRequest) -> Result<UAssetResponse> {
        let request_json = serde_json::to_string(request)?;
        let response_json = self.invoke_raw(&request_json)?;
        serde_json::from_str::<UAssetResponse>(&response_json).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse response: {} (Response: {})",
                e,
                &response_json[..std::cmp::min(500, response_json.len())]
            )
        })
    }

    pub fn batch_detect_skeletal_mesh(&self, file_paths: &[String]) -> Result<bool> {
        let request = UAssetRequest::BatchDetectSkeletalMesh {
            file_paths: file_paths.to_vec(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to batch detect skeletal mesh: {}", response.message);
        }
        Ok(response.data.and_then(|d| d.as_bool()).unwrap_or(false))
    }

    pub fn batch_detect_static_mesh(&self, file_paths: &[String]) -> Result<bool> {
        let request = UAssetRequest::BatchDetectStaticMesh {
            file_paths: file_paths.to_vec(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to batch detect static mesh: {}", response.message);
        }
        Ok(response.data.and_then(|d| d.as_bool()).unwrap_or(false))
    }

    pub fn batch_detect_texture(&self, file_paths: &[String]) -> Result<bool> {
        let request = UAssetRequest::BatchDetectTexture {
            file_paths: file_paths.to_vec(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to batch detect texture: {}", response.message);
        }
        Ok(response.data.and_then(|d| d.as_bool()).unwrap_or(false))
    }

    pub fn batch_detect_blueprint(&self, file_paths: &[String]) -> Result<bool> {
        let request = UAssetRequest::BatchDetectBlueprint {
            file_paths: file_paths.to_vec(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to batch detect blueprint: {}", response.message);
        }
        Ok(response.data.and_then(|d| d.as_bool()).unwrap_or(false))
    }

    pub fn is_texture_uasset(&self, file_path: &str) -> Result<bool> {
        self.batch_detect_texture(&[file_path.to_string()])
    }

    pub fn strip_mipmaps_native(&self, file_path: &str, usmap_path: Option<&str>) -> Result<bool> {
        let request = UAssetRequest::StripMipmapsNative {
            file_path: file_path.to_string(),
            usmap_path: usmap_path.map(|s| s.to_string()),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to strip mipmaps native: {}", response.message);
        }
        Ok(true)
    }

    pub fn convert_texture(&self, file_path: &str) -> Result<bool> {
        let request = UAssetRequest::ConvertTexture {
            file_path: file_path.to_string(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to convert texture: {}", response.message);
        }
        Ok(true)
    }

    pub fn set_no_mipmaps(&self, file_path: &str) -> Result<()> {
        let request = UAssetRequest::SetMipGen {
            file_path: file_path.to_string(),
            mip_gen: "NoMipmaps".to_string(),
        };
        let response = self.send_request(&request)?;
        if !response.success {
            anyhow::bail!("Failed to set no mipmaps: {}", response.message);
        }
        Ok(())
    }

    pub fn batch_has_inline_texture_data(
        &self,
        file_paths: &[String],
        usmap_path: Option<&str>,
    ) -> Result<Vec<String>> {
        let request = UAssetRequest::BatchHasInlineTextureData {
            file_paths: file_paths.to_vec(),
            usmap_path: usmap_path.map(|s| s.to_string()),
        };

        let response = self.send_request(&request)?;

        if !response.success {
            anyhow::bail!(
                "Failed to batch check inline texture data: {}",
                response.message
            );
        }

        // Parse response as list of file paths with inline data
        let inline_files = response
            .data
            .and_then(|d| d.as_array().cloned())
            .map(|arr| {
                arr.into_iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(inline_files)
    }

    pub fn batch_strip_mipmaps_native(
        &self,
        file_paths: &[String],
        usmap_path: Option<&str>,
        parallel: bool,
    ) -> Result<(usize, usize, usize, Vec<String>)> {
        let request = UAssetRequest::BatchStripMipmapsNative {
            file_paths: file_paths.to_vec(),
            usmap_path: usmap_path.map(|s| s.to_string()),
            parallel,
        };

        let response = self.send_request(&request)?;

        if !response.success {
            anyhow::bail!("Failed to batch strip mipmaps: {}", response.message);
        }

        let data = response.data.unwrap_or(serde_json::json!({}));
        let success_count = data
            .get("success_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let skip_count = data.get("skip_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let error_count = data
            .get("error_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let mut processed_files = Vec::new();
        if let Some(results) = data.get("results").and_then(|v| v.as_array()) {
            for result in results {
                if result
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    if result
                        .get("skipped")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    if let Some(path) = result.get("path").and_then(|v| v.as_str()) {
                        if let Some(file_name) = std::path::Path::new(path).file_stem() {
                            processed_files.push(file_name.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        Ok((success_count, skip_count, error_count, processed_files))
    }

    pub fn list_iostore_files(
        &self,
        file_path: &str,
        aes_key: Option<&str>,
    ) -> Result<IoStoreListResult> {
        let request = UAssetRequest::ListIoStoreFiles {
            file_path: file_path.to_string(),
            aes_key: aes_key.map(|s| s.to_string()),
        };

        let response = self.send_request(&request)?;

        if !response.success {
            anyhow::bail!("Failed to list IoStore files: {}", response.message);
        }

        let data = response.data.unwrap_or(serde_json::json!({}));
        let package_count = data
            .get("package_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let container_name = data
            .get("container_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let files = data
            .get("files")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(IoStoreListResult {
            package_count,
            container_name,
            files,
        })
    }

    pub fn create_mod_iostore(
        &self,
        output_path: &str,
        input_dir: &str,
        mount_point: Option<&str>,
        compress: Option<bool>,
        aes_key: Option<&str>,
        parallel: bool,
        obfuscate: bool,
        hybrid: bool,
    ) -> Result<IoStoreResult> {
        let request = UAssetRequest::CreateModIoStore {
            output_path: output_path.to_string(),
            input_dir: input_dir.to_string(),
            input_pak: None,
            mount_point: mount_point.map(|s| s.to_string()),
            compress,
            aes_key: aes_key.map(|s| s.to_string()),
            parallel,
            obfuscate,
            hybrid,
        };

        let response = self.send_request(&request)?;

        if !response.success {
            anyhow::bail!("Failed to create mod IoStore: {}", response.message);
        }

        let data = response.data.unwrap_or(serde_json::json!({}));
        let utoc_path = data
            .get("utoc_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ucas_path = data
            .get("ucas_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pak_path = data
            .get("pak_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let converted_count = data
            .get("converted_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let file_count = data.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        Ok(IoStoreResult {
            utoc_path,
            ucas_path,
            pak_path,
            converted_count,
            file_count,
        })
    }

    /// Create a mod IoStore bundle directly from a PAK file (extracts internally)
    pub fn create_mod_iostore_from_pak(
        &self,
        output_path: &str,
        input_pak: &str,
        mount_point: Option<&str>,
        compress: Option<bool>,
        aes_key: Option<&str>,
        parallel: bool,
        obfuscate: bool,
        hybrid: bool,
    ) -> Result<IoStoreResult> {
        let request = UAssetRequest::CreateModIoStore {
            output_path: output_path.to_string(),
            input_dir: String::new(), // Not used when input_pak is set
            input_pak: Some(input_pak.to_string()),
            mount_point: mount_point.map(|s| s.to_string()),
            compress,
            aes_key: aes_key.map(|s| s.to_string()),
            parallel,
            obfuscate,
            hybrid,
        };

        log::info!(
            "[SyncToolkit] Creating IoStore from PAK: {} -> {}",
            input_pak,
            output_path
        );
        let response = self.send_request(&request)?;

        if !response.success {
            anyhow::bail!(
                "Failed to create mod IoStore from PAK: {}",
                response.message
            );
        }

        let data = response.data.unwrap_or(serde_json::json!({}));
        let utoc_path = data
            .get("utoc_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ucas_path = data
            .get("ucas_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pak_path = data
            .get("pak_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let converted_count = data
            .get("converted_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let file_count = data.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        log::info!(
            "[SyncToolkit] IoStore created: utoc={}, converted={}",
            utoc_path,
            converted_count
        );
        Ok(IoStoreResult {
            utoc_path,
            ucas_path,
            pak_path,
            converted_count,
            file_count,
        })
    }
}

/// Get or initialize the global synchronous toolkit
pub fn get_global_toolkit() -> Result<&'static SyncToolkit> {
    let toolkit = GLOBAL_TOOLKIT_SYNC.get_or_init(|| {
        log::info!("[SyncToolkit] Initializing global singleton...");
        match SyncToolkit::new(None) {
            Ok(t) => {
                log::info!("[SyncToolkit] Global singleton created successfully");
                t
            }
            Err(e) => {
                log::error!("[SyncToolkit] Failed to create singleton: {}", e);
                panic!("Failed to initialize SyncToolkit: {}", e);
            }
        }
    });
    Ok(toolkit)
}

/// Initialize the global toolkit at app startup
pub fn init_global_toolkit() -> Result<()> {
    get_global_toolkit()?;
    log::info!("[SyncToolkit] Global singleton initialized successfully");
    Ok(())
}

/// Invoke the native UAssetTool with a raw JSON request envelope and return the raw JSON
/// response, going through the process-global toolkit (loaded on first use). For callers
/// that build their own request JSON and parse their own response shape — e.g. the VFX
/// pipeline — while still using the in-process FFI instead of spawning a child process.
pub fn invoke_json(request_json: &str) -> Result<String> {
    let toolkit = get_global_toolkit()?;
    toolkit.invoke_raw(request_json)
}

/// Resolve the on-disk path of the native UAssetTool library (the location the FFI loads
/// it from — beside the executable, then legacy/dev fallbacks). Exposed so callers can
/// surface the tool location to the UI/logs; loading itself is handled by the FFI.
pub fn native_library_path() -> Result<String> {
    SyncToolkit::find_dll_path()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum UAssetRequest {
    #[serde(rename = "detect_texture")]
    DetectTexture { file_path: String },
    #[serde(rename = "set_mip_gen")]
    SetMipGen { file_path: String, mip_gen: String },
    #[serde(rename = "get_texture_info")]
    GetTextureInfo { file_path: String },
    #[serde(rename = "detect_mesh")]
    DetectMesh { file_path: String },
    #[serde(rename = "detect_skeletal_mesh")]
    DetectSkeletalMesh { file_path: String },
    #[serde(rename = "detect_static_mesh")]
    DetectStaticMesh { file_path: String },
    #[serde(rename = "patch_mesh")]
    PatchMesh {
        file_path: String,
        uexp_path: String,
    },
    #[serde(rename = "get_mesh_info")]
    GetMeshInfo { file_path: String },
    // Batch detection - sends all files at once, returns first match
    #[serde(rename = "batch_detect_skeletal_mesh")]
    BatchDetectSkeletalMesh { file_paths: Vec<String> },
    #[serde(rename = "batch_detect_static_mesh")]
    BatchDetectStaticMesh { file_paths: Vec<String> },
    #[serde(rename = "batch_detect_texture")]
    BatchDetectTexture { file_paths: Vec<String> },
    #[serde(rename = "batch_detect_blueprint")]
    BatchDetectBlueprint { file_paths: Vec<String> },
    // Texture conversion using UE4-DDS-Tools (export -> re-inject with no_mipmaps)
    #[serde(rename = "convert_texture")]
    ConvertTexture { file_path: String },
    #[serde(rename = "strip_mipmaps")]
    StripMipmaps { file_path: String },
    // Native C# mipmap stripping using UAssetAPI TextureExport
    #[serde(rename = "strip_mipmaps_native")]
    StripMipmapsNative {
        file_path: String,
        usmap_path: Option<String>,
    },
    // Batch native C# mipmap stripping - processes multiple files in one call
    #[serde(rename = "batch_strip_mipmaps_native")]
    BatchStripMipmapsNative {
        file_paths: Vec<String>,
        usmap_path: Option<String>,
        #[serde(default)]
        parallel: bool,
    },
    // Check if texture has inline data (no .ubulk needed)
    #[serde(rename = "has_inline_texture_data")]
    HasInlineTextureData {
        file_path: String,
        usmap_path: Option<String>,
    },
    // Batch check for inline texture data - returns list of files with inline data
    #[serde(rename = "batch_has_inline_texture_data")]
    BatchHasInlineTextureData {
        file_paths: Vec<String>,
        usmap_path: Option<String>,
    },

    // PAK operations
    #[serde(rename = "list_pak")]
    ListPak {
        file_path: String,
        aes_key: Option<String>,
        filter_patterns: Option<Vec<String>>,
    },
    #[serde(rename = "extract_pak_file")]
    ExtractPakFile {
        file_path: String,
        internal_path: String,
        output_path: String,
        aes_key: Option<String>,
    },
    #[serde(rename = "extract_pak_all")]
    ExtractPakAll {
        file_path: String,
        output_path: String,
        aes_key: Option<String>,
    },
    #[serde(rename = "create_pak")]
    CreatePak {
        output_path: String,
        file_paths: Vec<String>,
        mount_point: Option<String>,
        path_hash_seed: Option<u64>,
        aes_key: Option<String>,
    },
    #[serde(rename = "create_companion_pak")]
    CreateCompanionPak {
        output_path: String,
        file_paths: Vec<String>,
        mount_point: Option<String>,
        path_hash_seed: Option<u64>,
        aes_key: Option<String>,
    },

    // IoStore operations
    #[serde(rename = "list_iostore_files")]
    ListIoStoreFiles {
        file_path: String,
        aes_key: Option<String>,
    },
    #[serde(rename = "create_iostore")]
    CreateIoStore {
        output_path: String,
        input_dir: String,
        usmap_path: Option<String>,
        compress: Option<bool>,
        aes_key: Option<String>,
    },
    #[serde(rename = "is_iostore_compressed")]
    IsIoStoreCompressed { file_path: String },
    #[serde(rename = "is_iostore_encrypted")]
    IsIoStoreEncrypted { file_path: String },
    #[serde(rename = "recompress_iostore")]
    RecompressIoStore { file_path: String },
    #[serde(rename = "extract_iostore")]
    ExtractIoStore {
        file_path: String,
        output_path: String,
        aes_key: Option<String>,
    },
    #[serde(rename = "extract_script_objects")]
    ExtractScriptObjects {
        file_path: String,
        output_path: String,
    },
    #[serde(rename = "create_mod_iostore")]
    CreateModIoStore {
        output_path: String,
        input_dir: String,
        input_pak: Option<String>,
        mount_point: Option<String>,
        compress: Option<bool>,
        aes_key: Option<String>,
        #[serde(default)]
        parallel: bool,
        #[serde(default)]
        obfuscate: bool,
        /// Hybrid mode: embed non-Unreal files (audio/.png/.bin/...) as loose entries in the
        /// companion PAK instead of dropping them. Defaults off for backward compatibility.
        #[serde(default)]
        hybrid: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UAssetResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TextureInfo {
    pub mip_gen_settings: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeshInfo {
    pub material_count: Option<i32>,
    pub vertex_count: Option<i32>,
    pub triangle_count: Option<i32>,
    pub is_skeletal_mesh: Option<bool>,
}

// ============================================================================
// GLOBAL SYNC API - Module-level functions using the global singleton
// ============================================================================

/// Batch strip mipmaps from multiple textures (using global singleton)
/// Returns (success_count, skip_count, error_count, processed_file_names)
pub fn batch_strip_mipmaps_native(
    file_paths: &[String],
    usmap_path: Option<&str>,
) -> Result<(usize, usize, usize, Vec<String>)> {
    batch_strip_mipmaps_native_parallel(file_paths, usmap_path, false)
}

/// Batch strip mipmaps with parallel processing option (using global singleton)
/// Returns (success_count, skip_count, error_count, processed_file_names)
pub fn batch_strip_mipmaps_native_parallel(
    file_paths: &[String],
    usmap_path: Option<&str>,
    parallel: bool,
) -> Result<(usize, usize, usize, Vec<String>)> {
    let toolkit = get_global_toolkit()?;
    toolkit.batch_strip_mipmaps_native(file_paths, usmap_path, parallel)
}

// Type aliases for backward compatibility
pub type UAssetToolkit = SyncToolkit;
pub type UAssetToolkitSync = SyncToolkit;

/// Check if a file is a skeletal mesh (using global singleton)
pub fn is_skeletal_mesh_uasset(file_path: &str) -> Result<bool> {
    let toolkit = get_global_toolkit()?;
    toolkit.batch_detect_skeletal_mesh(&[file_path.to_string()])
}

/// Check if a file is a texture (using global singleton)
pub fn is_texture_uasset(file_path: &str) -> Result<bool> {
    let toolkit = get_global_toolkit()?;
    toolkit.batch_detect_texture(&[file_path.to_string()])
}

/// Check if a file is a static mesh (using global singleton)
pub fn is_static_mesh_uasset(file_path: &str) -> Result<bool> {
    let toolkit = get_global_toolkit()?;
    toolkit.batch_detect_static_mesh(&[file_path.to_string()])
}

/// Recompress an IoStore file
pub fn recompress_iostore(file_path: &str) -> Result<()> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::RecompressIoStore {
        file_path: file_path.to_string(),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to recompress IoStore: {}", response.message);
    }
    Ok(())
}

/// Extract files from an IoStore to legacy format
pub fn extract_iostore(file_path: &str, output_path: &str, aes_key: Option<&str>) -> Result<usize> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::ExtractIoStore {
        file_path: file_path.to_string(),
        output_path: output_path.to_string(),
        aes_key: aes_key.map(|s| s.to_string()),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to extract IoStore: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let converted = data
        .get("extracted_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    Ok(converted)
}

/// Extract script objects from an IoStore
pub fn extract_script_objects(file_path: &str, output_path: &str) -> Result<usize> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::ExtractScriptObjects {
        file_path: file_path.to_string(),
        output_path: output_path.to_string(),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to extract script objects: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let count = data.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    Ok(count)
}

/// Check if IoStore is compressed
pub fn is_iostore_compressed(file_path: &str) -> Result<bool> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::IsIoStoreCompressed {
        file_path: file_path.to_string(),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to check IoStore compression: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let compressed = data
        .get("compressed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(compressed)
}

/// Check if IoStore is encrypted (obfuscated)
pub fn is_iostore_encrypted(file_path: &str) -> Result<bool> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::IsIoStoreEncrypted {
        file_path: file_path.to_string(),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to check IoStore encryption: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let encrypted = data
        .get("encrypted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(encrypted)
}

/// IoStore creation result
#[derive(Debug, Serialize, Deserialize)]
pub struct IoStoreResult {
    pub utoc_path: String,
    pub ucas_path: String,
    pub pak_path: String,
    pub converted_count: usize,
    pub file_count: usize,
}

/// Create mod IoStore
/// parallel: when true, uses 75% of CPU threads; when false, uses 50%
pub fn create_mod_iostore(
    output_path: &str,
    input_dir: &str,
    mount_point: Option<&str>,
    compress: Option<bool>,
    aes_key: Option<&str>,
    parallel: bool,
    obfuscate: bool,
    hybrid: bool,
) -> Result<IoStoreResult> {
    let toolkit = get_global_toolkit()?;
    toolkit.create_mod_iostore(
        output_path,
        input_dir,
        mount_point,
        compress,
        aes_key,
        parallel,
        obfuscate,
        hybrid,
    )
}

/// Create mod IoStore directly from a PAK file (extracts internally in C# tool)
/// This is more efficient than extract_pak_all + create_mod_iostore as it avoids
/// writing to a temp directory on the Rust side.
pub fn create_mod_iostore_from_pak(
    output_path: &str,
    input_pak: &str,
    mount_point: Option<&str>,
    compress: Option<bool>,
    aes_key: Option<&str>,
    parallel: bool,
    obfuscate: bool,
    hybrid: bool,
) -> Result<IoStoreResult> {
    let toolkit = get_global_toolkit()?;
    toolkit.create_mod_iostore_from_pak(
        output_path,
        input_pak,
        mount_point,
        compress,
        aes_key,
        parallel,
        obfuscate,
        hybrid,
    )
}

/// Patch mesh materials
pub fn patch_mesh(file_path: &str, uexp_path: &str) -> Result<()> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::PatchMesh {
        file_path: file_path.to_string(),
        uexp_path: uexp_path.to_string(),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to patch mesh: {}", response.message);
    }
    Ok(())
}

/// List files in IoStore
pub fn list_iostore_files(file_path: &str, aes_key: Option<&str>) -> Result<IoStoreListResult> {
    let toolkit = get_global_toolkit()?;
    toolkit.list_iostore_files(file_path, aes_key)
}

/// List the file paths inside a PAK (paths only).
///
/// Thin convenience wrapper over [`list_pak`]: all PAK listing goes through the
/// single `list_pak` JSON action, and this projects out just the file paths for
/// the many call sites that don't need the per-entry size/flags. This avoids a
/// second listing action whose response schema can drift from `list_pak`.
pub fn list_pak_files(file_path: &str, aes_key: Option<&str>) -> Result<Vec<String>> {
    let listing = list_pak(file_path, aes_key, None)?;
    Ok(listing.files.into_iter().map(|e| e.path).collect())
}

/// One entry returned by `list_pak`: a file inside a PAK with its size and flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakListEntry {
    pub path: String,
    pub size: u64,
    pub compressed_size: u64,
    pub encrypted: bool,
    pub compressed: bool,
}

/// Full result returned by `list_pak`: header metadata plus per-file entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakListResult {
    pub pak_path: String,
    pub version: i32,
    pub mount_point: String,
    pub encrypted_index: bool,
    pub total_files: usize,
    pub filtered_count: usize,
    pub total_uncompressed_bytes: u64,
    pub total_compressed_bytes: u64,
    pub files: Vec<PakListEntry>,
}

/// List the contents of a PAK without extracting it. Optionally filter entries
/// by substring patterns (case-insensitive, matches the JSON API).
pub fn list_pak(
    file_path: &str,
    aes_key: Option<&str>,
    filter_patterns: Option<Vec<String>>,
) -> Result<PakListResult> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::ListPak {
        file_path: file_path.to_string(),
        aes_key: aes_key.map(|s| s.to_string()),
        filter_patterns,
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to list PAK: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let parsed: PakListResult = serde_json::from_value(data)
        .map_err(|e| anyhow::anyhow!("Failed to parse list_pak response: {}", e))?;
    Ok(parsed)
}

/// Extract all files from a PAK to a directory (using global singleton)
pub fn extract_pak_all(file_path: &str, output_path: &str, aes_key: Option<&str>) -> Result<usize> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::ExtractPakAll {
        file_path: file_path.to_string(),
        output_path: output_path.to_string(),
        aes_key: aes_key.map(|s| s.to_string()),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to extract PAK: {}", response.message);
    }
    let data = response.data.unwrap_or(serde_json::json!({}));
    let count = data
        .get("extracted_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    Ok(count)
}

/// Create a PAK file from a list of files (using global singleton)
pub fn create_pak(
    output_path: &str,
    file_paths: Vec<(String, String)>,
    mount_point: Option<&str>,
    path_hash_seed: Option<u64>,
    aes_key: Option<&str>,
) -> Result<()> {
    let toolkit = get_global_toolkit()?;
    let request = UAssetRequest::CreatePak {
        output_path: output_path.to_string(),
        file_paths: file_paths
            .into_iter()
            .map(|(rel, abs)| format!("{}={}", rel, abs))
            .collect(),
        mount_point: mount_point.map(|s| s.to_string()),
        path_hash_seed,
        aes_key: aes_key.map(|s| s.to_string()),
    };
    let response = toolkit.send_request(&request)?;
    if !response.success {
        anyhow::bail!("Failed to create PAK: {}", response.message);
    }
    Ok(())
}

/// IoStore listing result
#[derive(Debug, Serialize, Deserialize)]
pub struct IoStoreListResult {
    pub package_count: usize,
    pub container_name: String,
    pub files: Vec<String>,
}
