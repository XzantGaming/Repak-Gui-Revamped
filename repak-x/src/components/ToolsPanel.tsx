import React, { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-shell';
import { IoIosSkipForward } from "react-icons/io";
import { RiFileZipFill } from "react-icons/ri";
import { FaUsers } from "react-icons/fa";
import { FaToolbox } from "react-icons/fa6";
import { MdRemoveModerator } from "react-icons/md";
import { IconType } from 'react-icons';
import Progress from './ui/Progress';
import { uiLog } from '../utils/uiLog';
import './SettingsPanel.css'; // Reuse the same styles
import './ToolsPanel.css';

type RecompressProgress = {
    current: number;
    total: number;
};

type ToolsPanelProps = {
    onClose: () => void;
};

type StatusTone = 'on' | 'off' | 'unknown' | 'error';

type ToolCardConfig = {
    id: string;
    icon: IconType;
    title: string;
    description: string;
    onClick: () => void;
    busy: boolean;
    disabled?: boolean;
    /** Persistent state indicator, for the tools that have one */
    state?: { tone: StatusTone; label: string };
    /** Transient result message, takes over the status line while present */
    message?: string;
    progress?: RecompressProgress;
};

export default function ToolsPanel({ onClose }: ToolsPanelProps) {
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
            // The backend already logs its own failure detail; this covers the
            // case where the invoke itself never reached it.
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
            uiLog.info('Tools', `Skip launcher ${isEnabled ? 'enabled' : 'disabled'}`);
            setSkipLauncherStatus(
                isEnabled
                    ? 'Skip launcher enabled (launch_record = 0)'
                    : 'Skip launcher disabled (launch_record = 6)'
            );
        } catch (error) {
            uiLog.error('Tools', `Could not toggle skip launcher: ${error}`);
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
            uiLog.info('Tools', `Signature bypass is now ${newStatus.toLowerCase()}`);
            setSigBypasserStatusMsg(
                newStatus === 'Enabled'
                    ? 'Sig Bypasser enabled'
                    : 'Sig Bypasser disabled'
            );
        } catch (error) {
            uiLog.error('Tools', `Could not toggle the signature bypass: ${error}`);
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
            uiLog.error('Recompress', `Recompression failed: ${error}`);
            setRecompressStatus(`Error: ${error}`);
        } finally {
            setIsRecompressing(false);
            setRecompressProgress({ current: 0, total: 0 });
        }
    };

    const tools: ToolCardConfig[] = [
        {
            id: 'sig-bypasser',
            icon: MdRemoveModerator,
            title: 'Sig Bypasser',
            description: 'Toggles the signature checks bypass.',
            onClick: handleToggleSigBypasser,
            busy: isTogglingSigBypasser,
            disabled: isTogglingSigBypasser || sigBypasserState === 'NotInstalled',
            state: sigBypasserState === 'Enabled'
                ? { tone: 'on', label: 'Enabled' }
                : sigBypasserState === 'Disabled'
                    ? { tone: 'off', label: 'Disabled' }
                    : { tone: 'unknown', label: 'Not Installed' },
            message: sigBypasserStatusMsg
        },
        {
            id: 'skip-launcher',
            icon: IoIosSkipForward,
            title: 'Skip Launcher Patch',
            description: 'Sets the launch_record value to 0.',
            onClick: handleSkipLauncherPatch,
            busy: isSkippingLauncher,
            disabled: isSkippingLauncher,
            state: isLauncherPatchEnabled
                ? { tone: 'on', label: 'Enabled' }
                : { tone: 'off', label: 'Disabled' },
            message: skipLauncherStatus
        },
        {
            id: 'hero-database',
            icon: FaUsers,
            title: 'Heroes Database',
            description: 'Update from GitHub to support new heroes and skins.',
            onClick: handleUpdateCharacterData,
            busy: isUpdatingChars,
            disabled: isUpdatingChars,
            message: charUpdateStatus
        },
        {
            id: 'recompress',
            icon: RiFileZipFill,
            title: 'ReCompress',
            description: 'Apply Oodle compression to old mod bundles.',
            onClick: handleReCompress,
            busy: isRecompressing,
            disabled: isRecompressing,
            message: recompressStatus,
            progress: recompressProgress
        }
    ];

    const renderStatus = (tool: ToolCardConfig) => {
        // A tool with a real state always shows that state and nothing else.
        // It deliberately does not show a busy or result line: those flipped the
        // status to a third, transient value mid-toggle, which read as if the
        // tool had entered some other mode. The pulsing icon already signals
        // work in progress, and the state below settles on its own.
        if (tool.state) {
            return (
                <span className={`tool-card-status is-${tool.state.tone}`}>
                    <span className="tool-status-dot" />
                    {tool.state.label}
                </span>
            );
        }

        // Action tools have no steady state, so the result is their only feedback
        if (tool.message) {
            const isError = tool.message.includes('Error') || tool.message.includes('Cancelled');
            return (
                <span className={`tool-card-status ${isError ? 'is-error' : 'is-on'}`}>
                    <span className="tool-status-dot" />
                    {tool.message}
                </span>
            );
        }

        return null;
    };

    return (
        <>
            <div className="modal-overlay" onClick={onClose}>
                <motion.div
                    className="modal-content settings-modal tools-modal"
                    onClick={(e) => e.stopPropagation()}
                    initial={{ opacity: 0, scale: 0.95 }}
                    animate={{ opacity: 1, scale: 1 }}
                    transition={{ duration: 0.15 }}
                >
                    <div className="modal-header">
                        <h2>
                            <FaToolbox size={20} /> Tools
                        </h2>
                        <button className="modal-close" onClick={onClose}>×</button>
                    </div>

                    <div className="modal-body">
                        <div className="tools-grid">
                            {tools.map((tool) => {
                                const Icon = tool.icon;
                                const showProgress = tool.busy && !!tool.progress && tool.progress.total > 0;
                                return (
                                    // The progress bar sits beside the button, not inside it:
                                    // <button> takes phrasing content only, and Progress renders divs.
                                    <div className="tool-card-slot" key={tool.id}>
                                        <button
                                            className={`tool-card ${tool.busy ? 'is-busy' : ''}`}
                                            onClick={tool.onClick}
                                            disabled={tool.disabled}
                                        >
                                            <span className="tool-card-icon">
                                                <Icon />
                                            </span>
                                            <span className="tool-card-title">{tool.title}</span>
                                            <span className="tool-card-sub">{tool.description}</span>
                                            {renderStatus(tool)}
                                        </button>
                                        {showProgress && (
                                            <div className="tool-card-progress">
                                                <Progress
                                                    value={tool.progress!.current}
                                                    maxValue={tool.progress!.total}
                                                    size="sm"
                                                    color="primary"
                                                    isStriped
                                                />
                                            </div>
                                        )}
                                    </div>
                                );
                            })}
                        </div>

                        {/* Kept outside the cards: a link nested inside a <button> would be
                            invalid markup and its click would also fire the card. */}
                        <p className="tools-footnote">
                            Heroes database maintained by{' '}
                            <span
                                className="tools-footnote-link"
                                onClick={() => open('https://github.com/donutman07/MarvelRivalsCharacterIDs')}
                            >
                                donutman07
                            </span>
                        </p>
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
        </>
    );
}
