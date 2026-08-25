// Asset Explorer — one browsable tree over every asset in every installed mod.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useDebounce } from 'use-debounce'
import { emitTo } from '@tauri-apps/api/event'
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { AnimatePresence, motion } from 'framer-motion'
import {
    VscListTree,
    VscListFlat,
    VscPackage,
    VscExpandAll,
    VscCollapseAll,
    VscClose,
    VscRefresh,
    VscSearch,
} from 'react-icons/vsc'
import { IoMdWarning } from 'react-icons/io'
import { FiChevronDown, FiDownload } from 'react-icons/fi'
import { formatFileSize } from '../../utils/format'
import Checkbox from '../../components/ui/Checkbox'
import AssetTree from './AssetTree'
import {
    allFolderIds,
    buildTree,
    defaultExpanded,
    filterRows,
    visibleConflicts,
    type FileNode,
    type Filters,
    type FolderNode,
} from './assetIndex'
import { useAssetIndex } from './useAssetIndex'
import { ASSET_KINDS, KIND_LABELS, type AssetKindId, type ViewMode } from './types'
import './AssetExplorer.css'

/** Below this many filtered rows a search opens every folder holding a hit. */
const AUTO_EXPAND_LIMIT = 2000

/** Mod picker windowing. Must match .ae-mod-menu-item / .ae-mod-menu-list CSS. */
const MOD_ROW_HEIGHT = 28
const MOD_LIST_HEIGHT = 280
const MOD_OVERSCAN = 4

const VIEW_MODES: { id: ViewMode; label: string; icon: JSX.Element; hint: string }[] = [
    { id: 'tree', label: 'Tree', icon: <VscListTree />, hint: 'Merged folder tree across all mods' },
    { id: 'flat', label: 'Flat', icon: <VscListFlat />, hint: 'Every asset as a flat list of full paths' },
    { id: 'by-mod', label: 'By Mod', icon: <VscPackage />, hint: 'Group each mod above its own folder tree' },
]

