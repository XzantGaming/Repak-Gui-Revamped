// Toast Events - Emits toast notifications to the frontend via Tauri events
// The frontend AlertHandler.jsx listens for these events and displays toast notifications

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Window};

/// Toast notification types matching the frontend AlertHandler colors
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastType {
    Success,
    Danger,
    Warning,
    Primary,
    Secondary,
    Default,
}

/// Toast notification payload sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastPayload {
    /// Type/color of the toast (success, danger, warning, primary, secondary, default)
    #[serde(rename = "type")]
    pub toast_type: ToastType,
    /// Title of the toast notification
    pub title: String,
    /// Description/body of the toast notification
    pub description: String,
    /// Optional duration in milliseconds (0 = no auto-dismiss)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

impl ToastPayload {
    /// Create a new error/danger toast
    pub fn error(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            toast_type: ToastType::Danger,
            title: title.into(),
            description: description.into(),
            duration: None,
        }
    }

    /// Create a new warning toast
    pub fn warning(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            toast_type: ToastType::Warning,
            title: title.into(),
            description: description.into(),
            duration: None,
        }
    }

    /// Create a new success toast
    pub fn success(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            toast_type: ToastType::Success,
            title: title.into(),
            description: description.into(),
            duration: None,
        }
    }

    /// Create a new info/primary toast
    pub fn info(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            toast_type: ToastType::Primary,
            title: title.into(),
            description: description.into(),
            duration: None,
        }
    }

    /// Set custom duration for auto-dismiss
    pub fn with_duration(mut self, duration_ms: u32) -> Self {
        self.duration = Some(duration_ms);
        self
    }

    /// Set duration to 0 (no auto-dismiss - user must manually close)
    pub fn persistent(mut self) -> Self {
        self.duration = Some(0);
        self
    }
}

/// Event name for toast notifications
pub const TOAST_EVENT: &str = "toast_notification";

/// Emit a toast notification to the frontend
pub fn emit_toast(window: &Window, payload: ToastPayload) {
    if let Err(e) = window.emit(TOAST_EVENT, &payload) {
        log::error!("Failed to emit toast notification: {}", e);
    }
}

/// Emit a success toast notification
pub fn emit_success(window: &Window, title: impl Into<String>, description: impl Into<String>) {
    emit_toast(window, ToastPayload::success(title, description));
}

// ============================================================================
// SPECIALIZED ERROR NOTIFICATIONS
// ============================================================================

/// Emit installation failed error
pub fn emit_installation_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Installation Failed",
            format!("Could not install mod: {}", error),
        )
        .persistent(),
    );
}

/// Emit mod toggle failed error
pub fn emit_toggle_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Toggle Failed",
            format!("Could not enable/disable mod: {}", error),
        ),
    );
}

/// Emit mod delete failed error
pub fn emit_delete_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error("Delete Failed", format!("Could not remove mod: {}", error)),
    );
}

/// Emit mod rename failed error
pub fn emit_rename_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error("Rename Failed", format!("Could not rename mod: {}", error)),
    );
}

/// Emit folder creation failed error
pub fn emit_folder_create_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Folder Error",
            format!("Could not create folder: {}", error),
        ),
    );
}

/// Emit folder delete failed error
pub fn emit_folder_delete_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Delete Failed",
            format!("Could not remove folder: {}", error),
        ),
    );
}

/// Emit folder rename failed error
pub fn emit_folder_rename_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Rename Failed",
            format!("Could not rename folder: {}", error),
        ),
    );
}

/// Emit mod move failed error
pub fn emit_move_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error("Move Failed", format!("Could not move mod: {}", error)),
    );
}

/// Emit game path detection failed error
pub fn emit_game_path_failed(window: &Window, error: &str) {
    emit_toast(
        window,
        ToastPayload::error(
            "Detection Failed",
            format!("Could not auto-detect game path: {}", error),
        ),
    );
}

// ============================================================================
// CRASH DETECTION NOTIFICATIONS
// ============================================================================

/// Crash notification payload with detailed error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashToastPayload {
    /// Base toast info
    #[serde(flatten)]
    pub base: ToastPayload,
    /// The raw error message from the crash dump
    pub error_message: Option<String>,
    /// Parsed crash type (e.g., "ObjectSerializationError", "Access Violation")
    pub crash_type: Option<String>,
    /// Affected asset path if detected
    pub asset_path: Option<String>,
    /// Additional details (e.g., "Serial size mismatch: ...")
    pub details: Option<String>,
    /// Character ID if detected from the crash
    pub character_id: Option<String>,
    /// Whether this is a mesh-related crash
    pub is_mesh_crash: bool,
    /// Time in game before crash (seconds)
    pub seconds_in_game: Option<u64>,
    /// Path to the crash folder for user reference
    pub crash_folder: Option<String>,
}

/// Event name for crash notifications (separate from regular toasts for special handling)
pub const CRASH_EVENT: &str = "game_crash_detected";

/// Emit a game crash notification with detailed error information
pub fn emit_crash_detected(
    window: &Window,
    error_message: Option<String>,
    crash_type: Option<String>,
    asset_path: Option<String>,
    details: Option<String>,
    character_id: Option<String>,
    is_mesh_crash: bool,
    seconds_in_game: Option<u64>,
    crash_folder: Option<String>,
) {
    // Build a user-friendly description
    let mut description = String::new();

    if let Some(ref err_type) = crash_type {
        description.push_str(&format!("Error: {}", err_type));
    }

    if let Some(ref asset) = asset_path {
        if !description.is_empty() {
            description.push_str("\n");
        }
        description.push_str(&format!("Asset: {}", asset));
    }

    if let Some(ref detail) = details {
        if !description.is_empty() {
            description.push_str("\n");
        }
        description.push_str(detail);
    }

    if description.is_empty() {
        description = error_message
            .clone()
            .unwrap_or_else(|| "Unknown crash error".to_string());
    }

    let payload = CrashToastPayload {
        base: ToastPayload {
            toast_type: ToastType::Danger,
            title: "Game Crashed".to_string(),
            description,
            duration: Some(0), // Persistent - user must dismiss
        },
        error_message,
        crash_type,
        asset_path,
        details,
        character_id,
        is_mesh_crash,
        seconds_in_game,
        crash_folder,
    };

    if let Err(e) = window.emit(CRASH_EVENT, &payload) {
        log::error!("Failed to emit crash notification: {}", e);
    }
}

/// Emit a crash notification from CrashInfo struct
pub fn emit_crash_from_info(window: &Window, crash_info: &crate::crash_monitor::CrashInfo) {
    let error_msg = crash_info.error_message.clone().unwrap_or_default();

    // Parse error details
    let (asset_path, error_type, details) = crate::crash_monitor::parse_error_details(&error_msg);
    let character_id = crate::crash_monitor::extract_character_id(&error_msg);
    let is_mesh_crash = crate::crash_monitor::is_mesh_related_crash(&error_msg);

    emit_crash_detected(
        window,
        crash_info.error_message.clone(),
        error_type.or(crash_info.crash_type.clone()),
        asset_path,
        details,
        character_id,
        is_mesh_crash,
        crash_info.seconds_since_start,
        Some(crash_info.crash_folder.to_string_lossy().to_string()),
    );
}
