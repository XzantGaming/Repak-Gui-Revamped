#![allow(dead_code)]
//! Secure P2P Mod Sharing Module
//!
//! Provides secure peer-to-peer mod pack sharing functionality with:
//! - AES-256-GCM encryption for all transfers
//! - SHA256 integrity verification
//! - Random share code generation for peer discovery
//! - TCP-based file transfer with progress tracking

// use crate::ip_obfuscation; // Disabled - P2P stub mode
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bincode;
use log::{error, info, warn};
use rand::{rngs::OsRng as RandOsRng, Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default port range for P2P sharing
const P2P_PORT_START: u16 = 47820;
const P2P_PORT_END: u16 = 47830;

/// Maximum file size for single transfer (2GB)
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

/// Chunk size for file transfer (1MB)
const CHUNK_SIZE: usize = 1024 * 1024;

/// Protocol version for compatibility checking
const PROTOCOL_VERSION: u32 = 1;

/// Magic bytes to identify our protocol
const MAGIC_BYTES: &[u8; 4] = b"RPMK";

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Represents a mod pack that can be shared
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareableModPack {
    /// Display name for the mod pack
    pub name: String,
    /// Description of the mod pack
    pub description: String,
    /// List of mod files to share
    pub mods: Vec<ShareableMod>,
    /// Timestamp when created
    pub created_at: u64,
    /// Creator identifier (optional)
    pub creator: Option<String>,
}

/// Represents a single mod file in a pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareableMod {
    /// Original filename
    pub filename: String,
    /// File size in bytes
    pub size: u64,
    /// SHA256 hash of the file
    pub hash: String,
    /// Associated IoStore files (if any)
    pub iostore_files: Vec<IoStoreFile>,
}

/// IoStore companion file info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoStoreFile {
    pub extension: String, // "ucas" or "utoc"
    pub size: u64,
    pub hash: String,
}

/// Share session info (returned when hosting)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSession {
    /// Unique share code (user-friendly format)
    pub share_code: String,
    /// The encryption key (base64 encoded, part of full code)
    pub encryption_key: String,
    /// Local IP address (internal use only - not serialized to frontend)
    #[serde(skip_serializing)]
    pub local_ip: String,
    /// Obfuscated IP for display purposes
    pub obfuscated_ip: String,
    /// Port listening on
    pub port: u16,
    /// Full connection string for sharing
    pub connection_string: String,
    /// Obfuscated connection string for display purposes
    pub obfuscated_connection_string: String,
    /// Is the session active
    pub active: bool,
}

/// Message types for P2P protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
enum P2PMessage {
    /// Initial handshake
    Hello {
        protocol_version: u32,
        client_name: Option<String>,
    },
    /// Handshake response
    Welcome {
        protocol_version: u32,
        pack_info: ShareableModPack,
    },
    /// Request to download a file
    RequestFile { filename: String },
    /// File data chunk
    FileChunk {
        filename: String,
        offset: u64,
        data: Vec<u8>,
        is_last: bool,
    },
    /// Transfer complete
    TransferComplete { filename: String, hash: String },
    /// Error message
    Error { message: String },
    /// Acknowledge receipt
    Ack,
    /// Session ended
    Goodbye,
}

/// Progress info for transfers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub current_file: String,
    pub files_completed: usize,
    pub total_files: usize,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransferStatus {
    Connecting,
    Handshaking,
    Transferring,
    Verifying,
    Completed,
    Failed(String),
    Cancelled,
}

/// Result type for P2P operations
pub type P2PResult<T> = Result<T, P2PError>;

/// P2P error types
#[derive(Debug, Clone)]
pub enum P2PError {
    NetworkError(String),
    EncryptionError(String),
    ProtocolError(String),
    FileError(String),
    ValidationError(String),
    ConnectionError(String),
    Cancelled,
}

impl std::fmt::Display for P2PError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            P2PError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            P2PError::EncryptionError(msg) => write!(f, "Encryption error: {}", msg),
            P2PError::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            P2PError::FileError(msg) => write!(f, "File error: {}", msg),
            P2PError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            P2PError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            P2PError::Cancelled => write!(f, "Operation cancelled"),
        }
    }
}

impl std::error::Error for P2PError {}

// ============================================================================
// ENCRYPTION UTILITIES
// ============================================================================

/// Generates a new random 256-bit encryption key
pub fn generate_encryption_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    RandOsRng.fill_bytes(&mut key);
    key
}

/// Encrypts data using AES-256-GCM
pub fn encrypt_data(key: &[u8; 32], plaintext: &[u8]) -> P2PResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| P2PError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

    // Generate random nonce
    let mut nonce_bytes = [0u8; 12];
    RandOsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| P2PError::EncryptionError(format!("Encryption failed: {}", e)))?;

    // Prepend nonce to ciphertext
    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

