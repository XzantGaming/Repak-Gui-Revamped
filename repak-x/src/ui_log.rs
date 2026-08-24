//! Structured UI logging -> the frontend LogDrawer.
//!
//! Before this module the drawer was fed by a single stringly-typed
//! `install_log` event emitted from the mod-detection path, so everything the
//! app does around it (character database checks, hero portrait syncs, startup
//! diagnostics) was invisible unless you opened `repakx.log`. This gives those
//! call sites a level, a scope and a timestamp the drawer can actually render,
//! filter and export.
//!
//! Two details make this reliable:
//!
//! * **Backlog.** `setup()` kicks off background checks immediately, long before
//!   the webview has attached its listener, so those events would otherwise be
//!   emitted into the void. Every entry is also kept in a capped ring buffer
//!   that the frontend drains once on mount.
//! * **Sequence numbers.** The frontend subscribes *then* drains, so an entry
//!   can arrive twice. `seq` is monotonic per process, which lets the frontend
//!   dedupe instead of racing.

use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Event name the frontend listens on.
pub const UI_LOG_EVENT: &str = "ui_log";

/// How many entries the replay buffer keeps. Startup emits a few dozen; the cap
/// only matters if the frontend never mounts, in which case we must not grow
/// without bound.
const BACKLOG_CAPACITY: usize = 500;

static SEQ: AtomicU64 = AtomicU64::new(0);
static BACKLOG: Lazy<Mutex<VecDeque<UiLogPayload>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(BACKLOG_CAPACITY)));
static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

/// Severity, mapped 1:1 to the drawer's line styling.
///
/// `Success` has no `log` crate equivalent; it exists because "the thing you
/// were waiting on finished cleanly" deserves to be visually distinct from the
/// running commentary around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLogPayload {
    /// Monotonic per process. The frontend dedupes on this.
    pub seq: u64,
    /// Unix milliseconds, stamped at emit so ordering survives the event hop.
    pub ts: u64,
    pub level: LogLevel,
    /// Short subsystem tag, e.g. `CharDB`, `Hero`, `Install`.
    pub scope: String,
    pub message: String,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Store the handle used by the global helpers. Called once from `setup()`.
pub fn init(app: AppHandle) {
    if let Ok(mut guard) = APP.lock() {
        *guard = Some(app);
    }
}

/// Emit one entry to the drawer, the replay buffer and `repakx.log`.
pub fn emit(level: LogLevel, scope: &str, message: impl Into<String>) {
    let message = message.into();

    // Mirror into the file log so the drawer and repakx.log tell the same story.
    match level {
        LogLevel::Error => log::error!("[{}] {}", scope, message),
        LogLevel::Warn => log::warn!("[{}] {}", scope, message),
        LogLevel::Debug => log::debug!("[{}] {}", scope, message),
        _ => log::info!("[{}] {}", scope, message),
    }

    let payload = UiLogPayload {
        seq: SEQ.fetch_add(1, Ordering::Relaxed),
        ts: now_millis(),
        level,
        scope: scope.to_string(),
        message,
    };

    if let Ok(mut backlog) = BACKLOG.lock() {
        if backlog.len() == BACKLOG_CAPACITY {
            backlog.pop_front();
        }
        backlog.push_back(payload.clone());
    }

    if let Ok(guard) = APP.lock() {
        if let Some(app) = guard.as_ref() {
            let _ = app.emit(UI_LOG_EVENT, &payload);
        }
    }
}

pub fn debug(scope: &str, message: impl Into<String>) {
    emit(LogLevel::Debug, scope, message);
}

pub fn info(scope: &str, message: impl Into<String>) {
    emit(LogLevel::Info, scope, message);
}

pub fn success(scope: &str, message: impl Into<String>) {
    emit(LogLevel::Success, scope, message);
}

pub fn warn(scope: &str, message: impl Into<String>) {
    emit(LogLevel::Warn, scope, message);
}

pub fn error(scope: &str, message: impl Into<String>) {
    emit(LogLevel::Error, scope, message);
}

/// Everything emitted so far, for a frontend that has just mounted.
///
/// Non-draining on purpose: a reload should still see the startup banner, and
/// the frontend dedupes by `seq` anyway.
#[tauri::command]
pub fn get_ui_log_backlog() -> Vec<UiLogPayload> {
    BACKLOG
        .lock()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default()
}

// ============================================================================
// LOG EXPORT
// ============================================================================

/// Where log files live: next to the executable, matching `setup_logging()`.
fn log_dir() -> std::path::PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            return exe_dir.join("Logs");
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Repak-X")
}

/// Full path of the rolling application log, so the UI can offer to open it.
#[tauri::command]
pub fn get_app_log_path() -> String {
    log_dir().join("repakx.log").to_string_lossy().to_string()
}

/// Write the drawer's rendered lines to a timestamped file and return its path.
///
/// The drawer already formats each line the way the user sees it, so exporting
/// the frontend's strings keeps the file identical to what they were looking at
/// (including any active filter) rather than re-deriving it here.
#[tauri::command]
pub fn export_ui_logs(lines: Vec<String>) -> Result<String, String> {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {:?}: {}", dir, e))?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("repak-x-drawer-{}.log", stamp));

    let mut body = String::new();
    body.push_str(&format!(
        "Repak-X drawer export - {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    body.push_str(&format!("{} line(s)\n", lines.len()));
    body.push_str("========================================\n");
    for line in &lines {
        body.push_str(line);
        body.push('\n');
    }

    std::fs::write(&path, body).map_err(|e| format!("Could not write {:?}: {}", path, e))?;
    info("Logs", format!("Exported {} line(s) to {:?}", lines.len(), path));

    Ok(path.to_string_lossy().to_string())
}
