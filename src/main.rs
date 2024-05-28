use clap::{Parser, Subcommand};
use lockfile::{ArchiveBinary, Binary, FileBinary, Lockfile, PkgBinary, ToolDefinition, SCHEMA};
use once_cell::sync::Lazy as LazyLock;
use regex::Regex;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fs,
};

mod lockfile;

static GITHUB_RELEASE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        https://github\.com/
        (?P<org>[A-Za-z0-9_-]+)/
        (?P<repo>[A-Za-z0-9_-]+)/
        releases/download/
        (?P<version>v?[^/]+)/
        (?P<path>.+)",
    )
    .unwrap()
});

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

trait Common {
    fn url(&self) -> &str;
    fn sort_key(&self) -> String;
}

impl Common for Binary {
    fn url(&self) -> &str {
        match &self {
            Binary::File(file) => &file.url,
            Binary::Archive(archive) => &archive.url,
            Binary::Pkg(pkg) => &pkg.url,
        }
    }

    fn sort_key(&self) -> String {
        match &self {
            Binary::File(bin) => format!("{}_{}", bin.os, bin.cpu),
            Binary::Archive(bin) => format!("{}_{}", bin.os, bin.cpu),
            Binary::Pkg(bin) => format!("{}_{}", bin.os, bin.cpu),
        }
    }
}

struct GitHubRelease<'a> {
    org: &'a str,
    repo: &'a str,
    version: &'a str,
    path: &'a str,
}

impl GitHubRelease<'_> {
    fn from(url: &str) -> Option<GitHubRelease> {
        GITHUB_RELEASE_PATTERN.captures(url).map(|capture| {
            let (_, [org, repo, version, path]) = capture.extract();
            GitHubRelease {
                org,
                repo,
                version,
                path,
            }
        })
    }
}

fn compute_sha256(client: &reqwest::blocking::Client, url: &str) -> Result<String, Box<dyn Error>> {
    let response = client.get(url).send()?.error_for_status()?;
    let bytes = response.bytes()?;
    Ok(sha256::digest(bytes.to_vec()))
}

fn update_github_release(
    client: &reqwest::blocking::Client,
    gh_latest_releases: &mut HashMap<String, String>,
    tool: &str,
    binary: &Binary,
    release: &GitHubRelease,
) -> Result<Binary, Box<dyn Error>> {
    let org = release.org;
    let repo = release.repo;

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

    if release.version == latest_tag {
        return Ok(binary.clone());
    }

    let version = release.version.strip_prefix('v').unwrap_or(release.version);
    let latest = latest_tag.strip_prefix('v').unwrap_or(latest_tag);

    let url = format!(
        "https://github.com/{org}/{repo}/releases/download/{latest_tag}/{0}",
        release.path.replace(version, latest)
    );
    // TODO(mark): check that the new url is in .assets[].browser_download_url

    let sha256 = compute_sha256(client, &url)?;

    Ok(match binary {
        Binary::File(bin) => {
            println!(
                "Updating {tool} ({}/{}) from {version} to {latest}",
                bin.os, bin.cpu
            );
            Binary::File(FileBinary {
                url,
                cpu: bin.cpu.clone(),
                os: bin.os.clone(),
                sha256,
                headers: bin.headers.clone(),
            })
        }
        Binary::Archive(bin) => {
            println!(
                "Updating {tool} ({}/{}) from {version} to {latest}",
                bin.os, bin.cpu
            );
            Binary::Archive(ArchiveBinary {
                url,
                file: bin.file.replace(version, latest),
                cpu: bin.cpu.clone(),
                os: bin.os.clone(),
                sha256,
                headers: bin.headers.clone(),
                type_: bin.type_.clone(),
            })
        }
        Binary::Pkg(bin) => {
            println!(
                "Updating {tool} ({}/{}) from {version} to {latest}",
                bin.os, bin.cpu
            );
            Binary::Pkg(PkgBinary {
                url,
                file: bin.file.replace(version, latest),
                cpu: bin.cpu.clone(),
                os: bin.os.clone(),
                sha256,
                headers: bin.headers.clone(),
            })
        }
    })
}

fn update_lockfile(path: &std::path::Path) {
    let contents = fs::read_to_string(path).expect("Unable to load lockfile");

    let lockfile: Lockfile =
        serde_json::from_str(&contents).expect("Unable to deserialize lockfile");

    if lockfile.schema != SCHEMA {
        panic!("Unsupported lockfile schema {}", lockfile.schema)
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("multitool")
        .build()
        .unwrap();

    // basic cache of latest release lookups
    let mut gh_latest_releases: HashMap<String, String> = HashMap::new();

    let tools: BTreeMap<String, ToolDefinition> = lockfile
        .tools
        .into_iter()
        .map(|(tool, binary)| {
            let mut binaries: Vec<Binary> = binary
                .binaries
                .into_iter()
                .map(|binary| match GitHubRelease::from(binary.url()) {
                    Some(release) => update_github_release(
                        &client,
                        &mut gh_latest_releases,
                        &tool,
                        &binary,
                        &release,
                    )
                    .map_err(|e| {
                        println!("Encountered error while attempting to update {tool}: {e}")
                    })
                    .unwrap_or(binary),
                    None => binary,
                })
                .collect();

            binaries.sort_by_key(|v| v.sort_key());

            (tool, ToolDefinition { binaries })
        })
        .collect();

    let lockfile = Lockfile {
        schema: lockfile.schema,
        tools,
    };

    let contents = serde_json::to_string_pretty(&lockfile).unwrap();
    fs::write(path, contents + "\n").expect("Error updating lockfile")
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
