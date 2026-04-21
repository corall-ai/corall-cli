use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::Value;

#[cfg(unix)]
#[test]
fn setup_installs_bundled_polling_plugin_through_openclaw_cli() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("corall-openclaw-setup")?;
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    let config_path = temp.path().join("openclaw.json");
    let capture_path = temp.path().join("openclaw-args.txt");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&bin)?;
    fs::write(&config_path, r#"{"gateway":{},"hooks":{}}"#)?;

    write_fake_openclaw(&bin.join("openclaw"))?;

    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin.display(), old_path.to_string_lossy());
    let output = Command::new(env!("CARGO_BIN_EXE_corall"))
        .args([
            "openclaw",
            "setup",
            "--config",
            path_str(&config_path)?,
            "--webhook-token",
            "hook-token",
            "--eventbus-url",
            "http://eventbus.test:8787",
        ])
        .env("HOME", &home)
        .env("PATH", path)
        .env("OPENCLAW_CAPTURE_ARGS", &capture_path)
        .output()?;

    assert!(
        output.status.success(),
        "setup failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let staged = home.join(".corall/openclaw-plugins/corall-polling");
    assert!(staged.join("openclaw.plugin.json").is_file());
    assert!(staged.join("dist/index.js").is_file());

    let captured = fs::read_to_string(&capture_path)?;
    assert_eq!(
        captured.lines().collect::<Vec<_>>(),
        vec![
            "plugins",
            "install",
            "--force",
            staged.to_str().ok_or("staged plugin path is not utf-8")?
        ]
    );

    let cfg: Value = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
    let plugin = &cfg["plugins"]["entries"]["corall-polling"];
    assert_eq!(cfg["hooks"]["token"], "hook-token");
    assert_eq!(plugin["enabled"], true);
    assert_eq!(plugin["config"]["baseUrl"], "http://eventbus.test:8787");
    assert_eq!(plugin["config"]["credentialProfile"], "provider");

    let stdout: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(stdout["plugin"]["id"], "corall-polling");
    assert_eq!(stdout["plugin"]["installed"], true);
    assert_eq!(stdout["plugin"]["sourcePath"], staged.display().to_string());
    assert_eq!(stdout["plugin"]["baseUrl"], "http://eventbus.test:8787");

    Ok(())
}

#[cfg(unix)]
fn write_fake_openclaw(path: &Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    fs::write(
        path,
        r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$OPENCLAW_CAPTURE_ARGS"
if [ "$#" -eq 4 ] &&
   [ "$1" = "plugins" ] &&
   [ "$2" = "install" ] &&
   [ "$3" = "--force" ] &&
   [ -f "$4/openclaw.plugin.json" ] &&
   [ -f "$4/dist/index.js" ]; then
  exit 0
fi
exit 42
"#,
    )?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn path_str(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.to_str()
        .ok_or_else(|| format!("path is not valid utf-8: {}", path.display()).into())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
