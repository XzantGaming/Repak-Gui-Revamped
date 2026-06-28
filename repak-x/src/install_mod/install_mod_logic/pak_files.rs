use crate::install_mod::{InstallableMod, AES_KEY_HEX};
use crate::utils::collect_files;
use chrono;
use dirs;
use log::{debug, info};
use path_slash::PathExt;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicI32;
use tempfile::tempdir;

use super::iotoc::convert_to_iostore_directory;

pub fn extract_pak_to_dir(pak: &InstallableMod, install_dir: PathBuf) -> Result<(), String> {
    info!(
        "[ExtractPak] mod_name={}, mod_path={}",
        pak.mod_name,
        pak.mod_path.display()
    );
    info!("[ExtractPak] install_dir={}", install_dir.display());

    // Write debug info
    if let Some(config_dir) = dirs::config_dir() {
        let debug_log = config_dir.join("Repak-X").join("extract_pak_debug.log");
        let _ = std::fs::create_dir_all(debug_log.parent().unwrap());
        let log_content = format!(
            "=== ExtractPak Debug ({}) ===\nmod_name: {}\nmod_path: {}\ninstall_dir: {}\n",
            chrono::Local::now().format("%H:%M:%S"),
            pak.mod_name,
            pak.mod_path.display(),
            install_dir.display()
        );
        let _ = std::fs::write(&debug_log, &log_content);
    }

    fs::create_dir_all(&install_dir).map_err(|e| format!("Failed to create output dir: {}", e))?;

    uasset_toolkit::extract_pak_all(
        pak.mod_path.to_str().unwrap_or_default(),
        install_dir.to_str().unwrap_or_default(),
        Some(AES_KEY_HEX),
    )
    .map_err(|e| format!("Failed to extract PAK: {}", e))?;

    info!(
        "[ExtractPak] Extraction complete to {}",
        install_dir.display()
    );
    Ok(())
}

pub fn create_repak_from_pak(
    pak: &InstallableMod,
    mod_dir: PathBuf,
    packed_files_count: &AtomicI32,
) -> Result<(), String> {
    let temp_dir = tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let temp_path = temp_dir.path().to_path_buf();

    extract_pak_to_dir(pak, temp_path.clone())?;
    convert_to_iostore_directory(pak, mod_dir.clone(), temp_path, packed_files_count)
        .map_err(|e| format!("IoStore conversion failed: {}", e))?;
    Ok(())
}

/// Optimized version: Creates IoStore directly from PAK without Rust-side temp directory.
/// Uses UAssetTool's internal PAK extraction (single JSON call vs extract + convert).
pub fn create_repak_from_pak_fast(
    pak: &InstallableMod,
    mod_dir: PathBuf,
    packed_files_count: &AtomicI32,
) -> Result<(), String> {
    let output_base = mod_dir.join(&pak.mod_name);

    info!(
        "[CreateRepakFast] Creating IoStore directly from PAK: {}",
        pak.mod_path.display()
    );
    info!("[CreateRepakFast] Output base: {}", output_base.display());

    let result = uasset_toolkit::create_mod_iostore_from_pak(
        &output_base.to_string_lossy(),
        &pak.mod_path.to_string_lossy(),
        Some(&pak.mount_point),
        Some(true),              // Enable compression
        Some(AES_KEY_HEX),       // Use Marvel Rivals AES key
        pak.parallel_processing, // Toggle: false=50%, true=75% CPU threads
        pak.obfuscate,           // Encrypt if enabled
        pak.hybrid,              // Hybrid: keep non-Unreal files in the companion PAK
    )
    .map_err(|e| format!("Failed to create IoStore from PAK: {}", e))?;

    info!(
        "[CreateRepakFast] IoStore created: utoc={}, converted={} files",
        result.utoc_path, result.converted_count
    );

    packed_files_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

pub fn repak_dir(
    pak: &InstallableMod,
    to_pak_dir: PathBuf,
    mod_dir: PathBuf,
    installed_mods_ptr: &AtomicI32,
) -> Result<(), String> {
    let pak_name = format!("{}.pak", pak.mod_name);
    let output_path = mod_dir.join(&pak_name);

    let mut paths = vec![];
    collect_files(&mut paths, &to_pak_dir)
        .map_err(|e| format!("Failed to collect files: {}", e))?;

    let original_count = paths.len();
    paths.retain(|p| {
        let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        let should_exclude = ext == "bak" || ext == "temp" || file_name == "patched_files";
        if should_exclude {
            debug!("Excluding from PAK: {}", p.display());
        }
        !should_exclude
    });

    if paths.len() != original_count {
        info!(
            "Filtered {} files from PAK (temp/backup)",
            original_count - paths.len()
        );
    }

    paths.sort();

    let file_entries: Vec<(String, String)> = paths
        .par_iter()
        .filter_map(|p| {
            let rel = p
                .strip_prefix(&to_pak_dir)
                .ok()
                .and_then(|r| r.to_slash())
                .map(|r| r.to_string())?;
            let abs = p.to_str().map(|s| s.to_string())?;
            Some((rel, abs))
        })
        .collect();

    let seed = pak.path_hash_seed.parse::<u64>().ok();
    uasset_toolkit::create_pak(
        output_path.to_str().unwrap_or_default(),
        file_entries,
        Some(&pak.mount_point),
        seed,
        Some(AES_KEY_HEX),
    )
    .map_err(|e| format!("Failed to create PAK: {}", e))?;

    installed_mods_ptr.fetch_add(paths.len() as i32, std::sync::atomic::Ordering::SeqCst);
    info!("Wrote pak file successfully");
    Ok(())
}
