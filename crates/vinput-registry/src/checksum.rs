//! Registry asset checksum verification helpers.
//!
//! This module verifies bytes or files before future asset materialization code
//! exists. It does not download assets, extract archives, install files, or
//! mutate configuration.

use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use sha2::{Digest, Sha256};
use thiserror::Error;

/// SHA-256 verification errors.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum RegistrySha256Error {
    /// Expected checksum is not a lowercase 64-character hexadecimal SHA-256.
    #[error("invalid expected sha256 checksum `{0}`")]
    InvalidExpected(String),
    /// Actual bytes did not match the expected checksum.
    #[error("sha256 mismatch: expected {expected}, actual {actual}")]
    Mismatch {
        /// Expected lowercase hexadecimal SHA-256.
        expected: String,
        /// Actual lowercase hexadecimal SHA-256.
        actual: String,
    },
    /// Bytes could not be read for checksum verification.
    #[error("failed to read bytes for sha256 verification: {message}")]
    Read {
        /// Sanitized I/O failure message.
        message: String,
    },
    /// A file could not be opened before checksum verification.
    #[error("failed to open `{path}` for sha256 verification: {message}")]
    OpenFile {
        /// File path.
        path: String,
        /// Sanitized I/O failure message.
        message: String,
    },
}

/// Computes the lowercase hexadecimal SHA-256 digest of in-memory bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Verifies in-memory bytes against a lowercase hexadecimal SHA-256 checksum.
pub fn verify_sha256_bytes(bytes: &[u8], expected: &str) -> Result<(), RegistrySha256Error> {
    validate_expected_sha256(expected)?;
    compare_sha256(sha256_hex(bytes), expected)
}

/// Streams bytes from a reader and verifies them against a lowercase SHA-256.
pub fn verify_sha256_reader(reader: impl Read, expected: &str) -> Result<(), RegistrySha256Error> {
    validate_expected_sha256(expected)?;
    let actual = sha256_reader_hex(reader)?;
    compare_sha256(actual, expected)
}

/// Opens a file and verifies it against a lowercase hexadecimal SHA-256.
pub fn verify_sha256_file(
    path: impl AsRef<Path>,
    expected: &str,
) -> Result<(), RegistrySha256Error> {
    validate_expected_sha256(expected)?;
    let path = path.as_ref();
    let file = File::open(path).map_err(|error| RegistrySha256Error::OpenFile {
        path: path.display().to_string(),
        message: sanitize_io_error(&error),
    })?;
    verify_sha256_reader(file, expected)
}

fn sha256_reader_hex(mut reader: impl Read) -> Result<String, RegistrySha256Error> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| RegistrySha256Error::Read {
                message: sanitize_io_error(&error),
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn compare_sha256(actual: String, expected: &str) -> Result<(), RegistrySha256Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(RegistrySha256Error::Mismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

fn validate_expected_sha256(expected: &str) -> Result<(), RegistrySha256Error> {
    if expected.len() == 64
        && expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(RegistrySha256Error::InvalidExpected(expected.to_owned()))
    }
}

fn sanitize_io_error(error: &io::Error) -> String {
    error.kind().to_string()
}
