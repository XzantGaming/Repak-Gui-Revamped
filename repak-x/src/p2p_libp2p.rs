#![allow(dead_code)]
//! P2P LibP2P Module - Stub Implementation

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub encryption_key: String,
    pub share_code: String,
}

impl ShareInfo {
    pub fn encode(&self) -> Result<String, Box<dyn Error>> {
        let json = serde_json::to_string(self)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(json.as_bytes()))
    }

    pub fn decode(encoded: &str) -> Result<Self, Box<dyn Error>> {
        let json_bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
        let json_str = String::from_utf8(json_bytes)?;
        let share_info: ShareInfo = serde_json::from_str(&json_str)?;
        Ok(share_info)
    }
}
