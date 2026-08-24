import React, { useState, useMemo, useEffect, useRef } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { VscFolder, VscFolderOpened, VscChevronRight, VscChevronDown, VscClose, VscNewFolder, VscPackage } from 'react-icons/vsc';
import { MdExtension, MdCreateNewFolder, MdDriveFileMoveOutline } from 'react-icons/md';
import { getHeroImageById, useHeroImages } from '../utils/heroImages';
import './ExtensionModOverlay.css';

/** One installable mod found inside the incoming archive. */
type ArchiveModEntry = {
    rel_path: string;
    rel_dir: string;
    base_name: string;
    is_iostore: boolean;
    size: number;
    mod_type: string;
    hero_ids: string[];
};

export type ExtensionInstallOptions = {
    /** rel_paths of the mods to install */
    selections: string[];
    /** rel_path -> new base name, only for entries the user actually renamed */
    renames: Record<string, string>;
    /** drop the archive's internal folders instead of recreating them */
    flatten: boolean;
    /** remove the downloaded archive once the install succeeds */
    deleteArchive: boolean;
};

// Characters Windows will not accept in a file name.
const INVALID_NAME_CHARS = /[\\/:*?"<>|]/g;

const sanitizeName = (value: string) => value.replace(INVALID_NAME_CHARS, '');

/**
 * Split a mod's file stem into the part worth editing and the affixes that
 * encode load order. Both a leading "!" and a trailing "_999..._P" decide how
 * the game prioritises the mod, so the author's choice is preserved verbatim
 * and only the middle is offered for renaming.
 *
 *   "!MyMod_9999999_P"  ->  { prefix: "!", name: "MyMod", suffix: "_9999999_P" }
 */
const splitModName = (stem: string) => {
    const prefixMatch = stem.match(/^!+/);
    const prefix = prefixMatch ? prefixMatch[0] : '';
    let name = stem.slice(prefix.length);

    const suffixMatch = name.match(/_\d+_P$/i);
    const suffix = suffixMatch ? suffixMatch[0] : '';
    if (suffix) name = name.slice(0, -suffix.length);

    return { prefix, name, suffix };
};

const formatSize = (bytes: number) => {
    if (!bytes) return '';
    const mb = bytes / (1024 * 1024);
    if (mb >= 1) return `${mb.toFixed(mb >= 10 ? 0 : 1)} MB`;
    return `${Math.max(1, Math.round(bytes / 1024))} KB`;
};

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

type ExtensionModOverlayProps = {
    isVisible: boolean;
    filePath: string | null;
    folders?: FolderRecord[];
    onInstall: (folderId: string | null, options: ExtensionInstallOptions) => Promise<void>;
    onCancel: () => void;
    onCreateFolder?: (name: string) => Promise<string | null>;
    onNewFolder?: (callback: (name: string) => void) => void;
};

// Simplified folder tree for the overlay (reusing logic from DropZoneOverlay)
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

// Folder node component
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

const ExtensionModOverlay = ({
    isVisible,
    filePath,
    folders = [],
    onInstall,
    onCancel,
    onCreateFolder,
    onNewFolder
}: ExtensionModOverlayProps) => {
    const [selectedFolderId, setSelectedFolderId] = useState<string | null>(null);
    const [isInstalling, setIsInstalling] = useState(false);
    const [isCreatingFolder, setIsCreatingFolder] = useState(false);
    const folderTreeRef = useRef<HTMLDivElement | null>(null);

    // Contents of the incoming archive, resolved when the overlay opens
    const [entries, setEntries] = useState<ArchiveModEntry[]>([]);
    const [isInspecting, setIsInspecting] = useState(false);
    const [inspectError, setInspectError] = useState<string | null>(null);
    const [selected, setSelected] = useState<Set<string>>(new Set());
    const [names, setNames] = useState<Record<string, string>>({});
    const [flatten, setFlatten] = useState(false);
    const [deleteArchive, setDeleteArchive] = useState(true);
    // Which archive the entries above actually describe. Installing is only
    // allowed once this matches the incoming file.
    const [inspectedPath, setInspectedPath] = useState<string | null>(null);

    // Re-render when the portrait cache finishes syncing
    useHeroImages();

    const rootFolder = useMemo(() => folders.find((f: FolderRecord) => f.is_root), [folders]);
    const subfolders = useMemo(() => folders.filter(f => !f.is_root), [folders]);
    const treeData = useMemo(() => {
        const root = buildTree(subfolders);
        return convertToArray(root);
    }, [subfolders]);

    // Extract filename from path
    const fileName = useMemo(() => {
        if (!filePath) return 'Unknown file';
        const parts = filePath.split(/[/\\]/);
        return parts[parts.length - 1];
    }, [filePath]);

    // Reset state when overlay becomes visible
    useEffect(() => {
        if (isVisible) {
            setSelectedFolderId(null);
            setIsInstalling(false);
        }
    }, [isVisible]);

    // Read what is actually inside the download so the user can pick and rename
    // before anything is written to the mods folder.
    useEffect(() => {
        if (!isVisible || !filePath) return;

        let cancelled = false;
        setIsInspecting(true);
        setInspectError(null);
        setEntries([]);
        setSelected(new Set());
        setNames({});
        setFlatten(false);
        setDeleteArchive(true);
        setInspectedPath(null);

        invoke('inspect_archive_mods', { path: filePath })
            .then((result) => {
                if (cancelled) return;
                const found = (result as ArchiveModEntry[]) || [];
                setEntries(found);
                // Everything is selected by default -- matches the old behaviour
                // of installing the whole archive.
                setSelected(new Set(found.map(e => e.rel_path)));
                // Only the editable middle goes in the box; the affixes are
                // reattached verbatim at install time.
                setNames(found.reduce((acc, e) => {
                    acc[e.rel_path] = splitModName(e.base_name).name;
                    return acc;
                }, {} as Record<string, string>));
                setInspectedPath(filePath);
            })
            .catch((err) => {
                if (cancelled) return;
                console.error('Failed to inspect archive:', err);
                setInspectError(String(err));
            })
            .finally(() => {
                if (!cancelled) setIsInspecting(false);
            });

        return () => { cancelled = true; };
    }, [isVisible, filePath]);

    // Only worth offering the structure choice when there is structure to keep
    const hasNestedStructure = useMemo(
        () => entries.some(e => e.rel_dir && e.rel_dir.length > 0),
        [entries]
    );

    const toggleEntry = (relPath: string) => {
        setSelected(prev => {
            const next = new Set(prev);
            if (next.has(relPath)) next.delete(relPath);
            else next.add(relPath);
            return next;
        });
    };

    const renameEntry = (relPath: string, value: string) => {
        setNames(prev => ({ ...prev, [relPath]: sanitizeName(value) }));
    };

    // A selected mod with a blank name would produce a nameless file
    const hasInvalidName = useMemo(
        () => entries.some(e => selected.has(e.rel_path) && !(names[e.rel_path] || '').trim()),
        [entries, selected, names]
    );

    const heroIconFor = (entry: ArchiveModEntry) => {
        for (const id of entry.hero_ids || []) {
            const src = getHeroImageById(id);
            if (src) return { src, id };
        }
        return null;
    };

    // `inspectedPath === filePath` is what actually locks the button while the
    // archive is being read. `isInspecting` alone leaves a gap: it starts false
    // and is only set inside an effect, so on the first render after the overlay
    // reopens with a new file, the previous archive's entries are still in state
    // and the button would briefly accept a click against the wrong path.
    const canInstall = !isInstalling
        && !isInspecting
        && inspectedPath === filePath
        && selected.size > 0
        && !hasInvalidName;

    const handleInstall = async () => {
        if (!canInstall) return;

        // Only send names that actually changed, so untouched mods keep the
        // exact filename the author shipped.
        const renames = entries.reduce((acc, e) => {
            const parts = splitModName(e.base_name);
            const next = (names[e.rel_path] || '').trim();
            if (selected.has(e.rel_path) && next && next !== parts.name) {
                // Reattach the priority affixes so load order is untouched
                acc[e.rel_path] = `${parts.prefix}${next}${parts.suffix}`;
            }
            return acc;
        }, {} as Record<string, string>);

        setIsInstalling(true);
        try {
            await onInstall(selectedFolderId, {
                selections: Array.from(selected),
                renames,
                flatten,
                deleteArchive
            });
        } catch (err) {
            console.error('Install failed:', err);
        } finally {
            setIsInstalling(false);
        }
    };

    const handleBackdropClick = (e: React.MouseEvent) => {
        // Only close if clicking the backdrop itself
        if (e.target === e.currentTarget) {
            onCancel();
        }
    };

    const handleNewFolder = () => {
        if (onNewFolder) {
            onNewFolder(async (name) => {
                if (!name || !name.trim()) return;
                setIsCreatingFolder(true);
                try {
                    if (onCreateFolder) {
                        const newFolderId = await onCreateFolder(name.trim());
                        if (newFolderId) {
                            setSelectedFolderId(newFolderId);
                        }
                    }
                } catch (err) {
                    console.error('Failed to create folder:', err);
                } finally {
                    setIsCreatingFolder(false);
                }
            });
        }
    };

    return (
        <AnimatePresence>
            {isVisible && (
                <motion.div
                    className="extension-overlay"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={{ duration: 0.2 }}
                    onClick={handleBackdropClick}
                >
                    <motion.div
                        className="extension-panel"
                        initial={{ y: 50, opacity: 0, scale: 0.95 }}
                        animate={{ y: 0, opacity: 1, scale: 1 }}
                        exit={{ y: 50, opacity: 0, scale: 0.95 }}
                        transition={{ duration: 0.25, ease: 'easeOut' }}
                    >
                        {/* Header */}
                        <div className="extension-header">
                            <div className="extension-icon">
                                <MdExtension />
                            </div>
                            <div className="extension-title">
                                <h2>Mod from Repak X Extension</h2>
                                <p className="file-name" title={filePath ?? undefined}>{fileName}</p>
                            </div>
                            <button className="close-btn" onClick={onCancel}>
                                <VscClose />
                            </button>
                        </div>

                        {/* Content: two independently scrolling columns. Stacked,
                            the two lists competed for one height budget and the
                            container had to scroll on top of them. */}
                        <div className="extension-content">
                            {/* What is inside the download */}
                            <div className="ext-mods-section">
                                <div className="section-header">
                                    <VscPackage />
                                    <span>
                                        {isInspecting
                                            ? 'Reading archive...'
                                            : entries.length === 1
                                                ? '1 mod found'
                                                : `${entries.length} mods found`}
                                    </span>
                                    {entries.length > 1 && !isInspecting && (
                                        <span className="ext-select-actions">
                                            <button
                                                type="button"
                                                onClick={() => setSelected(new Set(entries.map(e => e.rel_path)))}
                                            >
                                                All
                                            </button>
                                            <button
                                                type="button"
                                                onClick={() => setSelected(new Set())}
                                            >
                                                None
                                            </button>
                                        </span>
                                    )}
                                </div>

                                {inspectError && (
                                    <div className="ext-inspect-error">
                                        Could not read the archive: {inspectError}
                                    </div>
                                )}

                                {!isInspecting && !inspectError && entries.length === 0 && (
                                    <div className="ext-inspect-empty">
                                        No .pak or IoStore bundles found in this download.
                                    </div>
                                )}

                                {entries.length > 0 && (
                                    <div className="ext-mod-list">
                                        {entries.map(entry => {
                                            const isChecked = selected.has(entry.rel_path);
                                            const parts = splitModName(entry.base_name);
                                            const value = names[entry.rel_path] ?? parts.name;
                                            const isBlank = isChecked && !value.trim();
                                            const hero = heroIconFor(entry);
                                            return (
                                                <div
                                                    key={entry.rel_path}
                                                    className={`ext-mod-row ${isChecked ? 'checked' : ''}`}
                                                >
                                                    <input
                                                        type="checkbox"
                                                        className="ext-mod-check"
                                                        checked={isChecked}
                                                        onChange={() => toggleEntry(entry.rel_path)}
                                                        aria-label={`Install ${entry.base_name}`}
                                                    />
                                                    {hero ? (
                                                        <img
                                                            className="ext-mod-hero"
                                                            src={hero.src}
                                                            alt=""
                                                            title={entry.mod_type && entry.mod_type !== 'Unknown' ? entry.mod_type : undefined}
                                                        />
                                                    ) : (
                                                        <span
                                                            className="ext-mod-hero placeholder"
                                                            title={entry.mod_type && entry.mod_type !== 'Unknown' ? entry.mod_type : undefined}
                                                        >
                                                            <VscPackage />
                                                        </span>
                                                    )}
                                                    <div className="ext-mod-main">
                                                        {/* The load-order affixes are hidden rather than shown:
                                                            they are reattached verbatim on install, so the box
                                                            stays clean. The tooltip says what is preserved. */}
                                                        <input
                                                            type="text"
                                                            className={`ext-mod-name ${isBlank ? 'invalid' : ''}`}
                                                            value={value}
                                                            onChange={(e) => renameEntry(entry.rel_path, e.target.value)}
                                                            disabled={!isChecked}
                                                            spellCheck={false}
                                                            placeholder="Mod name"
                                                            title={
                                                                parts.prefix || parts.suffix
                                                                    ? `Installs as ${parts.prefix}${value || '...'}${parts.suffix}\nThe author's load-order marks are kept`
                                                                    : undefined
                                                            }
                                                        />
                                                        {entry.rel_dir && (
                                                            <span className="ext-mod-path" title={entry.rel_path}>
                                                                {entry.rel_dir}/
                                                            </span>
                                                        )}
                                                    </div>
                                                    <div className="ext-mod-meta">
                                                        {entry.is_iostore && <span className="ext-mod-tag">IoStore</span>}
                                                        <span className="ext-mod-size">{formatSize(entry.size)}</span>
                                                    </div>
                                                </div>
                                            );
                                        })}
                                    </div>
                                )}

                                {hasNestedStructure && (
                                    <div className="ext-structure-row">
                                        <MdDriveFileMoveOutline />
                                        <span className="ext-structure-label">Folder structure</span>
                                        <div className="ext-structure-toggle">
                                            <button
                                                type="button"
                                                className={!flatten ? 'active' : ''}
                                                onClick={() => setFlatten(false)}
                                                title={'Keep the archive\'s folders\nMods install into subfolders matching the archive'}
                                            >
                                                Retain
                                            </button>
                                            <button
                                                type="button"
                                                className={flatten ? 'active' : ''}
                                                onClick={() => setFlatten(true)}
                                                title={'Ignore the archive\'s folders\nEvery mod installs side by side in the chosen folder'}
                                            >
                                                Flatten
                                            </button>
                                        </div>
                                    </div>
                                )}

                                {entries.length > 0 && (
                                    <label
                                        className="ext-cleanup-row"
                                        title={'Removes the downloaded file once every selected mod is installed\nThe download is left alone if the install fails'}
                                    >
                                        <input
                                            type="checkbox"
                                            checked={deleteArchive}
                                            onChange={(e) => setDeleteArchive(e.target.checked)}
                                        />
                                        Delete the download after installing
                                    </label>
                                )}
                            </div>

                            <div className="folder-section">
                                <div className="section-header">
                                    <MdCreateNewFolder />
                                    <span>Choose installation folder</span>
                                    <button
                                        className="btn-new-folder"
                                        onClick={handleNewFolder}
                                        disabled={isCreatingFolder}
                                        title="Create new folder"
                                    >
                                        <VscNewFolder />
                                        {isCreatingFolder ? 'Creating...' : 'New Folder'}
                                    </button>
                                </div>

                                <div className="folder-tree-container" ref={folderTreeRef}>
                                    {/* Root folder */}
                                    {rootFolder && (
                                        <div
                                            className={`ext-folder-item root-item ${selectedFolderId === null ? 'selected' : ''}`}
                                            onClick={() => setSelectedFolderId(null)}
                                        >
                                            <span className="folder-icon"><VscFolderOpened /></span>
                                            <span className="folder-name">{rootFolder.name}</span>
                                        </div>
                                    )}

                                    {/* Subfolders */}
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

                                {selectedFolderId && (
                                    <div className="selected-hint">
                                        Installing to: <strong>{selectedFolderId}</strong>
                                    </div>
                                )}
                            </div>
                        </div>

                        {/* Footer */}
                        <div className="extension-footer">
                            <button className="btn-cancel" onClick={onCancel}>
                                Cancel
                            </button>
                            <button
                                className={`btn-install ${isInstalling ? 'loading' : ''}`}
                                onClick={handleInstall}
                                disabled={!canInstall}
                                title={hasInvalidName ? 'Every selected mod needs a name' : undefined}
                            >
                                {isInstalling
                                    ? 'Installing...'
                                    : isInspecting || inspectedPath !== filePath
                                        ? 'Reading archive...'
                                        : selected.size > 1
                                            ? `Install ${selected.size} Mods`
                                            : 'Install Mod'}
                            </button>
                        </div>
                    </motion.div>
                </motion.div>
            )}
        </AnimatePresence>
    );
};

export default ExtensionModOverlay;
