#![warn(missing_docs, unused_variables, unused_crate_dependencies)]

//! Shared core for Pocket Ark client
//!
//! This library handles creating and running the local servers required
//! for connecting to Pocket Ark servers.
//!
//! It provides shared backend for the different variants to make it easier
//! to keep feature parody across versions

// Re-exports for dependencies
pub use reqwest;
pub use semver::Version;
use semver::{BuildMetadata, Prerelease};
pub use url::Url;

pub mod api;
pub mod ctx;
pub mod servers;
pub mod ssl;
pub mod update;
pub mod virtual_peer;

/// Version constant for the backend
pub const SHARED_BACKEND_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The minimum server version supported by this client
fn min_server_version() -> Version {
    Version {
        major: 0,
        minor: 0,
        patch: 1,
        pre: Prerelease::new("alpha.1").expect("invalid pre-release version"),
        build: BuildMetadata::EMPTY,
    }
}
