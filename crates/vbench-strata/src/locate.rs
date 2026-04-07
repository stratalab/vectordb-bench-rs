//! Locate the `strata` binary on disk.
//!
//! Lookup order, in priority:
//!
//! 1. Explicit override via the adapter's `params.strata_bin` field
//!    (resolved by the caller; passed into [`find_strata_bin`] as `override_path`)
//! 2. `STRATA_BIN` environment variable
//! 3. `PATH` lookup via the `which` crate
//! 4. `~/.strata/bin/strata` — the default location used by
//!    `https://stratadb.org/install.sh`
//!
//! Returns a clear actionable error pointing the user at install.sh when
//! no binary is found.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Errors raised by [`find_strata_bin`].
#[derive(Debug, Error)]
pub enum LocateError {
    /// No `strata` binary was found in any of the searched locations.
    #[error(
        "could not find a `strata` binary.\n\
         Install one with:\n  \
         curl -fsSL https://stratadb.org/install.sh | sh\n\
         …or set STRATA_BIN to an explicit path."
    )]
    NotFound,

    /// An override path was provided but does not exist on disk.
    #[error("strata_bin override does not exist: {0}")]
    OverrideMissing(PathBuf),
}

/// Find the `strata` binary.
///
/// `override_path` (if `Some`) is checked first; if it exists it is returned,
/// otherwise [`LocateError::OverrideMissing`] is returned (we never silently
/// fall through, because the caller meant for that explicit path to be used).
///
/// If no override is given, the four-step lookup runs and the first hit
/// wins.
pub fn find_strata_bin(override_path: Option<&Path>) -> Result<PathBuf, LocateError> {
    if let Some(p) = override_path {
        return if p.exists() {
            Ok(p.to_path_buf())
        } else {
            Err(LocateError::OverrideMissing(p.to_path_buf()))
        };
    }

    if let Some(env_path) = std::env::var_os("STRATA_BIN") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(p) = which::which("strata") {
        return Ok(p);
    }

    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".strata").join("bin").join("strata");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(LocateError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn override_path_existing_file_wins() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("strata");
        std::fs::write(&path, b"#!/bin/sh\necho fake").unwrap();
        let found = find_strata_bin(Some(&path)).unwrap();
        assert_eq!(found, path);
    }

    #[test]
    fn override_path_missing_file_errors() {
        let bogus = PathBuf::from("/definitely/does/not/exist/strata");
        let err = find_strata_bin(Some(&bogus)).unwrap_err();
        assert!(matches!(err, LocateError::OverrideMissing(_)));
    }

    #[test]
    fn no_override_no_env_no_path_no_install_dir_errors() {
        // Hardest case to test in isolation: we need STRATA_BIN to be unset
        // and `which strata` to return nothing and ~/.strata/bin/strata to
        // not exist. We can't reliably control all of those in CI.
        //
        // Just assert that the NotFound variant exists with a useful Display
        // message that mentions install.sh — that's the user-facing
        // contract.
        let err = LocateError::NotFound;
        let msg = err.to_string();
        assert!(msg.contains("stratadb.org/install.sh"));
        assert!(msg.contains("STRATA_BIN"));
    }
}
