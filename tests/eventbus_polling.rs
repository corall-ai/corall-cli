use std::error::Error;
use std::io::Read;
use std::io::Write;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use serde_json::Value;
use serde_json::json;

#[test]
fn eventbus_binary_polls_and_acks_redis_without_llm_config() -> Result<(), Box<dyn Error>> {
    let Some(redis_url) = std::env::var("CORALL_TEST_REDIS_URL").ok() else {
        eprintln!("skipping eventbus integration test: CORALL_TEST_REDIS_URL is unset");
        return Ok(());
    };
    let Some(redis) = RedisEndpoint::parse(&redis_url) else {
        eprintln!("skipping eventbus integration test: unsupported Redis URL {redis_url}");
        return Ok(());
    };

    let agent_id = unique_id("agent_proc");
    let group = unique_id("group_proc");
    let stream = format!("corall:eventbus:agent:{agent_id}:stream");
    let registration = format!("corall:eventbus:agent:{agent_id}:registration");
    let event_payload = json!({
        "id": "domain-event-process",
        "type": "order.paid",
        "agentId": agent_id,
        "orderId": "order-process",
        "hook": {
            "message": "paid",
            "name": "Corall",
            "sessionKey": "hook:corall:order-process",
            "deliver": false
        }
    })
    .to_string();

    redis_command(&redis, &["DEL", &registration, &stream])?;
    redis_command(&redis, &["SET", &registration, r#"{"token":"secret"}"#])?;
    redis_command(&redis, &["XADD", &stream, "*", "payload", &event_payload])?;

    let listen = reserve_local_addr()?;
    let mut child = ChildGuard::spawn(
        env!("CARGO_BIN_EXE_corall"),
        &[
            "eventbus",
            "serve",
            "--listen",
            &listen.to_string(),
            "--redis-url",
            &redis_url,
            "--consumer-group",
            &group,
            "--default-wait-ms",
            "0",
            "--max-wait-ms",
            "100",
            "--claim-idle-ms",
            "0",
        ],
    )?;

    wait_for_health(listen, child.as_mut())?;

    let poll_path = format!("/v1/agents/{agent_id}/events?consumerId=worker-proc&wait=0&count=1");
    let poll = raw_http_json(listen, "GET", &poll_path, Some("secret"))?;
    assert_eq!(poll["status"], 200);
    let event = &poll["body"]["events"][0];
    let event_id = event["id"].as_str().expect("event id must be present");
    assert_eq!(event["eventId"], "domain-event-process");
    assert_eq!(event["hook"]["sessionKey"], "hook:corall:order-process");

    let ack_path = format!("/v1/agents/{agent_id}/events/{event_id}/ack");
    let ack = raw_http_json(listen, "POST", &ack_path, Some("secret"))?;
    assert_eq!(ack["status"], 200);
    assert_eq!(ack["body"]["acked"], 1);

    redis_command(&redis, &["DEL", &registration, &stream])?;
    child.kill();
    Ok(())
}

struct ChildGuard {
    child: Child,
}

impl ChildGuard {
    fn spawn(binary: &str, args: &[&str]) -> Result<Self, Box<dyn Error>> {
        let child = Command::new(binary)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self { child })
    }

    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        self.kill();
    }
}

struct RedisEndpoint {
    host: String,
    port: u16,
    db: usize,
}

impl RedisEndpoint {
    fn parse(raw: &str) -> Option<Self> {
        let rest = raw.strip_prefix("redis://")?;
        let (authority, path) = rest.split_once('/').unwrap_or((rest, "0"));
        if authority.contains('@') {
            return None;
        }
        let (host, port) = authority
            .rsplit_once(':')
            .map(|(host, port)| Some((host.to_owned(), port.parse().ok()?)))
            .unwrap_or_else(|| Some((authority.to_owned(), 6379)))?;
        let db = path.split('?').next().unwrap_or("0").parse().unwrap_or(0);
        Some(Self { host, port, db })
    }

    fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn reserve_local_addr() -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?)
}

fn wait_for_health(addr: SocketAddr, child: &mut Child) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(format!("eventbus process exited before health check: {status}").into());
        }

        if let Ok(response) = raw_http_json(addr, "GET", "/health", None) {
            if response["status"] == 200 {
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            return Err("eventbus process did not become healthy".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn raw_http_json(
    addr: SocketAddr,
    method: &str,
    path: &str,
    bearer: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let auth = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Connection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes())?;

    let mut raw = String::new();
    stream.read_to_string(&mut raw)?;
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or("HTTP response did not contain a header/body split")?;
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or("HTTP response did not contain a status")?
        .parse()?;
    Ok(json!({
        "status": status,
        "body": serde_json::from_str::<Value>(body)?,
    }))
}

fn redis_command(redis: &RedisEndpoint, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let mut stream = TcpStream::connect(redis.addr())?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    if redis.db != 0 {
        write_resp(&mut stream, &["SELECT", &redis.db.to_string()])?;
        read_resp(&mut stream)?;
    }
    write_resp(&mut stream, args)?;
    read_resp(&mut stream)
}

fn write_resp(stream: &mut TcpStream, args: &[&str]) -> Result<(), Box<dyn Error>> {
    write!(stream, "*{}\r\n", args.len())?;
    for arg in args {
        write!(stream, "${}\r\n{}\r\n", arg.len(), arg)?;
    }
    stream.flush()?;
    Ok(())
}

fn read_resp(stream: &mut TcpStream) -> Result<String, Box<dyn Error>> {
    let mut prefix = [0_u8; 1];
    stream.read_exact(&mut prefix)?;
    let line = read_crlf_line(stream)?;
    match prefix[0] {
        b'+' | b':' => Ok(line),
        b'-' => Err(format!("Redis returned error: {line}").into()),
        b'$' => read_bulk(stream, line.parse()?),
        other => Err(format!("unsupported Redis response prefix: {}", other as char).into()),
    }
}

fn read_bulk(stream: &mut TcpStream, len: isize) -> Result<String, Box<dyn Error>> {
    if len < 0 {
        return Ok(String::new());
    }
    let mut body = vec![0_u8; len as usize];
    stream.read_exact(&mut body)?;
    let mut crlf = [0_u8; 2];
    stream.read_exact(&mut crlf)?;
    if crlf != *b"\r\n" {
        return Err("Redis bulk response missing CRLF".into());
    }
    Ok(String::from_utf8(body)?)
}

fn read_crlf_line(stream: &mut TcpStream) -> Result<String, Box<dyn Error>> {
    let mut bytes = Vec::new();
    let mut previous = 0_u8;
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte)?;
        if previous == b'\r' && byte[0] == b'\n' {
            bytes.pop();
            return Ok(String::from_utf8(bytes)?);
        }
        bytes.push(byte[0]);
        previous = byte[0];
    }
}

fn unique_id(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{nanos}_{}", std::process::id())
}
