use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as AsyncCommand;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

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
    #[serde(rename = "patch_mesh")]
    PatchMesh { file_path: String, uexp_path: String },
    #[serde(rename = "get_mesh_info")]
    GetMeshInfo { file_path: String },
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

pub struct UAssetToolkit {
    bridge_path: String,
}

impl UAssetToolkit {
    /// Create a new UAssetToolkit instance
    /// 
    /// # Arguments
    /// * `bridge_path` - Path to the UAssetBridge executable. If None, will try to find it in target/uassetbridge/
    pub fn new(bridge_path: Option<String>) -> Result<Self> {
        let bridge_path = match bridge_path {
            Some(path) => path,
            None => {
                // Try to find the bridge in the expected location
                let exe_path = std::env::current_exe()?;
                let exe_dir = exe_path.parent().context("Failed to get executable directory")?;
                let bridge_path = exe_dir.join("uassetbridge").join("UAssetBridge.exe");
                
                if !bridge_path.exists() {
                    // Try relative to workspace
                    let workspace_bridge = Path::new("target/uassetbridge/UAssetBridge.exe");
                    if workspace_bridge.exists() {
                        workspace_bridge.to_string_lossy().to_string()
                    } else {
                        anyhow::bail!("UAssetBridge.exe not found. Please build the project first or provide explicit path.");
                    }
                } else {
                    bridge_path.to_string_lossy().to_string()
                }
            }
        };

        if !Path::new(&bridge_path).exists() {
            anyhow::bail!("UAssetBridge executable not found at: {}", bridge_path);
        }

        Ok(Self { bridge_path })
    }

