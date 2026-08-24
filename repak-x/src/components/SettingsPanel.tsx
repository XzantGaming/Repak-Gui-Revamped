import React, { useState, useEffect, useMemo } from 'react';
import { AnimatedThemeToggler } from './ui/AnimatedThemeToggler'
import Switch from './ui/Switch'
import Checkbox from './ui/Checkbox'
import TailInput from './ui/TailInput'
import { LuFolderInput } from "react-icons/lu"
import { RiSparkling2Fill } from "react-icons/ri"
import { CgPerformance } from "react-icons/cg"
import {
  MdRefresh,
  MdArticle,
  MdTune,
  MdPalette,
  MdExtension,
  MdHelpOutline
} from "react-icons/md"
import { FaDiscord, FaSteam } from "react-icons/fa"
import { SiEpicgames } from "react-icons/si"
import { RiGraduationCapFill } from "react-icons/ri"
import { BsKeyboardFill } from "react-icons/bs"
import { IoIosSettings } from "react-icons/io"
import DiscordWidget from './DiscordWidget'
import './SettingsPanel.css'
import { useAlert } from './AlertHandler'
import { motion } from 'framer-motion'

const ACCENT_COLORS = {
  repakRed: '#be1c1c',
  blue: '#4a9eff',
  purple: '#9c27b0',
  green: '#4CAF50',
  orange: '#ff9800',
  pink: '#FF96BC'
};

const CATEGORIES = [
  { id: 'general', label: 'General', icon: MdTune },
  { id: 'interface', label: 'Interface', icon: MdPalette },
  { id: 'integrations', label: 'Integrations', icon: MdExtension },
  { id: 'help', label: 'Help & Community', icon: MdHelpOutline }
] as const;

type CategoryId = typeof CATEGORIES[number]['id'];

type SettingsPayload = {
  hideSuffix: boolean;
  autoOpenDetails: boolean;
  showHeroIcons: boolean;
  showHeroBg: boolean;
  showModType: boolean;
  showExperimental: boolean;
  autoCheckUpdates: boolean;
  parallelProcessing: boolean;
  enableDrp: boolean;
  holdToDelete: boolean;
  showSubfolderMods: boolean;
  bypassGameRunningLock: boolean;
  launcherType: 'steam' | 'epic';
};

type SettingsPanelProps = {
  settings: Partial<SettingsPayload>;
  onSave: (settings: SettingsPayload) => void;
  onClose: () => void;
  theme: string;
  setTheme: (theme: string) => void;
  accentColor: string;
  setAccentColor: (accent: string) => void;
  gamePath?: string;
  onAutoDetectGamePath: () => void;
  onBrowseGamePath: () => void;
  isGamePathLoading: boolean;
  setParallelProcessing: (enabled: boolean) => void;
  onCheckForUpdates: () => void;
  onViewChangelog: () => void;
  isCheckingUpdates: boolean;
  onReplayTour: () => void;
  onOpenShortcuts: () => void;
  changelogMarker?: { lastSeen: string; current: string; recorded: boolean } | null;
};


