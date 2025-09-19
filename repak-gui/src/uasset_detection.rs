use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use log::{debug, warn, error};
use uasset_mesh_patch_rivals::{process_mesh_file, is_mesh_uasset};

/// Detects if a UAsset file is a mesh using the integrated mesh patch library
pub fn detect_mesh_with_toolkit(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    // Use the integrated uasset-mesh-patch-rivals library
    match is_mesh_uasset(path) {
        Ok(result) => Ok(result),
        Err(_) => {
            // Fallback to heuristic detection
            Ok(is_mesh_uasset_heuristic(path))
        }
    }
}

/// Detects if a UAsset file is a texture using heuristic analysis
pub fn detect_texture_with_toolkit(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    // For now, use heuristic detection for textures
    // TODO: Integrate UAssetAPI for proper texture detection
    Ok(is_texture_uasset_heuristic(path))
}

/// Heuristic detection for mesh UAsset files
pub fn is_mesh_uasset_heuristic(path: &Path) -> bool {
    let file_name = path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let path_str = path.to_string_lossy().to_lowercase();
    
    let mesh_indicators = [
        "mesh", "sk_", "sm_", "skeletal", "static", "character", "weapon", 
        "armor", "body", "head", "hair", "face", "/meshes/", "\\meshes\\", 
        "_mesh", "model", "_sk", "_sm", "hero"
    ];
    
    // Pattern matching
    let has_mesh_pattern = mesh_indicators.iter().any(|indicator| {
        file_name.contains(indicator) || path_str.contains(indicator)
    });
    
    // File size threshold (meshes are typically larger than 50KB)
    let has_size_threshold = fs::metadata(path)
        .map(|m| m.len() > 50000)
        .unwrap_or(false);
    
    has_mesh_pattern || has_size_threshold
}

/// Heuristic detection for texture UAsset files
pub fn is_texture_uasset_heuristic(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    
    // Path-based indicators
    let path_indicates_texture = 
        path_str.contains("texture") ||
        path_str.contains("/textures/") ||
        path_str.contains("\\textures\\") ||
        path_str.contains("_t.") ||      // UE naming convention
        path_str.contains("_diffuse") ||
        path_str.contains("_normal") ||
        path_str.contains("_specular") ||
        path_str.contains("_albedo") ||
        path_str.contains("_roughness") ||
        path_str.contains("_metallic");
    
    if path_indicates_texture {
        return true;
    }
    
    // Binary analysis fallback
    analyze_uasset_for_texture_class(path).unwrap_or(false)
}

/// Analyzes UAsset binary for texture class references
fn analyze_uasset_for_texture_class(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let data_str = String::from_utf8_lossy(&data);
    
    // Look for UTexture2D class references
    let has_texture_class = 
        data_str.contains("UTexture2D") ||
        data_str.contains("TextureSource") ||
        data_str.contains("MipGenSettings") ||
        data_str.contains("TMGS_");
    
    Ok(has_texture_class)
}

/// Detects mesh files in a list of mod contents
pub fn detect_mesh_files(mod_contents: &[String]) -> bool {
    mod_contents.iter().any(|file| {
        let lower_file = file.to_lowercase();
        
        if lower_file.ends_with(".uasset") {
            let path = PathBuf::from(file);
            
            // Use UAsset Toolkit for accurate detection
            if let Ok(is_mesh) = detect_mesh_with_toolkit(&path) {
                return is_mesh;
            }
            
            // Fallback to heuristics
            return is_mesh_uasset_heuristic(&path);
        }
        false
    })
}

