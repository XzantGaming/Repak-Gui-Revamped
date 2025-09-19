pub mod archives;
pub mod iotoc;
pub mod pak_files;
pub mod patch_meshes;

use crate::install_mod::install_mod_logic::archives::*;
use crate::install_mod::InstallableMod;
use iotoc::convert_to_iostore_directory;
use log::{error, info, warn};
use pak_files::create_repak_from_pak;
use std::path::{Path, PathBuf};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::collections::BTreeMap;
use dirs;
use serde_json;

pub fn install_mods_in_viewport(
    mods: &mut [InstallableMod],
    mod_directory: &Path,
    installed_mods_ptr: &AtomicI32,
    stop_thread: &AtomicBool,
) {
    for installable_mod in mods.iter_mut() {
        // Ensure naming suffix consistency up-front for all flows
        installable_mod.mod_name = normalize_mod_base_name(&installable_mod.mod_name);
        
        if !installable_mod.enabled{
            continue;
        }

pub fn normalize_mod_base_name(name: &str) -> String {
    if name.ends_with("_9999999_P") {
        name.to_string()
    } else if name.ends_with("_P") {
        // replace trailing _P with _9999999_P
        let trimmed = name.strip_suffix("_P").unwrap_or(name);
        format!("{}_9999999_P", trimmed)
    } else {
        format!("{}_9999999_P", name)
    }
}

pub fn record_installed_tags(base_name: &str, tags: &Vec<String>) {
    if tags.is_empty() { return; }
    let mut cfg_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    cfg_dir.push("repak_manager");
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
        
        
        if stop_thread.load(Ordering::SeqCst) {
            warn!("Stopping thread");
            break;
        }

        if installable_mod.iostore {
            // copy the iostore files
            let pak_path = installable_mod.mod_path.with_extension("pak");
            let utoc_path = installable_mod.mod_path.with_extension("utoc");
            let ucas_path = installable_mod.mod_path.with_extension("ucas");

            // Ensure output names follow suffix rule
            let base = normalize_mod_base_name(&installable_mod.mod_name);
            let dests = vec![
                (pak_path, format!("{}.pak", base)),
                (utoc_path, format!("{}.utoc", base)),
                (ucas_path, format!("{}.ucas", base)),
            ];

            for (src, dest_name) in dests {
                if let Err(e) = std::fs::copy(&src, mod_directory.join(dest_name)) {
                    error!("Unable to copy file {:?}: {:?}", src, e);
                }
            }
            // Record tags for pickup by main app
            record_installed_tags(&base, &installable_mod.custom_tags);
            continue;
        }

        if installable_mod.repak {
            if let Err(e) = create_repak_from_pak(
                installable_mod,
                PathBuf::from(mod_directory),
                installed_mods_ptr,
            ) {
                error!("Failed to create repak from pak: {}", e);
            } else {
                let base = normalize_mod_base_name(&installable_mod.mod_name);
                record_installed_tags(&base, &installable_mod.custom_tags);
            }
        }

        // This shit shouldnt even be possible why do I still have this in the codebase???
        if !installable_mod.repak && !installable_mod.is_dir {
            // just move files to the correct location
            info!(
                "Copying mod instead of repacking: {}",
                installable_mod.mod_name
            );
            let base = normalize_mod_base_name(&installable_mod.mod_name);
            std::fs::copy(&installable_mod.mod_path, mod_directory.join(format!("{}.pak", &base)))
            .unwrap();
            record_installed_tags(&base, &installable_mod.custom_tags);
            installed_mods_ptr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            continue;
        }

        if installable_mod.is_dir {
            let res = convert_to_iostore_directory(
                installable_mod,
                PathBuf::from(&mod_directory),
                PathBuf::from(&installable_mod.mod_path),
                installed_mods_ptr,
            );
            if let Err(e) = res {
                error!("Failed to create repak from pak: {}", e);
            } else {
                info!("Installed mod: {}", installable_mod.mod_name);
            }
        }
    }
    // set i32 to -255 magic value to indicate mod installation is done
    AtomicI32::store(installed_mods_ptr, -255, Ordering::SeqCst);
}
