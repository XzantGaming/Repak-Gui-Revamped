// Index building, filtering and tree construction for the Asset Explorer.
//
// Everything here runs over every asset of every installed mod, which is
// routinely six figures of rows, so the shapes are deliberately flat and the
// hot paths avoid per-row allocation where they can. The React layer never
// touches the full row set directly: it asks for a filtered tree and then only
// renders the slice of it that is on screen (see AssetTree.tsx).

import type { AssetKindId, ExplorerMod, ViewMode } from './types'

/**
 * One asset as it appears in one mod.
 *
 * Deliberately NOT deduplicated across mods: when two mods ship the same asset
 * that is exactly the conflict the explorer exists to surface, so both rows
 * survive and sit next to each other in the tree.
 */
export type AssetRow = {
    /** Normalized path, e.g. `Game/Marvel/Characters/1048/SK_1048.uasset`. */
    path: string
    name: string
    /** Lowercased basename, precomputed because search runs on every keystroke. */
    lowerName: string
    kind: AssetKindId
    /** Index into the `mods` array this row came from. */
    modIndex: number
    /** How many mods ship this exact path. 1 for the overwhelming majority. */
    owners: number
}

export type AssetIndex = {
    mods: ExplorerMod[]
    rows: AssetRow[]
    /** Paths shipped by more than one mod, mapped to the mods that ship them. */
    conflicts: Map<string, number[]>
    totalAssets: number
    conflictAssets: number
}

export type FolderNode = {
    type: 'folder'
    id: string
    name: string
    /** Full folder path, or the mod path when this is a by-mod group header. */
    path: string
    /** Set when this folder is a mod header in `by-mod` view. */
    modIndex?: number
    folders: FolderNode[]
    files: FileNode[]
    assetCount: number
    conflictCount: number
    modCount: number
    /**
     * Build-time index of `folders` by name. Present only while a tree is being
     * constructed; `finalize` clears it, so rendering never sees it.
     */
    byName?: Map<string, FolderNode>
}

export type FileNode = {
    type: 'file'
    id: string
    row: AssetRow
    /**
     * Whether this asset is shipped by more than one *currently visible* mod.
     * See [`visibleConflicts`] for why this is not just `row.owners > 1`.
     */
    conflict: boolean
}

export type TreeNode = FolderNode | FileNode

/** A tree node paired with the depth it renders at. */
export type FlatRow = {
    key: string
    depth: number
    node: TreeNode
}

export type Filters = {
    query: string
    kinds: Set<AssetKindId>
    /** Mod paths to keep. Empty means "all mods". */
    mods: Set<string>
    /** Folder ids to keep. Empty means "all folders". */
    folders: Set<string>
    showDisabled: boolean
    conflictsOnly: boolean
}

export const EMPTY_FILTERS: Filters = {
    query: '',
    kinds: new Set(),
    mods: new Set(),
    folders: new Set(),
    showDisabled: false,
    conflictsOnly: false,
}

// ---------------------------------------------------------------------------
// Index
// ---------------------------------------------------------------------------

/**
 * Expand the backend's parallel arrays into rows and find the cross-mod
 * overlaps.
 *
 * Two passes: the first counts how many mods ship each path, the second builds
 * the rows and stamps that count onto each one. Only paths with more than one
 * owner are kept in the conflicts map — carrying a map entry for every unique
 * asset would dwarf the rows themselves.
 */
export function buildIndex(mods: ExplorerMod[]): AssetIndex {
    const ownerCounts = new Map<string, number>()
    let totalAssets = 0

    for (const mod of mods) {
        for (const file of mod.files) {
            ownerCounts.set(file, (ownerCounts.get(file) ?? 0) + 1)
            totalAssets++
        }
    }

    const conflicts = new Map<string, number[]>()
    const rows: AssetRow[] = new Array(totalAssets)
    let cursor = 0

    for (let modIndex = 0; modIndex < mods.length; modIndex++) {
        const mod = mods[modIndex]
        for (let i = 0; i < mod.files.length; i++) {
            const path = mod.files[i]
            const owners = ownerCounts.get(path) ?? 1
            const slash = path.lastIndexOf('/')
            const name = slash === -1 ? path : path.slice(slash + 1)

            if (owners > 1) {
                const existing = conflicts.get(path)
                if (existing) existing.push(modIndex)
                else conflicts.set(path, [modIndex])
            }

            rows[cursor++] = {
                path,
                name,
                lowerName: name.toLowerCase(),
                kind: mod.kinds[i] ?? 'other',
                modIndex,
                owners,
            }
        }
    }

    let conflictAssets = 0
    for (const row of rows) {
        if (row.owners > 1) conflictAssets++
    }

    return { mods, rows, conflicts, totalAssets, conflictAssets }
}

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

