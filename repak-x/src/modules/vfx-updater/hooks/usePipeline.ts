// VFX Updater - Pipeline Hook
// Orchestrates the 8-step VFX update pipeline

import { useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ColorParam, VfxPipelineProgress, VfxTempDirectories, PipelineStep, PIPELINE_STEPS } from "../types";
import { parseJsonAndExtractColors } from "../lib/colors/extractColors";
import { applyColorToJson } from "../lib/colors/applyColors";

interface StepStatus {
  message?: string;
  error?: boolean;
}

interface PipelineState {
  tempDirs: VfxTempDirectories | null;
  modAssets: string[];
  updatableAssets: string[];
  nonUpdatableAssets: string[];
  modJsonFiles: string[];
  vanillaAssets: string[];
  vanillaJsonFiles: string[];
  outputUassets: string[];
}

// Parallel processing utility
async function parallelMapWithLimit<T, R>(
  items: T[],
  limit: number,
  worker: (item: T, index: number) => Promise<R>
): Promise<R[]> {
  const results = new Array<R>(items.length);
  const safeLimit = Math.max(1, Math.min(limit, items.length || 1));
  let nextIndex = 0;

  const runners = Array.from({ length: safeLimit }, async () => {
    while (true) {
      const currentIndex = nextIndex;
      nextIndex += 1;
      if (currentIndex >= items.length) break;
      results[currentIndex] = await worker(items[currentIndex], currentIndex);
    }
  });

  await Promise.all(runners);
  return results;
}

export interface UsePipelineProps {
  usmapPath: string | null;
  gamePaksPath: string | null;
  modPath: string | null;
  outputPath: string | null;
  addLog: (message: string, type?: "info" | "success" | "warning" | "error" | "debug") => void;
}

