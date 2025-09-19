extern crate core;

mod file_table;
mod install_mod;
mod uasset_detection;
mod uasset_api_integration;
mod utils;

pub mod ios_widget;
mod utoc_utils;
mod welcome;

use crate::file_table::FileTable;
use crate::install_mod::{
    map_dropped_file_to_mods, map_paths_to_mods, InstallableMod, ModInstallRequest, AES_KEY,
};
use crate::utils::find_marvel_rivals;
use crate::utils::get_current_pak_characteristics;
use crate::utoc_utils::read_utoc;
use eframe::egui::{
    self, style::Selection, Align, Align2, Button, Color32, IconData, Id, Label, LayerId, Order,
    RichText, ScrollArea, Stroke, Style, TextEdit, TextStyle, Theme,
};
use egui_flex::{item, Flex, FlexAlign};
use install_mod::install_mod_logic::pak_files::extract_pak_to_dir;
use log::{debug, error, info, trace, warn, LevelFilter};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use path_clean::PathClean;
use repak::PakReader;
use rfd::{FileDialog, MessageButtons};
use serde::{Deserialize, Serialize};
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use std::cell::LazyCell;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use std::{fs, thread};
use uuid::Uuid;
use walkdir::WalkDir;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize, Serialize, Clone, Debug)]
struct CustomPalette {
    // Stored as RGBA components for serde compatibility
    accent: [u8; 4],
    panel_fill: [u8; 4],
    window_fill: [u8; 4],
    widget_inactive: [u8; 4],
    widget_hovered: [u8; 4],
    widget_active: [u8; 4],
    widget_open: [u8; 4],
    text: Option<[u8; 4]>,
}

impl Default for CustomPalette {
    fn default() -> Self {
        // Defaults roughly aligned to current bubbly dark theme
        Self {
            accent: [0xff, 0x6b, 0x9d, 0xff],
            panel_fill: [0x1f, 0x29, 0x37, 0xff],
            window_fill: [0x11, 0x18, 0x27, 0xff],
            widget_inactive: [0x2a, 0x2d, 0x3a, 0xff],
            widget_hovered: [0x3d, 0x43, 0x54, 0xff],
            widget_active: [0x4a, 0x55, 0x68, 0xff],
            widget_open: [0x55, 0x3c, 0x4e, 0xff],
            text: Some([0xf9, 0xfa, 0xfb, 0xff]),
        }
    }
}

impl CustomPalette {
    fn rgba(c: [u8; 4]) -> Color32 { Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]) }
    fn accent_color(&self) -> Color32 { Self::rgba(self.accent) }
}

#[derive(Deserialize, Serialize)]
struct RepakModManager {
    game_path: PathBuf,
    default_font_size: f32,
    folders: Vec<ModFolder>,
    #[serde(default)]
    mod_metadata: Vec<ModMetadata>,
    #[serde(skip)]
    current_pak_file_idx: Option<usize>,
    #[serde(skip)]
    pak_files: Vec<ModEntry>,
    #[serde(skip)]
    table: Option<FileTable>,
    #[serde(skip)]
    file_drop_viewport_open: bool,
    #[serde(skip)]
    install_mod_dialog: Option<ModInstallRequest>,
    #[serde(skip)]
    receiver: Option<Receiver<Event>>,
    #[serde(skip)]
    welcome_screen: Option<ShowWelcome>,
    #[serde(skip)]
    hide_welcome: bool,
    #[serde(skip)]
    creating_folder: bool,
    #[serde(skip)]
    new_folder_name: String,
    #[serde(skip)]
    game_path_input: String,
    #[serde(skip)]
    search_query: String,
    #[serde(skip)]
    filtered_mods: Vec<usize>,
    #[serde(skip)]
    expanded_folders_for_search: std::collections::HashSet<String>,
    #[serde(skip)]
    tag_filter_enabled: bool,
    #[serde(skip)]
    selected_tag_filters: std::collections::HashSet<String>,
    #[serde(skip)]
    show_tag_filter_dropdown: bool,
    #[serde(skip)]
    custom_tag_filter_enabled: bool,
    #[serde(skip)]
    selected_custom_tag_filters: std::collections::HashSet<String>,
    #[serde(default)]
    custom_tag_catalog: Vec<String>,
    #[serde(skip)]
    show_custom_tag_filter_dropdown: bool,
    #[serde(skip)]
    show_tag_manager: bool,
    #[serde(skip)]
    new_global_tag_input: String,
    #[serde(skip)]
    rename_tag_from: Option<String>,
    #[serde(skip)]
    rename_tag_to: String,
    #[serde(skip)]
    selection_mode: bool,
    #[serde(skip)]
    selected_mods: std::collections::BTreeSet<usize>,
    #[serde(skip)]
    bulk_tag_input: String,
    #[serde(skip)]
    bulk_remove_choice: Option<String>,
    version: Option<String>,
    // Custom palette support
    #[serde(default)]
    use_custom_palette: bool,
    #[serde(default)]
    custom_palette: CustomPalette,
    #[serde(skip)]
    show_palette_window: bool,
    #[serde(skip)]
    preset_name_input: String,
    #[serde(skip)]
    refresh_after_delete: bool,
    #[serde(skip)]
    delete_sender: Option<Sender<Vec<std::path::PathBuf>>>,
    #[serde(skip)]
    delete_results: Option<Receiver<Result<Vec<std::path::PathBuf>, String>>>,
    #[serde(skip)]
    deleting_mods: std::collections::HashSet<std::path::PathBuf>,
    #[serde(skip)]
    pending_remove_paths: Vec<std::path::PathBuf>,
}

impl Default for RepakModManager {
    fn default() -> Self {
        Self {
            game_path: PathBuf::new(),
            default_font_size: 18.0,
            folders: Vec::new(),
            mod_metadata: Vec::new(),
            current_pak_file_idx: None,
            pak_files: Vec::new(),
            table: None,
            file_drop_viewport_open: false,
            install_mod_dialog: None,
            receiver: None,
            welcome_screen: None,
            hide_welcome: false,
            creating_folder: false,
            new_folder_name: String::new(),
            game_path_input: String::new(),
            search_query: String::new(),
            filtered_mods: Vec::new(),
            expanded_folders_for_search: std::collections::HashSet::new(),
            tag_filter_enabled: false,
            selected_tag_filters: std::collections::HashSet::new(),
            show_tag_filter_dropdown: false,
            custom_tag_filter_enabled: false,
            selected_custom_tag_filters: std::collections::HashSet::new(),
            custom_tag_catalog: Vec::new(),
            show_custom_tag_filter_dropdown: false,
            show_tag_manager: false,
            new_global_tag_input: String::new(),
            rename_tag_from: None,
            rename_tag_to: String::new(),
            selection_mode: false,
            selected_mods: std::collections::BTreeSet::new(),
            bulk_tag_input: String::new(),
            bulk_remove_choice: None,
            version: None,
            use_custom_palette: false,
            custom_palette: CustomPalette::default(),
            show_palette_window: false,
            preset_name_input: String::new(),
            refresh_after_delete: false,
            delete_sender: None,
            delete_results: None,
            deleting_mods: std::collections::HashSet::new(),
            pending_remove_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ModFolder {
    id: String,
    name: String,
    enabled: bool,
    expanded: bool,
    color: Option<[u8; 3]>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ModMetadata {
    path: PathBuf,
    custom_name: Option<String>,
    folder_id: Option<String>,
    #[serde(default)]
    custom_tags: Vec<String>,
}

#[derive(Clone)]
struct ModEntry {
    reader: PakReader,
    path: PathBuf,
    enabled: bool,
    custom_name: Option<String>,
    editing_name: bool,
    folder_id: Option<String>,
    custom_tags: Vec<String>,
}
fn use_bubbly_light_theme(style: &mut egui::Style) {
    // Bubbly pastel colors for light mode
    style.visuals.widgets.inactive.bg_fill = Color32::from_hex("#f8f9ff").unwrap();
    style.visuals.widgets.hovered.bg_fill = Color32::from_hex("#e8f4fd").unwrap();
    style.visuals.widgets.active.bg_fill = Color32::from_hex("#c8e6c9").unwrap();
    style.visuals.widgets.open.bg_fill = Color32::from_hex("#fff3e0").unwrap();
    
    // Soft rounded corners for bubbly effect
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(12);
    
    // Soft borders using bg_stroke for light mode
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_hex("#e0e7ff").unwrap());
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, Color32::from_hex("#c7d2fe").unwrap());
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, Color32::from_hex("#a5b4fc").unwrap());
    
    // Light bubbly panel background
    style.visuals.panel_fill = Color32::from_hex("#fafbff").unwrap();
    style.visuals.window_fill = Color32::from_hex("#ffffff").unwrap();
    
    // Spacing for bubbly feel
    style.spacing.item_spacing = egui::Vec2::new(8.0, 6.0);
    style.spacing.button_padding = egui::Vec2::new(12.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 20.0;
}

fn use_bubbly_dark_theme(style: &mut egui::Style) {
    // Bubbly dark colors for dark mode
    style.visuals.widgets.inactive.bg_fill = Color32::from_hex("#2a2d3a").unwrap();
    style.visuals.widgets.hovered.bg_fill = Color32::from_hex("#3d4354").unwrap();
    style.visuals.widgets.active.bg_fill = Color32::from_hex("#4a5568").unwrap();
    style.visuals.widgets.open.bg_fill = Color32::from_hex("#553c4e").unwrap();
    
    // Soft rounded corners for bubbly effect
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
    style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(12);
    
    // Soft borders using bg_stroke for dark mode
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_hex("#4a5568").unwrap());
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, Color32::from_hex("#6b7280").unwrap());
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, Color32::from_hex("#8b5cf6").unwrap());
    
    // Dark bubbly panel background
    style.visuals.panel_fill = Color32::from_hex("#1f2937").unwrap();
    style.visuals.window_fill = Color32::from_hex("#111827").unwrap();
    
    // Dark mode text colors
    style.visuals.override_text_color = Some(Color32::from_hex("#f9fafb").unwrap());
    
    // Spacing for bubbly feel
    style.spacing.item_spacing = egui::Vec2::new(8.0, 6.0);
    style.spacing.button_padding = egui::Vec2::new(12.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.indent = 20.0;
}

