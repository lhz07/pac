use std::collections::{HashMap, HashSet};

use serde::Deserialize;

pub mod parse;
pub mod task;

#[derive(Debug, Deserialize)]
struct Pac {
    basic: Basic,
    #[serde(default)]
    conflicts: HashMap<String, String>,
    #[serde(default)]
    file: Vec<PacFile>,
    #[serde(default)]
    task: Vec<Task>,
}

#[derive(Debug, Deserialize)]
struct Basic {
    name: String,
    version: String,
    #[serde(rename = "self-update")]
    self_update: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PacFile {
    url: String,
    path: Vec<PacPath>,
    checksum: Option<Checksum>,
}

#[derive(Debug, Deserialize)]
struct Checksum {
    method: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PacPath {
    original: String,
    target: String,
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
