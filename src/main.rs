use clap::{Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fs,
};

#[derive(Parser)]
struct Cli {
    #[clap(long)]
    /// Path to a multitool lockfile (defaults to './multitool.lock.json')
    lockfile: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Updates GitHub release artifacts in the specified lockfile
    Update,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SupportedOs {
    Linux,
    MacOS,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SupportedCpu {
    Arm64,
    X86_64,
}

#[derive(Clone, Serialize, Deserialize)]
struct FileBinary {
    url: String,
    sha256: String,
    os: SupportedOs,
    cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ArchiveBinary {
    url: String,
    file: String,
    sha256: String,
    os: SupportedOs,
    cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    type_: Option<String>, // TODO(mark): we should probably make this an enum
}

#[derive(Clone, Serialize, Deserialize)]
struct PkgBinary {
    url: String,
    file: String,
    sha256: String,
    os: SupportedOs,
    cpu: SupportedCpu,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<HashMap<String, String>>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum BinaryUnion {
    File(FileBinary),
    Archive(ArchiveBinary),
    Pkg(PkgBinary),
}

#[derive(Serialize, Deserialize)]
struct Binary {
    binaries: Vec<BinaryUnion>,
}

impl ToString for SupportedCpu {
    fn to_string(&self) -> String {
        match &self {
            SupportedCpu::Arm64 => "arm64".to_string(),
            SupportedCpu::X86_64 => "x86_64".to_string(),
        }
    }
}

impl ToString for SupportedOs {
    fn to_string(&self) -> String {
        match &self {
            SupportedOs::Linux => "linux".to_string(),
            SupportedOs::MacOS => "macos".to_string(),
        }
    }
}

trait Common {
    fn url(&self) -> &str;
    fn sort_key(&self) -> String;
}

impl Common for BinaryUnion {
    fn url(&self) -> &str {
        match &self {
            BinaryUnion::File(file) => &file.url,
            BinaryUnion::Archive(archive) => &archive.url,
            BinaryUnion::Pkg(pkg) => &pkg.url,
        }
    }

    fn sort_key(&self) -> String {
        match &self {
            BinaryUnion::File(bin) => format!("{:?}_{:?}", bin.os, bin.cpu),
            BinaryUnion::Archive(bin) => format!("{:?}_{:?}", bin.os, bin.cpu),
            BinaryUnion::Pkg(bin) => format!("{:?}_{:?}", bin.os, bin.cpu),
        }
    }
}

fn compute_sha256(client: &reqwest::blocking::Client, url: &str) -> Result<String, Box<dyn Error>> {
    let bytes = client.get(url).send()?.bytes()?;
    Ok(sha256::digest(bytes.to_vec()))
}

fn update_github_release(
    client: &reqwest::blocking::Client,
    gh_latest_releases: &mut HashMap<String, String>,
    binary: &BinaryUnion,
    org: &str,
    repo: &str,
    version: &str,
    _path: &str,
) -> Result<BinaryUnion, Box<dyn Error>> {
    let key = format!("https://api.github.com/repos/{org}/{repo}/releases/latest");
    let raw = gh_latest_releases.entry(key.clone()).or_insert_with(|| {
        client
            .get(&key)
            .send()
            .unwrap_or_else(|_| panic!("Error making request to GitHub"))
            .text()
            .unwrap()
    });

    let response: Value = serde_json::from_str(&raw)?;
    let latest = response["tag_name"]
        .as_str()
        .unwrap_or_else(|| panic!("Failed to find tag_name in response:\n===\n{raw}\n===\n"));

    if version == latest {
        return Ok(binary.clone());
    }

    let version = version.strip_prefix("v").unwrap_or(version);
    let latest = latest.strip_prefix("v").unwrap_or(latest);

    let url = binary.url().replace(version, latest);
    // TODO(mark): check that the new url is in .assets[].browser_download_url

    let sha256 = compute_sha256(client, &url)?;

    Ok(match binary {
        BinaryUnion::File(bin) => BinaryUnion::File(FileBinary {
            url,
            cpu: bin.cpu.clone(),
            os: bin.os.clone(),
            sha256,
            headers: bin.headers.clone(),
        }),
        BinaryUnion::Archive(bin) => BinaryUnion::Archive(ArchiveBinary {
            url,
            file: bin.file.replace(version, latest),
            cpu: bin.cpu.clone(),
            os: bin.os.clone(),
            sha256,
            headers: bin.headers.clone(),
            type_: bin.type_.clone(),
        }),
        BinaryUnion::Pkg(bin) => BinaryUnion::Pkg(PkgBinary {
            url,
            file: bin.file.replace(version, latest),
            cpu: bin.cpu.clone(),
            os: bin.os.clone(),
            sha256,
            headers: bin.headers.clone(),
        }),
    })
}

fn update_lockfile(lockfile: &std::path::Path) {
    let contents = fs::read_to_string(lockfile).expect("Unable to load lockfile");

    let tools: HashMap<String, Binary> =
        serde_json::from_str(&contents).expect("Unable to deserialize lockfile");

    let github_release_pattern = Regex::new(
        r"https://github\.com/(?P<org>[A-Za-z0-9_-]+)/(?P<repo>[A-Za-z0-9_-]+)/releases/download/(?P<version>v?[^/]+)/(?P<path>.+)"
    ).unwrap();

    let client = reqwest::blocking::Client::builder()
        .user_agent("multitool")
        .build()
        .unwrap();

    // basic cache of latest release lookups
    let mut gh_latest_releases: HashMap<String, String> = HashMap::new();

    let mut new_tools: BTreeMap<String, Binary> = BTreeMap::new();
    for (tool_name, binary) in tools.into_iter() {
        let mut new_binaries: Vec<BinaryUnion> = Vec::new();
        for binary in binary.binaries.into_iter() {
            let new_binary = match github_release_pattern.captures(binary.url()) {
                Some(cap) => {
                    let (_, [org, repo, version, path]) = cap.extract();
                    update_github_release(
                        &client,
                        &mut gh_latest_releases,
                        &binary,
                        org,
                        repo,
                        version,
                        path,
                    )
                    .expect("Error while updating GitHub release")
                }
                None => binary.clone(),
            };
            new_binaries.push(new_binary)
        }

        new_binaries.sort_by_key(|v| v.sort_key());
        new_tools.insert(
            tool_name.clone(),
            Binary {
                binaries: new_binaries,
            },
        );
    }

    let contents = serde_json::to_string_pretty(&new_tools).unwrap();
    fs::write(lockfile, contents + "\n").expect("Error updating lockfile")
}

fn main() {
    let cli = Cli::parse();
    let lockfile = cli
        .lockfile
        .as_ref()
        .map(|p| p.as_path())
        .unwrap_or_else(|| std::path::Path::new("./multitool.lock.json"));

    if !lockfile.exists() {
        panic!("Cannot find lockfile '{:?}'", lockfile);
    }

    match &cli.command {
        Commands::Update => update_lockfile(lockfile),
    }
}
