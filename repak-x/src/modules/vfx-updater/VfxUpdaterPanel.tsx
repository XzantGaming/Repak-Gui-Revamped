import React, { useState, useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence, useMotionValue, useTransform, animate } from "framer-motion";
import { usePipeline } from "./hooks/usePipeline";
import { PIPELINE_STEPS } from "./types";
import type { VfxSettings } from "./types";
import { ShineBorder } from "../../components/ui/ShineBorder";
import { AuroraText } from "../../components/ui/AuroraText";
import {
  VscFolder,
  VscFolderOpened,
  VscChevronRight,
  VscChevronDown,
  VscClose
} from 'react-icons/vsc';
import { MdCheckCircle } from 'react-icons/md';
import { BiCopyAlt } from 'react-icons/bi';

import "../../components/ExtensionModOverlay.css";
import "./VfxUpdater.css";

const VFX_UPDATER_MOD_PREFILL_KEY = 'repakx:vfxUpdater:modPath'

// -------------------------------------------------------------
// Folder Tree Types and Logic (Adapted from ExtensionModOverlay)
// -------------------------------------------------------------
type FolderRecord = {
  id: string;
  name: string;
  is_root?: boolean;
};

type TreeNode = {
  id: string;
  name: string;
  children: TreeNode[];
  isVirtual: boolean;
  fullPath?: string;
  originalName?: string;
};

const buildTree = (folders: FolderRecord[]) => {
  const root: any = { id: 'root', name: 'root', children: {}, isVirtual: true };
  const sortedFolders = [...folders].sort((a, b) => a.name.localeCompare(b.name));

  sortedFolders.forEach(folder => {
    const parts = folder.id.split(/[/\\]/);
    let current = root;

    parts.forEach((part, index) => {
      if (!current.children[part]) {
        current.children[part] = {
          name: part,
          children: {},
          isVirtual: true,
          fullPath: parts.slice(0, index + 1).join('/')
        };
      }
      current = current.children[part];

      if (index === parts.length - 1) {
        current.id = folder.id;
        current.isVirtual = false;
        current.originalName = folder.name;
      }
    });
  });

  return root;
};

const convertToArray = (node: any): TreeNode[] => {
  if (!node.children) return [];
  const children = Object.values(node.children).map((child: any): TreeNode => ({
    id: child.id ?? child.fullPath ?? child.name,
    name: child.name,
    children: convertToArray(child),
    isVirtual: Boolean(child.isVirtual),
    fullPath: child.fullPath,
    originalName: child.originalName
  }));
  children.sort((a, b) => a.name.localeCompare(b.name));
  return children;
};

