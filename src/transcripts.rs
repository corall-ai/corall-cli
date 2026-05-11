use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredTranscript {
    pub agent_id: String,
    pub event_id: String,
    pub message_id: String,
    pub session_key: String,
    pub hook_name: String,
    pub captured_at: String,
    pub message: String,
}

pub fn store(
    agent_id: &str,
    event_id: &str,
    message_id: &str,
    session_key: &str,
    hook_name: &str,
    message: &str,
) -> Result<PathBuf> {
    let dir = transcripts_dir()?;
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}--{}.md",
        sanitize_component(message_id),
        sanitize_component(session_key)
    ));
    let content = format!(
        "# Corall Transcript\n\n\
         - capturedAt: {captured_at}\n\
         - agentId: {agent_id}\n\
         - eventId: {event_id}\n\
         - messageId: {message_id}\n\
         - sessionKey: {session_key}\n\
         - hookName: {hook_name}\n\n\
         ## Message\n\n\
         {message}\n",
        captured_at = chrono_like_now(),
    );
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

pub fn load_by_session_key(session_key: &str) -> Result<StoredTranscript> {
    let dir = transcripts_dir()?;
    let entries =
        fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let transcript = parse_transcript(&path)?;
        if transcript.session_key == session_key {
            return Ok(transcript);
        }
    }
    bail!("no local transcript found for sessionKey `{session_key}`")
}

fn transcripts_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    Ok(home.join(".corall").join("transcripts"))
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn chrono_like_now() -> String {
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    seconds.to_string()
}

fn parse_transcript(path: &PathBuf) -> Result<StoredTranscript> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut agent_id = None;
    let mut event_id = None;
    let mut message_id = None;
    let mut session_key = None;
    let mut hook_name = None;
    let mut captured_at = None;
    let mut in_message = false;
    let mut message_lines = Vec::new();

    for line in content.lines() {
        if in_message {
            message_lines.push(line);
            continue;
        }
        if line == "## Message" {
            in_message = true;
            continue;
        }
        if let Some(value) = line.strip_prefix("- agentId: ") {
            agent_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- eventId: ") {
            event_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- messageId: ") {
            message_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- sessionKey: ") {
            session_key = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- hookName: ") {
            hook_name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- capturedAt: ") {
            captured_at = Some(value.to_string());
        }
    }

    Ok(StoredTranscript {
        agent_id: agent_id.context("missing agentId in transcript")?,
        event_id: event_id.context("missing eventId in transcript")?,
        message_id: message_id.context("missing messageId in transcript")?,
        session_key: session_key.context("missing sessionKey in transcript")?,
        hook_name: hook_name.context("missing hookName in transcript")?,
        captured_at: captured_at.context("missing capturedAt in transcript")?,
        message: message_lines.join("\n").trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    #[test]
    fn store_and_load_round_trip() {
        let temp = tempfile_dir("corall-transcript-test");
        let original_home = env::var_os("HOME");
        unsafe {
            env::set_var("HOME", temp.display().to_string());
        }

        let path = store(
            "agent-1",
            "stream-1",
            "order.paid:1",
            "hook:corall:1",
            "Corall",
            "hello world",
        )
        .unwrap();
        assert!(path.exists());

        let loaded = load_by_session_key("hook:corall:1").unwrap();
        assert_eq!(loaded.agent_id, "agent-1");
        assert_eq!(loaded.message_id, "order.paid:1");
        assert_eq!(loaded.message, "hello world");

        if let Some(home) = original_home {
            unsafe {
                env::set_var("HOME", home);
            }
        }
    }

    fn tempfile_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }
}
