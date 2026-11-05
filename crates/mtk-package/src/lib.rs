#![deny(unsafe_code)]

//! # mtk-package
//!
//! Unified binary package container codec for MoziToolKit (`.mtkcache`, `.mtkscene`).

mod error;
mod header;
mod reader;
mod toc;
mod writer;

pub use error::PackageError;
pub use header::{PackageHeader, PackageProfile, CURRENT_VERSION_MAJOR};
pub use reader::MtkPackageReader;
pub use writer::{ChunkWriteOptions, MtkPackageFileWriter, MtkPackageWriter};
