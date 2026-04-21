use std::error::Error;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use ring::signature;
use serde_json::Value;
use serde_json::json;

#[test]
fn browser_approve_signs_challenge_without_leaking_secrets() -> Result<(), Box<dyn Error>> {
    let challenge = b"corall browser login challenge";
    let challenge_hex = hex::encode(challenge);
    let state = Arc::new(Mutex::new(FakeAuthState {
        challenge_hex: challenge_hex.clone(),
        public_key: None,
        saw_register_without_password: false,
        saw_valid_browser_signature: false,
    }));
    let server = FakeAuthServer::start(state.clone())?;
    let home = TempHome::new("corall-cli-browser-auth")?;

    let help = run_corall(&home, &["auth", "register", "--help"])?;
    assert!(help.status.success(), "help failed: {help:?}");
    let help_stdout = String::from_utf8(help.stdout)?;
    assert!(!help_stdout.contains("--email"));
    assert!(!help_stdout.contains("--password"));
    assert!(help_stdout.contains("--name"));

    let register = run_corall(
        &home,
        &[
            "--profile",
            "agent-test",
            "auth",
            "register",
            &server.base_url(),
            "--name",
            "Agent Test",
        ],
    )?;
    assert!(register.status.success(), "register failed: {register:?}");
    let register_stdout = String::from_utf8(register.stdout)?;
    assert!(!register_stdout.contains("privateKeyPkcs8"));
    assert!(!register_stdout.contains("password"));

    let approve = run_corall(
        &home,
        &[
            "--profile",
            "agent-test",
            "auth",
            "browser",
            "approve",
            &server.base_url(),
            "--code",
            "ABCD-EFGH",
        ],
    )?;
    assert!(approve.status.success(), "approve failed: {approve:?}");
    let approve_stdout = String::from_utf8(approve.stdout)?;
    assert!(approve_stdout.contains(r#""approved": true"#));
    assert!(!approve_stdout.contains("token"));
    assert!(!approve_stdout.contains("privateKeyPkcs8"));
    assert!(!approve_stdout.contains("signature"));

    let wrong_site = run_corall(
        &home,
        &[
            "--profile",
            "agent-test",
            "auth",
            "browser",
            "approve",
            "http://127.0.0.1:9",
            "--code",
            "ABCD-EFGH",
        ],
    )?;
    assert!(!wrong_site.status.success());
    let wrong_site_stderr = String::from_utf8(wrong_site.stderr)?;
    assert!(wrong_site_stderr.contains("belong to"));

    let state = state.lock().unwrap();
    assert!(state.saw_register_without_password);
    assert!(state.saw_valid_browser_signature);
    Ok(())
}

fn run_corall(home: &TempHome, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_corall"))
        .args(args)
        .env("HOME", home.path())
        .output()?)
}

#[derive(Debug)]
struct FakeAuthState {
    challenge_hex: String,
    public_key: Option<String>,
    saw_register_without_password: bool,
    saw_valid_browser_signature: bool,
}

struct FakeAuthServer {
    addr: SocketAddr,
    handle: Option<thread::JoinHandle<Result<(), String>>>,
}

impl FakeAuthServer {
    fn start(state: Arc<Mutex<FakeAuthState>>) -> Result<Self, Box<dyn Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let handle = thread::spawn(move || {
            for _ in 0..3 {
                let (stream, _) = listener.accept().map_err(|e| e.to_string())?;
                handle_request(stream, &state)?;
            }
            Ok(())
        });
        Ok(Self {
            addr,
            handle: Some(handle),
        })
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for FakeAuthServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => panic!("fake auth server failed: {e}"),
                Err(_) => panic!("fake auth server panicked"),
            }
        }
    }
}

fn handle_request(mut stream: TcpStream, state: &Arc<Mutex<FakeAuthState>>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let request = read_http_request(&mut stream)?;
    match request.path.as_str() {
        "/api/auth/register" => handle_register(stream, state, request.body),
        "/api/auth/browser/challenge" => handle_browser_challenge(stream, state, request.body),
        "/api/auth/browser/approve" => handle_browser_approve(stream, state, request.body),
        _ => respond_json(stream, 404, json!({ "error": "not found" })),
    }
}

