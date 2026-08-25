//! Support types for the Asset Explorer window.
//!
//! The explorer shows one merged tree built from every installed mod, so the
//! two very different listing sources — `list_pak` for legacy PAKs and the
//! IoStore `.utoc` reader — have to agree on what an asset path looks like
//! before their entries can be compared. [`normalize_asset_path`] is that
//! single point of agreement: without it the same logical asset arrives under
//! two different spellings and every cross-mod overlap silently goes unnoticed.
//!
//! [`classify_asset`] applies the same Marvel Rivals naming conventions the
//! mod-level detector in [`crate::utils::get_pak_characteristics_detailed`]
//! uses (`SK_` skeletal meshes, `T_` textures, `MI_` materials, WwiseAudio,
//! …), so an asset's kind in the explorer lines up with the category badge
//! shown on the mod it came from.

use serde::Serialize;

/// Broad asset kind, derived from path and filename conventions alone.
///
/// Deliberately cheap: real UE class detection means parsing each `.uasset`
/// with a mappings file (see `vfx_get_asset_classes`), which is far too slow
/// for a listing that spans every installed mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    SkeletalMesh,
    StaticMesh,
    Texture,
    Material,
    Animation,
    Blueprint,
    Audio,
    Ui,
    Movie,
    Text,
    Data,
    Level,
    Other,
}

impl AssetKind {
    /// Stable identifier shared with the frontend filter chips.
    pub fn id(self) -> &'static str {
        match self {
            AssetKind::SkeletalMesh => "skeletal-mesh",
            AssetKind::StaticMesh => "static-mesh",
            AssetKind::Texture => "texture",
            AssetKind::Material => "material",
            AssetKind::Animation => "animation",
            AssetKind::Blueprint => "blueprint",
            AssetKind::Audio => "audio",
            AssetKind::Ui => "ui",
            AssetKind::Movie => "movie",
            AssetKind::Text => "text",
            AssetKind::Data => "data",
            AssetKind::Level => "level",
            AssetKind::Other => "other",
        }
    }
}

/// Bring a listing path from either source into one canonical spelling.
///
/// PAK entries arrive mount-point relative (`Marvel/Content/Marvel/...`) while
/// IoStore entries arrive as UE virtual paths, sometimes still carrying the
/// `../../../` prefix the container stores them with. Both collapse to
/// `Game/<rest>`, which is also what the per-mod file tree already displays.
/// Paths outside a `Content/` directory (loose `.wem` banks, config files) keep
/// their relative form.
pub fn normalize_asset_path(raw: &str) -> String {
    let mut path = raw.replace('\\', "/");

    // Resolve `..` segments so `../../../Marvel/Content/X` becomes `Marvel/Content/X`.
    if path.contains("..") {
        let mut resolved: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                ".." => {
                    resolved.pop();
                }
                "" | "." => {}
                other => resolved.push(other),
            }
        }
        path = resolved.join("/");
    }

    // Collapse duplicate separators and strip any leading slash.
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    let path = path.trim_start_matches('/');

    // Everything under a `Content/` directory is a UE virtual path rooted at
    // `/Game`. Match case-insensitively: mods are authored on case-insensitive
    // filesystems and the casing is not consistent between the two readers.
    let lower = path.to_lowercase();
    if let Some(idx) = lower.find("/content/") {
        return format!("Game/{}", &path[idx + "/content/".len()..]);
    }
    if lower.starts_with("content/") {
        return format!("Game/{}", &path["content/".len()..]);
    }
    if lower.starts_with("game/") {
        return format!("Game/{}", &path["game/".len()..]);
    }

    path.to_string()
}

/// Metadata files repak/IoStore write into a bundle that are not mod content.
///
/// The per-mod tree hides these too; hiding them here as well keeps the file
/// counts and the conflict badges consistent with what is actually drawn.
pub fn is_metadata_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("chunknames") || lower.contains("patched_files")
}

