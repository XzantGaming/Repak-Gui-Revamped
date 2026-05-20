#![allow(dead_code)]
//! IP Address Obfuscation Utilities
//!
//! Provides functions to obfuscate IP addresses and multiaddresses
//! for privacy protection while maintaining functionality.

use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Obfuscate an IP address for display purposes
///
/// - IPv4: Shows first octet and hashed representation (e.g., "192.xxx.xxx.xxx [a1b2c3]")
/// - IPv6: Shows first segment and hashed representation (e.g., "2001:xxxx:... [a1b2c3]")
pub fn obfuscate_ip(ip: &str) -> String {
    if let Ok(addr) = ip.parse::<IpAddr>() {
        match addr {
            IpAddr::V4(ipv4) => obfuscate_ipv4(&ipv4),
            IpAddr::V6(ipv6) => obfuscate_ipv6(&ipv6),
        }
    } else {
        // If parsing fails, just show a generic obfuscated string
        format!("***.***.***.*** [{}]", hash_string(ip))
    }
}

/// Obfuscate an IPv4 address
fn obfuscate_ipv4(ip: &Ipv4Addr) -> String {
    let octets = ip.octets();
    let hash = hash_ip_addr(&IpAddr::V4(*ip));
    format!("{}.xxx.xxx.xxx [{}]", octets[0], hash)
}

/// Obfuscate an IPv6 address
fn obfuscate_ipv6(ip: &Ipv6Addr) -> String {
    let segments = ip.segments();
    let hash = hash_ip_addr(&IpAddr::V6(*ip));
    format!("{:x}:xxxx:xxxx:... [{}]", segments[0], hash)
}

/// Create a short hash of an IP address for identification
fn hash_ip_addr(ip: &IpAddr) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.to_string().as_bytes());
    let result = hasher.finalize();
    // Take first 6 hex chars for brevity
    hex::encode(&result[..3])
}

/// Create a short hash of any string
fn hash_string(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..3])
}

/// Obfuscate a multiaddress string
///
/// Example: "/ip4/192.168.1.5/tcp/8080" -> "/ip4/192.xxx.xxx.xxx/tcp/8080 [a1b2c3]"
pub fn obfuscate_multiaddr(addr: &str) -> String {
    // Parse multiaddr components
    let parts: Vec<&str> = addr.split('/').collect();
    let mut obfuscated_parts = Vec::new();
    let mut found_ip = false;
    let mut ip_value;

    for (i, part) in parts.iter().enumerate() {
        if *part == "ip4" || *part == "ip6" {
            obfuscated_parts.push(part.to_string());
            if i + 1 < parts.len() {
                ip_value = parts[i + 1].to_string();
                obfuscated_parts.push(obfuscate_ip(&ip_value));
                found_ip = true;
            }
        } else if found_ip && i > 0 && (parts[i - 1] == "ip4" || parts[i - 1] == "ip6") {
            // Skip the IP part as we already processed it
            found_ip = false;
            continue;
        } else {
            obfuscated_parts.push(part.to_string());
        }
    }

    obfuscated_parts.join("/")
}

/// Obfuscate a list of multiaddresses
pub fn obfuscate_multiaddrs(addrs: &[String]) -> Vec<String> {
    addrs.iter().map(|addr| obfuscate_multiaddr(addr)).collect()
}

/// Check if an IP address is private/local
pub fn is_private_ip(ip: &str) -> bool {
    if let Ok(addr) = ip.parse::<IpAddr>() {
        match addr {
            IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_loopback() || ipv4.is_link_local(),
            IpAddr::V6(ipv6) => ipv6.is_loopback() || ipv6.is_unicast_link_local(),
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obfuscate_ipv4() {
        let ip = "192.168.1.100";
        let obfuscated = obfuscate_ip(ip);
        assert!(obfuscated.starts_with("192.xxx.xxx.xxx"));
        assert!(obfuscated.contains("["));
    }

    #[test]
    fn test_obfuscate_ipv6() {
        let ip = "2001:0db8:85a3:0000:0000:8a2e:0370:7334";
        let obfuscated = obfuscate_ip(ip);
        assert!(obfuscated.starts_with("2001:xxxx:xxxx:..."));
        assert!(obfuscated.contains("["));
    }

    #[test]
    fn test_obfuscate_multiaddr() {
        let addr = "/ip4/192.168.1.5/tcp/8080";
        let obfuscated = obfuscate_multiaddr(addr);
        assert!(obfuscated.contains("192.xxx.xxx.xxx"));
        assert!(obfuscated.contains("tcp/8080"));
    }

    #[test]
    fn test_is_private_ip() {
        assert!(is_private_ip("192.168.1.1"));
        assert!(is_private_ip("10.0.0.1"));
        assert!(is_private_ip("127.0.0.1"));
        assert!(!is_private_ip("8.8.8.8"));
    }
}
