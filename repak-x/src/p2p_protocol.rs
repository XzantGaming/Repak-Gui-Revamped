#![allow(dead_code)]
//! P2P File Transfer Protocol for libp2p
//!
//! Defines the request/response protocol for file transfers over libp2p streams.

use async_trait::async_trait;
use futures::prelude::*;
use libp2p::request_response as req_resp;
use libp2p::request_response::{Codec, ProtocolSupport, ResponseChannel};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use std::io;

// ============================================================================
// PROTOCOL DEFINITION
// ============================================================================

/// Protocol name for file transfer
#[derive(Debug, Clone)]
pub struct FileTransferProtocol;

impl AsRef<str> for FileTransferProtocol {
    fn as_ref(&self) -> &str {
        "/repak/file-transfer/1.0.0"
    }
}

// ============================================================================
// MESSAGE TYPES
// ============================================================================

/// Request types for file transfer protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferRequest {
    /// Request mod pack information
    GetPackInfo,
    /// Request a specific file
    GetFile { filename: String },
    /// Request a specific chunk of a file
    GetChunk {
        filename: String,
        offset: u64,
        size: usize,
    },
    /// Ping to keep connection alive
    Ping,
}

/// Response types for file transfer protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileTransferResponse {
    /// Mod pack information
    PackInfo {
        pack_data: Vec<u8>, // Serialized ShareableModPack
    },
    /// File chunk data
    FileChunk {
        filename: String,
        offset: u64,
        data: Vec<u8>,
        is_last: bool,
        hash: String, // SHA256 of this chunk
    },
    /// Transfer complete with final hash
    TransferComplete {
        filename: String,
        total_size: u64,
        hash: String, // SHA256 of entire file
    },
    /// Pong response
    Pong,
    /// Error occurred
    Error { message: String },
}

// ============================================================================
// CODEC IMPLEMENTATION
// ============================================================================

/// Codec for encoding/decoding file transfer messages
#[derive(Debug, Clone, Default)]
pub struct FileTransferCodec;

#[async_trait]
impl Codec for FileTransferCodec {
    type Protocol = FileTransferProtocol;
    type Request = FileTransferRequest;
    type Response = FileTransferResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes)
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read message data
        let mut data = vec![0u8; len];
        io.read_exact(&mut data).await?;

        // Deserialize
        bincode::deserialize(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        // Read length prefix (4 bytes)
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read message data
        let mut data = vec![0u8; len];
        io.read_exact(&mut data).await?;

        // Deserialize
        bincode::deserialize(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Serialize
        let data =
            bincode::serialize(&req).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Write length prefix
        let len = data.len() as u32;
        io.write_all(&len.to_be_bytes()).await?;

        // Write data
        io.write_all(&data).await?;
        io.flush().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        // Serialize
        let data =
            bincode::serialize(&res).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Write length prefix
        let len = data.len() as u32;
        io.write_all(&len.to_be_bytes()).await?;

        // Write data
        io.write_all(&data).await?;
        io.flush().await?;

        Ok(())
    }
}

// ============================================================================
// PROTOCOL CONFIGURATION
// ============================================================================

/// Create a configured file transfer protocol
pub fn create_file_transfer_protocol() -> req_resp::Behaviour<FileTransferCodec> {
    let protocols = vec![(FileTransferProtocol, ProtocolSupport::Full)];
    let config = req_resp::Config::default();

    req_resp::Behaviour::new(protocols.into_iter(), config)
}

// ============================================================================
// EVENT HANDLING
// ============================================================================

/// Events from the file transfer protocol
#[derive(Debug)]
pub enum FileTransferEvent {
    /// Received a request
    RequestReceived {
        peer: PeerId,
        request: FileTransferRequest,
        channel: ResponseChannel<FileTransferResponse>,
    },
    /// Received a response
    ResponseReceived {
        peer: PeerId,
        response: FileTransferResponse,
    },
    /// Request failed
    RequestFailed { peer: PeerId, error: String },
}

impl From<req_resp::Event<FileTransferRequest, FileTransferResponse>> for FileTransferEvent {
    fn from(event: req_resp::Event<FileTransferRequest, FileTransferResponse>) -> Self {
        match event {
            req_resp::Event::Message { peer, message } => match message {
                req_resp::Message::Request {
                    request, channel, ..
                } => FileTransferEvent::RequestReceived {
                    peer,
                    request,
                    channel,
                },
                req_resp::Message::Response { response, .. } => {
                    FileTransferEvent::ResponseReceived { peer, response }
                }
            },
            req_resp::Event::OutboundFailure { peer, error, .. } => {
                FileTransferEvent::RequestFailed {
                    peer,
                    error: format!("{:?}", error),
                }
            }
            req_resp::Event::InboundFailure { peer, error, .. } => {
                FileTransferEvent::RequestFailed {
                    peer,
                    error: format!("{:?}", error),
                }
            }
            _ => FileTransferEvent::RequestFailed {
                peer: PeerId::random(),
                error: "Unknown event".to_string(),
            },
        }
    }
}