/// Decrypts data using AES-256-GCM
pub fn decrypt_data(key: &[u8; 32], ciphertext: &[u8]) -> P2PResult<Vec<u8>> {
    if ciphertext.len() < 12 {
        return Err(P2PError::EncryptionError(
            "Ciphertext too short".to_string(),
        ));
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| P2PError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

    let nonce = Nonce::from_slice(&ciphertext[..12]);
    let plaintext = cipher
        .decrypt(nonce, &ciphertext[12..])
        .map_err(|e| P2PError::EncryptionError(format!("Decryption failed: {}", e)))?;

    Ok(plaintext)
}

// ============================================================================
// SHARE CODE UTILITIES
// ============================================================================

/// Generates a user-friendly share code
/// Format: XXXX-XXXX-XXXX where X is alphanumeric (no ambiguous chars)
pub fn generate_share_code() -> String {
    const CHARSET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"; // No 0,1,I,O
    let mut rng = RandOsRng;
    let mut code = String::with_capacity(14);

    for i in 0..12 {
        if i > 0 && i % 4 == 0 {
            code.push('-');
        }
        let idx = rng.gen_range(0..CHARSET.len());
        code.push(CHARSET[idx] as char);
    }

    code
}

/// Creates a full connection string from components
/// Format: share_code:key_base64:ip:port
pub fn create_connection_string(share_code: &str, key: &[u8; 32], ip: &str, port: u16) -> String {
    let key_b64 = URL_SAFE_NO_PAD.encode(key);
    format!("{}:{}:{}:{}", share_code, key_b64, ip, port)
}

/// Creates an obfuscated connection string for display
/// Format: share_code:key_base64:obfuscated_ip:port
pub fn create_obfuscated_connection_string(
    share_code: &str,
    key: &[u8; 32],
    ip: &str,
    port: u16,
) -> String {
    let key_b64 = URL_SAFE_NO_PAD.encode(key);
    let obfuscated_ip = format!("[{}]", &ip[..ip.len().min(4)]); // Simple obfuscation
    format!("{}:{}:{}:{}", share_code, key_b64, obfuscated_ip, port)
}

/// Parses a connection string into components
pub fn parse_connection_string(conn_str: &str) -> P2PResult<(String, [u8; 32], String, u16)> {
    let parts: Vec<&str> = conn_str.split(':').collect();
    if parts.len() != 4 {
        return Err(P2PError::ValidationError(
            "Invalid connection string format".to_string(),
        ));
    }

    let share_code = parts[0].to_string();

    let key_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| P2PError::ValidationError(format!("Invalid key encoding: {}", e)))?;

    if key_bytes.len() != 32 {
        return Err(P2PError::ValidationError("Invalid key length".to_string()));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);

    let ip = parts[2].to_string();
    let port: u16 = parts[3]
        .parse()
        .map_err(|e| P2PError::ValidationError(format!("Invalid port: {}", e)))?;

    Ok((share_code, key, ip, port))
}

// ============================================================================
// FILE UTILITIES
// ============================================================================

/// Calculate SHA256 hash of a file
pub fn hash_file(path: &Path) -> P2PResult<String> {
    let file =
        File::open(path).map_err(|e| P2PError::FileError(format!("Failed to open file: {}", e)))?;

    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| P2PError::FileError(format!("Failed to read file: {}", e)))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Creates a ShareableModPack from a list of mod paths
