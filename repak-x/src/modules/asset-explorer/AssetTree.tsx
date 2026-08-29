// The virtualized row list at the heart of the Asset Explorer.
//
// The per-mod FileTree renders every node and expands them all by default,
// which is fine for one mod and impossible across all of them: a typical
// install is six figures of assets. Here the tree is flattened into an ordered
// row array and only the slice inside the viewport is mounted, with spacers
// standing in for the rest. Rows are a fixed height so the mapping between
// scroll offset and row index stays a division.

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
    VscFolder,
    VscFolderOpened,
    VscChevronRight,
    VscChevronDown,
} from 'react-icons/vsc'
import { IoMdWarning } from 'react-icons/io'
import type { AssetRow, FileNode, FlatRow, FolderNode } from './assetIndex'
import { flattenTree } from './assetIndex'
import type { ExplorerMod } from './types'
import { KIND_BADGES, KIND_LABELS } from './types'

export const ROW_HEIGHT = 26
const OVERSCAN = 12

type AssetTreeProps = {
    root: FolderNode
    mods: ExplorerMod[]
    expanded: Set<string>
    onToggleFolder: (id: string) => void
    /**
     * Flat mode: the already-filtered rows, rendered without any folder
     * hierarchy. Passing the raw rows rather than prebuilt nodes is deliberate
     * — see the `visible` memo.
     */
    flatRows?: AssetRow[]
    conflictPaths: Map<string, number>
    onRevealMod: (modPath: string) => void
    selectedPath: string | null
    onSelectFile: (node: FileNode) => void
}

/** Copy variants, matching the per-mod FileTree so the muscle memory carries. */
const copyVariant = (fullPath: string, nameOnly: boolean, stripExtension: boolean) => {
    let value = nameOnly ? (fullPath.split('/').pop() || fullPath) : fullPath
    if (stripExtension) value = value.replace(/\.[^./]+$/, '')
    return value
}

export default function AssetTree({
    root,
    mods,
    expanded,
    onToggleFolder,
    flatRows,
    conflictPaths,
    onRevealMod,
    selectedPath,
    onSelectFile,
}: AssetTreeProps) {
    const scrollRef = useRef<HTMLDivElement | null>(null)
    const [scrollTop, setScrollTop] = useState(0)
    const [viewportHeight, setViewportHeight] = useState(600)
    const [copied, setCopied] = useState<{ id: string; label: string } | null>(null)

    const isFlat = !!flatRows

    const treeRows: FlatRow[] | null = useMemo(
        () => (flatRows ? null : flattenTree(root, expanded)),
        [root, expanded, flatRows]
    )

    const rowCount = flatRows ? flatRows.length : treeRows!.length

    // Track the viewport so the visible window is sized to the real container
    // rather than a guess that breaks when the user resizes.
    useEffect(() => {
        const el = scrollRef.current
        if (!el) return
        const measure = () => setViewportHeight(el.clientHeight)
        measure()
        const observer = new ResizeObserver(measure)
        observer.observe(el)
        return () => observer.disconnect()
    }, [])

    // A filter change can shorten the list below the current offset; without
    // this the view would sit in empty space until the user scrolls.
    useEffect(() => {
        const el = scrollRef.current
        if (el && el.scrollTop > rowCount * ROW_HEIGHT) {
            el.scrollTop = 0
            setScrollTop(0)
        }
    }, [rowCount])

    const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
        setScrollTop(e.currentTarget.scrollTop)
    }, [])

    const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
    const visibleCount = Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN * 2
    const end = Math.min(rowCount, start + visibleCount)

    const visible: FlatRow[] = useMemo(() => {
        if (!flatRows) return treeRows!.slice(start, end)
        // Flat mode wraps only the rows actually on screen. Building a node per
        // asset up front would allocate six figures of objects on every filter
        // change to render the fifty that are visible.
        const out: FlatRow[] = []
        for (let i = start; i < end; i++) {
            const row = flatRows[i]
            const id = `${row.path}#${row.modIndex}`
            out.push({
                key: id,
                depth: 0,
                node: { type: 'file', id, row, conflict: conflictPaths.has(row.path) },
            })
        }
        return out
    }, [flatRows, treeRows, start, end, conflictPaths])

    // Stable identities, so the memoized rows below really do skip re-rendering
    // on scroll. Each row closes over its own node instead of being handed a
    // freshly allocated arrow on every parent render.
    const handleCopy = useCallback((e: React.MouseEvent, id: string, fullPath: string) => {
        e.preventDefault()
        e.stopPropagation()
        const nameOnly = e.ctrlKey || e.metaKey
        const stripExtension = e.shiftKey
        const value = copyVariant(fullPath, nameOnly, stripExtension)
        navigator.clipboard.writeText(value).then(() => {
            setCopied({ id, label: nameOnly ? 'Copied name' : 'Copied path' })
            setTimeout(() => setCopied(null), 1400)
        }).catch(err => console.error('[AssetExplorer] copy failed:', err))
    }, [])

    if (rowCount === 0) {
        return (
            <div className="ae-tree-empty">
                No assets match the current filters.
            </div>
        )
    }

    return (
        <div className="ae-tree-scroll" ref={scrollRef} onScroll={handleScroll}>
            {/* The spacer owns the full scroll height; the mounted slice is
                positioned absolutely inside it so its own height never adds to
                the scroll range. */}
            <div style={{ height: rowCount * ROW_HEIGHT, position: 'relative' }}>
                <div
                    style={{
                        position: 'absolute',
                        top: 0,
                        left: 0,
                        right: 0,
                        transform: `translateY(${start * ROW_HEIGHT}px)`,
                    }}
                >
                    {/* Striping keys off the absolute row index: the mounted
                        slice starts at a different parity as you scroll, so
                        :nth-child() would make the bands jump. */}
                    {visible.map(({ key, depth, node }, i) => (
                        node.type === 'folder' ? (
                            <FolderRow
                                key={key}
                                node={node}
                                depth={depth}
                                alt={(start + i) % 2 === 1}
                                isOpen={expanded.has(node.id)}
                                mod={node.modIndex !== undefined ? mods[node.modIndex] : undefined}
                                copiedLabel={copied?.id === key ? copied.label : null}
                                onToggle={onToggleFolder}
                                onCopy={handleCopy}
                                onReveal={onRevealMod}
                            />
                        ) : (
                            <FileRow
                                key={key}
                                node={node}
                                depth={depth}
                                alt={(start + i) % 2 === 1}
                                mod={mods[node.row.modIndex]}
                                showFullPath={isFlat}
                                selected={selectedPath === node.id}
                                copiedLabel={copied?.id === key ? copied.label : null}
                                onSelect={onSelectFile}
                                onCopy={handleCopy}
                                onReveal={onRevealMod}
                            />
                        )
                    ))}
                </div>
            </div>
        </div>
    )
}