pub fn setup_custom_style(ctx: &egui::Context) {
    ctx.style_mut_of(Theme::Dark, use_bubbly_dark_theme);
    ctx.style_mut_of(Theme::Light, use_bubbly_light_theme);
}

impl RepakModManager {
    fn ensure_delete_worker(&mut self) {
        let need_spawn = self.delete_sender.is_none() || self.delete_results.is_none();
        if !need_spawn { return; }
        let (job_tx, job_rx): (Sender<Vec<std::path::PathBuf>>, Receiver<Vec<std::path::PathBuf>>) = channel();
        let (res_tx, res_rx): (Sender<Result<Vec<std::path::PathBuf>, String>>, Receiver<Result<Vec<std::path::PathBuf>, String>>) = channel();

        // Spawn a background thread to process deletions off the UI thread
        std::thread::spawn(move || {
            while let Ok(paths) = job_rx.recv() {
                // Try to delete each file; ignore NotFound but report other errors
                let mut first_err: Option<String> = None;
                for p in &paths {
                    // Try to rename to a temporary ".pending_delete" extension first to
                    // sidestep possible locks and make deletion safer on Windows
                    let mut target = p.clone();
                    if target.exists() {
                        let mut ext = target.extension().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        if ext.is_empty() { ext = "pending_delete".to_string(); } else { ext.push_str(".pending_delete"); }
                        let mut tmp = target.clone();
                        tmp.set_extension(ext);
                        if let Ok(_) = std::fs::rename(&target, &tmp) {
                            target = tmp;
                        }
                    }
                    match std::fs::remove_file(&target) {
                        Ok(_) => {}
                        Err(e) => {
                            if e.kind() == std::io::ErrorKind::NotFound {
                                // Ignore
                            } else {
                                if first_err.is_none() {
                                    first_err = Some(format!("{}: {}", target.display(), e));
                                }
                            }
                        }
                    }
                }
                // Send result back to UI thread
                let _ = match first_err {
                    Some(err) => res_tx.send(Err(err)),
                    None => res_tx.send(Ok(paths)),
                };
            }
        });

        self.delete_sender = Some(job_tx);
        self.delete_results = Some(res_rx);
    }
    fn apply_custom_palette_to_style(&self, style: &mut egui::Style) {
        let p = &self.custom_palette;
        style.visuals.panel_fill = CustomPalette::rgba(p.panel_fill);
        style.visuals.window_fill = CustomPalette::rgba(p.window_fill);
        style.visuals.widgets.inactive.bg_fill = CustomPalette::rgba(p.widget_inactive);
        style.visuals.widgets.hovered.bg_fill = CustomPalette::rgba(p.widget_hovered);
        style.visuals.widgets.active.bg_fill = CustomPalette::rgba(p.widget_active);
        style.visuals.widgets.open.bg_fill = CustomPalette::rgba(p.widget_open);
        if let Some(txt) = p.text { style.visuals.override_text_color = Some(CustomPalette::rgba(txt)); }
    }
    fn accent(&self) -> Color32 { self.custom_palette.accent_color() }

    fn app_dir() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn palettes_dir() -> PathBuf {
        let mut d = Self::app_dir();
        d.push("palettes");
        let _ = fs::create_dir_all(&d);
        d
    }

