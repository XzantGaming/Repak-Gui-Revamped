use std::path::Path;
use log::{debug, warn};
use uasset_toolkit::{UAssetToolkitSync, TextureInfo, MeshInfo};

/// Integration module for UAssetAPI from GitHub
/// This module provides enhanced detection and processing capabilities for UAsset files

/// Detects if a UAsset file is a mesh using UAssetAPI toolkit
pub fn detect_mesh_with_uasset_api(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    debug!("Detecting mesh with UAssetAPI toolkit: {:?}", path);
    
    // Try UAssetAPI toolkit first for most accurate detection
    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            match toolkit.is_mesh_uasset(&path.to_string_lossy()) {
                Ok(is_mesh) => {
                    debug!("UAssetAPI toolkit mesh detection result: {}", is_mesh);
                    return Ok(is_mesh);
                }
                Err(e) => {
                    warn!("UAssetAPI toolkit mesh detection failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to initialize UAssetAPI toolkit: {}", e);
        }
    }
    
    // Fallback to existing mesh patch library detection
    match crate::uasset_detection::detect_mesh_with_toolkit(path) {
        Ok(result) => Ok(result),
        Err(e) => {
            warn!("Mesh patch library detection failed, using heuristics: {}", e);
            Ok(crate::uasset_detection::is_mesh_uasset_heuristic(path))
        }
    }
}

/// Detects if a UAsset file is a texture using UAssetAPI toolkit
pub fn detect_texture_with_uasset_api(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    debug!("Detecting texture with UAssetAPI toolkit: {:?}", path);
    
    // Try UAssetAPI toolkit first for most accurate detection
    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            match toolkit.is_texture_uasset(&path.to_string_lossy()) {
                Ok(is_texture) => {
                    debug!("UAssetAPI toolkit texture detection result: {}", is_texture);
                    return Ok(is_texture);
                }
                Err(e) => {
                    warn!("UAssetAPI toolkit texture detection failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to initialize UAssetAPI toolkit: {}", e);
        }
    }
    
    // Fallback to heuristic detection with binary analysis
    Ok(crate::uasset_detection::is_texture_uasset_heuristic(path))
}

/// Processes texture files using UAssetAPI toolkit for MipGenSettings modification
pub fn process_texture_with_uasset_api(uasset_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    debug!("Processing texture with UAssetAPI toolkit: {:?}", uasset_path);
    
    // Try UAssetAPI toolkit first for most accurate processing
    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            match toolkit.process_texture_uasset(&uasset_path.to_string_lossy()) {
                Ok(was_processed) => {
                    if was_processed {
                        debug!("UAssetAPI toolkit successfully processed texture: {:?}", uasset_path);
                        return Ok(true);
                    } else {
                        debug!("UAssetAPI toolkit determined file is not a texture: {:?}", uasset_path);
                    }
                }
                Err(e) => {
                    warn!("UAssetAPI toolkit texture processing failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to initialize UAssetAPI toolkit: {}", e);
        }
    }
    
    // Return false to indicate fallback to existing processing methods
    Ok(false)
}

/// Enhanced asset analysis using UAssetAPI toolkit
pub fn analyze_asset_with_uasset_api(path: &Path) -> Result<AssetInfo, Box<dyn std::error::Error>> {
    debug!("Analyzing asset with UAssetAPI toolkit: {:?}", path);
    
    // Use UAssetAPI toolkit for accurate detection
    let is_mesh = detect_mesh_with_uasset_api(path)?;
    let is_texture = detect_texture_with_uasset_api(path)?;
    
    // Get detailed information if available
    let mut properties = Vec::new();
    
    if is_texture {
        // Try to get texture information
        if let Ok(toolkit) = UAssetToolkitSync::new(None) {
            if let Ok(texture_info) = toolkit.get_texture_info(&path.to_string_lossy()) {
                if let Some(mip_gen) = texture_info.mip_gen_settings {
                    properties.push(AssetProperty {
                        name: "MipGenSettings".to_string(),
                        property_type: "String".to_string(),
                        value: mip_gen,
                    });
                }
                if let Some(width) = texture_info.width {
                    properties.push(AssetProperty {
                        name: "Width".to_string(),
                        property_type: "Int32".to_string(),
                        value: width.to_string(),
                    });
                }
                if let Some(height) = texture_info.height {
                    properties.push(AssetProperty {
                        name: "Height".to_string(),
                        property_type: "Int32".to_string(),
                        value: height.to_string(),
                    });
                }
            }
        }
    }
    
    if is_mesh {
        // Try to get mesh information
        if let Ok(toolkit) = UAssetToolkitSync::new(None) {
            if let Ok(mesh_info) = toolkit.get_mesh_info(&path.to_string_lossy()) {
                if let Some(material_count) = mesh_info.material_count {
                    properties.push(AssetProperty {
                        name: "MaterialCount".to_string(),
                        property_type: "Int32".to_string(),
                        value: material_count.to_string(),
                    });
                }
                if let Some(is_skeletal) = mesh_info.is_skeletal_mesh {
                    properties.push(AssetProperty {
                        name: "IsSkeletalMesh".to_string(),
                        property_type: "Boolean".to_string(),
                        value: is_skeletal.to_string(),
                    });
                }
            }
        }
    }
    
    Ok(AssetInfo {
        path: path.to_path_buf(),
        is_mesh,
        is_texture,
        class_name: if is_mesh {
            Some("SkeletalMesh".to_string())
        } else if is_texture {
            Some("Texture2D".to_string())
        } else {
            None
        },
        properties,
    })
}

/// Information about a UAsset file
#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub path: std::path::PathBuf,
    pub is_mesh: bool,
    pub is_texture: bool,
    pub class_name: Option<String>,
    pub properties: Vec<AssetProperty>,
}

/// Property information from a UAsset file
#[derive(Debug, Clone)]
pub struct AssetProperty {
    pub name: String,
    pub property_type: String,
    pub value: String,
}

/// Processes mesh files using UAssetAPI toolkit for material patching
pub fn process_mesh_with_uasset_api(uasset_path: &Path, uexp_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    debug!("Processing mesh with UAssetAPI toolkit: {:?} and {:?}", uasset_path, uexp_path);
    
    // Try UAssetAPI toolkit first for most accurate processing
    match UAssetToolkitSync::new(None) {
        Ok(toolkit) => {
            match toolkit.process_mesh_uasset(&uasset_path.to_string_lossy(), &uexp_path.to_string_lossy()) {
                Ok(was_processed) => {
                    if was_processed {
                        debug!("UAssetAPI toolkit successfully processed mesh: {:?}", uasset_path);
                        return Ok(true);
                    } else {
                        debug!("UAssetAPI toolkit determined file is not a mesh: {:?}", uasset_path);
                    }
                }
                Err(e) => {
                    warn!("UAssetAPI toolkit mesh processing failed: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to initialize UAssetAPI toolkit: {}", e);
        }
    }
    
    // Return false to indicate fallback to existing processing methods
    Ok(false)
}

/// Batch processes multiple assets using UAssetAPI toolkit
pub fn batch_analyze_assets(paths: &[&Path]) -> Vec<Result<AssetInfo, Box<dyn std::error::Error>>> {
    debug!("Batch analyzing {} assets with UAssetAPI toolkit", paths.len());
    
    paths.iter()
        .map(|path| analyze_asset_with_uasset_api(path))
        .collect()
}
