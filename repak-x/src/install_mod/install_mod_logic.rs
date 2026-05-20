pub mod archives;
pub mod iotoc;
pub mod pak_files;

use crate::install_mod::InstallableMod;
use iotoc::convert_to_iostore_directory;
use log::{error, info, warn};
use pak_files::create_repak_from_pak;
use std::path::{Path, PathBuf};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::collections::BTreeMap;
use std::collections::HashMap;
use dirs;
use serde_json;
use regex_lite::Regex;

/// Clean up any existing variants of a mod file (.bak_repak, .pak_disabled) before installing
/// This prevents duplicate entries when reinstalling a toggled-off mod
fn cleanup_existing_mod_variants(output_dir: &Path, base_name: &str) {
    let variants = [
        format!("{}.pak", base_name),
        format!("{}.bak_repak", base_name),
        format!("{}.pak_disabled", base_name),
    ];
    
    for variant in &variants {
        let path = output_dir.join(variant);
        if path.exists() {
            info!("Cleaning up existing mod variant: {}", path.display());
            if let Err(e) = fs::remove_file(&path) {
                warn!("Failed to remove existing variant {}: {}", path.display(), e);
            }
        }
    }
    
    // Also clean up IoStore variants if they exist
    let iostore_extensions = ["utoc", "ucas"];
    for ext in &iostore_extensions {
        let variants = [
            format!("{}.{}", base_name, ext),
            format!("{}.{}.bak_repak", base_name, ext),
            format!("{}.{}.pak_disabled", base_name, ext),
        ];
        for variant in &variants {
            let path = output_dir.join(variant);
            if path.exists() {
                info!("Cleaning up existing IoStore variant: {}", path.display());
                if let Err(e) = fs::remove_file(&path) {
                    warn!("Failed to remove existing variant {}: {}", path.display(), e);
                }
            }
        }
    }
}

/// Recursively copy a directory and all its contents to a destination
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    
    Ok(())
}

pub fn normalize_mod_base_name(name: &str, min_nines: usize) -> String {
    // Regex to find existing 9s suffix
    // Looking for pattern: any characters, then underscore, then 7 or more 9s, then _P
    
    // First, strip _P if present to work with the base
    let base = if name.ends_with("_P") {
        name.strip_suffix("_P").unwrap()
    } else {
        name
    };

    // Now check if base ends with _999...
    let re = Regex::new(r"^(.*)_(\d+)$").unwrap();
    
    if let Some(caps) = re.captures(base) {
        let prefix = &caps[1];
        let numbers = &caps[2];
        
        // Check if numbers are all 9s
        if numbers.chars().all(|c| c == '9') {
             let num_nines = numbers.len();
             if num_nines >= min_nines {
                 // Already has enough 9s.
                 // Re-append _P
                 return format!("{}_{}_P", prefix, numbers);
             } else {
                 // Has 9s but not enough. Replace with min_nines 9s.
                 let new_nines = "9".repeat(min_nines);
                 return format!("{}_{}_P", prefix, new_nines);
             }
        }
    }
    
    // If we are here, it doesn't have a valid 9s suffix.
    // Append suffix.
    let new_nines = "9".repeat(min_nines);
    format!("{}_{}_P", base, new_nines)
}

pub fn record_installed_tags(base_name: &str, tags: &Vec<String>) {
    if tags.is_empty() { return; }
    let mut cfg_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    cfg_dir.push("Repak-X");
    let _ = fs::create_dir_all(&cfg_dir);
    let mut path = cfg_dir.clone();
    path.push("pending_custom_tags.json");

    let mut map: BTreeMap<String, Vec<String>> = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<BTreeMap<String, Vec<String>>>(&s).ok())
            .unwrap_or_default()
    } else { BTreeMap::new() };

    let entry = map.entry(base_name.to_string()).or_default();
    for t in tags {
        if !entry.contains(t) { entry.push(t.clone()); }
    }
    entry.sort();
    entry.dedup();
    let _ = fs::write(&path, serde_json::to_string_pretty(&map).unwrap());
}