export default function AssetExplorerPanel() {
    const { index, failed, loading, refreshing, error, refresh } = useAssetIndex()

    const [viewMode, setViewMode] = useState<ViewMode>('tree')
    const [rawQuery, setRawQuery] = useState('')
    const [query] = useDebounce(rawQuery, 180)
    const [kinds, setKinds] = useState<Set<AssetKindId>>(new Set())
    const [modFilter, setModFilter] = useState<Set<string>>(new Set())
    const [showDisabled, setShowDisabled] = useState(false)
    const [conflictsOnly, setConflictsOnly] = useState(false)
    const [expanded, setExpanded] = useState<Set<string>>(new Set())
    const [selected, setSelected] = useState<FileNode | null>(null)
    const [exportOpen, setExportOpen] = useState(false)
    const [toast, setToast] = useState<string | null>(null)

    const filters: Filters = useMemo(() => ({
        query,
        kinds,
        mods: modFilter,
        folders: new Set<string>(),
        showDisabled,
        conflictsOnly,
    }), [query, kinds, modFilter, showDisabled, conflictsOnly])

    const filteredRows = useMemo(
        () => (index ? filterRows(index, filters) : []),
        [index, filters]
    )

    const conflictPaths = useMemo(() => visibleConflicts(filteredRows), [filteredRows])

    const tree: FolderNode = useMemo(() => {
        if (!index) return buildTree([], [], 'tree', conflictPaths)
        // Flat mode never renders folders, so skip building them entirely —
        // that is the whole reason it stays responsive on huge selections.
        if (viewMode === 'flat') return buildTree([], index.mods, 'tree', conflictPaths)
        return buildTree(filteredRows, index.mods, viewMode, conflictPaths)
    }, [index, filteredRows, viewMode, conflictPaths])

    // Handed straight to the tree, which wraps only the rows on screen.
    const flatRows = viewMode === 'flat' ? filteredRows : undefined

    // A rebuilt tree has new node ids, so the previous expansion no longer
    // applies. Searches open their hits; an unfiltered tree opens as far as it
    // can without flooding the viewport.
    useEffect(() => {
        if (viewMode === 'flat') return
        if (query && filteredRows.length <= AUTO_EXPAND_LIMIT) {
            setExpanded(allFolderIds(tree))
        } else {
            setExpanded(defaultExpanded(tree))
        }
    }, [tree, viewMode, query, filteredRows.length])

    const showToast = useCallback((message: string) => {
        setToast(message)
        setTimeout(() => setToast(null), 1800)
    }, [])

    const handleToggleFolder = useCallback((id: string) => {
        setExpanded(prev => {
            const next = new Set(prev)
            if (next.has(id)) next.delete(id)
            else next.add(id)
            return next
        })
    }, [])

    /**
     * Select this asset's mod over in the main window.
     *
     * The explorer is the foreground window, so it is the one allowed to raise
     * the other; the main window only has to handle the selection.
     */
    const handleRevealMod = useCallback(async (modPath: string) => {
        try {
            await emitTo('main', 'asset-explorer:reveal-mod', { modPath })
            const main = await WebviewWindow.getByLabel('main')
            await main?.setFocus()
        } catch (e) {
            console.error('[AssetExplorer] failed to reveal mod:', e)
        }
    }, [])

    const toggleKind = (kind: AssetKindId) => {
        setKinds(prev => {
            const next = new Set(prev)
            if (next.has(kind)) next.delete(kind)
            else next.add(kind)
            return next
        })
    }

    const toggleMod = (modPath: string) => {
        setModFilter(prev => {
            const next = new Set(prev)
            if (next.has(modPath)) next.delete(modPath)
            else next.add(modPath)
            return next
        })
    }

    const clearFilters = () => {
        setRawQuery('')
        setKinds(new Set())
        setModFilter(new Set())
        setConflictsOnly(false)
    }

    const hasFilters = !!rawQuery || kinds.size > 0 || modFilter.size > 0 || conflictsOnly

    // Kind counts drive the chip labels, and hide chips for kinds nothing in
    // the current mod set actually uses.
    const kindCounts = useMemo(() => {
        const counts = new Map<AssetKindId, number>()
        for (const row of filteredRows) {
            counts.set(row.kind, (counts.get(row.kind) ?? 0) + 1)
        }
        return counts
    }, [filteredRows])

    const availableKinds = useMemo(() => {
        if (!index) return new Set<AssetKindId>()
        const present = new Set<AssetKindId>()
        for (const row of index.rows) present.add(row.kind)
        return present
    }, [index])

    const visibleModCount = useMemo(() => {
        const set = new Set<number>()
        for (const row of filteredRows) set.add(row.modIndex)
        return set.size
    }, [filteredRows])

    const conflictRowCount = useMemo(() => {
        let n = 0
        for (const row of filteredRows) if (conflictPaths.has(row.path)) n++
        return n
    }, [filteredRows, conflictPaths])

    // ---- export -----------------------------------------------------------

    const exportText = useCallback(async (kind: 'txt' | 'csv' | 'json') => {
        if (!index) return
        setExportOpen(false)

        const lines = filteredRows
        let content: string
        let extension: string

        if (kind === 'txt') {
            content = lines.map(r => r.path.replace(/^Game\//, '')).join('\n')
            extension = 'txt'
        } else if (kind === 'csv') {
            const escape = (v: string) => `"${v.replace(/"/g, '""')}"`
            content = ['asset_path,asset_kind,mod,mod_enabled,mod_priority,conflicting']
                .concat(lines.map(r => {
                    const mod = index.mods[r.modIndex]
                    return [
                        escape(r.path),
                        escape(KIND_LABELS[r.kind]),
                        escape(mod.display_name),
                        mod.enabled ? 'true' : 'false',
                        String(mod.priority),
                        conflictPaths.has(r.path) ? 'true' : 'false',
                    ].join(',')
                }))
                .join('\n')
            extension = 'csv'
        } else {
            content = JSON.stringify({
                generated: new Date().toISOString(),
                asset_count: lines.length,
                assets: lines.map(r => {
                    const mod = index.mods[r.modIndex]
                    return {
                        path: r.path,
                        kind: r.kind,
                        mod: mod.display_name,
                        mod_path: mod.path,
                        mod_enabled: mod.enabled,
                        mod_priority: mod.priority,
                        conflicting: conflictPaths.has(r.path),
                    }
                }),
            }, null, 2)
            extension = 'json'
        }

        try {
            const { save } = await import('@tauri-apps/plugin-dialog')
            const target = await save({
                defaultPath: `repakx-assets.${extension}`,
                filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
            })
            if (!target) return
            const { writeTextFile } = await import('@tauri-apps/plugin-fs')
            await writeTextFile(target, content)
            showToast(`Exported ${lines.length.toLocaleString()} assets`)
        } catch (e) {
            console.error('[AssetExplorer] export failed:', e)
            showToast('Export failed')
        }
    }, [index, filteredRows, conflictPaths, showToast])

    const copyVisiblePaths = useCallback(() => {
        setExportOpen(false)
        const text = filteredRows.map(r => r.path.replace(/^Game\//, '')).join('\n')
        navigator.clipboard.writeText(text)
            .then(() => showToast(`Copied ${filteredRows.length.toLocaleString()} paths`))
            .catch(() => showToast('Copy failed'))
    }, [filteredRows, showToast])

    // ---- render -----------------------------------------------------------

    if (loading) {
        return (
            <div className="ae-state">
                <div className="ae-spinner" />
                <p>Reading every installed mod…</p>
            </div>
        )
    }

    if (error) {
        return (
            <div className="ae-state">
                <h3>Could not build the asset index</h3>
                <p className="ae-state-detail">{error}</p>
                <button className="btn-primary" onClick={refresh}>Try again</button>
            </div>
        )
    }

    if (!index) return null

    return (
        <div className="ae-root">
            <div className="ae-toolbar">
                <div className="ae-search">
                    <VscSearch className="ae-search-icon" />
                    <input
                        value={rawQuery}
                        onChange={e => setRawQuery(e.target.value)}
                        placeholder="Search assets… (include a / to search full paths)"
                        spellCheck={false}
                    />
                    {rawQuery && (
                        <button className="ae-search-clear" onClick={() => setRawQuery('')} title="Clear search">
                            <VscClose />
                        </button>
                    )}
                </div>

                <div className="ae-view-switch">
                    {VIEW_MODES.map(mode => (
                        <button
                            key={mode.id}
                            className={`ae-view-btn ${viewMode === mode.id ? 'active' : ''}`}
                            onClick={() => setViewMode(mode.id)}
                            title={mode.hint}
                        >
                            {mode.icon}
                            <span>{mode.label}</span>
                        </button>
                    ))}
                </div>

                <div className="ae-toolbar-actions">
                    {viewMode !== 'flat' && (
                        <>
                            <button
                                className="ae-icon-btn"
                                onClick={() => setExpanded(allFolderIds(tree))}
                                title="Expand all folders"
                            >
                                <VscExpandAll />
                            </button>
                            <button
                                className="ae-icon-btn"
                                onClick={() => setExpanded(new Set())}
                                title="Collapse all folders"
                            >
                                <VscCollapseAll />
                            </button>
                        </>
                    )}
                    <button
                        className={`ae-icon-btn ${refreshing ? 'is-busy' : ''}`}
                        onClick={refresh}
                        title="Rescan all mods"
                    >
                        <VscRefresh />
                    </button>

                    <div className="ae-export">
                        <button className="ae-icon-btn" onClick={() => setExportOpen(o => !o)} title="Export">
                            <FiDownload />
                            <FiChevronDown className="ae-export-caret" />
                        </button>
                        <AnimatePresence>
                            {exportOpen && (
                                <>
                                    <div className="ae-export-scrim" onClick={() => setExportOpen(false)} />
                                    <motion.div
                                        className="ae-export-menu"
                                        initial={{ opacity: 0, y: -4 }}
                                        animate={{ opacity: 1, y: 0 }}
                                        exit={{ opacity: 0, y: -4 }}
                                        transition={{ duration: 0.12 }}
                                    >
                                        <button onClick={copyVisiblePaths}>Copy visible paths</button>
                                        <button onClick={() => exportText('txt')}>Export as .txt</button>
                                        <button onClick={() => exportText('csv')}>Export as .csv</button>
                                        <button onClick={() => exportText('json')}>Export as .json</button>
                                    </motion.div>
                                </>
                            )}
                        </AnimatePresence>
                    </div>
                </div>
            </div>

            <div className="ae-filters">
                <ChipRail>
                    {ASSET_KINDS.filter(k => availableKinds.has(k.id)).map(kind => {
                        const active = kinds.has(kind.id)
                        const count = kindCounts.get(kind.id) ?? 0
                        return (
                            <button
                                key={kind.id}
                                className={`ae-filter-chip ${active ? 'active' : ''}`}
                                onClick={() => toggleKind(kind.id)}
                                title={`${kind.label}: ${count.toLocaleString()} visible`}
                            >
                                <span className={`ae-kind-dot ae-kind-${kind.id}`} />
                                {kind.label}
                                <span className="ae-filter-count">{count.toLocaleString()}</span>
                            </button>
                        )
                    })}
                </ChipRail>

                <div className="ae-filter-controls">
                    <ModFilter
                        mods={index.mods}
                        selected={modFilter}
                        showDisabled={showDisabled}
                        onToggle={toggleMod}
                        onClear={() => setModFilter(new Set())}
                    />
                    {/* The tooltip lives on a wrapper: Checkbox spreads extra
                        props onto its inner button, which would leave the label
                        text — most of the target — without one. */}
                    <span className="ae-toggle" title="Include mods that are currently turned off">
                        <Checkbox size="sm" checked={showDisabled} onChange={setShowDisabled}>
                            Disabled mods
                        </Checkbox>
                    </span>
                    <span className="ae-toggle" title="Only assets shipped by more than one visible mod">
                        <Checkbox size="sm" checked={conflictsOnly} onChange={setConflictsOnly}>
                            Conflicts only
                        </Checkbox>
                    </span>
                    {hasFilters && (
                        <button className="ae-clear-filters" onClick={clearFilters}>Clear filters</button>
                    )}
                </div>
            </div>

            <div className="ae-body">
                <div className="ae-tree-pane">
                    <AssetTree
                        root={tree}
                        mods={index.mods}
                        expanded={expanded}
                        onToggleFolder={handleToggleFolder}
                        flatRows={flatRows}
                        conflictPaths={conflictPaths}
                        onRevealMod={handleRevealMod}
                        selectedPath={selected?.id ?? null}
                        onSelectFile={setSelected}
                    />
                </div>

                <AnimatePresence>
                    {selected && (
                        <motion.aside
                            className="ae-detail"
                            initial={{ width: 0, opacity: 0 }}
                            animate={{ width: 320, opacity: 1 }}
                            exit={{ width: 0, opacity: 0 }}
                            transition={{ duration: 0.18, ease: 'easeOut' }}
                        >
                            <AssetDetail
                                node={selected}
                                index={index}
                                conflicting={conflictPaths.has(selected.row.path)}
                                onClose={() => setSelected(null)}
                                onRevealMod={handleRevealMod}
                            />
                        </motion.aside>
                    )}
                </AnimatePresence>
            </div>

            <div className="ae-status">
                <span>
                    <strong>{filteredRows.length.toLocaleString()}</strong> assets
                    {filteredRows.length !== index.totalAssets && (
                        <span className="ae-status-dim"> of {index.totalAssets.toLocaleString()}</span>
                    )}
                </span>
                <span className="ae-status-sep" />
                <span><strong>{visibleModCount}</strong> mods</span>
                {conflictRowCount > 0 && (
                    <>
                        <span className="ae-status-sep" />
                        <button
                            className="ae-status-conflicts"
                            onClick={() => setConflictsOnly(v => !v)}
                            title="Show only overlapping assets"
                        >
                            <IoMdWarning />
                            <strong>{conflictRowCount.toLocaleString()}</strong> overlapping
                        </button>
                    </>
                )}
                {failed.length > 0 && (
                    <>
                        <span className="ae-status-sep" />
                        <span
                            className="ae-status-failed"
                            title={failed.map(f => `${f.display_name}: ${f.error}`).join('\n')}
                        >
                            {failed.length} unreadable
                        </span>
                    </>
                )}
                <span className="ae-status-spacer" />
                {refreshing && <span className="ae-status-dim">Rescanning…</span>}
            </div>

            <AnimatePresence>
                {toast && (
                    <motion.div
                        className="ae-toast"
                        initial={{ opacity: 0, y: 8 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: 8 }}
                    >
                        {toast}
                    </motion.div>
                )}
            </AnimatePresence>
        </div>
    )
}

// ---------------------------------------------------------------------------

/**
 * Single-line horizontal rail for the kind chips.
 *
 * The filter bar is pinned to one row, so the chips scroll sideways instead of
 * wrapping. The edge fades are driven from the scroll position rather than
 * being a fixed mask, because an always-on gradient dims the last chip even
 * when every chip already fits.
 */
function ChipRail({ children }: { children: React.ReactNode }) {
    const ref = useRef<HTMLDivElement | null>(null)
    const [edges, setEdges] = useState({ start: false, end: false })

    const update = useCallback(() => {
        const el = ref.current
        if (!el) return
        const max = el.scrollWidth - el.clientWidth
        const start = el.scrollLeft > 1
        const end = max > 1 && el.scrollLeft < max - 1
        // Returning the previous object lets React bail out; a fresh object on
        // every pass would re-render forever, since this runs after each one.
        setEdges(prev => (prev.start === start && prev.end === end ? prev : { start, end }))
    }, [])

    // No dependency array: the chip set changes with the filters, and this is
    // only a few layout reads.
    useLayoutEffect(update)

    useEffect(() => {
        const el = ref.current
        if (!el) return
        const observer = new ResizeObserver(update)
        observer.observe(el)
        return () => observer.disconnect()
    }, [update])

    // A vertical wheel over a horizontal rail should move it sideways.
    const handleWheel = (e: React.WheelEvent<HTMLDivElement>) => {
        const el = ref.current
        if (!el || Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return
        if (el.scrollWidth - el.clientWidth <= 1) return
        el.scrollLeft += e.deltaY
    }

    return (
        <div className={`ae-chip-rail ${edges.start ? 'fade-start' : ''} ${edges.end ? 'fade-end' : ''}`}>
            <div className="ae-chip-row" ref={ref} onScroll={update} onWheel={handleWheel}>
                {children}
            </div>
        </div>
    )
}

// ---------------------------------------------------------------------------

type ModFilterProps = {
    mods: { path: string; display_name: string; enabled: boolean }[]
    selected: Set<string>
    showDisabled: boolean
    onToggle: (modPath: string) => void
    onClear: () => void
}

/** Mod picker. Its own search box, because installs run to hundreds of mods. */
function ModFilter({ mods, selected, showDisabled, onToggle, onClear }: ModFilterProps) {
    const [open, setOpen] = useState(false)
    const [search, setSearch] = useState('')
    const [listScroll, setListScroll] = useState(0)
    const ref = useRef<HTMLDivElement | null>(null)
    const listRef = useRef<HTMLDivElement | null>(null)

    useEffect(() => {
        if (!open) return
        const onDown = (e: MouseEvent) => {
            if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
        }
        document.addEventListener('mousedown', onDown)
        return () => document.removeEventListener('mousedown', onDown)
    }, [open])

    const listed = useMemo(() => {
        const q = search.trim().toLowerCase()
        return mods.filter(m => {
            if (!showDisabled && !m.enabled) return false
            if (q && !m.display_name.toLowerCase().includes(q)) return false
            return true
        })
    }, [mods, search, showDisabled])

    // Narrowing the search shortens the list; without this the popover can end
    // up scrolled past its own content.
    useEffect(() => {
        if (listRef.current) listRef.current.scrollTop = 0
        setListScroll(0)
    }, [search])

    const listStart = Math.max(0, Math.floor(listScroll / MOD_ROW_HEIGHT) - MOD_OVERSCAN)
    const listEnd = Math.min(
        listed.length,
        listStart + Math.ceil(MOD_LIST_HEIGHT / MOD_ROW_HEIGHT) + MOD_OVERSCAN * 2
    )

    return (
        <div className="ae-mod-filter" ref={ref}>
            <button
                className={`ae-filter-select ${selected.size > 0 ? 'active' : ''}`}
                onClick={() => setOpen(o => !o)}
            >
                <VscPackage />
                {selected.size === 0 ? 'All mods' : `${selected.size} mod${selected.size === 1 ? '' : 's'}`}
                <FiChevronDown className={`ae-select-caret ${open ? 'is-open' : ''}`} />
            </button>

            <AnimatePresence>
                {open && (
                    <motion.div
                        className="ae-mod-menu"
                        initial={{ opacity: 0, y: -4 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -4 }}
                        transition={{ duration: 0.12 }}
                    >
                        <div className="ae-mod-menu-search">
                            <VscSearch />
                            <input
                                value={search}
                                onChange={e => setSearch(e.target.value)}
                                placeholder="Filter mods…"
                                spellCheck={false}
                                autoFocus
                            />
                        </div>
                        {/* Windowed the same way the tree is: a large install
                            would otherwise mount a Checkbox per mod, and every
                            keystroke in the search box above would re-render
                            all of them. */}
                        <div
                            className="ae-mod-menu-list"
                            ref={listRef}
                            onScroll={e => setListScroll(e.currentTarget.scrollTop)}
                        >
                            {listed.length === 0 ? (
                                <div className="ae-mod-menu-empty">No mods match</div>
                            ) : (
                                <div style={{ height: listed.length * MOD_ROW_HEIGHT, position: 'relative' }}>
                                    <div
                                        style={{
                                            position: 'absolute',
                                            top: 0,
                                            left: 0,
                                            right: 0,
                                            transform: `translateY(${listStart * MOD_ROW_HEIGHT}px)`,
                                        }}
                                    >
                                        {/* The row content is the Checkbox's own
                                            label, so clicking the mod name
                                            toggles it too. */}
                                        {listed.slice(listStart, listEnd).map(mod => (
                                            <Checkbox
                                                key={mod.path}
                                                size="sm"
                                                className="ae-mod-menu-item"
                                                checked={selected.has(mod.path)}
                                                onChange={() => onToggle(mod.path)}
                                            >
                                                <span className={`ae-mod-dot ${mod.enabled ? 'is-on' : 'is-off'}`} />
                                                <span className="ae-mod-menu-name" title={mod.display_name}>
                                                    {mod.display_name}
                                                </span>
                                            </Checkbox>
                                        ))}
                                    </div>
                                </div>
                            )}
                        </div>
                        {selected.size > 0 && (
                            <button className="ae-mod-menu-clear" onClick={onClear}>Clear selection</button>
                        )}
                    </motion.div>
                )}
            </AnimatePresence>
        </div>
    )
}

// ---------------------------------------------------------------------------

type AssetDetailProps = {
    node: FileNode
    index: { mods: { path: string; display_name: string; enabled: boolean; priority: number; category: string; character_name: string; is_iostore: boolean; total_size: number }[]; rows: { path: string; modIndex: number }[] }
    conflicting: boolean
    onClose: () => void
    onRevealMod: (modPath: string) => void
}

/**
 * Reverse lookup: given one asset, which mods ship it.
 *
 * Scans the row set rather than keeping an owners map for every asset — a
 * single linear pass on click is far cheaper than carrying that index around.
 */
function AssetDetail({ node, index, conflicting, onClose, onRevealMod }: AssetDetailProps) {
    const owners = useMemo(() => {
        const seen = new Set<number>()
        for (const row of index.rows) {
            if (row.path === node.row.path) seen.add(row.modIndex)
        }
        return Array.from(seen).map(i => index.mods[i])
    }, [index, node.row.path])

    const folder = node.row.path.slice(0, node.row.path.lastIndexOf('/'))

    return (
        <div className="ae-detail-inner">
            <div className="ae-detail-header">
                <h3>Asset</h3>
                <button className="ae-icon-btn" onClick={onClose} title="Close"><VscClose /></button>
            </div>

            <div className="ae-detail-section">
                <div className="ae-detail-name">{node.row.name}</div>
                <div className="ae-detail-path" title={node.row.path}>{folder.replace(/^Game\//, '')}</div>
                <div className="ae-detail-badges">
                    <span className={`ae-chip ae-chip-kind`}>
                        <span className={`ae-kind-dot ae-kind-${node.row.kind}`} />
                        {KIND_LABELS[node.row.kind]}
                    </span>
                    {conflicting && (
                        <span className="ae-chip ae-chip-conflict">
                            <IoMdWarning /> Overlapping
                        </span>
                    )}
                </div>
            </div>

            <div className="ae-detail-section">
                <h4>
                    Shipped by {owners.length} mod{owners.length === 1 ? '' : 's'}
                </h4>
                <p className="ae-detail-hint">Double click a mod to select it in Repak X.</p>
                <div className="ae-owner-list">
                    {owners.map(mod => (
                        <button
                            key={mod.path}
                            className={`ae-owner ${mod.path === index.mods[node.row.modIndex].path ? 'is-source' : ''}`}
                            onDoubleClick={() => onRevealMod(mod.path)}
                            title={`${mod.path}\n\nDouble click to select in Repak X`}
                        >
                            <span className={`ae-mod-dot ${mod.enabled ? 'is-on' : 'is-off'}`} />
                            <span className="ae-owner-name">{mod.display_name}</span>
                            <span className="ae-owner-meta">P{mod.priority}</span>
                        </button>
                    ))}
                </div>
            </div>

            <div className="ae-detail-section">
                <h4>Source mod</h4>
                {(() => {
                    const mod = index.mods[node.row.modIndex]
                    return (
                        <div className="ae-detail-rows">
                            <div><span>Category</span><span>{mod.category || 'Unknown'}</span></div>
                            {mod.character_name && (
                                <div><span>Character</span><span>{mod.character_name}</span></div>
                            )}
                            <div><span>Format</span><span>{mod.is_iostore ? 'IO Store' : 'Legacy PAK'}</span></div>
                            <div><span>Size</span><span>{formatFileSize(mod.total_size)}</span></div>
                            <div><span>Priority</span><span>{mod.priority}</span></div>
                        </div>
                    )
                })()}
            </div>
        </div>
    )
}
