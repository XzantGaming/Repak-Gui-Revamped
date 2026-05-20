//! VFX Updater - Progress Reporting Utilities

use super::logging::vfx_debug;
use super::models::VfxPipelineProgress;
use tauri::Emitter;

/// Trait for progress reporting in VFX pipeline operations
pub trait VfxProgressSink: Send + Sync {
    fn emit(&self, progress: VfxPipelineProgress);
}

/// Progress sink that emits to Tauri window via events
pub struct TauriVfxProgressSink<'a> {
    window: &'a tauri::Window,
}

impl<'a> TauriVfxProgressSink<'a> {
    pub fn new(window: &'a tauri::Window) -> Self {
        Self { window }
    }
}

impl<'a> VfxProgressSink for TauriVfxProgressSink<'a> {
    fn emit(&self, progress: VfxPipelineProgress) {
        vfx_debug(&format!(
            "Progress: step={}, stage='{}', {}/{}, message='{}'",
            progress.step, progress.stage, progress.current, progress.total, progress.message
        ));

        let _ = self.window.emit("vfx_progress", &progress);

        // Also emit as a log message for LogDrawer integration
        let log_msg = format!(
            "[Step {}] {}: {}",
            progress.step, progress.stage, progress.message
        );
        let _ = self.window.emit("vfx_log", &log_msg);
    }
}

/// No-op progress sink for testing or when no UI is available
pub struct NoOpProgressSink;

impl VfxProgressSink for NoOpProgressSink {
    fn emit(&self, progress: VfxPipelineProgress) {
        vfx_debug(&format!(
            "Progress (no-op): step={}, stage='{}', {}/{}, message='{}'",
            progress.step, progress.stage, progress.current, progress.total, progress.message
        ));
    }
}
