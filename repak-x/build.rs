// extern crate winres; // Disabled for Tauri - Tauri handles icons
fn main() {
    // Tauri build - handles icons and resources
    tauri_build::build();

    // Platform-specific build steps
    #[cfg(windows)]
    windows_build();

    #[cfg(target_os = "linux")]
    linux_build();

    #[cfg(target_os = "macos")]
    macos_build();
}

#[cfg(target_os = "linux")]
fn linux_build() {
    use std::{env, fs, path::Path, path::PathBuf};

    // Compute key paths from OUT_DIR
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .expect("Failed to derive target directory from OUT_DIR");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let exe_dir = target_dir.join(&profile);

    // 1) Copy UAssetTool (Linux executable, no .exe)
    let dest_dir = exe_dir.join("uassettool");
    let dest_path = dest_dir.join("UAssetTool");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    // Use workspace_root to derive the CI-published binary path, because target_dir
    // includes the target triple when building with --target (cross-compilation).
    let primary_src = workspace_root.join("target").join("uassettool").join("UAssetTool");
    let tools_dir = workspace_root
        .join("UAssetToolRivals")
        .join("src")
        .join("UAssetTool");
    let fallback_release_publish = tools_dir
        .join("bin")
        .join("Release")
        .join("net8.0")
        .join("linux-x64")
        .join("publish")
        .join("UAssetTool");
    let fallback_release = tools_dir
        .join("bin")
        .join("Release")
        .join("net8.0")
        .join("linux-x64")
        .join("UAssetTool");
    let fallback_debug = tools_dir
        .join("bin")
        .join("Debug")
        .join("net8.0")
        .join("linux-x64")
        .join("UAssetTool");

    let source = if primary_src.exists() {
        Some(primary_src)
    } else if fallback_release_publish.exists() {
        Some(fallback_release_publish)
    } else if fallback_release.exists() {
        Some(fallback_release)
    } else if fallback_debug.exists() {
        Some(fallback_debug)
    } else {
        None
    };

    if let Some(src) = source {
        if let Err(e) = fs::create_dir_all(&dest_dir) {
            println!(
                "cargo:warning=failed to create {}: {}",
                dest_dir.display(),
                e
            );
        } else {
            match fs::copy(&src, &dest_path) {
                Ok(_) => {
                    println!("cargo:warning=UAssetTool copied to {}", dest_path.display());
                    // Make executable on Linux
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(metadata) = fs::metadata(&dest_path) {
                            let mut perms = metadata.permissions();
                            perms.set_mode(0o755);
                            let _ = fs::set_permissions(&dest_path, perms);
                        }
                    }
                }
                Err(e) => {
                    println!(
                        "cargo:warning=failed to copy {} to {}: {}",
                        src.display(),
                        dest_path.display(),
                        e
                    );
                }
            }

            // Copy required dependencies and config files
            let src_dir = src.parent().unwrap();
            let required_files = vec![
                "UAssetTool.dll",
                "UAssetAPI.dll",
                "Newtonsoft.Json.dll",
                "ZstdSharp.dll",
                "UAssetTool.runtimeconfig.json",
                "UAssetTool.deps.json",
            ];

            for file_name in required_files {
                let file_src = src_dir.join(file_name);
                let file_dest = dest_dir.join(file_name);

                if file_src.exists() {
                    match fs::copy(&file_src, &file_dest) {
                        Ok(_) => {
                            println!(
                                "cargo:warning={} copied to {}",
                                file_name,
                                file_dest.display()
                            );
                        }
                        Err(e) => {
                            println!(
                                "cargo:warning=failed to copy {} to {}: {}",
                                file_src.display(),
                                file_dest.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    } else {
        println!("cargo:warning=UAssetTool not found. To enable asset pipeline, build it via: 'dotnet publish UAssetToolRivals/src/UAssetTool -c Release -r linux-x64 --self-contained true'");
    }

    // 2) Copy character_data.json to data folder
    let char_data_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("character_data.json");
    let char_data_dest_dir = exe_dir.join("data");
    let char_data_dest = char_data_dest_dir.join("character_data.json");

    if char_data_src.exists() {
        if let Err(e) = fs::create_dir_all(&char_data_dest_dir) {
            println!(
                "cargo:warning=failed to create data directory {}: {}",
                char_data_dest_dir.display(),
                e
            );
        } else {
            match fs::copy(&char_data_src, &char_data_dest) {
                Ok(_) => {
                    println!(
                        "cargo:warning=character_data.json copied to {}",
                        char_data_dest.display()
                    );
                }
                Err(e) => {
                    println!(
                        "cargo:warning=failed to copy character_data.json to {}: {}",
                        char_data_dest.display(),
                        e
                    );
                }
            }
        }
    } else {
        println!(
            "cargo:warning=character_data.json not found at {}",
            char_data_src.display()
        );
    }
}

#[cfg(target_os = "macos")]
fn macos_build() {
    use std::{env, fs, path::Path, path::PathBuf};

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .expect("Failed to derive target directory from OUT_DIR");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let exe_dir = target_dir.join(&profile);

    // Copy character_data.json
    let char_data_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("character_data.json");
    let char_data_dest_dir = exe_dir.join("data");
    let char_data_dest = char_data_dest_dir.join("character_data.json");

    if char_data_src.exists() {
        if let Err(e) = fs::create_dir_all(&char_data_dest_dir) {
            println!(
                "cargo:warning=failed to create data directory {}: {}",
                char_data_dest_dir.display(),
                e
            );
        } else {
            match fs::copy(&char_data_src, &char_data_dest) {
                Ok(_) => {
                    println!(
                        "cargo:warning=character_data.json copied to {}",
                        char_data_dest.display()
                    );
                }
                Err(e) => {
                    println!(
                        "cargo:warning=failed to copy character_data.json to {}: {}",
                        char_data_dest.display(),
                        e
                    );
                }
            }
        }
    }

    println!("cargo:warning=macOS build: UAssetTool support requires 'dotnet publish -r osx-x64'");
}

#[cfg(windows)]
fn windows_build() {
    use std::{env, fs, path::Path, path::PathBuf};

    // Winres disabled for Tauri to avoid duplicate resources
    // Tauri handles icon embedding via tauri.conf.json

    // Compute key paths from OUT_DIR
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|p| p.to_path_buf())
        .expect("Failed to derive target directory from OUT_DIR");

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let exe_dir = target_dir.join(&profile);
    let dest_dir = exe_dir.clone();
    let dest_path = dest_dir.join("UAssetTool.dll");

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();

    // Use workspace_root to derive the CI-published DLL path, because target_dir
    // includes the target triple when building with --target (cross-compilation),
    // which would give the wrong path (target/<triple>/uassettool instead of target/uassettool).
    let primary_src = workspace_root.join("target").join("uassettool").join("UAssetTool.dll");
    let tools_dir = workspace_root
        .join("UAssetToolRivals")
        .join("src")
        .join("UAssetTool");
    let fallback_release_publish = tools_dir
        .join("bin")
        .join("Release")
        .join("net8.0")
        .join("win-x64")
        .join("publish")
        .join("UAssetTool.dll");
    let fallback_release = tools_dir
        .join("bin")
        .join("Release")
        .join("net8.0")
        .join("win-x64")
        .join("UAssetTool.dll");
    let fallback_debug = tools_dir
        .join("bin")
        .join("Debug")
        .join("net8.0")
        .join("win-x64")
        .join("UAssetTool.dll");

    let source = if primary_src.exists() {
        Some(primary_src)
    } else if fallback_release_publish.exists() {
        Some(fallback_release_publish)
    } else if fallback_release.exists() {
        Some(fallback_release)
    } else if fallback_debug.exists() {
        Some(fallback_debug)
    } else {
        None
    };

    if let Some(src) = source {
        if let Err(e) = fs::create_dir_all(&dest_dir) {
            println!(
                "cargo:warning=failed to create {}: {}",
                dest_dir.display(),
                e
            );
        } else {
            match fs::copy(&src, &dest_path) {
                Ok(_) => {
                    println!("cargo:warning=UAssetTool copied to {}", dest_path.display());
                }
                Err(e) => {
                    println!(
                        "cargo:warning=failed to copy {} to {}: {}",
                        src.display(),
                        dest_path.display(),
                        e
                    );
                }
            }
        }
    } else {
        println!("cargo:warning=UAssetTool.dll not found. To enable asset pipeline, build it via: 'dotnet publish UAssetToolRivals/src/UAssetTool/UAssetToolNative.csproj -c Release -r win-x64'");
    }

    // Oodle DLL is downloaded on-demand by oodle_loader

    // Copy character_data.json to data folder
    let char_data_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("data")
        .join("character_data.json");
    let char_data_dest_dir = exe_dir.join("data");
    let char_data_dest = char_data_dest_dir.join("character_data.json");

    if char_data_src.exists() {
        if let Err(e) = fs::create_dir_all(&char_data_dest_dir) {
            println!(
                "cargo:warning=failed to create data directory {}: {}",
                char_data_dest_dir.display(),
                e
            );
        } else {
            match fs::copy(&char_data_src, &char_data_dest) {
                Ok(_) => {
                    println!(
                        "cargo:warning=character_data.json copied to {}",
                        char_data_dest.display()
                    );
                }
                Err(e) => {
                    println!(
                        "cargo:warning=failed to copy character_data.json to {}: {}",
                        char_data_dest.display(),
                        e
                    );
                }
            }
        }
    } else {
        println!(
            "cargo:warning=character_data.json not found at {}",
            char_data_src.display()
        );
    }
}