export default function SettingsPanel({ settings, onSave, onClose, theme, setTheme, accentColor, setAccentColor, gamePath, onAutoDetectGamePath, onBrowseGamePath, isGamePathLoading, setParallelProcessing, onCheckForUpdates, onViewChangelog, isCheckingUpdates, onReplayTour, onOpenShortcuts, changelogMarker = null }: SettingsPanelProps) {
  const alert = useAlert();
  const [activeCategory, setActiveCategory] = useState<CategoryId>('general');
  const [hideSuffix, setHideSuffix] = useState(settings.hideSuffix || false);
  const [autoOpenDetails, setAutoOpenDetails] = useState(settings.autoOpenDetails || false);
  const [showHeroIcons, setShowHeroIcons] = useState(settings.showHeroIcons || false);
  const [showHeroBg, setShowHeroBg] = useState(settings.showHeroBg || false);
  const [showModType, setShowModType] = useState(settings.showModType || false);
  const [showExperimental, setShowExperimental] = useState(settings.showExperimental || false);
  const [autoCheckUpdates, setAutoCheckUpdates] = useState(settings.autoCheckUpdates || false);
  const [parallelProcessing, setLocalParallelProcessing] = useState(settings.parallelProcessing || false);
  const [holdToDelete, setHoldToDelete] = useState(settings.holdToDelete !== false);
  const [showSubfolderMods, setShowSubfolderMods] = useState(settings.showSubfolderMods !== false);
  const [bypassGameRunningLock, setBypassGameRunningLock] = useState(settings.bypassGameRunningLock || false);
  const [enableDrp, setEnableDrp] = useState(settings.enableDrp !== false);
  const [launcherType, setLauncherType] = useState<'steam' | 'epic'>(settings.launcherType || 'steam');
  const [showRatMode, setShowRatMode] = useState(false);

  // Shown only when the startup check disagreed with what was on disk, which is
  // the exact state that reopens the changelog every launch. This uses the
  // snapshot taken BEFORE the marker was rewritten -- comparing the live value
  // here would always look correct, since startup has already corrected it.
  const changelogStateMismatch = useMemo(() => {
    if (!changelogMarker) return null;
    const { lastSeen, current, recorded } = changelogMarker;
    if (recorded && lastSeen === current) return null;
    return { lastSeen: lastSeen || 'not recorded', current, recorded };
  }, [changelogMarker]);

  // Easter egg: briefly show "Rat Mode" when switching to light theme
  const handleThemeToggle = (newTheme: string) => {
    if (newTheme === 'light') {
      setShowRatMode(true);
      setTimeout(() => setShowRatMode(false), 300);
    }
    setTheme(newTheme);
  };

  const handleSave = () => {
    onSave({
      hideSuffix,
      autoOpenDetails,
      showHeroIcons,
      showHeroBg,
      showModType,
      showExperimental,
      autoCheckUpdates,
      parallelProcessing,
      enableDrp,
      holdToDelete,
      showSubfolderMods,
      bypassGameRunningLock,
      launcherType
    });
    alert.success('Settings Saved', 'Your preferences have been updated.');
    onClose();
  };

  // Sync local state with props when opening/changing
  useEffect(() => {
    if (settings.enableDrp !== undefined) {
      setEnableDrp(settings.enableDrp);
    }
  }, [settings.enableDrp]);

  useEffect(() => {
    setHoldToDelete(settings.holdToDelete !== false);
  }, [settings.holdToDelete]);

  useEffect(() => {
    setShowSubfolderMods(settings.showSubfolderMods !== false);
  }, [settings.showSubfolderMods]);

  useEffect(() => {
    setBypassGameRunningLock(settings.bypassGameRunningLock || false);
  }, [settings.bypassGameRunningLock]);

  useEffect(() => {
    if (settings.launcherType) {
      setLauncherType(settings.launcherType);
    }
  }, [settings.launcherType]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <motion.div
        className="modal-content settings-modal settings-modal-split"
        onClick={(e) => e.stopPropagation()}
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.15 }}
      >
        <div className="modal-header">
          <h2>
            <IoIosSettings size={20} /> Settings
          </h2>
          <button className="modal-close" onClick={onClose}>×</button>
        </div>

        <div className="settings-layout">
          <nav className="settings-sidebar" aria-label="Settings categories">
            {CATEGORIES.map(({ id, label, icon: Icon }) => (
              <button
                key={id}
                type="button"
                className={`settings-nav-item ${activeCategory === id ? 'active' : ''}`}
                onClick={() => setActiveCategory(id)}
                aria-current={activeCategory === id ? 'page' : undefined}
              >
                <Icon size={16} className="settings-nav-icon" />
                <span>{label}</span>
              </button>
            ))}
          </nav>

          <div className="modal-body settings-content" key={activeCategory}>
            {activeCategory === 'general' && (
              <>
                <div className="setting-section">
                  <h3>Game Mods Path</h3>
                  <div className="setting-group">
                    <p className="setting-hint">Your game's mods folder path.</p>
                    <div className="combined-input-group">
                      <TailInput
                        value={gamePath || ''}
                        placeholder="No game path set"
                        className="integrated-input"
                      />
                      <div className="input-actions">
                        <button
                          onClick={onAutoDetectGamePath}
                          disabled={isGamePathLoading}
                          className="action-btn"
                          title="Auto Detect Game Path"
                        >
                          <RiSparkling2Fill />
                          {isGamePathLoading ? 'Detecting…' : 'Auto Detect'}
                        </button>
                        <button
                          onClick={onBrowseGamePath}
                          className="action-btn icon-only"
                          title="Browse Folder"
                        >
                          <LuFolderInput size={16} />
                        </button>
                      </div>
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Launcher</h3>
                  <div className="setting-group">
                    <p className="setting-hint">Select which launcher to use when clicking "Launch Game".</p>

                    <div className="segmented-control" style={{ maxWidth: '400px' }}>
                      <button
                        className={`segment-btn ${launcherType === 'steam' ? 'active' : ''}`}
                        onClick={() => setLauncherType('steam')}
                        title="Launch via Steam protocol"
                      >
                        <FaSteam size={18} /> Steam
                      </button>
                      <button
                        className={`segment-btn ${launcherType === 'epic' ? 'active' : ''}`}
                        onClick={() => setLauncherType('epic')}
                        title="Launch via Epic Games executable"
                      >
                        <SiEpicgames size={16} /> Epic Games
                      </button>
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Performance</h3>
                  <div className="setting-group">
                    <div className="setting-item">
                      <div className="setting-toggle-row">
                        <span className="setting-toggle-label">
                          <CgPerformance size={20} style={{ color: accentColor }} />
                          Parallel Processing Mode
                        </span>
                        <div className="setting-toggle-actions">
                          <span style={{
                            fontSize: '0.85rem',
                            opacity: parallelProcessing ? 1 : 0.8,
                            fontWeight: parallelProcessing ? '900' : '500',
                            fontStyle: parallelProcessing ? 'italic' : 'normal',
                            color: parallelProcessing ? accentColor : 'inherit',
                            textShadow: parallelProcessing ? '2px 2px 0px rgba(0,0,0,0.2)' : 'none',
                            transition: 'all 0.2s ease'
                          }}>
                            {parallelProcessing ? 'BOOST' : 'Normal'}
                          </span>
                          <Switch
                            checked={parallelProcessing}
                            onChange={(checked: boolean) => setLocalParallelProcessing(checked)}
                          />
                        </div>
                      </div>
                      <p className="setting-subtext">
                        {parallelProcessing
                          ? 'Boost mode uses 75% of available threads for backend operations.'
                          : 'Normal mode uses 50% of available threads for backend operations.'}
                      </p>
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Repak X Updates</h3>
                  <div className="setting-group">
                    <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', flexWrap: 'wrap' }}>
                      <button
                        onClick={onCheckForUpdates}
                        disabled={isCheckingUpdates}
                        className="action-btn"
                        title="Check for updates now"
                        style={{ minWidth: '140px' }}
                      >
                        <MdRefresh className={isCheckingUpdates ? 'spin-icon' : ''} />
                        {isCheckingUpdates ? 'Checking...' : 'Check Now'}
                      </button>
                      <button
                        onClick={onViewChangelog}
                        className="action-btn"
                        title="View changelog"
                        style={{ minWidth: '160px' }}
                      >
                        <MdArticle />
                        View Changelog
                      </button>
                      <span style={{ fontSize: '0.8rem', opacity: 0.6 }}>Current Version: v{typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0'}</span>
                    </div>

                    {/* Only rendered when the recorded version disagrees with the
                        running one, which is the exact state that makes the
                        changelog reappear on every launch. Release builds have no
                        devtools, so this is the only way to read it back. */}
                    {changelogStateMismatch && (
                      <p className="setting-subtext is-warning" style={{ paddingLeft: 0 }}>
                        Changelog state: startup saw "{changelogStateMismatch.lastSeen}", running
                        v{changelogStateMismatch.current}
                        {!changelogStateMismatch.recorded && ' (could not be saved)'}. Please report this.
                      </p>
                    )}

                    <Checkbox
                      checked={autoCheckUpdates}
                      onChange={(checked: boolean) => setAutoCheckUpdates(checked)}
                    >
                      <span>Auto-check for updates on startup</span>
                    </Checkbox>
                  </div>
                </div>
              </>
            )}

            {activeCategory === 'interface' && (
              <>
                <div className="setting-section">
                  <h3>Theme</h3>
                  <div className="setting-group">
                    <div style={{ display: 'flex', alignItems: 'center', gap: '1rem' }}>
                      <AnimatedThemeToggler theme={theme} setTheme={handleThemeToggle} />
                      <span style={{ fontSize: '0.9rem', opacity: 0.8 }}>
                        {theme === 'dark' ? 'Dark Mode' : (showRatMode ? 'Rat Mode 🐀' : 'Light Mode')}
                      </span>
                    </div>

                    <label style={{ display: 'block', fontSize: '0.9rem', opacity: 0.9 }}>Accent Color</label>
                    <div className="color-options">
                      {Object.entries(ACCENT_COLORS).map(([name, color]) => (
                        <button
                          key={name}
                          className={`color-option ${accentColor === color ? 'selected' : ''}`}
                          style={{ backgroundColor: color }}
                          onClick={() => setAccentColor(color)}
                          title={name.charAt(0).toUpperCase() + name.slice(1)}
                        />
                      ))}
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Mods View Settings</h3>
                  <div className="setting-group">
                    <div className="setting-item">
                      <Checkbox
                        checked={hideSuffix}
                        onChange={(checked: boolean) => setHideSuffix(checked)}
                      >
                        <span>Hide file suffix in mod names</span>
                      </Checkbox>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={autoOpenDetails}
                        onChange={(checked: boolean) => setAutoOpenDetails(checked)}
                      >
                        <span>Auto-open details panel on click</span>
                      </Checkbox>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={showHeroIcons}
                        onChange={(checked: boolean) => setShowHeroIcons(checked)}
                      >
                        <span>Show hero icons on mod cards</span>
                      </Checkbox>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={showHeroBg}
                        onChange={(checked: boolean) => setShowHeroBg(checked)}
                      >
                        <span>Show hero background on mod cards</span>
                      </Checkbox>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={showSubfolderMods}
                        onChange={(checked: boolean) => setShowSubfolderMods(checked)}
                      >
                        <span>Show mods from subfolders</span>
                      </Checkbox>
                      <p className="setting-subtext">
                        When enabled, selecting a folder also shows mods in its subfolders.
                      </p>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={showModType}
                        onChange={(checked: boolean) => setShowModType(checked)}
                      >
                        <span>Show mod type badge on cards</span>
                      </Checkbox>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={showExperimental}
                        onChange={(checked: boolean) => setShowExperimental(checked)}
                      >
                        <span>Enables "Compact List" view</span>
                      </Checkbox>
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Advanced UI Settings</h3>
                  <div className="setting-group">
                    <div className="setting-item">
                      <Checkbox
                        checked={holdToDelete}
                        onChange={(checked: boolean) => setHoldToDelete(checked)}
                      >
                        <span>Require hold to delete (2s)</span>
                      </Checkbox>
                      <p className={`setting-subtext ${!holdToDelete ? 'is-danger' : ''}`}>
                        {!holdToDelete
                          ? 'Deleting mods is irreversible. Mods will be removed instantly on click.'
                          : 'Hold the delete button for 2 seconds to confirm deletion.'}
                      </p>
                    </div>
                    <div className="setting-item">
                      <Checkbox
                        checked={bypassGameRunningLock}
                        onChange={(checked: boolean) => setBypassGameRunningLock(checked)}
                      >
                        <span>Bypass game-running operation lock</span>
                      </Checkbox>
                      <p className={`setting-subtext ${bypassGameRunningLock ? 'is-warning' : ''}`}>
                        {bypassGameRunningLock
                          ? 'Warning: Rename, move, toggle, delete, and priority actions will stay enabled while the game is running.'
                          : 'When disabled, mod operations are blocked while the game is running.'}
                      </p>
                    </div>
                  </div>
                </div>
              </>
            )}

            {activeCategory === 'integrations' && (
              <div className="setting-section">
                <h3>Integrations</h3>
                <div className="setting-group">
                  <div className="setting-item">
                    <div className="setting-toggle-row">
                      <span className="setting-toggle-label">
                        <FaDiscord size={20} style={{ color: '#5865F2' }} />
                        Enable Discord Rich Presence
                      </span>
                      <Switch
                        checked={enableDrp}
                        onChange={(checked: boolean) => setEnableDrp(checked)}
                      />
                    </div>
                    <p className="setting-subtext">
                      Show your active modding status on Discord.
                    </p>
                  </div>
                </div>
              </div>
            )}

            {activeCategory === 'help' && (
              <>
                <div className="setting-section">
                  <h3>Help</h3>
                  <div className="setting-group">
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '1rem' }}>
                      <span style={{ fontWeight: 'normal', opacity: 0.9 }}>Replay the app tour to learn about key features</span>
                      <button
                        onClick={onReplayTour}
                        className="action-btn"
                        title="Replay the onboarding tour"
                        style={{ minWidth: '120px' }}
                      >
                        <RiGraduationCapFill style={{ color: accentColor }} /> Replay Tour
                      </button>
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '1rem' }}>
                      <span style={{ fontSize: '1rem', opacity: 0.9 }}>
                        Press <strong style={{ opacity: 1 }}>F1</strong> anytime to view all available keyboard shortcuts
                      </span>
                      <button
                        onClick={onOpenShortcuts}
                        className="action-btn"
                        title="View keyboard shortcuts"
                        style={{ minWidth: '120px' }}
                      >
                        <BsKeyboardFill style={{ color: accentColor }} /> Shortcuts
                      </button>
                    </div>
                  </div>
                </div>

                <div className="setting-section">
                  <h3>Community</h3>
                  <div className="setting-group">
                    <div className="setting-item">
                      <p style={{ margin: 0, fontSize: '0.925rem', fontWeight: 600, opacity: 0.9 }}>
                        Repak X is built for the community.
                      </p>
                      <p className="setting-hint" style={{ marginBottom: 0 }}>
                        If you need help, want to report a bug, or have a feature request, join the Discord server and help make Repak X better for everyone.
                      </p>
                    </div>
                    <DiscordWidget />
                  </div>
                </div>
              </>
            )}
          </div>
        </div>

        <div className="modal-footer">
          <button
            onClick={onClose}
            className="btn-secondary"
            style={{ padding: '0.4rem 1rem', fontSize: '0.9rem', minWidth: 'auto' }}
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="btn-primary"
            style={{ padding: '0.4rem 1rem', fontSize: '0.9rem', minWidth: 'auto' }}
          >
            Save
          </button>
        </div>
      </motion.div>
    </div>
  )
}
