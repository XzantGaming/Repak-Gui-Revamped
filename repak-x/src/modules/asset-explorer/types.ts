// Shared types for the Asset Explorer window.

/** Asset kind ids, matching `AssetKind::id()` in src/asset_explorer.rs. */
export type AssetKindId =
    | 'skeletal-mesh'
    | 'static-mesh'
    | 'texture'
    | 'material'
    | 'animation'
    | 'blueprint'
    | 'audio'
    | 'ui'
    | 'movie'
    | 'text'
    | 'data'
    | 'level'
    | 'other'

/** One mod's slice of the index, as `get_asset_explorer_index` returns it. */
export type ExplorerMod = {
    path: string
    display_name: string
    enabled: boolean
    priority: number
    folder_id: string | null
    category: string
    character_name: string
    character_id: string
    is_iostore: boolean
    total_size: number
    /** Normalized asset paths, parallel to `kinds`. */
    files: string[]
    kinds: AssetKindId[]
}

export type ExplorerFailure = {
    path: string
    display_name: string
    error: string
}

export type ExplorerIndexPayload = {
    mods: ExplorerMod[]
    failed: ExplorerFailure[]
    total_assets: number
}

/** How the tree groups its rows. */
export type ViewMode = 'tree' | 'flat' | 'by-mod'

/** Display metadata for each asset kind: label, badge class and icon colour. */
export const ASSET_KINDS: { id: AssetKindId; label: string; badge: string }[] = [
    { id: 'skeletal-mesh', label: 'Mesh', badge: 'mesh-badge' },
    { id: 'static-mesh', label: 'Static Mesh', badge: 'static-mesh-badge' },
    { id: 'texture', label: 'Texture', badge: 'texture-badge' },
    { id: 'material', label: 'Material', badge: 'vfx-badge' },
    { id: 'animation', label: 'Animation', badge: 'animation-badge' },
    { id: 'blueprint', label: 'Blueprint', badge: 'blueprint-badge' },
    { id: 'audio', label: 'Audio', badge: 'audio-badge' },
    { id: 'ui', label: 'UI', badge: 'ui-badge' },
    { id: 'movie', label: 'Movie', badge: 'movies-badge' },
    { id: 'text', label: 'Text', badge: 'text-badge' },
    { id: 'data', label: 'Data', badge: 'data-badge' },
    { id: 'level', label: 'Level', badge: 'logic-badge' },
    { id: 'other', label: 'Other', badge: 'unknown-badge' },
]

export const KIND_LABELS: Record<AssetKindId, string> = ASSET_KINDS.reduce(
    (acc, k) => { acc[k.id] = k.label; return acc },
    {} as Record<AssetKindId, string>
)

export const KIND_BADGES: Record<AssetKindId, string> = ASSET_KINDS.reduce(
    (acc, k) => { acc[k.id] = k.badge; return acc },
    {} as Record<AssetKindId, string>
)