/// Result of installing a single mod, used to report per-mod success/failure to the UI.
#[derive(Debug, Clone)]
pub struct ModInstallResult {
    pub mod_name: String,
    pub success: bool,
    pub error: Option<String>,
}

pub fn install_mods_in_viewport(
    mods: &mut [InstallableMod],
    mod_directory: &Path,
    installed_mods_ptr: &AtomicI32,
    stop_thread: &AtomicBool,
) -> Vec<ModInstallResult> {
    let mut type_tracker: HashMap<String, usize> = HashMap::new();
    let mut results: Vec<ModInstallResult> = Vec::with_capacity(mods.len());

    for installable_mod in mods.iter_mut() {
        let min_nines = if installable_mod.enabled {
             let count = type_tracker.entry(installable_mod.mod_type.clone()).or_insert(0);
             let n = 7 + *count;
             *count += 1;
             n
        } else {
             7
        };

        // Ensure naming suffix consistency up-front for all flows
        installable_mod.mod_name = normalize_mod_base_name(&installable_mod.mod_name, min_nines);
        
        if !installable_mod.enabled {
            continue;
        }
        
        if stop_thread.load(Ordering::SeqCst) {
            warn!("Stopping thread");
            break;
        }

        // Determine the actual output directory (base + subfolder if specified)
        let output_directory = if installable_mod.install_subfolder.is_empty() {
            mod_directory.to_path_buf()
        } else {
            let subfolder_path = mod_directory.join(&installable_mod.install_subfolder);
            // Create the subfolder if it doesn't exist
            if !subfolder_path.exists() {
                if let Err(e) = fs::create_dir_all(&subfolder_path) {
                    error!("Failed to create subfolder '{}': {}", installable_mod.install_subfolder, e);
                    continue;
                }
                info!("Created install subfolder: {}", subfolder_path.display());
            }
            subfolder_path
        };

        // Debug logging for install path tracing
        crate::install_mod::write_install_debug(&format!(
            "=== Installing mod: name={}, iostore={}, repak={}, is_dir={}, mod_path={}, mod_path_exists={}",
            installable_mod.mod_name, installable_mod.iostore, installable_mod.repak, 
            installable_mod.is_dir, installable_mod.mod_path.display(), installable_mod.mod_path.exists()
        ));

        if installable_mod.iostore {
            crate::install_mod::write_install_debug("  -> Taking IOSTORE COPY path");
            // copy the iostore files
            let pak_path = installable_mod.mod_path.with_extension("pak");
            let utoc_path = installable_mod.mod_path.with_extension("utoc");
            let ucas_path = installable_mod.mod_path.with_extension("ucas");

            // Ensure output names follow suffix rule
            let base = normalize_mod_base_name(&installable_mod.mod_name, 7);
            
            // Clean up any existing variants before installing
            cleanup_existing_mod_variants(&output_directory, &base);
            let dests = vec![
                (pak_path, format!("{}.pak", base)),
                (utoc_path, format!("{}.utoc", base)),
                (ucas_path, format!("{}.ucas", base)),
            ];

            let mut copy_errors: Vec<String> = Vec::new();
            for (src, dest_name) in dests {
                crate::install_mod::write_install_debug(&format!("  Copying {} -> {}", src.display(), dest_name));
                if let Err(e) = std::fs::copy(&src, output_directory.join(&dest_name)) {
                    error!("Unable to copy file {:?}: {:?}", src, e);
                    crate::install_mod::write_install_debug(&format!("  ERROR copying: {}", e));
                    copy_errors.push(format!("copy {}: {}", src.display(), e));
                }
            }
            // Record tags for pickup by main app
            record_installed_tags(&base, &installable_mod.custom_tags);
            if copy_errors.is_empty() {
                results.push(ModInstallResult { mod_name: installable_mod.mod_name.clone(), success: true, error: None });
            } else {
                results.push(ModInstallResult {
                    mod_name: installable_mod.mod_name.clone(),
                    success: false,
                    error: Some(copy_errors.join("; ")),
                });
            }
            continue;
        }

        if installable_mod.repak {
            crate::install_mod::write_install_debug("  -> Taking REPAK path (direct PAK -> IoStore)");
            // Clean up any existing variants before installing
            let base = normalize_mod_base_name(&installable_mod.mod_name, 7);
            cleanup_existing_mod_variants(&output_directory, &base);

            // Use optimized path: UAssetTool extracts PAK internally, no Rust-side temp dir
            match pak_files::create_repak_from_pak_fast(
                installable_mod,
                output_directory.clone(),
                installed_mods_ptr,
            ) {
                Err(e) => {
                    error!("Failed to create repak from pak: {}", e);
                    crate::install_mod::write_install_debug(&format!("  ERROR repak: {}", e));
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: false,
                        error: Some(e),
                    });
                }
                Ok(_) => {
                    let base = normalize_mod_base_name(&installable_mod.mod_name, 7);
                    record_installed_tags(&base, &installable_mod.custom_tags);
                    results.push(ModInstallResult { mod_name: installable_mod.mod_name.clone(), success: true, error: None });
                }
            }
            continue;
        }

        // This shit shouldnt even be possible why do I still have this in the codebase???
        if !installable_mod.repak && !installable_mod.is_dir {
            // just move files to the correct location
            info!(
                "Copying mod instead of repacking: {}",
                installable_mod.mod_name
            );
            let base = normalize_mod_base_name(&installable_mod.mod_name, 7);
            
            // Clean up any existing variants before installing
            cleanup_existing_mod_variants(&output_directory, &base);
            
            let dest = output_directory.join(format!("{}.pak", &base));
            match std::fs::copy(&installable_mod.mod_path, &dest) {
                Ok(_) => {
                    record_installed_tags(&base, &installable_mod.custom_tags);
                    installed_mods_ptr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    results.push(ModInstallResult { mod_name: installable_mod.mod_name.clone(), success: true, error: None });
                }
                Err(e) => {
                    let msg = format!("Failed to copy PAK to {}: {}", dest.display(), e);
                    error!("{}", msg);
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: false,
                        error: Some(msg),
                    });
                }
            }
            continue;
        }

        if installable_mod.is_dir {
            // Clean up any existing variants before installing
            let base = normalize_mod_base_name(&installable_mod.mod_name, 7);
            cleanup_existing_mod_variants(&output_directory, &base);
            
            // Copy source directory to temp dir to avoid modifying original files
            let temp_dir = match tempfile::tempdir() {
                Ok(dir) => dir,
                Err(e) => {
                    let msg = format!("Failed to create temp directory: {}", e);
                    error!("{}", msg);
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: false,
                        error: Some(msg),
                    });
                    continue;
                }
            };
            let temp_path = temp_dir.path().to_path_buf();
            
            // Copy all files from source to temp
            let source_path = PathBuf::from(&installable_mod.mod_path);
            if let Err(e) = copy_dir_recursive(&source_path, &temp_path) {
                let msg = format!("Failed to copy mod files to temp directory: {}", e);
                error!("{}", msg);
                results.push(ModInstallResult {
                    mod_name: installable_mod.mod_name.clone(),
                    success: false,
                    error: Some(msg),
                });
                continue;
            }
            info!("Copied mod files to temp directory for processing");
            
            let res = convert_to_iostore_directory(
                installable_mod,
                output_directory.clone(),
                temp_path,
                installed_mods_ptr,
            );
            // temp_dir is automatically cleaned up when it goes out of scope
            match res {
                Err(e) => {
                    error!("Failed to create repak from directory: {}", e);
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: false,
                        error: Some(e),
                    });
                }
                Ok(_) => {
                    info!("Installed mod: {}", installable_mod.mod_name);
                    results.push(ModInstallResult { mod_name: installable_mod.mod_name.clone(), success: true, error: None });
                }
            }
        }
    }
    // set i32 to -255 magic value to indicate mod installation is done
    AtomicI32::store(installed_mods_ptr, -255, Ordering::SeqCst);
    results
}
