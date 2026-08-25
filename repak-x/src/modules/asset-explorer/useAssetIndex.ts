// Loads the merged asset index and keeps it current while the window is open.

import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { buildIndex, type AssetIndex } from './assetIndex'
import type { ExplorerFailure, ExplorerIndexPayload } from './types'

type State = {
    index: AssetIndex | null
    failed: ExplorerFailure[]
    loading: boolean
    /** True while a background refresh replaces an index already on screen. */
    refreshing: boolean
    error: string | null
}

export function useAssetIndex() {
    const [state, setState] = useState<State>({
        index: null,
        failed: [],
        loading: true,
        refreshing: false,
        error: null,
    })

    // Guards against two refreshes racing: only the newest result is applied.
    const requestId = useRef(0)

    const load = useCallback(async (isRefresh: boolean) => {
        const id = ++requestId.current
        setState(prev => ({
            ...prev,
            loading: !isRefresh,
            refreshing: isRefresh,
            error: null,
        }))

        try {
            const payload = await invoke<ExplorerIndexPayload>('get_asset_explorer_index')
            if (id !== requestId.current) return
            setState({
                index: buildIndex(payload.mods),
                failed: payload.failed,
                loading: false,
                refreshing: false,
                error: null,
            })
        } catch (e) {
            if (id !== requestId.current) return
            console.error('[AssetExplorer] failed to load index:', e)
            setState(prev => ({
                ...prev,
                loading: false,
                refreshing: false,
                error: e instanceof Error ? e.message : String(e),
            }))
        }
    }, [])

    useEffect(() => { load(false) }, [load])

    // Toggling, installing or deleting a mod in the main window renames or
    // moves files in the mods folder, which the backend watcher reports to
    // every window. Debounced because one user action can produce a burst.
    useEffect(() => {
        let timer: ReturnType<typeof setTimeout> | null = null
        let unlisten: (() => void) | null = null
        let cancelled = false

        listen('mods_dir_changed', () => {
            if (timer) clearTimeout(timer)
            timer = setTimeout(() => load(true), 700)
        }).then(fn => {
            if (cancelled) fn()
            else unlisten = fn
        })

        return () => {
            cancelled = true
            if (timer) clearTimeout(timer)
            if (unlisten) unlisten()
        }
    }, [load])

    return { ...state, refresh: () => load(true) }
}
