#![allow(dead_code)]
//! P2P Manager - TCP Direct P2P with Internet NAT Traversal
//! Uses encrypted TCP connections for direct peer-to-peer mod sharing
//! with AES-256-GCM encryption, SHA256 integrity verification,
//! UPnP automatic port forwarding, and public IP detection for
//! internet-wide sharing without manual configuration.

use crate::p2p_libp2p::ShareInfo;
use crate::p2p_sharing::{P2PError, P2PResult, ShareSession, TransferProgress, TransferStatus};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use log::{error, info, warn};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Window};

// ============================================================================
// UPnP & PUBLIC IP HELPERS
// ============================================================================

/// Attempt UPnP port mapping on the router.
/// Returns (external_ip, external_port) on success.
async fn try_upnp_port_mapping(
    local_ip: Ipv4Addr,
    local_port: u16,
) -> Option<(std::net::IpAddr, u16)> {
    info!(
        "[P2P/UPnP] Attempting UPnP port mapping {}:{} ...",
        local_ip, local_port
    );

    let search_opts = igd_next::SearchOptions {
        timeout: Some(Duration::from_secs(5)),
        ..Default::default()
    };

    let gateway = match igd_next::aio::tokio::search_gateway(search_opts).await {
        Ok(gw) => {
            info!("[P2P/UPnP] Found gateway: {}", gw.addr);
            gw
        }
        Err(e) => {
            warn!("[P2P/UPnP] Gateway discovery failed: {}", e);
            return None;
        }
    };

    // Get external IP from router
    let external_ip = match gateway.get_external_ip().await {
        Ok(ip) => {
            info!("[P2P/UPnP] External IP from router: {}", ip);
            ip
        }
        Err(e) => {
            warn!("[P2P/UPnP] Failed to get external IP: {}", e);
            return None;
        }
    };

    // Add port mapping (TCP, same external port, 2 hour lease)
    let internal_addr = SocketAddr::V4(SocketAddrV4::new(local_ip, local_port));
    match gateway
        .add_port(
            igd_next::PortMappingProtocol::TCP,
            local_port,
            internal_addr,
            7200, // 2 hour lease duration
            "Repak-X P2P Mod Sharing",
        )
        .await
    {
        Ok(()) => {
            info!(
                "[P2P/UPnP] Port mapping created: external {}:{} -> internal {}",
                external_ip, local_port, internal_addr
            );
            Some((external_ip, local_port))
        }
        Err(e) => {
            warn!(
                "[P2P/UPnP] Port mapping failed: {} - trying add_any_port",
                e
            );
            // Fallback: let the router pick an external port
            match gateway
                .add_any_port(
                    igd_next::PortMappingProtocol::TCP,
                    internal_addr,
                    7200,
                    "Repak-X P2P Mod Sharing",
                )
                .await
            {
                Ok(ext_port) => {
                    info!(
                        "[P2P/UPnP] Port mapping created (any port): external {}:{} -> internal {}",
                        external_ip, ext_port, internal_addr
                    );
                    Some((external_ip, ext_port))
                }
                Err(e2) => {
                    warn!("[P2P/UPnP] add_any_port also failed: {}", e2);
                    None
                }
            }
        }
    }
}

/// Remove a UPnP port mapping.
async fn remove_upnp_port_mapping(port: u16) {
    info!("[P2P/UPnP] Removing port mapping for port {}", port);
    let search_opts = igd_next::SearchOptions {
        timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    if let Ok(gateway) = igd_next::aio::tokio::search_gateway(search_opts).await {
        match gateway
            .remove_port(igd_next::PortMappingProtocol::TCP, port)
            .await
        {
            Ok(()) => info!("[P2P/UPnP] Port mapping removed for port {}", port),
            Err(e) => warn!("[P2P/UPnP] Failed to remove port mapping: {}", e),
        }
    }
}

/// Detect public IP via HTTP API (fallback when UPnP doesn't provide it).
async fn get_public_ip_http() -> Option<String> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Try multiple services for reliability
    let services = [
        "https://api.ipify.org",
        "https://api.ip.sb/ip",
        "https://ifconfig.me/ip",
    ];

    for url in &services {
        match client.get(*url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(ip) = resp.text().await {
                    let ip = ip.trim().to_string();
                    if !ip.is_empty() && ip.parse::<std::net::IpAddr>().is_ok() {
                        info!("[P2P] Public IP detected via {}: {}", url, ip);
                        return Some(ip);
                    }
                }
            }
            _ => continue,
        }
    }
    warn!("[P2P] Could not detect public IP from any HTTP service");
    None
}