/// Classify one asset from its normalized path.
///
/// Order matters: the checks run most-specific first, because a texture living
/// under `.../UI/...` is more usefully filed as UI than as a texture, and an
/// `SK_` mesh under `.../VFX/...` is still a mesh.
pub fn classify_asset(path: &str) -> AssetKind {
    let lower = path.to_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(&lower);

    // Container-level extensions win outright.
    if filename.ends_with(".wem") || filename.ends_with(".bnk") {
        return AssetKind::Audio;
    }
    if filename.ends_with(".bik") || filename.ends_with(".mp4") {
        return AssetKind::Movie;
    }
    if filename.ends_with(".umap") {
        return AssetKind::Level;
    }

    // Path-scoped categories.
    if lower.contains("wwiseaudio") || lower.contains("/audio/") {
        return AssetKind::Audio;
    }
    if lower.contains("/movies/") {
        return AssetKind::Movie;
    }
    if lower.contains("/stringtable/") {
        return AssetKind::Text;
    }
    if lower.contains("/ui/") || lower.contains("/uiresources/") {
        return AssetKind::Ui;
    }
    if lower.contains("/blueprints/") {
        return AssetKind::Blueprint;
    }

    // Filename prefixes, the convention Marvel Rivals content follows.
    let stem = filename.split('.').next().unwrap_or(filename);
    if stem.starts_with("sk_") {
        return AssetKind::SkeletalMesh;
    }
    if stem.starts_with("sm_") {
        return AssetKind::StaticMesh;
    }
    if stem.starts_with("t_") || stem.starts_with("tex_") {
        return AssetKind::Texture;
    }
    if stem.starts_with("mi_") || stem.starts_with("m_") || stem.starts_with("mf_") {
        return AssetKind::Material;
    }
    if stem.starts_with("bp_") || stem.ends_with("_c") || stem.ends_with("bp") {
        return AssetKind::Blueprint;
    }
    if stem.starts_with("ab_")
        || stem.starts_with("as_")
        || stem.starts_with("abp_")
        || stem.starts_with("anim_")
        || lower.contains("/animation")
    {
        return AssetKind::Animation;
    }
    if stem.starts_with("ns_") || stem.starts_with("nsc_") || stem.starts_with("ps_") {
        return AssetKind::Material;
    }
    if stem.starts_with("dt_") || stem.starts_with("ct_") || stem.starts_with("da_") {
        return AssetKind::Data;
    }
    if lower.contains("/vfx/") {
        return AssetKind::Material;
    }

    AssetKind::Other
}

/// Strip the priority suffix chain and extension from a mod filename.
///
/// Mirrors the mods list, so the same mod is recognisable in both windows:
/// `!Cool_Mod_9999999_P.pak` reads as `Cool_Mod`.
pub fn clean_mod_display_name(file_name: &str) -> String {
    let stem = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name);

    // Drop the extension (.pak / .pak_disabled / .bak_repak).
    let mut name = match stem.rfind('.') {
        Some(idx) => stem[..idx].to_string(),
        None => stem.to_string(),
    };

    name = name.trim_start_matches('!').to_string();

    // Priority suffixes can stack (`_9999999_P_9999999_P`), so peel them all.
    loop {
        let trimmed = strip_one_priority_suffix(&name);
        if trimmed.len() == name.len() {
            break;
        }
        name = trimmed;
    }

    name
}

fn strip_one_priority_suffix(name: &str) -> String {
    let lower = name.to_lowercase();
    let Some(base) = lower.strip_suffix("_p") else {
        return name.to_string();
    };
    // `_<digits>_P` — the digits are the priority run.
    let digits_end = base.len();
    let digits_start = base
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    if digits_start == digits_end || digits_start == 0 {
        return name.to_string();
    }
    if !base[..digits_start].ends_with('_') {
        return name.to_string();
    }
    name[..digits_start - 1].to_string()
}

/// Load priority for a mod, from its file stem.
///
/// Mirrors the rule `get_pak_files` applies: a `!` prefix is priority 0, and
/// otherwise a `_999…_P` suffix of seven or more nines maps to a 1-based
/// priority. Kept in sync by hand so the number the explorer shows on a
/// conflicting asset is the same one the mods list shows.
pub fn mod_priority(file_stem: &str) -> usize {
    if file_stem.starts_with('!') {
        return 0;
    }
    let Some(base) = file_stem.strip_suffix("_P") else {
        return 0;
    };
    let digits_start = match base.rfind(|c: char| !c.is_ascii_digit()) {
        Some(i) if base[..i].len() == i && base.as_bytes()[i] == b'_' => i + 1,
        _ => return 0,
    };
    let nines = &base[digits_start..];
    if nines.is_empty() || !nines.chars().all(|c| c == '9') {
        return 0;
    }
    if nines.len() >= 7 {
        nines.len() - 6
    } else {
        0
    }
}