pub fn create_mod_pack(
    name: String,
    description: String,
    mod_paths: &[PathBuf],
    creator: Option<String>,
) -> P2PResult<ShareableModPack> {
    let mut mods = Vec::new();

    for path in mod_paths {
        if !path.exists() {
            return Err(P2PError::FileError(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let metadata = fs::metadata(path)
            .map_err(|e| P2PError::FileError(format!("Failed to get metadata: {}", e)))?;

        if metadata.len() > MAX_FILE_SIZE {
            return Err(P2PError::FileError(format!(
                "File too large: {} ({} bytes)",
                path.display(),
                metadata.len()
            )));
        }

        let hash = hash_file(path)?;
        let raw_filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| P2PError::FileError("Invalid filename".to_string()))?
            .to_string();
        // Normalize disabled mod extensions so receiver always gets .pak
        let filename = if raw_filename.ends_with(".bak_repak") {
            format!("{}.pak", raw_filename.trim_end_matches(".bak_repak"))
        } else {
            raw_filename
        };

        // Check for IoStore companion files
        let mut iostore_files = Vec::new();
        let base_path = path.with_extension("");

        for ext in &["ucas", "utoc"] {
            let companion = base_path.with_extension(ext);
            if companion.exists() {
                let comp_meta = fs::metadata(&companion).map_err(|e| {
                    P2PError::FileError(format!("Failed to get companion metadata: {}", e))
                })?;
                let comp_hash = hash_file(&companion)?;
                iostore_files.push(IoStoreFile {
                    extension: ext.to_string(),
                    size: comp_meta.len(),
                    hash: comp_hash,
                });
            }
        }

        mods.push(ShareableMod {
            filename,
            size: metadata.len(),
            hash,
            iostore_files,
        });
    }

    Ok(ShareableModPack {
        name,
        description,
        mods,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        creator,
    })
}

// ============================================================================
// P2P SERVER (HOST) IMPLEMENTATION
// ============================================================================

/// Pack preview for frontend display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackPreview {
    pub total_size: u64,
    pub file_count: usize,
}

/// Creates a lightweight preview of a mod pack (total size and file count)
pub fn create_mod_pack_preview(
    name: String,
    description: String,
    mod_paths: &[PathBuf],
    creator: Option<String>,
) -> P2PResult<PackPreview> {
    let pack = create_mod_pack(name, description, mod_paths, creator)?;
    let mut total_size = 0u64;
    let mut file_count = 0usize;
    for m in &pack.mods {
        total_size += m.size;
        file_count += 1;
        for io in &m.iostore_files {
            total_size += io.size;
            file_count += 1;
        }
    }
    Ok(PackPreview {
        total_size,
        file_count,
    })
}

/// Active share session manager
pub struct P2PServer {
    listener: Option<TcpListener>,
    session: ShareSession,
    mod_pack: ShareableModPack,
    mod_paths: HashMap<String, PathBuf>,
    encryption_key: [u8; 32],
    stop_flag: Arc<AtomicBool>,
    active_connections: Arc<Mutex<usize>>,
}

impl P2PServer {
    /// Create a new P2P server for sharing mods
    pub fn new(mod_pack: ShareableModPack, mod_paths: Vec<PathBuf>) -> P2PResult<Self> {
        // Generate encryption key
        let encryption_key = generate_encryption_key();

        // Find an available port
        let mut listener = None;
        let mut port = 0;

        for p in P2P_PORT_START..=P2P_PORT_END {
            match TcpListener::bind(("0.0.0.0", p)) {
                Ok(l) => {
                    listener = Some(l);
                    port = p;
                    break;
                }
                Err(_) => continue,
            }
        }

        let listener = listener
            .ok_or_else(|| P2PError::NetworkError("No available ports in range".to_string()))?;

        // Get local IP
        let local_ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        // Generate share code
        let share_code = generate_share_code();
        let key_b64 = URL_SAFE_NO_PAD.encode(&encryption_key);
        let connection_string =
            create_connection_string(&share_code, &encryption_key, &local_ip, port);
        let obfuscated_connection_string =
            create_obfuscated_connection_string(&share_code, &encryption_key, &local_ip, port);

        // Build path map (keys are normalized .pak filenames to match share metadata)
        let mut path_map = HashMap::new();
        for path in mod_paths {
            if let Some(raw_name) = path.file_name().and_then(|n| n.to_str()) {
                // Normalize .bak_repak → .pak so requests from receiver match
                let key = if raw_name.ends_with(".bak_repak") {
                    format!("{}.pak", raw_name.trim_end_matches(".bak_repak"))
                } else {
                    raw_name.to_string()
                };
                path_map.insert(key, path.clone());

                // Also add IoStore files
                let base_path = path.with_extension("");
                for ext in &["ucas", "utoc"] {
                    let companion = base_path.with_extension(ext);
                    if companion.exists() {
                        if let Some(comp_name) = companion.file_name().and_then(|n| n.to_str()) {
                            path_map.insert(comp_name.to_string(), companion);
                        }
                    }
                }
            }
        }

        let session = ShareSession {
            share_code,
            encryption_key: key_b64,
            obfuscated_ip: format!("[{}...]", &local_ip[..local_ip.len().min(4)]),
            local_ip,
            port,
            connection_string,
            obfuscated_connection_string,
            active: true,
        };

        info!(
            "P2P Server created on port {} with code {}",
            port, session.share_code
        );

        Ok(Self {
            listener: Some(listener),
            session,
            mod_pack,
            mod_paths: path_map,
            encryption_key,
            stop_flag: Arc::new(AtomicBool::new(false)),
            active_connections: Arc::new(Mutex::new(0)),
        })
    }

    /// Get the current session info
    pub fn get_session(&self) -> ShareSession {
        self.session.clone()
    }

    /// Get the raw encryption key bytes
    pub fn encryption_key_bytes(&self) -> &[u8; 32] {
        &self.encryption_key
    }

    /// Get a clone of the stop flag for external signaling
    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Start accepting connections (blocking)
    pub fn run(&mut self) -> P2PResult<()> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| P2PError::NetworkError("Server already started".to_string()))?;

        listener
            .set_nonblocking(true)
            .map_err(|e| P2PError::NetworkError(format!("Failed to set non-blocking: {}", e)))?;

        info!("P2P Server listening for connections...");

        while !self.stop_flag.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    info!("New connection from: {}", addr);
                    if let Err(e) = self.handle_connection(stream, addr) {
                        warn!("Connection handler error: {} — continuing to accept", e);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Stop the server
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Handle a single client connection
    fn handle_connection(&self, mut stream: TcpStream, addr: SocketAddr) -> P2PResult<()> {
        // The listener is non-blocking for the accept loop, but accepted
        // streams must be switched back to blocking for read/write to work.
        stream
            .set_nonblocking(false)
            .map_err(|e| P2PError::NetworkError(format!("Failed to set blocking: {}", e)))?;
        stream
            .set_nodelay(true)
            .map_err(|e| P2PError::NetworkError(format!("Failed to set nodelay: {}", e)))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| P2PError::NetworkError(format!("Failed to set timeout: {}", e)))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| P2PError::NetworkError(format!("Failed to set timeout: {}", e)))?;

        // Increment active connections
        {
            let mut count = self.active_connections.lock().unwrap();
            *count += 1;
        }

        let result = self.handle_client_session(&mut stream);

        // Decrement active connections
        {
            let mut count = self.active_connections.lock().unwrap();
            *count = count.saturating_sub(1);
        }

        if let Err(ref e) = result {
            warn!("Connection from {} ended with error: {}", addr, e);
        } else {
            info!("Connection from {} completed successfully", addr);
        }

        result
    }

    /// Handle the client session protocol
    fn handle_client_session(&self, stream: &mut TcpStream) -> P2PResult<()> {
        // Read hello message
        let hello_data = read_encrypted_message(stream, &self.encryption_key)?;
        let hello: P2PMessage = bincode::deserialize(&hello_data)
            .map_err(|e| P2PError::ProtocolError(format!("Invalid hello message: {}", e)))?;

        match hello {
            P2PMessage::Hello {
                protocol_version,
                client_name,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    let error = P2PMessage::Error {
                        message: format!(
                            "Protocol version mismatch: expected {}, got {}",
                            PROTOCOL_VERSION, protocol_version
                        ),
                    };
                    send_encrypted_message(stream, &self.encryption_key, &error)?;
                    return Err(P2PError::ProtocolError("Version mismatch".to_string()));
                }

                info!("Client connected: {:?}", client_name);

                // Send welcome with pack info
                let welcome = P2PMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    pack_info: self.mod_pack.clone(),
                };
                send_encrypted_message(stream, &self.encryption_key, &welcome)?;
                info!("[Server] Welcome sent, entering file-request loop");
            }
            _ => {
                return Err(P2PError::ProtocolError(
                    "Expected Hello message".to_string(),
                ));
            }
        }

        // Handle file requests
        loop {
            info!("[Server] Waiting for next message from client...");
            let msg_data = match read_encrypted_message(stream, &self.encryption_key) {
                Ok(data) => {
                    info!("[Server] Received message ({} bytes)", data.len());
                    data
                }
                Err(P2PError::NetworkError(ref msg)) => {
                    warn!("[Server] Connection lost while reading: {}", msg);
                    break;
                }
                Err(e) => return Err(e),
            };

            let msg: P2PMessage = bincode::deserialize(&msg_data)
                .map_err(|e| P2PError::ProtocolError(format!("Invalid message: {}", e)))?;

            match msg {
                P2PMessage::RequestFile { filename } => {
                    self.send_file(stream, &filename)?;
                }
                P2PMessage::Goodbye => {
                    info!("Client disconnected gracefully");
                    break;
                }
                _ => {
                    warn!("Unexpected message type");
                }
            }
        }

        Ok(())
    }

    /// Send a file to the client
    fn send_file(&self, stream: &mut TcpStream, filename: &str) -> P2PResult<()> {
        let path = self
            .mod_paths
            .get(filename)
            .ok_or_else(|| P2PError::FileError(format!("File not found: {}", filename)))?;

        let file = File::open(path)
            .map_err(|e| P2PError::FileError(format!("Failed to open file: {}", e)))?;
        let file_size = file
            .metadata()
            .map_err(|e| P2PError::FileError(format!("Failed to get file size: {}", e)))?
            .len();

        let mut reader = BufReader::new(file);
        let mut buffer = vec![0u8; CHUNK_SIZE];
        let mut offset = 0u64;
        let mut hasher = Sha256::new();

        info!("Sending file: {} ({} bytes)", filename, file_size);

        loop {
            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|e| P2PError::FileError(format!("Failed to read file: {}", e)))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            let is_last = offset + bytes_read as u64 >= file_size;

            let chunk = P2PMessage::FileChunk {
                filename: filename.to_string(),
                offset,
                data: buffer[..bytes_read].to_vec(),
                is_last,
            };

            send_encrypted_message(stream, &self.encryption_key, &chunk)?;

            // Wait for ACK
            let ack_data = read_encrypted_message(stream, &self.encryption_key)?;
            let _ack: P2PMessage = bincode::deserialize(&ack_data)
                .map_err(|e| P2PError::ProtocolError(format!("Invalid ACK: {}", e)))?;

            offset += bytes_read as u64;
        }

        // Send transfer complete with hash
        let hash = hex::encode(hasher.finalize());
        let complete = P2PMessage::TransferComplete {
            filename: filename.to_string(),
            hash,
        };
        send_encrypted_message(stream, &self.encryption_key, &complete)?;

        info!("File sent successfully: {}", filename);
        Ok(())
    }
}