// ============================================================================
// BORE RELAY TUNNEL (free TCP relay via bore.pub)
// ============================================================================

/// Create a bore tunnel to relay TCP traffic through bore.pub.
/// Returns (bore_address, bore_port, join_handle) on success.
async fn try_bore_tunnel(local_port: u16) -> Option<(String, u16, tokio::task::JoinHandle<()>)> {
    info!(
        "[P2P/Bore] Creating relay tunnel for local port {} via bore.pub ...",
        local_port
    );

    // Connect to bore.pub with a 15s timeout
    let client = match tokio::time::timeout(
        Duration::from_secs(15),
        bore_cli::client::Client::new("localhost", local_port, "bore.pub", 0, None),
    )
    .await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            warn!("[P2P/Bore] Failed to create tunnel: {}", e);
            return None;
        }
        Err(_) => {
            warn!("[P2P/Bore] Tunnel creation timed out");
            return None;
        }
    };

    let remote_port = client.remote_port();
    info!(
        "[P2P/Bore] Tunnel established! Public address: bore.pub:{}",
        remote_port
    );

    // Spawn the forwarding loop — runs until aborted or error
    let handle = tokio::spawn(async move {
        if let Err(e) = client.listen().await {
            warn!("[P2P/Bore] Tunnel closed: {}", e);
        }
    });

    // Give the listen task time to start accepting forwarded connections
    tokio::time::sleep(Duration::from_millis(500)).await;

    Some(("bore.pub".to_string(), remote_port, handle))
}

// ============================================================================
// MANAGER
// ============================================================================

pub struct UnifiedP2PManager {
    instance_id: String,
    pub active_shares: Arc<Mutex<HashMap<String, ActiveShare>>>,
    pub active_downloads: Arc<Mutex<HashMap<String, ActiveDownload>>>,
}

pub struct ActiveShare {
    pub session: ShareSession,
    pub stop_flag: Arc<AtomicBool>,
    pub upnp_external_port: Option<u16>,
    pub bore_task_handle: Option<tokio::task::JoinHandle<()>>,
}

pub struct ActiveDownload {
    pub share_info: ShareInfo,
    pub progress: TransferProgress,
    pub output_dir: PathBuf,
    pub stop_flag: Arc<AtomicBool>,
}

