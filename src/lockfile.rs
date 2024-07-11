use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};

pub const SCHEMA: &str =
    "https://raw.githubusercontent.com/theoremlp/rules_multitool/main/lockfile.schema.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportedOs {
    Linux,
    MacOS,
    Windows,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportedCpu {
    Arm64,
    X86_64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FileBinary {
    pub url: String,
    pub sha256: String,
    pub os: SupportedOs,
    pub cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_patterns: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ArchiveBinary {
    pub url: String,
    pub file: String,
    pub sha256: String,
    pub os: SupportedOs,
    pub cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>, // TODO(mark): we should probably make this an enum
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_patterns: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PkgBinary {
    pub url: String,
    pub file: String,
    pub sha256: String,
    pub os: SupportedOs,
    pub cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_patterns: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Binary {
    File(FileBinary),
    Archive(ArchiveBinary),
    Pkg(PkgBinary),
}

#[derive(Serialize, Deserialize)]
pub struct ToolDefinition {
    pub binaries: Vec<Binary>,
}

impl Display for SupportedCpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            SupportedCpu::Arm64 => write!(f, "arm64"),
            SupportedCpu::X86_64 => write!(f, "x86_64"),
        }
    }
}

impl Display for SupportedOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            SupportedOs::Linux => write!(f, "linux"),
            SupportedOs::MacOS => write!(f, "macos"),
            SupportedOs::Windows => write!(f, "windows"),
        }
    }
}

fn schema() -> String {
    SCHEMA.to_owned()
}

#[derive(Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(rename = "$schema", default = "schema")]
    pub schema: String,

    #[serde(flatten)]
    pub tools: BTreeMap<String, ToolDefinition>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_empty_lockfile() {
        let lockfile: Lockfile = serde_json::from_str("{}").unwrap();
        assert_eq!(lockfile.schema, schema());
        assert_eq!(lockfile.tools.len(), 0);
    }

    #[test]
    fn deserialize_lockfile_with_schema_and_no_tools() {
        let lockfile: Lockfile = serde_json::from_str(r#"{
           "$schema": "https://raw.githubusercontent.com/theoremlp/rules_multitool/main/lockfile.schema.json"
        }"#).unwrap();
        assert_eq!(
            lockfile.schema,
            "https://raw.githubusercontent.com/theoremlp/rules_multitool/main/lockfile.schema.json"
                .to_owned()
        );
        assert_eq!(lockfile.tools.len(), 0);
    }

    #[test]
    fn deserialize_lockfile_with_schema_and_tools() {
        let lockfile: Lockfile = serde_json::from_str(r#"{
           "$schema": "https://raw.githubusercontent.com/theoremlp/rules_multitool/main/lockfile.schema.json",
           "tool-name": {
             "binaries": [
                {
                  "kind": "file",
                  "url": "https://github.com/theoremlp/multitool/releases/download/v0.2.1/multitool-x86_64-unknown-linux-gnu.tar.xz",
                  "sha256": "9523faf97e4e3fea5f98ba9d051e67c90799182580d8ae56cba2e45c7de0b4ce",
                  "os": "linux",
                  "cpu": "x86_64"
                }
             ]
           }
        }"#).unwrap();
        assert_eq!(
            lockfile.schema,
            "https://raw.githubusercontent.com/theoremlp/rules_multitool/main/lockfile.schema.json"
                .to_owned()
        );
        assert_eq!(lockfile.tools.len(), 1);
        assert_eq!(lockfile.tools["tool-name"].binaries.len(), 1);
        // TOOD(mark): richer tests
    }
}