// ============================================================================
// P2P CLIENT (RECEIVER) IMPLEMENTATION
// ============================================================================

/// P2P client for receiving mods
pub struct P2PClient {
    encryption_key: [u8; 32],
    server_addr: String,
    stop_flag: Arc<AtomicBool>,
    progress: Arc<Mutex<TransferProgress>>,
}

impl P2PClient {
    /// Create a new P2P client from raw components
    pub fn new(encryption_key: [u8; 32], server_addr: String) -> Self {
        Self {
            encryption_key,
            server_addr,
            stop_flag: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(TransferProgress {
                current_file: String::new(),
                files_completed: 0,
                total_files: 0,
                bytes_transferred: 0,
                total_bytes: 0,
                status: TransferStatus::Connecting,
            })),
        }
    }

    /// Get a shared handle to the progress tracker
    pub fn progress_handle(&self) -> Arc<Mutex<TransferProgress>> {
        self.progress.clone()
    }

    /// Get a clone of the stop flag for external signaling
    pub fn get_stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Create a new P2P client from a connection string
    pub fn from_connection_string(conn_str: &str) -> P2PResult<Self> {
        let (_, key, ip, port) = parse_connection_string(conn_str)?;
        let server_addr = format!("{}:{}", ip, port);

        Ok(Self {
            encryption_key: key,
            server_addr,
            stop_flag: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(Mutex::new(TransferProgress {
                current_file: String::new(),
                files_completed: 0,
                total_files: 0,
                bytes_transferred: 0,
                total_bytes: 0,
                status: TransferStatus::Connecting,
            })),
        })
    }

    /// Get current transfer progress
    pub fn get_progress(&self) -> TransferProgress {
        self.progress.lock().unwrap().clone()
    }

    /// Stop the transfer
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Connect and download all mods to the specified directory
    pub fn download_pack(
        &self,
        output_dir: &Path,
        client_name: Option<String>,
    ) -> P2PResult<ShareableModPack> {
        // Update progress
        {
            let mut progress = self.progress.lock().unwrap();
            progress.status = TransferStatus::Connecting;
        }

        // Connect to server
        info!("Connecting to {}", self.server_addr);
        let mut stream = TcpStream::connect(&self.server_addr)
            .map_err(|e| P2PError::NetworkError(format!("Failed to connect: {}", e)))?;

        stream
            .set_nodelay(true)
            .map_err(|e| P2PError::NetworkError(format!("Failed to set nodelay: {}", e)))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| P2PError::NetworkError(format!("Failed to set timeout: {}", e)))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(120)))
            .map_err(|e| P2PError::NetworkError(format!("Failed to set timeout: {}", e)))?;

        // Update progress
        {
            let mut progress = self.progress.lock().unwrap();
            progress.status = TransferStatus::Handshaking;
        }

        // Send hello
        let hello = P2PMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            client_name,
        };
        send_encrypted_message(&mut stream, &self.encryption_key, &hello)?;

        // Receive welcome
        let welcome_data = read_encrypted_message(&mut stream, &self.encryption_key)?;
        let welcome: P2PMessage = bincode::deserialize(&welcome_data)
            .map_err(|e| P2PError::ProtocolError(format!("Invalid welcome: {}", e)))?;

        let pack_info = match welcome {
            P2PMessage::Welcome {
                protocol_version,
                pack_info,
            } => {
                if protocol_version != PROTOCOL_VERSION {
                    return Err(P2PError::ProtocolError(
                        "Protocol version mismatch".to_string(),
                    ));
                }
                pack_info
            }
            P2PMessage::Error { message } => {
                return Err(P2PError::ProtocolError(message));
            }
            _ => {
                return Err(P2PError::ProtocolError(
                    "Expected Welcome message".to_string(),
                ));
            }
        };

        info!("Connected to share: {}", pack_info.name);

        // Brief pause to let relay tunnel stabilize before file requests
        std::thread::sleep(Duration::from_millis(500));

        // Calculate total files and bytes
        let mut total_files = 0usize;
        let mut total_bytes = 0u64;
        for mod_info in &pack_info.mods {
            total_files += 1;
            total_bytes += mod_info.size;
            for io_file in &mod_info.iostore_files {
                total_files += 1;
                total_bytes += io_file.size;
            }
        }

        // Update progress
        {
            let mut progress = self.progress.lock().unwrap();
            progress.total_files = total_files;
            progress.total_bytes = total_bytes;
            progress.status = TransferStatus::Transferring;
        }

        // Create output directory if needed
        fs::create_dir_all(output_dir).map_err(|e| {
            P2PError::FileError(format!("Failed to create output directory: {}", e))
        })?;

        // Download each file
        let mut files_completed = 0usize;
        let mut bytes_transferred = 0u64;

        for mod_info in &pack_info.mods {
            if self.stop_flag.load(Ordering::SeqCst) {
                return Err(P2PError::Cancelled);
            }

            // Download main .pak file
            let downloaded_bytes =
                self.download_file(&mut stream, &mod_info.filename, &mod_info.hash, output_dir)?;
            bytes_transferred += downloaded_bytes;
            files_completed += 1;

            {
                let mut progress = self.progress.lock().unwrap();
                progress.files_completed = files_completed;
                progress.bytes_transferred = bytes_transferred;
            }

            // Download IoStore files
            for io_file in &mod_info.iostore_files {
                if self.stop_flag.load(Ordering::SeqCst) {
                    return Err(P2PError::Cancelled);
                }

                let io_filename = mod_info
                    .filename
                    .replace(".pak", &format!(".{}", io_file.extension));

                let downloaded_bytes =
                    self.download_file(&mut stream, &io_filename, &io_file.hash, output_dir)?;
                bytes_transferred += downloaded_bytes;
                files_completed += 1;

                {
                    let mut progress = self.progress.lock().unwrap();
                    progress.files_completed = files_completed;
                    progress.bytes_transferred = bytes_transferred;
                }
            }
        }

        // Send goodbye
        let goodbye = P2PMessage::Goodbye;
        send_encrypted_message(&mut stream, &self.encryption_key, &goodbye)?;

        // Update progress
        {
            let mut progress = self.progress.lock().unwrap();
            progress.status = TransferStatus::Completed;
        }

        info!("Download completed: {} files", pack_info.mods.len());
        Ok(pack_info)
    }

    /// Download a single file
    fn download_file(
        &self,
        stream: &mut TcpStream,
        filename: &str,
        expected_hash: &str,
        output_dir: &Path,
    ) -> P2PResult<u64> {
        // Update progress
        {
            let mut progress = self.progress.lock().unwrap();
            progress.current_file = filename.to_string();
        }

        // Request file
        let request = P2PMessage::RequestFile {
            filename: filename.to_string(),
        };
        send_encrypted_message(stream, &self.encryption_key, &request)?;

        // Create output file
        let output_path = output_dir.join(filename);
        let file = File::create(&output_path)
            .map_err(|e| P2PError::FileError(format!("Failed to create file: {}", e)))?;
        let mut writer = BufWriter::new(file);
        let mut hasher = Sha256::new();
        let mut total_received = 0u64;

        // Receive chunks
        loop {
            let chunk_data = read_encrypted_message(stream, &self.encryption_key)?;
            let msg: P2PMessage = bincode::deserialize(&chunk_data)
                .map_err(|e| P2PError::ProtocolError(format!("Invalid chunk: {}", e)))?;

            match msg {
                P2PMessage::FileChunk {
                    filename: _,
                    offset: _,
                    data,
                    is_last,
                } => {
                    hasher.update(&data);
                    writer
                        .write_all(&data)
                        .map_err(|e| P2PError::FileError(format!("Failed to write: {}", e)))?;
                    total_received += data.len() as u64;

                    // Send ACK
                    let ack = P2PMessage::Ack;
                    send_encrypted_message(stream, &self.encryption_key, &ack)?;

                    if is_last {
                        break;
                    }
                }
                P2PMessage::Error { message } => {
                    // Clean up partial file
                    drop(writer);
                    let _ = fs::remove_file(&output_path);
                    return Err(P2PError::ProtocolError(message));
                }
                _ => {
                    return Err(P2PError::ProtocolError("Unexpected message".to_string()));
                }
            }
        }

        writer
            .flush()
            .map_err(|e| P2PError::FileError(format!("Failed to flush: {}", e)))?;

        // Receive transfer complete and verify hash
        let complete_data = read_encrypted_message(stream, &self.encryption_key)?;
        let complete: P2PMessage = bincode::deserialize(&complete_data)
            .map_err(|e| P2PError::ProtocolError(format!("Invalid complete message: {}", e)))?;

        match complete {
            P2PMessage::TransferComplete { filename: _, hash } => {
                let computed_hash = hex::encode(hasher.finalize());
                if computed_hash != hash || computed_hash != expected_hash {
                    // Hash mismatch - delete the file
                    let _ = fs::remove_file(&output_path);
                    return Err(P2PError::ValidationError(format!(
                        "Hash mismatch for {}: expected {}, got {}",
                        filename, expected_hash, computed_hash
                    )));
                }
            }
            _ => {
                return Err(P2PError::ProtocolError(
                    "Expected TransferComplete".to_string(),
                ));
            }
        }

        info!(
            "Downloaded and verified: {} ({} bytes)",
            filename, total_received
        );
        Ok(total_received)
    }
}