    fn list_palette_presets(&self) -> Vec<String> {
        let d = Self::palettes_dir();
        let mut out = Vec::new();
        if let Ok(read) = fs::read_dir(d) {
            for e in read.flatten() {
                if let Some(ext) = e.path().extension() {
                    if ext == "json" {
                        if let Some(stem) = e.path().file_stem() {
                            out.push(stem.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    fn save_palette_preset(&self, name: &str) -> std::io::Result<()> {
        let mut path = Self::palettes_dir();
        path.push(format!("{}.json", name));
        let json = serde_json::to_string_pretty(&self.custom_palette)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        fs::write(path, json)
    }

    fn load_palette_preset(&mut self, name: &str) -> std::io::Result<()> {
        let mut path = Self::palettes_dir();
        path.push(format!("{}.json", name));
        let data = fs::read_to_string(path)?;
        let pal: CustomPalette = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        self.custom_palette = pal;
        self.use_custom_palette = true;
        Ok(())
    }

    fn delete_palette_preset(&self, name: &str) -> std::io::Result<()> {
        let mut path = Self::palettes_dir();
        path.push(format!("{}.json", name));
        if path.exists() { fs::remove_file(path)?; }
        Ok(())
    }
}

fn set_custom_font_size(ctx: &egui::Context, size: f32) {
    let mut style = (*ctx.style()).clone();
    for (text_style, font_id) in style.text_styles.iter_mut() {
        match text_style {
            TextStyle::Small => {
                font_id.size = size - 4.;
            }
            TextStyle::Body => {
                font_id.size = size - 3.;
            }
            TextStyle::Monospace => {
                font_id.size = size;
            }
            TextStyle::Button => {
                font_id.size = size - 1.;
            }
            TextStyle::Heading => {
                font_id.size = size + 4.;
            }
            TextStyle::Name(_) => {
                font_id.size = size;
            }
        }
    }
    ctx.set_style(style);
}

impl RepakModManager {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let game_install_path = find_marvel_rivals();

        let mut game_path = PathBuf::new();
        if let Some(path) = game_install_path {
            game_path = path.join("~mods").clean();
            fs::create_dir_all(&game_path).unwrap();
        }
        setup_custom_style(&cc.egui_ctx);
        let mut x = Self {
            game_path: game_path.clone(),
            default_font_size: 18.0,
            folders: vec![],
            pak_files: vec![],
            current_pak_file_idx: None,
            table: None,
            version: Some(VERSION.to_string()),
            creating_folder: false,
            new_folder_name: String::new(),
            game_path_input: game_path.to_string_lossy().to_string(), // Initialize the editable input with the detected/loaded path
            ..Default::default()
        };
        x.update_search_filter();
        set_custom_font_size(&cc.egui_ctx, x.default_font_size);
        x
    }

    fn collect_pak_files(&mut self) {
        if self.game_path.exists() {
            let mut vecs = vec![];

            for entry in WalkDir::new(&self.game_path)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().is_file())
            {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                let mut disabled = false;

                if path.extension().unwrap_or_default() != "pak" {
                    // left in old file extension for compatibility reason
                    if path.extension().unwrap_or_default() == "pak_disabled"
                        || path.extension().unwrap_or_default() == "bak_repak"
                    {
                        disabled = true;
                    } else {
                        continue;
                    }
                }

                let mut builder = repak::PakBuilder::new();
                builder = builder.key(AES_KEY.clone().0);
                let pak = builder.reader(&mut BufReader::new(File::open(path.clone()).unwrap()));

                if let Err(_e) = pak {
                    warn!("Error opening pak file");
                    continue;
                }
                let pak = pak.unwrap();
                
                // Find existing metadata for this path
                let metadata = self.mod_metadata.iter().find(|m| m.path == path.to_path_buf());
                
                let entry = ModEntry {
                    reader: pak,
                    path: path.to_path_buf(),
                    enabled: !disabled,
                    custom_name: metadata.and_then(|m| m.custom_name.clone()),
                    editing_name: false,
                    folder_id: metadata.and_then(|m| m.folder_id.clone()),
                    custom_tags: metadata
                        .map(|m| m.custom_tags.clone())
                        .unwrap_or_default(),
                };
                vecs.push(entry);
            }
            self.pak_files = vecs;
            // Merge any pending custom tags recorded during install
            self.apply_pending_custom_tags();
            self.update_search_filter();
        }
    }

    fn create_folder(&mut self, name: String) {
        let folder = ModFolder {
            id: Uuid::new_v4().to_string(),
            name,
            enabled: true,
            expanded: true,
            color: None,
        };
        self.folders.push(folder);
        self.save_state().ok();
    }

    fn toggle_folder(&mut self, folder_id: &str) {
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.enabled = !folder.enabled;
            let target_enabled = folder.enabled;
            
            // Toggle all mods in this folder
            for mod_entry in &mut self.pak_files {
                if mod_entry.folder_id.as_ref() == Some(&folder_id.to_string()) {
                    mod_entry.enabled = target_enabled;
                }
            }
            self.save_state().ok();
        }
    }

    fn toggle_mod_enabled(&mut self, mod_entry: &mut ModEntry) {
        mod_entry.enabled = !mod_entry.enabled;
        if mod_entry.enabled {
            let new_pak = &mod_entry.path.with_extension("pak");
            match std::fs::rename(&mod_entry.path, new_pak) {
                Ok(_) => {
                    mod_entry.path = new_pak.clone();
                    info!("Enabled mod: {:?}", new_pak);
                }
                Err(e) => {
                    warn!("Failed to enable mod: {:?}", e);
                    mod_entry.enabled = false;
                }
            }
        } else {
            let new_pak = &mod_entry.path.with_extension("bak_repak");
            match std::fs::rename(&mod_entry.path, new_pak) {
                Ok(_) => {
                    mod_entry.path = new_pak.clone();
                    info!("Disabled mod: {:?}", new_pak);
                }
                Err(e) => {
                    warn!("Failed to disable mod: {:?}", e);
                    mod_entry.enabled = true;
                }
            }
        }
    }

    fn get_mod_display_name(&self, pak_file: &ModEntry) -> String {
        pak_file.custom_name.as_ref().unwrap_or(&pak_file
            .path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string()).clone()
    }

    fn assign_mod_to_folder(&mut self, mod_index: usize, folder_id: Option<String>) {
        if let Some(mod_entry) = self.pak_files.get_mut(mod_index) {
            mod_entry.folder_id = folder_id;
            self.save_state().ok();
        }
    }

    fn update_search_filter(&mut self) {
        self.filtered_mods.clear();
        self.expanded_folders_for_search.clear();
        
        let has_search = !self.search_query.trim().is_empty();
        let has_tag_filter = self.tag_filter_enabled && !self.selected_tag_filters.is_empty();
        let has_custom_tag_filter = self.custom_tag_filter_enabled
            && !self.selected_custom_tag_filters.is_empty();
        
        if !has_search && !has_tag_filter && !has_custom_tag_filter {
            // If no filters are active, show all mods
            for i in 0..self.pak_files.len() {
                // Skip entries currently being deleted
                if self.deleting_mods.contains(&self.pak_files[i].path) { continue; }
                self.filtered_mods.push(i);
            }
            return;
        }
        
        let query = self.search_query.to_lowercase();
        
        for (index, pak_file) in self.pak_files.iter().enumerate() {
            // Skip entries currently being deleted
            if self.deleting_mods.contains(&pak_file.path) { continue; }
            let mut matches = true;
            
            // Check search query match
            if has_search {
                let display_name = self.get_mod_display_name(pak_file).to_lowercase();
                let file_name = pak_file.path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                
                if !display_name.contains(&query) && !file_name.contains(&query) {
                    matches = false;
                }
            }
            
            // Check tag filter match
            if has_tag_filter && matches {
                let mod_type = self.get_mod_type(&pak_file.reader, &pak_file.path);
                if !self.selected_tag_filters.contains(&mod_type) {
                    matches = false;
                }
            }

            // Check custom tag filter match (ANY)
            if has_custom_tag_filter && matches {
                let has_any = pak_file
                    .custom_tags
                    .iter()
                    .any(|t| self.selected_custom_tag_filters.contains(t));
                if !has_any {
                    matches = false;
                }
            }
            
            if matches {
                self.filtered_mods.push(index);
                
                // If this mod is in a folder, mark the folder for expansion
                if let Some(folder_id) = &pak_file.folder_id {
                    self.expanded_folders_for_search.insert(folder_id.clone());
                }
            }
        }
    }

    fn get_mod_type(&self, pak_reader: &PakReader, pak_path: &PathBuf) -> String {
        let mut utoc_path = pak_path.to_path_buf();
        utoc_path.set_extension("utoc");

        let paths = {
            if utoc_path.exists() {
                let file = read_utoc(&utoc_path, pak_reader, pak_path)
                    .iter()
                    .map(|entry| entry.file_path.clone())
                    .collect::<Vec<_>>();
                file
            } else {
                pak_reader.files().into_iter().collect::<Vec<_>>()
            }
        };

        get_current_pak_characteristics(paths)
    }

    fn get_all_mod_types(&self) -> std::collections::BTreeSet<String> {
        let mut types = std::collections::BTreeSet::new();
        for pak_file in &self.pak_files {
            let mod_type = self.get_mod_type(&pak_file.reader, &pak_file.path);
            types.insert(mod_type);
        }
        types
    }

    fn get_all_custom_tags(&self) -> std::collections::BTreeSet<String> {
        let mut tags = std::collections::BTreeSet::new();
        // Include catalog
        for t in &self.custom_tag_catalog { tags.insert(t.clone()); }
        // Include any assigned tags
        for pak_file in &self.pak_files {
            for t in &pak_file.custom_tags {
                tags.insert(t.clone());
            }
        }
        tags
    }

    fn apply_pending_custom_tags(&mut self) {
        // pending file lives in the same config dir as main config
        let mut cfg = Self::config_path();
        cfg.pop(); // repak_mod_manager.json -> dir
        let mut pending = cfg.clone();
        pending.push("pending_custom_tags.json");
        if !pending.exists() { return; }
        let Ok(s) = fs::read_to_string(&pending) else { return; };
        let mut map: std::collections::BTreeMap<String, Vec<String>> = match serde_json::from_str(&s) {
            Ok(m) => m,
            Err(_) => return,
        };
        if map.is_empty() { let _ = fs::remove_file(&pending); return; }

        let mut used_keys: Vec<String> = Vec::new();
        for i in 0..self.pak_files.len() {
            let stem = self.pak_files[i]
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if let Some(tags) = map.get(&stem) {
                // merge into ModEntry
                for t in tags {
                    if !self.pak_files[i].custom_tags.contains(t) {
                        self.pak_files[i].custom_tags.push(t.clone());
                    }
                    if !self.custom_tag_catalog.contains(t) {
                        self.custom_tag_catalog.push(t.clone());
                    }
                }
                self.pak_files[i].custom_tags.sort();
                self.pak_files[i].custom_tags.dedup();
                self.custom_tag_catalog.sort();
                self.custom_tag_catalog.dedup();
                // sync to metadata for persistence
                self.sync_metadata();
                used_keys.push(stem);
            }
        }
        // prune used keys and save
        if !used_keys.is_empty() {
            for k in used_keys { map.remove(&k); }
            if map.is_empty() {
                let _ = fs::remove_file(&pending);
            } else if let Ok(json) = serde_json::to_string_pretty(&map) {
                let _ = fs::write(&pending, json);
            }
        }
    }

    fn rename_custom_tag(&mut self, from: &str, to: &str) {
        if from == to || to.trim().is_empty() { return; }
        for pak in &mut self.pak_files {
            let mut changed = false;
            for t in &mut pak.custom_tags {
                if t == from { *t = to.to_string(); changed = true; }
            }
            if changed {
                pak.custom_tags.sort();
                pak.custom_tags.dedup();
            }
        }
        self.update_search_filter();
        let _ = self.save_state();
    }

    fn delete_custom_tag_global(&mut self, tag: &str) {
        for pak in &mut self.pak_files {
            if pak.custom_tags.iter().any(|t| t == tag) {
                pak.custom_tags.retain(|t| t != tag);
            }
        }
        self.update_search_filter();
        let _ = self.save_state();
    }

    fn bulk_add_tag_to_selected(&mut self, tag: &str) {
        if tag.trim().is_empty() { return; }
        for &i in &self.selected_mods {
            if let Some(p) = self.pak_files.get_mut(i) {
                if !p.custom_tags.contains(&tag.to_string()) {
                    p.custom_tags.push(tag.to_string());
                    p.custom_tags.sort();
                    p.custom_tags.dedup();
                }
            }
        }
        if !self.custom_tag_catalog.contains(&tag.to_string()) {
            self.custom_tag_catalog.push(tag.to_string());
            self.custom_tag_catalog.sort();
            self.custom_tag_catalog.dedup();
        }
        self.update_search_filter();
        let _ = self.save_state();
    }

    fn bulk_remove_tag_from_selected(&mut self, tag: &str) {
        for &i in &self.selected_mods {
            if let Some(p) = self.pak_files.get_mut(i) {
                p.custom_tags.retain(|t| t != tag);
            }
        }
        self.update_search_filter();
        let _ = self.save_state();
    }

    fn is_mod_visible(&self, mod_index: usize) -> bool {
        // Hide while deleting to avoid heavy work and races
        if let Some(p) = self.pak_files.get(mod_index).map(|m| m.path.clone()) {
            if self.deleting_mods.contains(&p) { return false; }
        }
        let has_search = !self.search_query.trim().is_empty();
        let has_tag_filter = self.tag_filter_enabled && !self.selected_tag_filters.is_empty();
        let has_custom_tag_filter = self.custom_tag_filter_enabled
            && !self.selected_custom_tag_filters.is_empty();
        
        if !has_search && !has_tag_filter && !has_custom_tag_filter {
            return true;
        }
        self.filtered_mods.contains(&mod_index)
    }

    fn should_expand_folder_for_search(&self, folder_id: &str) -> bool {
        let has_search = !self.search_query.trim().is_empty();
        let has_tag_filter = self.tag_filter_enabled && !self.selected_tag_filters.is_empty();
        let has_custom_tag_filter = self.custom_tag_filter_enabled
            && !self.selected_custom_tag_filters.is_empty();
        
        if !has_search && !has_tag_filter && !has_custom_tag_filter {
            return false;
        }
        self.expanded_folders_for_search.contains(folder_id)
    }
    fn list_pak_contents(&mut self, ui: &mut egui::Ui) -> Result<(), repak::Error> {
        ui.label("Files");
        ui.separator();
        let ctx = ui.ctx();
        self.preview_files_being_dropped(ctx, ui.available_rect_before_wrap());

        // if files are being dropped
        if self.current_pak_file_idx.is_none() && ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let rect = ui.available_rect_before_wrap();
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

            let color = ui.style().visuals.faint_bg_color;
            painter.rect_filled(rect, 0.0, color);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop .pak files or mod folders here",
                TextStyle::Heading.resolve(&ctx.style()),
                Color32::WHITE,
            );
        }
        ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let table = &mut self.table;
                if let Some(ref mut table) = table {
                    table.table_ui(ui);
                }
            });
        Ok(())
    }