    async fn send_request(&self, request: UAssetRequest) -> Result<UAssetResponse> {
        let mut cmd = AsyncCommand::new(&self.bridge_path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW flag on Windows
        
        let mut child = cmd.spawn()
            .context("Failed to spawn UAssetBridge process")?;

        let stdin = child.stdin.as_mut().context("Failed to get stdin")?;
        let request_json = serde_json::to_string(&request)?;
        
        stdin.write_all(request_json.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        drop(stdin);

        let output = child.wait_with_output().await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("UAssetBridge failed: {}", stderr);
        }

        let response_str = String::from_utf8(output.stdout)?;
        let response: UAssetResponse = serde_json::from_str(&response_str.trim())?;
        
        Ok(response)
    }

    /// Check if a uasset file is a texture asset
    pub async fn is_texture_uasset(&self, file_path: &str) -> Result<bool> {
        let request = UAssetRequest::DetectTexture {
            file_path: file_path.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to detect texture: {}", response.message);
        }
        
        Ok(response.data
            .and_then(|d| d.as_bool())
            .unwrap_or(false))
    }

    /// Check if a uasset file is a mesh asset
    pub async fn is_mesh_uasset(&self, file_path: &str) -> Result<bool> {
        let request = UAssetRequest::DetectMesh {
            file_path: file_path.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to detect mesh: {}", response.message);
        }
        
        Ok(response.data
            .and_then(|d| d.as_bool())
            .unwrap_or(false))
    }

    /// Set mip generation settings to NoMipmaps for a texture uasset
    pub async fn set_no_mipmaps(&self, file_path: &str) -> Result<()> {
        self.set_mip_gen(file_path, "NoMipmaps").await
    }

    /// Set mip generation settings for a texture uasset
    pub async fn set_mip_gen(&self, file_path: &str, mip_gen: &str) -> Result<()> {
        let request = UAssetRequest::SetMipGen {
            file_path: file_path.to_string(),
            mip_gen: mip_gen.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to set mip gen: {}", response.message);
        }
        
        Ok(())
    }

    /// Get texture information from a uasset file
    pub async fn get_texture_info(&self, file_path: &str) -> Result<TextureInfo> {
        let request = UAssetRequest::GetTextureInfo {
            file_path: file_path.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to get texture info: {}", response.message);
        }
        
        let data = response.data.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let texture_info = TextureInfo {
            mip_gen_settings: data.get("MipGenSettings").and_then(|v| v.as_str()).map(|s| s.to_string()),
            width: data.get("Width").and_then(|v| v.as_i64()).map(|i| i as i32),
            height: data.get("Height").and_then(|v| v.as_i64()).map(|i| i as i32),
            format: data.get("Format").and_then(|v| v.as_str()).map(|s| s.to_string()),
        };
        
        Ok(texture_info)
    }

    /// Patch mesh materials in a uasset file
    pub async fn patch_mesh(&self, file_path: &str, uexp_path: &str) -> Result<()> {
        let request = UAssetRequest::PatchMesh {
            file_path: file_path.to_string(),
            uexp_path: uexp_path.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to patch mesh: {}", response.message);
        }
        
        Ok(())
    }

    /// Get mesh information from a uasset file
    pub async fn get_mesh_info(&self, file_path: &str) -> Result<MeshInfo> {
        let request = UAssetRequest::GetMeshInfo {
            file_path: file_path.to_string(),
        };
        
        let response = self.send_request(request).await?;
        
        if !response.success {
            anyhow::bail!("Failed to get mesh info: {}", response.message);
        }
        
        let data = response.data.unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let mesh_info = MeshInfo {
            material_count: data.get("MaterialCount").and_then(|v| v.as_i64()).map(|i| i as i32),
            vertex_count: data.get("VertexCount").and_then(|v| v.as_i64()).map(|i| i as i32),
            triangle_count: data.get("TriangleCount").and_then(|v| v.as_i64()).map(|i| i as i32),
            is_skeletal_mesh: data.get("IsSkeletalMesh").and_then(|v| v.as_bool()),
        };
        
        Ok(mesh_info)
    }

    /// Process a uasset file: detect if it's a texture and set NoMipmaps if it is
    /// Returns true if the file was processed (was a texture), false otherwise
    pub async fn process_texture_uasset(&self, file_path: &str) -> Result<bool> {
        if self.is_texture_uasset(file_path).await? {
            self.set_no_mipmaps(file_path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Process a mesh uasset file: detect if it's a mesh and patch materials if it is
    /// Returns true if the file was processed (was a mesh), false otherwise
    pub async fn process_mesh_uasset(&self, file_path: &str, uexp_path: &str) -> Result<bool> {
        if self.is_mesh_uasset(file_path).await? {
            self.patch_mesh(file_path, uexp_path).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Batch process multiple uasset files
    /// Returns a vector of (file_path, was_processed, error_message)
    pub async fn batch_process_textures(&self, file_paths: &[String]) -> Vec<(String, bool, Option<String>)> {
        let mut results = Vec::new();
        
        for file_path in file_paths {
            match self.process_texture_uasset(file_path).await {
                Ok(was_processed) => results.push((file_path.clone(), was_processed, None)),
                Err(e) => results.push((file_path.clone(), false, Some(e.to_string()))),
            }
        }
        
        results
    }
}

/// Synchronous wrapper for common operations (blocks on tokio runtime)
pub struct UAssetToolkitSync {
    toolkit: UAssetToolkit,
    runtime: tokio::runtime::Runtime,
}

impl UAssetToolkitSync {
    pub fn new(bridge_path: Option<String>) -> Result<Self> {
        let toolkit = UAssetToolkit::new(bridge_path)?;
        let runtime = tokio::runtime::Runtime::new()?;
        Ok(Self { toolkit, runtime })
    }

    pub fn is_texture_uasset(&self, file_path: &str) -> Result<bool> {
        self.runtime.block_on(self.toolkit.is_texture_uasset(file_path))
    }

    pub fn is_mesh_uasset(&self, file_path: &str) -> Result<bool> {
        self.runtime.block_on(self.toolkit.is_mesh_uasset(file_path))
    }

    pub fn set_no_mipmaps(&self, file_path: &str) -> Result<()> {
        self.runtime.block_on(self.toolkit.set_no_mipmaps(file_path))
    }

    pub fn patch_mesh(&self, file_path: &str, uexp_path: &str) -> Result<()> {
        self.runtime.block_on(self.toolkit.patch_mesh(file_path, uexp_path))
    }

    pub fn process_texture_uasset(&self, file_path: &str) -> Result<bool> {
        self.runtime.block_on(self.toolkit.process_texture_uasset(file_path))
    }

    pub fn process_mesh_uasset(&self, file_path: &str, uexp_path: &str) -> Result<bool> {
        self.runtime.block_on(self.toolkit.process_mesh_uasset(file_path, uexp_path))
    }

    pub fn get_texture_info(&self, file_path: &str) -> Result<TextureInfo> {
        self.runtime.block_on(self.toolkit.get_texture_info(file_path))
    }

    pub fn get_mesh_info(&self, file_path: &str) -> Result<MeshInfo> {
        self.runtime.block_on(self.toolkit.get_mesh_info(file_path))
    }
}