// ============================================================================
// PROTOCOL HELPERS
// ============================================================================

/// Send an encrypted message over the stream
fn send_encrypted_message<T: Serialize>(
    stream: &mut TcpStream,
    key: &[u8; 32],
    message: &T,
) -> P2PResult<()> {
    let serialized = bincode::serialize(message)
        .map_err(|e| P2PError::ProtocolError(format!("Serialization failed: {}", e)))?;

    let encrypted = encrypt_data(key, &serialized)?;

    // Write magic bytes
    stream
        .write_all(MAGIC_BYTES)
        .map_err(|e| P2PError::NetworkError(format!("Failed to write magic: {}", e)))?;

    // Write length (4 bytes, big-endian)
    let len = encrypted.len() as u32;
    stream
        .write_all(&len.to_be_bytes())
        .map_err(|e| P2PError::NetworkError(format!("Failed to write length: {}", e)))?;

    // Write encrypted data
    stream
        .write_all(&encrypted)
        .map_err(|e| P2PError::NetworkError(format!("Failed to write data: {}", e)))?;

    stream
        .flush()
        .map_err(|e| P2PError::NetworkError(format!("Failed to flush: {}", e)))?;

    Ok(())
}

/// Read an encrypted message from the stream
fn read_encrypted_message(stream: &mut TcpStream, key: &[u8; 32]) -> P2PResult<Vec<u8>> {
    // Read and verify magic bytes (manual loop for better diagnostics)
    let mut magic = [0u8; 4];
    let mut total_read = 0usize;
    while total_read < 4 {
        match stream.read(&mut magic[total_read..]) {
            Ok(0) => {
                return Err(P2PError::NetworkError(format!(
                    "Connection closed after reading {}/4 magic bytes (got {:?})",
                    total_read,
                    &magic[..total_read]
                )));
            }
            Ok(n) => total_read += n,
            Err(e) => {
                return Err(P2PError::NetworkError(format!(
                    "Read error after {}/4 magic bytes: {}",
                    total_read, e
                )));
            }
        }
    }

    if &magic != MAGIC_BYTES {
        return Err(P2PError::ProtocolError(format!(
            "Invalid magic bytes: got {:?}, expected {:?}",
            magic, MAGIC_BYTES
        )));
    }

    // Read length
    let mut len_bytes = [0u8; 4];
    stream
        .read_exact(&mut len_bytes)
        .map_err(|e| P2PError::NetworkError(format!("Failed to read length: {}", e)))?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > 100 * 1024 * 1024 {
        // 100MB max message size
        return Err(P2PError::ProtocolError("Message too large".to_string()));
    }

    // Read encrypted data
    let mut encrypted = vec![0u8; len];
    stream
        .read_exact(&mut encrypted)
        .map_err(|e| P2PError::NetworkError(format!("Failed to read data: {}", e)))?;

    // Decrypt
    decrypt_data(key, &encrypted)
}

