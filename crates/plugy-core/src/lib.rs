pub mod bitwise;
pub mod guest;

use std::{path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ModuleFile {
    File(PathBuf),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginConfig<D> {
    pub title: String,
    pub module: ModuleFile,
    pub metadata: D
}
