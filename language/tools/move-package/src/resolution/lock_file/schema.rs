// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Serde compatible types to deserialize the schematized parts of the lock file (everything in the
//! [move] table).  This module does not support serialization because of limitations in the `toml`
//! crate related to serializing types as inline tables.

use std::{fs::File, io::Read};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use toml::value::Value;

/// Lock file version written by this version of the compiler.  Backwards compatibility is
/// guaranteed (the compiler can read lock files with older versions), forward compatibility is not
/// (the compiler will fail to read lock files at newer versions).
///
/// TODO(amnn): Set to version 1 when stabilised.
pub const VERSION: u64 = 0;

#[derive(Deserialize)]
pub struct Dependencies {
    #[serde(rename = "dependency")]
    dependencies: Option<Vec<Dependency>>,
}

#[derive(Deserialize)]
pub struct Dependency {
    /// The name of the dependency (corresponds to the key for the dependency in the source
    /// manifest).
    pub name: String,

    /// The description of the dependency from its source manifest.  Its schema is not described in
    /// terms of serde-compatible structs, so it is deserialized into a generic data structure.
    pub source: Value,

    pub dependencies: Option<Vec<String>>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Schema<T> {
    #[serde(rename = "move")]
    move_: T,
}

#[derive(Deserialize)]
struct Header {
    version: u64,
}

impl Dependencies {
    /// Read dependencies from the lock file, assuming the file's format matches the schema expected
    /// by this lock file, and its version is not newer than the version supported by this library.
    pub fn read(lock: &mut File) -> Result<Vec<Dependency>> {
        let contents = {
            let mut buf = String::new();
            lock.read_to_string(&mut buf).context("Reading lock file")?;
            buf
        };

        let Schema {
            move_: Header { version },
        } = toml::de::from_str::<Schema<Header>>(&contents).context("Deserializing lock header")?;

        if version > VERSION {
            bail!(
                "Lock file format is too new, expected version {} or below, found {}",
                VERSION,
                version
            );
        }

        let Schema {
            move_: Dependencies { dependencies },
        } = toml::de::from_str::<Schema<Dependencies>>(&contents)
            .context("Deserializing dependencies")?;

        Ok(dependencies.unwrap_or_default())
    }
}
