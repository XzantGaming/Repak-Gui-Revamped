import {
  Check as CheckIcon,
  Clear as ClearIcon,
  CreateNewFolder as CreateNewFolderIcon,
  GridView as GridViewIcon,
  PlayArrow as PlayArrowIcon,
  Refresh as RefreshIcon,
  Search as SearchIcon,
  ToggleOff as ToggleOffIcon,
  ToggleOn as ToggleOnIcon,
  ViewHeadline as ViewHeadlineIcon,
  ViewList as ViewListIcon,
  ViewModule as ViewModuleIcon,
  ViewSidebar as ViewSidebarIcon
} from '@mui/icons-material'
import { IconButton } from '@mui/material'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { AnimatePresence, motion } from 'framer-motion'
import type { ChangeEvent } from 'react'
import { useEffect, useRef, useState } from 'react'
import { FaSort, FaTag, FaToolbox } from "react-icons/fa6"
import { GiLightningTrio } from "react-icons/gi"
import { IoIosSettings, IoMdWarning, IoMdWifi } from "react-icons/io"
import { MdDriveFileMoveOutline, MdLocalOffer } from "react-icons/md"
import { RiDeleteBin2Fill } from 'react-icons/ri'
import { useDebouncedCallback } from 'use-debounce'
import './App.css'
import AddModSplitButton from './components/AddModSplitButton'
import { AlertProvider, useAlert } from './components/AlertHandler'
import ChangelogModal from './components/ChangelogModal'
import ContextMenu from './components/ContextMenu'
import CustomDropdown from './components/CustomDropdown'
import DropZoneOverlay from './components/DropZoneOverlay'
import ExtensionModOverlay from './components/ExtensionModOverlay'
import FileTree from './components/FileTree'
import FolderTree from './components/FolderTree'
import HeroFilterDropdown from './components/HeroFilterDropdown'
import InputPromptModal from './components/InputPromptModal'
import LogDrawer from './components/LogDrawer'
import ModDetailsPanel from './components/ModDetailsPanel'
import ModsList from './components/ModsList'
import OnboardingTour from './components/OnboardingTour'
import PromiseTransitionLoader from './components/PromiseTransitionLoader'
import QuickOrganizeOverlay from './components/QuickOrganizeOverlay'
import ShortcutsHelpModal from './components/ShortcutsHelpModal'
import { AuroraText } from './components/ui/AuroraText'
import ModularLogo from './components/ui/ModularLogo'
import UpdateAppModal from './components/UpdateAppModal'
import UpdateModModal from './components/UpdateModModal'
import characterDataStatic from './data/character_data.json'
import { useAprilFools } from './hooks/useAprilFools'
import { useGlobalTooltips } from './hooks/useGlobalTooltips'
import './styles/Badges.css'
import './styles/Fonts.css'
import './styles/GlobalTooltips.css'
import './styles/theme.css'

// Utility functions
import { normalizeModBaseName } from './utils/format'
import { detectHeroes } from './utils/heroes'
import { getAdditionalCategories } from './utils/mods'
import { toTagArray } from './utils/tags'

const ACCENT_COLORS_MAP: Record<string, string> = {
  red: '#be1c1c',
  blue: '#4a9eff',
  purple: '#9c27b0',
  green: '#4CAF50',
  orange: '#ff9800',
  pink: '#FF96BC'
};

const AURORA_PALETTES: Record<string, string[]> = {
  '#be1c1c': ['#be1c1c', '#ff9800', '#ffcc00', '#ff6b35'], // Repak Red: warm fire tones
  '#4a9eff': ['#4a9eff', '#a855f7', '#ff6b9d', '#38bdf8'], // Blue: cool to pink
  '#9c27b0': ['#9c27b0', '#e91e63', '#00bcd4', '#7c3aed'], // Purple: vibrant mix
  '#4CAF50': ['#4CAF50', '#8bc34a', '#00e676', '#e91e63'], // Green: nature with pop
  '#ff9800': ['#ff9800', '#ff5722', '#ffc107', '#4a9eff'], // Orange: sunset vibes
  '#FF96BC': ['#FF96BC', '#f472b6', '#c084fc', '#fda4af'], // Pink: soft pastel tones
};

const VFX_UPDATER_MOD_PREFILL_KEY = 'repakx:vfxUpdater:modPath'

import TitleBar from './components/TitleBar'

import ClashPanel from './components/ClashPanel'
import CreditsPanel from './components/CreditsPanel'
import InstallModPanel from './components/InstallModPanel'
import SettingsPanel from './components/SettingsPanel'
import SharingPanel from './components/SharingPanel'
import ToolsPanel from './components/ToolsPanel'

type ModRecord = {
  path: string
  utoc_path?: string
  mod_name?: string
  custom_name?: string
  customName?: string
  custom_tags?: string[]
  folder_id?: string | null
  enabled?: boolean
  priority?: number
  mod_type?: string
  modified_date?: number
  [key: string]: any
}

type FolderRecord = {
  id: string
  name: string
  depth?: number
  is_root?: boolean
  [key: string]: any
}

type ClashRecord = {
  file_path: string
  mod_paths: string[]
}

type PanelState = {
  settings: boolean
  tools: boolean
  sharing: boolean
  credits: boolean
  install: boolean
  clash: boolean
  shortcuts: boolean
}

type UpdateInfo = {
  latest?: string
  url?: string
  asset_url?: string
  asset_name?: string
  changelog?: string
  [key: string]: any
}

type UpdateDownloadProgress = {
  percentage?: number
  status?: string
  [key: string]: any
}

type ContextMenuState = {
  x: number
  y: number
  mod?: ModRecord | null
  folder?: FolderRecord | null
}

type NewFolderPromptState = { paths: string[]; moveFolderId?: string }
type NewTagPromptState = { callback: (tag: string) => void }
type NewFolderFromInstallState = { callback: (name: string) => void }
type RenameFolderPromptState = { folderId: string; currentName: string }

type UpdateModState = {
  isOpen: boolean
  mod: ModRecord | null
  newSourcePath: string | null
  obfuscatePreference: boolean | null
}

type CharacterDataEntry = {
  name: string
  id: string
  skinid?: string
  skin_name?: string
  [key: string]: any
}

type ViewMode = 'grid' | 'compact' | 'list' | 'list-compact'

type DroppedModParse = {
  is_dir?: boolean
  contains_uassets?: boolean
  contains_raw_assets?: boolean
  [key: string]: any
}

type SingleModConflict = {
  conflicting_mod_path: string
  overlapping_files: string[]
  [key: string]: any
}

type InstallModPayload = ModRecord & {
  customName?: string
  selectedTags?: string[]
  hybrid?: boolean
}

type AppSettings = {
  theme: string
  accentColor: string
  viewMode: string
  hideSuffix: boolean
  autoOpenDetails: boolean
  showHeroIcons: boolean
  showHeroBg: boolean
  showModType: boolean
  showExperimental: boolean
  enableDrp: boolean
  parallelProcessing: boolean
  autoCheckUpdates: boolean
  holdToDelete: boolean
  showSubfolderMods: boolean
  bypassGameRunningLock: boolean
  launcherType: 'steam' | 'epic'
}

