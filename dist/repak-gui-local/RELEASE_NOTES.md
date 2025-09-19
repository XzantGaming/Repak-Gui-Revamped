# Repak GUI Release Notes

## Highlights
- **UCAS compression restored and enforced**
  - Oodle compression enabled for all chunk types except `ContainerHeader`.
  - `ExportBundleData` compression enabled (improves size; still only used if it reduces block size).
  - Compression block size increased to **128 KB (0x20000)** for better ratios.
- **Stability hardening**
  - Legacy asset name/path resolution (`legacy_asset.rs`) now bounds-checks indices to prevent panics.
  - Zen asset conversion (`zen_asset_conversion.rs`) now bounds-checks preload dependency indices and export map lookups.
- **Clear compression summary**
  - After container build, a single-line summary prints:
    `IoStore compression summary: total_blocks_compressed=X bulk=Y shaders=Z export=W`
- **Compatibility**
  - Companion `chunknames` **.pak** remains uncompressed by design.
  - Target engine version is **UE5_3**.

## Requirements
- Windows x64 (MSVC)
- Target game with UE5.3 IOStore
- Game-provided Oodle available (loader expects game's Oodle; none is redistributed)

## Usage
1. Launch `repak-gui.exe`.
2. Drag a PAK (non-Audio/Movies) -> Repack -> Install.
3. The output triplet is created in `~mods/`:
   - `<stem>_9999999_P.ucas` (compressed)
   - `<stem>_9999999_P.utoc` (metadata)
   - `<stem>_9999999_P.pak` (tiny, uncompressed)
4. After repack, check `target/release/latest.log` for the compression summary.

## Notes
- Texture bridge is optional. If `uassetbridge/UAssetBridge.dll` is missing, texture post-processing is skipped with warnings.
- If a specific mod ever fails with compression, report the stem and `latest.log` so a narrow fallback can be added, without affecting others.

## Changelog
- Enforce UCAS Oodle compression (ExportBundleData included).
- Increase TOC compression block size to 0x20000.
- Hardened index handling in `legacy_asset.rs` and `zen_asset_conversion.rs`.
- Add compression summary line in `iostore_writer.rs` finalize.
- Keep companion `.pak` uncompressed; target **UE5_3**.
