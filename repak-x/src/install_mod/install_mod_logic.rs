pub mod archives;
pub mod iotoc;
pub mod pak_files;

use crate::install_mod::InstallableMod;
use dirs;
use iotoc::convert_to_iostore_directory;
use log::{error, info, warn};
use pak_files::create_repak_from_pak;
use regex_lite::Regex;
use serde_json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// Number of trailing `9`s an installed mod gets by default.
///
/// The game loads paks in filename order, so Repak-X encodes load priority as the
/// length of the `_99…_P` suffix — 7 nines is priority 1, the baseline (see
/// `set_mod_priority`). Installing must never invent a priority of its own; a
/// longer suffix is only ever carried over from one the user already chose.
pub const BASE_MOD_NINES: usize = 7;

/// File extensions an installed mod can occupy, longest-first so that stacked
/// suffixes like `Mod.utoc.bak_repak` peel off correctly.
const INSTALLED_EXTENSIONS: [&str; 5] = [".bak_repak", ".pak_disabled", ".pak", ".utoc", ".ucas"];

/// Split an installed mod name into its `!` marker, clean base, and 9-suffix length.
///
/// `!Foo_99999999_P` -> `(true, "Foo", Some(8))`, `Foo` -> `(false, "Foo", None)`.
/// The clean base is the mod's identity: everything else is priority decoration.
fn split_mod_name(name: &str) -> (bool, String, Option<usize>) {
    static NINES_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = NINES_RE.get_or_init(|| Regex::new(r"^(.*)_(9+)$").unwrap());

    let has_bang = name.starts_with('!');
    let stem = name.strip_prefix('!').unwrap_or(name);
    let stem = stem.strip_suffix("_P").unwrap_or(stem);

    match re.captures(stem) {
        Some(caps) => (has_bang, caps[1].to_string(), Some(caps[2].len())),
        None => (has_bang, stem.to_string(), None),
    }
}

/// Strip the installed-file extensions from a file name to recover the mod stem.
/// Returns `None` for files that are not installed mod artifacts.
fn installed_stem(file_name: &str) -> Option<String> {
    let mut stem = file_name;
    let mut stripped = false;
    loop {
        match INSTALLED_EXTENSIONS
            .iter()
            .find(|ext| stem.to_lowercase().ends_with(&ext.to_lowercase()))
        {
            Some(ext) => {
                stem = &stem[..stem.len() - ext.len()];
                stripped = true;
            }
            None => break,
        }
    }
    if stripped && !stem.is_empty() {
        Some(stem.to_string())
    } else {
        None
    }
}