    fn show_pak_details(&mut self, ui: &mut egui::Ui) {
        if self.current_pak_file_idx.is_none() {
            return;
        }
        use egui::{Label, RichText};
        let pak = &self.pak_files[self.current_pak_file_idx.unwrap()].reader;
        let pak_path = self.pak_files[self.current_pak_file_idx.unwrap()]
            .path
            .clone();

        let full_paths = pak.files().into_iter().collect::<Vec<_>>();

        ui.collapsing("Encryption details", |ui| {
            ui.horizontal(|ui| {
                ui.add(Label::new(RichText::new("Encryption: ").strong()));
                ui.add(Label::new(format!("{}", pak.encrypted_index())));
            });

            ui.horizontal(|ui| {
                ui.add(Label::new(RichText::new("Encryption GUID: ").strong()));
                ui.add(Label::new(format!("{:?}", pak.encryption_guid())));
            });
        });

        ui.collapsing("Pak details", |ui| {
            ui.horizontal(|ui| {
                ui.add(Label::new(RichText::new("Mount Point: ").strong()));
                ui.add(Label::new(pak.mount_point().to_string()));
            });

            ui.horizontal(|ui| {
                ui.add(Label::new(RichText::new("Path Hash Seed: ").strong()));
                ui.add(Label::new(format!("{:?}", pak.path_hash_seed())));
            });

            ui.horizontal(|ui| {
                ui.add(Label::new(RichText::new("Version: ").strong()));
                ui.add(Label::new(format!("{:?}", pak.version())));
            });
        });
        ui.horizontal(|ui| {
            ui.add(Label::new(
                RichText::new("Mod type: ")
                    .strong()
                    .size(self.default_font_size + 1.),
            ));
            let mut utoc_path = pak_path.to_path_buf();
            utoc_path.set_extension("utoc");

            let paths = {
                if utoc_path.exists() {
                    let file = read_utoc(&utoc_path, pak, &pak_path)
                        .iter()
                        .map(|entry| entry.file_path.clone())
                        .collect::<Vec<_>>();
                    file
                } else {
                    full_paths.clone()
                }
            };

            ui.add(Label::new(get_current_pak_characteristics(paths)));
        });
        if self.table.is_none() {
            self.table = Some(FileTable::new(pak, &pak_path));
        }
    }
    fn show_pak_files_in_dir(&mut self, ui: &mut egui::Ui) {
        // Enhanced scrolling with better performance for large mod lists
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height())
            .stick_to_bottom(false)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    // Create bubbly search and filter section
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            // Bubbly search bar
                            ui.horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Search:").strong().color(self.accent()));
                                let search_response = ui.add(
                                    TextEdit::singleline(&mut self.search_query)
                                        .hint_text("Search mods...")
                                        .desired_width(180.0)
                                );
                                
                                if search_response.changed() {
                                    self.update_search_filter();
                                }
                                
                                // Clear search button
                                if ui.add(egui::Button::new("Clear").corner_radius(egui::CornerRadius::same(8))).clicked() {
                                    self.search_query.clear();
                                    self.update_search_filter();
                                }
                                
                                ui.separator();

                                // Bubbly tag filter button
                                let filter_button_text = if self.tag_filter_enabled {
                                    format!("Filter ({} selected)", self.selected_tag_filters.len())
                                } else {
                                    "Filter".to_string()
                                };
                                
                                if ui.add(egui::Button::new(filter_button_text).corner_radius(egui::CornerRadius::same(12))).clicked() {
                                    self.show_tag_filter_dropdown = !self.show_tag_filter_dropdown;
                                }
                                
                                // Custom tag filter button
                                let custom_filter_text = if self.custom_tag_filter_enabled {
                                    format!("Custom Tags ({} selected)", self.selected_custom_tag_filters.len())
                                } else {
                                    "Custom Tags".to_string()
                                };

                                if ui.add(egui::Button::new(custom_filter_text).corner_radius(egui::CornerRadius::same(12))).clicked() {
                                    self.show_custom_tag_filter_dropdown = !self.show_custom_tag_filter_dropdown;
                                }

                                if ui.add(egui::Button::new("Clear Filters").corner_radius(egui::CornerRadius::same(12))).clicked() {
                                    self.tag_filter_enabled = false;
                                    self.selected_tag_filters.clear();
                                    self.custom_tag_filter_enabled = false;
                                    self.selected_custom_tag_filters.clear();
                                    self.update_search_filter();
                                }

