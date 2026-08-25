// Asset Explorer - window shell (mirrors VfxUpdaterPage).

import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import TitleBar from '../../components/TitleBar'
import { useGlobalTooltips } from '../../hooks/useGlobalTooltips'
import AssetExplorerPanel from './AssetExplorerPanel'
// Imported here rather than relied on through App.tsx: this window never
// renders App, so the dependency would otherwise only hold by accident of
// everything landing in one bundled stylesheet.
import '../../styles/GlobalTooltips.css'
import './AssetExplorer.css'

const ACCENT_COLORS_MAP: Record<string, string> = {
    red: '#be1c1c',
    blue: '#4a9eff',
    purple: '#9c27b0',
    green: '#4CAF50',
    orange: '#ff9800',
    pink: '#FF96BC',
}

const AURORA_PALETTES: Record<string, string[]> = {
    '#be1c1c': ['#be1c1c', '#ff9800', '#ffcc00', '#ff6b35'],
    '#4a9eff': ['#4a9eff', '#a855f7', '#ff6b9d', '#38bdf8'],
    '#9c27b0': ['#9c27b0', '#e879f9', '#60a5fa', '#c084fc'],
    '#4CAF50': ['#4CAF50', '#a3e635', '#22d3ee', '#34d399'],
    '#ff9800': ['#ff9800', '#facc15', '#fb7185', '#fbbf24'],
    '#FF96BC': ['#FF96BC', '#f472b6', '#c084fc', '#fda4af'],
}

export default function AssetExplorerPage() {
    // Swaps the native `title` popups for the app's styled tooltips, the same
    // as the main window. Multi-line hints (the tree's copy shortcuts) survive:
    // .global-tooltip renders pre-line.
    useGlobalTooltips()

    // Secondary windows get their own document, so they have to apply the
    // saved theme themselves and follow it when the main window changes it.
    useEffect(() => {
        const applyTheme = (theme: string, accent: string) => {
            const hexAccent = ACCENT_COLORS_MAP[accent] || accent || '#4a9eff'
            document.documentElement.setAttribute('data-theme', theme)
            document.documentElement.style.setProperty('--accent-primary', hexAccent)
            document.documentElement.style.setProperty('--accent-secondary', hexAccent)

            const palette = AURORA_PALETTES[hexAccent] || AURORA_PALETTES['#4a9eff']
            palette.forEach((color, i) => {
                document.documentElement.style.setProperty(`--aurora-color-${i + 1}`, color)
            })
        }

        invoke('get_app_settings')
            .then((settings: any) => applyTheme(settings.theme, settings.accentColor))
            .catch(console.error)

        let unlisten: (() => void) | null = null
        let cancelled = false
        import('@tauri-apps/api/event').then(({ listen }) => {
            listen('settings_changed', (event: any) => {
                const settings = event.payload
                if (settings) applyTheme(settings.theme, settings.accentColor)
            }).then(fn => {
                if (cancelled) fn()
                else unlisten = fn
            })
        })

        return () => {
            cancelled = true
            if (unlisten) unlisten()
        }
    }, [])

    return (
        <div className="ae-window">
            <TitleBar title="Repak X — Asset Explorer" />
            <div className="ae-window-body">
                <AssetExplorerPanel />
            </div>
        </div>
    )
}
