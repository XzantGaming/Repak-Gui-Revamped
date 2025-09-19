pub mod install_mod_logic;

use crate::install_mod::install_mod_logic::archives::*;
use crate::install_mod::install_mod_logic::pak_files::create_repak_from_pak;
use crate::uasset_detection::{detect_mesh_files, detect_texture_files};
use crate::utils::{collect_files, get_current_pak_characteristics};
use crate::utoc_utils::read_utoc;
use crate::{setup_custom_style, ICON};
use eframe::egui;
use eframe::egui::{Align, Checkbox, ComboBox, Context, Label, TextEdit};
use std::collections::BTreeSet;
use std::fs;
use dirs;
use serde_json::Value as JsonValue;
use egui_extras::{Column, TableBuilder};
use egui_flex::{item, Flex, FlexAlign};
use install_mod_logic::install_mods_in_viewport;
use log::{debug, error};
use repak::utils::AesKey;
use repak::Compression::Oodle;
use repak::{Compression, PakReader};
use serde::de::Unexpected::Str;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicBool, AtomicI32};
use std::sync::{Arc, LazyLock};
use std::thread;
use tempfile::tempdir;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct InstallableMod {
    pub mod_name: String,
    pub mod_type: String,
    pub custom_tags: Vec<String>,
    pub custom_tag_input: String,
    pub repak: bool,
    pub fix_mesh: bool,
    pub fix_textures: bool,
    pub is_dir: bool,
    pub editing: bool,
    pub path_hash_seed: String,
    pub mount_point: String,
    pub compression: Compression,
    pub reader: Option<PakReader>,
    pub mod_path: PathBuf,
    pub total_files: usize,
    pub iostore: bool,
    // the only reason we keep this is to filter out the archives during collection
    pub is_archived: bool,
    pub enabled: bool,
    // pub audio_mod: bool,
}

impl Default for InstallableMod {
    fn default() -> Self {
        InstallableMod{
            mod_name: "".to_string(),
            mod_type: "".to_string(),
            custom_tags: Vec::new(),
            custom_tag_input: String::new(),
            repak: false,
            fix_mesh: false,
            fix_textures: false,
            is_dir: false,
            editing: false,
            path_hash_seed: "".to_string(),
            mount_point: "".to_string(),
            compression: Default::default(),
            reader: None,
            mod_path: Default::default(),
            total_files: 0,
            iostore: false,
            is_archived: false,
            enabled: true,
        }
    }
}

#[derive(Debug)]
pub struct ModInstallRequest {
    pub(crate) mods: Vec<InstallableMod>,
    pub mod_directory: PathBuf,
    pub animate: bool,
    pub total_mods: f32,
    pub installed_mods_cbk: Arc<AtomicI32>,
    pub joined_thread: Option<thread::JoinHandle<()>>,
    pub stop_thread: Arc<AtomicBool>,
    // Filtering state
    pub filter_enabled: bool,
    pub selected_filter_types: std::collections::HashSet<String>,
    pub show_unknown_tagging_dialog: bool,
    pub unknown_mod_being_tagged: Option<usize>,
    pub new_tag_input: String,
}
impl ModInstallRequest {
    pub fn new(mods: Vec<InstallableMod>, mod_directory: PathBuf) -> Self {
        let len = mods.iter().map(|m| m.total_files).sum::<usize>();
        Self {
            animate: false,
            mods,
            mod_directory,
            total_mods: len as f32,
            installed_mods_cbk: Arc::new(AtomicI32::new(0)),
            joined_thread: None,
            stop_thread: Arc::new(AtomicBool::new(false)),
            filter_enabled: false,
            selected_filter_types: std::collections::HashSet::new(),
            show_unknown_tagging_dialog: false,
            unknown_mod_being_tagged: None,
            new_tag_input: String::new(),
        }
    }
}