/**
 * Narrow the row set to what the current filters allow.
 *
 * A query without a `/` matches filenames only — that is what people actually
 * type, and it keeps the common case to a substring test on a short, already
 * lowercased string. A query containing a `/` is read as a path fragment and
 * lowercases the full path per row, which is slower but rare.
 */
export function filterRows(index: AssetIndex, filters: Filters): AssetRow[] {
    const { rows, mods } = index
    const query = filters.query.trim().toLowerCase()
    const byPath = query.includes('/')
    const hasKinds = filters.kinds.size > 0
    const hasMods = filters.mods.size > 0
    const hasFolders = filters.folders.size > 0

    // Resolve the per-mod predicates once instead of per row.
    const modAllowed = mods.map(mod => {
        if (!filters.showDisabled && !mod.enabled) return false
        if (hasMods && !filters.mods.has(mod.path)) return false
        if (hasFolders && !filters.folders.has(mod.folder_id ?? '')) return false
        return true
    })

    const out: AssetRow[] = []
    for (let i = 0; i < rows.length; i++) {
        const row = rows[i]
        if (!modAllowed[row.modIndex]) continue
        if (filters.conflictsOnly && row.owners < 2) continue
        if (hasKinds && !filters.kinds.has(row.kind)) continue
        if (query) {
            if (byPath) {
                if (!row.path.toLowerCase().includes(query)) continue
            } else if (!row.lowerName.includes(query)) continue
        }
        out.push(row)
    }
    return out
}

/**
 * Recount conflicts against the visible mods only.
 *
 * `AssetRow.owners` counts every mod in the index, including ones the filters
 * hide. Marking a row as conflicting because of a mod the user cannot currently
 * see would be a conflict they have no way to act on, so the badge is driven by
 * this narrower count instead.
 */
export function visibleConflicts(rows: AssetRow[]): Map<string, number> {
    const counts = new Map<string, number>()
    for (const row of rows) {
        if (row.owners < 2) continue
        counts.set(row.path, (counts.get(row.path) ?? 0) + 1)
    }
    for (const [path, count] of counts) {
        if (count < 2) counts.delete(path)
    }
    return counts
}

// ---------------------------------------------------------------------------
// Tree
// ---------------------------------------------------------------------------

/**
 * Build the display tree from an already-filtered row set.
 *
 * `tree` merges every mod into one path hierarchy, `by-mod` puts a mod header
 * above each mod's own hierarchy, and `flat` skips folders entirely (callers
 * render the row list directly and never come here).
 */
export function buildTree(
    rows: AssetRow[],
    mods: ExplorerMod[],
    mode: ViewMode,
    conflictPaths: Map<string, number>
): FolderNode {
    const root = makeFolder('', '')

    if (mode === 'by-mod') {
        // One header folder per mod, each holding that mod's own hierarchy.
        const groups = new Map<number, FolderNode>()
        for (const row of rows) {
            let group = groups.get(row.modIndex)
            if (!group) {
                const mod = mods[row.modIndex]
                group = makeFolder(`mod:${mod.path}`, mod.display_name)
                group.path = mod.path
                group.modIndex = row.modIndex
                groups.set(row.modIndex, group)
                root.folders.push(group)
            }
            insertRow(group, row, conflictPaths)
        }
        root.folders.sort((a, b) => a.name.localeCompare(b.name))
    } else {
        for (const row of rows) {
            insertRow(root, row, conflictPaths)
        }
    }

    finalize(root, mode === 'by-mod')
    return root
}

function makeFolder(path: string, name: string): FolderNode {
    return {
        type: 'folder',
        id: path || 'root',
        name,
        path,
        folders: [],
        files: [],
        assetCount: 0,
        conflictCount: 0,
        modCount: 0,
    }
}

function insertRow(root: FolderNode, row: AssetRow, conflictPaths: Map<string, number>) {
    const parts = row.path.split('/')
    let current = root

    // The last part is the file itself; everything before it is a folder.
    for (let i = 0; i < parts.length - 1; i++) {
        const part = parts[i]
        if (!part) continue

        // `byName` is the build-time index of this node's children. It lives on
        // the node so the lookup is a single string hash per path segment, and
        // finalize drops it so the finished tree is plain data. A linear scan
        // over `folders` is competitive at small fan-out but collapses on the
        // wide directories real installs produce (thousands of sibling ids),
        // and this runs once per path segment per rebuild.
        let children = current.byName
        if (!children) {
            children = new Map()
            current.byName = children
        }

        let next = children.get(part)
        if (!next) {
            const childPath = current.path ? `${current.path}/${part}` : part
            next = makeFolder(current.id === 'root' ? childPath : `${current.id}/${part}`, part)
            next.path = childPath
            current.folders.push(next)
            children.set(part, next)
        }
        current = next
    }

    current.files.push({
        type: 'file',
        // Two mods shipping the same asset produce two sibling leaves, so the
        // mod index has to be part of the key or React would collapse them.
        id: `${row.path}#${row.modIndex}`,
        row,
        conflict: conflictPaths.has(row.path),
    })
}

