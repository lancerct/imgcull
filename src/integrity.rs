//! Shared hashes for source identity, cached evidence, and metadata recovery.
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// SHA-256 of an in-memory payload.
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Hash a file without loading another full-resolution copy into memory.
pub fn file_digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