// ============================================================================
// STATE MANAGEMENT FOR ACTIVE SESSIONS
// ============================================================================

/// Global state for managing active P2P sessions
pub struct P2PManager {
    active_server: Option<Arc<Mutex<P2PServer>>>,
    server_thread: Option<thread::JoinHandle<()>>,
    active_client: Option<Arc<P2PClient>>,
    client_thread: Option<thread::JoinHandle<P2PResult<ShareableModPack>>>,
}

impl P2PManager {
    pub fn new() -> Self {
        Self {
            active_server: None,
            server_thread: None,
            active_client: None,
            client_thread: None,
        }
    }

    /// Start hosting a mod pack
    pub fn start_sharing(
        &mut self,
        name: String,
        description: String,
        mod_paths: Vec<PathBuf>,
        creator: Option<String>,
    ) -> P2PResult<ShareSession> {
        // Stop any existing session
        self.stop_sharing();

        // Create mod pack
        let pack = create_mod_pack(name, description, &mod_paths, creator)?;

        // Create server
        let server = P2PServer::new(pack, mod_paths)?;
        let session = server.get_session();

        let server = Arc::new(Mutex::new(server));
        let server_clone = server.clone();

        // Start server thread
        let handle = thread::spawn(move || {
            if let Ok(mut server) = server_clone.lock() {
                if let Err(e) = server.run() {
                    error!("P2P Server error: {}", e);
                }
            }
        });

        self.active_server = Some(server);
        self.server_thread = Some(handle);

        Ok(session)
    }

