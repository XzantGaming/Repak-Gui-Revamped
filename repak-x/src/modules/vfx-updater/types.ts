// VFX Updater - Type Definitions

export interface ColorParam {
  id: string;
  fileName: string;
  paramName: string;
  path: (string | number)[];
  rgba: { R: number; G: number; B: number; A: number };
  relativePath: string;
}

export interface VfxPipelineProgress {
  stage: string;
  step: number;
  current: number;
  total: number;
  message: string;
}

export interface VfxPipelineResult {
  success: boolean;
  outputPath: string | null;
  colorsExtracted: number;
  colorsApplied: number;
  warnings: string[];
  error: string | null;
}

export interface VfxSettings {
  usmapPath: string | null;
  // Auto-update bookkeeping for the managed USMAP — must be preserved when
  // saving settings from the UI.
  usmapSha?: string | null;
  usmapFilename?: string | null;
  usmapEtag?: string | null;
}

export interface VfxTempDirectories {
  base: string;
  modExtract: string;
  modJson: string;
  vanillaExtract: string;
  vanillaJson: string;
  editedJson: string;
  finalUassets: string;
}

export interface AssetClassInfo {
  filePath: string;
  className: string | null;
  isMaterialInstance: boolean;
  isNiagara: boolean;
  isWidget: boolean;
}

export interface LogEntry {
  message: string;
  type: "info" | "success" | "warning" | "error" | "debug";
  time: string;
}

export interface PipelineStep {
  id: number;
  title: string;
  description: string;
  status: "pending" | "running" | "completed" | "error";
}

export const PIPELINE_STEPS: PipelineStep[] = [
  { id: 1, title: "Extract Mod Assets", description: "Extracting assets from mod IOStore", status: "pending" },
  { id: 2, title: "Convert Mod to JSON", description: "Converting UAssets to JSON format", status: "pending" },
  { id: 3, title: "Parse Colors", description: "Extracting color parameters from mod", status: "pending" },
  { id: 4, title: "Extract Vanilla Assets", description: "Extracting matching vanilla assets", status: "pending" },
  { id: 5, title: "Convert Vanilla to JSON", description: "Converting vanilla UAssets to JSON", status: "pending" },
  { id: 6, title: "Apply Colors", description: "Applying mod colors to vanilla assets", status: "pending" },
  { id: 7, title: "Convert to UAssets", description: "Converting edited JSON back to UAssets", status: "pending" },
  { id: 8, title: "Pack IOStore", description: "Creating final mod package", status: "pending" },
];