                                ui.separator();
                                if ui.add(egui::Button::new("Tag Manager").corner_radius(egui::CornerRadius::same(12))).clicked() {
                                    self.show_tag_manager = !self.show_tag_manager;
                                }
                            });
                        });
                    });
                    
                    // Tag filter dropdown
                    if self.show_tag_filter_dropdown {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.vertical(|ui| {
                                ui.checkbox(&mut self.tag_filter_enabled, "Enable tag filtering");
                                
                                if self.tag_filter_enabled {
                                    ui.separator();
                                    ui.label("Select tags to show:");
                                    
                                    let all_types = self.get_all_mod_types();
                                    for mod_type in &all_types {
                                        let mut is_selected = self.selected_tag_filters.contains(mod_type);
                                        if ui.checkbox(&mut is_selected, mod_type).changed() {
                                            if is_selected {
                                                self.selected_tag_filters.insert(mod_type.clone());
                                            } else {
                                                self.selected_tag_filters.remove(mod_type);
                                            }
                                            self.update_search_filter();
                                        }
                                    }
                                    
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        if ui.button("Select All").clicked() {
                                            self.selected_tag_filters = all_types.iter().cloned().collect();
                                            self.update_search_filter();
                                        }
                                        if ui.button("Clear All").clicked() {
                                            self.selected_tag_filters.clear();
                                            self.update_search_filter();
                                        }
                                    });
                                }
                            });
                        });
                    }

                    // Global Tag Manager UI
                    if self.show_tag_manager {
                        ui.separator();
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Custom Tag Manager").strong().color(self.accent()));
                                ui.horizontal(|ui| {
                                    ui.label("Create new tag:");
                                    let resp = ui.add(TextEdit::singleline(&mut self.new_global_tag_input).hint_text("e.g. SFW, NSFW"));
                                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if enter || ui.button("Add").clicked() {
                                        let tag = self.new_global_tag_input.trim().to_string();
                                        if !tag.is_empty() {
                                            if !self.custom_tag_catalog.contains(&tag) {
                                                self.custom_tag_catalog.push(tag.clone());
                                                self.custom_tag_catalog.sort();
                                                self.custom_tag_catalog.dedup();
                                                let _ = self.save_state();
                                            }
                                            self.new_global_tag_input.clear();
                                        }
                                    }
                                });

                                ui.separator();
                                ui.label("Existing tags:");
                                let tags: Vec<String> = self.get_all_custom_tags().into_iter().collect();
                                for t in tags {
                                    ui.horizontal(|ui| {
                                        ui.label(t.clone());
                                        if ui.button("Rename").clicked() {
                                            self.rename_tag_from = Some(t.clone());
                                            self.rename_tag_to = t.clone();
                                        }
                                        if ui.button("Delete").clicked() {
                                            self.delete_custom_tag_global(&t);
                                            // Also remove from catalog
                                            self.custom_tag_catalog.retain(|x| x != &t);
                                            let _ = self.save_state();
                                        }
                                    });
                                }

                                if let Some(ref from) = self.rename_tag_from {
                                    let mut action_apply: Option<(String, String)> = None;
                                    let from_clone = from.clone();
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(format!("Rename '{}' to:", from_clone));
                                        let _ = ui.add(TextEdit::singleline(&mut self.rename_tag_to));
                                        if ui.button("Apply").clicked() {
                                            let new_name = self.rename_tag_to.trim().to_string();
                                            if !new_name.is_empty() {
                                                action_apply = Some((from_clone.clone(), new_name));
                                            }
                                        }
                                        if ui.button("Cancel").clicked() {
                                            self.rename_tag_from = None;
                                            self.rename_tag_to.clear();
                                        }
                                    });
                                    if let Some((from_name, new_name)) = action_apply {
                                        self.rename_custom_tag(&from_name, &new_name);
                                        if let Some(pos) = self.custom_tag_catalog.iter().position(|x| x == &from_name) {
                                            self.custom_tag_catalog[pos] = new_name.clone();
                                        } else if !self.custom_tag_catalog.contains(&new_name) {
                                            self.custom_tag_catalog.push(new_name.clone());
                                        }
                                        self.custom_tag_catalog.sort();
                                        self.custom_tag_catalog.dedup();
                                        let _ = self.save_state();
                                        self.rename_tag_from = None;
                                        self.rename_tag_to.clear();
                                    }
                                }
                            });
                        });
                    }
                    // Custom tags filter dropdown
                    if self.show_custom_tag_filter_dropdown {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.vertical(|ui| {
                                ui.checkbox(&mut self.custom_tag_filter_enabled, "Enable custom tag filtering");
                                
                                if self.custom_tag_filter_enabled {
                                    ui.separator();
                                    ui.label("Select custom tags to show:");
                                    
                                    let all_tags = self.get_all_custom_tags();
                                    for tag in &all_tags {
                                        let mut is_selected = self.selected_custom_tag_filters.contains(tag);
                                        if ui.checkbox(&mut is_selected, tag).changed() {
                                            if is_selected {
                                                self.selected_custom_tag_filters.insert(tag.clone());
                                            } else {
                                                self.selected_custom_tag_filters.remove(tag);
                                            }
                                            self.update_search_filter();
                                        }
                                    }
                                    
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        if ui.button("Select All").clicked() {
                                            self.selected_custom_tag_filters = all_tags.iter().cloned().collect();
                                            self.update_search_filter();
                                        }
                                        if ui.button("Clear All").clicked() {
                                            self.selected_custom_tag_filters.clear();
                                            self.update_search_filter();
                                        }
                                    });
                                }
                            });
                        });
                    }
                    
                    ui.separator();
                    
                    // Bubbly folder creation UI
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            if ui.add(egui::Button::new("New Folder").corner_radius(egui::CornerRadius::same(12))).clicked() {
                                self.creating_folder = true;
                            }
                            
                            if ui.add(egui::Button::new("Expand All").corner_radius(egui::CornerRadius::same(12))).clicked() {
                                for folder in &mut self.folders {
                                    folder.expanded = true;
                                }
                                self.save_state().ok();
                            }
                            
                            if ui.add(egui::Button::new("Collapse All").corner_radius(egui::CornerRadius::same(12))).clicked() {
                                for folder in &mut self.folders {
                                    folder.expanded = false;
                                }
                                self.save_state().ok();
                            }
                            
                            if self.creating_folder {
                                ui.separator();
                                ui.label(egui::RichText::new("Name:").strong());
                                let response = ui.add(TextEdit::singleline(&mut self.new_folder_name).desired_width(120.0));
                                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    if !self.new_folder_name.trim().is_empty() {
                                        self.create_folder(self.new_folder_name.clone());
                                        self.new_folder_name.clear();
                                        self.creating_folder = false;
                                    }
                                }
                                if ui.add(egui::Button::new("Create").corner_radius(egui::CornerRadius::same(8))).clicked() {
                                    if !self.new_folder_name.trim().is_empty() {
                                        self.create_folder(self.new_folder_name.clone());
                                        self.new_folder_name.clear();
                                        self.creating_folder = false;
                                    }
                                }
                                if ui.add(egui::Button::new("Cancel").corner_radius(egui::CornerRadius::same(8))).clicked() {
                                    self.creating_folder = false;
                                    self.new_folder_name.clear();
                                }
                            }
                        });
                    });

                    ui.separator();

                    // Display folders with bubbly styling
                    let folders_clone = self.folders.clone();
                    for folder in &folders_clone {
                        // Bubbly folder container
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.with_layout(egui::Layout::left_to_right(Align::LEFT), |ui| {
                                    ui.set_max_width(ui.available_width() * 0.85);
                                    ui.add_space(8.0);
                                    
                                    let folder_icon = if folder.expanded { "▼" } else { "▶" };
                                    let folder_label = format!("{} {}", folder_icon, folder.name);
                                    
                                    let folder_response = ui.add(
                                        Label::new(RichText::new(folder_label).strong().size(16.0).color(self.accent()))
                                        .selectable(false)
                                        .sense(egui::Sense::click())
                                    );
                                    
                                    if folder_response.clicked() {
                                        if let Some(f) = self.folders.iter_mut().find(|f| f.id == folder.id) {
                                            f.expanded = !f.expanded;
                                            self.save_state().ok();
                                        }
                                    }

                                    folder_response.context_menu(|ui| {
                                        if ui.button("Delete folder").clicked() {
                                            // Remove folder assignment from mods
                                            for mod_entry in &mut self.pak_files {
                                                if mod_entry.folder_id.as_ref() == Some(&folder.id) {
                                                    mod_entry.folder_id = None;
                                                }
                                            }
                                            // Remove folder
                                            self.folders.retain(|f| f.id != folder.id);
                                            self.save_state().ok();
                                            ui.close_menu();
                                        }
                                    });
                                });

                                ui.with_layout(egui::Layout::right_to_left(Align::RIGHT), |ui| {
                                    let mut folder_enabled = folder.enabled;
                                    let toggler = ui.add(ios_widget::toggle(&mut folder_enabled));
                                    if toggler.clicked() {
                                        self.toggle_folder(&folder.id);
                                    }
                                });
                            });
                        });

                        // Display mods in this folder with bubbly styling
                        let should_expand = folder.expanded || self.should_expand_folder_for_search(&folder.id);
                        if should_expand {
                            let folder_id = folder.id.clone();
                            let pak_files_len = self.pak_files.len();
                            for i in 0..pak_files_len {
                                if self.pak_files[i].folder_id.as_ref() == Some(&folder_id) && self.is_mod_visible(i) {
                                    // Bubbly mod entry container
                                    ui.group(|ui| {
                                        ui.set_width(ui.available_width() - 16.0);
                                        ui.horizontal(|ui| {
                                            ui.add_space(24.0); // Indent for folder contents
                                            
                                            // Highlight matched mods with bubbly colors
                                            let is_search_match = !self.search_query.trim().is_empty() && 
                                                self.filtered_mods.contains(&i);
                                            
                                            if is_search_match {
                                                ui.visuals_mut().override_text_color = Some(self.accent());
                                            }
                                            
                                            self.show_mod_entry_by_index(ui, i);
                                            
                                            if is_search_match {
                                                ui.visuals_mut().override_text_color = None;
                                            }
                                        });
                                    });
                                }
                            }
                        }
                    }

                    // Display ungrouped mods with bubbly styling
                    ui.separator();
                    ui.label(egui::RichText::new("Ungrouped Mods").strong().size(16.0).color(self.accent()));
                    let pak_files_len = self.pak_files.len();
                    for i in 0..pak_files_len {
                        if self.pak_files[i].folder_id.is_none() && self.is_mod_visible(i) {
                            // Bubbly ungrouped mod container
                            ui.group(|ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    // Highlight matched mods with bubbly colors
                                    let is_search_match = !self.search_query.trim().is_empty() && 
                                        self.filtered_mods.contains(&i);
                                    
                                    if is_search_match {
                                        ui.visuals_mut().override_text_color = Some(self.accent());
                                    }
                                    
                                    self.show_mod_entry_by_index(ui, i);
                                    
                                    if is_search_match {
                                        ui.visuals_mut().override_text_color = None;
                                    }
                                });
                            });
                        }
                    }
                });
            });
    }

    fn show_mod_entry_by_index(&mut self, ui: &mut egui::Ui, index: usize) {
        let display_name = self.get_mod_display_name(&self.pak_files[index]);
        let color = if self.current_pak_file_idx == Some(index) {
            self.accent()
        } else {
            ui.style().visuals.faint_bg_color
        };
        
        let mut should_save = false;
        let mut should_set_current = false;
        let mut new_table: Option<FileTable> = None;
        let mut should_toggle = false;
        let mut start_editing = false;
        let mut stop_editing = false;
        let mut new_custom_name: Option<String> = None;
        let mut reset_name = false;
        let mut new_folder_id: Option<Option<String>> = None;
        let folders_clone = self.folders.clone();
        // Custom tags temp state for this context menu
        let available_custom_tags: Vec<String> = self.get_all_custom_tags().into_iter().collect();
        let mut new_tag_input: String = String::new();
        let mut tags_to_toggle: Vec<(String, bool)> = Vec::new();
        
        // Get current state before borrowing
        let is_editing = self.pak_files[index].editing_name;
        let current_name = self.pak_files[index].custom_name.clone();
        let pak_enabled = self.pak_files[index].enabled;
        let pak_reader = self.pak_files[index].reader.clone();
        let pak_path = self.pak_files[index].path.clone();
        let has_custom_name = self.pak_files[index].custom_name.is_some();
        
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::left_to_right(Align::LEFT), |ui| {
                ui.set_max_width(ui.available_width() * 0.85);
                // Selection checkbox for bulk operations
                if self.selection_mode {
                    let mut checked = self.selected_mods.contains(&index);
                    if ui.checkbox(&mut checked, "").changed() {
                        if checked { self.selected_mods.insert(index); } else { self.selected_mods.remove(&index); }
                    }
                    ui.add_space(4.0);
                }

                if is_editing {
                    let mut temp_name = current_name.unwrap_or_else(|| {
                        pak_path.file_stem().unwrap().to_string_lossy().to_string()
                    });
                    
                    let response = ui.add(TextEdit::singleline(&mut temp_name).desired_width(200.0));
                    
                    // Request focus when starting to edit
                    response.request_focus();
                    
                    // Handle input events
                    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let escape_pressed = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    
                    if enter_pressed {
                        // Save the new name
                        new_custom_name = Some(temp_name.clone());
                        stop_editing = true;
                        should_save = true;
                    } else if escape_pressed {
                        // Cancel editing on Escape - revert to original name
                        stop_editing = true;
                    } else if response.lost_focus() && !temp_name.trim().is_empty() {
                        // Save when losing focus if name is not empty
                        new_custom_name = Some(temp_name.clone());
                        stop_editing = true;
                        should_save = true;
                    } else {
                        // Keep updating the custom name while editing
                        new_custom_name = Some(temp_name.clone());
                    }
                    
                    if ui.button("Save").clicked() {
                        new_custom_name = Some(temp_name);
                        stop_editing = true;
                        should_save = true;
                    }
                    
                    if ui.button("Cancel").clicked() {
                        stop_editing = true;
                    }
                } else {
                    // Bubbly mod entry styling
                    let pakfile = ui.add(
                        Label::new(RichText::new(display_name).strong().size(14.0).color(
                            if self.current_pak_file_idx == Some(index) { self.accent() } else { ui.style().visuals.text_color() }
                        ))
                        .truncate()
                        .selectable(true),
                    );

                    if pakfile.clicked() {
                        should_set_current = true;
                        new_table = Some(FileTable::new(&pak_reader, &pak_path));
                    }

                    pakfile.context_menu(|ui| {
                        if ui.button("Rename mod").clicked() {
                            start_editing = true;
                            ui.close_menu();
                            ui.ctx().request_repaint();
                        }
                        
                        if has_custom_name && ui.button("Reset to original name").clicked() {
                            reset_name = true;
                            should_save = true;
                            ui.close_menu();
                        }

                        ui.separator();
                        
                        ui.menu_button("Assign to folder", |ui| {
                            if ui.button("None").clicked() {
                                new_folder_id = Some(None);
                                should_save = true;
                                ui.close_menu();
                            }
                            
                            for folder in &folders_clone {
                                if ui.button(&folder.name).clicked() {
                                    new_folder_id = Some(Some(folder.id.clone()));
                                    should_save = true;
                                    ui.close_menu();
                                }
                            }
                        });

                        ui.separator();

                        // Custom Tags submenu: toggle existing tags, add new ones
                        ui.menu_button("Tags", |ui| {
                            ui.label("Assign or remove custom tags:");
                            ui.separator();

                            for tag in &available_custom_tags {
                                let mut has_tag = self.pak_files[index].custom_tags.contains(tag);
                                if ui.checkbox(&mut has_tag, tag).changed() {
                                    tags_to_toggle.push((tag.clone(), has_tag));
                                }
                            }

                            ui.separator();
                            ui.label("Create new tag:");
                            let resp = ui.add(TextEdit::singleline(&mut new_tag_input).hint_text("e.g. SFW, NSFW"));
                            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if enter || ui.button("Add").clicked() {
                                if !new_tag_input.trim().is_empty() {
                                    let tag = new_tag_input.trim().to_string();
                                    if !self.pak_files[index].custom_tags.contains(&tag) {
                                        self.pak_files[index].custom_tags.push(tag);
                                        self.pak_files[index].custom_tags.sort();
                                        self.pak_files[index].custom_tags.dedup();
                                        should_save = true;
                                        self.update_search_filter();
                                    }
                                    new_tag_input.clear();
                                    ui.close_menu();
                                }
                            }
                        });

                        ui.separator();

                        if ui.button("Extract pak to directory").clicked() {
                            should_set_current = true;
                            let dir = rfd::FileDialog::new().pick_folder();
                            if let Some(dir) = dir {
                                let mod_name = pak_path.file_stem().unwrap().to_string_lossy().to_string();
                                let to_create = dir.join(&mod_name);
                                fs::create_dir_all(&to_create).unwrap();

                                let installable_mod = InstallableMod {
                                    mod_name: mod_name.clone(),
                                    mod_type: "".to_string(),
                                    reader: Option::from(pak_reader.clone()),
                                    mod_path: pak_path.clone(),
                                    ..Default::default()
                                };
                                if let Err(e) = extract_pak_to_dir(&installable_mod, to_create) {
                                    error!("Failed to extract pak directory: {}", e);
                                }
                            }
                            ui.close_menu();
                        }
                        
                        let is_deleting_this = self.deleting_mods.contains(&pak_path);
                        let del_btn = ui.add_enabled(!is_deleting_this, egui::Button::new("Delete mod"));
                        if del_btn.clicked() {
                            // Queue deletion on background thread (non-blocking)
                            self.ensure_delete_worker();
                            let utoc_path = pak_path.with_extension("utoc");
                            let ucas_path = pak_path.with_extension("ucas");
                            // Enqueue original paths; background worker handles rename+delete
                            let files_to_delete = vec![pak_path.clone(), utoc_path, ucas_path];
                            if let Some(tx) = &self.delete_sender {
                                // Track as deleting to prevent double-actions
                                self.deleting_mods.insert(pak_path.clone());
                                // Defer actual UI list mutation until after iteration
                                self.pending_remove_paths.push(pak_path.clone());
                                if let Err(e) = tx.send(files_to_delete) {
                                    error!("Failed to queue delete: {}", e);
                                    // If we failed to enqueue, clear deleting state
                                    self.deleting_mods.remove(&pak_path);
                                    if let Some(pos) = self.pending_remove_paths.iter().position(|p| p == &pak_path) { self.pending_remove_paths.remove(pos); }
                                }
                            } else {
                                error!("Delete worker not available");
                            }
                            should_set_current = false;
                            // Immediately clear selection to prevent heavy details UI from running
                            self.current_pak_file_idx = None;
                            self.table = None;
                            ui.close_menu();
                            ui.ctx().request_repaint();
                        }
                    });
                    if ui.add(egui::Button::new("✏").corner_radius(egui::CornerRadius::same(8))).clicked() {
                        start_editing = true;
                    }

                    // Show custom tag chips for this mod
                    if !self.pak_files[index].custom_tags.is_empty() {
                        ui.add_space(8.0);
                        ui.horizontal_wrapped(|ui| {
                            for tag in &self.pak_files[index].custom_tags {
                                let chip = egui::Button::new(
                                    egui::RichText::new(tag).size(10.0)
                                )
                                .fill(ui.style().visuals.extreme_bg_color)
                                .frame(true)
                                .corner_radius(egui::CornerRadius::same(10))
                                .small();
                                let _ = ui.add(chip);
                            }
                        });
                    }

                    // Inline quick tag editor menu next to the entry
                    let mut inline_toggles: Vec<(String, bool)> = Vec::new();
                    ui.menu_button("🏷", |ui| {
                        ui.set_min_width(200.0);
                        ui.label("Tags for this mod:");
                        ui.separator();
                        let all_tags = self.get_all_custom_tags();
                        for t in &all_tags {
                            let mut has_tag = self.pak_files[index].custom_tags.contains(t);
                            if ui.checkbox(&mut has_tag, t).changed() {
                                inline_toggles.push((t.clone(), has_tag));
                            }
                        }
                    });
                    if !inline_toggles.is_empty() {
                        for (t, add) in inline_toggles {
                            if add {
                                if !self.pak_files[index].custom_tags.contains(&t) {
                                    self.pak_files[index].custom_tags.push(t);
                                }
                            } else {
                                self.pak_files[index].custom_tags.retain(|x| x != &t);
                            }
                        }
                        self.pak_files[index].custom_tags.sort();
                        self.pak_files[index].custom_tags.dedup();
                        should_save = true;
                        self.update_search_filter();
                    }
                }
            });
            
            ui.with_layout(egui::Layout::right_to_left(Align::RIGHT), |ui| {
                let mut enabled = pak_enabled;
                let toggler = ui.add(ios_widget::toggle(&mut enabled));
                if toggler.clicked() {
                    should_toggle = true;
                    should_save = true;
                }
            });
        });
        
        // Apply changes after UI is done
        if start_editing {
            self.pak_files[index].editing_name = true;
        }
        
        if stop_editing {
            self.pak_files[index].editing_name = false;
        }
        
        if let Some(name) = new_custom_name {
            self.pak_files[index].custom_name = Some(name);
        }
        
        if reset_name {
            self.pak_files[index].custom_name = None;
        }
        
        if let Some(folder_id) = new_folder_id {
            self.pak_files[index].folder_id = folder_id;
        }
        
        if should_toggle {
            // Handle toggle manually to avoid borrowing conflicts
            let pak_file = &mut self.pak_files[index];
            pak_file.enabled = !pak_file.enabled;
            
            if pak_file.enabled {
                let new_pak = pak_file.path.with_extension("pak");
                match std::fs::rename(&pak_file.path, &new_pak) {
                    Ok(_) => {
                        pak_file.path = new_pak;
                        info!("Enabled mod: {:?}", pak_file.path);
                    }
                    Err(e) => {
                        warn!("Failed to enable mod: {:?}", e);
                        pak_file.enabled = false;
                    }
                }
            } else {
                let new_pak = pak_file.path.with_extension("bak_repak");
                match std::fs::rename(&pak_file.path, &new_pak) {
                    Ok(_) => {
                        pak_file.path = new_pak;
                        info!("Disabled mod: {:?}", pak_file.path);
                    }
                    Err(e) => {
                        warn!("Failed to disable mod: {:?}", e);
                        pak_file.enabled = true;
                    }
                }
            }
        }
        
        // Apply tag toggles
        if !tags_to_toggle.is_empty() {
            for (tag, add) in tags_to_toggle {
                if add {
                    if !self.pak_files[index].custom_tags.contains(&tag) {
                        self.pak_files[index].custom_tags.push(tag);
                    }
                } else {
                    self.pak_files[index].custom_tags.retain(|t| t != &tag);
                }
            }
            self.pak_files[index].custom_tags.sort();
            self.pak_files[index].custom_tags.dedup();
            should_save = true;
            self.update_search_filter();
        }

        if should_save {
            self.save_state().ok();
        }
        
        if should_set_current {
            self.current_pak_file_idx = Some(index);
            if let Some(table) = new_table {
                self.table = Some(table);
            }
        }
    }
    fn config_path() -> PathBuf {
        let mut path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("repak_manager");

        debug!("Config path: {}", path.to_string_lossy());
        if !path.exists() {
            fs::create_dir_all(&path).unwrap();
            info!("Created config directory: {}", path.to_string_lossy());
        }

        path.push("repak_mod_manager.json");

        path
    }

    fn load(ctx: &eframe::CreationContext) -> std::io::Result<Self> {
        let (tx, rx) = channel();
        let path = Self::config_path();
        let mut shit = if path.exists() {
            info!("Loading config: {}", path.to_string_lossy());
            let data = fs::read_to_string(path)?;
            let mut config: Self = serde_json::from_str(&data)?;

            debug!("Setting custom style");
            setup_custom_style(&ctx.egui_ctx);
            debug!("Setting font size: {}", config.default_font_size);
            set_custom_font_size(&ctx.egui_ctx, config.default_font_size);
            if config.use_custom_palette {
                debug!("Applying custom palette");
                let mut style = (*ctx.egui_ctx.style()).clone();
                config.apply_custom_palette_to_style(&mut style);
                ctx.egui_ctx.set_style(style);
            }

            info!("Loading mods: {}", config.game_path.to_string_lossy());
            config.collect_pak_files();
            config.update_search_filter();

            let mut show_welcome = false;
            if let Some(ref version) = config.version {
                if version != VERSION {
                    show_welcome = true;
                }
            } else {
                show_welcome = true;
            }
            config.version = Option::from(VERSION.to_string());
            config.hide_welcome = !show_welcome;
            config.welcome_screen = Some(ShowWelcome{});
            config.receiver = Some(rx);

            Ok(config)
        } else {
            info!(
                "First Launch creating new directory: {}",
                path.to_string_lossy()
            );
            let mut x = Self::new(ctx);
            x.welcome_screen = Some(ShowWelcome{});
            x.hide_welcome=false;
            x.receiver = Some(rx);
            Ok(x)
        };

        if let Ok(ref mut shit) = shit {
            let path = shit.game_path.clone();
            thread::spawn(move || {
                let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| {
                    if let Ok(event) = res {
                        tx.send(event).unwrap();
                    }
                })
                .unwrap();

                if path.exists() {
                    watcher.watch(&path, RecursiveMode::Recursive).unwrap();
                }

                // Keep the thread alive
                loop {
                    thread::sleep(Duration::from_secs(1));
                }
            });
            shit.collect_pak_files();
        }

        shit
    }
    fn save_state(&mut self) -> std::io::Result<()> {
        // Sync pak_files metadata back to mod_metadata for persistence
        self.sync_metadata();
        
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self)?;
        info!("Saving config: {}", path.to_string_lossy());
        fs::write(path, json)?;
        Ok(())
    }
    
    fn sync_metadata(&mut self) {
        // Clear existing metadata and rebuild from current pak_files
        self.mod_metadata.clear();
        
        for pak_file in &self.pak_files {
            let metadata = ModMetadata {
                path: pak_file.path.clone(),
                custom_name: pak_file.custom_name.clone(),
                folder_id: pak_file.folder_id.clone(),
                custom_tags: pak_file.custom_tags.clone(),
            };
            self.mod_metadata.push(metadata);
        }
    }

    /// Preview hovering files:
    fn preview_files_being_dropped(&self, ctx: &egui::Context, rect: egui::Rect) {
        use egui::{Align2, Color32, Id, LayerId, Order, TextStyle};

        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            let painter =
                ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file_drop_target")));

            let msg = match self.game_path.is_dir() {
                true => "Drop mod files here",
                false => "Choose a game directory first!!!",
            };
            painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(241, 24, 14, 40));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                msg,
                TextStyle::Heading.resolve(&ctx.style()),
                Color32::WHITE,
            );
        }
    }

    fn check_drop(&mut self, ctx: &egui::Context) {
        if !self.game_path.is_dir() {
            return;
        }
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                let dropped_files = i.raw.dropped_files.clone();
                // Check if all files are either directories or have the .pak extension
                let all_valid = dropped_files.iter().all(|file| {
                    let path = file.path.clone().unwrap();
                    path.is_dir()
                        || path
                            .extension()
                            .map(|ext| ext == "pak" || ext == "zip" || ext == "rar")
                            .unwrap_or(false)
                });

                if all_valid {
                    let mods = map_dropped_file_to_mods(&dropped_files);
                    if mods.is_empty() {
                        error!("No mods found in dropped files.");
                        return;
                    }
                    self.file_drop_viewport_open = true;
                    debug!("Mods: {:?}", mods);
                    self.install_mod_dialog =
                        Some(ModInstallRequest::new(mods, self.game_path.clone()));

                    if let Some(dialog) = &self.install_mod_dialog {
                        trace!("Installing mod: {:#?}", dialog.mods);
                    }
                } else {
                    // Handle the case where not all dropped files are valid
                    // You can show an error or prompt the user here
                    warn!(
                        "Not all files are valid. Only directories or .pak files are allowed."
                    );
                }
            }
        });
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui) -> Result<(), repak::Error> {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                let msg = match self.game_path.is_dir() {
                    true => "Drop mod files here",
                    false => "Choose a game directory first!!!",
                };

                if ui
                    .add_enabled(self.game_path.is_dir(), Button::new("Install mods").corner_radius(egui::CornerRadius::same(8)))
                    .on_hover_text(msg)
                    .clicked()
                {
                    ui.close_menu(); // Closes the menu
                    let mod_files = rfd::FileDialog::new()
                        .set_title("Pick mods")
                        .pick_files()
                        .unwrap_or_default();

                    if mod_files.is_empty() {
                        error!("No mods found in dropped files.");
                        return;
                    }

                    let mods = map_paths_to_mods(&mod_files);
                    if mods.is_empty() {
                        error!("No mods found in dropped files.");
                        return;
                    }

                    self.file_drop_viewport_open = true;
                    self.install_mod_dialog =
                        Some(ModInstallRequest::new(mods, self.game_path.clone()));
                }

                if ui
                    .add_enabled(self.game_path.is_dir(), Button::new("Pack folder").corner_radius(egui::CornerRadius::same(8)))
                    .on_hover_text(msg)
                    .clicked()
                {
                    ui.close_menu(); // Closes the menu
                    let mod_files = rfd::FileDialog::new()
                        .set_title("Pick mods")
                        .pick_folders()
                        .unwrap_or_default();

                    if mod_files.is_empty() {
                        error!("No folders picked. Please pick a folder with mods in it.");
                        return;
                    }

                    let mods = map_paths_to_mods(&mod_files);
                    if mods.is_empty() {
                        error!("No mods found in dropped files.");
                        return;
                    }
                    self.file_drop_viewport_open = true;
                    self.install_mod_dialog =
                        Some(ModInstallRequest::new(mods, self.game_path.clone()));
                }
                if ui.add(Button::new("Quit").corner_radius(egui::CornerRadius::same(8))).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.menu_button("Settings", |ui| {
                ui.add(
                    egui::Slider::new(&mut self.default_font_size, 12.0..=32.0).text("Font size"),
                );
                set_custom_font_size(ui.ctx(), self.default_font_size);
                ui.horizontal(|ui| {
                    let mode = match ui.ctx().style().visuals.dark_mode {
                        true => "Switch to light mode",
                        false => "Switch to dark mode",
                    };
                    ui.add(egui::Label::new(mode).halign(Align::Center));
                    egui::widgets::global_theme_preference_switch(ui);
                });

                ui.separator();
                ui.checkbox(&mut self.use_custom_palette, "Enable custom color palette");
                if self.use_custom_palette {
                    if ui.button("Open Palette Editor...").clicked() {
                        self.show_palette_window = true;
                        ui.close_menu();
                    }
                }
            });

            if ui.add(Button::new("💖 Donate").corner_radius(egui::CornerRadius::same(12))).clicked() {
                self.hide_welcome = false;
            }
        });

        Ok(())
    }

    fn show_file_dialog(&mut self, ui: &mut egui::Ui) {
        Flex::horizontal()
            .w_full()
            .align_items(FlexAlign::Center)
            .show(ui, |flex_ui| {
                flex_ui.add(item(), Label::new("Mod folder:"));
                // Bind the text field to a persistent buffer and sync back to game_path
                let resp = flex_ui.add(
                    item().grow(1.0),
                    TextEdit::singleline(&mut self.game_path_input)
                        .hint_text("Type or paste a path..."),
                );
                if resp.changed() {
                    self.game_path = PathBuf::from(self.game_path_input.clone());
                }
                let browse_button = flex_ui.add(item(), Button::new("Browse").corner_radius(egui::CornerRadius::same(8)));
                if browse_button.clicked() {
                    if let Some(path) = FileDialog::new().pick_folder() {
                        self.game_path = path;
                        self.game_path_input = self.game_path.to_string_lossy().to_string();
                    }
                }
                flex_ui.add_ui(item(), |ui| {
                    let x = ui.add_enabled(self.game_path.exists(), Button::new("Open mod folder").corner_radius(egui::CornerRadius::same(8)));
                    if x.clicked() {
                        info!("Opening mod folder: {}", self.game_path.to_string_lossy());
                        #[cfg(target_os = "windows")]
                        {
                            let process = std::process::Command::new("explorer.exe")
                                .arg(self.game_path.clone())
                                .spawn();

                            if let Err(e) = process {
                                error!("Failed to open folder: {}", e);
                                return;
                            } else {
                                info!("Opened mod folder: {}", self.game_path.to_string_lossy());
                            }
                            process.unwrap().wait().unwrap();
                        }

                        #[cfg(target_os = "linux")]
                        {
                            debug!("Opening mod folder: {}", self.game_path.to_string_lossy());
                            let _ = std::process::Command::new("xdg-open")
                                .arg(self.game_path.to_string_lossy().to_string())
                                .spawn();
                        }
                    }
                });
            });
    }
}
impl eframe::App for RepakModManager {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Re-apply custom palette on every frame if enabled, so other flows don't reset it
        if self.use_custom_palette {
            let mut style = (*ctx.style()).clone();
            self.apply_custom_palette_to_style(&mut style);
            ctx.set_style(style);
        }
        if let Some(ref mut welcome) = self.welcome_screen{
            if !self.hide_welcome{
                welcome.welcome_screen(ctx,&mut self.hide_welcome);
            }
        }

