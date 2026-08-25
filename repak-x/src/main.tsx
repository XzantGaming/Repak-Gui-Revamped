import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import VfxUpdaterPage from './modules/vfx-updater/VfxUpdaterPage'
import AssetExplorerPage from './modules/asset-explorer/AssetExplorerPage'

// Disable default browser context menu
document.addEventListener('contextmenu', (e) => {
  e.preventDefault();
});

const rootElement = document.getElementById('root')

if (!rootElement) {
  throw new Error('Root element #root not found')
}

// Secondary windows share this bundle and are told apart by their route.
// Adding one here also means adding it to SECONDARY_WINDOW_ROUTES in
// index.html, which is what dismisses the splash overlay for them.
const SECONDARY_WINDOWS: { route: string; view: () => JSX.Element }[] = [
  { route: 'vfx-updater', view: () => <VfxUpdaterPage /> },
  { route: 'asset-explorer', view: () => <AssetExplorerPage /> },
]

const secondary = SECONDARY_WINDOWS.find(w => window.location.href.includes(w.route))
const rootView = secondary ? secondary.view() : <App />

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    {rootView}
  </React.StrictMode>,
)