const FolderNode = ({ node, selectedFolderId, onSelect, depth = 0 }: { node: TreeNode; selectedFolderId: string | null; onSelect: (id: string | null) => void; depth?: number }) => {
  const [isOpen, setIsOpen] = useState(true);
  const hasChildren = node.children && node.children.length > 0;
  const isSelected = selectedFolderId === node.id;

  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!node.isVirtual) {
      onSelect(node.id);
    } else {
      setIsOpen(!isOpen);
    }
  };

  return (
    <div className="ext-folder-node">
      <div
        className={`ext-folder-item ${isSelected ? 'selected' : ''} ${node.isVirtual ? 'virtual' : ''}`}
        onClick={handleClick}
        style={{ paddingLeft: `${depth * 16 + 8}px` }}
      >
        <span className="folder-toggle" onClick={(e) => { e.stopPropagation(); setIsOpen(!isOpen); }}>
          {hasChildren ? (isOpen ? <VscChevronDown /> : <VscChevronRight />) : <span style={{ width: 16 }} />}
        </span>
        <span className="folder-icon">
          {isSelected || isOpen ? <VscFolderOpened /> : <VscFolder />}
        </span>
        <span className="folder-name">{node.name}</span>
      </div>

      {hasChildren && isOpen && (
        <div className="ext-folder-children">
          {node.children.map(child => (
            <FolderNode
              key={child.fullPath || child.id}
              node={child}
              selectedFolderId={selectedFolderId}
              onSelect={onSelect}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  );
};

interface LogEntry {
  message: string;
  type: "info" | "success" | "warning" | "error" | "debug";
  time: string;
}

export default function VfxUpdaterPanel() {
  const [usmapPath, setUsmapPath] = useState<string | null>(null);
  const [gamePaksPath, setGamePaksPath] = useState<string | null>(null);
  const [modPath, setModPath] = useState<string | null>(null);
  const outputPath = null; // Forces processing to default to ~mods 

  const [viewMode, setViewMode] = useState<'input' | 'progress' | 'complete'>('input');

  const [folders, setFolders] = useState<FolderRecord[]>([]);
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(null);

  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [copyFeedback, setCopyFeedback] = useState<string | null>(null);

  useEffect(() => {
    const applyIncomingModPath = () => {
      const prefilledPath = localStorage.getItem(VFX_UPDATER_MOD_PREFILL_KEY)
      if (!prefilledPath) return

      setModPath(prefilledPath)
      setViewMode('input')
      localStorage.removeItem(VFX_UPDATER_MOD_PREFILL_KEY)
    }

    applyIncomingModPath()

    const onStorage = (e: StorageEvent) => {
      if (e.key === VFX_UPDATER_MOD_PREFILL_KEY) {
        applyIncomingModPath()
      }
    }

    window.addEventListener('storage', onStorage)
    return () => window.removeEventListener('storage', onStorage)
  }, [])

  const handleCopyLogs = async () => {
    if (logs.length === 0) return;
    try {
      const text = logs.map(l => `[${l.time}] ${l.message}`).join('\n');
      await navigator.clipboard.writeText(text);
      setCopyFeedback('Copied!');
      setTimeout(() => setCopyFeedback(null), 1500);
    } catch (e) {
      console.error('Failed to copy logs', e);
    }
  };

  // Sync Theme
  useEffect(() => {
    const AURORA_PALETTES: Record<string, string[]> = {
      '#be1c1c': ['#be1c1c', '#ff9800', '#ffcc00', '#ff6b35'],
      '#4a9eff': ['#4a9eff', '#a855f7', '#ff6b9d', '#38bdf8'],
      '#9c27b0': ['#9c27b0', '#e91e63', '#00bcd4', '#7c3aed'],
      '#4CAF50': ['#4CAF50', '#8bc34a', '#00e676', '#e91e63'],
      '#ff9800': ['#ff9800', '#ff5722', '#ffc107', '#4a9eff'],
      '#FF96BC': ['#FF96BC', '#f472b6', '#c084fc', '#fda4af'],
    };

    const ACCENT_COLORS_MAP: Record<string, string> = {
      red: '#be1c1c',
      blue: '#4a9eff',
      purple: '#9c27b0',
      green: '#4CAF50',
      orange: '#ff9800',
      pink: '#FF96BC'
    };

    const applyTheme = (themeStr: string, accentStr: string) => {
      const hexAccent = ACCENT_COLORS_MAP[accentStr] || accentStr || '#4a9eff';
      document.documentElement.setAttribute('data-theme', themeStr);
      document.documentElement.style.setProperty('--accent-primary', hexAccent);
      document.documentElement.style.setProperty('--accent-secondary', hexAccent);

      const palette = AURORA_PALETTES[hexAccent] || ['#4a9eff', '#a855f7', '#ff6b9d', '#38bdf8'];
      document.documentElement.style.setProperty('--aurora-color-1', palette[0]);
      document.documentElement.style.setProperty('--aurora-color-2', palette[1]);
      document.documentElement.style.setProperty('--aurora-color-3', palette[2]);
      document.documentElement.style.setProperty('--aurora-color-4', palette[3]);
    };

    // Load initial settings from backend
    invoke('get_app_settings')
      .then((settings: any) => {
        applyTheme(settings.theme, settings.accentColor);
      })
      .catch(console.error);

    // Listen for setting changes
    let unlisten: (() => void) | null = null;
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('settings_changed', (event: any) => {
        const settings = event.payload;
        if (settings) {
          if (document.startViewTransition) {
            const x = window.innerWidth / 2;
            const y = window.innerHeight / 2;
            const maxRadius = Math.hypot(x, y);

            document.documentElement.style.setProperty('--theme-toggle-x', `${x}px`);
            document.documentElement.style.setProperty('--theme-toggle-y', `${y}px`);
            document.documentElement.style.setProperty('--theme-toggle-radius', `${maxRadius}px`);
            document.documentElement.style.setProperty('--theme-toggle-duration', `400ms`);

            document.startViewTransition(() => {
              applyTheme(settings.theme, settings.accentColor);
            });
          } else {
            applyTheme(settings.theme, settings.accentColor);
          }
        }
      }).then(fn => { unlisten = fn; });
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // System Setup
  useEffect(() => {
    const initialize = async () => {
      try {
        const rxModsPath = await invoke<string | null>("get_game_path");
        if (rxModsPath) {
          const lastSlash = Math.max(rxModsPath.lastIndexOf("/"), rxModsPath.lastIndexOf("\\"));
          const paksPath = lastSlash > 0 ? rxModsPath.substring(0, lastSlash) : rxModsPath;
          setGamePaksPath(paksPath);
        }

        const vfxSettings = await invoke<{ usmapPath?: string | null }>("vfx_get_settings");
        if (vfxSettings?.usmapPath) setUsmapPath(vfxSettings.usmapPath);

        const loadedFolders = await invoke<FolderRecord[]>("get_folders");
        if (loadedFolders) setFolders(loadedFolders);
      } catch (e) {
        console.error("[VFX] Failed to load settings:", e);
      }
    };

    initialize();
  }, []);

  const addLog = useCallback((message: string, type: LogEntry["type"] = "info") => {
    const time = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev, { message, type, time }]);
  }, []);

  const [isFetchingUsmap, setIsFetchingUsmap] = useState(false);
  const didRunInitialUsmapCheckRef = useRef(false);

  // Run the rivals-depot USMAP check. When `force` is true, the user clicked
  // the Fetch button — show extra UI feedback and ALWAYS apply the latest
  // (even over a custom user selection).
  const runUsmapCheck = useCallback(async (force: boolean) => {
    if (force) {
      setIsFetchingUsmap(true);
      addLog(`Fetching latest USMAP from rivals-depot...`, "info");
    }
    try {
      const result = await invoke<{
        updated: boolean;
        upToDate: boolean;
        skipped: boolean;
        localPath?: string | null;
        filename?: string | null;
        version?: string | null;
        buildNumber?: number | null;
        commitDate?: string | null;
        message: string;
      }>("vfx_check_usmap_update", { force });

      // `skipped` means the backend downloaded the latest but deliberately
      // kept a custom user selection — that only happens on auto-checks, so
      // never override the path in that case.
      if (result.updated && result.localPath && !result.skipped) {
        setUsmapPath(result.localPath);
        addLog(`USMAP ${force ? "fetched" : "auto-updated"}: ${result.filename ?? "(latest)"}`, "success");
      } else if (result.updated && result.skipped) {
        addLog(result.message, "warning");
      } else if (result.upToDate) {
        const details = [
          result.version,
          result.buildNumber != null ? `${result.buildNumber}` : null,
          result.commitDate ? `${result.commitDate}` : null,
        ].filter(Boolean).join(" | ");
        addLog(`USMAP up to date${details ? ` (${details})` : ""}`, force ? "success" : "info");
        if (result.localPath && !result.skipped && (force || !usmapPath)) {
          setUsmapPath(result.localPath);
        }
      } else if (result.message) {
        addLog(result.message, "warning");
      }
      return result;
    } catch (e) {
      console.warn("[VFX] USMAP check failed:", e);
      addLog(`USMAP ${force ? "fetch" : "auto-check"} failed: ${String(e)}`, "warning");
      return null;
    } finally {
      if (force) setIsFetchingUsmap(false);
    }
  }, [addLog, usmapPath]);

  // Auto-pull latest USMAP from rivals-depot on mount.
  // This runs in the background — uses GitHub ETag so the typical case is a
  // cheap 304 Not Modified.
  useEffect(() => {
    if (didRunInitialUsmapCheckRef.current) return;
    didRunInitialUsmapCheckRef.current = true;

    let cancelled = false;
    (async () => {
      const _ = await runUsmapCheck(false);
      if (cancelled) return;
    })();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Make logs auto-scroll
  useEffect(() => {
    if (logsEndRef.current) logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  const { currentStep, stepStatus, isProcessing, runPipeline, cancelPipeline } = usePipeline({
    usmapPath,
    gamePaksPath,
    modPath,
    outputPath, // Forces save to gamePaksPath/~mods/ 
    addLog,
  });

  const handleFilePick = async (setter: (path: string | null) => void, filters?: { name: string; extensions: string[] }[], saveAsUsmap = false) => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, filters });
      if (selected && typeof selected === "string") {
        setter(selected);
        if (saveAsUsmap) {
          try {
            // Merge into the existing settings — posting only `usmapPath`
            // would reset the sha/filename/etag the auto-update relies on.
            const current = await invoke<VfxSettings>("vfx_get_settings").catch(() => ({} as VfxSettings));
            await invoke("vfx_save_settings", { settings: { ...current, usmapPath: selected } });
          } catch (e) { }
        }
      }
    } catch (e) {
      console.error("[VFX] File picker error:", e);
    }
  };

  const handleStart = async () => {
    setViewMode('progress');
    await runPipeline();
  };

  const emitModsRefresh = async () => {
    try {
      const { emit } = await import('@tauri-apps/api/event')
      await emit('mods_dir_changed')
    } catch (e) {
      console.error('[VFX] Failed to emit mods refresh event:', e)
    }
  }

  const handleCancelProcess = async () => {
    await cancelPipeline();
    setViewMode('input');
  }

  // Effect to catch completion (Step 9 means success!)
  useEffect(() => {
    if (viewMode === 'progress' && !isProcessing && currentStep === 9) {
      setViewMode('complete');
      void emitModsRefresh()
    }
  }, [isProcessing, currentStep, viewMode]);

  // Handle post install logic (Move mod into selected Folder Tree)
  const handleSaveOutput = async () => {
    if (!modPath || !gamePaksPath) return;

    const modBaseName = modPath
      .split(/[\\/]/)
      .pop()!
      .replace(".utoc", "")
      .replace(/_\d+_P$/, "")
      .replace(/_P$/, "");

    const pakPath = `${gamePaksPath}/~mods/${modBaseName}_UPDATED_9999999_P.pak`;

    if (selectedFolderId) {
      const selectedFolder = folders.find((f: FolderRecord) => f.id === selectedFolderId);
      const effectiveFolderId = selectedFolder?.is_root ? null : selectedFolderId;

      if (effectiveFolderId) {
        try {
          await invoke('assign_mod_to_folder', {
            modPath: pakPath,
            folderId: effectiveFolderId,
          });
          addLog(`Mod moved to ${selectedFolderId} successfully!`, "success");
        } catch (e) {
          console.error("Error moving:", e);
          addLog(`Failed to move mod to folder: ${e}`, "error");
        }
      }
    }

    await emitModsRefresh()

    setViewMode('input');
    setModPath(null);
  };

  // Tree variables
  const rootFolder = useMemo(() => folders.find((f: FolderRecord) => f.is_root), [folders]);
  const subfolders = useMemo(() => folders.filter(f => !f.is_root), [folders]);
  const treeData = useMemo(() => {
    const root = buildTree(subfolders);
    return convertToArray(root);
  }, [subfolders]);

  useEffect(() => {
    if (viewMode === 'complete' && rootFolder?.id) {
      setSelectedFolderId(rootFolder.id)
    }
  }, [viewMode, rootFolder?.id])

  const modDisplayName = useMemo(() => {
    if (!modPath) return null;
    const fileName = modPath.split(/[\\/]/).pop() || "";
    return fileName
      .replace(/\.[^/.]+$/, "")
      .replace(/_\d+_P$/i, "")
      .replace(/_P$/i, "");
  }, [modPath]);
  const progressTitle = modDisplayName ? `Updating ${modDisplayName}` : "Updating mod";

  const TOTAL_STEPS = 8;
  const DIAL_RADIUS = 75;
  const DIAL_CIRCUMFERENCE = 2 * Math.PI * DIAL_RADIUS;
  const clampedStep = Math.max(0, Math.min(currentStep, TOTAL_STEPS));
  const targetProgressRatio = clampedStep / TOTAL_STEPS;
  
  const progressVal = useMotionValue(0);
  
  useEffect(() => {
    // Animate smoothly to the target progress ratio over 2 seconds
    const controls = animate(progressVal, targetProgressRatio, {
      duration: 2,
      ease: "easeOut"
    });
    return controls.stop;
  }, [targetProgressRatio]);

  useEffect(() => {
    console.debug("[VFX][ProgressDial]", {
      currentStep,
      clampedStep,
      targetProgressRatio,
      dialCircumference: DIAL_CIRCUMFERENCE
    });
  }, [currentStep, clampedStep, targetProgressRatio, DIAL_CIRCUMFERENCE]);

  useEffect(() => {
    if (!modPath) return;
    console.debug("[VFX][ProgressTitle]", {
      modPath,
      modDisplayName,
      progressTitle
    });
  }, [modPath, modDisplayName, progressTitle]);

  const animatedDashoffset = useTransform(progressVal, (v) => {
    const clampedRatio = Math.max(0, Math.min(v, 1));
    return DIAL_CIRCUMFERENCE * (1 - clampedRatio);
  });
  const animatedPercentageText = useTransform(progressVal, (v) => {
    const clampedRatio = Math.max(0, Math.min(v, 1));
    return Math.round(clampedRatio * 100);
  });

  const isStartDisabled = !usmapPath || !gamePaksPath || !modPath;

  return (
    <div className="vfx-panel">
      <header className="vfx-header">
        <h1 className="font-logo">
          Repak <AuroraText className="font-logo">VFX</AuroraText>
        </h1>
      </header>

      <AnimatePresence mode="wait">
        {viewMode === 'input' && (
          <motion.div
            key="input"
            className="vfx-input-screen"
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.3 }}
          >
            <div className="vfx-inputs-container">
              <div className="vfx-setting-row">
                <label>USMAP File</label>
                <div className="vfx-input-group">
                  <input type="text" value={usmapPath || ""} readOnly placeholder="Select .usmap file..." />
                  <button
                    onClick={() => { void runUsmapCheck(true); }}
                    disabled={isFetchingUsmap}
                    title="Check rivals-depot for the latest USMAP and replace the current one"
                  >
                    {isFetchingUsmap ? "Fetching..." : "Fetch"}
                  </button>
                  <button onClick={() => handleFilePick(setUsmapPath, [{ name: "USMAP", extensions: ["usmap"] }], true)}>Browse</button>
                </div>
              </div>

              <div className="vfx-setting-row">
                <label>Mod File (.utoc)</label>
                <div className="vfx-input-group">
                  <input type="text" value={modPath || ""} readOnly placeholder="Select mod to update..." />
                  <button onClick={() => handleFilePick(setModPath, [{ name: "IOStore", extensions: ["utoc"] }])}>Browse</button>
                </div>
              </div>
            </div>

            <ShineBorder
              className="vfx-start-wrapper"
              borderRadius={16}
              borderWidth={2}
              shineColor={isStartDisabled ? ['var(--panel-border, #333)', 'transparent'] : ['var(--accent-primary)', 'color-mix(in srgb, var(--accent-primary), transparent 50%)']}
            >
              <button
                className="vfx-start-btn"
                onClick={handleStart}
                disabled={isStartDisabled}
                onMouseMove={(e) => {
                  const rect = e.currentTarget.getBoundingClientRect();
                  e.currentTarget.style.setProperty('--mouse-x', `${e.clientX - rect.left}px`);
                  e.currentTarget.style.setProperty('--mouse-y', `${e.clientY - rect.top}px`);
                }}
              >
                <span>UPDATE MOD</span>
              </button>
            </ShineBorder>
          </motion.div>
        )}

        {viewMode === 'progress' && (
          <motion.div
            key="progress"
            className="vfx-progress-screen"
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.4 }}
          >
            <div className="vfx-progress-top-panel">
              <h3 className="vfx-step-title">{progressTitle}</h3>
              <div className="vfx-segmented-track">
                {PIPELINE_STEPS.map(step => (
                  <div key={step.id} className={`vfx-segment ${clampedStep > step.id ? 'completed' : clampedStep === step.id ? 'active' : ''}`} />
                ))}
              </div>
              <div className="vfx-step-status">{stepStatus?.message || "Running Pipeline..."}</div>
            </div>

            <div className="vfx-dial-panel">
              <div className="vfx-dial-container">
                <svg viewBox="0 0 200 200" className="vfx-dial-svg">
                  <defs>
                    <linearGradient id="vfx-radial-pulse" x1="0%" y1="0%" x2="100%" y2="100%" gradientUnits="userSpaceOnUse">
                      <stop offset="0%" stopColor="var(--accent-primary)" />
                      <stop offset="25%" stopColor="var(--accent-primary)" />
                      <stop offset="50%" stopColor="var(--text-primary)" stopOpacity="1.0" />
                      <stop offset="75%" stopColor="var(--accent-primary)" />
                      <stop offset="100%" stopColor="var(--accent-primary)" />
                      <animateTransform
                        attributeName="gradientTransform"
                        type="rotate"
                        from="0 100 100"
                        to="360 100 100"
                        dur="4s"
                        repeatCount="indefinite"
                      />
                    </linearGradient>
                  </defs>
                  <circle cx="100" cy="100" r="75" className="vfx-dial-bg" />
                  <motion.circle
                    cx="100" cy="100" r="75"
                    className="vfx-dial-progress"
                    style={{
                      stroke: "url(#vfx-radial-pulse)",
                      strokeDasharray: `${DIAL_CIRCUMFERENCE}`,
                      strokeDashoffset: animatedDashoffset
                    }}
                  />
                </svg>
                <div className="vfx-dial-text">
                  <span className="vfx-dial-percent"><motion.span>{animatedPercentageText}</motion.span>%</span>
                  <span className="vfx-dial-fraction">{clampedStep} / {TOTAL_STEPS}</span>
                </div>
              </div>
            </div>

            <button className="vfx-cancel-btn" onClick={handleCancelProcess}>
              <VscClose size={18} />
              <span>Cancel Process</span>
            </button>
          </motion.div>
        )}

        {viewMode === 'complete' && (
          <motion.div
            key="complete"
            className="vfx-complete-screen"
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.4 }}
          >
            <div className="vfx-complete-header">
              <MdCheckCircle />
              <h2>Update Completed Successfully!</h2>
            </div>
            <div className="vfx-complete-content">
              <p>Select an output destination for the updated mod:</p>
              <div className="vfx-folder-container">
                {rootFolder && (
                  <div
                    className={`ext-folder-item root-item ${selectedFolderId === rootFolder.id ? 'selected' : ''}`}
                    onClick={() => setSelectedFolderId(rootFolder.id)}
                  >
                    <span className="folder-icon"><VscFolderOpened /></span>
                    <span className="folder-name">{rootFolder.name}</span>
                  </div>
                )}
                <div className="ext-folder-tree">
                  {treeData.map(node => (
                    <FolderNode
                      key={node.fullPath || node.id}
                      node={node}
                      selectedFolderId={selectedFolderId}
                      onSelect={setSelectedFolderId}
                    />
                  ))}
                </div>
              </div>
            </div>
            <div className="vfx-complete-footer">
              <button className="btn-secondary" onClick={() => { void emitModsRefresh(); setViewMode('input'); }}>Leave in ~mods</button>
              <button className="btn-save" onClick={handleSaveOutput}>Save to Selected</button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="vfx-logs">
        {logs.length > 0 && (
          <button 
            className="vfx-logs-copy-btn" 
            onClick={handleCopyLogs}
            title="Copy all logs"
          >
            {copyFeedback ? <span style={{ fontSize: '10px', marginRight: '4px' }}>Copied!</span> : null}
            <BiCopyAlt size={16} />
          </button>
        )}
        <div className="vfx-logs-content">
          {logs.length === 0 && <span style={{ opacity: 0.5 }}>Awaiting pipeline execution...</span>}
          {logs.map((log, i) => (
            <div key={i} className={`vfx-log-entry log-${log.type}`}>
              <span className="log-time">[{log.time}]</span>
              <span className="log-message">{log.message}</span>
            </div>
          ))}
          <div ref={logsEndRef} />
        </div>
      </div>
    </div>
  );
}
