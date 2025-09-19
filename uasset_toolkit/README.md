# UAsset Toolkit

A Rust-C# interop solution for processing Unreal Engine uasset files using atenfyr's UAssetAPI. This toolkit provides functionality to detect texture uassets and modify their mip generation settings.

## Architecture

- **uasset_app/** - Rust library and binary for JSON communication with C# bridge
- **tools/UAssetBridge/** - .NET 8 console application that wraps UAssetAPI
- **external/UAssetAPI/** - Vendored UAssetAPI source code

## Features

- Detect if a uasset file is a texture asset
- Modify mip generation settings (specifically set to NoMipmaps)
- Get texture information (dimensions, format, etc.)
- Batch processing capabilities
- Both async and sync APIs

## Building

The build is integrated with Cargo. When you run `cargo build`, it will automatically:

1. Build the Rust components
2. Run `dotnet publish` on the UAssetBridge
3. Output the bridge executable to `target/uassetbridge/`

```bash
cd uasset_toolkit
cargo build --release
```

## Usage

### Command Line

```bash
# Process single file
./target/release/uasset_app.exe path/to/texture.uasset

# Process multiple files
./target/release/uasset_app.exe file1.uasset file2.uasset file3.uasset

# With explicit bridge path
./target/release/uasset_app.exe ./target/uassetbridge/UAssetBridge.exe file.uasset
```

### Interactive Mode

```bash
./target/release/uasset_app.exe
# Then enter file paths or commands:
# path/to/texture.uasset
# info path/to/texture.uasset
# quit
```

### Library Usage

```rust
use uasset_toolkit::{UAssetToolkit, UAssetToolkitSync};

// Async API
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let toolkit = UAssetToolkit::new(None)?; // Auto-detect bridge path
    
    // Check if file is texture and process it
    let was_processed = toolkit.process_texture_uasset("path/to/file.uasset").await?;
    
    // Get detailed texture info
    let info = toolkit.get_texture_info("path/to/texture.uasset").await?;
    println!("Mip Gen: {:?}", info.mip_gen_settings);
    
    Ok(())
}

// Sync API (for non-async contexts)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let toolkit = UAssetToolkitSync::new(None)?;
    
    let was_processed = toolkit.process_texture_uasset("path/to/file.uasset")?;
    
    Ok(())
}
```

## Integration with Marvel Rivals Mod Manager

To integrate this into your mod manager, add the dependency to your `Cargo.toml`:

```toml
[dependencies]
uasset_toolkit = { path = "../uasset_toolkit/uasset_app" }
```

Then use it in your mod processing code:

```rust
use uasset_toolkit::UAssetToolkitSync;

// In your mod installation/processing code
fn process_mod_textures(mod_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let toolkit = UAssetToolkitSync::new(None)?;
    
    // Find all uasset files in the mod
    let uasset_files = find_uasset_files(mod_path)?;
    
    for file_path in uasset_files {
        match toolkit.process_texture_uasset(&file_path) {
            Ok(true) => println!("✓ Processed texture: {}", file_path),
            Ok(false) => {}, // Not a texture, skip
            Err(e) => eprintln!("Error processing {}: {}", file_path, e),
        }
    }
    
    Ok(())
}
```

## API Reference

### UAssetToolkit (Async)

- `new(bridge_path: Option<String>) -> Result<Self>` - Create new toolkit instance
- `is_texture_uasset(&self, file_path: &str) -> Result<bool>` - Check if file is texture
- `set_no_mipmaps(&self, file_path: &str) -> Result<()>` - Set mip gen to NoMipmaps
- `process_texture_uasset(&self, file_path: &str) -> Result<bool>` - Detect and process if texture
- `get_texture_info(&self, file_path: &str) -> Result<TextureInfo>` - Get texture details
- `batch_process_textures(&self, file_paths: &[String]) -> Vec<(String, bool, Option<String>)>` - Batch process

### UAssetToolkitSync (Blocking)

Same methods as async version but blocking (for use in non-async contexts).

### TextureInfo

```rust
pub struct TextureInfo {
    pub mip_gen_settings: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub format: Option<String>,
}
```

## Requirements

- .NET 8 SDK
- Rust toolchain
- Windows (currently configured for win-x64)

## Error Handling

The toolkit provides comprehensive error handling:
- File not found errors
- Invalid uasset format errors
- Bridge communication errors
- UAssetAPI processing errors

All errors are wrapped in `anyhow::Error` for easy propagation and display.
