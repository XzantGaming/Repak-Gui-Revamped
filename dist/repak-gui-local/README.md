# Repak GUI (Marvel Rivals)

Repak GUI is a Windows application for installing and repacking Marvel Rivals mods. It automates UE IOStore packaging, applies mesh fixes during repack, and intelligently handles compression so your installed mods load and stay small.

This repository still contains CLI and library crates under the hood, but end users should use the GUI app.

## Features
- Install PAK mods with one drag-and-drop.
- Automatic mesh patching during repack (no manual steps).
- IOStore packaging (UE5.3 target): produces `.utoc/.ucas` + a tiny companion `.pak`.
- Oodle compression for UCAS (ExportBundleData included); ContainerHeader is kept uncompressed by spec.
- Compression summary after each build so you can verify size reduction.
- Hardened import/name/preload resolution to avoid crashes on malformed assets.

## Requirements
- Windows x64
- Marvel Rivals installed (UE5.3)
- Game-provided Oodle available at runtime (standard for the game)

## Installation
1) Download a prebuilt ZIP from the Releases page.
2) Extract anywhere (avoid Program Files to simplify permissions).
3) Launch `repak-gui.exe`.

Optional (texture pipeline): place `uassetbridge/UAssetBridge.dll` next to the EXE under `target/release/` in dev builds, or inside the app folder if your release ships it. Without it, texture post-processing is skipped with warnings (non-fatal).

## Usage
1) Drag a `.pak` mod into the GUI and click Repack/Install.
2) The app will unpack, patch meshes, and repack to IOStore.
3) Output in your game `Paks/~mods` folder:
   - `<stem>_9999999_P.utoc`
   - `<stem>_9999999_P.ucas`
   - `<stem>_9999999_P.pak` (small companion, uncompressed)
4) After completion, check `latest.log` for a line like:
   - `IoStore compression summary: total_blocks_compressed=X bulk=Y shaders=Z export=W`

Notes:
- Ensure the `_9999999_P` suffix is used so the game prioritizes your mod.
- Audio/Movie mods are handled by the existing logic; game data mods use the IOStore path above.

## Compression behavior
- UCAS is compressed with Oodle where it reduces size.
- ExportBundleData is allowed to compress; ContainerHeader stays uncompressed.
- The companion `chunknames` `.pak` is always uncompressed (by design, very small).

## Troubleshooting
- Texture warnings about `UAssetBridge.dll` missing: optional; place the DLL under `uassetbridge/` to enable the texture pipeline.
- If a mod fails to load, share `target/release/latest.log` and the mod name so we can tailor fixes without disabling compression globally.

## For modders
- Advanced users can still build and use the underlying crates, but the recommended path is the GUI.
- A packaging script `package_release.ps1` is provided to create a clean ZIP for releases.

*If you like my support on this game consider [buying me a coffee](https://ko-fi.com/natimerry)*


## Acknowledgements
- [unpak](https://github.com/bananaturtlesandwich/unpak): original crate featuring read-only pak operations
- [rust-u4pak](https://github.com/panzi/rust-u4pak)'s README detailing the pak file layout
- [jieyouxu](https://github.com/jieyouxu) for serialization implementation of the significantly more complex V11 index
- [repak](https://github.com/trumank/repak) for the original repak implementation
