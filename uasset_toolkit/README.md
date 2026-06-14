# UAsset Toolkit (`uasset_app` / `uasset_toolkit`)

A Rust wrapper around the **UAssetTool** C# CLI (from the [`UAssetToolRivals`](https://github.com/XzantGaming/UAssetToolRivals) submodule, built on a modified fork of [UAssetAPI](https://github.com/atenfyr/UAssetAPI), .NET 8). It drives UAssetTool as a long-lived child process and talks to it over a JSON stdin/stdout protocol, exposing Unreal Engine asset operations (texture/mesh/blueprint detection, mipmap stripping, PAK and IoStore read/write, mod creation) to the rest of Repak X.

> This crate replaced the old "UAssetBridge" design. There is no longer a separate bridge executable or a vendored `external/UAssetAPI/` — the C# tool now lives in the `UAssetToolRivals` git submodule and is the single asset backend.

## Crate layout

This is one Cargo workspace member, `uasset_app`:

```
uasset_toolkit/
└── uasset_app/
    ├── Cargo.toml      # lib name = "uasset_toolkit", bin name = "uasset_app"
    ├── build.rs        # publishes/copies UAssetTool into <target>/uassettool/
    └── src/
        ├── lib.rs      # SyncToolkit + JSON protocol + module-level helper fns
        └── main.rs     # small interactive/CLI binary for manual testing
```

- **Library** (`uasset_toolkit`): the API consumed by `repak-x`.
- **Binary** (`uasset_app`): a thin CLI/interactive harness for poking the tool by hand.

The actual asset logic lives in the C# tool at `UAssetToolRivals/src/UAssetTool` (see that submodule's README for the full command/JSON-action surface).

## How it works

`UAssetTool` is launched once and kept alive. Each request is a single JSON line written to its stdin; the tool replies with a single JSON line on stdout (`UAssetResponse { success, message, data }`). Communication is synchronous and uses only `std` primitives (`std::sync::Mutex`, channels, threads) — deliberately **no tokio** — to avoid cross-runtime deadlocks when called from Tauri's async context.

- `SyncToolkit` — owns the child process and exposes typed methods.
- A process-wide singleton is initialized once at app startup via `init_global_toolkit()` and retrieved with `get_global_toolkit()`. The module-level helper functions use this singleton.
- The tool executable is located next to the host app at `uassettool/UAssetTool(.exe)`, or relative to the workspace `target/uassettool/` during development.

## Build

`uasset_app/build.rs` provisions the C# tool automatically on `cargo build`:

1. If `dotnet` is available, it runs
   `dotnet publish UAssetToolRivals/src/UAssetTool -c Release -r <rid> --self-contained true -o <target>/uassettool`
   (`<rid>` = `win-x64` / `linux-x64` / `osx-x64`, chosen from the build target).
2. Otherwise it falls back to copying a precompiled `UAssetTool(.exe)` from the submodule's
   `bin/Release/net8.0/<rid>/publish/` (or `bin/Release|Debug/net8.0/<rid>/`).
3. If neither succeeds it **panics** — UAssetTool is required.

Set `SKIP_UASSET_TOOL_BUILD=1` to skip provisioning when the executable already exists (e.g. a CI step built it).

**Prerequisites:** .NET 8 SDK (to publish from source) and the initialized `UAssetToolRivals` submodule. Initialize it with `scripts/Init-Submodule.ps1` from the repo root.

> Note: `repak-x/build.rs` separately copies the published tool (plus `character_data.json`) next to the final app binary for packaging. This crate's `build.rs` only produces `<target>/uassettool/`.

## Library usage

```rust
use uasset_toolkit::{init_global_toolkit, get_global_toolkit, SyncToolkit};

// Option A: initialize the global singleton once at startup, then use helpers anywhere.
init_global_toolkit()?;                       // auto-detects the tool path
let toolkit = get_global_toolkit()?;
let is_tex = toolkit.is_texture_uasset("T_Skin_D.uasset")?;

// Option B: own an instance directly.
let toolkit = SyncToolkit::new(None)?;        // None = auto-detect; Some(path) to override
toolkit.strip_mipmaps_native("T_Skin_D.uasset", Some("game.usmap"))?;
```

### `SyncToolkit` methods

- Detection: `is_texture_uasset`, `batch_detect_texture`, `batch_detect_skeletal_mesh`, `batch_detect_static_mesh`, `batch_detect_blueprint`
- Textures: `strip_mipmaps_native`, `batch_strip_mipmaps_native`, `batch_has_inline_texture_data`, `convert_texture`, `set_no_mipmaps`
- IoStore: `list_iostore_files`, `create_mod_iostore`, `create_mod_iostore_from_pak`

### Module-level helpers (use the global singleton)

`create_pak`, `list_pak`, `list_pak_files`, `extract_pak_all`, `create_mod_iostore`, `create_mod_iostore_from_pak`, `extract_iostore`, `extract_script_objects`, `recompress_iostore`, `is_iostore_compressed`, `is_iostore_encrypted`, `list_iostore_files`, `patch_mesh`, `is_texture_uasset`, `is_skeletal_mesh_uasset`, `is_static_mesh_uasset`.

### Types

```rust
pub struct UAssetResponse { pub success: bool, pub message: String, pub data: Option<serde_json::Value> }
pub struct TextureInfo { pub mip_gen_settings: Option<String>, pub width: Option<i32>, pub height: Option<i32>, pub format: Option<String> }
pub struct MeshInfo { pub material_count: Option<i32>, pub vertex_count: Option<i32>, pub triangle_count: Option<i32>, pub is_skeletal_mesh: Option<bool> }
```

The JSON request protocol is the `UAssetRequest` enum in `lib.rs` (serde-tagged action names such as `detect_texture`, `strip_mipmaps_native`, `list_pak`, `create_pak`, `create_mod_iostore`, `extract_iostore`, …).

## CLI / interactive binary (`uasset_app`)

For manual testing, run the `uasset_app` binary:

```bash
# Process file(s): checks if each is a texture uasset and sets NoMipmaps
uasset_app path/to/texture.uasset [more.uasset ...]

# With an explicit tool path as the first argument (anything not ending in .uasset)
uasset_app /path/to/UAssetTool.exe file.uasset

# Interactive mode (no args)
uasset_app
#   <file_path>             - process texture uasset
#   info <file>             - is-texture check
#   mesh <file>             - is-mesh check
#   mesh-info <file>        - skeletal-mesh check
#   patch-mesh <uasset> <uexp>
#   quit / exit
```

## Error handling

All fallible APIs return `anyhow::Result`, wrapping file-not-found, tool-communication, JSON, and UAssetTool-side errors with context for easy propagation.

## Requirements

- Rust toolchain
- .NET 8 SDK (only to build UAssetTool from source; not needed if a precompiled tool is present)
- `UAssetToolRivals` submodule initialized
- Windows / Linux (macOS publishes but is not a primary target)
