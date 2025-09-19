use crate::install_mod::install_mod_logic::pak_files::repak_dir;
use crate::install_mod::install_mod_logic::patch_meshes;
use crate::install_mod::{InstallableMod, AES_KEY};
use crate::uasset_detection::{modify_texture_mipmaps, patch_mesh_files};
use crate::uasset_api_integration::process_texture_with_uasset_api;
use crate::utils::collect_files;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use repak::Version;
use std::io::BufWriter;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicI32;
use retoc::*;
use std::sync::Arc;
use log::{debug, error, warn};
use std::fs::File;
use path_slash::PathExt;


pub fn convert_to_iostore_directory(
    pak: &InstallableMod,
    mod_dir: PathBuf,
    to_pak_dir: PathBuf,
    packed_files_count: &AtomicI32,
) -> Result<(), repak::Error> {
    let mod_type = pak.mod_type.clone();
    if mod_type == "Audio" || mod_type == "Movies" {
        debug!("{} mod detected. Not creating iostore packages",mod_type);
        repak_dir(pak, to_pak_dir, mod_dir, packed_files_count)?;
        return Ok(());
    }


    let mut pak_name = pak.mod_name.clone();
    pak_name.push_str(".pak");

    let mut utoc_name = pak.mod_name.clone();
    utoc_name.push_str(".utoc");

    let mut paths = vec![];
    collect_files(&mut paths, &to_pak_dir)?;

    if pak.fix_mesh {
        patch_meshes::mesh_patch(&mut paths, &to_pak_dir.to_path_buf())?;
    }

    if pak.fix_textures {
        if let Err(e) = process_texture_files(&paths) {
            error!("Failed to process texture files: {}", e);
        }
    }

    let action = ActionToZen::new(
        to_pak_dir.clone(),
        mod_dir.join(utoc_name),
        EngineVersion::UE5_3,
    );
    let mut config = Config {
        container_header_version_override: None,
        ..Default::default()
    };

    let aes_toc =
        retoc::AesKey::from_str("0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74")
            .unwrap();

    config.aes_keys.insert(FGuid::default(), aes_toc.clone());
    let config = Arc::new(config);

    action_to_zen(action, config).expect("Failed to convert to zen");

    // NOW WE CREATE THE FAKE PAK FILE WITH THE CONTENTS BEING A TEXT FILE LISTING ALL CHUNKNAMES

    let output_file = File::create(mod_dir.join(pak_name))?;

    let rel_paths = paths
        .par_iter()
        .map(|p| {
            let rel = &p
                .strip_prefix(to_pak_dir.clone())
                .expect("file not in input directory")
                .to_slash()
                .expect("failed to convert to slash path");
            rel.to_string()
        })
        .collect::<Vec<_>>();

    // Build the tiny companion PAK uncompressed on purpose.
    // Rationale: Only UCAS should be compressed; the small PAK is only a mount aid (chunknames)
    // and keeping it uncompressed improves compatibility.
    // To revert: add `.compression(vec![pak.compression])` back below and set build_entry to true.
    let builder = repak::PakBuilder::new()
        .key(AES_KEY.clone().0);

    let mut pak_writer = builder.writer(
        BufWriter::new(output_file),
        Version::V11,
        pak.mount_point.clone(),
        Some(pak.path_hash_seed.parse().unwrap()),
    );
    let entry_builder = pak_writer.entry_builder();

    let rel_paths_bytes: Vec<u8> = rel_paths.join("\n").into_bytes();
    // Write the chunknames entry without compression
    let entry = entry_builder
        .build_entry(false, rel_paths_bytes, "chunknames")
        .expect("Failed to build entry");

    pak_writer.write_entry("chunknames".to_string(), entry)?;
    pak_writer.write_index()?;

    log::info!("Wrote pak file successfully");
    packed_files_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Ok(())

    // now generate the fake pak file
}

/// Process texture files to set MipGenSettings to NoMipmaps
pub fn process_texture_files(paths: &Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let texture_files: Vec<_> = paths
        .iter()
        .filter(|p| {
            p.extension().and_then(|ext| ext.to_str()) == Some("uasset") &&
            crate::uasset_detection::is_texture_uasset_heuristic(p)
        })
        .collect();

    debug!("Found {} texture files to process", texture_files.len());

    for uasset_file in &texture_files {
        let uexp_file = uasset_file.with_extension("uexp");
        
        // Create backups
        if let Err(e) = std::fs::copy(uasset_file, format!("{}.bak", uasset_file.display())) {
            warn!("Failed to create backup for {}: {}", uasset_file.display(), e);
        }
        
        // Try UAssetAPI processing first
        match process_texture_with_uasset_api(uasset_file) {
            Ok(true) => {
                debug!("Successfully processed texture with UAssetAPI: {:?}", uasset_file);
                continue;
            }
            Ok(false) => {
                debug!("UAssetAPI processing not available, falling back to toolkit");
            }
            Err(e) => {
                warn!("UAssetAPI processing failed for {:?}: {}", uasset_file, e);
            }
        }
        
        // Fallback to existing toolkit method
        match modify_texture_mipmaps(uasset_file, &uexp_file) {
            Ok(modified) => {
                if modified {
                    debug!("Successfully modified texture mipmaps: {:?}", uasset_file);
                } else {
                    debug!("No mipmap modification needed for: {:?}", uasset_file);
                }
            }
            Err(e) => {
                error!("Failed to modify texture mipmaps for {:?}: {}", uasset_file, e);
            }
        }
    }
    
    Ok(())
}