/// Highest 9-suffix an already-installed copy of this mod uses in `dir`.
///
/// A reinstall reuses it so a priority the user set through `set_mod_priority`
/// survives instead of quietly dropping back to the baseline — which would also
/// strand the old file as a second entry in the mods list.
fn existing_install_nines(dir: &Path, clean_base: &str) -> Option<usize> {
    if clean_base.is_empty() {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<usize> = None;

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let file_name = entry.file_name();
        let Some(stem) = file_name.to_str().and_then(installed_stem) else {
            continue;
        };
        let (_, base, nines) = split_mod_name(&stem);
        if base.eq_ignore_ascii_case(clean_base) {
            if let Some(n) = nines {
                best = Some(best.map_or(n, |b: usize| b.max(n)));
            }
        }
    }

    best
}

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
                warn!(
                    "Failed to remove existing variant {}: {}",
                    path.display(),
                    e
                );
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
                    warn!(
                        "Failed to remove existing variant {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }

    // Finally, sweep copies of the same mod installed under a different
    // priority suffix (a leftover `Mod_99999999_P.pak` next to the incoming
    // `Mod_9999999_P.pak`). The app keys a mod on its clean base name, so those
    // would show up as a duplicate entry rather than as a second mod.
    let (_, clean_base, _) = split_mod_name(base_name);
    if clean_base.is_empty() {
        return;
    }

    let entries = match fs::read_dir(output_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let file_name = entry.file_name();
        let Some(stem) = file_name.to_str().and_then(installed_stem) else {
            continue;
        };
        if stem.eq_ignore_ascii_case(base_name) {
            continue; // handled by the exact-name passes above
        }
        let (_, base, nines) = split_mod_name(&stem);
        // Require a 9-suffix so unrelated files that merely share the base name
        // are never touched.
        if nines.is_none() || !base.eq_ignore_ascii_case(&clean_base) {
            continue;
        }

        let path = entry.path();
        info!(
            "Cleaning up same-mod variant with a different priority suffix: {}",
            path.display()
        );
        if let Err(e) = fs::remove_file(&path) {
            warn!(
                "Failed to remove existing variant {}: {}",
                path.display(),
                e
            );
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

/// Bring a mod name to the canonical `Name_99…9_P` form.
///
/// An existing 9-suffix is kept when it is already at least `min_nines` long —
/// that suffix is a load priority the user chose, so normalizing must never
/// shorten it. Anything else gets exactly `min_nines`.
pub fn normalize_mod_base_name(name: &str, min_nines: usize) -> String {
    let (has_bang, clean_base, nines) = split_mod_name(name);
    let nines = nines.unwrap_or(0).max(min_nines);

    format!(
        "{}{}_{}_P",
        if has_bang { "!" } else { "" },
        clean_base,
        "9".repeat(nines)
    )
}

pub fn record_installed_tags(base_name: &str, tags: &Vec<String>) {
    if tags.is_empty() {
        return;
    }
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
    } else {
        BTreeMap::new()
    };

    let entry = map.entry(base_name.to_string()).or_default();
    for t in tags {
        if !entry.contains(t) {
            entry.push(t.clone());
        }
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
    let mut results: Vec<ModInstallResult> = Vec::with_capacity(mods.len());

    for installable_mod in mods.iter_mut() {
        if !installable_mod.enabled {
            // Nothing is written, but keep the name canonical for the UI.
            installable_mod.mod_name =
                normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);
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
                    error!(
                        "Failed to create subfolder '{}': {}",
                        installable_mod.install_subfolder, e
                    );
                    continue;
                }
                info!("Created install subfolder: {}", subfolder_path.display());
            }
            subfolder_path
        };

        // Ensure naming suffix consistency up-front for all flows.
        //
        // Every mod installs at the baseline priority. A longer 9-suffix is only
        // kept when it is a priority the user actually chose — either carried in
        // the incoming name, or already on disk from a previous `set_mod_priority`
        // (reusing it also means the reinstall overwrites that file instead of
        // landing beside it as a second entry).
        let min_nines = {
            let (_, clean_base, _) = split_mod_name(&installable_mod.mod_name);
            existing_install_nines(&output_directory, &clean_base)
                .unwrap_or(BASE_MOD_NINES)
                .max(BASE_MOD_NINES)
        };
        installable_mod.mod_name = normalize_mod_base_name(&installable_mod.mod_name, min_nines);

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
            let base = normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);

            // Clean up any existing variants before installing
            cleanup_existing_mod_variants(&output_directory, &base);
            let dests = vec![
                (pak_path, format!("{}.pak", base)),
                (utoc_path, format!("{}.utoc", base)),
                (ucas_path, format!("{}.ucas", base)),
            ];

            let mut copy_errors: Vec<String> = Vec::new();
            for (src, dest_name) in dests {
                crate::install_mod::write_install_debug(&format!(
                    "  Copying {} -> {}",
                    src.display(),
                    dest_name
                ));
                if let Err(e) = std::fs::copy(&src, output_directory.join(&dest_name)) {
                    error!("Unable to copy file {:?}: {:?}", src, e);
                    crate::install_mod::write_install_debug(&format!("  ERROR copying: {}", e));
                    copy_errors.push(format!("copy {}: {}", src.display(), e));
                }
            }
            // Record tags for pickup by main app
            record_installed_tags(&base, &installable_mod.custom_tags);
            if copy_errors.is_empty() {
                results.push(ModInstallResult {
                    mod_name: installable_mod.mod_name.clone(),
                    success: true,
                    error: None,
                });
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
            crate::install_mod::write_install_debug(
                "  -> Taking REPAK path (direct PAK -> IoStore)",
            );
            // Clean up any existing variants before installing
            let base = normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);
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
                    let base = normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);
                    record_installed_tags(&base, &installable_mod.custom_tags);
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: true,
                        error: None,
                    });
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
            let base = normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);

            // Clean up any existing variants before installing
            cleanup_existing_mod_variants(&output_directory, &base);

            let dest = output_directory.join(format!("{}.pak", &base));
            match std::fs::copy(&installable_mod.mod_path, &dest) {
                Ok(_) => {
                    record_installed_tags(&base, &installable_mod.custom_tags);
                    installed_mods_ptr.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: true,
                        error: None,
                    });
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
            let base = normalize_mod_base_name(&installable_mod.mod_name, BASE_MOD_NINES);
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
                    results.push(ModInstallResult {
                        mod_name: installable_mod.mod_name.clone(),
                        success: true,
                        error: None,
                    });
                }
            }
        }
    }
    // set i32 to -255 magic value to indicate mod installation is done
    AtomicI32::store(installed_mods_ptr, -255, Ordering::SeqCst);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_appends_baseline_suffix() {
        assert_eq!(
            normalize_mod_base_name("CoolMod", BASE_MOD_NINES),
            "CoolMod_9999999_P"
        );
        assert_eq!(
            normalize_mod_base_name("CoolMod_P", BASE_MOD_NINES),
            "CoolMod_9999999_P"
        );
    }

    #[test]
    fn normalize_is_idempotent_and_keeps_user_priority() {
        // Re-normalizing must not grow the suffix — that was the batch-install bug.
        let once = normalize_mod_base_name("CoolMod", BASE_MOD_NINES);
        let twice = normalize_mod_base_name(&once, BASE_MOD_NINES);
        assert_eq!(once, twice);

        // A longer suffix is a priority the user set; never shorten it.
        assert_eq!(
            normalize_mod_base_name("CoolMod_999999999_P", BASE_MOD_NINES),
            "CoolMod_999999999_P"
        );
        // A shorter one is not a valid priority, so it is raised to the baseline.
        assert_eq!(
            normalize_mod_base_name("CoolMod_99_P", BASE_MOD_NINES),
            "CoolMod_9999999_P"
        );
    }

    #[test]
    fn normalize_preserves_priority_zero_marker() {
        assert_eq!(
            normalize_mod_base_name("!CoolMod_9999999_P", BASE_MOD_NINES),
            "!CoolMod_9999999_P"
        );
    }

    #[test]
    fn normalize_ignores_non_nine_digits() {
        assert_eq!(
            normalize_mod_base_name("Mod_v2", BASE_MOD_NINES),
            "Mod_v2_9999999_P"
        );
        assert_eq!(
            normalize_mod_base_name("Mod_129", BASE_MOD_NINES),
            "Mod_129_9999999_P"
        );
    }

    #[test]
    fn split_reads_the_priority_suffix() {
        assert_eq!(
            split_mod_name("!Foo_99999999_P"),
            (true, "Foo".to_string(), Some(8))
        );
        assert_eq!(split_mod_name("Foo"), (false, "Foo".to_string(), None));
    }

    #[test]
    fn installed_stem_peels_stacked_extensions() {
        assert_eq!(
            installed_stem("Foo_9999999_P.utoc.bak_repak").as_deref(),
            Some("Foo_9999999_P")
        );
        assert_eq!(
            installed_stem("Foo_9999999_P.pak_disabled").as_deref(),
            Some("Foo_9999999_P")
        );
        // Not an installed mod artifact.
        assert_eq!(installed_stem("notes.txt"), None);
    }

    #[test]
    fn existing_nines_finds_highest_installed_priority() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Foo_99999999_P.pak"), b"x").unwrap();
        fs::write(dir.path().join("Foo_99999999_P.utoc"), b"x").unwrap();
        fs::write(dir.path().join("Bar_9999999_P.pak"), b"x").unwrap();

        assert_eq!(existing_install_nines(dir.path(), "Foo"), Some(8));
        assert_eq!(existing_install_nines(dir.path(), "Bar"), Some(7));
        assert_eq!(existing_install_nines(dir.path(), "Baz"), None);
    }

    #[test]
    fn cleanup_removes_other_priority_suffixes_of_same_mod() {
        let dir = tempfile::tempdir().unwrap();
        let stale = dir.path().join("Foo_99999999_P.pak");
        let stale_utoc = dir.path().join("Foo_99999999_P.utoc");
        let other = dir.path().join("FooBar_9999999_P.pak");
        fs::write(&stale, b"x").unwrap();
        fs::write(&stale_utoc, b"x").unwrap();
        fs::write(&other, b"x").unwrap();

        cleanup_existing_mod_variants(dir.path(), "Foo_9999999_P");

        assert!(!stale.exists(), "stale duplicate should be removed");
        assert!(!stale_utoc.exists(), "stale companion should be removed");
        assert!(other.exists(), "a different mod must be left alone");
    }
}
