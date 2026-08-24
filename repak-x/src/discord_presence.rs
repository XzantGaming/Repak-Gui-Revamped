use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use log::info;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const DISCORD_APP_ID: &str = "1465997185341718559";

/// Maps color theme names to Discord asset keys
/// These asset keys must be uploaded to the Discord Developer Portal
/// under Rich Presence -> Art Assets with matching names
///
/// Available color presets in the app:
/// - Red (crimson)
/// - Blue (bright blue)
/// - Cyan/Teal (turquoise)
/// - Green (bright green)
/// - Orange
/// - Pink (hot pink/magenta)
fn get_logo_asset_for_theme(theme: &str) -> &'static str {
    match theme.to_lowercase().as_str() {
        "red" | "crimson" => "repakx_logo_red",
        "blue" | "default" => "repakx_logo_blue",
        "green" => "repakx_logo_green",
        "orange" => "repakx_logo_orange",
        "purple" | "violet" => "repakx_logo_purple",
        "pink" | "magenta" | "hotpink" => "repakx_logo_pink",
        _ => "repakx_logo", // Fallback to default
    }
}

pub struct DiscordPresenceManager {
    client: Mutex<Option<DiscordIpcClient>>,
    enabled: Mutex<bool>,
    /// Set while a `connect()` call is doing the actual (blocking, unbounded)
    /// IPC handshake, so concurrent callers don't pile up behind it - see the
    /// note on `connect()` below.
    connecting: AtomicBool,
    start_timestamp: i64,
    current_theme: Mutex<String>,
    current_state: Mutex<Option<String>>,
    current_details: Mutex<Option<String>>,
}

impl DiscordPresenceManager {
    pub fn new() -> Self {
        let start_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Self {
            client: Mutex::new(None),
            enabled: Mutex::new(false),
            connecting: AtomicBool::new(false),
            start_timestamp,
            current_theme: Mutex::new("default".to_string()),
            current_state: Mutex::new(None),
            current_details: Mutex::new(None),
        }
    }

    /// Set the color theme for the Discord logo
    /// This will be applied immediately if connected
    pub fn set_theme(&self, theme: &str) {
        *self.current_theme.lock() = theme.to_string();
        info!("Discord theme set to: {}", theme);

        // Safely extract the cloned state and details without holding locks
        // to avoid deadlock inside set_activity
        let state_opt = { self.current_state.lock().clone() };
        let details_opt = { self.current_details.lock().clone() };

        if let Some(state) = state_opt {
            let _ = self.set_activity(&state, details_opt.as_deref());
        }
    }

    /// Get the current theme
    pub fn get_theme(&self) -> String {
        self.current_theme.lock().clone()
    }

    /// Connect to the Discord desktop client over its local IPC pipe.
    ///
    /// The underlying handshake (`DiscordIpcClient::connect`) does a blocking
    /// pipe read with no timeout, and Discord itself can delay or throttle
    /// that response (e.g. after rapid reconnects) for tens of seconds. The
    /// actual I/O therefore happens *outside* `client`'s lock, so a slow
    /// handshake only stalls the caller of `connect()` - it can no longer
    /// block every other Discord command that just wants to check/update
    /// `client` or `enabled` while a connect is in flight. `connecting` stops
    /// two callers from racing to open a second IPC pipe at once.
    pub fn connect(&self) -> Result<(), String> {
        *self.enabled.lock() = true;

        if self.client.lock().is_some() {
            return Ok(()); // Already connected
        }

        if self.connecting.swap(true, Ordering::SeqCst) {
            return Err("Discord connection already in progress".to_string());
        }

        info!("Connecting to Discord...");
        let mut client = DiscordIpcClient::new(DISCORD_APP_ID);
        let result = client.connect();
        self.connecting.store(false, Ordering::SeqCst);

        match result {
            Ok(()) => {
                info!("Connected to Discord Rich Presence");
                *self.client.lock() = Some(client);
                Ok(())
            }
            Err(e) => Err(format!("Failed to connect to Discord: {}", e)),
        }
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let mut client_guard = self.client.lock();

        if let Some(mut client) = client_guard.take() {
            info!("Disconnecting from Discord...");
            let _ = client.clear_activity();
            let _ = client.close();
        }

        *self.enabled.lock() = false;
        *self.current_state.lock() = None;
        *self.current_details.lock() = None;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        *self.enabled.lock()
    }

    pub fn set_activity(&self, state: &str, details: Option<&str>) -> Result<(), String> {
        // Self-heal: auto-reconnect if enabled but connection was dropped/failed earlier.
        // Routed through `connect()` so the (potentially slow) handshake never runs
        // while `client` is locked - see the note on `connect()`.
        if self.client.lock().is_none() && *self.enabled.lock() {
            info!("Discord RPC client connection was lost; attempting self-healing reconnect...");
            if let Err(e) = self.connect() {
                return Err(format!(
                    "Failed to reconnect to Discord during self-healing: {}",
                    e
                ));
            }
            info!("Self-healing reconnect succeeded");
        }

        let mut client_guard = self.client.lock();
        let client = client_guard.as_mut().ok_or("Discord not connected")?;

        // Get the logo asset based on current theme
        let theme = self.current_theme.lock().clone();
        let logo_asset = get_logo_asset_for_theme(&theme);

        let mut activity_builder = activity::Activity::new()
            .state(state)
            .timestamps(activity::Timestamps::new().start(self.start_timestamp))
            .assets(
                activity::Assets::new()
                    .large_image(logo_asset)
                    .large_text("Repak X - Marvel Rivals Mod Manager"),
            );

        if let Some(details_text) = details {
            activity_builder = activity_builder.details(details_text);
        }

        if let Err(e) = client.set_activity(activity_builder) {
            // Clear the client connection so we can self-heal on the next update
            *client_guard = None;
            return Err(format!(
                "Failed to set Discord activity (cleared client for self-healing): {}",
                e
            ));
        }

        // Save current state and details for theme updates
        *self.current_state.lock() = Some(state.to_string());
        *self.current_details.lock() = details.map(|s| s.to_string());

        Ok(())
    }

    pub fn set_idle(&self) -> Result<(), String> {
        self.set_activity("Idle", Some("Managing mods"))
    }

    pub fn set_managing_mods(&self, mod_count: usize) -> Result<(), String> {
        let state = format!(
            "Managing {} mod{}",
            mod_count,
            if mod_count == 1 { "" } else { "s" }
        );
        self.set_activity(&state, Some("Repak X"))
    }

    pub fn set_installing_mod(&self, mod_name: &str) -> Result<(), String> {
        self.set_activity("Installing mod", Some(mod_name))
    }

    pub fn set_sharing_mods(&self) -> Result<(), String> {
        self.set_activity("Sharing mods via P2P", Some("Repak X"))
    }

    pub fn set_receiving_mods(&self) -> Result<(), String> {
        self.set_activity("Receiving mods via P2P", Some("Repak X"))
    }

    pub fn clear_activity(&self) -> Result<(), String> {
        let mut client_guard = self.client.lock();

        if let Some(client) = client_guard.as_mut() {
            let _ = client.clear_activity();
        }

        Ok(())
    }
}

impl Default for DiscordPresenceManager {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedDiscordPresence = Arc<DiscordPresenceManager>;

pub fn create_discord_manager() -> SharedDiscordPresence {
    Arc::new(DiscordPresenceManager::new())
}
