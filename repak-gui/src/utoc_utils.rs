use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use repak::PakReader;
use retoc::{action_manifest, ActionManifest, Config, FGuid};

pub fn read_utoc(utoc_path: &Path, pak_reader: &PakReader, pak_path: &Path) -> Vec<crate::file_table::FileEntry> {
    let action_mn = ActionManifest::new(PathBuf::from(utoc_path));
    let mut config = Config {
        container_header_version_override: None,
        ..Default::default()
    };

    let aes_toc =
        retoc::AesKey::from_str("0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74")
            .unwrap();

    config.aes_keys.insert(FGuid::default(), aes_toc.clone());
    let config = Arc::new(config);

    let ops = action_manifest(action_mn,config).expect("Failed to read utoc");
    let ret = ops.oplog.entries.iter().map(|entry| {
        let name = entry.packagestoreentry.packagename.clone();
        crate::file_table::FileEntry {
            file_path: name,
            pak_path: PathBuf::from(pak_path),
            pak_reader: pak_reader.clone(),
            // entry: pak_reader.get_file_entry(entry).unwrap(),
            compressed: "Unavailable".to_string(),
            uncompressed: "Unavailable".to_string(),
            offset: "Unavailable".to_string(),
            bulkdata: Some(entry.bulkdata.len()),
            package_data: Some(entry.packagedata.len()),
        }
    }).collect::<Vec<_>>();

    ret
}