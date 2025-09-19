using System;
using System.IO;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Collections.Generic;

namespace UAssetBridge;

public class UAssetRequest
{
    [JsonPropertyName("action")]
    public string Action { get; set; } = "";
    
    [JsonPropertyName("file_path")]
    public string? FilePath { get; set; }
    
    [JsonPropertyName("mip_gen")]
    public string? MipGen { get; set; }
    
    [JsonPropertyName("uexp_path")]
    public string? UexpPath { get; set; }
}

public class UAssetResponse
{
    [JsonPropertyName("success")]
    public bool Success { get; set; }
    
    [JsonPropertyName("message")]
    public string Message { get; set; } = "";
    
    [JsonPropertyName("data")]
    public object? Data { get; set; }
}

public class Program
{
    public static async Task Main(string[] args)
    {
        try
        {
            // Read JSON request from stdin
            var input = await Console.In.ReadToEndAsync();
            if (string.IsNullOrWhiteSpace(input))
            {
                WriteError("No input provided");
                return;
            }

            var request = JsonSerializer.Deserialize<UAssetRequest>(input);
            if (request == null)
            {
                WriteError("Invalid JSON request");
                return;
            }

            var response = ProcessRequest(request);
            var responseJson = JsonSerializer.Serialize(response);
            Console.WriteLine(responseJson);
        }
        catch (Exception ex)
        {
            WriteError($"Unhandled exception: {ex.Message}");
        }
    }

    private static UAssetResponse ProcessRequest(UAssetRequest request)
    {
        try
        {
            return request.Action switch
            {
                "detect_texture" => DetectTexture(request.FilePath),
                "set_mip_gen" => SetMipGen(request.FilePath, request.MipGen),
                "get_texture_info" => GetTextureInfo(request.FilePath),
                "detect_mesh" => DetectMesh(request.FilePath),
                "patch_mesh" => PatchMesh(request.FilePath, request.UexpPath),
                "get_mesh_info" => GetMeshInfo(request.FilePath),
                _ => new UAssetResponse 
                { 
                    Success = false, 
                    Message = $"Unknown action: {request.Action}" 
                }
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error processing request: {ex.Message}" 
            };
        }
    }

