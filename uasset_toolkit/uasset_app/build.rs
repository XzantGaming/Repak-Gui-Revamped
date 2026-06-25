use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Get the runtime identifier for the current target platform
fn get_runtime_identifier() -> &'static str {
    // Check CARGO_CFG_TARGET_OS which is set during cross-compilation
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "windows".to_string()
        } else if cfg!(target_os = "linux") {
            "linux".to_string()
        } else if cfg!(target_os = "macos") {
            "macos".to_string()
        } else {
            "windows".to_string() // default fallback
        }
    });

    match target_os.as_str() {
        "linux" => "linux-x64",
        "macos" => "osx-x64",
        _ => "win-x64",
    }
}

/// Native library file name produced by the NativeAOT build for the target platform.
fn get_native_lib_name() -> &'static str {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "windows".to_string()
        } else if cfg!(target_os = "macos") {
            "macos".to_string()
        } else {
            "linux".to_string()
        }
    });

    match target_os.as_str() {
        "windows" => "UAssetTool.dll",
        "macos" => "libUAssetTool.dylib",
        _ => "libUAssetTool.so",
    }
}

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    // OUT_DIR = target/<profile>/build/uasset_app-XXingr/out -> target/<profile>
    let target_dir = Path::new(&out_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    // Place the native library directly beside the final executable so the Rust
    // loader (`SyncToolkit::find_dll_path`) finds it as its primary location, and
    // so the OS resolves the library's own native deps from the same directory.
    let tool_output_dir: PathBuf = target_dir.to_path_buf();

    // Workspace root: uasset_app -> uasset_toolkit -> workspace root
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    // The NativeAOT FFI library project inside the UAssetToolRivals submodule.
    let tool_project_dir = workspace_root
        .join("UAssetToolRivals")
        .join("src")
        .join("UAssetTool");

    // Rebuild when the FFI surface or the C# sources change.
    for watched in [
        "NativeExports.cs",
        "Program.cs",
        "UAssetToolNative.csproj",
        "UAssetTool.csproj",
        "Directory.Build.props",
    ] {
        let p = tool_project_dir.join(watched);
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
    let uasset_api_dir = workspace_root
        .join("UAssetToolRivals")
        .join("src")
        .join("UAssetAPI");
    if uasset_api_dir.exists() {
        println!("cargo:rerun-if-changed={}", uasset_api_dir.display());
    }

    if let Err(e) = fs::create_dir_all(&tool_output_dir) {
        println!(
            "cargo:warning=failed to create {}: {}",
            tool_output_dir.display(),
            e
        );
    }

    let runtime_id = get_runtime_identifier();
    let lib_name = get_native_lib_name();
    let dest_lib = tool_output_dir.join(lib_name);

    println!(
        "cargo:warning=Building NativeAOT UAssetTool for runtime: {}, library: {}",
        runtime_id, lib_name
    );

    if !dest_lib.exists() {
        println!("cargo:rerun-if-changed=build.rs");
    }

    // Allow skipping the (slow) AOT build, e.g. for fast Rust-only iteration or when
    // the library was produced out of band. When set, never invoke dotnet.
    if env::var("SKIP_UASSET_TOOL_BUILD").is_ok() {
        if dest_lib.exists() {
            println!("cargo:warning=SKIP_UASSET_TOOL_BUILD set; using existing {}", dest_lib.display());
        } else {
            println!(
                "cargo:warning=SKIP_UASSET_TOOL_BUILD set but {} is missing; \
                 UAssetTool calls will fail at runtime until it is built.",
                dest_lib.display()
            );
        }
        return;
    }

    // NativeAOT publish output layout:
    //   bin_native/Release/net8.0/<rid>/native/<lib>   (the AOT library)
    //   bin_native/Release/net8.0/<rid>/publish/       (library + native deps)
    let native_dir = tool_project_dir
        .join("bin_native")
        .join("Release")
        .join("net8.0")
        .join(runtime_id)
        .join("native");
    let publish_dir = tool_project_dir
        .join("bin_native")
        .join("Release")
        .join("net8.0")
        .join(runtime_id)
        .join("publish");

    // 1) Try to publish the NativeAOT library.
    let mut produced = false;
    let dotnet_available = Command::new("dotnet").arg("--version").output().is_ok();
    if dotnet_available {
        let mut cmd = Command::new("dotnet");
        cmd.current_dir(&tool_project_dir).args([
            "publish",
            "UAssetToolNative.csproj",
            "-c",
            "Release",
            "-r",
            runtime_id,
        ]);
        // NativeAOT's final link step locates the MSVC toolset by invoking `vswhere.exe`
        // as a bare command, so it must be on PATH. It ships in the VS Installer dir,
        // which is usually NOT on PATH. Add it so a plain `cargo build` can link.
        ensure_vswhere_on_path(&mut cmd);
        let status = cmd.status();
        match status {
            Ok(s) if s.success() => {
                let built_lib = native_dir.join(lib_name);
                if built_lib.exists() {
                    match fs::copy(&built_lib, &dest_lib) {
                        Ok(_) => {
                            println!("cargo:warning=UAssetTool AOT library -> {}", dest_lib.display());
                            produced = true;
                            copy_native_deps(&publish_dir, &tool_output_dir);
                        }
                        Err(e) => println!(
                            "cargo:warning=Failed to copy {} to {}: {}",
                            built_lib.display(),
                            dest_lib.display(),
                            e
                        ),
                    }
                } else {
                    println!(
                        "cargo:warning=dotnet publish succeeded but {} not found",
                        built_lib.display()
                    );
                }
            }
            Ok(s) => println!("cargo:warning=dotnet publish failed with status: {}", s),
            Err(e) => println!("cargo:warning=failed to run dotnet publish: {}", e),
        }
    } else {
        println!("cargo:warning=dotnet not found; looking for a precompiled UAssetTool library");
    }

    // 2) Fall back to an already-built library (native or publish dir).
    if !produced {
        for candidate in [native_dir.join(lib_name), publish_dir.join(lib_name)] {
            if candidate.exists() {
                if let Err(e) = fs::copy(&candidate, &dest_lib) {
                    println!(
                        "cargo:warning=Failed to copy precompiled {} to {}: {}",
                        candidate.display(),
                        dest_lib.display(),
                        e
                    );
                    continue;
                }
                println!(
                    "cargo:warning=Using precompiled UAssetTool library: {}",
                    candidate.display()
                );
                copy_native_deps(&publish_dir, &tool_output_dir);
                produced = true;
                break;
            }
        }
    }

    // 3) Nothing produced. The Rust crate still compiles (the library is only needed
    //    at runtime), so don't fail debug builds — but a release build that ships
    //    without the library would be broken, so fail loudly there.
    if !produced {
        let msg = format!(
            "UAssetTool native library was not produced. Install the .NET SDK + NativeAOT \
             prerequisites (C/C++ toolchain) and build with: \
             'dotnet publish UAssetToolRivals/src/UAssetTool/UAssetToolNative.csproj -c Release -r {}'",
            runtime_id
        );
        let profile = env::var("PROFILE").unwrap_or_default();
        if profile == "release" {
            panic!("{}", msg);
        } else {
            println!("cargo:warning={}", msg);
            println!(
                "cargo:warning=Continuing debug build without the native library; \
                 UAssetTool calls will fail at runtime until it is built."
            );
        }
    }
}

/// Ensure the directory containing `vswhere.exe` is on the child command's PATH.
///
/// NativeAOT's native link step shells out to `vswhere.exe` (no full path) to find the
/// MSVC linker. `vswhere.exe` lives in the VS Installer directory, which is typically not
/// on PATH; without it the link fails with a confusing error. No-op off Windows / when
/// already discoverable.
#[cfg(windows)]
fn ensure_vswhere_on_path(cmd: &mut Command) {
    let candidates = [
        env::var("ProgramFiles(x86)").ok(),
        env::var("ProgramFiles").ok(),
    ];
    for base in candidates.into_iter().flatten() {
        let installer = Path::new(&base)
            .join("Microsoft Visual Studio")
            .join("Installer");
        if installer.join("vswhere.exe").exists() {
            let current = env::var("PATH").unwrap_or_default();
            cmd.env("PATH", format!("{};{}", installer.display(), current));
            println!("cargo:warning=Added VS Installer dir to PATH for AOT link: {}", installer.display());
            return;
        }
    }
}

#[cfg(not(windows))]
fn ensure_vswhere_on_path(_cmd: &mut Command) {}

/// Copy native dependency libraries that NativeAOT does not statically link into the
/// main library (e.g. blake3_dotnet.dll) next to it, so the OS can resolve them.
fn copy_native_deps(src_dir: &Path, dest_dir: &Path) {
    let native_deps = [
        "blake3_dotnet.dll",
        "libblake3_dotnet.so",
        "libblake3_dotnet.dylib",
    ];

    for dep_name in &native_deps {
        let src = src_dir.join(dep_name);
        if src.exists() {
            let dst = dest_dir.join(dep_name);
            match fs::copy(&src, &dst) {
                Ok(_) => println!("cargo:warning=Copied native dependency {}", dep_name),
                Err(e) => println!(
                    "cargo:warning=Failed to copy native dep {} to {}: {}",
                    src.display(),
                    dst.display(),
                    e
                ),
            }
        }
    }
}
