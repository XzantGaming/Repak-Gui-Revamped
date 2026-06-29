# Repak X — Changelog

## [1.5.0](https://github.com/XzantGaming/Repak-X/releases/latest)

### 🔧 Backend / Logic
- UAssetTool communication rewrite for a more stable and faster connection
- Added Hybrid mod install support (Merge raw assets and uasset files into a single IO Store bundle)
- Added detection for Hybrid IO Store Mods

### 🎨 Frontend / UI
- Added Hybrid bundle toggle to mod install panel, drop an hybrid-ready mod folder or merge paks and folders to create one
- Added a Sig Bypasser toggle in the Tools panel
- Mod type labels are now additive
- Dropping mods (pak/folders) into Mod Install panel will now add them to the installation queue
- Pressing 'Enter' in the Mod Install panel will now execute the installation queue
- Added Hybrid label for Hybrid IO Store Mods
- Added sorting for mods by modified date and name with asc/desc order
- Added a resize handle for the filters and folders panel
- Added support for Epic Games launcher and a toggle in settings to switch between steam and epic
- Added Cyclops hero icon
- Minor UI changes