impl ModInstallRequest {
    pub fn new_mod_dialog(&mut self, ctx: &egui::Context, show_callback: &mut bool) {
        let viewport_options = egui::ViewportBuilder::default()
            .with_title("Install mods")
            .with_icon(ICON.clone())
            .with_inner_size([1000.0, 800.0])
            .with_always_on_top();

        Context::show_viewport_immediate(
            ctx,
            egui::ViewportId::from_hash_of("immediate_viewport"),
            viewport_options,
            |ctx, class| {
                assert!(
                    class == egui::ViewportClass::Immediate,
                    "This egui backend doesn't support multiple viewports"
                );

                setup_custom_style(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.label("Mods to install");
                    
                    // Add filtering UI
                    self.show_filter_ui(ui);
                    ui.separator();
                    
                    self.table_ui(ui);
                });
                egui::TopBottomPanel::bottom("bottom_panel")
                    .min_height(50.)
                    .show(ctx, |ui| {
                        Flex::horizontal()
                            .align_items(FlexAlign::Center)
                            .w_auto()
                            .h_auto()
                            .show(ui, |ui| {
                                let selection_bg_color = ctx.style().visuals.selection.bg_fill;

                                let install_mod = ui.add(
                                    item(),
                                    egui::Button::new("Install mod").fill(selection_bg_color),
                                );

                                let cancel = ui.add(item(), egui::Button::new("Cancel"));
                                cancel.clicked().then(|| {
                                    self.stop_thread.store(true, SeqCst);
                                    *show_callback = false;
                                });

                                if install_mod.clicked() {
                                    let mut mods = self.mods.to_vec(); // clone

                                    let dir = self.mod_directory.clone();
                                    let new_atomic = self.installed_mods_cbk.clone();
                                    let new_stop_thread = self.stop_thread.clone();
                                    self.joined_thread = Some(std::thread::spawn(move || {
                                        install_mods_in_viewport(
                                            &mut mods,
                                            &dir,
                                            &new_atomic,
                                            &new_stop_thread,
                                        );
                                    }));
                                    self.animate = true;
                                }
                            });

                        let total_mods = self.total_mods;
                        let installed = self
                            .installed_mods_cbk
                            .load(std::sync::atomic::Ordering::SeqCst);
                        let mut percentage = installed as f32 / total_mods;
                        if installed == -255 {
                            percentage = 1.0;
                        }
                        ui.add(
                            egui::ProgressBar::new(percentage)
                                .text("Installing mods...")
                                .animate(self.animate)
                                .show_percentage(),
                        );

                        if installed == -255 {
                            self.animate = false;
                            *show_callback = false;
                        }
                    });
                if ctx.input(|i| i.viewport().close_requested()) {
                    // Tell parent viewport that we should not show next frame:
                    *show_callback = false;
                }
            },
        );
        self.show_unknown_tagging_dialog(ctx);
    }

    fn show_filter_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.filter_enabled, "Enable filtering");
            
            if self.filter_enabled {
                ui.separator();
                ui.label("Show types:");
                
                // Get all unique mod types from current mods
                let all_types: std::collections::HashSet<String> = self.mods
                    .iter()
                    .map(|m| m.mod_type.clone())
                    .collect();
                
                // Create checkboxes for each mod type
                for mod_type in &all_types {
                    let mut is_selected = self.selected_filter_types.contains(mod_type);
                    if ui.checkbox(&mut is_selected, mod_type).changed() {
                        if is_selected {
                            self.selected_filter_types.insert(mod_type.clone());
                        } else {
                            self.selected_filter_types.remove(mod_type);
                        }
                    }
                }
                
                ui.separator();
                if ui.button("Select All").clicked() {
                    self.selected_filter_types = all_types.clone();
                }
                if ui.button("Clear All").clicked() {
                    self.selected_filter_types.clear();
                }
            }
        });
    }

    fn show_unknown_tagging_dialog(&mut self, ctx: &egui::Context) {
        if self.show_unknown_tagging_dialog {
            let viewport_options = egui::ViewportBuilder::default()
                .with_title("Tag Unknown Mod")
                .with_icon(ICON.clone())
                .with_inner_size([400.0, 200.0])
                .with_always_on_top();

            Context::show_viewport_immediate(
                ctx,
                egui::ViewportId::from_hash_of("tag_unknown_mod"),
                viewport_options,
                |ctx, class| {
                    setup_custom_style(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(mod_index) = self.unknown_mod_being_tagged {
                            if let Some(mod_item) = self.mods.get(mod_index) {
                                ui.label(format!("Mod: {}", mod_item.mod_name));
                                ui.label("This mod is currently tagged as 'Unknown'.");
                                ui.label("Please select an appropriate category:");
                                 
                                ui.separator();
                                 
                                // Predefined categories based on existing system
                                let categories = vec![
                                    "Character".to_string(),
                                    "UI".to_string(),
                                    "Audio".to_string(),
                                    "Movies".to_string(),
                                    "VFX".to_string(),
                                    "Map".to_string(),
                                    "Texture".to_string(),
                                    "Mesh".to_string(),
                                    "Animation".to_string(),
                                    "Other".to_string(),
                                ];
                                 
                                for category in &categories {
                                    if ui.button(category).clicked() {
                                        if let Some(mod_item) = self.mods.get_mut(mod_index) {
                                            mod_item.mod_type = category.clone();
                                        }
                                        self.show_unknown_tagging_dialog = false;
                                        self.unknown_mod_being_tagged = None;
                                    }
                                }
                                 
                                ui.separator();
                                if ui.button("Cancel").clicked() {
                                    self.show_unknown_tagging_dialog = false;
                                    self.unknown_mod_being_tagged = None;
                                    self.new_tag_input.clear();
                                }
                            }
                        }
                    });
                }
            );
        }
    }

    fn table_ui(&mut self, ui: &mut egui::Ui) {
        let available_height = ui.available_height();
        ui.separator();

        let table = TableBuilder::new(ui)
            .striped(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::LEFT))
            .column(Column::auto())
            .column(Column::auto())
            .column(Column::remainder().at_least(400.)) // mod name
            .column(Column::auto()) // Character
            .column(Column::auto()) // type
            .column(Column::remainder()) // options
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height);

        table
            .header(20., |mut header| {
                header.col(|ui| {
                    ui.label("Enabled");
                });

                header.col(|ui| {
                    ui.label("Row");
                });

                header.col(|ui| {
                    ui.label("Name");
                });

                header.col(|ui| {
                    ui.label("Category");
                });

                header.col(|ui| {
                    ui.label("Mod type");
                });
                header.col(|ui| {
                    ui.label("Options");
                });
            })
            .body(|mut body| {
                let mut visible_row_idx = 0;
                for (rowidx, mods) in self.mods.iter_mut().enumerate() {
                    // Apply filtering if enabled
                    if self.filter_enabled {
                        // If no filters are selected, show all mods
                        if !self.selected_filter_types.is_empty() {
                            if !self.selected_filter_types.contains(&mods.mod_type) {
                                continue; // Skip this mod if it doesn't match the filter
                            }
                        }
                    }
                    
                    visible_row_idx += 1;
                    body.row(20., |mut row| {
                        row.col(|ui|{
                            ui.add(Checkbox::new(&mut mods.enabled,""));
                        });

                        row.col(|ui| {
                            ui.add(Label::new(format!("{})", visible_row_idx)).halign(Align::RIGHT));
                        });
                        // name field
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if mods.editing {
                                    ui.set_width(ui.available_width() * 0.8); // padding for edit
                                    let text_edit = ui.add(
                                        egui::TextEdit::singleline(&mut mods.mod_name)
                                            .clip_text(true),
                                    );
                                    // Proactively ensure the text edit has focus while editing
                                    if !text_edit.has_focus() {
                                        text_edit.request_focus();
                                    }

                                    let mut finish_edit = false;
                                    let mut finished_now = false;
                                    if text_edit.lost_focus()
                                        || ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    {
                                        finish_edit = true;
                                    }
                                    // Render the confirm/edit button aligned to the right and
                                    // coordinate with finish_edit state.
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("✔").clicked() {
                                                finish_edit = true;
                                            }
                                        },
                                    );
                                    if finish_edit {
                                        // Explicitly release focus from the text field to avoid
                                        // immediate re-triggering of edit state on the next frame.
                                        text_edit.surrender_focus();
                                        mods.editing = false;
                                        finished_now = true;
                                    }
                                } else {
                                    ui.add(
                                        Label::new(&mods.mod_name).halign(Align::LEFT).truncate(),
                                    );
                                }
                                // align button right (only the edit button when not editing)
                                if !mods.editing {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.button("✏").clicked() {
                                                mods.editing = true;
                                            }
                                        },
                                    );
                                }
                            });
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(&mods.mod_type);
                                // Add "Tag" button for Unknown mods
                                if mods.mod_type == "Unknown" || mods.mod_type.contains("(Unknown)") {
                                    if ui.small_button("🏷 Tag").clicked() {
                                        self.show_unknown_tagging_dialog = true;
                                        self.unknown_mod_being_tagged = Some(rowidx);
                                    }
                                }
                            });
                        });
                        row.col(|ui| {
                            let label = if mods.is_dir{
                                "Directory"
                            }
                            else if mods.iostore {
                                "Iostore"
                            }
                            else {
                                "Pakfile"
                            };
                            ui.label(label);
                        });
                        row.col(|ui| {
                            ui.collapsing("Options", |ui| {
                                ui.add_enabled(
                                    !mods.is_dir,
                                    Checkbox::new(&mut mods.repak, "To repak"),
                                );
                                ui.add_enabled(
                                    mods.is_dir || mods.repak,
                                    Checkbox::new(&mut mods.fix_mesh, "Fix mesh"),
                                );
                                ui.add_enabled(
                                    mods.is_dir || mods.repak,
                                    Checkbox::new(&mut mods.fix_textures, "Fix textures (NoMipmaps)"),
                                );

                                let text_edit = TextEdit::singleline(&mut mods.mount_point);
                                ui.add(text_edit.hint_text("Enter mount point..."));

                                // Text edit for path_hash_seed with hint
                                let text_edit = TextEdit::singleline(&mut mods.path_hash_seed);
                                ui.add(text_edit.hint_text("Enter path hash seed..."));

                                ComboBox::new("comp_level", "Compression Algorithm")
                                    .selected_text(format!("{:?}", mods.compression))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut mods.compression,
                                            Compression::Zlib,
                                            "Zlib",
                                        );
                                        ui.selectable_value(
                                            &mut mods.compression,
                                            Compression::Gzip,
                                            "Gzip",
                                        );
                                        ui.selectable_value(
                                            &mut mods.compression,
                                            Compression::Oodle,
                                            "Oodle",
                                        );
                                        ui.selectable_value(
                                            &mut mods.compression,
                                            Compression::Zstd,
                                            "Zstd",
                                        );
                                        ui.selectable_value(
                                            &mut mods.compression,
                                            Compression::LZ4,
                                            "LZ4",
                                        );
                                    });

                                ui.separator();
                                ui.label("Custom Tags");
                                // Show tag chips with remove buttons
                                if !mods.custom_tags.is_empty() {
                                    ui.horizontal_wrapped(|ui| {
                                        let mut to_remove: Option<String> = None;
                                        for t in &mods.custom_tags {
                                            let resp = ui.add(egui::Button::new(format!("{} ✕", t)).small());
                                            if resp.clicked() { to_remove = Some(t.clone()); }
                                        }
                                        if let Some(rem) = to_remove { mods.custom_tags.retain(|x| x != &rem); }
                                    });
                                }
                                // Add from existing global tags (from config and pending)
                                let existing_tags = read_global_custom_tags();
                                if !existing_tags.is_empty() {
                                    ComboBox::new(format!("existing_tag_picker_{}", rowidx), "Add existing")
                                        .selected_text("Select tag")
                                        .show_ui(ui, |ui| {
                                            for t in &existing_tags {
                                                if ui.selectable_label(false, t).clicked() {
                                                    if !mods.custom_tags.contains(t) {
                                                        mods.custom_tags.push(t.clone());
                                                        mods.custom_tags.sort();
                                                        mods.custom_tags.dedup();
                                                    }
                                                }
                                            }
                                        });
                                }

                                ui.horizontal(|ui| {
                                    let resp = ui.add(TextEdit::singleline(&mut mods.custom_tag_input).hint_text("Add tag"));
                                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if (enter || ui.button("Add").clicked()) && !mods.custom_tag_input.trim().is_empty() {
                                        let tag = mods.custom_tag_input.trim().to_string();
                                        if !mods.custom_tags.contains(&tag) {
                                            mods.custom_tags.push(tag);
                                            mods.custom_tags.sort();
                                            mods.custom_tags.dedup();
                                        }
                                        mods.custom_tag_input.clear();
                                    }
                                });
                            });
                        });
                    })
                }
            });
    }
}