        let mut collect_pak = false;

        if !self.file_drop_viewport_open && self.install_mod_dialog.is_some() {
            self.install_mod_dialog = None;
        }

        // Poll background delete results (non-blocking) and schedule refresh
        if let Some(ref rx) = self.delete_results {
            while let Ok(res) = rx.try_recv() {
                match res {
                    Ok(paths) => {
                        // Best-effort: first element is the pak path we queued
                        if let Some(pak_p) = paths.get(0) { self.deleting_mods.remove(pak_p); }
                        self.refresh_after_delete = true;
                    }
                    Err(err) => {
                        error!("Delete failed: {}", err);
                        self.deleting_mods.clear();
                        self.refresh_after_delete = true;
                    }
                }
            }
        }

        // Apply any pending removals immediately to drop file handles and reduce UI work
        if !self.pending_remove_paths.is_empty() {
            let mut to_remove = std::mem::take(&mut self.pending_remove_paths);
            to_remove.sort();
            to_remove.dedup();
            // Remove from pak_files
            self.pak_files.retain(|m| !to_remove.contains(&m.path));
            // Remove metadata entries as well
            self.mod_metadata.retain(|md| !to_remove.contains(&md.path));
            // Clear selection/table and refresh filter
            self.current_pak_file_idx = None;
            self.table = None;
            self.update_search_filter();
            // No need to collect here; deletion worker will trigger a final refresh
        }