    /// Stop hosting
    pub fn stop_sharing(&mut self) {
        if let Some(server) = self.active_server.take() {
            if let Ok(s) = server.lock() {
                s.stop();
            }
        }

        if let Some(handle) = self.server_thread.take() {
            let _ = handle.join();
        }
    }

    /// Get current share session info
    pub fn get_share_session(&self) -> Option<ShareSession> {
        self.active_server
            .as_ref()
            .and_then(|s| s.lock().ok().map(|server| server.get_session()))
    }

    /// Check if currently sharing
    pub fn is_sharing(&self) -> bool {
        self.active_server.is_some()
    }

    /// Start receiving mods from a connection string
    pub fn start_receiving(
        &mut self,
        connection_string: &str,
        output_dir: PathBuf,
        client_name: Option<String>,
    ) -> P2PResult<()> {
        // Stop any existing receive
        self.stop_receiving();

        let client = Arc::new(P2PClient::from_connection_string(connection_string)?);
        let client_clone = client.clone();

        let handle = thread::spawn(move || client_clone.download_pack(&output_dir, client_name));

        self.active_client = Some(client);
        self.client_thread = Some(handle);

        Ok(())
    }

    /// Stop receiving
    pub fn stop_receiving(&mut self) {
        if let Some(client) = self.active_client.take() {
            client.stop();
        }

        if let Some(handle) = self.client_thread.take() {
            let _ = handle.join();
        }
    }

