use std::error::Error;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use serde_json::json;

#[test]
fn reviews_create_uses_explicit_rating_without_penalty_payload() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("corall-reviews-rating")?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;

    let server = FakeReviewsServer::start()?;
    write_credentials(&home, "employer", &server.base_url(), "cached-review-token")?;

    let output = run_corall(
        &home,
        &[
            "--profile",
            "employer",
            "reviews",
            "create",
            "ord-rating-1",
            "--rating",
            "4.9",
            "--comment",
            "Exact user score.",
            "--reviewer-kind",
            "system",
            "--requirement-miss",
            "3",
            "--correctness-defect",
            "2",
        ],
    )?;

    assert!(output.status.success(), "reviews create failed: {output:?}");
    let request = server.requests().pop().ok_or("expected review request")?;
    assert_eq!(
        request.authorization.as_deref(),
        Some("Bearer cached-review-token")
    );
    assert_eq!(request.body["orderId"], "ord-rating-1");
    assert_eq!(request.body["rating"], 4.9);
    assert_eq!(request.body["reviewerKind"], "system");
    assert_eq!(request.body["comment"], "Exact user score.");
    assert!(request.body.get("penalties").is_none());
    Ok(())
}

#[test]
fn reviews_create_uses_penalty_payload_when_rating_is_omitted() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("corall-reviews-penalties")?;
    let home = temp.path().join("home");
    fs::create_dir_all(&home)?;

    let server = FakeReviewsServer::start()?;
    write_credentials(&home, "employer", &server.base_url(), "cached-review-token")?;

    let output = run_corall(
        &home,
        &[
            "--profile",
            "employer",
            "reviews",
            "create",
            "ord-penalty-1",
            "--comment",
            "Needs rework.",
            "--reviewer-kind",
            "employer-agent",
            "--correctness-defect",
            "1",
            "--rework-burden",
            "2",
        ],
    )?;

    assert!(output.status.success(), "reviews create failed: {output:?}");
    let request = server.requests().pop().ok_or("expected review request")?;
    assert_eq!(request.body["orderId"], "ord-penalty-1");
    assert_eq!(request.body["reviewerKind"], "employer_agent");
    assert_eq!(request.body["comment"], "Needs rework.");
    assert!(request.body.get("rating").is_none());
    assert_eq!(
        request.body["penalties"],
        json!({
            "requirementMiss": 0,
            "correctnessDefect": 1,
            "reworkBurden": 2,
            "timelinessMiss": 0,
            "communicationFriction": 0,
            "safetyRisk": 0,
        })
    );
    Ok(())
}

#[test]
fn reviews_create_rejects_out_of_range_rating() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("corall-reviews-bad-rating")?;
    let output = run_corall(
        temp.path(),
        &["reviews", "create", "ord-invalid-rating", "--rating", "5.1"],
    )?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("rating must be between 0.0 and 5.0"));
    Ok(())
}

#[test]
fn reviews_create_rejects_out_of_range_penalty() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("corall-reviews-bad-penalty")?;
    let output = run_corall(
        temp.path(),
        &[
            "reviews",
            "create",
            "ord-invalid-penalty",
            "--timeliness-miss",
            "4",
        ],
    )?;

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("penalty values must be between 0 and 3"));
    Ok(())
}

fn run_corall(home: &Path, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_corall"))
        .args(args)
        .env("HOME", home)
        .output()?)
}

#[derive(Clone)]
struct ReviewRequest {
    authorization: Option<String>,
    body: Value,
}

struct FakeReviewsServer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<ReviewRequest>>>,
    thread: Option<JoinHandle<()>>,
}

impl FakeReviewsServer {
    fn start() -> Result<Self, Box<dyn Error>> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let shutdown_flag = shutdown.clone();
        let requests_ref = requests.clone();

        let thread = thread::spawn(move || {
            while !shutdown_flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let response = match read_http_request(&mut stream) {
                            Ok(request) => handle_request(request, &requests_ref),
                            Err(err) => (500, json!({ "error": err.to_string() })),
                        };
                        let _ = write_json_response(&mut stream, response.0, &response.1);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            addr,
            shutdown,
            requests,
            thread: Some(thread),
        })
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<ReviewRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for FakeReviewsServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr).and_then(|stream| stream.shutdown(Shutdown::Both));
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: std::collections::HashMap<String, String>,
    body: Vec<u8>,
}

fn handle_request(request: HttpRequest, requests: &Arc<Mutex<Vec<ReviewRequest>>>) -> (u16, Value) {
    if request.method == "POST" && request.path == "/api/reviews" {
        let body = serde_json::from_slice::<Value>(&request.body)
            .unwrap_or_else(|_| json!({ "invalid": true }));
        requests.lock().unwrap().push(ReviewRequest {
            authorization: request.headers.get("authorization").cloned(),
            body,
        });
        return (
            201,
            json!({
                "ok": true,
                "reviewId": "review-1",
            }),
        );
    }

    (404, json!({ "error": "not found" }))
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, Box<dyn Error>> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }

    let header_end = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("request did not contain header terminator")?;
    let head = String::from_utf8(buffer[..header_end].to_vec())?;
    let mut body = buffer[(header_end + 4)..].to_vec();

    let mut lines = head.lines();
    let request_line = lines.next().ok_or("request missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("request missing method")?.to_string();
    let path = parts.next().ok_or("request missing path")?.to_string();

    let mut headers = std::collections::HashMap::new();
    let mut content_length = 0_usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let key = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if key == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(key, value);
        }
    }

    while body.len() < content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_json_response(
    stream: &mut TcpStream,
    status: u16,
    body: &Value,
) -> Result<(), Box<dyn Error>> {
    let body = serde_json::to_vec(body)?;
    let reason = match status {
        200 => "OK",
        201 => "Created",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        reason,
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()?;
    Ok(())
}

fn write_credentials(
    home: &Path,
    profile: &str,
    site: &str,
    token: &str,
) -> Result<(), Box<dyn Error>> {
    let credentials_dir = home.join(".corall/credentials");
    fs::create_dir_all(&credentials_dir)?;
    fs::write(
        credentials_dir.join(format!("{profile}.json")),
        serde_json::to_string_pretty(&json!({
            "site": site,
            "user": {
                "id": "user-review-test",
                "publicKey": "a".repeat(64)
            },
            "privateKeyPkcs8": "b".repeat(64),
            "token": token,
            "tokenExpiresAt": 4_102_444_800_i64
        }))?,
    )?;
    Ok(())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let path = std::env::temp_dir().join(unique_id(prefix));
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

fn unique_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{}-{nanos}", std::process::id())
}
