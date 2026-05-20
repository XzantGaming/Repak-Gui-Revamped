//! VFX Updater - Dual Logging System
//!
//! Routes logs to both RX's standard logging (-> LogDrawer) and a dedicated VFX log file.

use chrono::Local;
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Dedicated VFX log file writer
static VFX_LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

/// Initialize VFX-specific log file
pub fn init_vfx_log(app_data_dir: &PathBuf) -> std::io::Result<()> {
    let log_path = app_data_dir.join("repak_vfx.log");

    // Rotate log if it's too large (> 5MB)
    if log_path.exists() {
        if let Ok(metadata) = std::fs::metadata(&log_path) {
            if metadata.len() > 5 * 1024 * 1024 {
                let backup_path = app_data_dir.join("repak_vfx.log.old");
                let _ = std::fs::rename(&log_path, &backup_path);
            }
        }
    }

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    *VFX_LOG_FILE.lock().unwrap() = Some(file);
    info!("[VFX] Log file initialized: {:?}", log_path);

    // Write session start marker
    vfx_log("info", "=== VFX Updater Session Started ===");

    Ok(())
}

/// Close the VFX log file
pub fn close_vfx_log() {
    vfx_log("info", "=== VFX Updater Session Ended ===");

    if let Ok(mut guard) = VFX_LOG_FILE.lock() {
        if let Some(mut file) = guard.take() {
            let _ = file.flush();
        }
    }
}

/// Log message to both standard logger (-> RX LogDrawer) and VFX file
pub fn vfx_log(level: &str, message: &str) {
    // Standard log (goes to RX's logging system -> LogDrawer)
    match level {
        "error" => error!("[VFX] {}", message),
        "warn" => warn!("[VFX] {}", message),
        "debug" => debug!("[VFX] {}", message),
        _ => info!("[VFX] {}", message),
    }

    // Also write to dedicated VFX log file
    if let Ok(mut guard) = VFX_LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(
                file,
                "[{}] [{}] {}",
                timestamp,
                level.to_uppercase(),
                message
            );
            let _ = file.flush();
        }
    }
}

/// Log info level message
pub fn vfx_info(message: &str) {
    vfx_log("info", message);
}

/// Log debug level message
pub fn vfx_debug(message: &str) {
    vfx_log("debug", message);
}

/// Log warning level message
pub fn vfx_warn(message: &str) {
    vfx_log("warn", message);
}

/// Log error level message
pub fn vfx_error(message: &str) {
    vfx_log("error", message);
}