    /// Get current transfer progress
    pub fn get_receive_progress(&self) -> Option<TransferProgress> {
        self.active_client.as_ref().map(|c| c.get_progress())
    }

    /// Check if currently receiving
    pub fn is_receiving(&self) -> bool {
        self.active_client.is_some()
    }

    /// Check if receive completed and get result
    pub fn check_receive_result(&mut self) -> Option<P2PResult<ShareableModPack>> {
        if let Some(handle) = self.client_thread.take() {
            if handle.is_finished() {
                self.active_client = None;
                return Some(
                    handle
                        .join()
                        .unwrap_or(Err(P2PError::NetworkError("Thread panicked".to_string()))),
                );
            } else {
                self.client_thread = Some(handle);
            }
        }
        None
    }
}

impl Default for P2PManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_code_generation() {
        let code = generate_share_code();
        assert_eq!(code.len(), 14); // 12 chars + 2 dashes
        assert!(code.chars().filter(|c| *c == '-').count() == 2);
    }

    #[test]
    fn test_encryption_roundtrip() {
        let key = generate_encryption_key();
        let plaintext = b"Hello, World!";
        let encrypted = encrypt_data(&key, plaintext).unwrap();
        let decrypted = decrypt_data(&key, &encrypted).unwrap();
        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_connection_string_roundtrip() {
        let key = generate_encryption_key();
        let share_code = "TEST-CODE-1234";
        let ip = "192.168.1.100";
        let port = 47820u16;

        let conn_str = create_connection_string(share_code, &key, ip, port);
        let (parsed_code, parsed_key, parsed_ip, parsed_port) =
            parse_connection_string(&conn_str).unwrap();

        assert_eq!(share_code, parsed_code);
        assert_eq!(key, parsed_key);
        assert_eq!(ip, parsed_ip);
        assert_eq!(port, parsed_port);
    }
}
