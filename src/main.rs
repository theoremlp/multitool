use clap::{Parser, Subcommand};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt::Display,
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

impl Display for SupportedCpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for SupportedOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
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
    path: &str,
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

    let response: Value = serde_json::from_str(raw)?;
    let latest_tag = response["tag_name"]
        .as_str()
        .unwrap_or_else(|| panic!("Failed to find tag_name in response:\n===\n{raw}\n===\n"));

    if version == latest_tag {
        return Ok(binary.clone());
    }

    let version = version.strip_prefix('v').unwrap_or(version);
    let latest = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

    let url = format!(
        "https://github.com/{org}/{repo}/releases/download/{latest_tag}/{0}",
        path.replace(version, latest)
    );
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

    let tools: BTreeMap<String, Binary> = tools
        .into_iter()
        .map(|(tool, binary)| {
            let mut binaries: Vec<BinaryUnion> = binary
                .binaries
                .into_iter()
                .map(
                    |binary| match github_release_pattern.captures(binary.url()) {
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
                            .map_err(|e| {
                                println!("Encountered error while attempting to update {tool}: {e}")
                            })
                            .unwrap_or(binary)
                        }
                        None => binary,
                    },
                )
                .collect();

            binaries.sort_by_key(|v| v.sort_key());

            (tool, Binary { binaries })
        })
        .collect();

    let contents = serde_json::to_string_pretty(&tools).unwrap();
    fs::write(lockfile, contents + "\n").expect("Error updating lockfile")
}

fn main() {
    let cli = Cli::parse();
    let lockfile = cli
        .lockfile
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("./multitool.lock.json"));

    if !lockfile.exists() {
        panic!("Cannot find lockfile '{:?}'", lockfile);
    }

    match &cli.command {
        Commands::Update => update_lockfile(lockfile),
    }
}