    private static UAssetResponse DetectTexture(string? filePath)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"File not found: {filePath}" 
            };
        }

        try
        {
            // Simple heuristic: check file extension and basic file structure
            // This is a placeholder implementation - can be enhanced with UAssetAPI later
            var isTexture = IsLikelyTextureUAsset(filePath);

            return new UAssetResponse 
            { 
                Success = true, 
                Message = isTexture ? "File is likely a texture" : "File is not a texture",
                Data = isTexture
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error reading uasset file: {ex.Message}" 
            };
        }
    }

    private static bool IsLikelyTextureUAsset(string filePath)
    {
        try
        {
            // Basic heuristic: check if filename suggests it's a texture
            var fileName = Path.GetFileNameWithoutExtension(filePath).ToLowerInvariant();
            
            // Common texture naming patterns
            var textureIndicators = new[] { "tex", "texture", "diffuse", "normal", "specular", "roughness", "metallic", "albedo", "basecolor", "t_" };
            
            foreach (var indicator in textureIndicators)
            {
                if (fileName.Contains(indicator))
                {
                    return true;
                }
            }

            // Check file size - textures are typically larger
            var fileInfo = new FileInfo(filePath);
            if (fileInfo.Length > 10000) // 10KB threshold
            {
                return true;
            }

            return false;
        }
        catch
        {
            return false;
        }
    }

    private static UAssetResponse SetMipGen(string? filePath, string? mipGen)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (string.IsNullOrEmpty(mipGen))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "Mip gen setting is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"File not found: {filePath}" 
            };
        }

        try
        {
            // Placeholder implementation - create backup and simulate modification
            var backupPath = filePath + ".backup";
            File.Copy(filePath, backupPath, true);
            
            // TODO: Implement actual UAssetAPI modification when API issues are resolved
            // For now, just return success to allow testing of the Rust integration
            
            return new UAssetResponse 
            { 
                Success = true, 
                Message = $"Placeholder: Would set MipGenSettings to {mipGen} (backup created)" 
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error modifying uasset file: {ex.Message}" 
            };
        }
    }

    private static UAssetResponse GetTextureInfo(string? filePath)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"File not found: {filePath}" 
            };
        }

        try
        {
            var textureInfo = new Dictionary<string, object>
            {
                ["MipGenSettings"] = "Unknown",
                ["Width"] = 0,
                ["Height"] = 0,
                ["Format"] = "Unknown"
            };

            // TODO: Implement actual texture info extraction with UAssetAPI
            // For now, return placeholder data
            
            return new UAssetResponse 
            { 
                Success = true, 
                Message = "Placeholder texture info retrieved",
                Data = textureInfo
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error reading texture info: {ex.Message}" 
            };
        }
    }

    private static UAssetResponse DetectMesh(string? filePath)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"File not found: {filePath}" 
            };
        }

        try
        {
            // Heuristic mesh detection based on filename patterns and path
            var isMesh = IsLikelyMeshUAsset(filePath);

            return new UAssetResponse 
            { 
                Success = true, 
                Message = isMesh ? "File is likely a mesh" : "File is not a mesh",
                Data = isMesh
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error reading uasset file: {ex.Message}" 
            };
        }
    }

    private static bool IsLikelyMeshUAsset(string filePath)
    {
        try
        {
            var fileName = Path.GetFileNameWithoutExtension(filePath).ToLowerInvariant();
            var pathStr = filePath.ToLowerInvariant();
            
            // Common mesh naming patterns and path indicators
            var meshIndicators = new[] { 
                "mesh", "sk_", "sm_", "skeletal", "static", "character", "weapon", "armor", 
                "body", "head", "hair", "face", "/meshes/", "\\meshes\\", "_mesh", "model" 
            };
            
            foreach (var indicator in meshIndicators)
            {
                if (fileName.Contains(indicator) || pathStr.Contains(indicator))
                {
                    return true;
                }
            }

            // Check file size - meshes are typically larger than textures
            var fileInfo = new FileInfo(filePath);
            if (fileInfo.Length > 50000) // 50KB threshold for meshes
            {
                return true;
            }

            return false;
        }
        catch
        {
            return false;
        }
    }

    private static UAssetResponse PatchMesh(string? filePath, string? uexpPath)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (string.IsNullOrEmpty(uexpPath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "UEXP path is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"UAsset file not found: {filePath}" 
            };
        }

        if (!File.Exists(uexpPath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"UEXP file not found: {uexpPath}" 
            };
        }

        try
        {
            // Create backups
            var uassetBackup = filePath + ".backup";
            var uexpBackup = uexpPath + ".backup";
            File.Copy(filePath, uassetBackup, true);
            File.Copy(uexpPath, uexpBackup, true);
            
            // TODO: Implement actual mesh patching using UAssetAPI
            // For now, just return success to allow testing of the Rust integration
            // The actual mesh patching logic from uasset_mesh_patch_rivals can be integrated here
            
            return new UAssetResponse 
            { 
                Success = true, 
                Message = "Placeholder: Would patch mesh materials (backups created)" 
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error patching mesh: {ex.Message}" 
            };
        }
    }

    private static UAssetResponse GetMeshInfo(string? filePath)
    {
        if (string.IsNullOrEmpty(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = "File path is required" 
            };
        }

        if (!File.Exists(filePath))
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"File not found: {filePath}" 
            };
        }

        try
        {
            var meshInfo = new Dictionary<string, object>
            {
                ["MaterialCount"] = 0,
                ["VertexCount"] = 0,
                ["TriangleCount"] = 0,
                ["IsSkeletalMesh"] = false
            };

            // TODO: Implement actual mesh info extraction with UAssetAPI
            // For now, return placeholder data
            
            return new UAssetResponse 
            { 
                Success = true, 
                Message = "Placeholder mesh info retrieved",
                Data = meshInfo
            };
        }
        catch (Exception ex)
        {
            return new UAssetResponse 
            { 
                Success = false, 
                Message = $"Error reading mesh info: {ex.Message}" 
            };
        }
    }

    private static void WriteError(string message)
    {
        var errorResponse = new UAssetResponse 
        { 
            Success = false, 
            Message = message 
        };
        var errorJson = JsonSerializer.Serialize(errorResponse);
        Console.WriteLine(errorJson);
    }
}