/// One mod's contribution to the merged index.
///
/// File paths and their kinds travel as parallel arrays rather than a vector
/// of structs: the payload spans every asset of every installed mod, and
/// repeating JSON keys across a hundred thousand entries costs more than the
/// data itself.
#[derive(Debug, Clone, Serialize)]
pub struct AssetExplorerMod {
    /// Absolute path of the `.pak`/`.pak_disabled`/`.bak_repak`, and the id the
    /// frontend uses to address this mod (including across windows).
    pub path: String,
    pub display_name: String,
    pub enabled: bool,
    /// Load priority as the mods list computes it (0 = `!` prefix, then the
    /// nines run). Shown on conflicting assets so the fix is one number away.
    pub priority: usize,
    pub folder_id: Option<String>,
    pub category: String,
    pub character_name: String,
    pub character_id: String,
    pub is_iostore: bool,
    pub total_size: u64,
    pub files: Vec<String>,
    pub kinds: Vec<&'static str>,
}

/// Everything the explorer window needs for one refresh.
#[derive(Debug, Clone, Serialize)]
pub struct AssetExplorerIndex {
    pub mods: Vec<AssetExplorerMod>,
    /// Mods that could not be read, reported so the UI can say so rather than
    /// silently showing an incomplete tree.
    pub failed: Vec<AssetExplorerFailure>,
    pub total_assets: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetExplorerFailure {
    pub path: String,
    pub display_name: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_listing_sources_agree_on_one_spelling() {
        // IoStore, still carrying the container's relative prefix.
        assert_eq!(
            normalize_asset_path("../../../Marvel/Content/Marvel/Characters/1048/SK_1048.uasset"),
            "Game/Marvel/Characters/1048/SK_1048.uasset"
        );
        // IoStore, already resolved to a UE virtual path.
        assert_eq!(
            normalize_asset_path("/Game/Marvel/Characters/1048/SK_1048.uasset"),
            "Game/Marvel/Characters/1048/SK_1048.uasset"
        );
        // Legacy PAK, mount-point relative.
        assert_eq!(
            normalize_asset_path("Marvel/Content/Marvel/Characters/1048/SK_1048.uasset"),
            "Game/Marvel/Characters/1048/SK_1048.uasset"
        );
    }

    #[test]
    fn non_content_paths_keep_their_shape() {
        assert_eq!(
            normalize_asset_path("Marvel/WwiseAudio/Events/foo.bnk"),
            "Marvel/WwiseAudio/Events/foo.bnk"
        );
    }

    #[test]
    fn kinds_follow_naming_conventions() {
        assert_eq!(
            classify_asset("Game/Marvel/Characters/1048/SK_1048.uasset"),
            AssetKind::SkeletalMesh
        );
        assert_eq!(
            classify_asset("Game/Marvel/Characters/1048/T_1048_D.uasset"),
            AssetKind::Texture
        );
        assert_eq!(
            classify_asset("Marvel/WwiseAudio/Events/foo.bnk"),
            AssetKind::Audio
        );
        assert_eq!(classify_asset("Game/Marvel/UI/T_Icon.uasset"), AssetKind::Ui);
        assert_eq!(
            classify_asset("Game/Marvel/VFX/Materials/MI_Blast.uasset"),
            AssetKind::Material
        );
    }

    #[test]
    fn priority_matches_the_mods_list_rule() {
        assert_eq!(mod_priority("!Cool_Mod"), 0);
        assert_eq!(mod_priority("Cool_Mod_9999999_P"), 1);
        assert_eq!(mod_priority("Cool_Mod_99999999_P"), 2);
        // Fewer than seven nines, or digits that are not all nines, is not a
        // priority suffix.
        assert_eq!(mod_priority("Cool_Mod_999_P"), 0);
        assert_eq!(mod_priority("Cool_Mod_1234567_P"), 0);
        assert_eq!(mod_priority("PlainMod"), 0);
    }

    #[test]
    fn display_name_drops_prefix_suffixes_and_extension() {
        assert_eq!(clean_mod_display_name("Cool_Mod_9999999_P.pak"), "Cool_Mod");
        assert_eq!(
            clean_mod_display_name("!Cool_Mod_9999999_P_9999999_P.pak_disabled"),
            "Cool_Mod"
        );
        assert_eq!(clean_mod_display_name("PlainMod.pak"), "PlainMod");
    }
}
