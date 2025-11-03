use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::brew_api::HashMethod;

pub mod parse;
pub mod task;

#[derive(Debug, Deserialize)]
pub struct Pac {
    pub basic: Basic,
    #[serde(default)]
    pub conflicts: HashMap<String, String>,
    #[serde(default)]
    pub file: Vec<PacFile>,
    #[serde(default)]
    task: Vec<Task>,
}

#[derive(Debug, Deserialize)]
pub struct Basic {
    pub name: String,
    pub version: String,
    #[serde(rename = "self-update")]
    pub self_update: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "brew-dependencies")]
    pub brew_dependencies: Vec<String>,
    #[serde(default, rename = "pac-dependencies")]
    pub pac_dependencies: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PacFile {
    pub url: String,
    pub path: Vec<PacPath>,
    pub checksum: Option<Checksum>,
}

#[derive(Debug, Deserialize)]
pub struct Checksum {
    pub method: HashMethod,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct PacPath {
    pub original: String,
    pub target: String,
}

#[derive(Debug, Deserialize)]
pub struct Task {
    script: String,
}

#[derive(Debug)]
pub enum Dependency {
    Single(String),
    Multi(HashSet<String>),
}
