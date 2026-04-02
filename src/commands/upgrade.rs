use std::fs;
use std::io::Write;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use sha2::Digest;
use sha2::Sha256;

const GITHUB_API: &str = "https://api.github.com/repos/corall-ai/corall-cli/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

fn target_triple() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-musl"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-musl"),
        (os, arch) => bail!("unsupported platform: {os}/{arch}"),
    }
}

pub async fn run() -> Result<()> {
    eprintln!("Checking latest version...");

    let client = reqwest::Client::builder()
        .user_agent(concat!("corall-cli/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let release: serde_json::Value = client
        .get(GITHUB_API)
        .send()
        .await
        .context("failed to fetch release info from GitHub")?
        .json()
        .await
        .context("failed to parse GitHub release response")?;

    let latest = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .context("no tag_name in GitHub release response")?;

    let latest_version = latest.trim_start_matches('v');
    if latest_version == CURRENT_VERSION {
        println!("Already up to date (v{CURRENT_VERSION}).");
        return Ok(());
    }

    eprintln!("Upgrading v{CURRENT_VERSION} → {latest}...");

    let target = target_triple()?;
    let asset_name = format!("corall-{latest}-{target}.zip");
    let base_url = format!("https://github.com/corall-ai/corall-cli/releases/download/{latest}");

    eprintln!("Downloading {asset_name}...");
    let zip_bytes = client
        .get(format!("{base_url}/{asset_name}"))
        .send()
        .await
        .context("failed to download release archive")?
        .error_for_status()
        .context("release archive not found")?
        .bytes()
        .await
        .context("failed to read release archive")?;

    eprintln!("Verifying checksum...");
    let checksum_text = client
        .get(format!("{base_url}/{asset_name}.sha256"))
        .send()
        .await
        .context("failed to download checksum file")?
        .error_for_status()
        .context("checksum file not found")?
        .text()
        .await
        .context("failed to read checksum file")?;

    let expected = checksum_text
        .split_whitespace()
        .next()
        .context("checksum file is empty")?;

    let actual = hex::encode(Sha256::digest(&zip_bytes));
    if actual != expected {
        bail!("checksum mismatch — expected {expected}, got {actual}");
    }

    eprintln!("Extracting...");
    let cursor = std::io::Cursor::new(&zip_bytes[..]);
    let mut archive = zip::ZipArchive::new(cursor).context("failed to open zip archive")?;

    let binary_name = if std::env::consts::OS == "windows" {
        "corall.exe"
    } else {
        "corall"
    };

    let mut binary_bytes = Vec::new();
    {
        let mut entry = archive
            .by_name(binary_name)
            .with_context(|| format!("'{binary_name}' not found in archive"))?;
        std::io::copy(&mut entry, &mut binary_bytes)?;
    }

    let current_exe = std::env::current_exe().context("failed to locate current binary")?;
    let tmp_path = current_exe.with_extension("upgrade_tmp");

    {
        let mut f =
            fs::File::create(&tmp_path).context("failed to create temporary file for upgrade")?;
        f.write_all(&binary_bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(fs::Permissions::from_mode(0o755))?;
        }
    }

    fs::rename(&tmp_path, &current_exe).context("failed to replace binary")?;

    println!("Successfully upgraded to {latest}.");
    Ok(())
}
