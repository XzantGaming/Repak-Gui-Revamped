use anyhow::Result;
use std::io::{self, BufRead};
use uasset_toolkit::{UAssetToolkit, UAssetToolkitSync};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    // Try to auto-detect bridge path if not provided
    let bridge_path = if args.len() >= 2 && !args[1].ends_with(".uasset") {
        Some(args[1].clone())
    } else {
        None
    };
    
    let toolkit = UAssetToolkit::new(bridge_path)?;
    
    // Determine if we have file arguments
    let file_args: Vec<&String> = if args.len() >= 2 && !args[1].ends_with(".uasset") {
        // First arg is bridge path, files start from index 2
        args.iter().skip(2).collect()
    } else {
        // No bridge path provided, files start from index 1
        args.iter().skip(1).collect()
    };
    
    if !file_args.is_empty() {
        // Command line mode - process files
        for file_path in file_args {
            println!("Processing: {}", file_path);
            match toolkit.process_texture_uasset(file_path).await {
                Ok(true) => println!("✓ {} - Texture detected and set to NoMipmaps", file_path),
                Ok(false) => println!("- {} - Not a texture uasset", file_path),
                Err(e) => eprintln!("✗ {} - Error: {}", file_path, e),
            }
        }
    } else {
        // Interactive mode - read from stdin
        println!("UAsset Toolkit - Interactive Mode");
        println!("Enter uasset file paths (one per line), or 'quit' to exit:");
        println!("Commands:");
        println!("  <file_path>           - Process texture uasset");
        println!("  info <file>           - Get texture info");
        println!("  mesh <file>           - Check if file is mesh uasset");
        println!("  mesh-info <file>      - Get mesh info");
        println!("  patch-mesh <uasset> <uexp> - Patch mesh materials");
        println!("  quit/exit             - Exit program");
        println!();
        
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            let line = line.trim();
            
            if line == "quit" || line == "exit" {
                break;
            }
            
            if line.is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            
            match parts.as_slice() {
                ["info", file_path] => {
                    match toolkit.get_texture_info(file_path).await {
                        Ok(info) => {
                            println!("Texture Info for {}:", file_path);
                            if let Some(mip_gen) = &info.mip_gen_settings {
                                println!("  MipGenSettings: {}", mip_gen);
                            }
                            if let Some(width) = info.width {
                                println!("  Width: {}", width);
                            }
                            if let Some(height) = info.height {
                                println!("  Height: {}", height);
                            }
                            if let Some(format) = &info.format {
                                println!("  Format: {}", format);
                            }
                        }
                        Err(e) => eprintln!("✗ Error getting texture info: {}", e),
                    }
                }
                ["mesh", file_path] => {
                    match toolkit.is_mesh_uasset(file_path).await {
                        Ok(is_mesh) => {
                            if is_mesh {
                                println!("✓ {} is a mesh uasset", file_path);
                            } else {
                                println!("- {} is not a mesh uasset", file_path);
                            }
                        }
                        Err(e) => eprintln!("✗ Error detecting mesh: {}", e),
                    }
                }
                ["mesh-info", file_path] => {
                    match toolkit.get_mesh_info(file_path).await {
                        Ok(info) => {
                            println!("Mesh Info for {}:", file_path);
                            if let Some(mat_count) = info.material_count {
                                println!("  Material Count: {}", mat_count);
                            }
                            if let Some(vert_count) = info.vertex_count {
                                println!("  Vertex Count: {}", vert_count);
                            }
                            if let Some(tri_count) = info.triangle_count {
                                println!("  Triangle Count: {}", tri_count);
                            }
                            if let Some(is_skeletal) = info.is_skeletal_mesh {
                                println!("  Is Skeletal Mesh: {}", is_skeletal);
                            }
                        }
                        Err(e) => eprintln!("✗ Error getting mesh info: {}", e),
                    }
                }
                ["patch-mesh", uasset_path, uexp_path] => {
                    match toolkit.patch_mesh(uasset_path, uexp_path).await {
                        Ok(()) => println!("✓ Successfully patched mesh: {}", uasset_path),
                        Err(e) => eprintln!("✗ Error patching mesh: {}", e),
                    }
                }
                _ => {
                    // Treat as file path
                    match toolkit.process_texture_uasset(line).await {
                        Ok(true) => println!("✓ {} - Texture detected and set to NoMipmaps", line),
                        Ok(false) => println!("- {} - Not a texture uasset", line),
                        Err(e) => eprintln!("✗ {} - Error: {}", line, e),
                    }
                }
            }
        }
    }
    
    Ok(())
}
