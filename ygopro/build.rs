//! Read the upstream ygopro version from this crate's `[package.metadata]`
//! and export it as build-time env vars.
//!
//! The version must live in this crate's own manifest instead of the
//! workspace root so the published `.crate` stays standalone: downstream
//! cargo only sees the package directory, never a parent workspace manifest.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set")).join("Cargo.toml");
    let content = fs::read_to_string(&manifest_path).expect("cannot read Cargo.toml");
    let manifest: toml::Value = toml::from_str(&content).expect("cannot parse Cargo.toml");
    let version_string = manifest["package"]["metadata"]["ygopro-version"].as_str().expect("package.metadata.ygopro-version is missing");
    let version = semver::Version::parse(version_string).expect("package.metadata.ygopro-version is not a valid semver version");

    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rustc-env=YGOPRO_VERSION_MAJOR={}", version.major);
    println!("cargo:rustc-env=YGOPRO_VERSION_MINOR={}", version.minor);
    println!("cargo:rustc-env=YGOPRO_VERSION_PATCH={}", version.patch);
}