/// Detects texture files in a list of mod contents
pub fn detect_texture_files(mod_contents: &[String]) -> bool {
    mod_contents.iter().any(|file| {
        let lower_file = file.to_lowercase();
        
        // Image file extensions
        if lower_file.ends_with(".png") ||
           lower_file.ends_with(".jpg") ||
           lower_file.ends_with(".jpeg") ||
           lower_file.ends_with(".dds") ||
           lower_file.ends_with(".tga") {
            return true;
        }
        
        // UAsset files in texture directories
        if lower_file.ends_with(".uasset") {
            let path = PathBuf::from(file);
            
            // Use UAsset Toolkit for accurate detection
            if let Ok(is_texture) = detect_texture_with_toolkit(&path) {
                return is_texture;
            }
            
            // Fallback to heuristics
            return is_texture_uasset_heuristic(&path);
        }
        
        false
    })
}

/// Patches mesh files using available tools
pub fn patch_mesh_files(paths: &mut Vec<PathBuf>, mod_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let uasset_files: Vec<_> = paths
        .iter()
        .filter(|p| {
            p.extension().and_then(|ext| ext.to_str()) == Some("uasset") &&
            is_mesh_uasset_heuristic(p)
        })
        .collect();

    debug!("Found {} mesh files to patch", uasset_files.len());

    // Process each mesh file
    for uasset_file in &uasset_files {
        let uexp_file = uasset_file.with_extension("uexp");
        
        if !uexp_file.exists() {
            warn!("Missing .uexp file for mesh: {:?}", uasset_file);
            continue;
        }
        
        // Create backups
        if let Err(e) = fs::copy(&uexp_file, format!("{}.bak", uexp_file.display())) {
            warn!("Failed to create backup for {}: {}", uexp_file.display(), e);
        }
        if let Err(e) = fs::copy(uasset_file, format!("{}.bak", uasset_file.display())) {
            warn!("Failed to create backup for {}: {}", uasset_file.display(), e);
        }
        
        // Try to patch using the integrated library
        if let Err(e) = patch_single_mesh_file(uasset_file, &uexp_file) {
            error!("Failed to patch mesh file {:?}: {}", uasset_file, e);
        } else {
            debug!("Successfully patched mesh file: {:?}", uasset_file);
        }
    }
    
    Ok(())
}

/// Patches mesh files using UAssetAPI toolkit with fallback to integrated library
pub fn patch_single_mesh_file(uasset_path: &Path, uexp_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Patching mesh files: {:?} and {:?}", uasset_path, uexp_path);
    
    // Try UAssetAPI toolkit first
    match crate::uasset_api_integration::process_mesh_with_uasset_api(uasset_path, uexp_path) {
        Ok(true) => {
            debug!("Successfully patched mesh files using UAssetAPI toolkit");
            return Ok(());
        }
        Ok(false) => {
            debug!("UAssetAPI toolkit not available, falling back to integrated library");
        }
        Err(e) => {
            warn!("UAssetAPI toolkit mesh patching failed: {}", e);
        }
    }
    
    // Fallback to integrated mesh patch library
    match process_mesh_file(uasset_path, uexp_path) {
        Ok(()) => {
            debug!("Successfully patched mesh files using integrated library");
            Ok(())
        }
        Err(e) => {
            error!("Mesh patching failed: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Modifies texture mipmaps using available tools
pub fn modify_texture_mipmaps(uasset_path: &Path, uexp_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    // Try using uasset-mesh-patch-rivals for texture processing
    let toolkit_paths = [
        "./uasset-mesh-patch-rivals/target/release/uasset-mesh-patch-rivals.exe",
        "./uasset-mesh-patch-rivals/target/debug/uasset-mesh-patch-rivals.exe",
        "../uasset-mesh-patch-rivals/target/release/uasset-mesh-patch-rivals.exe",
        "../uasset-mesh-patch-rivals/target/debug/uasset-mesh-patch-rivals.exe",
    ];
    
    for toolkit_path in &toolkit_paths {
        if std::path::Path::new(toolkit_path).exists() {
            let output = Command::new(toolkit_path)
                .arg("process-texture")
                .arg(uasset_path)
                .output();
                
            if let Ok(output) = output {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    if output_str.contains("Texture detected and set to NoMipmaps") {
                        return Ok(true);
                    }
                }
            }
        }
    }
    
    Ok(false)
}