function App() {
  const isAprilFools = useAprilFools();
  const [hideSuffix, setHideSuffix] = useState(false);
  const [autoOpenDetails, setAutoOpenDetails] = useState(false);
  const [showHeroIcons, setShowHeroIcons] = useState(false);
  const [showHeroBg, setShowHeroBg] = useState(false);
  const [showModType, setShowModType] = useState(false);
  const [showExperimental, setShowExperimental] = useState(false);
  const [showSubfolderMods, setShowSubfolderMods] = useState(true);
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(true);
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);
  const [enableDrp, setEnableDrp] = useState(false);
  const [parallelProcessing, setParallelProcessing] = useState(false);
  const [holdToDelete, setHoldToDelete] = useState(true);
  const [bypassGameRunningLock, setBypassGameRunningLock] = useState(false);
  const [launcherType, setLauncherType] = useState<'steam' | 'epic'>('steam');
  const [theme, setTheme] = useState('dark');
  const [accentColor, setAccentColor] = useState('#4a9eff');
  const [sortBy, setSortBy] = useState<'name' | 'modified'>('name');
  const [sortDirection, setSortDirection] = useState<'asc' | 'desc'>('asc');

  // Update system state
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null); // { latest, url, asset_url, asset_name }
  const [updateDownloadProgress, setUpdateDownloadProgress] = useState<UpdateDownloadProgress | null>(null); // { percentage, status }
  const [downloadedUpdatePath, setDownloadedUpdatePath] = useState<string | null>(null);
  const [showUpdateModal, setShowUpdateModal] = useState(false);
  const [showChangelogModal, setShowChangelogModal] = useState(false);
  const [changelogContent, setChangelogContent] = useState('');

  // Panel visibility state - grouped for cleaner management
  const [panels, setPanels] = useState<PanelState>({
    settings: false,
    tools: false,
    sharing: false,
    credits: false,
    install: false,
    clash: false,
    shortcuts: false
  });
  
  const panelsRef = useRef<PanelState>(panels);
  useEffect(() => {
    panelsRef.current = panels;
  }, [panels]);

  // Helper to open/close a specific panel
  const setPanel = (panelName: keyof PanelState, isOpen: boolean) => {
    if (panelName === 'clash' && !isOpen) {
      clashScopeModPath.current = null
    }
    setPanels(prev => ({ ...prev, [panelName]: isOpen }));
  };

  const [gamePath, setGamePath] = useState('')
  const [mods, setMods] = useState<ModRecord[]>([])
  const [folders, setFolders] = useState<FolderRecord[]>([])
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('')
  const [gameRunning, setGameRunning] = useState(false)
  const [version, setVersion] = useState('')
  const [selectedMod, setSelectedMod] = useState<ModRecord | null>(null)
  const [leftPanelWidth, setLeftPanelWidth] = useState(100) // percentage
  const [lastPanelWidth, setLastPanelWidth] = useState(70) // to restore after collapse (default 30% right panel)
  const [isRightPanelOpen, setIsRightPanelOpen] = useState(false)
  const [isResizing, setIsResizing] = useState(false)
  const [filtersHeight, setFiltersHeight] = useState<number | null>(null)
  const [isFiltersResizing, setIsFiltersResizing] = useState(false)
  const [selectedMods, setSelectedMods] = useState<Set<string>>(new Set())
  const [showBulkActions, setShowBulkActions] = useState(false)
  const [newTagInput, setNewTagInput] = useState('')
  const [allTags, setAllTags] = useState<string[]>([])
  const [filterTag, setFilterTag] = useState('')
  const [filterType, setFilterType] = useState('')
  const [modDetails, setModDetails] = useState<Record<string, any>>({}) // { [path]: ModDetails }
  const [detailsLoading, setDetailsLoading] = useState(false)
  const [selectedCharacters, setSelectedCharacters] = useState<Set<string>>(new Set()) // values: character_name, '__generic', '__multi'
  const [selectedCategories, setSelectedCategories] = useState<Set<string>>(new Set()) // category strings
  const [availableCharacters, setAvailableCharacters] = useState<any[]>([])
  const [availableCategories, setAvailableCategories] = useState<string[]>([])
  const [showCharacterFilters, setShowCharacterFilters] = useState(false)
  const [showTypeFilters, setShowTypeFilters] = useState(false)

  // Search state with debounce
  const [searchQuery, setSearchQuery] = useState('') // Actual filter query (debounced)
  const [localSearch, setLocalSearch] = useState('') // Input value (immediate)

  const debouncedSetSearch = useDebouncedCallback((value: string) => {
    setSearchQuery(value)
  }, 300)

  const handleSearchChange = (e: ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value
    setLocalSearch(value)
    debouncedSetSearch(value)
  }

  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set())
  const [modsToInstall, setModsToInstall] = useState<any[]>([])
  const [installLogs, setInstallLogs] = useState<string[]>([])
  const [modLoadingProgress, setModLoadingProgress] = useState(0) // 0-100 for progress, -1 for indeterminate
  const [isModsLoading, setIsModsLoading] = useState(false) // Track if mods are being loaded
  const [selectedFolderId, setSelectedFolderId] = useState('all')
  const [viewMode, setViewMode] = useState<ViewMode>('list') // 'grid', 'compact', 'list'
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null) // { x, y, mod }
  const [isLogDrawerOpen, setIsLogDrawerOpen] = useState(false)

  const [clashes, setClashes] = useState<ClashRecord[]>([])
  const [launchSuccess, setLaunchSuccess] = useState(false)
  const [characterData, setCharacterData] = useState<CharacterDataEntry[]>(characterDataStatic as CharacterDataEntry[])
  const [isDragging, setIsDragging] = useState(false)
  const [dropTargetFolder, setDropTargetFolder] = useState<string | null>(null)
  const [renamingModPath, setRenamingModPath] = useState<string | null>(null) // Track which mod should start inline renaming
  const [extensionModPath, setExtensionModPath] = useState<string | null>(null) // Path of mod received from browser extension
  const [showOnboarding, setShowOnboarding] = useState(false)
  const [quickOrganizePaths, setQuickOrganizePaths] = useState<string[] | null>(null) // Paths of PAKs to quick-organize (no uassets)
  const [newFolderPrompt, setNewFolderPrompt] = useState<NewFolderPromptState | null>(null) // {paths: []} when prompting for new folder name
  const [newTagPrompt, setNewTagPrompt] = useState<NewTagPromptState | null>(null) // { callback: (tag) => void } when prompting for new tag name
  const [newFolderFromInstall, setNewFolderFromInstall] = useState<NewFolderFromInstallState | null>(null) // { callback: (name) => void } when prompting for new folder from install panel
  const [renameFolderPrompt, setRenameFolderPrompt] = useState<RenameFolderPromptState | null>(null) // { folderId, currentName } when prompting for folder rename
  const [deleteTagConfirm, setDeleteTagConfirm] = useState<{ tag: string; modCount: number } | null>(null)

  // Update Mod State
  const [updateModState, setUpdateModState] = useState<UpdateModState>({
    isOpen: false,
    mod: null,
    newSourcePath: null,
    obfuscatePreference: null
  })
  const [promiseLoaderCount, setPromiseLoaderCount] = useState(0)
  const [promiseLoaderMessage, setPromiseLoaderMessage] = useState('Working...')

  const dropTargetFolderRef = useRef<string | null>(null)
  const searchInputRef = useRef<HTMLInputElement | null>(null)
  const modsGridRef = useRef<HTMLDivElement | null>(null)
  const gameRunningRef = useRef(false)
  const lastSelectedModIndex = useRef<number | null>(null) // For Shift+click range selection
  const filteredModsRef = useRef<ModRecord[]>([]) // Keep in sync with filteredMods for selection handler
  const clashScopeModPath = useRef<string | null>(null) // null = global clashes, path = single-mod scope
  const safeRefreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const filtersResizeRef = useRef({ startY: 0, startHeight: 0 })

  const showPromiseTransitionLoader = (message: string) => {
    console.debug('[PromiseTransitionLoader] show', { message })
    setPromiseLoaderMessage(message)
    setPromiseLoaderCount(prev => prev + 1)
  }

  const hidePromiseTransitionLoader = () => {
    setPromiseLoaderCount(prev => {
      const next = Math.max(0, prev - 1)
      console.debug('[PromiseTransitionLoader] hide', { previous: prev, next })
      return next
    })
  }

  const withPromiseTransitionLoader = async <T,>(message: string, work: () => Promise<T>): Promise<T> => {
    showPromiseTransitionLoader(message)
    try {
      return await work()
    } finally {
      hidePromiseTransitionLoader()
    }
  }

  const scheduleSafeModsRefresh = (reason: string, delayMs = 450) => {
    if (safeRefreshTimerRef.current) {
      clearTimeout(safeRefreshTimerRef.current)
    }

    safeRefreshTimerRef.current = setTimeout(async () => {
      console.debug('[SafeModsRefresh] Running delayed refresh', { reason, delayMs })
      try {
        const refreshedMods = await loadMods()
        console.debug('[SafeModsRefresh] Delayed refresh complete', {
          reason,
          refreshedModsCount: refreshedMods.length
        })
      } catch (error) {
        console.error('[SafeModsRefresh] Delayed refresh failed', { reason, error })
      } finally {
        safeRefreshTimerRef.current = null
      }
    }, delayMs)
  }

  useEffect(() => {
    return () => {
      if (safeRefreshTimerRef.current) {
        clearTimeout(safeRefreshTimerRef.current)
        safeRefreshTimerRef.current = null
      }
    }
  }, [])

  // Bulk delete state
  const [isDeletingBulk, setIsDeletingBulk] = useState(false)
  const deleteBulkTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)


  const alert = useAlert();

  const openVfxUpdaterWindow = async (modUtocPath?: string | null) => {
    if (modUtocPath) {
      localStorage.setItem(VFX_UPDATER_MOD_PREFILL_KEY, modUtocPath)
    }

    try {
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
      const existing = await WebviewWindow.getByLabel('vfx-updater')
      if (existing) {
        await existing.setFocus()
        return
      }

      const vfxWindow = new WebviewWindow('vfx-updater', {
        url: '/vfx-updater',
        title: 'Repak VFX Updater',
        width: 650,
        height: 750,
        center: true,
        resizable: false,
        decorations: false,
      })

      vfxWindow.once('tauri://error', (e) => {
        console.error('[VFX] Failed to create window:', e)
      })
    } catch (e) {
      console.error('[VFX] Window error:', e)
    }
  }

  const handleSendToVfxUpdater = async (mod: ModRecord | null) => {
    if (!mod) return

    const utocPath = typeof mod.utoc_path === 'string' && mod.utoc_path.length > 0
      ? mod.utoc_path
      : null

    if (!utocPath) {
      alert.warning('No UTOC Metadata', 'Selected mod does not include backend .utoc metadata for VFX Updater.')
      return
    }

    await openVfxUpdaterWindow(utocPath)
  }

  // Global tooltips - replaces native browser tooltips with styled ones
  useGlobalTooltips();

  // Unified file drop handler function
  const handleFileDrop = async (paths: string[]) => {
    if (!paths || paths.length === 0) return
    console.log('Dropped items:', paths)

    // Check if we should quick-organize to a folder (using ref for current value in closure)
    const targetFolder = dropTargetFolderRef.current
    if (targetFolder) {
      // Special case: user dropped on "New Folder" target
      if (targetFolder === '__NEW_FOLDER__') {
        // Show the custom folder name prompt modal
        setNewFolderPrompt({ paths })
        setDropTargetFolder(null)
        return
      }

      // Check if any dropped items are folders with uassets that need proper processing
      try {
        const modsData = await invoke('parse_dropped_files', { paths }) as DroppedModParse[]
        const hasFolderWithUassets = modsData.some((mod: DroppedModParse) =>
          mod.is_dir === true && mod.contains_uassets !== false
        )

        if (hasFolderWithUassets) {
          // Cancel quick-organize and show alert
          setDropTargetFolder(null)
          alert.warning(
            'Cannot Quick-Organize Folder Mods',
            'Folder mods with UAssets need to be processed. Please drop them on the Install Mods area.',
            { duration: 8000 }
          )
          return
        }
      } catch (parseError) {
        console.error('Parse error during quick organize check:', parseError)
        // If parsing fails, we still try quick organize (might be simple PAK files)
      }

      // Quick organize: directly install to the folder without showing install panel
      console.log('Quick organizing to folder:', targetFolder)

      const pathCount = paths.length
      const pathsCopy = [...paths]
      const folderName = targetFolder

      setDropTargetFolder(null) // Reset for next drop

      // Start progress bar (indeterminate since quick_organize doesn't report progress)
      setIsModsLoading(true)
      setModLoadingProgress(-1)

      // Use promise toast for loading state and result
      alert.promise(
        (async () => {
          try {
            await invoke('quick_organize', { paths: pathsCopy, targetFolder: folderName })
            await loadMods()
            await loadFolders()
            setStatus(`Installed ${pathCount} item(s) to ${folderName}!`)

            // Show warning after success if game is running
            if (gameRunningRef.current) {
              alert.warning(
                'Game Running',
                'Mods installed, but changes will only take effect after restarting the game.',
                { duration: 8000 }
              )
            }

            return { count: pathCount, folder: folderName }
          } finally {
            setIsModsLoading(false)
            setModLoadingProgress(0)
          }
        })(),
        {
          loading: {
            title: 'Quick Installing',
            description: `Copying ${pathCount} file${pathCount > 1 ? 's' : ''} to "${folderName}"...`
          },
          success: (result) => ({
            title: 'Installation Complete',
            description: `Installed ${result.count} mod${result.count > 1 ? 's' : ''} to "${result.folder}"`
          }),
          error: (err) => ({
            title: 'Installation Failed',
            description: String(err)
          })
        }
      )

      return
    }

    try {
      setStatus('Processing dropped items...')
      const modsData = await invoke('parse_dropped_files', { paths }) as DroppedModParse[]
      if (!modsData || modsData.length === 0) {
        setStatus('No installable mods found in dropped items')
        return
      }
      console.log('Parsed mods:', modsData)

      // Check if ALL mods are PAK files with no uassets - if so, use quick organize
      const allPaksWithNoUassets = modsData.every((mod: DroppedModParse) =>
        mod.is_dir === false && mod.contains_uassets === false
      )

      if (allPaksWithNoUassets && modsData.length > 0) {
        // Skip install panel, show quick organize folder picker
        console.log('All mods are PAKs with no uassets, using quick organize')
        setQuickOrganizePaths(paths)
        return
      }

      // Normal drop: show install panel
      if (panelsRef.current.install) {
        setModsToInstall(prev => [...prev, ...modsData])
      } else {
        setModsToInstall(modsData)
        setPanel('install', true)
      }
    } catch (error) {
      console.error('Parse error:', error)
      setStatus(`Error parsing dropped items: ${error}`)
    }
  }
  const handleMergeHybrid = async (path1: string, path2: string) => {
    try {
      const mergedModsData = await invoke('merge_mods_for_hybrid', { path1, path2 }) as DroppedModParse[]
      if (mergedModsData && mergedModsData.length > 0) {
        setModsToInstall(mergedModsData)
      }
    } catch (error) {
      console.error('Failed to merge mods:', error)
      setStatus(`Error merging hybrid mods: ${error}`)
    }
  }


  const handleCheckClashes = async () => {
    try {
      await withPromiseTransitionLoader('Checking conflicts...', async () => {
        console.debug('[Conflicts] Starting global conflict check')
        setStatus('Checking for clashes...')
        clashScopeModPath.current = null
        const result = await invoke('check_mod_clashes') as any
        setClashes(result)
        setPanel('clash', true)
        setStatus(`Found ${result.length} clashes`)
        console.debug('[Conflicts] Global conflict check completed', { count: Array.isArray(result) ? result.length : 0 })
      })
    } catch (error) {
      setStatus('Error checking clashes: ' + error)
      console.error('[Conflicts] Global conflict check failed:', error)
    }
  }

  const handleCheckForUpdates = async (silent = false) => {
    setIsCheckingUpdates(true);
    try {
      const result = await invoke('check_for_updates') as any;
      if (result) {
        setUpdateInfo(result);
        setShowUpdateModal(true);
        console.debug('[Updates] Update available, opening UpdateAppModal', { latest: result.latest });
      } else if (!silent) {
        alert.success('Up to Date', 'You are running the latest version.');
      }
    } catch (error) {
      console.error('Failed to check for updates:', error);
      if (!silent) {
        alert.error('Update Check Failed', String(error));
      }
    } finally {
      setIsCheckingUpdates(false);
    }
  };

  const handleViewChangelog = async () => {
    try {
      console.debug('[Changelog] Manual changelog open requested from settings');
      const res = await fetch('https://api.github.com/repos/XzantGaming/Repak-X/releases/latest', {
        headers: { 'Accept': 'application/vnd.github.v3+json' }
      });
      console.debug('[Changelog] Latest changelog fetch response', { status: res.status, ok: res.ok });

      if (!res.ok) {
        alert.error('Changelog Unavailable', 'Could not fetch latest changelog right now.');
        return;
      }

      const release = await res.json();
      const releaseBody = String(release?.body || '').trim();

      if (!releaseBody) {
        alert.info('No Changelog', 'No changelog content was found for the latest release.');
        return;
      }

      setChangelogContent(releaseBody);
      setShowChangelogModal(true);
      console.debug('[Changelog] Opened changelog modal from settings', {
        bodyLength: releaseBody.length,
        tag: release?.tag_name
      });
    } catch (error) {
      console.error('[Changelog] Failed to open latest changelog from settings:', error);
      alert.error('Changelog Failed', String(error));
    }
  };

  const handleDownloadUpdate = async () => {
    if (!updateInfo?.asset_url || !updateInfo?.asset_name) {
      // No direct download available, open release page
      if (updateInfo?.url) {
        const { open: openUrl } = await import('@tauri-apps/plugin-shell');
        await openUrl(updateInfo.url);
      }
      return;
    }

    try {
      const path = await invoke('download_update', {
        assetUrl: updateInfo.asset_url,
        assetName: updateInfo.asset_name
      }) as string;
      setDownloadedUpdatePath(path);
    } catch (error) {
      console.error('Download failed:', error);
      alert.error('Download Failed', String(error));
    }
  };

  const handleApplyUpdate = async () => {
    if (!downloadedUpdatePath) return;

    try {
      console.debug('[Updates] Applying update and handing off to backend auto-exit flow', { downloadedUpdatePath });
      await invoke('apply_update', { downloadedPath: downloadedUpdatePath });
    } catch (error) {
      console.error('Apply update failed:', error);
      alert.error('Update Failed', String(error));
    }
  };

  const handleCancelUpdate = async () => {
    try {
      await invoke('cancel_update_download');
    } catch (e) {
      console.warn('Cancel cleanup failed:', e);
    }
    setUpdateInfo(null);
    setUpdateDownloadProgress(null);
    setDownloadedUpdatePath(null);
    setShowUpdateModal(false);
  };

  const handleCheckSingleModClashes = async (mod: ModRecord) => {
    try {
      await withPromiseTransitionLoader('Checking conflicts...', async () => {
        console.debug('[Conflicts] Starting single-mod conflict check', { modPath: mod.path })
        setStatus(`Checking conflicts for ${mod.customName || mod.mod_name || 'mod'}...`)
        clashScopeModPath.current = mod.path
        const conflicts = await invoke('check_single_mod_conflicts', { modPath: mod.path }) as SingleModConflict[]

        // Transform SingleModConflict objects to ModClash format for the ClashPanel
        // Backend SingleModConflict: { conflicting_mod_path, conflicting_mod_name, overlapping_files, ... }
        // Frontend ModClash expected: { file_path, mod_paths: [path1, path2] }

        const fileMap = new Map<string, Set<string>>() // file_path -> Set(mod_paths)

        if (conflicts && conflicts.length > 0) {
          conflicts.forEach((conflict: SingleModConflict) => {
            conflict.overlapping_files.forEach((file: string) => {
              if (!fileMap.has(file)) {
                fileMap.set(file, new Set())
              }
              const fileMods = fileMap.get(file)
              if (!fileMods) return
              // Add both the checked mod and the conflicting mod
              fileMods.add(mod.path)
              fileMods.add(conflict.conflicting_mod_path)
            })
          })
        }

        const transformedClashes: ClashRecord[] = Array.from(fileMap.entries()).map(([file_path, modPathsSet]) => ({
          file_path,
          mod_paths: Array.from(modPathsSet)
        }))

        setClashes(transformedClashes)
        setPanel('clash', true)
        setStatus(`Found ${transformedClashes.length} conflicts for this mod`)
        console.debug('[Conflicts] Single-mod conflict check completed', {
          modPath: mod.path,
          count: transformedClashes.length
        })
      })
    } catch (error) {
      setStatus('Error checking mod conflicts: ' + error)
      console.error('[Conflicts] Single-mod conflict check failed:', error)
    }
  }

  const handleSetPriority = async (modPath: string, priority: number) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot change priority while game is running.'
      )
      return
    }
    try {
      await invoke('set_mod_priority', { modPath, priority })
      setStatus(`Priority set to ${priority}`)

      // If the modified mod is currently selected, clear selection to force refresh of details
      // This ensures the details panel updates with the new filename (since priority changes filename)
      if (selectedMod && selectedMod.path === modPath) {
        setSelectedMod(null)
      }

      await loadMods()

      // Refresh clash list if panel is open
      if (panels.clash) {
        if (clashScopeModPath.current) {
          // Single-mod conflict scope: find the mod after priority rename by matching clean base name
          // Strip priority suffixes to get stable base: remove leading "!", trailing "_P", trailing "_999..."
          const getCleanBase = (filePath: string) => {
            let stem = (filePath.split(/[/\\]/).pop() || '').replace(/\.[^.]+$/, '')
            stem = stem.replace(/^!/, '')
            stem = stem.replace(/_P$/, '')
            stem = stem.replace(/_9+$/, '')
            return stem
          }
          const scopeBase = getCleanBase(clashScopeModPath.current)
          const freshMods = await invoke('get_pak_files') as any
          const scopeMod = freshMods.find((m: ModRecord) => getCleanBase(m.path) === scopeBase)
          if (scopeMod) {
            await handleCheckSingleModClashes(scopeMod)
          }
        } else {
          const result = await invoke('check_mod_clashes') as any
          setClashes(result)
        }
      }
    } catch (error) {
      setStatus('Error setting priority: ' + error)
    }
  }

  const handleSetParallelProcessing = async (enabled: boolean) => {
    try {
      // Optimistically update UI
      setParallelProcessing(enabled);

      // Try to call backend
      await invoke('set_parallel_processing', { enabled });

      setStatus(enabled ? 'Parallel processing set to Boost mode' : 'Parallel processing set to Normal mode');
    } catch (error) {
      console.warn('Backend command for parallel processing failed (expected):', error);
      // We don't revert state here because this is a placeholder feature on frontend
      // and backend might not exist yet. We keep the UI toggle working.
    }
  };

  const handleModSelect = (mod: ModRecord) => {
    setSelectedMod(mod)
    if (autoOpenDetails && !isRightPanelOpen) {
      setLeftPanelWidth(lastPanelWidth > 60 ? lastPanelWidth : 70) // Ensure reasonable width
      setIsRightPanelOpen(true)
    }
  }

  const handleContextMenu = (e: React.MouseEvent, mod: ModRecord) => {
    e.preventDefault()
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      mod
    })
  }

  const handleFolderContextMenu = (e: React.MouseEvent, folder: FolderRecord) => {
    e.preventDefault()
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      folder
    })
  }

  const closeContextMenu = () => {
    setContextMenu(null)
  }

  // Bulk Delete Handlers
  const handleBulkDelete = async () => {
    if (selectedMods.size === 0) return

    try {
      setStatus(`Deleting ${selectedMods.size} mods...`)
      setModLoadingProgress(-1)
      setIsModsLoading(true)

      const modPaths = Array.from(selectedMods)
      let deletedCount = 0
      let errors: string[] = []

      for (const path of modPaths) {
        try {
          // Find mod details to check if it's a folder mod or regular file
          // If we don't have details, try delete_mod anyway
          await invoke('delete_mod', { path: path })
          deletedCount++
        } catch (e) {
          console.error(`Failed to delete ${path}:`, e)
          errors.push(path)
        }
      }

      if (errors.length > 0) {
        alert.warning(
          'Bulk Delete Incomplete',
          `Deleted ${deletedCount} mods. Failed to delete ${errors.length} mods.`
        )
      } else {
        alert.success('Bulk Delete', `Successfully deleted ${deletedCount} mods.`)
      }

      setSelectedMods(new Set()) // Clear selection
      await loadMods()
      await loadFolders()

    } catch (e) {
      console.error('Bulk delete failed:', e)
      setStatus('Bulk delete failed: ' + e)
    } finally {
      setIsModsLoading(false)
      setModLoadingProgress(0)
    }
  }

  const handleBulkDeleteDown = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    if (!holdToDelete) {
      handleBulkDelete()
      return
    }
    setIsDeletingBulk(true)
    deleteBulkTimeoutRef.current = setTimeout(() => {
      handleBulkDelete()
      setIsDeletingBulk(false)
    }, 2000)
  }

  const handleBulkDeleteUp = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDeletingBulk(false)
    if (deleteBulkTimeoutRef.current) clearTimeout(deleteBulkTimeoutRef.current)
  }

  const handleBulkToggle = async (enable: boolean) => {
    if (selectedMods.size === 0 || (gameRunning && !bypassGameRunningLock)) {
      if (gameRunning && !bypassGameRunningLock) alert.warning('Game Running', 'Cannot toggle mods while game is running.')
      return
    }
    const targetMods = mods.filter(m => selectedMods.has(m.path) && m.enabled !== enable)
    if (targetMods.length === 0) return

    try {
      setStatus(`${enable ? 'Enabling' : 'Disabling'} ${targetMods.length} mods...`)
      const oldPaths = new Set(selectedMods)
      const normalizeToggleKey = (path: string) => path.replace(/\.(pak|bak_repak|pak_disabled)$/i, '')
      let count = 0

      console.debug('[BulkToggle] Starting', {
        target: enable ? 'enable' : 'disable',
        selectedCount: selectedMods.size,
        targetCount: targetMods.length
      })

      for (const mod of targetMods) {
        try {
          await invoke('toggle_mod', { modPath: mod.path })
          count++
        } catch (e) {
          console.error(`Failed to toggle ${mod.path}:`, e)
        }
      }

      setStatus(`${enable ? 'Enabled' : 'Disabled'} ${count} mods.`)
      const refreshedMods = await loadMods()

      // Re-map selection to new paths (extension changes on toggle)
      const refreshedByToggleKey = new Map<string, ModRecord>()
      for (const mod of refreshedMods) {
        refreshedByToggleKey.set(normalizeToggleKey(mod.path), mod)
      }

      const newSelected = new Set<string>()
      for (const oldPath of oldPaths) {
        const match = refreshedByToggleKey.get(normalizeToggleKey(oldPath))
        if (match) newSelected.add(match.path)
      }
      setSelectedMods(newSelected)

      console.debug('[BulkToggle] Remapped selection after reload', {
        previousSelectedCount: oldPaths.size,
        newSelectedCount: newSelected.size,
        refreshedModsCount: refreshedMods.length
      })

      if (selectedMod) {
        const updatedSelected = refreshedByToggleKey.get(normalizeToggleKey(selectedMod.path))
        if (updatedSelected) {
          setSelectedMod(updatedSelected)
        }
      }

      scheduleSafeModsRefresh('bulk-toggle-post-reload')
    } catch (e) {
      console.error('Bulk toggle failed:', e)
      setStatus('Bulk toggle failed: ' + e)
    }
  }

  const handleExtractAssets = async (mod: ModRecord) => {
    try {
      const destFolder = await open({
        directory: true,
        multiple: false,
        title: 'Select destination folder for extracted assets'
      })
      if (!destFolder) return

      let modPath = mod.path
      if (mod.is_iostore && mod.utoc_path) {
        modPath = mod.utoc_path
      }

      setStatus('Extracting assets...')
      setModLoadingProgress(-1)
      setIsModsLoading(true)

      const unlistenProgress = await listen('extraction_progress', (event: any) => {
        const p = event.payload
        if (p.status === 'extracting') {
          setStatus(`Extracting assets — ${p.current_file}`)
        }
      })

      try {
        const fileCount = await invoke('extract_mod_assets', {
          modPath,
          destPath: destFolder
        })
        alert.success('Extraction Complete', `Extracted ${fileCount} assets.`)
        setStatus(`Extracted ${fileCount} assets`)

        const modName = (modPath.split(/[/\\]/).pop() || '').replace(/\.[^.]+$/, '')
        await invoke('open_in_explorer', { path: `${destFolder}\\${modName}` })
      } finally {
        unlistenProgress()
        setIsModsLoading(false)
        setModLoadingProgress(0)
      }
    } catch (e) {
      console.error('Failed to extract assets:', e)
      alert.error('Extraction Failed', String(e))
      setStatus('Extraction failed')
      setIsModsLoading(false)
      setModLoadingProgress(0)
    }
  }

  // Update Mod Handlers
  const handleInitiateUpdate = async (mod: ModRecord | null) => {
    if (!mod) return

    try {
      console.debug('[UpdateMod] Opening source picker', { modPath: mod.path })
      const selected = await open({
        multiple: false,
        filters: [{
          name: 'Mod Files',
          extensions: ['pak', 'zip', 'rar', '7z']
        }]
      })

      if (selected) {
        // No persisted global obfuscate preference exists; default to false
        // (same behavior as the Install panel). The modal lets the user override.
        setUpdateModState({
          isOpen: true,
          mod: mod,
          newSourcePath: selected,
          obfuscatePreference: false
        })
        console.debug('[UpdateMod] Update modal opened', { modPath: mod.path, selectedPath: selected })
      }
    } catch (e) {
      console.error('Failed to select update file:', e)
      alert.error('Selection Failed', `Could not open file picker: ${e}`)
    }
  }

  const handleConfirmUpdate = async (preserveName: boolean, obfuscate: boolean) => {
    const { mod, newSourcePath } = updateModState
    if (!mod || !newSourcePath) return

    // Close modal first
    setUpdateModState(prev => ({ ...prev, isOpen: false }))

    try {
      setStatus(`Updating ${mod.custom_name || 'mod'}...`)
      setModLoadingProgress(-1)
      setIsModsLoading(true)

      const result = await invoke('update_mod', {
        oldModPath: mod.path,
        newModSource: newSourcePath,
        preserveName: preserveName,
        obfuscate: obfuscate
      })

      console.log('Update result:', result)

      // Refresh data
      await loadMods()

      // If we're updating the currently selected mod, we might need to update selection if path changed
      if (selectedMod && selectedMod.path === mod.path) {
        // If path changed (preserveName=false), we should try to find the new mod
        // But loadMods is async, so best effort is to deselect or just let user reselect
        if (!preserveName) {
          setSelectedMod(null)
        }
      }

      console.debug('[UpdateMod] Completed update flow; relying on backend named success toast')

    } catch (e) {
      console.error('Update failed:', e)
      alert.error('Update Failed', String(e))
    } finally {
      setIsModsLoading(false)
      setModLoadingProgress(0)
    }
  }

  useEffect(() => {
    loadInitialData().then((modCount) => {
      invoke('get_app_settings')
        .then((settings: any) => {
          if (settings.enableDrp) {
            invoke('discord_connect')
              .then(() => invoke('discord_set_managing_mods', { modCount }))
              .catch(console.warn);
          }
        })
        .catch(console.warn);
    });
    loadTags()

    // Listen for install progress
    const unlisten = listen('install_progress', (event: any) => {
      const progress = Math.round(event.payload)
      setStatus(`Installing... ${progress}%`)
      setModLoadingProgress(progress)
      setIsModsLoading(true)
    })

    const unlistenComplete = listen('install_complete', () => {
      setStatus('Installation complete!')
      setIsModsLoading(false)
      setModLoadingProgress(0)
      loadMods()
    })

    const unlistenLogs = listen('install_log', (event: any) => {
      setInstallLogs((prev) => [...prev, String(event.payload)])
    })

    // Refresh mod list when character data is updated
    const unlistenCharUpdate = listen('character_data_updated', async () => {
      try {
        const data = await invoke('get_character_data') as any
        setCharacterData(data)
      } catch (err) {
        console.error('Failed to refresh character data:', err)
      }
      loadMods()
    })

    // Listen for directory changes (new folders, deleted folders, etc.)
    const unlistenDirChanged = listen('mods_dir_changed', () => {
      console.log('Directory changed, reloading mods and folders...')
      loadMods()
      loadFolders()
    })

    // Listen for mods received from browser extension via repakx:// protocol
    const unlistenExtensionMod = listen('extension-mod-received', (event: any) => {
      const filePath = event.payload
      console.log('Received mod from extension:', filePath)
      setExtensionModPath(filePath)
    })

    // Listen for extension mod errors
    const unlistenExtensionError = listen('extension-mod-error', (event: any) => {
      console.error('Extension mod error:', event.payload)
      alert.error('Extension Error', event.payload)
    })

    // Listen for general toast notifications from Rust backend
    const unlistenToast = listen('toast_notification', (event: any) => {
      const { type, title, description, duration } = event.payload

      // Map Rust type to AlertHandler method
      const showAlertByType = {
        'danger': () => alert.error(title, description, { duration: duration ?? 5000 }),
        'warning': () => alert.warning(title, description, { duration: duration ?? 5000 }),
        'success': () => alert.success(title, description, { duration: duration ?? 5000 }),
        'primary': () => alert.info(title, description, { duration: duration ?? 5000 }),
        'default': () => alert.showAlert({ color: 'default', title, description, duration: duration ?? 5000 })
      }

      const toastType = typeof type === 'string' && type in showAlertByType
        ? (type as keyof typeof showAlertByType)
        : 'default'
      const showFn = showAlertByType[toastType]
      showFn()
    })

    // Listen for game crash notifications
    const unlistenCrash = listen('game_crash_detected', (event: any) => {
      const payload = event.payload

      // Show persistent error toast for crashes
      alert.crash(payload.title, payload.description, {
        action: {
          label: 'Details',
          onClick: () => {
            const report = [
              '--- CRASH REPORT ---',
              `Timestamp: ${new Date().toLocaleString()}`,
              `Type: ${payload.crash_type || 'Unknown'}`,
              `Error: ${payload.error_message || 'N/A'}`,
              `Asset: ${payload.asset_path || 'N/A'}`,
              `Is Mesh Crash: ${payload.is_mesh_crash ? 'Yes' : 'No'}`,
              '-------------------',
              'Full Details for dev debugging:',
              JSON.stringify(payload, null, 2),
              '-------------------'
            ]
            setInstallLogs(report)
            setIsLogDrawerOpen(true)
            alert.info('Crash Details', 'Report opened in Log Drawer.')
          }
        }
      })

      // Log detailed crash info to console for debugging
      console.error('Game Crash Detected:', {
        crashType: payload.crash_type,
        assetPath: payload.asset_path,
        details: payload.details,
        isMeshCrash: payload.is_mesh_crash,
        crashFolder: payload.crash_folder
      })
    })

    // Check for crashes from previous game sessions
    invoke('check_for_previous_crash').catch(err => {
      console.error('Failed to check for previous crashes:', err)
    })

    // Listen for Tauri drag-drop event
    const unlistenDragDrop = listen('tauri://drag-drop', (event: any) => {
      const files = event.payload.paths || event.payload
      setIsDragging(false)
      handleFileDrop(files)
    })

    // Listen for Tauri file-drop event
    const unlistenFileDrop = listen('tauri://file-drop', (event: any) => {
      const files = event.payload.paths || event.payload
      setIsDragging(false)
      handleFileDrop(files)
    })

    // Add dragover event to prevent default browser behavior
    const preventDefault = (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
    }

    document.addEventListener('dragover', preventDefault)
    document.addEventListener('drop', preventDefault)

    return () => {
      // Cleanup listeners
      unlisten.then(f => f())
      unlistenComplete.then(f => f())
      unlistenCharUpdate.then(f => f())
      unlistenDragDrop.then(f => f())
      unlistenFileDrop.then(f => f())
      unlistenLogs.then(f => f())
      unlistenDirChanged.then(f => f())
      unlistenExtensionMod.then(f => f())
      unlistenExtensionError.then(f => f())
      unlistenToast.then(f => f())
      unlistenCrash.then(f => f())
      document.removeEventListener('dragover', preventDefault)
      document.removeEventListener('drop', preventDefault)
    }
  }, [])

  // Listen for update events
  useEffect(() => {
    const unlistenUpdateAvailable = listen('update_available', (event: any) => {
      console.log('Update available:', event.payload);
      setUpdateInfo(event.payload);
      setShowUpdateModal(true);
    });

    const unlistenUpdateProgress = listen('update_download_progress', (event: any) => {
      setUpdateDownloadProgress(event.payload);
    });

    const unlistenUpdateDownloaded = listen('update_downloaded', (event: any) => {
      console.log('Update downloaded:', event.payload);
      setDownloadedUpdatePath(event.payload.path);
    });

    const unlistenUpdateReady = listen('update_ready_to_apply', () => {
      console.debug('[Updates] Backend reported update ready; waiting for auto-exit/app shutdown');
    });

    return () => {
      unlistenUpdateAvailable.then(f => f());
      unlistenUpdateProgress.then(f => f());
      unlistenUpdateDownloaded.then(f => f());
      unlistenUpdateReady.then(f => f());
    };
  }, [])

  // Tauri drag hover detection - use Tauri's events instead of browser events
  useEffect(() => {
    // Listen for Tauri drag-enter event (when files first enter the window)
    const unlistenDragEnter = listen('tauri://drag-enter', () => {
      console.log('Tauri drag-enter detected')
      if (!panelsRef.current.install) {
        setIsDragging(true)
      }
    })

    // Listen for Tauri drag-leave event (when files leave the window)
    const unlistenDragLeave = listen('tauri://drag-leave', () => {
      console.log('Tauri drag-leave detected')
      setIsDragging(false)
    })

    // Also reset on drag-cancelled
    const unlistenDragCancelled = listen('tauri://drag-cancelled', () => {
      console.log('Tauri drag-cancelled detected')
      setIsDragging(false)
    })

    return () => {
      unlistenDragEnter.then(f => f())
      unlistenDragLeave.then(f => f())
      unlistenDragCancelled.then(f => f())
    }
  }, [])

  // Keep the ref in sync with state for access in event listener closures
  useEffect(() => {
    dropTargetFolderRef.current = dropTargetFolder
  }, [dropTargetFolder])

  // Keep gameRunning ref in sync for event listener closures
  useEffect(() => {
    gameRunningRef.current = gameRunning
  }, [gameRunning])

  // Periodically check game running state every 5 seconds
  useEffect(() => {
    const intervalId = setInterval(() => {
      checkGame()
    }, 5000)

    return () => clearInterval(intervalId)
  }, [])

  const loadInitialData = async () => {
    try {
      const path = await invoke('get_game_path') as any
      setGamePath(path)

      const ver = await invoke('get_app_version') as any
      setVersion(ver)

      // Check if we just updated — show changelog if version changed
      const lastSeen = localStorage.getItem('lastSeenVersion')
      const normalizedVersion = String(ver || '').replace(/^v/, '')
      if (lastSeen && lastSeen !== ver) {
        console.debug('[Changelog] Version change detected', { lastSeen, currentVersion: ver, normalizedVersion })
        try {
          const headers = { 'Accept': 'application/vnd.github.v3+json' }
          let releaseBody = ''
          const tagCandidates = [`v${normalizedVersion}`, normalizedVersion]

          for (const tag of tagCandidates) {
            const tagUrl = `https://api.github.com/repos/XzantGaming/Repak-X/releases/tags/${encodeURIComponent(tag)}`
            console.debug('[Changelog] Trying tag endpoint', { tag, tagUrl })
            const tagRes = await fetch(tagUrl, { headers })
            console.debug('[Changelog] Tag endpoint response', { tag, status: tagRes.status, ok: tagRes.ok })
            if (!tagRes.ok) continue

            const tagRelease = await tagRes.json()
            if (tagRelease?.body) {
              releaseBody = String(tagRelease.body)
              console.debug('[Changelog] Changelog found via tag endpoint', { tag })
              break
            }
          }

          // Fallback: releases/latest (helps when the expected tag is not published yet)
          if (!releaseBody) {
            const latestUrl = 'https://api.github.com/repos/XzantGaming/Repak-X/releases/latest'
            console.debug('[Changelog] Falling back to latest release endpoint', { latestUrl })
            const latestRes = await fetch(latestUrl, { headers })
            console.debug('[Changelog] Latest release response', { status: latestRes.status, ok: latestRes.ok })
            if (latestRes.ok) {
              const latestRelease = await latestRes.json()
              const latestTagNormalized = String(latestRelease?.tag_name || '').replace(/^v/, '')
              console.debug('[Changelog] Latest release payload', {
                latestTag: latestRelease?.tag_name,
                latestTagNormalized,
                expectedNormalizedVersion: normalizedVersion
              })

              if (latestTagNormalized === normalizedVersion && latestRelease?.body) {
                releaseBody = String(latestRelease.body)
                console.debug('[Changelog] Changelog found via latest release fallback')
              }
            }
          }

          if (releaseBody) {
            setChangelogContent(releaseBody)
            setShowChangelogModal(true)
            console.debug('[Changelog] Showing changelog modal', {
              version: ver,
              bodyLength: releaseBody.length
            })
          } else {
            console.warn('[Changelog] No changelog body found for updated version', {
              lastSeen,
              currentVersion: ver,
              normalizedVersion,
              tagCandidates
            })
          }
        } catch (err) {
          console.warn('[Changelog] Failed to fetch changelog after update:', err)
        }
      } else {
        console.debug('[Changelog] Skipping changelog check on startup', {
          reason: !lastSeen ? 'first_install_or_no_last_seen' : 'version_unchanged',
          lastSeen,
          currentVersion: ver
        })
      }
      localStorage.setItem('lastSeenVersion', ver)

      // Fetch character data from backend (up-to-date from GitHub sync)
      try {
        const charData = await invoke('get_character_data') as any
        setCharacterData(charData)
      } catch (charErr) {
        console.error('Failed to fetch character data:', charErr)
      }

      const modList = await loadMods()
      await loadFolders()
      await checkGame()

      // Start the file watcher
      await invoke('start_file_watcher')
      return modList.length
    } catch (error) {
      console.error('Failed to load initial data:', error)
      return 0
    }
  }

  const loadMods = async (): Promise<ModRecord[]> => {
    try {
      console.log('Loading mods...')
      setIsModsLoading(true)
      setModLoadingProgress(-1) // Indeterminate while fetching list
      setStatus('Loading mods...')

      const modList = await invoke('get_pak_files') as any
      console.log('Loaded mods:', modList)
      setMods(modList)
      setStatus(`Loading ${modList.length} mod(s) details...`)

      // After loading mods, refresh details for each (with progress tracking)
      await preloadModDetails(modList)

      if (enableDrp) {
        invoke('discord_set_managing_mods', { modCount: modList.length }).catch(console.warn);
      }

      setStatus(`Loaded ${modList.length} mod(s)`)
      return modList as ModRecord[]
    } catch (error) {
      console.error('Error loading mods:', error)
      setStatus('Error loading mods: ' + error)
      return []
    } finally {
      setIsModsLoading(false)
      setModLoadingProgress(0)
    }
  }

  // Preload details for all mods using the new Mod Detection API
  const preloadModDetails = async (modList: ModRecord[]) => {
    if (!Array.isArray(modList) || modList.length === 0) {
      setAvailableCharacters([])
      setAvailableCategories([])
      return
    }

    try {
      setDetailsLoading(true)
      const existing = modDetails
      const pathsToFetch = modList
        .map(m => m.path)
        .filter(p => !existing[p])

      if (pathsToFetch.length === 0) {
        // Already have details; recompute filters source lists
        recomputeFilterSources(modList, modDetails)
        setModLoadingProgress(100)
        return
      }

      // Track progress as details are loaded
      let completedCount = 0
      const totalCount = pathsToFetch.length
      setModLoadingProgress(0)

      const results = await Promise.allSettled(
        pathsToFetch.map(async (p) => {
          const result = await invoke('get_mod_details', { modPath: p })
          completedCount++
          setModLoadingProgress(Math.round((completedCount / totalCount) * 100))
          return result
        })
      )

      const newMap = { ...existing }
      results.forEach((res, idx) => {
        const path = pathsToFetch[idx]
        if (res.status === 'fulfilled' && res.value) {
          newMap[path] = res.value
        }
      })
      setModDetails(newMap)
      recomputeFilterSources(modList, newMap)
    } catch (e) {
      console.error('Failed to preload mod details:', e)
    } finally {
      setDetailsLoading(false)
    }
  }

  const recomputeFilterSources = (modList: ModRecord[], detailsMap: Record<string, any>) => {
    const charSet = new Set<string>()
    let hasMulti = false
    modList.forEach(m => {
      const d = detailsMap[m.path]
      if (!d) return

      // Add single-character mods
      if (d.character_name && !d.character_name.startsWith('Multiple Heroes')) {
        charSet.add(d.character_name)
      }

      // For Multiple Heroes mods, extract individual character names from files
      if (typeof d.mod_type === 'string' && d.mod_type.startsWith('Multiple Heroes')) {
        hasMulti = true
        // Extract individual heroes from the mod's file list
        if (d.files && Array.isArray(d.files)) {
          const heroes = detectHeroes(d.files)
          heroes.forEach(h => charSet.add(h))
        }
      }
    })
    const catSet = new Set<string>()
    modList.forEach(m => {
      const d = detailsMap[m.path]
      if (!d) return
      if (d.category) catSet.add(d.category)
      const adds = getAdditionalCategories(d)
      adds.forEach((cat: string) => catSet.add(cat))
    })
    setAvailableCharacters(Array.from(charSet).sort((a, b) => a.localeCompare(b)))
    setAvailableCategories(Array.from(catSet).sort((a, b) => a.localeCompare(b)))
    // Keep multi-selections if still valid; otherwise prune invalids
    const validChars = new Set<string>(charSet)
    setSelectedCharacters(prev => {
      const next = new Set<string>()
      for (const v of prev) {
        if (v === '__generic' || v === '__multi' || validChars.has(v)) next.add(v)
      }
      return next
    })
    const validCats = new Set<string>(catSet)
    setSelectedCategories(prev => {
      const next = new Set<string>()
      for (const v of prev) {
        if (validCats.has(v)) next.add(v)
      }
      return next
    })
  }

  const loadTags = async () => {
    try {
      const tags = await invoke('get_all_tags') as any
      setAllTags(tags)
    } catch (error) {
      console.error('Error loading tags:', error)
    }
  }

  const loadFolders = async () => {
    try {
      const folderList = await invoke('get_folders') as any
      setFolders(folderList)
    } catch (error) {
      console.error('Failed to load folders:', error)
    }
  }

  const checkGame = async () => {
    try {
      const running = await invoke('check_game_running') as any
      setGameRunning(running)
    } catch (error) {
      console.error('Failed to check game status:', error)
    }
  }

  const handleAutoDetect = async () => {
    try {
      setLoading(true)
      const path = await invoke('auto_detect_game_path') as any
      setGamePath(path)
      setStatus('Game path detected: ' + path)
      await loadMods()
      await loadFolders()
    } catch (error) {
      setStatus('Failed to auto-detect: ' + error)
    } finally {
      setLoading(false)
    }
  }

  const handleBrowseGamePath = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Marvel Rivals Installation Directory'
      })

      if (selected) {
        await invoke('set_game_path', { path: selected })
        setGamePath(selected)
        setStatus('Game path set: ' + selected)
        await loadMods()
        await loadFolders()
      }
    } catch (error) {
      setStatus('Error setting game path: ' + error)
    }
  }

  const handleInstallModClick = async () => {
    try {
      const selected = await open({
        multiple: true,
        filters: [{
          name: 'PAK Files',
          extensions: ['pak']
        }],
        title: 'Select Mods to Install'
      })

      if (selected && selected.length > 0) {
        const paths = Array.isArray(selected) ? selected : [selected]
        const modsData = await invoke('parse_dropped_files', { paths }) as any
        setModsToInstall(modsData)
        setPanel('install', true)
      }
    } catch (error) {
      setStatus('Error selecting mods: ' + error)
    }
  }

  const handleDevInstallPanel = () => {
    const categories = ['Skin', 'Audio', 'UI', 'VFX', 'Mesh', 'Texture']
    const additionalCats = ['Blueprint', 'Text', '']

    const getRandomMod = (i: number) => {
      const randomChar = characterData[Math.floor(Math.random() * characterData.length)].name
      const randomCat = categories[Math.floor(Math.random() * categories.length)]
      const randomAdd = additionalCats[Math.floor(Math.random() * additionalCats.length)]

      let modType = `${randomChar} - ${randomCat}`
      if (randomAdd) {
        modType += ` [${randomAdd}]`
      }

      return {
        path: `C:\\Fake\\Path\\Mod${i}.pak`,
        mod_name: `Mod${i}.pak`,
        file_size: Math.floor(Math.random() * 1024 * 1024 * 50),
        mod_type: modType,
        auto_fix_mesh: Math.random() > 0.5,
        auto_to_repak: Math.random() > 0.5
      }
    }

    setModsToInstall([getRandomMod(1), getRandomMod(2), getRandomMod(3)])
    setPanel('install', true)
  }

  const handleDeleteMod = async (modPath: string) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot delete mods while game is running.'
      )
      return
    }
    // No confirmation prompt needed here, the hold-to-delete button handles the intent

    try {
      // Strip .bak_repak extension to get base path for proper deletion of all associated files
      const basePath = modPath.replace(/\.bak_repak$/i, '.pak')
      await invoke('delete_mod', { path: basePath })
      setStatus('Mod deleted')

      // Clear selection if the deleted mod was selected
      if (selectedMod && selectedMod.path === modPath) {
        setSelectedMod(null)
      }

      await loadMods()
    } catch (error) {
      setStatus('Error deleting mod: ' + error)
    }
  }

  const handleToggleMod = async (modPath: string) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot toggle mods while game is running.'
      )
      return
    }
    try {
      const newState = await invoke('toggle_mod', { modPath })
      setStatus(newState ? 'Mod enabled' : 'Mod disabled')

      // Extract the base name (without extension) to find the mod after toggle
      // The path changes from .pak to .bak_repak or vice versa
      const baseName = modPath.replace(/\.(pak|bak_repak)$/i, '')

      const refreshedMods = await loadMods()

      // Update selectedMod if the toggled mod was selected
      // Find the mod by matching the base path (without extension)
      if (selectedMod && selectedMod.path === modPath) {
        const updatedMod = refreshedMods.find(m =>
          m.path.replace(/\.(pak|bak_repak)$/i, '') === baseName
        )
        if (updatedMod) {
          setSelectedMod(updatedMod)
        }
      }

      scheduleSafeModsRefresh('single-toggle-post-reload')
    } catch (error) {
      setStatus('Error toggling mod: ' + error)
    }
  }

  const handleCreateFolder = (options?: { moveFolderId?: string }) => {
    setNewFolderPrompt({ paths: [], moveFolderId: options?.moveFolderId })
  }

  // Create a folder and return its ID (for use by overlay components)
  const handleCreateFolderAndReturn = async (name: string) => {
    if (!name) throw new Error('Folder name is required')

    try {
      await invoke('create_folder', { name })
      await loadFolders()
      setStatus('Folder created')
      // The folder ID is just the folder name
      return name
    } catch (error) {
      setStatus('Error creating folder: ' + error)
      throw error
    }
  }

  // Handle new folder prompt confirmation (from drop zone or manual creation)
  const handleNewFolderConfirm = async (folderName: string) => {
    if (!newFolderPrompt) return

    const paths = newFolderPrompt.paths || []
    const moveFolderId = newFolderPrompt.moveFolderId
    const pathCount = paths.length
    const pathsCopy = [...paths]

    // Check specific to quick organize flow
    const isQuickOrganize = pathCount > 0
    const isMoveAfterCreate = !!moveFolderId

    if (isMoveAfterCreate && gameRunning && !bypassGameRunningLock) {
      setNewFolderPrompt(null)
      alert.warning(
        'Game Running',
        'Cannot move folders while game is running.'
      )
      return
    }

    setNewFolderPrompt(null) // Close the modal

    // Start progress bar (indeterminate)
    // Only show full loading UI if we are doing a quick organize (installing files)
    // For simple folder creation, we can just use the status bar, or a lighter loader
    if (isQuickOrganize) {
      setIsModsLoading(true)
      setModLoadingProgress(-1)
    }

    // Use promise toast for loading state and result
    alert.promise(
      (async () => {
        try {
          // Create the folder first
          await invoke('create_folder', { name: folderName })
          await loadFolders()

          if (isMoveAfterCreate && moveFolderId) {
            const newId = await invoke('move_folder', { id: moveFolderId, newParentId: folderName }) as string
            if (selectedFolderId === moveFolderId) {
              setSelectedFolderId(newId)
            }
            await loadFolders()
            await loadMods()
            setStatus(`Folder moved to "${newId}"`)

            return { folder: folderName, movedFolderId: moveFolderId, isInstall: false, isMove: true }
          }

          if (isQuickOrganize) {
            // Then quick organize to the new folder
            await invoke('quick_organize', { paths: pathsCopy, targetFolder: folderName })
            await loadMods()
            await loadFolders()
            setStatus(`Installed ${pathCount} item(s) to "${folderName}"!`)

            return { count: pathCount, folder: folderName, isInstall: true, isMove: false }
          } else {
            setStatus(`Folder "${folderName}" created`)
            return { folder: folderName, isInstall: false, isMove: false }
          }
        } finally {
          setIsModsLoading(false)
          setModLoadingProgress(0)
        }
      })(),
      {
        loading: {
          title: isQuickOrganize ? 'Creating Folder & Installing' : 'Creating Folder',
          description: isQuickOrganize
            ? `Creating "${folderName}" and copying ${pathCount} file${pathCount > 1 ? 's' : ''}...`
            : `Creating folder "${folderName}"...`
        },
        success: (result) => ({
          title: result.isInstall
            ? 'Installation Complete'
            : result.isMove
              ? 'Folder Moved'
              : 'Folder Created',
          description: result.isInstall
            ? `Created folder and installed ${result.count} mod${result.count > 1 ? 's' : ''}`
            : result.isMove
              ? `Created "${result.folder}" and moved "${result.movedFolderId}" into it`
              : `Successfully created "${result.folder}"`
        }),
        error: (err) => ({
          title: 'Operation Failed',
          description: String(err)
        })
      }
    )
  }

  const handleDeleteFolder = async (folderId: string) => {
    // No confirmation prompt needed here, the hold-to-delete button handles the intent

    try {
      await invoke('delete_folder', { id: folderId })
      if (selectedFolderId === folderId) {
        setSelectedFolderId('all')
      }

      await loadFolders()
      await loadMods()
      setStatus('Folder deleted')
    } catch (error) {
      setStatus('Error deleting folder: ' + error)
    }
  }

  const handleRenameFolder = (folderId: string, currentName: string) => {
    setRenameFolderPrompt({ folderId, currentName })
  }

  const handleMoveFolder = async (folderId: string, newParentId: string | null) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot move folders while game is running.'
      )
      return
    }
    try {
      const newId = await invoke('move_folder', { id: folderId, newParentId }) as string
      if (selectedFolderId === folderId) {
        setSelectedFolderId(newId)
      }
      await loadFolders()
      await loadMods()
      setStatus(`Folder moved to "${newId}"`)
    } catch (error) {
      setStatus('Error moving folder: ' + error)
    }
  }

  const handleRenameFolderConfirm = async (newName: string) => {
    if (!renameFolderPrompt) return
    const { folderId } = renameFolderPrompt
    setRenameFolderPrompt(null)

    try {
      const newId = await invoke('rename_folder', { id: folderId, newName: newName }) as any
      if (selectedFolderId === folderId) {
        setSelectedFolderId(newId)
      }
      await loadFolders()
      await loadMods()
      setStatus(`Folder renamed to "${newName}"`)
    } catch (error) {
      setStatus('Error renaming folder: ' + error)
    }
  }

  const handleToggleModSelection = (mod: ModRecord, e?: MouseEvent | React.MouseEvent) => {
    const currentList = filteredModsRef.current
    const clickedIndex = currentList.findIndex(m => m.path === mod.path)

    // Shift+Ctrl+click: deselect range from last selected index to clicked index
    if (e?.shiftKey && e?.ctrlKey && lastSelectedModIndex.current !== null && clickedIndex !== -1) {
      const start = Math.min(lastSelectedModIndex.current, clickedIndex)
      const end = Math.max(lastSelectedModIndex.current, clickedIndex)
      const newSelected = new Set(selectedMods)
      for (let i = start; i <= end; i++) {
        newSelected.delete(currentList[i].path)
      }
      setSelectedMods(newSelected)
      return
    }

    // Shift+click: range selection from last selected index to clicked index
    if (e?.shiftKey && lastSelectedModIndex.current !== null && clickedIndex !== -1) {
      const start = Math.min(lastSelectedModIndex.current, clickedIndex)
      const end = Math.max(lastSelectedModIndex.current, clickedIndex)
      const newSelected = new Set(selectedMods)
      for (let i = start; i <= end; i++) {
        newSelected.add(currentList[i].path)
      }
      setSelectedMods(newSelected)
      return
    }

    // Normal toggle (Ctrl+click or checkbox click)
    const newSelected = new Set(selectedMods)
    if (newSelected.has(mod.path)) {
      newSelected.delete(mod.path)
    } else {
      newSelected.add(mod.path)
    }
    setSelectedMods(newSelected)

    // Track last selected index for future Shift+click
    if (clickedIndex !== -1) {
      lastSelectedModIndex.current = clickedIndex
    }
  }

  const handleSelectAll = () => {
    setSelectedMods(new Set(mods.map(m => m.path)))
  }

  const handleDeselectAll = () => {
    setSelectedMods(new Set())
  }

  const handleAssignToFolder = async (folderId: string | null) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot move mods while game is running.'
      )
      return
    }

    if (selectedMods.size === 0) {
      setStatus('No mods selected')
      return
    }

    // Check if folderId corresponds to the root folder (depth 0)
    // If so, pass null to backend to move to root
    const targetFolder = folders.find(f => f.id === folderId)
    const effectiveFolderId = (targetFolder && targetFolder.depth === 0) ? null : folderId

    // Clear the mod details panel to prevent stale reference crashes
    setSelectedMod(null)

    try {
      for (const modPath of selectedMods) {
        await invoke('assign_mod_to_folder', { modPath, folderId: effectiveFolderId })
      }
      setStatus(`Moved ${selectedMods.size} mod(s) to folder!`)
      setSelectedMods(new Set())
      await loadMods()
      await loadFolders()
    } catch (error) {
      setStatus(`Error: ${error}`)
    }
  }

  const handleMoveSingleMod = async (modPath: string, folderId: string | null) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot move mods while game is running.'
      )
      return
    }

    // Check if folderId corresponds to the root folder (depth 0)
    const targetFolder = folders.find(f => f.id === folderId)
    const effectiveFolderId = (targetFolder && targetFolder.depth === 0) ? null : folderId

    // Clear the mod details panel if the moved mod was selected
    if (selectedMod && selectedMod.path === modPath) {
      setSelectedMod(null)
    }

    try {
      await invoke('assign_mod_to_folder', { modPath, folderId: effectiveFolderId })
      setStatus('Mod moved to folder')
      await loadMods()
      await loadFolders()
    } catch (error) {
      setStatus('Error moving mod: ' + error)
    }
  }

  const handleAddTagToSingleMod = async (modPath: string, tag: string) => {
    try {
      await invoke('add_custom_tag', { modPath, tag })
      setStatus(`Added tag "${tag}"`)
      await loadMods()
      await loadTags()
    } catch (error) {
      setStatus('Error adding tag: ' + error)
    }
  }

  const handleAddCustomTag = async () => {
    if (!newTagInput.trim() || selectedMods.size === 0) return

    try {
      for (const modPath of selectedMods) {
        await invoke('add_custom_tag', { modPath, tag: newTagInput.trim() })
      }
      setStatus(`Added tag "${newTagInput}" to ${selectedMods.size} mod(s)`)
      setNewTagInput('')
      await loadMods()
      await loadTags()
    } catch (error) {
      setStatus(`Error: ${error}`)
    }
  }

  // Keep the global tag list in sync when the Install panel creates a new tag
  const registerTagFromInstallPanel = (tag: string) => {
    const trimmed = (tag || '').trim()
    if (!trimmed) return
    setAllTags(prev => prev.includes(trimmed) ? prev : [...prev, trimmed].sort())
  }

  const handleDeleteTagFromCatalog = async (tag: string) => {
    const modCount = mods.filter(m => (m.custom_tags || []).includes(tag)).length
    if (modCount > 0) {
      setDeleteTagConfirm({ tag, modCount })
      return
    }
    await executeDeleteTag(tag)
  }

  const executeDeleteTag = async (tag: string) => {
    try {
      await invoke('delete_tag_from_all_mods', { tag })
      if (filterTag === tag) setFilterTag('')
      await loadMods()
      await loadTags()
    } catch (error) {
      console.error('Error deleting tag:', error)
    }
  }

  const confirmDeleteTag = async () => {
    if (!deleteTagConfirm) return
    await executeDeleteTag(deleteTagConfirm.tag)
    setDeleteTagConfirm(null)
  }

  const handleRemoveTag = async (modPath: string, tag: string) => {
    try {
      await invoke('remove_custom_tag', { modPath, tag })
      setStatus(`Removed tag "${tag}"`)
      await loadMods()
      await loadTags()
    } catch (error) {
      setStatus(`Error removing tag: ${error}`)
    }
  }

  const handleBulkToggleTag = async (tag: string) => {
    const selectedModsList = mods.filter(m => selectedMods.has(m.path))
    const allHaveTag = selectedModsList.every(m => m.custom_tags && m.custom_tags.includes(tag))
    
    try {
      if (allHaveTag) {
        for (const modPath of selectedMods) {
          await handleRemoveTag(modPath, tag)
        }
        setStatus(`Removed tag "${tag}" from ${selectedMods.size} mod(s)`)
      } else {
        for (const modPath of selectedMods) {
          await handleAddTagToSingleMod(modPath, tag)
        }
        setStatus(`Assigned tag "${tag}" to ${selectedMods.size} mod(s)`)
      }
    } catch (e) {
      setStatus(`Error: ${e}`)
    }
  }

  // Rename a mod (calls backend to rename actual file)
  const handleRenameMod = async (modPath: string, newName: string) => {
    if (gameRunning && !bypassGameRunningLock) {
      alert.warning(
        'Game Running',
        'Cannot rename mods while game is running.'
      )
      return
    }

    try {
      const newPath = await invoke('rename_mod', { modPath, newName }) as string
      setStatus(`Renamed to "${newName}"`)
      const refreshed = await loadMods()
      if (selectedMod && selectedMod.path === modPath) {
        const updated = refreshed.find(m => m.path === newPath)
        setSelectedMod(updated || null)
      }
    } catch (error) {
      setStatus(`Error renaming mod: ${error}`)
      console.error('Error renaming mod:', error)
    }
  }

  // Handle installing a mod received from the browser extension
  const handleExtensionModInstall = async (targetFolderId: string | null) => {
    if (!extensionModPath) return

    const modPath = extensionModPath // Copy path before clearing state

    // Close the overlay immediately
    setExtensionModPath(null)

    // Start progress bar (indeterminate)
    setIsModsLoading(true)
    setModLoadingProgress(-1)

    // Update DRP status
    invoke('discord_set_installing').catch(console.warn)

    // Use promise toast for loading state and result
    alert.promise(
      (async () => {
        try {
          await invoke('quick_organize', {
            paths: [modPath],
            targetFolder: targetFolderId || ''
          })

          await loadMods()
          await loadFolders()
          setStatus(`Mod installed successfully!`)

          // Show warning after success if game is running
          if (gameRunning) {
            alert.warning(
              'Game Running',
              'Mods installed, but changes will only take effect after restarting the game.',
              { duration: 8000 }
            )
          }

          return {}
        } finally {
          setIsModsLoading(false)
          setModLoadingProgress(0)
          invoke('discord_set_idle').catch(console.warn)
        }
      })(),
      {
        loading: {
          title: 'Installing from Extension',
          description: 'Copying mod file...'
        },
        success: () => ({
          title: 'Installation Complete',
          description: 'Mod installed successfully from browser extension'
        }),
        error: (err) => ({
          title: 'Installation Failed',
          description: String(err)
        })
      }
    )
  }

  // Handle quick organize for PAKs with no uassets (skips install panel)
  const handleQuickOrganizeInstall = async (targetFolderId: string | null) => {
    if (!quickOrganizePaths || quickOrganizePaths.length === 0) return

    const pathCount = quickOrganizePaths.length
    const pathsCopy = [...quickOrganizePaths] // Copy paths before clearing state

    // Close the overlay immediately
    setQuickOrganizePaths(null)

    // Start progress bar (indeterminate)
    setIsModsLoading(true)
    setModLoadingProgress(-1)

    // Update DRP status
    invoke('discord_set_installing').catch(console.warn)

    // Use promise toast for loading state and result
    alert.promise(
      (async () => {
        try {
          await invoke('quick_organize', {
            paths: pathsCopy,
            targetFolder: targetFolderId || ''
          })

          await loadMods()
          await loadFolders()
          setStatus(`${pathCount} PAK file(s) copied successfully!`)

          // Show warning after success if game is running
          if (gameRunning) {
            alert.warning(
              'Game Running',
              'Mods installed, but changes will only take effect after restarting the game.',
              { duration: 8000 }
            )
          }

          return { count: pathCount }
        } finally {
          setIsModsLoading(false)
          setModLoadingProgress(0)
          invoke('discord_set_idle').catch(console.warn)
        }
      })(),
      {
        loading: {
          title: 'Quick Installing',
          description: `Copying ${pathCount} PAK file${pathCount > 1 ? 's' : ''}...`
        },
        success: (result) => ({
          title: 'Installation Complete',
          description: `Successfully installed ${result.count} mod${result.count > 1 ? 's' : ''}`
        }),
        error: (err) => ({
          title: 'Installation Failed',
          description: String(err)
        })
      }
    )
  }

  const handleResizeStart = (e: React.MouseEvent<HTMLDivElement>) => {
    setIsResizing(true)
    e.preventDefault()
  }

  const handleResizeMove = (e: MouseEvent) => {
    if (!isResizing) return

    const containerWidth = window.innerWidth
    const newLeftWidth = (e.clientX / containerWidth) * 100

    // Constrain right panel between 25% and 30% (left panel 70% - 75%)
    if (newLeftWidth >= 70 && newLeftWidth <= 75) {
      setLeftPanelWidth(newLeftWidth)
      if (isRightPanelOpen) {
        setLastPanelWidth(newLeftWidth)
      }
    }
  }

  const handleResizeEnd = () => {
    setIsResizing(false)
  }

  const handleFiltersResizeStart = (e: React.MouseEvent<HTMLDivElement>) => {
    setIsFiltersResizing(true)
    const filtersElement = e.currentTarget.previousElementSibling as HTMLElement
    filtersResizeRef.current = {
      startY: e.clientY,
      startHeight: filtersHeight !== null ? filtersHeight : (filtersElement?.offsetHeight || 200)
    }
    e.preventDefault()
  }

  const handleFiltersResizeMove = (e: MouseEvent) => {
    if (!isFiltersResizing) return
    const deltaY = e.clientY - filtersResizeRef.current.startY
    const newHeight = Math.max(50, Math.min(window.innerHeight - 200, filtersResizeRef.current.startHeight + deltaY))
    setFiltersHeight(newHeight)
  }

  const handleFiltersResizeEnd = () => {
    setIsFiltersResizing(false)
  }

  const toggleRightPanel = () => {
    if (isRightPanelOpen) {
      // Collapse
      setLastPanelWidth(leftPanelWidth)
      setLeftPanelWidth(100)
      setIsRightPanelOpen(false)
    } else {
      // Expand
      setLeftPanelWidth(lastPanelWidth)
      setIsRightPanelOpen(true)
    }
  }

  useEffect(() => {
    if (isResizing) {
      document.addEventListener('mousemove', handleResizeMove)
      document.addEventListener('mouseup', handleResizeEnd)
      return () => {
        document.removeEventListener('mousemove', handleResizeMove)
        document.removeEventListener('mouseup', handleResizeEnd)
      }
    }
  }, [isResizing])

  useEffect(() => {
    if (isFiltersResizing) {
      document.addEventListener('mousemove', handleFiltersResizeMove)
      document.addEventListener('mouseup', handleFiltersResizeEnd)
      return () => {
        document.removeEventListener('mousemove', handleFiltersResizeMove)
        document.removeEventListener('mouseup', handleFiltersResizeEnd)
      }
    }
  }, [isFiltersResizing])

  // Compute base filtered mods (excluding folder filter)
  const baseFilteredMods = mods.filter(mod => {
    // Hide LODs_Disabler mods from the list - they are controlled via Tools panel
    const modName = mod.mod_name || mod.custom_name || mod.path.split(/[/\\]/).pop() || ''
    if (modName.toLowerCase().includes('lods_disabler') || mod.path.toLowerCase().includes('lods_disabler')) {
      return false
    }

    // Search query
    if (searchQuery) {
      const query = searchQuery.toLowerCase()
      const displayName = (mod.custom_name || mod.path.split(/[/\\]/).pop() || '').toLowerCase()
      if (!displayName.includes(query)) return false
    }

    const modTags = toTagArray(mod.custom_tags)

    if (filterTag && !modTags.includes(filterTag)) {
      return false
    }

    // New: Multi-select Character/Hero and Category filters using Mod Detection API
    const hasCharFilter = selectedCharacters.size > 0
    const hasCatFilter = selectedCategories.size > 0
    if (hasCharFilter || hasCatFilter) {
      const d = modDetails[mod.path]
      if (!d) return false // wait for details when filters active

      if (hasCatFilter) {
        const mainCatMatch = d.category && selectedCategories.has(d.category)
        const adds = getAdditionalCategories(d)
        const addCatMatch = adds.some((cat: string) => selectedCategories.has(cat))
        if (!mainCatMatch && !addCatMatch) return false
      }

      if (hasCharFilter) {
        const isMulti = typeof d.mod_type === 'string' && d.mod_type.startsWith('Multiple Heroes')
        const isGeneric = !d.character_name && !isMulti

        let multiMatch = false
        if (isMulti && d.files) {
          const heroes = detectHeroes(d.files)
          multiMatch = heroes.some(h => selectedCharacters.has(h))
        }

        const match = (
          (d.character_name && selectedCharacters.has(d.character_name)) ||
          (isMulti && selectedCharacters.has('__multi')) ||
          (isGeneric && selectedCharacters.has('__generic')) ||
          multiMatch
        )
        if (!match) return false
      }
    }

    return true
  })

  // Apply folder filter to get final list for display
  const filteredMods = baseFilteredMods.filter(mod => {
    if (selectedFolderId === 'all') return true

    if (showSubfolderMods) {
      // Match exact folder OR subfolder
      // e.g. if selected is "Category", match "Category" and "Category/Sub"
      return mod.folder_id === selectedFolderId ||
        (mod.folder_id && mod.folder_id.startsWith(selectedFolderId + '/'))
    } else {
      // Match exact folder only
      return mod.folder_id === selectedFolderId
    }
  }).sort((a, b) => {
    if (sortBy === 'name') {
      const aName = (a.custom_name || a.path.split(/[/\\]/).pop() || '').toLowerCase()
      const bName = (b.custom_name || b.path.split(/[/\\]/).pop() || '').toLowerCase()
      return sortDirection === 'asc' ? aName.localeCompare(bName) : bName.localeCompare(aName)
    } else {
      const aTime = a.modified_date || 0
      const bTime = b.modified_date || 0
      return sortDirection === 'asc' ? aTime - bTime : bTime - aTime
    }
  })

  // Keep filteredModsRef in sync for Shift+click range selection
  filteredModsRef.current = filteredMods

  // Keyboard shortcuts handler (must be after filteredMods is defined)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase()
      const ctrl = e.ctrlKey
      const shift = e.shiftKey

      // Skip if typing in an input field (except Escape and Ctrl+F)
      const eventTarget = e.target
      const isInputActive = eventTarget instanceof HTMLElement &&
        (eventTarget.tagName === 'INPUT' || eventTarget.tagName === 'TEXTAREA')
      if (isInputActive && key !== 'escape' && !(ctrl && key === 'f')) return

      // F5 - Disable browser refresh
      if (key === 'f5') {
        e.preventDefault()
        return
      }
      // Ctrl+R - Full React reinit
      else if (ctrl && key === 'r' && !shift) {
        e.preventDefault()
        window.location.reload()
      }
      // Ctrl+F - Focus search
      else if (ctrl && key === 'f') {
        e.preventDefault()
        searchInputRef.current?.focus()
      }
      // Ctrl+Shift+R - Refresh mods only
      else if (ctrl && shift && key === 'r') {
        e.preventDefault()
        alert.info('Mods Refreshed', 'Refreshed mods list.')
        loadMods()
      }
      // Ctrl+, - Settings
      else if (ctrl && key === ',') {
        e.preventDefault()
        setPanel('settings', true)
      }
      // Escape - Close panels or deselect
      else if (key === 'escape') {
        if (panels.shortcuts) setPanel('shortcuts', false)
        else if (panels.settings) setPanel('settings', false)
        else if (panels.tools) setPanel('tools', false)
        else if (panels.sharing) setPanel('sharing', false)
        else if (panels.install) setPanel('install', false)
        else if (panels.clash) setPanel('clash', false)
        else if (selectedMod) setSelectedMod(null)
      }
      // Ctrl+E - Toggle mod enabled/disabled
      else if (ctrl && key === 'e' && selectedMod) {
        e.preventDefault()
        handleToggleMod(selectedMod.path)
      }
      // F2 - Rename mod
      else if (key === 'f2' && selectedMod) {
        e.preventDefault()
        if (gameRunning && !bypassGameRunningLock) {
          alert.warning(
            'Game Running',
            'Cannot rename mods while game is running.'
          )
          return
        }
        setRenamingModPath(selectedMod.path)
      }
      // Enter - Open mod details
      else if (key === 'enter' && selectedMod && !isRightPanelOpen) {
        e.preventDefault()
        setLeftPanelWidth(lastPanelWidth > 60 ? lastPanelWidth : 70)
        setIsRightPanelOpen(true)
      }
      // Arrow navigation
      else if (['arrowup', 'arrowdown', 'arrowleft', 'arrowright'].includes(key)) {
        if (filteredMods.length === 0) return
        e.preventDefault()

        const currentIndex = selectedMod
          ? filteredMods.findIndex(m => m.path === selectedMod.path)
          : -1

        let newIndex = currentIndex

        if (viewMode === 'list' || viewMode === 'list-compact') {
          // List view: only up/down
          if (key === 'arrowup') newIndex = Math.max(0, currentIndex - 1)
          else if (key === 'arrowdown') newIndex = Math.min(filteredMods.length - 1, currentIndex + 1)
        } else {
          // Grid/Card view: all 4 directions
          // Calculate actual items per row by measuring the grid layout
          let itemsPerRow = 1
          const grid = modsGridRef.current
          if (grid) {
            const items = grid.querySelectorAll<HTMLElement>('.mod-card')
            if (items.length >= 2) {
              // Count how many items share the same top offset (are in the first row)
              const firstTop = items[0].offsetTop
              let count = 0
              for (const item of items) {
                if (item.offsetTop === firstTop) count++
                else break
              }
              itemsPerRow = Math.max(1, count)
            }
          }
          if (key === 'arrowup') newIndex = Math.max(0, currentIndex - itemsPerRow)
          else if (key === 'arrowdown') newIndex = Math.min(filteredMods.length - 1, currentIndex + itemsPerRow)
          else if (key === 'arrowleft') newIndex = Math.max(0, currentIndex - 1)
          else if (key === 'arrowright') newIndex = Math.min(filteredMods.length - 1, currentIndex + 1)
        }

        if (newIndex !== currentIndex && newIndex >= 0 && newIndex < filteredMods.length) {
          setSelectedMod(filteredMods[newIndex])
        } else if (currentIndex === -1 && filteredMods.length > 0) {
          setSelectedMod(filteredMods[0])
        }
      }
      // F1 - Show shortcuts help
      else if (key === 'f1') {
        e.preventDefault()
        setPanel('shortcuts', true)
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [
    selectedMod, panels, viewMode,
    filteredMods, isRightPanelOpen, lastPanelWidth
  ])

  // Group mods by folder
  const modsByFolder: Record<string, ModRecord[]> = {}
  modsByFolder['_root'] = filteredMods.filter(m => !m.folder_id)
  folders.forEach(folder => {
    modsByFolder[folder.id] = filteredMods.filter(m => m.folder_id === folder.id)
  })

  const toggleFolder = (folderId: string) => {
    const newExpanded = new Set(expandedFolders)
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId)
    } else {
      newExpanded.add(folderId)
    }
    setExpandedFolders(newExpanded)
  }

  const handleInstallMods = async (modsWithSettings: InstallModPayload[]) => {
    setPanel('install', false)
    setInstallLogs([])

    // Update DRP status to Installing
    invoke('discord_set_installing').catch(console.warn)

    const modCount = modsWithSettings.length

    // Start progress bar (indeterminate until backend sends progress events)
    setIsModsLoading(true)
    setModLoadingProgress(-1)

    // Use promise toast for loading state and result
    // The backend spawns threads and returns immediately, so we need to wait
    // for the install_complete event to know when installation is actually done
    alert.promise(
      (async () => {
        // Create a promise that resolves when install_complete event fires
        const installCompletePromise = new Promise<void>((resolve, reject) => {
          let unlistenComplete: UnlistenFn | null = null
          let unlistenError: UnlistenFn | null = null
          let timeoutId: ReturnType<typeof setTimeout> | null = null

          // Set a reasonable timeout (10 minutes for large mods)
          timeoutId = setTimeout(() => {
            if (unlistenComplete) unlistenComplete()
            if (unlistenError) unlistenError()
            reject(new Error('Installation timed out after 10 minutes'))
          }, 10 * 60 * 1000)

          // Listen for success
          listen('install_complete', () => {
            if (timeoutId) clearTimeout(timeoutId)
            if (unlistenComplete) unlistenComplete()
            if (unlistenError) unlistenError()
            resolve()
          }).then(unlisten => { unlistenComplete = unlisten })

          // Listen for failure (from toast_events via toast_notification)
          listen('toast_notification', (event: any) => {
            // Check if this is an installation failure toast
            if (event.payload?.title === 'Installation Failed') {
              if (timeoutId) clearTimeout(timeoutId)
              if (unlistenComplete) unlistenComplete()
              if (unlistenError) unlistenError()
              reject(new Error(event.payload?.description || 'Installation failed'))
            }
          }).then(unlisten => { unlistenError = unlisten })
        })

        // Start the installation (returns immediately since backend spawns threads)
        await invoke('install_mods', { mods: modsWithSettings })

        // Wait for the actual installation to complete
        await installCompletePromise

        // Mirror tag assignment flow used by the main list/context menu
        const typeTracker: Record<string, number> = {}
        for (const mod of modsWithSettings) {
          const modType = mod.mod_type || 'Unknown'
          const count = typeTracker[modType] || 0
          const minNines = 7 + count
          const name = mod.customName || mod.mod_name || 'Unnamed_Mod'
          const filename = `${normalizeModBaseName(name, minNines)}.pak`

          if (mod.selectedTags && mod.selectedTags.length > 0) {
            const separator = gamePath.includes('\\') ? '\\' : '/'
            const fullPath = `${gamePath}${separator}${filename}`

            for (const tag of mod.selectedTags) {
              try {
                await invoke('add_custom_tag', { modPath: fullPath, tag })
              } catch (e) {
                console.error(`Failed to add tag ${tag} to ${fullPath}:`, e)
              }
            }
          }

          typeTracker[modType] = count + 1
        }

        // Reload in background so alert shows success immediately
        loadMods()
        loadFolders()
        loadTags()
        setStatus('Mods installed successfully!')

        // Reset DRP status to Idle (or effective mod count if we could easily get it)
        // For now, Idle is safe and functionally correct
        invoke('discord_set_idle').catch(console.warn)

        // Show warning after success if game is running
        if (gameRunning) {
          alert.warning(
            'Game Running',
            'Mods installed, but changes will only take effect after restarting the game.',
            { duration: 8000 }
          )
        }

        return { count: modCount }
      })(),
      {
        loading: {
          title: 'Installing Mods',
          description: `Processing ${modCount} mod${modCount > 1 ? 's' : ''}...`
        },
        success: (result) => ({
          title: 'Installation Complete',
          description: `Successfully installed ${result.count} mod${result.count > 1 ? 's' : ''}`
        }),
        error: (err) => ({
          title: 'Installation Failed',
          description: String(err)
        })
      }
    )
  }

  const saveAllSettings = async (overrides: Partial<AppSettings>) => {
    // 1. Determine new values (merging overrides with current React state)
    const newTheme = overrides.theme !== undefined ? overrides.theme : theme;
    const newAccentColor = overrides.accentColor !== undefined ? overrides.accentColor : accentColor;
    const newViewMode = (overrides.viewMode !== undefined ? overrides.viewMode : viewMode) as ViewMode;
    const newHideSuffix = overrides.hideSuffix !== undefined ? overrides.hideSuffix : hideSuffix;
    const newAutoOpenDetails = overrides.autoOpenDetails !== undefined ? overrides.autoOpenDetails : autoOpenDetails;
    const newShowHeroIcons = overrides.showHeroIcons !== undefined ? overrides.showHeroIcons : showHeroIcons;
    const newShowHeroBg = overrides.showHeroBg !== undefined ? overrides.showHeroBg : showHeroBg;
    const newShowModType = overrides.showModType !== undefined ? overrides.showModType : showModType;
    const newShowExperimental = overrides.showExperimental !== undefined ? overrides.showExperimental : showExperimental;
    const newEnableDrp = overrides.enableDrp !== undefined ? overrides.enableDrp : enableDrp;
    const newParallelProcessing = overrides.parallelProcessing !== undefined ? overrides.parallelProcessing : parallelProcessing;
    const newAutoCheckUpdates = overrides.autoCheckUpdates !== undefined ? overrides.autoCheckUpdates : autoCheckUpdates;
    const newHoldToDelete = overrides.holdToDelete !== undefined ? overrides.holdToDelete : holdToDelete;
    const newShowSubfolderMods = overrides.showSubfolderMods !== undefined ? overrides.showSubfolderMods : showSubfolderMods;
    const newBypassGameRunningLock = overrides.bypassGameRunningLock !== undefined ? overrides.bypassGameRunningLock : bypassGameRunningLock;
    const newLauncherType = overrides.launcherType !== undefined ? overrides.launcherType : launcherType;

    // 2. Map newAccentColor from hex to the preset name if it is a hex value
    const accentName = Object.keys(ACCENT_COLORS_MAP).find(key => ACCENT_COLORS_MAP[key] === newAccentColor) || newAccentColor;

    // 3. Construct the payload for save_app_settings
    const payload: AppSettings = {
      theme: newTheme,
      accentColor: accentName,
      viewMode: newViewMode,
      hideSuffix: newHideSuffix,
      autoOpenDetails: newAutoOpenDetails,
      showHeroIcons: newShowHeroIcons,
      showHeroBg: newShowHeroBg,
      showModType: newShowModType,
      showExperimental: newShowExperimental,
      enableDrp: newEnableDrp,
      parallelProcessing: newParallelProcessing,
      autoCheckUpdates: newAutoCheckUpdates,
      holdToDelete: newHoldToDelete,
      showSubfolderMods: newShowSubfolderMods,
      bypassGameRunningLock: newBypassGameRunningLock,
      launcherType: newLauncherType,
    };

    // 4. Save to backend
    await invoke('save_app_settings', { settings: payload }).catch(console.warn);

    // 5. Update React states immediately
    if (overrides.theme !== undefined) {
      setTheme(newTheme);
      document.documentElement.setAttribute('data-theme', newTheme);
    }
    if (overrides.accentColor !== undefined) {
      setAccentColor(newAccentColor);
      document.documentElement.style.setProperty('--accent-primary', newAccentColor);
      document.documentElement.style.setProperty('--accent-secondary', newAccentColor);
      const palette = AURORA_PALETTES[newAccentColor] || ['#be1c1c', '#ff9800', '#ffcc00', '#ff6b35'];
      document.documentElement.style.setProperty('--aurora-color-1', palette[0]);
      document.documentElement.style.setProperty('--aurora-color-2', palette[1]);
      document.documentElement.style.setProperty('--aurora-color-3', palette[2]);
      document.documentElement.style.setProperty('--aurora-color-4', palette[3]);
    }
    if (overrides.viewMode !== undefined) {
      setViewMode(newViewMode);
    }
    if (overrides.hideSuffix !== undefined) setHideSuffix(newHideSuffix);
    if (overrides.autoOpenDetails !== undefined) setAutoOpenDetails(newAutoOpenDetails);
    if (overrides.showHeroIcons !== undefined) setShowHeroIcons(newShowHeroIcons);
    if (overrides.showHeroBg !== undefined) setShowHeroBg(newShowHeroBg);
    if (overrides.showModType !== undefined) setShowModType(newShowModType);
    if (overrides.showExperimental !== undefined) setShowExperimental(newShowExperimental);
    if (overrides.enableDrp !== undefined) setEnableDrp(newEnableDrp);
    if (overrides.parallelProcessing !== undefined) setParallelProcessing(newParallelProcessing);
    if (overrides.autoCheckUpdates !== undefined) setAutoCheckUpdates(newAutoCheckUpdates);
    if (overrides.holdToDelete !== undefined) setHoldToDelete(newHoldToDelete);
    if (overrides.showSubfolderMods !== undefined) setShowSubfolderMods(newShowSubfolderMods);
    if (overrides.bypassGameRunningLock !== undefined) setBypassGameRunningLock(newBypassGameRunningLock);
    if (overrides.launcherType !== undefined) setLauncherType(newLauncherType);

    // 6. Handle side effects (DRP connection, theme, etc.)
    if (newEnableDrp && !enableDrp) {
      invoke('discord_connect')
        .then(() => invoke('discord_set_managing_mods', { modCount: mods.flat().length }))
        .catch(console.warn);
    } else if (!newEnableDrp && enableDrp) {
      invoke('discord_disconnect').catch(console.warn);
    }

    if (newEnableDrp) {
      invoke('discord_set_theme', { theme: accentName }).catch(console.warn);
    }

    if (overrides.parallelProcessing !== undefined && newParallelProcessing !== parallelProcessing) {
      handleSetParallelProcessing(newParallelProcessing);
    }

    if (newShowExperimental === false && newViewMode === 'list-compact') {
      setViewMode('list');
      await invoke('save_app_settings', { settings: { ...payload, viewMode: 'list' } }).catch(console.warn);
    }
  };

  const handleSaveSettings = async (settings: Partial<AppSettings>) => {
    await saveAllSettings(settings);
    setStatus('Settings saved');
  }

  // Add this effect to initialize theme and view settings
  useEffect(() => {
    // Load App Settings from backend state.json
    invoke('get_app_settings')
      .then((settings: any) => {
        // Resolve hex code from accent preset name
        const hexAccent = ACCENT_COLORS_MAP[settings.accentColor] || ACCENT_COLORS_MAP['red'] || '#be1c1c';
        
        // 1. Apply Theme
        setTheme(settings.theme);
        document.documentElement.setAttribute('data-theme', settings.theme);

        // 2. Apply Accent
        setAccentColor(hexAccent);
        document.documentElement.style.setProperty('--accent-primary', hexAccent);
        document.documentElement.style.setProperty('--accent-secondary', hexAccent);
        const palette = AURORA_PALETTES[hexAccent] || ['#be1c1c', '#ff9800', '#ffcc00', '#ff6b35'];
        document.documentElement.style.setProperty('--aurora-color-1', palette[0]);
        document.documentElement.style.setProperty('--aurora-color-2', palette[1]);
        document.documentElement.style.setProperty('--aurora-color-3', palette[2]);
        document.documentElement.style.setProperty('--aurora-color-4', palette[3]);

        // 3. Apply ViewMode
        const parsedViewMode: ViewMode =
          settings.viewMode === 'grid' || settings.viewMode === 'compact' || settings.viewMode === 'list-compact'
            ? settings.viewMode
            : 'list';
        setViewMode(parsedViewMode);

        // 4. Apply all other settings
        setHideSuffix(settings.hideSuffix);
        setAutoOpenDetails(settings.autoOpenDetails);
        setShowHeroIcons(settings.showHeroIcons);
        setShowHeroBg(settings.showHeroBg);
        setShowModType(settings.showModType);
        setShowExperimental(settings.showExperimental);
        setAutoCheckUpdates(settings.autoCheckUpdates);
        setParallelProcessing(settings.parallelProcessing);
        setHoldToDelete(settings.holdToDelete);
        setShowSubfolderMods(settings.showSubfolderMods);
        setBypassGameRunningLock(settings.bypassGameRunningLock);
        setEnableDrp(settings.enableDrp);
        if (settings.launcherType) setLauncherType(settings.launcherType);

        // 5. Run auto update check if enabled
        if (settings.autoCheckUpdates) {
          console.debug('[Updates] Running startup auto-check for updates');
          void handleCheckForUpdates(true);
        } else {
          console.debug('[Updates] Skipping startup auto-check (disabled in settings)');
        }
      })
      .catch((err) => {
        console.error('Failed to load app settings from backend:', err);
      });

    const hasSeenTour = localStorage.getItem('hasSeenOnboarding');
    if (!hasSeenTour) {
      setTimeout(() => setShowOnboarding(true), 1200);
    }
  }, []);


  // Add these handlers
  const handleThemeChange = (newTheme: string) => {
    saveAllSettings({ theme: newTheme });
  };

  const handleAccentChange = (newAccent: string) => {
    saveAllSettings({ accentColor: newAccent });
  };

  const handleViewModeChange = (newMode: ViewMode) => {
    saveAllSettings({ viewMode: newMode });
  };

  const handleCloseTour = () => {
    setShowOnboarding(false);
    localStorage.setItem('hasSeenOnboarding', 'true');
  };

  const handleReplayTour = () => {
    setPanel('settings', false);
    setTimeout(() => setShowOnboarding(true), 300);
  };

  // Remove static splash screen
  useEffect(() => {
    const splash = document.getElementById('splash-screen');
    if (splash) {
      splash.style.transition = 'opacity 0.4s ease-out';
      splash.style.opacity = '0';
      setTimeout(() => splash.remove(), 400);
    }
  }, []);

  return (
    <div className={`app${isAprilFools ? ' april-fools' : ''}`}>
      <TitleBar />
      {panels.install && (
        <InstallModPanel
          mods={modsToInstall}
          allTags={allTags}
          folders={folders}
          onCreateTag={registerTagFromInstallPanel}
          onDeleteTag={handleDeleteTagFromCatalog}
          onCreateFolder={handleCreateFolderAndReturn}
          onInstall={handleInstallMods}
          onCancel={() => setPanel('install', false)}
          onNewTag={(callback) => setNewTagPrompt({ callback })}
          onNewFolder={(callback) => setNewFolderFromInstall({ callback })}
          onMergeHybrid={handleMergeHybrid}
        />
      )}

      {panels.clash && (
        <ClashPanel
          clashes={clashes}
          mods={mods}
          onSetPriority={handleSetPriority}
          onClose={() => setPanel('clash', false)}
        />
      )}

      {panels.settings && (
        <SettingsPanel
          settings={{ hideSuffix, autoOpenDetails, showHeroIcons, showHeroBg, showModType, showExperimental, enableDrp, parallelProcessing, autoCheckUpdates, holdToDelete, showSubfolderMods, bypassGameRunningLock, launcherType }}
          onSave={handleSaveSettings}
          onClose={() => setPanel('settings', false)}
          theme={theme}
          setTheme={handleThemeChange}
          accentColor={accentColor}
          setAccentColor={handleAccentChange}
          gamePath={gamePath}
          onAutoDetectGamePath={handleAutoDetect}
          onBrowseGamePath={handleBrowseGamePath}
          isGamePathLoading={loading}
          setParallelProcessing={handleSetParallelProcessing}
          onCheckForUpdates={handleCheckForUpdates}
          onViewChangelog={handleViewChangelog}
          isCheckingUpdates={isCheckingUpdates}
          onReplayTour={handleReplayTour}
          onOpenShortcuts={() => setPanel('shortcuts', true)}
        />
      )}

      {panels.credits && (
        <CreditsPanel
          onClose={() => setPanel('credits', false)}
          version={version}
        />
      )}

      {panels.tools && (
        <ToolsPanel
          onClose={() => setPanel('tools', false)}
          mods={mods}
          onToggleMod={handleToggleMod}
        />
      )}

      {panels.sharing && (
        <SharingPanel
          onClose={() => setPanel('sharing', false)}
          gamePath={gamePath}
          installedMods={mods}
          selectedMods={selectedMods}
          folders={folders}
        />
      )}

      {/* Drop Zone Overlay */}
      <DropZoneOverlay
        isVisible={isDragging}
        folders={folders}
        isAprilFools={isAprilFools}
        onInstallDrop={() => {
          // Just signals intent - actual files come from Tauri event
          setDropTargetFolder(null)
        }}
        onQuickOrganizeDrop={(folderId) => {
          // Store the target folder for when Tauri fires the drop event
          setDropTargetFolder(folderId)
        }}
        onNewFolderDrop={() => {
          // Special marker to indicate we should prompt for new folder on drop
          setDropTargetFolder('__NEW_FOLDER__')
        }}
        onClose={() => setIsDragging(false)}
        onCreateFolder={handleCreateFolderAndReturn}
      />

      {/* Extension Mod Overlay - for mods received from browser extension */}
      <ExtensionModOverlay
        isVisible={!!extensionModPath}
        filePath={extensionModPath}
        folders={folders}
        onInstall={handleExtensionModInstall}
        onCancel={() => setExtensionModPath(null)}
        onCreateFolder={handleCreateFolderAndReturn}
        onNewFolder={(callback) => setNewFolderFromInstall({ callback })}
      />

      {/* Quick Organize Overlay - for PAK files with no uassets */}
      <QuickOrganizeOverlay
        isVisible={!!quickOrganizePaths && quickOrganizePaths.length > 0}
        paths={quickOrganizePaths || []}
        folders={folders}
        onInstall={handleQuickOrganizeInstall}
        onCancel={() => setQuickOrganizePaths(null)}
        onCreateFolder={handleCreateFolderAndReturn}
      />

      {/* New Folder Prompt Modal - for creating folders during drop */}
      {(() => {
        const promptPathCount = newFolderPrompt ? newFolderPrompt.paths.length : 0
        return (
          <InputPromptModal
            isOpen={!!newFolderPrompt}
            title={promptPathCount > 0 ? "Create Folder & Install" : "Create New Folder"}
            placeholder="Enter folder name..."
            confirmText={promptPathCount > 0 ? "Create & Install" : "Create"}
            onConfirm={handleNewFolderConfirm}
            onCancel={() => {
              setNewFolderPrompt(null)
              setStatus('Folder creation cancelled')
            }}
          />
        )
      })()}

      <InputPromptModal
        isOpen={!!renameFolderPrompt}
        title="Rename Folder"
        placeholder="Enter new folder name..."
        confirmText="Rename"
        initialValue={renameFolderPrompt?.currentName || ''}
        onConfirm={handleRenameFolderConfirm}
        onCancel={() => setRenameFolderPrompt(null)}
      />

      <InputPromptModal
        isOpen={!!newTagPrompt}
        title="Create New Tag"
        placeholder="Enter tag name..."
        confirmText="Create"
        icon={<FaTag />}
        onConfirm={(tag) => {
          if (newTagPrompt?.callback) newTagPrompt.callback(tag)
          setNewTagPrompt(null)
        }}
        onCancel={() => setNewTagPrompt(null)}
      />

      <InputPromptModal
        isOpen={!!newFolderFromInstall}
        title="Create New Folder"
        placeholder="Enter folder name..."
        confirmText="Create"
        onConfirm={(name) => {
          if (newFolderFromInstall?.callback) newFolderFromInstall.callback(name)
          setNewFolderFromInstall(null)
        }}
        onCancel={() => setNewFolderFromInstall(null)}
      />

      <InputPromptModal
        isOpen={!!deleteTagConfirm}
        mode="confirm"
        title="Delete Tag"
        description={
          deleteTagConfirm
            ? `"${deleteTagConfirm.tag}" is applied to ${deleteTagConfirm.modCount} mod${deleteTagConfirm.modCount > 1 ? 's' : ''}. This will remove it from all mods.`
            : ''
        }
        confirmText="Delete"
        icon={<FaTag />}
        accentColor="#ef4444"
        onConfirm={confirmDeleteTag}
        onCancel={() => setDeleteTagConfirm(null)}
      />

      <UpdateModModal
        isOpen={updateModState.isOpen}
        onClose={() => setUpdateModState(prev => ({ ...prev, isOpen: false }))}
        onConfirm={handleConfirmUpdate}
        oldMod={updateModState.mod}
        newSourcePath={updateModState.newSourcePath}
        initialObfuscate={updateModState.obfuscatePreference}
      />

      <PromiseTransitionLoader
        isVisible={promiseLoaderCount > 0}
        message={promiseLoaderMessage}
      />

      <UpdateAppModal
        isOpen={showUpdateModal}
        updateInfo={updateInfo}
        downloadProgress={updateDownloadProgress}
        downloadedPath={downloadedUpdatePath}
        onDownload={handleDownloadUpdate}
        onApply={handleApplyUpdate}
        onOpenReleasePage={(url: string) => { import('@tauri-apps/plugin-shell').then(m => m.open(url)); }}
        onClose={handleCancelUpdate}
      />

      <ChangelogModal
        isOpen={showChangelogModal}
        version={version}
        changelog={changelogContent}
        onClose={() => setShowChangelogModal(false)}
      />


      <header className="header">
        <div
          className="header-branding"
          data-tour="header-branding"
          onClick={() => setPanel('credits', true)}
          title="View Credits"
        >
          <ModularLogo size={50} className="repak-icon" />
          <div className="header-title-group">
            <h1 className="font-logo">Repak <AuroraText className="font-logo">X</AuroraText> </h1>
            <span className="version">v{version}</span>
          </div>
        </div>
        <div className="header-actions-right">
          <button
            className="btn-settings"
            data-tour="launch-btn"
            title={gameRunning ? "Game is currently running" : "Launch Rivals"}
            style={{
              background: gameRunning
                ? 'rgba(255, 152, 0, 0.15)'
                : launchSuccess
                  ? 'rgba(76, 175, 80, 0.15)'
                  : 'rgba(74, 158, 255, 0.1)',
              color: gameRunning
                ? '#ff9800'
                : launchSuccess
                  ? '#4CAF50'
                  : '#4a9eff',
              border: gameRunning
                ? '1px solid rgba(255, 152, 0, 0.5)'
                : launchSuccess
                  ? '1px solid rgba(76, 175, 80, 0.5)'
                  : '1px solid rgba(74, 158, 255, 0.3)',
              cursor: gameRunning ? 'default' : 'pointer'
            }}
            onClick={async () => {
              if (gameRunning || launchSuccess) return
              try {
                await invoke('launch_game')
                setStatus('Game launched')
                setLaunchSuccess(true)
                setTimeout(() => setLaunchSuccess(false), 3000)
              } catch (error) {
                setStatus('Error launching game: ' + error)
              }
            }}
          >
            <AnimatePresence mode="wait">
              {gameRunning ? (
                <motion.span
                  key="running"
                  className="launch-button-content"
                  initial={{ opacity: 0, scale: 0.5 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.5 }}
                >
                  <span className="blink-icon">⚠️</span> Game Running
                </motion.span>
              ) : launchSuccess ? (
                <motion.span
                  key="success"
                  className="launch-button-content"
                  initial={{ opacity: 0, scale: 0.5 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.5 }}
                >
                  <CheckIcon fontSize="small" /> Launched
                </motion.span>
              ) : (
                <motion.span
                  key="play"
                  className="launch-button-content"
                  initial={{ opacity: 0, scale: 0.5 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.5 }}
                >
                  <PlayArrowIcon fontSize="small" /> Launch Game
                </motion.span>
              )}
            </AnimatePresence>
          </button>

          {/* VFX Updater Button */}
          <button
            className="btn-settings btn-vfx"
            data-tour="vfx-updater-btn"
            title="Open Repak VFX Updater"
            onClick={async () => {
              await openVfxUpdaterWindow()
            }}
          >
            <GiLightningTrio style={{ marginRight: '6px' }} />
            <span>VFX Updater</span>
          </button>

          <button
            onClick={() => setPanel('sharing', true)}
            className="btn-settings"
            data-tour="sharing-btn"
            title="Share Mods"
          >
            <IoMdWifi size={20} /> Share
          </button>
          <button
            onClick={() => setPanel('tools', true)}
            className="btn-settings"
            data-tour="tools-btn"
            title="Tools"
          >
            <FaToolbox size={20} /> Tools
          </button>
          <button
            onClick={() => setPanel('settings', true)}
            className="btn-settings"
            data-tour="settings-btn"
          >
            <IoIosSettings size={20} /> Settings
          </button>
        </div>
      </header>

      <div className="container">
        {/* Main Action Bar */}
        <div className="main-action-bar">
          <div className="search-wrapper" data-tour="search-bar">
            <SearchIcon className="search-icon-large" />
            <input
              ref={searchInputRef}
              type="text"
              placeholder="Search installed mods..."
              value={localSearch}
              onChange={handleSearchChange}
              className="main-search-input"
            />
            {localSearch && (
              <IconButton
                size="small"
                className="clear-search-btn"
                onClick={() => {
                  setLocalSearch('')
                  debouncedSetSearch('') // Clear immediately
                  setSearchQuery('')
                  searchInputRef.current?.focus()
                }}
              >
                <ClearIcon fontSize="small" />
              </IconButton>
            )}
          </div>

          <div className="action-controls">
            <CustomDropdown
              options={[
                { value: 'name_asc', label: 'Name (A-Z)' },
                { value: 'name_desc', label: 'Name (Z-A)' },
                { value: 'modified_desc', label: 'Newest First' },
                { value: 'modified_asc', label: 'Oldest First' },
              ]}
              value={`${sortBy}_${sortDirection}`}
              onChange={(val) => {
                const [newSortBy, newSortDir] = val.split('_');
                setSortBy(newSortBy as 'name' | 'modified');
                setSortDirection(newSortDir as 'asc' | 'desc');
              }}
              placeholder="Sort By"
              icon={<FaSort style={{ fontSize: '1.2rem', opacity: 1, color: 'var(--accent-primary)' }} />}
            />
            <CustomDropdown
              options={[{ value: '', label: 'View All' }, ...allTags]}
              value={filterTag}
              onChange={setFilterTag}
              placeholder="All Tags"
              icon={<FaTag style={{ fontSize: '1.2rem', opacity: 1, color: 'var(--accent-primary)' }} />}
              onAddNew={() => setNewTagPrompt({
                callback: async (tag) => {
                  const trimmed = tag.trim()
                  if (!trimmed) return
                  setAllTags(prev => prev.includes(trimmed) ? prev : [...prev, trimmed].sort())
                  await invoke('add_tag_to_catalog', { tag: trimmed })
                }
              })}
              addNewLabel="+ Create Tag"
              onDeleteOption={handleDeleteTagFromCatalog}
            />

            <AddModSplitButton
              data-tour="add-mod-btn"
              onAddFiles={(files: string[]) => handleFileDrop(files)}
              onAddFolder={(folders: string[]) => handleFileDrop(folders)}
            />
          </div>
        </div>


        {!gamePath && (
          <div className="config-warning">
            ⚠️ Game path not configured. <button onClick={() => setPanel('settings', true)} className="btn-link-warning">Configure in Settings</button>
          </div>
        )}

        {/* Main 3-Panel Layout */}
        <div className="main-panels" onMouseMove={(e) => handleResizeMove(e.nativeEvent)}>
          {/* Wrapper for Left Sidebar and Center Panel */}
          <motion.div
            className="content-wrapper"
            animate={{ width: `${leftPanelWidth}%` }}
            transition={isResizing ? { duration: 0 } : { type: "tween", ease: "circOut", duration: 0.35 }}

          >
            {/* Left Sidebar - Folders */}
            <div className="left-sidebar" data-tour="folder-sidebar">
              {/* Filters Section */}
              <div 
                className="sidebar-filters"
                style={filtersHeight !== null ? { height: `${filtersHeight}px` } : { maxHeight: '35vh' }}
              >
                <div className="sidebar-filters-inner">
                  <div className="filter-title-row">
                    <div className="filter-label">FILTERS</div>
                    {(selectedCharacters.size > 0 || selectedCategories.size > 0) && (
                      <button
                        className="btn-ghost-mini"
                        onClick={() => { setSelectedCharacters(new Set()); setSelectedCategories(new Set()) }}
                        title="Clear all filters"
                      >
                        Clear
                      </button>
                    )}
                  </div>

                  {/* Character/Hero Chips */}
                  <div
                    className="filter-section-header"
                    onClick={() => setShowCharacterFilters(v => !v)}
                  >
                    <div className="filter-label-secondary">Characters {selectedCharacters.size > 0 && `(${selectedCharacters.size})`}</div>
                    <span className="filter-chevron">{showCharacterFilters ? '\u25bc' : '\u25b6'}</span>
                  </div>
                  {showCharacterFilters && (
                    <HeroFilterDropdown
                      availableCharacters={availableCharacters}
                      selectedCharacters={selectedCharacters}
                      modDetails={modDetails}
                      onToggle={(target: string | string[]) => setSelectedCharacters(prev => {
                        const next = new Set(prev);

                        if (Array.isArray(target)) {
                          // Bulk toggle logic
                          const allSelected = target.every(t => next.has(t));
                          if (allSelected) {
                            // If all are selected, deselect all
                            target.forEach(t => next.delete(t));
                          } else {
                            // Otherwise, select all (add missing ones)
                            target.forEach(t => next.add(t));
                          }
                        } else {
                          // Single toggle logic
                          next.has(target) ? next.delete(target) : next.add(target);
                        }

                        return next;
                      })}
                    />
                  )}

                  {/* Category Chips */}
                  <div
                    className="filter-section-header with-margin"
                    onClick={() => setShowTypeFilters(v => !v)}
                  >
                    <div className="filter-label-secondary">Types {selectedCategories.size > 0 && `(${selectedCategories.size})`}</div>
                    <span className="filter-chevron">{showTypeFilters ? '\u25bc' : '\u25b6'}</span>
                  </div>
                  {showTypeFilters && (
                    <div className="filter-chips-scroll">
                      {availableCategories.map(cat => {
                        const active = selectedCategories.has(cat)
                        return (
                          <button
                            key={cat}
                            className={`filter-chip-compact ${active ? 'active' : ''}`}
                            onClick={() => setSelectedCategories(prev => { const next = new Set(prev); active ? next.delete(cat) : next.add(cat); return next; })}
                            title={cat}
                          >
                            {cat}
                          </button>
                        )
                      })}
                    </div>
                  )}
                </div>
              </div>
              {/* Vertical Resize Handle */}
              <div 
                className={`vertical-resizer ${isFiltersResizing ? 'resizing' : ''}`}
                onMouseDown={handleFiltersResizeStart}
              />
              <div className="sidebar-header">
                <h3>Folders</h3>
                <div className="sidebar-header-actions">
                  <button onClick={() => handleCreateFolder()} className="btn-icon" title="New Folder">
                    <CreateNewFolderIcon fontSize="small" />
                  </button>
                </div>
              </div>
              <div className="folder-list">
                <FolderTree
                  folders={folders}
                  selectedFolderId={selectedFolderId}
                  onSelect={setSelectedFolderId}
                  onDelete={handleDeleteFolder}
                  onContextMenu={handleFolderContextMenu}
                  getCount={(id: string) => {
                    if (id === 'all') return baseFilteredMods.length;
                    if (showSubfolderMods) {
                      return baseFilteredMods.filter(m =>
                        m.folder_id === id ||
                        (m.folder_id && m.folder_id.startsWith(id + '/'))
                      ).length;
                    } else {
                      return baseFilteredMods.filter(m =>
                        m.folder_id === id
                      ).length;
                    }
                  }}
                  hasFilters={selectedCharacters.size > 0 || selectedCategories.size > 0}
                />
              </div>
            </div>

            {/* Center Panel - Mod List */}
            <div className="center-panel" data-tour="mod-list">
              <div className="center-header">
                <div className="header-title">
                  <h2>
                    {selectedFolderId === 'all' ? 'All Mods' :
                      folders.find(f => f.id === selectedFolderId)?.name || 'Unknown Folder'}
                    <span className="mod-count">
                      ({filteredMods.filter(m => m.enabled).length}/{filteredMods.length} enabled)
                    </span>
                  </h2>
                </div>
                <div className="header-actions" data-tour="header-actions">
                  <button onClick={handleCheckClashes} className="btn-ghost btn-check-conflicts" title="Check for conflicts">
                    <IoMdWarning className="warning-icon" style={{ color: 'var(--accent-primary)', width: '18px', height: '18px' }} /> Check Conflicts
                  </button>
                  <div className="divider-vertical" />
                  <div className="view-switcher">
                    <button
                      onClick={() => handleViewModeChange('grid')}
                      className={`btn-icon-small ${viewMode === 'grid' ? 'active' : ''}`}
                      title="Grid View"
                    >
                      <GridViewIcon fontSize="small" />
                    </button>
                    <button
                      onClick={() => handleViewModeChange('compact')}
                      className={`btn-icon-small ${viewMode === 'compact' ? 'active' : ''}`}
                      title="Compact Grid View"
                    >
                      <ViewModuleIcon fontSize="small" />
                    </button>
                    <button
                      onClick={() => handleViewModeChange('list')}
                      className={`btn-icon-small ${viewMode === 'list' ? 'active' : ''}`}
                      title="List View"
                    >
                      <ViewListIcon fontSize="small" />
                    </button>
                    {showExperimental && (
                      <button
                        onClick={() => handleViewModeChange('list-compact')}
                        className={`btn-icon-small ${viewMode === 'list-compact' ? 'active' : ''}`}
                        title="Compact List View (Experimental)"
                      >
                        <ViewHeadlineIcon fontSize="small" />
                      </button>
                    )}
                  </div>
                  <div className="divider-vertical" />
                  <button
                    onClick={toggleRightPanel}
                    className={`btn-ghost ${!isRightPanelOpen ? 'active' : ''}`}
                    title={isRightPanelOpen ? "Collapse Details" : "Expand Details"}
                  >
                    <ViewSidebarIcon fontSize="small" style={{ transform: isRightPanelOpen ? 'none' : 'rotate(180deg)' }} />
                  </button>
                  <div className="divider-vertical" />
                  <button onClick={loadMods} className="btn-ghost" title="Refresh list">
                    <RefreshIcon fontSize="small" />
                  </button>
                </div>
              </div>

              {/* Bulk Actions Toolbar */}
              <div className={`bulk-actions-toolbar ${selectedMods.size === 0 ? 'inactive' : ''}`}>
                <div className="selection-info" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <span>{selectedMods.size} selected</span>
                  <button onClick={() => {
                    const allPaths = filteredMods.map(m => m.path)
                    setSelectedMods(new Set(allPaths))
                  }} className="btn-ghost" style={{ padding: '4px 12px', height: '32px' }}>Select All</button>
                  <button onClick={handleDeselectAll} className="btn-ghost" style={{ padding: '4px 12px', height: '32px' }}>Clear</button>
                </div>
                <div className="bulk-controls">
                  <div style={{ width: '200px', height: '40px' }}>
                    <CustomDropdown
                      icon={<MdDriveFileMoveOutline style={{ fontSize: '1.2rem', opacity: 0.7 }} />}
                      options={[
                        { value: 'root', label: 'Root (~mods)' }, // Option to move back to root
                        ...folders.filter(f => f.name !== '~mods').map(f => ({ value: f.id, label: f.name }))
                      ]}
                      value="" // Always reset after selection locally handled by onChange logic below
                      onChange={(val) => {
                        if (!val) return
                        if (val === 'root') handleAssignToFolder(null) // Handle move to root
                        else handleAssignToFolder(val)
                      }}
                      placeholder="Move to..."
                      disabled={selectedMods.size === 0}
                    />
                  </div>

                  <div style={{ width: '200px', height: '40px', marginLeft: '4px' }}>
                    <CustomDropdown
                      icon={<MdLocalOffer style={{ fontSize: '1.2rem', opacity: 0.7 }} />}
                      options={[
                        { value: '_new_tag_', label: '+ New Tag...' },
                        ...(allTags || []).map(t => ({ value: t, label: t }))
                      ]}
                      value="" 
                      onChange={(val) => {
                        if (!val) return
                        if (val === '_new_tag_') {
                          setNewTagPrompt({ callback: async (tag) => {
                            if (tag) {
                              try {
                                for (const modPath of selectedMods) {
                                  await invoke('add_custom_tag', { modPath, tag })
                                }
                                setStatus(`Assigned new tag "${tag}" to ${selectedMods.size} mod(s)`)
                                await loadMods()
                                await loadTags()
                              } catch (e) {
                                setStatus(`Error: ${e}`)
                              }
                            }
                          }})
                        } else {
                          handleBulkToggleTag(val)
                        }
                      }}
                      placeholder="Manage Tags..."
                      disabled={selectedMods.size === 0}
                    />
                  </div>

                  {(() => {
                    const selMods = mods.filter(m => selectedMods.has(m.path))
                    const enabledCount = selMods.filter(m => m.enabled).length
                    const disabledCount = selMods.length - enabledCount
                    const allEnabled = disabledCount === 0
                    const allDisabled = enabledCount === 0
                    return (
                      <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                        <button
                          onClick={() => handleBulkToggle(true)}
                          className="btn-ghost"
                          style={{ height: '40px', opacity: allEnabled ? 0.4 : 1 }}
                          disabled={selectedMods.size === 0 || allEnabled}
                          title={allEnabled ? 'All selected mods are already enabled' : `Enable ${disabledCount} disabled mod${disabledCount !== 1 ? 's' : ''}`}
                        >
                          <ToggleOnIcon fontSize="small" style={{ color: accentColor }} />
                          {disabledCount > 0 ? `Enable (${disabledCount})` : 'Enable'}
                        </button>
                        <button
                          onClick={() => handleBulkToggle(false)}
                          className="btn-ghost"
                          style={{ height: '40px', opacity: allDisabled ? 0.4 : 1 }}
                          disabled={selectedMods.size === 0 || allDisabled}
                          title={allDisabled ? 'All selected mods are already disabled' : `Disable ${enabledCount} enabled mod${enabledCount !== 1 ? 's' : ''}`}
                        >
                          <ToggleOffIcon fontSize="small" style={{ opacity: 0.7 }} />
                          {enabledCount > 0 ? `Disable (${enabledCount})` : 'Disable'}
                        </button>
                      </div>
                    )
                  })()}

                  <div
                    className={`btn-ghost danger ${isDeletingBulk ? 'holding' : ''}`}
                    onMouseDown={handleBulkDeleteDown}
                    onMouseUp={handleBulkDeleteUp}
                    onMouseLeave={handleBulkDeleteUp}
                    style={{ marginLeft: '1rem', height: '40px' }}
                    title={holdToDelete ? "Hold 2s to delete selected mods" : "Click to delete selected mods"}
                  >
                    <div className="danger-bg" />
                    <span style={{ position: 'relative', zIndex: 2, display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                      <RiDeleteBin2Fill />
                      {`Delete (${selectedMods.size})`}
                    </span>
                  </div>
                </div>
              </div>

              <ModsList
                mods={filteredMods}
                viewMode={viewMode}
                selectedMod={selectedMod}
                selectedMods={selectedMods}
                onSelect={handleModSelect}
                onToggleSelection={handleToggleModSelection}
                onToggleMod={handleToggleMod}
                onDeleteMod={handleDeleteMod}
                onRemoveTag={handleRemoveTag}
                onSetPriority={handleSetPriority}
                onContextMenu={handleContextMenu}
                hideSuffix={hideSuffix}
                showHeroIcons={showHeroIcons}
                showHeroBg={showHeroBg}
                showModType={showModType}
                modDetails={modDetails}
                characterData={characterData}
                onRename={handleRenameMod}
                onCheckConflicts={handleCheckSingleModClashes}
                renamingModPath={renamingModPath}
                onClearRenaming={() => setRenamingModPath(null)}
                gridRef={modsGridRef}
                gameRunning={gameRunning && !bypassGameRunningLock}
                holdToDelete={holdToDelete}
                onRenameBlocked={() => alert.warning(
                  'Game Running',
                  'Cannot rename mods while game is running.'
                )}
                onDeleteBlocked={() => alert.warning(
                  'Game Running',
                  'Cannot delete mods while game is running.'
                )}
              />
            </div>
          </motion.div>

          {/* Resize Handle */}
          <motion.div
            className="resize-handle"
            onMouseDown={handleResizeStart}
            animate={{ left: `${leftPanelWidth}%` }}
            transition={isResizing ? { duration: 0 } : { type: "tween", ease: "circOut", duration: 0.35 }}
          />

          {/* Right Panel - Mod Details (Always Visible) */}
          <motion.div
            className="right-panel"
            animate={{ width: `${100 - leftPanelWidth}%` }}
            transition={isResizing ? { duration: 0 } : { type: "tween", ease: "circOut", duration: 0.35 }}
          >
            {selectedMod ? (
              <div className="mod-details-and-contents">
                <div className="mod-details-wrapper">
                  <ModDetailsPanel
                    mod={selectedMod}
                    initialDetails={modDetails[selectedMod.path]}
                    onClose={() => setSelectedMod(null)}
                    characterData={characterData}
                    onUpdateMod={() => handleInitiateUpdate(selectedMod)}
                  />
                </div>

                <div className="selected-mod-contents">
                  <h3>Contents</h3>
                  <FileTree files={selectedMod.file_contents || selectedMod.files || selectedMod.file_list || []} />
                </div>
              </div>
            ) : (
              <div className="no-selection">
                <p>Select a mod to view details</p>
              </div>
            )}
          </motion.div>
        </div>
      </div >

      <LogDrawer
        status={status}
        logs={installLogs}
        onClear={() => setInstallLogs([])}
        progress={modLoadingProgress}
        isLoading={isModsLoading}
        isOpen={isLogDrawerOpen}
        onToggle={() => setIsLogDrawerOpen(v => !v)}
      />

      {
        contextMenu && (
          <ContextMenu
            x={contextMenu.x}
            y={contextMenu.y}
            mod={contextMenu.mod}
            folder={contextMenu.folder}
            onClose={closeContextMenu}
            onAssignTag={(tag) => contextMenu.mod && handleAddTagToSingleMod(contextMenu.mod.path, tag)}
            onNewTag={(callback) => setNewTagPrompt({ callback })}
            onMoveTo={(folderId) => contextMenu.mod && handleMoveSingleMod(contextMenu.mod.path, folderId)}
            onMoveFolderTo={(newParentId) => contextMenu.folder && handleMoveFolder(contextMenu.folder.id, newParentId)}
            onCreateFolder={handleCreateFolder}
            folders={folders}
            onDelete={() => {
              if (contextMenu.folder) {
                handleDeleteFolder(contextMenu.folder.id)
              } else if (contextMenu.mod) {
                handleDeleteMod(contextMenu.mod.path)
              }
            }}
            onToggle={() => contextMenu.mod && handleToggleMod(contextMenu.mod.path)}
            onRename={() => {
              if (contextMenu.mod) {
                if (gameRunning && !bypassGameRunningLock) {
                  alert.warning(
                    'Game Running',
                    'Cannot rename mods while game is running.'
                  )
                  return
                }
                setRenamingModPath(contextMenu.mod.path)
              }
            }}
            onRenameFolder={() => {
              if (contextMenu.folder) {
                handleRenameFolder(contextMenu.folder.id, contextMenu.folder.name)
              }
            }}
            onCheckConflicts={() => contextMenu.mod && handleCheckSingleModClashes(contextMenu.mod)}
            onUpdateMod={() => contextMenu.mod && handleInitiateUpdate(contextMenu.mod)}
            onSendToVfxUpdater={() => contextMenu.mod && handleSendToVfxUpdater(contextMenu.mod)}
            onExtractAssets={handleExtractAssets}
            allTags={allTags}
            onDeleteTag={handleDeleteTagFromCatalog}
            gamePath={gamePath}
            holdToDelete={holdToDelete}
          />
        )
      }

      <ShortcutsHelpModal
        isOpen={panels.shortcuts}
        onClose={() => setPanel('shortcuts', false)}
      />

      <OnboardingTour
        isOpen={showOnboarding}
        onClose={handleCloseTour}
      />
    </div >
  )
}

// Wrap App with AlertProvider
function AppWithAlerts() {
  return (
    <AlertProvider>
      <App />
    </AlertProvider>
  );
}

export default AppWithAlerts