export function usePipeline({
  usmapPath,
  gamePaksPath,
  modPath,
  outputPath,
  addLog,
}: UsePipelineProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [stepStatus, setStepStatus] = useState<StepStatus | null>(null);
  const [extractedColors, setExtractedColors] = useState<ColorParam[]>([]);
  const [isProcessing, setIsProcessing] = useState(false);
  const runIdRef = useRef(0);

  const stateRef = useRef<PipelineState>({
    tempDirs: null,
    modAssets: [],
    updatableAssets: [],
    nonUpdatableAssets: [],
    modJsonFiles: [],
    vanillaAssets: [],
    vanillaJsonFiles: [],
    outputUassets: [],
  });

  const runPipeline = useCallback(async () => {
    if (!usmapPath || !gamePaksPath || !modPath) {
      addLog("Please configure USMAP path, game paks path, and select a mod file", "error");
      return;
    }

    setIsProcessing(true);
    setCurrentStep(1);
    setExtractedColors([]);
    const currentRunId = ++runIdRef.current;

    const checkCancel = () => {
      if (runIdRef.current !== currentRunId) throw new Error("Pipeline cancelled");
    };

    const warnings: string[] = [];

    try {
      // Initialize
      addLog("Preparing VFX update pipeline.", "info");
      console.debug("[VFX] Pipeline ready", { usmapPath, gamePaksPath, modPath, outputPath });

      // Cleanup previous temp directories
      addLog("Cleaning up previous temp files.", "info");
      await invoke("vfx_cleanup_temp_directories");

      // Start UAT session
      addLog("Starting UAssetTool session.", "info");
      await invoke("vfx_start_session");

      // Create temp directories
      const tempDirs = await invoke<VfxTempDirectories>("vfx_create_pipeline_directories");
      stateRef.current.tempDirs = tempDirs;
      console.debug("[VFX] Temp directories created", tempDirs);

      // ===== STEP 1: Extract Mod Assets =====
      checkCancel();
      addLog("Step 1: Extracting mod assets...", "info");
      setStepStatus({ message: "Extracting mod assets..." });

      const modAssets = await invoke<string[]>("vfx_extract_mod_assets", {
        gamePaks: gamePaksPath,
        modPath: modPath,
        outputDir: tempDirs.modExtract,
      });
      checkCancel();
      stateRef.current.modAssets = modAssets;
      addLog(`✓ Extracted ${modAssets.length} mod assets`, "success");
      console.debug("[VFX] Step 1 complete", { modAssets: modAssets.length });

      if (modAssets.length === 0) {
        throw new Error("No assets found in mod");
      }

      // Step 8 packs only these, so a stray dependency can never reach the bundle.
      const modExtractNorm = tempDirs.modExtract.replace(/\\/g, "/");
      const toRelative = (p: string) => {
        let norm = p.replace(/\\/g, "/");
        if (norm.startsWith(modExtractNorm)) {
          norm = norm.substring(modExtractNorm.length).replace(/^\//, "");
        }
        return norm;
      };
      const modAssetManifest = modAssets.map(toRelative);

      // ===== STEP 1.5: Scan Asset Classes =====
      checkCancel();
      addLog("Scanning asset classes...", "info");
      setStepStatus({ message: "Scanning asset classes..." });

      // Get UAssetTool path
      const uatPath = await invoke<string>("vfx_get_uasset_tool_path");
      checkCancel();

      // Get detected asset type for each asset (batch_detect)
      const assetTypes = await invoke<Record<string, string>>("vfx_get_asset_classes", {
        uatPath,
        usmapPath,
        uassetPaths: modAssets,
      });
      checkCancel();

      console.debug("[VFX] Asset types scanned", { total: Object.keys(assetTypes).length });

      // Separate processable assets from bypass assets.
      // Only blueprint + material_instance should go through edit pipeline.
      const updatableAssets: string[] = [];
      const nonUpdatableAssets: string[] = [];

      for (const assetPath of modAssets) {
        const assetType = assetTypes[assetPath] || "other";
        const isUpdatable = ["blueprint", "material_instance"].includes(assetType);

        if (isUpdatable) {
          updatableAssets.push(assetPath);
        } else {
          nonUpdatableAssets.push(assetPath);
        }
      }

      stateRef.current.updatableAssets = updatableAssets;
      stateRef.current.nonUpdatableAssets = nonUpdatableAssets;

      addLog(`✓ Found ${updatableAssets.length} processable (blueprint/material_instance) and ${nonUpdatableAssets.length} bypass assets`, "success");
      console.debug("[VFX] Asset classification", { updatable: updatableAssets.length, nonUpdatable: nonUpdatableAssets.length });

      // Copy non-updatable assets directly to final output (they pass through unchanged)
      if (nonUpdatableAssets.length > 0) {
        addLog(`Copying ${nonUpdatableAssets.length} bypass assets to output...`, "info");
        await invoke("vfx_copy_uasset_files", {
          sourcePaths: nonUpdatableAssets,
          sourceBaseDir: tempDirs.modExtract,
          destBaseDir: tempDirs.finalUassets,
        });
        checkCancel();
        console.debug("[VFX] Non-updatable assets copied");
      }

      // If no updatable assets, skip color pipeline and go to pack
      if (updatableAssets.length === 0) {
        addLog("No updatable assets found - skipping color pipeline", "warning");
        // Jump directly to pack step (step 8)
        setCurrentStep(8);
        addLog("Step 8: Creating IOStore mod bundle.", "info");
        setStepStatus({ message: "Packing IOStore..." });

        const modBaseName = modPath
          .split(/[\\/]/)
          .pop()!
          .replace(".utoc", "")
          .replace(/_\d+_P$/, "")
          .replace(/_P$/, "");

        const outputBase = outputPath
          ? `${outputPath}/${modBaseName}_UPDATED_9999999_P`
          : `${gamePaksPath}/~mods/${modBaseName}_UPDATED_9999999_P`;

        const finalBundle = await invoke<string>("vfx_pack_to_iostore", {
          usmapPath,
          inputDir: tempDirs.finalUassets,
          outputBase,
          allowedAssets: modAssetManifest,
        });
        checkCancel();

        addLog(`✓ Created updated mod: ${finalBundle}`, "success");
        setCurrentStep(9);
        addLog("Pipeline completed successfully!", "success");

        // Early return for non-updatable only mods
        return;
      }

      const processableModExtract = await invoke<string>("vfx_create_step_directory", {
        stepName: "processable_mod_extract",
      });
      await invoke("vfx_copy_uasset_files", {
        sourcePaths: updatableAssets,
        sourceBaseDir: tempDirs.modExtract,
        destBaseDir: processableModExtract,
      });
      checkCancel();
      console.debug("[VFX] Prepared processable-only extract", {
        processableModExtract,
        count: updatableAssets.length,
      });

      // ===== STEP 2: Mod UAssets → JSON (only blueprint/material_instance) =====
      checkCancel();
      setCurrentStep(2);
      addLog("Step 2: Converting processable mod assets to JSON.", "info");
      setStepStatus({ message: "Converting mod assets to JSON..." });

      const modJsonFiles = await invoke<string[]>("vfx_convert_uassets_to_json", {
        usmapPath,
        inputDir: processableModExtract,
        outputDir: tempDirs.modJson,
      });
      checkCancel();
      stateRef.current.modJsonFiles = modJsonFiles;
      addLog(`✓ Converted ${modJsonFiles.length} files to JSON`, "success");
      console.debug("[VFX] Step 2 complete", { modJsonFiles: modJsonFiles.length });

      // ===== STEP 3: Parse Colors =====
      checkCancel();
      setCurrentStep(3);
      addLog("Step 3: Extracting color parameters.", "info");
      setStepStatus({ message: "Parsing colors from mod assets..." });

      const allColors: ColorParam[] = [];
      const parseConcurrency = 6;

      const parseResults = await parallelMapWithLimit(modJsonFiles, parseConcurrency, async (jsonPath: string) => {
        if (runIdRef.current !== currentRunId) throw new Error("Pipeline cancelled");
        try {
          const jsonContent = await invoke<string>("vfx_read_json_file", { path: jsonPath });
          const json = JSON.parse(jsonContent);
          const fileName = jsonPath.split(/[\\/]/).pop()!;
          const relativePath = jsonPath.replace(tempDirs.modJson, "").replace(/^[\\/]/, "");
          const localColors: ColorParam[] = [];
          parseJsonAndExtractColors(json, fileName, relativePath, localColors);
          return { colors: localColors, warning: null as string | null };
        } catch (e) {
          return {
            colors: [] as ColorParam[],
            warning: `Could not parse ${jsonPath.split(/[\\/]/).pop()}: ${e}`,
          };
        }
      });

      for (const result of parseResults) {
        if (result.warning) {
          addLog(`Warning: ${result.warning}`, "warning");
          warnings.push(result.warning);
        }
        if (result.colors.length > 0) {
          allColors.push(...result.colors);
        }
      }

      setExtractedColors(allColors);
      addLog(`✓ Extracted ${allColors.length} color parameters`, "success");
      console.debug("[VFX] Step 3 complete", { colors: allColors.length });

      if (allColors.length === 0) {
        addLog("No color parameters found - pipeline will create passthrough mod", "warning");
      }

      // ===== STEP 4: Extract Vanilla Assets =====
      checkCancel();
      setCurrentStep(4);
      addLog("Step 4: Extracting vanilla assets...", "info");
      setStepStatus({ message: "Extracting vanilla assets from game..." });

      // Extract relative game paths from full paths for vanilla filter
      const filterPatterns = updatableAssets.map((p) => toRelative(p).replace(/\.uasset$/, ""));
      console.debug("[VFX] Filter patterns (first 3):", filterPatterns.slice(0, 3));

      const vanillaAssets = await invoke<string[]>("vfx_extract_vanilla_assets", {
        gamePaks: gamePaksPath,
        outputDir: tempDirs.vanillaExtract,
        filterPatterns,
      });
      checkCancel();
      stateRef.current.vanillaAssets = vanillaAssets;
      addLog(`✓ Extracted ${vanillaAssets.length} vanilla assets`, "success");
      console.debug("[VFX] Step 4 complete", { vanillaAssets: vanillaAssets.length });

      // ===== STEP 5: Vanilla UAssets → JSON =====
      checkCancel();
      setCurrentStep(5);
      addLog("Step 5: Converting vanilla assets to JSON.", "info");
      setStepStatus({ message: "Converting game assets to JSON..." });

      const vanillaJsonFiles = await invoke<string[]>("vfx_convert_uassets_to_json", {
        usmapPath,
        inputDir: tempDirs.vanillaExtract,
        outputDir: tempDirs.vanillaJson,
      });
      checkCancel();
      stateRef.current.vanillaJsonFiles = vanillaJsonFiles;
      addLog(`✓ Converted ${vanillaJsonFiles.length} vanilla files to JSON`, "success");
      console.debug("[VFX] Step 5 complete", { vanillaJsonFiles: vanillaJsonFiles.length });

      // ===== STEP 6: Apply Colors =====
      checkCancel();
      setCurrentStep(6);
      addLog("Step 6: Applying mod colors to vanilla assets.", "info");
      setStepStatus({ message: "Applying colors..." });

      let appliedCount = 0;
      const applyConcurrency = 6;

      const applyResults = await parallelMapWithLimit(vanillaJsonFiles, applyConcurrency, async (jsonPath) => {
        if (runIdRef.current !== currentRunId) throw new Error("Pipeline cancelled");
        try {
          const jsonContent = await invoke<string>("vfx_read_json_file", { path: jsonPath });
          let json = JSON.parse(jsonContent);
          const relativePath = jsonPath.replace(tempDirs.vanillaJson, "").replace(/^[\\/]/, "");

          const matchingColors = allColors.filter((c) =>
            c.relativePath === relativePath ||
            c.relativePath.replace(".json", "") === relativePath.replace(".json", "") ||
            c.relativePath.split(/[\\/]/).pop() === relativePath.split(/[\\/]/).pop()
          );

          let appliedToFile = 0;
          const fileWarnings: string[] = [];

          if (matchingColors.length > 0) {
            console.debug("[VFX] Applying colors to", relativePath, matchingColors.length);

            for (const color of matchingColors) {
              try {
                const applied = applyColorToJson(json, color);
                if (applied) {
                  appliedToFile++;
                } else {
                  fileWarnings.push(`Could not apply ${color.paramName}: path not found`);
                }
              } catch (e) {
                fileWarnings.push(`Error applying ${color.paramName}: ${e}`);
              }
            }

            if (appliedToFile > 0) {
              // Write to edited_json directory
              const editedPath = jsonPath.replace(tempDirs.vanillaJson, tempDirs.editedJson);
              await invoke("vfx_write_json_file", {
                path: editedPath,
                content: JSON.stringify(json, null, 2)
              });
            }
          }

          return { appliedToFile, fileWarnings };
        } catch (e) {
          return { appliedToFile: 0, fileWarnings: [`Failed to process: ${jsonPath}`] };
        }
      });

      appliedCount = applyResults.reduce((sum, r) => sum + r.appliedToFile, 0);
      for (const result of applyResults) {
        if (result.fileWarnings.length > 0) {
          result.fileWarnings.forEach(w => addLog(`Warning: ${w}`, "warning"));
          warnings.push(...result.fileWarnings);
        }
      }

      addLog(`✓ Applied ${appliedCount} color values`, "success");
      console.debug("[VFX] Step 6 complete", { appliedCount });

      // ===== STEP 7: JSON → UAssets =====
      checkCancel();
      setCurrentStep(7);
      addLog("Step 7: Converting edited JSON back to UAssets...", "info");
      setStepStatus({ message: "Converting to UAsset..." });

      const outputUassets = await invoke<string[]>("vfx_convert_json_to_uassets", {
        usmapPath,
        inputDir: tempDirs.editedJson,
        outputDir: tempDirs.finalUassets,
      });
      checkCancel();
      stateRef.current.outputUassets = outputUassets;
      addLog(`✓ Created ${outputUassets.length} UAsset files`, "success");
      console.debug("[VFX] Step 7 complete", { outputUassets: outputUassets.length });

      // ===== STEP 8: Pack IOStore =====
      checkCancel();
      setCurrentStep(8);
      addLog("Step 8: Creating IOStore mod bundle.", "info");
      setStepStatus({ message: "Packing updated mod..." });

      const modBaseName = modPath
        .split(/[\\/]/)
        .pop()!
        .replace(".utoc", "")
        .replace(/_\d+_P$/, "")
        .replace(/_P$/, "");

      const outputBase = outputPath
        ? `${outputPath}/${modBaseName}_UPDATED_9999999_P`
        : `${gamePaksPath}/~mods/${modBaseName}_UPDATED_9999999_P`;

      const finalBundle = await invoke<string>("vfx_pack_to_iostore", {
        usmapPath,
        inputDir: tempDirs.finalUassets,
        outputBase,
        allowedAssets: modAssetManifest,
      });
      checkCancel();

      addLog(`✓ Created updated mod: ${finalBundle}`, "success");
      console.debug("[VFX] Step 8 complete", { finalBundle });

      // Done!
      setCurrentStep(9);
      if (warnings.length > 0) {
        addLog(`Pipeline completed with ${warnings.length} warnings`, "warning");
      } else {
        addLog("Pipeline completed successfully!", "success");
      }

    } catch (error) {
      if (error instanceof Error && error.message.includes("Pipeline cancelled")) {
        addLog("Pipeline was cancelled by user.", "warning");
        setStepStatus({ error: true, message: "Cancelled." });
      } else {
        addLog(`Pipeline error: ${error}`, "error");
        setStepStatus({ error: true, message: String(error) });
        console.error("[VFX] Pipeline error", error);
      }
    } finally {
      // Cleanup
      try {
        if (runIdRef.current === currentRunId) {
          addLog("Stopping UAssetTool session.", "debug");
          await invoke("vfx_stop_session");
        }
      } catch (e) {
        console.debug("[VFX] Failed to stop session", e);
      }

      if (runIdRef.current === currentRunId) {
        setIsProcessing(false);
      }
    }
  }, [usmapPath, gamePaksPath, modPath, outputPath, addLog]);

  const cancelPipeline = useCallback(async () => {
    if (isProcessing) {
      addLog("Cancelling pipeline...", "warning");
      runIdRef.current++;
      try {
        await invoke("vfx_stop_session");
      } catch (e) {
        console.debug("[VFX] Stop session error", e);
      }
      setIsProcessing(false);
      setCurrentStep(0);
    }
  }, [isProcessing, addLog]);

  return {
    currentStep,
    stepStatus,
    isProcessing,
    extractedColors,
    runPipeline,
    cancelPipeline,
  };
}
