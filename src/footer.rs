//! DuckDB extension metadata footer.
//!
//! Byte-for-byte replica of the logic in `duckdb_nest/scripts/metadata.py`,
//! implemented in pure Rust so the build pipeline has no Python dependency.
//!
//! A `.duckdb_extension` file is a shared library (`.dll`/`.so`/`.dylib`) with
//! a fixed 534-byte metadata footer appended. The footer tells the DuckDB
//! loader the platform, the C-API version the extension was built against, and
//! the extension's version. A mismatched `--duckdb-version` is rejected at
//! LOAD time with `Unsupported C CAPI version detected during extension
//! initialization`.

/// The C-API version the scaffolded template is built against. Matches what
/// `duckdb_nest` stamps (duckdb-rs `~1.10504.0` → C-API v1.2.0), the most
/// host-compatible choice: any DuckDB host with C-API v1.2.0 or newer can
/// load the extension.
pub const DEFAULT_CAPI_VERSION: &str = "v1.2.0";

fn padded_byte_string(s: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    let bytes = s.as_bytes();
    let n = bytes.len().min(32);
    out[..n].copy_from_slice(&bytes[..n]);
    out
}

fn start_signature() -> Vec<u8> {
    let mut s = Vec::with_capacity(22);
    s.extend_from_slice(&[0, 147, 4, 16]);
    s.extend_from_slice(b"duckdb_signature");
    s.extend_from_slice(&[128, 4]);
    s
}

/// Build the 534-byte metadata footer.
pub fn footer_bytes(platform: &str, capi_version: &str, extension_version: &str) -> Vec<u8> {
    let mut out = start_signature();
    // FIELD8..FIELD1, as in metadata.py (order matters).
    out.extend_from_slice(&padded_byte_string("")); // FIELD8 (unused)
    out.extend_from_slice(&padded_byte_string("")); // FIELD7 (unused)
    out.extend_from_slice(&padded_byte_string("")); // FIELD6 (unused)
    out.extend_from_slice(&padded_byte_string("C_STRUCT")); // FIELD5 abi_type
    out.extend_from_slice(&padded_byte_string(extension_version)); // FIELD4
    out.extend_from_slice(&padded_byte_string(capi_version)); // FIELD3
    out.extend_from_slice(&padded_byte_string(platform)); // FIELD2
    out.extend_from_slice(&padded_byte_string("4")); // FIELD1 header signature
    out.extend_from_slice(&[0u8; 256]); // signature space
    out
}

/// Append the footer to a shared library in place. Returns total size.
pub fn append_footer(path: &std::path::Path, platform: &str, capi_version: &str, extension_version: &str) -> std::io::Result<usize> {
    use std::io::Write;
    let footer = footer_bytes(platform, capi_version, extension_version);
    let mut f = std::fs::OpenOptions::new().append(true).open(path)?;
    f.write_all(&footer)?;
    f.flush()?;
    Ok(std::fs::metadata(path)?.len() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_is_534_bytes() {
        assert_eq!(footer_bytes("windows_amd64", "v1.2.0", "0.1.0").len(), 534);
    }

    #[test]
    fn footer_matches_metadata_py_field_layout() {
        let f = footer_bytes("linux_amd64", "v1.2.0", "1.2.3");
        // start_signature: [0,147,4,16,"duckdb_signature",128,4]
        assert_eq!(&f[0..4], &[0, 147, 4, 16]);
        assert_eq!(&f[4..20], b"duckdb_signature");
        assert_eq!(f[20], 128);
        assert_eq!(f[21], 4);
        // FIELD5 (abi_type) is the 4th padded field: offset 22 + 3*32 = 118
        assert_eq!(&f[118..118 + 8], b"C_STRUCT");
        // FIELD4 (extension version): offset 22 + 4*32 = 150
        assert_eq!(&f[150..150 + 5], b"1.2.3");
        // FIELD3 (capi): offset 182
        assert_eq!(&f[182..182 + 6], b"v1.2.0");
        // FIELD2 (platform): offset 214
        assert_eq!(&f[214..214 + 11], b"linux_amd64");
        // FIELD1 (header sig "4"): offset 246
        assert_eq!(f[246], b'4');
    }
}
