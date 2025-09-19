use std::env;
use std::path::Path;
use std::fs;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir).parent().unwrap().parent().unwrap().parent().unwrap();
    let bridge_output_dir = target_dir.join("uassetbridge");
    
    // Get the workspace root (two levels up from uasset_app)
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let bridge_project_dir = workspace_root.join("tools").join("UAssetBridge");
    
    println!("cargo:rerun-if-changed={}", bridge_project_dir.join("Program.cs").display());
    println!("cargo:rerun-if-changed={}", bridge_project_dir.join("UAssetBridge.csproj").display());
    
    // Create output directory
    std::fs::create_dir_all(&bridge_output_dir).unwrap();
    
    // Try to use existing precompiled UAssetBridge.exe
    let precompiled_paths = [
        bridge_project_dir.join("bin").join("Release").join("net8.0").join("win-x64").join("UAssetBridge.exe"),
        bridge_project_dir.join("bin").join("Debug").join("net8.0").join("win-x64").join("UAssetBridge.exe"),
    ];
    
    let mut found_precompiled = false;
    for precompiled_path in &precompiled_paths {
        if precompiled_path.exists() {
            let dest_path = bridge_output_dir.join("UAssetBridge.exe");
            if let Err(e) = fs::copy(precompiled_path, &dest_path) {
                eprintln!("Failed to copy precompiled UAssetBridge.exe: {}", e);
                continue;
            }
            println!("Using precompiled UAssetBridge from: {}", precompiled_path.display());
            println!("UAssetBridge copied to: {}", dest_path.display());
            found_precompiled = true;
            break;
        }
    }
    
    if !found_precompiled {
        panic!("No precompiled UAssetBridge.exe found and dotnet is not available. Please ensure UAssetBridge is compiled or install .NET SDK.");
    }
}