type FolderRowProps = {
    node: FolderNode
    depth: number
    /** Odd absolute row index — the zebra band. */
    alt: boolean
    isOpen: boolean
    mod?: ExplorerMod
    copiedLabel: string | null
    onToggle: (id: string) => void
    onCopy: (e: React.MouseEvent, id: string, fullPath: string) => void
    onReveal: (modPath: string) => void
}

const FolderRow = React.memo(function FolderRow({
    node, depth, alt, isOpen, mod, copiedLabel, onToggle, onCopy, onReveal,
}: FolderRowProps) {
    const isModGroup = node.modIndex !== undefined

    return (
        <div
            className={`ae-row ae-row-folder ${alt ? 'is-alt' : ''} ${isModGroup ? 'is-mod-group' : ''}`}
            style={{ paddingLeft: 6 + depth * 14, height: ROW_HEIGHT }}
            onClick={() => onToggle(node.id)}
            onDoubleClick={isModGroup && mod
                ? (e) => { e.stopPropagation(); onReveal(mod.path) }
                : undefined}
            onContextMenu={(e) => onCopy(e, node.id, node.path)}
            title={isModGroup ? node.path : `${node.path}\nRight click to copy path`}
        >
            <span className="ae-chevron">
                {isOpen ? <VscChevronDown /> : <VscChevronRight />}
            </span>
            {isModGroup ? (
                <span className={`ae-mod-dot ${mod?.enabled ? 'is-on' : 'is-off'}`} />
            ) : isOpen ? (
                <VscFolderOpened className="ae-icon ae-icon-folder" />
            ) : (
                <VscFolder className="ae-icon ae-icon-folder" />
            )}
            <span className="ae-name">{node.name}</span>

            {isModGroup && mod && (
                <span className="ae-chip ae-chip-quiet">{mod.category}</span>
            )}
            <span className="ae-meta">{node.assetCount.toLocaleString()}</span>
            {!isModGroup && node.modCount > 1 && (
                <span className="ae-meta ae-meta-dim">{node.modCount} mods</span>
            )}
            {node.conflictCount > 0 && (
                <span className="ae-conflict-count" title={`${node.conflictCount} overlapping asset(s) inside`}>
                    <IoMdWarning />
                    {node.conflictCount}
                </span>
            )}
            {copiedLabel && <span className="ae-copied">{copiedLabel}</span>}
        </div>
    )
})

type FileRowProps = {
    node: FileNode
    depth: number
    /** Odd absolute row index — the zebra band. */
    alt: boolean
    mod: ExplorerMod
    showFullPath: boolean
    selected: boolean
    copiedLabel: string | null
    onSelect: (node: FileNode) => void
    onCopy: (e: React.MouseEvent, id: string, fullPath: string) => void
    onReveal: (modPath: string) => void
}

const FileRow = React.memo(function FileRow({
    node, depth, alt, mod, showFullPath, selected, copiedLabel, onSelect, onCopy, onReveal,
}: FileRowProps) {
    const { row, conflict } = node
    const label = showFullPath ? row.path.replace(/^Game\//, '') : row.name

    return (
        <div
            className={`ae-row ae-row-file ${alt ? 'is-alt' : ''} ${conflict ? 'is-conflict' : ''} ${selected ? 'is-selected' : ''}`}
            style={{ paddingLeft: 6 + depth * 14, height: ROW_HEIGHT }}
            onClick={() => onSelect(node)}
            onDoubleClick={(e) => { e.stopPropagation(); onReveal(mod.path) }}
            onContextMenu={(e) => onCopy(e, node.id, row.path)}
            title={`${row.path}\n${mod.display_name}\n\nDouble click to show this mod in Repak X\nRight click to copy full path\nCtrl + right click: name only\nShift + right click: without extension`}
        >
            <span className="ae-chevron" />
            <span className={`ae-kind-dot ae-kind-${row.kind}`} title={KIND_LABELS[row.kind]} />
            <span className="ae-name">{label}</span>
            {conflict && (
                <span className="ae-conflict-flag" title="Also shipped by another visible mod">
                    <IoMdWarning />
                </span>
            )}
            <span className={`ae-chip ae-chip-kind ${KIND_BADGES[row.kind]}`}>{KIND_LABELS[row.kind]}</span>
            <span className={`ae-chip ae-chip-mod ${mod.enabled ? '' : 'is-disabled'}`}>
                {mod.display_name}
            </span>
            {copiedLabel && <span className="ae-copied">{copiedLabel}</span>}
        </div>
    )
})
