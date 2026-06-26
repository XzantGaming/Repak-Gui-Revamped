import React, { useState, useEffect, useMemo } from 'react';
import { motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-shell';
import { IoMdRefresh, IoIosSkipForward } from "react-icons/io";
import { RiFileZipFill } from "react-icons/ri";
import { MdRemoveModerator } from "react-icons/md";
import Switch from './ui/Switch';
import Progress from './ui/Progress';
import './SettingsPanel.css'; // Reuse the same styles

type ModRecord = {
    path: string;
    custom_name?: string;
    enabled?: boolean;
};

type RecompressProgress = {
    current: number;
    total: number;
};

type ToolsPanelProps = {
    onClose: () => void;
    mods?: ModRecord[];
    onToggleMod?: (modPath: string) => void;
};

export default function ToolsPanel({ onClose, mods = [], onToggleMod }: ToolsPanelProps) {
    const [isUpdatingChars, setIsUpdatingChars] = useState(false);
    const [charUpdateStatus, setCharUpdateStatus] = useState('');
    const [isSkippingLauncher, setIsSkippingLauncher] = useState(false);
    const [skipLauncherStatus, setSkipLauncherStatus] = useState('');
    const [isLauncherPatchEnabled, setIsLauncherPatchEnabled] = useState(false);
    const [isTogglingSigBypasser, setIsTogglingSigBypasser] = useState(false);
    const [sigBypasserStatusMsg, setSigBypasserStatusMsg] = useState('');
    const [sigBypasserState, setSigBypasserState] = useState<string>('NotInstalled');
    const [isRecompressing, setIsRecompressing] = useState(false);
    const [recompressStatus, setRecompressStatus] = useState('');
    const [recompressResult, setRecompressResult] = useState<any | null>(null);
    const [recompressProgress, setRecompressProgress] = useState<RecompressProgress>({ current: 0, total: 0 });
    const [showThanosSnap, setShowThanosSnap] = useState<number | null>(null); // null or timestamp for cache-busting
    const [thanosIsFading, setThanosIsFading] = useState(false);

    // Find LOD Disabler mod - prioritize bundled mod in _LOD-Disabler folder
    const lodDisablerMod = useMemo(() => {
        // First look for the bundled mod in the special folder
        const bundledMod = mods.find(mod => {
            const modPath = mod.path?.toLowerCase() || '';
            return modPath.includes('_lod-disabler') && modPath.includes('lods_disabler');
        });
        if (bundledMod) return bundledMod;

        // Fallback to any LOD disabler mod
        return mods.find(mod => {
            const modName = mod.custom_name || mod.path?.split(/[/\\]/).pop() || '';
            return modName.toLowerCase().includes('lods_disabler') ||
                mod.path?.toLowerCase().includes('lods_disabler');
        });
    }, [mods]);

    // Check if this is the bundled mod
    const isBundledMod = useMemo(() => {
        if (!lodDisablerMod) return false;
        const modPath = lodDisablerMod.path?.toLowerCase() || '';
        return modPath.includes('_lod-disabler');
    }, [lodDisablerMod]);

    // Get display name for LOD Disabler mod
    const lodModDisplayName = useMemo(() => {
        if (!lodDisablerMod) return '';
        if (isBundledMod) return 'LOD Disabler (Built-in)';
        return lodDisablerMod.custom_name || lodDisablerMod.path?.split(/[/\\]/).pop() || 'Unknown';
    }, [lodDisablerMod, isBundledMod]);

    // Check skip launcher status on mount
    useEffect(() => {
        const checkStatus = async () => {
            try {
                const isEnabled = await invoke('get_skip_launcher_status') as any;
                setIsLauncherPatchEnabled(isEnabled);
            } catch (error) {
                console.error('Failed to check skip launcher status:', error);
            }
        };
        checkStatus();
    }, []);

    // Check Sig Bypasser status on mount
    useEffect(() => {
        const checkSigStatus = async () => {
            try {
                const status = await invoke('get_sig_bypasser_status') as string;
                setSigBypasserState(status);
            } catch (error) {
                console.error('Failed to check sig bypasser status:', error);
            }
        };
        checkSigStatus();
    }, []);

    // Clear skip launcher status after 5 seconds
    useEffect(() => {
        if (skipLauncherStatus) {
            const timer = setTimeout(() => {
                setSkipLauncherStatus('');
            }, 5000);
            return () => clearTimeout(timer);
        }
    }, [skipLauncherStatus]);

    // Clear sig bypasser status msg after 5 seconds
    useEffect(() => {
        if (sigBypasserStatusMsg) {
            const timer = setTimeout(() => {
                setSigBypasserStatusMsg('');
            }, 5000);
            return () => clearTimeout(timer);
        }
    }, [sigBypasserStatusMsg]);

    // Clear char update status after 5 seconds
    useEffect(() => {
        if (charUpdateStatus) {
            const timer = setTimeout(() => {
                setCharUpdateStatus('');
            }, 5000);
            return () => clearTimeout(timer);
        }
    }, [charUpdateStatus]);

    // Clear recompress status after 5 seconds
    useEffect(() => {
        if (recompressStatus && !isRecompressing) {
            const timer = setTimeout(() => {
                setRecompressStatus('');
            }, 5000);
            return () => clearTimeout(timer);
        }
    }, [recompressStatus, isRecompressing]);

    // Listen for recompress progress events
    useEffect(() => {
        const unlisten = listen('recompress_progress', (event) => {
            const { current, total, status } = event.payload as any;
            setRecompressProgress({ current, total });
            setRecompressStatus(`${status} (${current}/${total})`);
        });

        return () => {
            unlisten.then(f => f());
        };
    }, []);

    const handleUpdateCharacterData = async () => {
        setIsUpdatingChars(true);
        setCharUpdateStatus('Updating...');
        try {
            const count = await invoke('update_character_data_from_github') as any;
            setCharUpdateStatus(`Successfully updated! ${count} new skins added.`);
        } catch (error) {
            setCharUpdateStatus(`Error: ${error}`);
        } finally {
            setIsUpdatingChars(false);
        }
    };

    const handleSkipLauncherPatch = async () => {
        setIsSkippingLauncher(true);
        setSkipLauncherStatus('');
        try {
            // Toggle the skip launcher patch
            const isEnabled = await invoke('skip_launcher_patch') as any;
            setIsLauncherPatchEnabled(isEnabled);
            setSkipLauncherStatus(
                isEnabled
                    ? 'Skip launcher enabled (launch_record = 0)'
                    : 'Skip launcher disabled (launch_record = 6)'
            );
        } catch (error) {
            setSkipLauncherStatus(`Error: ${error}`);
        } finally {
            setIsSkippingLauncher(false);
        }
    };

    const handleToggleSigBypasser = async () => {
        setIsTogglingSigBypasser(true);
        setSigBypasserStatusMsg('');
        try {
            const newStatus = await invoke('toggle_sig_bypasser') as string;
            setSigBypasserState(newStatus);
            setSigBypasserStatusMsg(
                newStatus === 'Enabled'
                    ? 'Sig Bypasser enabled'
                    : 'Sig Bypasser disabled'
            );
        } catch (error) {
            setSigBypasserStatusMsg(`Error: ${error}`);
        } finally {
            setIsTogglingSigBypasser(false);
        }
    };

    const handleReCompress = async () => {
        setIsRecompressing(true);
        setRecompressStatus('Scanning mods...');
        setRecompressResult(null);
        try {
            const result = await invoke('recompress_mods') as any;
            setRecompressResult(result);
            if (result.recompressed > 0) {
                setRecompressStatus(`Recompressed ${result.recompressed} mod(s)! (${result.already_oodle} already compressed)`);
            } else if (result.already_oodle === result.total_scanned) {
                setRecompressStatus('All mods already use Oodle compression');
            } else if (result.total_scanned === 0) {
                setRecompressStatus('No mods found to scan');
            } else {
                setRecompressStatus(`Scanned ${result.total_scanned} mods - ${result.already_oodle} already compressed`);
            }
        } catch (error) {
            setRecompressStatus(`Error: ${error}`);
        } finally {
            setIsRecompressing(false);
            setRecompressProgress({ current: 0, total: 0 });
        }
    };

    return (
        <>
            <div className="modal-overlay" onClick={onClose}>
                <motion.div
                    className="modal-content settings-modal"
                    onClick={(e) => e.stopPropagation()}
                    initial={{ opacity: 0, scale: 0.95 }}
                    animate={{ opacity: 1, scale: 1 }}
                    transition={{ duration: 0.15 }}
                >
                    <div className="modal-header">
                        <h2>Tools</h2>
                        <button className="modal-close" onClick={onClose}>×</button>
                    </div>

                    <div className="modal-body">
                        <div className="setting-section">
                            <h3>Sig Bypasser</h3>
                            <div className="setting-group">
                                <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                    Enables or disables the signature checks bypass.
                                </p>
                                <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center' }}>
                                    <button
                                        onClick={handleToggleSigBypasser}
                                        disabled={isTogglingSigBypasser || sigBypasserState === 'NotInstalled'}
                                        style={{
                                            display: 'flex',
                                            alignItems: 'center',
                                            gap: '6px',
                                            opacity: (isTogglingSigBypasser || sigBypasserState === 'NotInstalled') ? 0.5 : 1,
                                            cursor: (isTogglingSigBypasser || sigBypasserState === 'NotInstalled') ? 'not-allowed' : 'pointer'
                                        }}
                                    >
                                        <MdRemoveModerator size={16} />
                                        {isTogglingSigBypasser ? 'Applying...' : 'Toggle Sig Bypasser'}
                                    </button>
                                    <span style={{
                                        display: 'inline-flex',
                                        alignItems: 'center',
                                        gap: '0.4rem',
                                        fontSize: '0.85rem',
                                        fontWeight: 600,
                                        color: sigBypasserState === 'Enabled' ? '#4CAF50' : (sigBypasserState === 'Disabled' ? '#ff5252' : '#9e9e9e')
                                    }}>
                                        <span style={{
                                            width: '8px',
                                            height: '8px',
                                            borderRadius: '50%',
                                            backgroundColor: sigBypasserState === 'Enabled' ? '#4CAF50' : (sigBypasserState === 'Disabled' ? '#ff5252' : '#9e9e9e')
                                        }}></span>
                                        {sigBypasserState === 'Enabled' ? 'Enabled' : (sigBypasserState === 'Disabled' ? 'Disabled' : 'Not Installed')}
                                    </span>
                                </div>
                                {sigBypasserStatusMsg && (
                                    <p style={{
                                        fontSize: '0.85rem',
                                        marginTop: '0.5rem',
                                        color: sigBypasserStatusMsg.includes('Error') ? '#ff5252' : '#4CAF50'
                                    }}>
                                        {sigBypasserStatusMsg}
                                    </p>
                                )}
                            </div>
                        </div>

                        <div className="setting-section">
                            <h3>Skip Launcher Patch</h3>
                            <div className="setting-group">
                                <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                    Sets <b>launch_record</b> value to 0.
                                </p>
                                <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center' }}>
                                    <button
                                        onClick={handleSkipLauncherPatch}
                                        disabled={isSkippingLauncher}
                                        style={{
                                            display: 'flex',
                                            alignItems: 'center',
                                            gap: '6px',
                                            opacity: isSkippingLauncher ? 0.5 : 1,
                                            cursor: isSkippingLauncher ? 'not-allowed' : 'pointer'
                                        }}
                                    >
                                        <IoIosSkipForward size={16} />
                                        {isSkippingLauncher ? 'Applying...' : 'Skip Launcher Patch'}
                                    </button>
                                    <span style={{
                                        display: 'inline-flex',
                                        alignItems: 'center',
                                        gap: '0.4rem',
                                        fontSize: '0.85rem',
                                        fontWeight: 600,
                                        color: isLauncherPatchEnabled ? '#4CAF50' : '#ff5252'
                                    }}>
                                        <span style={{
                                            width: '8px',
                                            height: '8px',
                                            borderRadius: '50%',
                                            backgroundColor: isLauncherPatchEnabled ? '#4CAF50' : '#ff5252'
                                        }}></span>
                                        {isLauncherPatchEnabled ? 'Enabled' : 'Disabled'}
                                    </span>
                                </div>
                                {skipLauncherStatus && (
                                    <p style={{
                                        fontSize: '0.85rem',
                                        marginTop: '0.5rem',
                                        color: skipLauncherStatus.includes('Error') ? '#ff5252' : '#4CAF50'
                                    }}>
                                        {skipLauncherStatus}
                                    </p>
                                )}
                            </div>
                        </div>

                        <div className="setting-section">
                            <h3>Character Database</h3>
                            <div className="setting-group">
                                <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                    Update the character database from GitHub to support new heroes and skins.
                                </p>
                                <div style={{ display: 'flex', gap: '0.5rem' }}>
                                    <button
                                        onClick={handleUpdateCharacterData}
                                        disabled={isUpdatingChars}
                                        style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
                                    >
                                        <IoMdRefresh size={18} className={isUpdatingChars ? 'spin-animation' : ''} />
                                        {isUpdatingChars ? 'Updating...' : 'Update Heroes Database'}
                                    </button>
                                </div>
                                {charUpdateStatus && (
                                    <p style={{
                                        fontSize: '0.85rem',
                                        marginTop: '0.5rem',
                                        color: charUpdateStatus.includes('Error') || charUpdateStatus.includes('Cancelled') ? '#ff5252' : '#4CAF50'
                                    }}>
                                        {charUpdateStatus}
                                    </p>
                                )}
                                <p style={{ fontSize: '0.75rem', opacity: 0.5, marginTop: '0.5rem' }}>
                                    Database maintained by{' '}
                                    <span
                                        style={{ textDecoration: 'underline', cursor: 'pointer' }}
                                        onClick={() => open('https://github.com/donutman07/MarvelRivalsCharacterIDs')}
                                    >
                                        donutman07
                                    </span>
                                </p>
                            </div>
                        </div>

                        <div className="setting-section">
                            <h3>ReCompress</h3>
                            <div className="setting-group">
                                <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                    Apply Oodle compression to all IOStore bundles paked with the old Repak GUI.
                                </p>
                                <div style={{ display: 'flex', gap: '0.5rem' }}>
                                    <button
                                        onClick={handleReCompress}
                                        disabled={isRecompressing}
                                        style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
                                    >
                                        <RiFileZipFill size={16} className={isRecompressing ? 'spin-animation' : ''} />
                                        {isRecompressing ? 'Scanning...' : 'ReCompress'}
                                    </button>
                                </div>
                                {isRecompressing && recompressProgress.total > 0 && (
                                    <div style={{ marginTop: '0.75rem' }}>
                                        <Progress
                                            value={recompressProgress.current}
                                            maxValue={recompressProgress.total}
                                            size="md"
                                            color="primary"
                                            showValueLabel
                                            isStriped
                                        />
                                    </div>
                                )}
                                {recompressStatus && (
                                    <p style={{
                                        fontSize: '0.85rem',
                                        marginTop: '0.5rem',
                                        color: recompressStatus.includes('Error') ? '#ff5252' : '#4CAF50'
                                    }}>
                                        {recompressStatus}
                                    </p>
                                )}
                            </div>
                        </div>

                        <div className="setting-section">
                            <h3>Character LODs Thanos</h3>
                            <div className="setting-group">
                                {lodDisablerMod ? (
                                    <>
                                        <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                            Disable character LODs to prevent texture mods from being reverted to vanilla textures from a far distance.
                                        </p>
                                        <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center' }}>
                                            <Switch
                                                checked={lodDisablerMod.enabled}
                                                onChange={() => {
                                                    const pathStr = typeof lodDisablerMod.path === 'string'
                                                        ? lodDisablerMod.path
                                                        : String(lodDisablerMod.path);
                                                    console.log('Toggling LOD mod:', pathStr);

                                                    // Show Thanos snap when ENABLING (currently disabled)
                                                    if (!lodDisablerMod.enabled) {
                                                        const timestamp = Date.now();
                                                        setThanosIsFading(false);
                                                        setShowThanosSnap(timestamp);
                                                        // Timer starts in onLoad handler after GIF is loaded
                                                    }

                                                    onToggleMod?.(pathStr);
                                                }}
                                            />
                                            <span style={{ fontSize: '0.9rem' }}>
                                                {lodDisablerMod.enabled ? 'LODs Disabled (Mod enabled)' : 'LODs Enabled (Default: best performance)'}
                                            </span>
                                        </div>
                                        <p style={{ fontSize: '0.75rem', opacity: 0.5, marginTop: '0.5rem' }}>
                                            {isBundledMod ? 'Built-in mod (auto-deployed)' : `Mod: ${lodModDisplayName}`}
                                        </p>
                                    </>
                                ) : (
                                    <>
                                        <p style={{ fontSize: '0.9rem', opacity: 0.7, marginBottom: '0.5rem' }}>
                                            LOD Disabler not found. This mod is bundled with the app and should be auto-deployed when you set a valid mods folder.
                                            If missing, try re-selecting your mods folder.
                                        </p>
                                        <div style={{ display: 'flex', gap: '0.75rem', alignItems: 'center', opacity: 0.5 }}>
                                            <Switch
                                                checked={false}
                                                onChange={() => { }}
                                                isDisabled={true}
                                            />
                                            <span style={{ fontSize: '0.9rem' }}>LOD Thanos (Not Available)</span>
                                        </div>
                                    </>
                                )}
                            </div>
                        </div>
                    </div>

                    <div className="modal-footer" style={{ gap: '0.5rem' }}>
                        <button
                            onClick={onClose}
                            className="btn-primary"
                            style={{ padding: '0.4rem 1rem', fontSize: '0.9rem', minWidth: 'auto' }}
                        >
                            Close
                        </button>
                    </div>
                </motion.div>
            </div>

            {/* Thanos Snap Easter Egg */}
            {
                showThanosSnap && (
                    <div
                        style={{
                            position: 'fixed',
                            inset: 0,
                            zIndex: 9999999,
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            background: 'rgba(0, 0, 0, 0.7)',
                            backdropFilter: 'blur(4px)',
                            opacity: thanosIsFading ? 0 : 1,
                            transition: 'opacity 0.5s ease-out'
                        }}
                        onClick={() => setShowThanosSnap(null)}
                    >
                        <img
                            key={showThanosSnap}
                            src={`https://i.imgur.com/RsIL6sH.gif?t=${showThanosSnap}`}
                            alt="Thanos Snap"
                            onLoad={() => {
                                // Start fade-out timer only after GIF is fully loaded
                                setTimeout(() => setThanosIsFading(true), 1250);
                                setTimeout(() => {
                                    setShowThanosSnap(null);
                                    setThanosIsFading(false);
                                }, 2100);
                            }}
                            style={{
                                maxWidth: '80%',
                                maxHeight: '80%',
                                borderRadius: '12px',
                                boxShadow: '0 0 60px rgba(185, 185, 185, 0.5)'
                            }}
                        />
                    </div>
                )}
        </>
    );
}