        // If a deletion happened last frame, safely refresh state now
        if self.refresh_after_delete {
            self.current_pak_file_idx = None;
            self.table = None;
            self.collect_pak_files();
            self.update_search_filter();
            self.refresh_after_delete = false;
        }

        if self.install_mod_dialog.is_none() {
            if let Some(ref receiver) = &self.receiver {
                while let Ok(event) = receiver.try_recv() {
                    match event.kind {
                        EventKind::Any => {
                            warn!("Unknown event received")
                        }
                        EventKind::Other => {}
                        _ => {
                            // If a background delete is in-flight, defer heavy refresh
                            if self.deleting_mods.is_empty() {
                                collect_pak = true;
                            } else {
                                self.refresh_after_delete = true;
                            }
                        }
                    }
                }
            }
        }
        // if install_mod_dialog is open we dont want to listen to events

        if collect_pak && self.deleting_mods.is_empty() {
            trace!("Collecting pak files");
            self.collect_pak_files();
        } else if collect_pak {
            // Defer to after delete completes
            self.refresh_after_delete = true;
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            if let Err(e) = self.show_menu_bar(ui) {
                error!("Error: {}", e);
            }

            ui.separator();
            self.show_file_dialog(ui);

            // Bulk actions toolbar
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.selection_mode, "Selection mode");
                if self.selection_mode {
                    let count = self.selected_mods.len();
                    ui.label(format!("Selected: {}", count));
                    let can_delete = count > 0;
                    if ui.add_enabled(can_delete, Button::new("Delete selected mods").corner_radius(egui::CornerRadius::same(8))).clicked() {
                        // Ensure worker exists
                        self.ensure_delete_worker();

                        // Build list of base pak paths from selected indices
                        let mut base_paths: Vec<std::path::PathBuf> = Vec::new();
                        for &i in &self.selected_mods {
                            if let Some(m) = self.pak_files.get(i) {
                                base_paths.push(m.path.clone());
                            }
                        }

                        // Prepare files to delete: try fast rename to .pending_delete first
                        let mut files_to_delete: Vec<std::path::PathBuf> = Vec::new();
                        for pak_path in &base_paths {
                            let utoc_path = pak_path.with_extension("utoc");
                            let ucas_path = pak_path.with_extension("ucas");
                            for p in [pak_path, &utoc_path, &ucas_path] {
                                if !p.exists() { continue; }
                                let mut tmp = p.clone();
                                let mut ext = tmp.extension().and_then(|s| s.to_str()).unwrap_or("").to_string();
                                if ext.is_empty() { ext = "pending_delete".to_string(); } else { ext.push_str(".pending_delete"); }
                                tmp.set_extension(ext);
                                match std::fs::rename(p, &tmp) {
                                    Ok(_) => files_to_delete.push(tmp),
                                    Err(_e) => files_to_delete.push(p.clone()),
                                }
                            }
                        }

                        // Queue one batch job; if channel fails, log and skip
                        let mut queued = false;
                        if let Some(tx) = &self.delete_sender {
                            if tx.send(files_to_delete).is_ok() { queued = true; }
                        }

                        // Update UI state regardless; background worker will finish deletion
                        // Mark each as deleting and remove from UI list next frame
                        for p in base_paths {
                            self.deleting_mods.insert(p.clone());
                            self.pending_remove_paths.push(p);
                        }
                        // Clear selection and current table
                        self.selected_mods.clear();
                        self.current_pak_file_idx = None;
                        self.table = None;
                        // Schedule refresh
                        self.refresh_after_delete = true;
                        ui.ctx().request_repaint();
                        if !queued { error!("Failed to queue bulk delete"); }
                    }
                }
            });
        });

        egui::SidePanel::left("left_panel")
            .min_width(300.)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.set_height(ui.available_height());
                    ui.label("Mod files");
                    ui.group(|ui| {
                        ui.set_width(ui.available_width());
                        ui.set_height(ui.available_height());
                        self.show_pak_files_in_dir(ui);
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Files section (top)
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(ui.available_height() * 0.6);
                    self.list_pak_contents(ui).expect("TODO: panic message");
                });

                ui.separator();

                // Details section (below Files)
                ui.label("Details");
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.set_height(ui.available_height());
                    self.show_pak_details(ui);
                });
            });
        });

        // Palette editor window (persistent, won't close on slider drag)
        if self.use_custom_palette && self.show_palette_window {
            let mut open = self.show_palette_window;
            egui::Window::new("Palette Editor")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label("Customize colors:");

                    let mut accent = Color32::from_rgba_unmultiplied(
                        self.custom_palette.accent[0],
                        self.custom_palette.accent[1],
                        self.custom_palette.accent[2],
                        self.custom_palette.accent[3],
                    );
                    let mut panel = CustomPalette::rgba(self.custom_palette.panel_fill);
                    let mut window = CustomPalette::rgba(self.custom_palette.window_fill);
                    let mut w_inactive = CustomPalette::rgba(self.custom_palette.widget_inactive);
                    let mut w_hovered = CustomPalette::rgba(self.custom_palette.widget_hovered);
                    let mut w_active = CustomPalette::rgba(self.custom_palette.widget_active);
                    let mut w_open = CustomPalette::rgba(self.custom_palette.widget_open);
                    let mut text = self
                        .custom_palette
                        .text
                        .map(CustomPalette::rgba)
                        .unwrap_or(ui.style().visuals.text_color());

                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut accent);
                        ui.label("Accent");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut panel);
                        ui.label("Panel fill");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut window);
                        ui.label("Window fill");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut w_inactive);
                        ui.label("Widget inactive");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut w_hovered);
                        ui.label("Widget hovered");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut w_active);
                        ui.label("Widget active");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut w_open);
                        ui.label("Widget open");
                    });
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut text);
                        ui.label("Text color");
                    });

                    // Persist back to palette
                    self.custom_palette.accent = [accent.r(), accent.g(), accent.b(), accent.a()];
                    self.custom_palette.panel_fill = [panel.r(), panel.g(), panel.b(), panel.a()];
                    self.custom_palette.window_fill = [window.r(), window.g(), window.b(), window.a()];
                    self.custom_palette.widget_inactive = [
                        w_inactive.r(),
                        w_inactive.g(),
                        w_inactive.b(),
                        w_inactive.a(),
                    ];
                    self.custom_palette.widget_hovered = [
                        w_hovered.r(),
                        w_hovered.g(),
                        w_hovered.b(),
                        w_hovered.a(),
                    ];
                    self.custom_palette.widget_active = [
                        w_active.r(),
                        w_active.g(),
                        w_active.b(),
                        w_active.a(),
                    ];
                    self.custom_palette.widget_open = [
                        w_open.r(),
                        w_open.g(),
                        w_open.b(),
                        w_open.a(),
                    ];
                    self.custom_palette.text = Some([text.r(), text.g(), text.b(), text.a()]);

                    // Apply immediately
                    let mut style = (*ui.ctx().style()).clone();
                    self.apply_custom_palette_to_style(&mut style);
                    ui.ctx().set_style(style);

                    ui.separator();
                    // Preset management UI
                    ui.label("Presets:");
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut self.preset_name_input);
                        let can_save = !self.preset_name_input.trim().is_empty();
                        if ui.add_enabled(can_save, egui::Button::new("Save Preset").corner_radius(egui::CornerRadius::same(8))).clicked() {
                            let name = self.preset_name_input.trim();
                            if let Err(e) = self.save_palette_preset(name) {
                                error!("Failed to save preset: {}", e);
                            }
                        }
                    });

                    ui.separator();
                    let presets = self.list_palette_presets();
                    for p in presets {
                        ui.horizontal(|ui| {
                            ui.label(&p);
                            if ui.button("Load").clicked() {
                                if let Err(e) = self.load_palette_preset(&p) {
                                    error!("Failed to load preset: {}", e);
                                } else {
                                    let mut style = (*ui.ctx().style()).clone();
                                    self.apply_custom_palette_to_style(&mut style);
                                    ui.ctx().set_style(style);
                                    self.save_state().ok();
                                }
                            }
                            if ui.button("Delete").clicked() {
                                if let Err(e) = self.delete_palette_preset(&p) {
                                    error!("Failed to delete preset: {}", e);
                                }
                            }
                        });
                    }

                    ui.separator();
                    if ui.button("Close").clicked() {
                        self.show_palette_window = false;
                    }
                    ui.add_space(4.0);
                    ui.label("Changes are saved automatically.");
                    self.save_state().ok();
                });
            self.show_palette_window = open;
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_state().unwrap();
        }
        self.check_drop(ctx);
        if let Some(ref mut install_mod) = self.install_mod_dialog {
            if self.file_drop_viewport_open {
                install_mod.new_mod_dialog(ctx, &mut self.file_drop_viewport_open);
            }
        }
    }
}