/**
 * Sort every level, roll aggregate counts up from the leaves, and collapse the
 * long single-child folder spines UE content is full of.
 */
function finalize(node: FolderNode, isModGroupRoot: boolean): Set<number> {
    const modIndices = new Set<number>()
    let assets = 0
    let conflicts = 0

    for (const child of node.folders) {
        const childMods = finalize(child, false)
        for (const m of childMods) modIndices.add(m)
        assets += child.assetCount
        conflicts += child.conflictCount
    }

    for (const file of node.files) {
        assets++
        if (file.conflict) conflicts++
        modIndices.add(file.row.modIndex)
    }

    node.assetCount = assets
    node.conflictCount = conflicts
    node.modCount = modIndices.size
    // Build scaffolding, not display data. Dropping it frees one Map per
    // folder and keeps the finished tree plain data for the renderer.
    node.byName = undefined

    node.folders.sort((a, b) => a.name.localeCompare(b.name))
    node.files.sort((a, b) => {
        const byName = a.row.name.localeCompare(b.row.name)
        if (byName !== 0) return byName
        // Same asset from different mods: keep the duplicates adjacent and in a
        // stable order so a conflict reads as one visual group.
        return a.row.modIndex - b.row.modIndex
    })

    // `A` containing only `B` renders as `A/B`. Skipped for mod headers, whose
    // name is the mod and must not absorb the first folder under it.
    if (!isModGroupRoot) {
        for (const child of node.folders) {
            collapseSpine(child)
        }
    } else {
        for (const child of node.folders) {
            for (const grandChild of child.folders) collapseSpine(grandChild)
        }
    }

    return modIndices
}

function collapseSpine(node: FolderNode) {
    while (node.folders.length === 1 && node.files.length === 0) {
        const only = node.folders[0]
        node.name = `${node.name}/${only.name}`
        node.path = only.path
        node.id = only.id
        node.folders = only.folders
        node.files = only.files
    }
    for (const child of node.folders) collapseSpine(child)
}

// ---------------------------------------------------------------------------
// Flattening for virtualization
// ---------------------------------------------------------------------------

/** Walk the tree into the ordered list of rows an expanded view would show. */
export function flattenTree(root: FolderNode, expanded: Set<string>): FlatRow[] {
    const out: FlatRow[] = []

    const walk = (node: FolderNode, depth: number) => {
        for (const folder of node.folders) {
            out.push({ key: folder.id, depth, node: folder })
            if (expanded.has(folder.id)) walk(folder, depth + 1)
        }
        for (const file of node.files) {
            out.push({ key: file.id, depth, node: file })
        }
    }

    walk(root, 0)
    return out
}

/**
 * Pick a starting expansion that shows something useful without unfolding a
 * six-figure tree.
 *
 * Expands breadth-first while the number of rows that would become visible
 * stays under `budget`, so a small mod set opens fully and a large one opens to
 * a browsable outline instead of freezing the window.
 */
export function defaultExpanded(root: FolderNode, budget = 250): Set<string> {
    const expanded = new Set<string>()
    let visible = root.folders.length + root.files.length
    const queue: FolderNode[] = [...root.folders]

    while (queue.length > 0) {
        const node = queue.shift()!
        const cost = node.folders.length + node.files.length
        if (visible + cost > budget) continue
        expanded.add(node.id)
        visible += cost
        for (const child of node.folders) queue.push(child)
    }

    return expanded
}

/** Every folder id in the tree, for the expand-all control. */
export function allFolderIds(root: FolderNode): Set<string> {
    const ids = new Set<string>()
    const walk = (node: FolderNode) => {
        for (const child of node.folders) {
            ids.add(child.id)
            walk(child)
        }
    }
    walk(root)
    return ids
}

/** The chain of folder ids leading to a node, so a target can be revealed. */
export function idsAlongPath(root: FolderNode, targetId: string): string[] {
    const chain: string[] = []
    const walk = (node: FolderNode): boolean => {
        for (const child of node.folders) {
            if (child.id === targetId) { chain.push(child.id); return true }
            if (walk(child)) { chain.push(child.id); return true }
        }
        return false
    }
    walk(root)
    return chain
}