// Read global custom tags from the main config and pending file
fn read_global_custom_tags() -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    // Config dir
    let mut cfg = dirs::config_dir().unwrap_or_default();
    cfg.push("repak_manager");
    // repak_mod_manager.json
    let mut config_path = cfg.clone();
    config_path.push("repak_mod_manager.json");
    if let Ok(s) = fs::read_to_string(&config_path) {
        if let Ok(json) = serde_json::from_str::<JsonValue>(&s) {
            if let Some(arr) = json.get("custom_tag_catalog").and_then(|v| v.as_array()) {
                for v in arr { if let Some(t) = v.as_str() { out.insert(t.to_string()); } }
            }
            if let Some(meta) = json.get("mod_metadata").and_then(|v| v.as_array()) {
                for m in meta {
                    if let Some(tags) = m.get("custom_tags").and_then(|v| v.as_array()) {
                        for t in tags { if let Some(s) = t.as_str() { out.insert(s.to_string()); } }
                    }
                }
            }
        }
    }
    // pending_custom_tags.json
    let mut pending = cfg.clone();
    pending.push("pending_custom_tags.json");
    if let Ok(s) = fs::read_to_string(&pending) {
        if let Ok(json) = serde_json::from_str::<JsonValue>(&s) {
            if let Some(obj) = json.as_object() {
                for (_k, v) in obj.iter() {
                    if let Some(arr) = v.as_array() {
                        for t in arr { if let Some(s) = t.as_str() { out.insert(s.to_string()); } }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

pub static AES_KEY: LazyLock<AesKey> = LazyLock::new(|| {
    AesKey::from_str("0C263D8C22DCB085894899C3A3796383E9BF9DE0CBFB08C9BF2DEF2E84F29D74")
        .expect("Unable to initialise AES_KEY")
});

fn find_mods_from_archive(path: &str) -> Vec<InstallableMod> {
    let mut new_mods = Vec::<InstallableMod>::new();
    for entry in WalkDir::new(path) {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();
        if path.is_file() {
            let builder = repak::PakBuilder::new()
                .key(AES_KEY.clone().0)
                .reader(&mut BufReader::new(File::open(path).unwrap()));

            if let Ok(builder) = builder {
                let mut len = 1;
                let mut modtype = String::from("Unknown");
                let mut iostore = false;


                let pak_path = path.with_extension("pak");
                let utoc_path = path.with_extension("utoc");
                let ucas_path = path.with_extension("ucas");

                if pak_path.exists() && utoc_path.exists() && ucas_path.exists()
                {
                    // this is a mod of type s2, create a new Installable mod from its characteristics
                    let utoc_path = path.with_extension("utoc");

                    let files = read_utoc(&utoc_path, &builder, &path);
                    let files = files
                        .iter()
                        .map(|x| x.file_path.clone())
                        .collect::<Vec<_>>();
                    len = files.len();
                    modtype = get_current_pak_characteristics(files);
                    iostore = true;
                }
                // IF ONLY PAK IS FOUND WE NEED TO EXTRACT AND INSTALL THE PAK
                else if pak_path.exists()  {
                    let files = builder.files();
                    len = files.len();
                    modtype = get_current_pak_characteristics(files);
                }

                let installable_mod = InstallableMod {
                    mod_name: path.file_stem().unwrap().to_str().unwrap().to_string(),
                    mod_type: modtype.to_string(),
                    repak: true,
                    fix_mesh: false,
                    is_dir: false,
                    reader: Some(builder),
                    mod_path: path.to_path_buf(),
                    mount_point: "../../../".to_string(),
                    path_hash_seed: "00000000".to_string(),
                    total_files: len,
                    iostore,
                    is_archived: false,
                    editing: false,
                    compression: Oodle,
                    ..Default::default()
                };

                new_mods.push(installable_mod);
            }
        }
    }

    new_mods
}

fn map_to_mods_internal(paths: &[PathBuf]) -> Vec<InstallableMod> {
    let mut extensible_vec: Vec<InstallableMod> = Vec::new();
    let mut installable_mods = paths
        .iter()
        .map(|path| {
            let is_dir = path.clone().is_dir();
            let extension = path.extension().unwrap_or_default();
            let is_archive = extension == "zip" || extension == "rar";

            let mut modtype = "Unknown".to_string();
            let mut pak = None;
            let mut len = 1;
            let mut auto_fix_mesh = false;
            let mut auto_fix_textures = false;

            if !is_dir && !is_archive {
                let builder = repak::PakBuilder::new()
                    .key(AES_KEY.clone().0)
                    .reader(&mut BufReader::new(File::open(path.clone()).unwrap()));
                match builder {
                    Ok(builder) => {
                        pak = Some(builder.clone());
                        let files = builder.files();
                        modtype = get_current_pak_characteristics(files.clone());
                        len = files.len();
                        
                        // Auto-detect mesh and texture files in pak files
                        auto_fix_mesh = detect_mesh_files(&files);
                        auto_fix_textures = detect_texture_files(&files);
                    }
                    Err(e) => {
                        error!("Error reading pak file: {}", e);
                        return Err(e);
                    }
                }
            }

            if is_dir {
                let mut files = vec![];
                collect_files(&mut files, path)?;
                let files = files
                    .iter()
                    .map(|s| s.to_str().unwrap().to_string())
                    .collect::<Vec<_>>();
                len = files.len();
                modtype = get_current_pak_characteristics(files.clone());
                
                // Auto-detect mesh and texture files
                auto_fix_mesh = detect_mesh_files(&files);
                auto_fix_textures = detect_texture_files(&files);
            }

            if is_archive {
                modtype = "Season 2 Archives".to_string();
                let tempdir = tempdir()
                    .unwrap()
                    .path()
                    .as_os_str()
                    .to_str()
                    .unwrap()
                    .to_string();

                if extension == "zip" {
                    extract_zip(path.to_str().unwrap(), &tempdir).expect("Unable to install mod")
                } else if extension == "rar" {
                    extract_rar(path.to_str().unwrap(), &tempdir).expect("Unable to install mod")
                }

                // Now find pak files / s2 archives and turn them into installable mods
                let mut new_mods = find_mods_from_archive(&tempdir);
                extensible_vec.append(&mut new_mods);
            }

            Ok(InstallableMod {
                mod_name: path.file_stem().unwrap().to_str().unwrap().to_string(),
                mod_type: modtype,
                repak: !is_dir,
                fix_mesh: auto_fix_mesh,
                fix_textures: auto_fix_textures,
                is_dir,
                reader: pak,
                mod_path: path.clone(),
                mount_point: "../../../".to_string(),
                path_hash_seed: "00000000".to_string(),
                total_files: len,
                is_archived: is_archive,
                ..Default::default()
            })
        })
        .filter_map(|x: Result<InstallableMod, repak::Error>| x.ok())
        .filter(|x| !x.is_archived)
        .collect::<Vec<_>>();

    installable_mods.extend(extensible_vec);

    debug!("Install mods: {:?}", installable_mods);
    installable_mods
}

pub fn map_paths_to_mods(paths: &[PathBuf]) -> Vec<InstallableMod> {
    let installable_mods = map_to_mods_internal(paths);
    installable_mods
}

pub fn map_dropped_file_to_mods(dropped_files: &[egui::DroppedFile]) -> Vec<InstallableMod> {
    let paths = dropped_files
        .iter()
        .map(|f| f.path.clone().unwrap())
        .collect::<Vec<_>>();

    let installable_mods = map_to_mods_internal(&paths);
    installable_mods
}