fn handle_register(
    stream: TcpStream,
    state: &Arc<Mutex<FakeAuthState>>,
    body: Vec<u8>,
) -> Result<(), String> {
    let body: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let public_key = body
        .get("publicKey")
        .and_then(Value::as_str)
        .ok_or("register missing publicKey")?;
    if body.get("email").is_some() || body.get("password").is_some() {
        return Err("register leaked legacy email/password fields".to_string());
    }

    let mut state = state.lock().unwrap();
    state.public_key = Some(public_key.to_string());
    state.saw_register_without_password = true;

    respond_json(
        stream,
        201,
        json!({
            "token": "server-register-token",
            "user": {
                "id": "user-agent-test",
                "name": body.get("name").and_then(Value::as_str).unwrap_or("Agent Test"),
                "publicKey": public_key,
                "status": "ACTIVE",
                "isAdmin": false,
                "createdAt": "2026-04-22T00:00:00"
            }
        }),
    )
}

fn handle_browser_challenge(
    stream: TcpStream,
    state: &Arc<Mutex<FakeAuthState>>,
    body: Vec<u8>,
) -> Result<(), String> {
    let body: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    assert_eq!(body["code"], "ABCD-EFGH");
    let public_key = body
        .get("publicKey")
        .and_then(Value::as_str)
        .ok_or("challenge missing publicKey")?;

    let state = state.lock().unwrap();
    assert_eq!(state.public_key.as_deref(), Some(public_key));
    respond_json(
        stream,
        200,
        json!({
            "requestId": "browser-request-1",
            "challenge": state.challenge_hex,
            "expiresAt": 1_776_807_600_i64
        }),
    )
}

fn handle_browser_approve(
    stream: TcpStream,
    state: &Arc<Mutex<FakeAuthState>>,
    body: Vec<u8>,
) -> Result<(), String> {
    let body: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    assert_eq!(body["code"], "ABCD-EFGH");
    assert!(body.get("token").is_none());
    assert!(body.get("privateKeyPkcs8").is_none());

    let public_key = body
        .get("publicKey")
        .and_then(Value::as_str)
        .ok_or("approve missing publicKey")?;
    let signature_hex = body
        .get("signature")
        .and_then(Value::as_str)
        .ok_or("approve missing signature")?;
    let public_key_bytes = hex::decode(public_key).map_err(|e| e.to_string())?;
    let signature_bytes = hex::decode(signature_hex).map_err(|e| e.to_string())?;
    let challenge = {
        let state = state.lock().unwrap();
        hex::decode(&state.challenge_hex).map_err(|e| e.to_string())?
    };
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key_bytes)
        .verify(&challenge, &signature_bytes)
        .map_err(|_| "browser approval signature was invalid".to_string())?;

    let mut state = state.lock().unwrap();
    state.saw_valid_browser_signature = true;
    respond_json(
        stream,
        200,
        json!({
            "approved": true,
            "requestId": "browser-request-1",
            "user": {
                "id": "user-agent-test",
                "name": "Agent Test",
                "publicKey": public_key,
                "status": "ACTIVE",
                "isAdmin": false,
                "createdAt": "2026-04-22T00:00:00"
            }
        }),
    )
}

struct HttpRequest {
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut raw = Vec::new();
    let mut buf = [0_u8; 1024];
    let header_end;
    loop {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed before headers".to_string());
        }
        raw.extend_from_slice(&buf[..n]);
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }

    let head = String::from_utf8(raw[..header_end].to_vec()).map_err(|e| e.to_string())?;
    let request_line = head.lines().next().ok_or("missing request line")?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("missing request path")?
        .to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    let mut body = raw[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("connection closed before body".to_string());
        }
        body.extend_from_slice(&buf[..n]);
    }
    body.truncate(content_length);
    Ok(HttpRequest { path, body })
}

fn respond_json(mut stream: TcpStream, status: u16, body: Value) -> Result<(), String> {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        _ => "OK",
    };
    let body = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    let head = format!(
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|e| e.to_string())
}

struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let path = std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