impl UnifiedP2PManager {
    pub async fn new() -> P2PResult<Self> {
        let id = format!("repak-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        info!("[P2P] Manager initialized: {}", id);
        Ok(Self {
            instance_id: id,
            active_shares: Arc::new(Mutex::new(HashMap::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start_sharing(
        &self,
        name: String,
        desc: String,
        paths: Vec<PathBuf>,
        creator: Option<String>,
    ) -> P2PResult<ShareInfo> {
        info!("[P2P] Starting share: {} ({} files)", name, paths.len());

        // Create mod pack metadata
        let pack = crate::p2p_sharing::create_mod_pack(name.clone(), desc, &paths, creator)?;

        // Create TCP P2P server (binds port, generates encryption key)
        let server = crate::p2p_sharing::P2PServer::new(pack, paths)?;
        let session = server.get_session();
        let stop_flag = server.get_stop_flag();
        let key_bytes = *server.encryption_key_bytes();
        let local_port = session.port;

        // -----------------------------------------------------------
        // NAT traversal: UPnP port mapping + public IP detection
        // -----------------------------------------------------------
        let local_ip_parsed: Ipv4Addr = session.local_ip.parse().unwrap_or(Ipv4Addr::LOCALHOST);
        let mut addresses: Vec<String> = Vec::new();
        let mut upnp_external_port: Option<u16> = None;
        let mut bore_task_handle: Option<tokio::task::JoinHandle<()>> = None;

        // 1) Try UPnP automatic port forwarding
        match try_upnp_port_mapping(local_ip_parsed, local_port).await {
            Some((ext_ip, ext_port)) => {
                info!(
                    "[P2P] UPnP success - public address: {}:{}",
                    ext_ip, ext_port
                );
                addresses.push(format!("{}:{}", ext_ip, ext_port));
                upnp_external_port = Some(ext_port);
            }
            None => {
                info!("[P2P] UPnP unavailable - trying bore.pub relay tunnel...");
            }
        }

        // 2) If no UPnP, create a bore.pub relay tunnel (free, works through any NAT)
        if upnp_external_port.is_none() {
            if let Some((host, port, handle)) = try_bore_tunnel(local_port).await {
                addresses.push(format!("{}:{}", host, port));
                bore_task_handle = Some(handle);
            } else {
                warn!("[P2P] Bore tunnel also failed - sharing limited to local network.");
            }
        }

        // 3) Always include local address as LAN fallback
        addresses.push(format!("{}:{}", session.local_ip, local_port));

        info!("[P2P] Share addresses: {:?}", addresses);

        // Build ShareInfo for frontend (base64 JSON format)
        let key_b64 = URL_SAFE_NO_PAD.encode(key_bytes);
        let share_info = ShareInfo {
            peer_id: self.instance_id.clone(),
            addresses,
            encryption_key: key_b64,
            share_code: session.share_code.clone(),
        };

        // Encode as base64 JSON connection string
        let conn = share_info
            .encode()
            .map_err(|e| P2PError::ValidationError(format!("{}", e)))?;

        // Create session with encoded connection string
        let sess = ShareSession {
            share_code: session.share_code.clone(),
            encryption_key: share_info.encryption_key.clone(),
            local_ip: session.local_ip.clone(),
            obfuscated_ip: session.obfuscated_ip.clone(),
            port: session.port,
            connection_string: conn,
            obfuscated_connection_string: session.obfuscated_connection_string.clone(),
            active: true,
        };

        let code = session.share_code.clone();
        let shares = self.active_shares.clone();
        let code_for_thread = code.clone();

        // Start TCP server in background thread (blocking accept loop)
        std::thread::spawn(move || {
            let mut server = server;
            info!("[P2P] Server thread started for share: {}", code_for_thread);
            if let Err(e) = server.run() {
                if !e.to_string().contains("Server already started") {
                    error!("[P2P] Server error: {}", e);
                }
            }
            info!("[P2P] Server thread ended for share: {}", code_for_thread);
            shares.lock().remove(&code_for_thread);
        });

        // Store active share
        self.active_shares.lock().insert(
            code.clone(),
            ActiveShare {
                session: sess,
                stop_flag,
                upnp_external_port,
                bore_task_handle,
            },
        );

        info!("[P2P] Share ready!");
        Ok(share_info)
    }

    pub fn stop_sharing(&self, code: &str) -> P2PResult<()> {
        info!("[P2P] Stopping share: {}", code);
        if let Some(share) = self.active_shares.lock().remove(code) {
            share.stop_flag.store(true, Ordering::SeqCst);

            // Clean up UPnP port mapping in background
            if let Some(ext_port) = share.upnp_external_port {
                tokio::spawn(async move {
                    remove_upnp_port_mapping(ext_port).await;
                });
            }

            // Abort bore relay tunnel
            if let Some(handle) = share.bore_task_handle {
                info!("[P2P/Bore] Aborting relay tunnel");
                handle.abort();
            }
        }
        Ok(())
    }

    pub async fn start_receiving(
        &self,
        conn: &str,
        out: PathBuf,
        client_name: Option<String>,
        window: Window,
    ) -> P2PResult<()> {
        info!("[P2P] Starting receive to: {}", out.display());

        // Decode connection string
        let share_info =
            ShareInfo::decode(conn).map_err(|e| P2PError::ValidationError(format!("{}", e)))?;
        if share_info.addresses.is_empty() {
            return Err(P2PError::ValidationError(
                "No addresses in share info".into(),
            ));
        }
        let code = share_info.share_code.clone();

        // Decode AES-256 encryption key from base64
        let key_bytes = URL_SAFE_NO_PAD
            .decode(&share_info.encryption_key)
            .map_err(|e| P2PError::ValidationError(format!("Invalid encryption key: {}", e)))?;
        if key_bytes.len() != 32 {
            return Err(P2PError::ValidationError(
                "Invalid encryption key length".into(),
            ));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);

        let addresses = share_info.addresses.clone();
        info!(
            "[P2P] Will try {} address(es): {:?}",
            addresses.len(),
            addresses
        );

        // Initial stop flag (replaced per-client once a working address is found)
        let initial_stop = Arc::new(AtomicBool::new(false));

        // Insert initial progress
        self.active_downloads.lock().insert(
            code.clone(),
            ActiveDownload {
                share_info: share_info.clone(),
                progress: TransferProgress {
                    current_file: "Connecting...".into(),
                    files_completed: 0,
                    total_files: 1,
                    bytes_transferred: 0,
                    total_bytes: 0,
                    status: TransferStatus::Connecting,
                },
                output_dir: out.clone(),
                stop_flag: initial_stop.clone(),
            },
        );

        let dl = self.active_downloads.clone();
        let c = code.clone();

        // Shared slot for the active progress handle so the sync task can
        // pick it up once the working client is determined.
        let active_progress: Arc<
            std::sync::Mutex<Option<Arc<std::sync::Mutex<TransferProgress>>>>,
        > = Arc::new(std::sync::Mutex::new(None));
        let active_progress_for_sync = active_progress.clone();

        // Spawn download thread - tries each address in order
        std::thread::spawn(move || {
            info!(
                "[P2P] Download thread started - trying {} address(es)",
                addresses.len()
            );
            let mut last_error = String::from("No addresses to try");

            for (i, addr) in addresses.iter().enumerate() {
                // Check cancellation
                if initial_stop.load(Ordering::SeqCst) {
                    info!("[P2P] Download cancelled by user");
                    if let Some(d) = dl.lock().get_mut(&c) {
                        d.progress.status = TransferStatus::Cancelled;
                    }
                    return;
                }

                info!(
                    "[P2P] Trying address {}/{}: {}",
                    i + 1,
                    addresses.len(),
                    addr
                );
                if let Some(d) = dl.lock().get_mut(&c) {
                    d.progress.current_file =
                        format!("Trying {} ({}/{})...", addr, i + 1, addresses.len());
                    d.progress.status = TransferStatus::Connecting;
                }

                // Relay addresses get extra retries (bore tunnel may need warmup)
                let max_attempts = if addr.contains("bore.pub") { 3 } else { 1 };

                for attempt in 0..max_attempts {
                    if attempt > 0 {
                        info!(
                            "[P2P] Retrying {} (attempt {}/{})",
                            addr,
                            attempt + 1,
                            max_attempts
                        );
                        std::thread::sleep(Duration::from_secs(2));
                        if let Some(d) = dl.lock().get_mut(&c) {
                            d.progress.current_file =
                                format!("Retrying {} ({}/{})...", addr, attempt + 1, max_attempts);
                        }
                    }

                    // Build a fresh client for this address (no probe — go directly)
                    let client = crate::p2p_sharing::P2PClient::new(key, addr.clone());

                    // Update stop flag so cancellation works on this client
                    let client_stop = client.get_stop_flag();
                    if let Some(d) = dl.lock().get_mut(&c) {
                        d.stop_flag = client_stop.clone();
                    }

                    // Publish progress handle for the sync task
                    if let Ok(mut guard) = active_progress.lock() {
                        *guard = Some(client.progress_handle());
                    }

                    match client.download_pack(&out, client_name.clone()) {
                        Ok(pack) => {
                            info!(
                                "[P2P] Download complete! {} mods received via {}",
                                pack.mods.len(),
                                addr
                            );
                            if let Some(d) = dl.lock().get_mut(&c) {
                                d.progress.status = TransferStatus::Completed;
                                d.progress.files_completed = d.progress.total_files;
                            }
                            let _ = window.emit("mods_dir_changed", ());
                            return; // success
                        }
                        Err(crate::p2p_sharing::P2PError::Cancelled) => {
                            info!("[P2P] Download cancelled by user");
                            if let Some(d) = dl.lock().get_mut(&c) {
                                d.progress.status = TransferStatus::Cancelled;
                            }
                            return;
                        }
                        Err(e) => {
                            warn!(
                                "[P2P] Transfer via {} failed (attempt {}/{}): {}",
                                addr,
                                attempt + 1,
                                max_attempts,
                                e
                            );
                            last_error = format!("{}: {}", addr, e);
                            // retry or try next address
                        }
                    }
                }
            }

            // All addresses exhausted
            error!("[P2P] All addresses failed. Last error: {}", last_error);
            if let Some(d) = dl.lock().get_mut(&c) {
                d.progress.status =
                    TransferStatus::Failed(format!("Could not connect to host: {}", last_error));
            }
        });

        // Spawn progress sync task
        let dl2 = self.active_downloads.clone();
        let c2 = code.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;

                // Try to read from the active client's progress handle
                let handle = active_progress_for_sync.lock().ok().and_then(|g| g.clone());
                if let Some(h) = handle {
                    if let Ok(prog) = h.lock().map(|p| p.clone()) {
                        let is_done = matches!(
                            prog.status,
                            TransferStatus::Completed
                                | TransferStatus::Failed(_)
                                | TransferStatus::Cancelled
                        );
                        if let Some(d) = dl2.lock().get_mut(&c2) {
                            d.progress = prog;
                        }
                        if is_done {
                            break;
                        }
                    }
                } else {
                    // No client yet - check if download already finished/failed
                    let done = dl2
                        .lock()
                        .get(&c2)
                        .map(|d| {
                            matches!(
                                d.progress.status,
                                TransferStatus::Completed
                                    | TransferStatus::Failed(_)
                                    | TransferStatus::Cancelled
                            )
                        })
                        .unwrap_or(true);
                    if done {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop all active downloads
    pub fn stop_all_downloads(&self) {
        let mut downloads = self.active_downloads.lock();
        for (code, download) in downloads.iter() {
            info!("[P2P] Stopping download: {}", code);
            download.stop_flag.store(true, Ordering::SeqCst);
        }
        downloads.clear();
    }

    pub fn get_share_session(&self, code: &str) -> Option<ShareSession> {
        self.active_shares
            .lock()
            .get(code)
            .map(|s| s.session.clone())
    }

    pub fn get_transfer_progress(&self, code: &str) -> Option<TransferProgress> {
        self.active_downloads
            .lock()
            .get(code)
            .map(|d| d.progress.clone())
    }

    pub fn is_sharing(&self, code: &str) -> bool {
        self.active_shares.lock().contains_key(code)
    }
    pub fn is_receiving(&self, code: &str) -> bool {
        self.active_downloads.lock().contains_key(code)
    }
    pub fn local_peer_id(&self) -> String {
        self.instance_id.clone()
    }
    pub fn listening_addresses(&self) -> Vec<String> {
        vec!["tcp://direct+upnp".into()]
    }
}

pub fn validate_connection_string(s: &str) -> P2PResult<bool> {
    info!("[P2P] Validating connection string: {} chars", s.len());

    fn try_decode(input: &str) -> P2PResult<bool> {
        match ShareInfo::decode(input) {
            Ok(info) => {
                info!(
                    "[P2P] Decoded ShareInfo - peer_id: {}, addresses: {:?}, share_code: {}",
                    info.peer_id, info.addresses, info.share_code
                );
                Ok(!info.addresses.is_empty())
            }
            Err(e) => {
                error!("[P2P] Failed to decode: {}", e);
                Err(P2PError::ValidationError(format!("{}", e)))
            }
        }
    }

    // Try the raw string as-is
    if let Ok(valid) = try_decode(s) {
        return Ok(valid);
    }

    // Fallback: strip any non-base64 prefix
    let trimmed = s.trim();
    let start_idx = trimmed
        .char_indices()
        .find(|&(_, c)| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        .map(|(i, _)| i);

    if let Some(i) = start_idx {
        let candidate = &trimmed[i..];
        return try_decode(candidate);
    }

    Err(P2PError::ValidationError(
        "Invalid connection string".to_string(),
    ))
}
