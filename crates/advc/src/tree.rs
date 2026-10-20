//! Read an authoring folder from disk or from an in-memory file map.
//!
//! `load_tree` uses this so the packer and Studio parse the same bytes.

use lastcall_core::{Error, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn join_rel(base: &str, name: &str) -> String {
    let name = name.trim_start_matches('/');
    if base.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{name}")
    }
}

pub fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn check_rel(path: &str) -> Result<()> {
    if path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err(Error::msg(format!("path escapes the project: {path}")));
    }
    Ok(())
}

pub trait ReadTree {
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    fn is_file(&self, path: &str) -> bool;
    fn is_dir(&self, path: &str) -> bool;
    fn list(&self, path: &str) -> Result<Vec<String>>;
}

pub struct DiskTree {
    root: PathBuf,
}

impl DiskTree {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf> {
        check_rel(path)?;
        Ok(if path.is_empty() {
            self.root.clone()
        } else {
            self.root.join(path)
        })
    }
}

impl ReadTree for DiskTree {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let full = self.resolve(path)?;
        fs::read(&full).map_err(|err| Error::msg(format!("{path}: {err}")))
    }

    fn is_file(&self, path: &str) -> bool {
        self.resolve(path).map(|p| p.is_file()).unwrap_or(false)
    }

    fn is_dir(&self, path: &str) -> bool {
        self.resolve(path).map(|p| p.is_dir()).unwrap_or(false)
    }

    fn list(&self, path: &str) -> Result<Vec<String>> {
        let full = self.resolve(path)?;
        if !full.is_dir() {
            return Err(Error::msg(format!("missing directory {path}")));
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&full)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            names.push(name.to_string());
        }
        names.sort();
        Ok(names)
    }
}

/// Project files keyed by forward-slash paths relative to the project root.
#[derive(Clone, Default)]
pub struct MemTree {
    files: BTreeMap<String, Vec<u8>>,
}

impl MemTree {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        let path = path.into().replace('\\', "/");
        let path = path.trim_start_matches("./").trim_start_matches('/');
        if path.is_empty() || path.split('/').any(|part| part == "..") {
            return;
        }
        self.files.insert(path.to_string(), bytes);
    }

    pub fn from_map(files: BTreeMap<String, Vec<u8>>) -> Self {
        let mut tree = Self::new();
        for (path, bytes) in files {
            tree.insert(path, bytes);
        }
        tree
    }

    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}

impl ReadTree for MemTree {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        check_rel(path)?;
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| Error::msg(format!("missing file {path}")))
    }

    fn is_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn is_dir(&self, path: &str) -> bool {
        if check_rel(path).is_err() {
            return false;
        }
        if path.is_empty() {
            return true;
        }
        let prefix = format!("{path}/");
        self.files.keys().any(|key| key.starts_with(&prefix))
    }

    fn list(&self, path: &str) -> Result<Vec<String>> {
        check_rel(path)?;
        if !self.is_dir(path) {
            return Err(Error::msg(format!("missing directory {path}")));
        }
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        let mut names = BTreeMap::new();
        for key in self.files.keys() {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            if rest.is_empty() {
                continue;
            }
            let name = rest.split('/').next().unwrap_or(rest);
            names.insert(name.to_string(), ());
        }
        Ok(names.into_keys().collect())
    }
}