const ICON: LazyCell<Arc<IconData>> = LazyCell::new(|| {
    let d = eframe::icon_data::from_png_bytes(include_bytes!(
        "../../repak-gui/icons/RepakLogoNonCurveFadedRed-modified.png"
    ))
    .expect("The icon data must be valid");

    Arc::new(d)
});

#[cfg(target_os = "windows")]
fn free_console() -> bool {
    unsafe { FreeConsole() == 0 }
}
#[cfg(target_os = "windows")]
fn is_console() -> bool {
    unsafe {
        let mut buffer = [0u32; 1];
        let count = GetConsoleProcessList(buffer.as_mut_ptr(), 1);
        count != 1
    }
}
#[cfg(target_os = "windows")]
#[link(name = "Kernel32")]
extern "system" {
    fn GetConsoleProcessList(processList: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> i32;
}
#[allow(unused_imports)]
#[cfg(target_os = "windows")]
use std::panic::PanicHookInfo;
use crate::welcome::ShowWelcome;

#[cfg(target_os = "windows")]
#[cfg(not(debug_assertions))]
fn custom_panic(_info: &PanicHookInfo) -> ! {
    let msg = format!(
        "Repak has crashed. Please report this issue to the developer with the following information:\
\n\n{}\
\nAdditonally include the log file in the bug report"
        ,_info);

    let _x = rfd::MessageDialog::new()
        .set_title("Repak has crashed")
        .set_buttons(MessageButtons::Ok)
        .set_description(msg)
        .show();
    std::process::exit(1);
}

fn main() {
    #[cfg(target_os = "windows")]
    if !is_console() {
        free_console();
    }
    #[cfg(target_os = "windows")]
    #[cfg(not(debug_assertions))]
    std::panic::set_hook(Box::new(move |info| {
        custom_panic(info.into());
    }));

    unsafe {
        #[cfg(target_os = "linux")]
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
        std::env::remove_var("WAYLAND_DISPLAY");
    }

    let log_file = File::create("latest.log").expect("Failed to create log file");
    let level_filter = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    CombinedLogger::init(vec![
        TermLogger::new(
            level_filter,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(LevelFilter::Info, Config::default(), log_file),
    ])
    .expect("Failed to initialize logger");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1366.0, 768.0])
            .with_min_inner_size([1100.0, 650.])
            .with_drag_and_drop(true)
            .with_icon(ICON.clone()),
        ..Default::default()
    };

    eframe::run_native(
        "Repak GUI",
        options,
        Box::new(|cc| {
            cc.egui_ctx
                .style_mut(|style| style.visuals.dark_mode = true);
            Ok(Box::new(
                RepakModManager::load(cc).expect("Unable to load config"),
            ))
        }),
    )
    .expect("Unable to spawn windows");
}
