#[cfg(unix)]
mod unix_only {
    use std::collections::HashMap;
    use std::collections::VecDeque;
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
    use std::process::Child;
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::thread::JoinHandle;
    use std::time::Duration;
    use std::time::Instant;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use serde_json::Value;
    use serde_json::json;

    #[test]
    fn nohup_eventbus_poll_exec_uses_saved_credentials_and_stays_alive()
    -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-nohup")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let agent_id = unique_id("agent_exec");
        let polling_token = "polling-secret";
        let eventbus = FakeEventbusServer::start(
            &agent_id,
            polling_token,
            vec![json!({
                "id": "stream-exec-1",
                "eventId": "order.paid:exec-1",
                "type": "order.paid",
                "hook": {
                    "message": "exec event",
                    "name": "Corall",
                    "sessionKey": "hook:corall:exec-1",
                    "deliver": false
                }
            })],
        )?;
        write_credentials(&home, "provider", &agent_id, polling_token)?;

        let worker_script = temp.path().join("worker.py");
        let payload_path = temp.path().join("worker-payload.json");
        let env_path = temp.path().join("worker-env.json");
        let stdout_path = temp.path().join("poller.stdout.log");
        let stderr_path = temp.path().join("poller.stderr.log");
        write_exec_worker(&worker_script)?;

        let output = Command::new("sh")
            .arg("-c")
            .arg(
                "nohup \"$BIN\" --profile provider eventbus poll \
                 --base-url \"$BASE_URL\" \
                 --exec python3 \
                 --exec-arg \"$SCRIPT\" \
                 --exec-arg \"$PAYLOAD\" \
                 --exec-arg \"$ENVFILE\" \
                 --wait-ms 5 \
                 --request-timeout-ms 1000 \
                 --ack-timeout-ms 1000 \
                 --idle-delay-ms 50 \
                 >\"$STDOUT\" 2>\"$STDERR\" & echo $!",
            )
            .env("BIN", env!("CARGO_BIN_EXE_corall"))
            .env("BASE_URL", eventbus.base_url())
            .env("SCRIPT", path_str(&worker_script)?)
            .env("PAYLOAD", path_str(&payload_path)?)
            .env("ENVFILE", path_str(&env_path)?)
            .env("STDOUT", path_str(&stdout_path)?)
            .env("STDERR", path_str(&stderr_path)?)
            .env("HOME", &home)
            .output()?;

        assert!(
            output.status.success(),
            "nohup launch failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let pid = String::from_utf8(output.stdout)?
            .trim()
            .parse::<u32>()
            .map_err(|err| format!("failed to parse nohup pid: {err}"))?;

        wait_until(Duration::from_secs(5), || {
            payload_path.exists() && env_path.exists() && eventbus.ack_count("stream-exec-1") == 1
        })?;

        let payload: Value = serde_json::from_str(&fs::read_to_string(&payload_path)?)?;
        assert_eq!(payload["id"], "stream-exec-1");
        assert_eq!(payload["dedupeId"], "order.paid:exec-1");
        assert_eq!(payload["hook"]["sessionKey"], "hook:corall:exec-1");

        let env_json: Value = serde_json::from_str(&fs::read_to_string(&env_path)?)?;
        assert_eq!(env_json["CORALL_AGENT_ID"], agent_id);
        assert_eq!(env_json["CORALL_EVENT_ID"], "stream-exec-1");
        assert_eq!(env_json["CORALL_EVENT_DEDUPE_ID"], "order.paid:exec-1");
        assert_eq!(env_json["CORALL_HOOK_NAME"], "Corall");
        assert_eq!(env_json["CORALL_HOOK_MESSAGE"], "exec event");
        assert_eq!(env_json["CORALL_HOOK_SESSION_KEY"], "hook:corall:exec-1");
        assert_eq!(env_json["CORALL_HOOK_DELIVER"], "false");

        let poll_consumers = eventbus.poll_consumers();
        assert!(
            poll_consumers
                .iter()
                .any(|consumer| consumer.starts_with(&format!("corall-cli-poll:{agent_id}:"))),
            "expected default consumer id, got {poll_consumers:?}"
        );

        assert!(
            process_is_alive(pid)?,
            "poller died early\nstdout:\n{}\nstderr:\n{}",
            fs::read_to_string(&stdout_path).unwrap_or_default(),
            fs::read_to_string(&stderr_path).unwrap_or_default()
        );

        kill_pid(pid)?;
        Ok(())
    }

    #[test]
    fn eventbus_poll_hook_mode_delivers_and_acks() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-hook")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let agent_id = unique_id("agent_hook");
        let polling_token = "hook-polling-secret";
        let hook_token = "local-hook-token";
        let eventbus = FakeEventbusServer::start(
            &agent_id,
            polling_token,
            vec![json!({
                "id": "stream-hook-1",
                "eventId": "order.paid:hook-1",
                "type": "order.paid",
                "hook": {
                    "message": "hook event",
                    "name": "Corall",
                    "sessionKey": "hook:corall:hook-1",
                    "deliver": false
                }
            })],
        )?;
        let hook_server = FakeHookServer::start(Some(hook_token))?;

        let stdout_path = temp.path().join("poller.stdout.log");
        let stderr_path = temp.path().join("poller.stderr.log");
        let mut child = ChildGuard::spawn(
            env!("CARGO_BIN_EXE_corall"),
            &[
                "--profile",
                "provider",
                "eventbus",
                "poll",
                "--base-url",
                &eventbus.base_url(),
                "--agent-id",
                &agent_id,
                "--webhook-token",
                polling_token,
                "--hook-url",
                &hook_server.url(),
                "--hook-token",
                hook_token,
                "--wait-ms",
                "5",
                "--request-timeout-ms",
                "1000",
                "--ack-timeout-ms",
                "1000",
                "--idle-delay-ms",
                "50",
            ],
            &home,
            &stdout_path,
            &stderr_path,
        )?;

        wait_until(Duration::from_secs(5), || {
            hook_server.request_count() == 1 && eventbus.ack_count("stream-hook-1") == 1
        })?;

        let request = hook_server
            .requests()
            .pop()
            .ok_or("expected hook request")?;
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer local-hook-token")
        );
        assert_eq!(
            request.body,
            json!({
                "message": "hook event",
                "name": "Corall",
                "sessionKey": "hook:corall:hook-1",
                "deliver": false
            })
        );

        assert!(
            child.is_running()?,
            "poller died early\nstdout:\n{}\nstderr:\n{}",
            fs::read_to_string(&stdout_path).unwrap_or_default(),
            fs::read_to_string(&stderr_path).unwrap_or_default()
        );

        child.kill();
        Ok(())
    }

    #[test]
    fn eventbus_poll_rejects_missing_delivery_target() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-missing-target")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let output = run_corall(
            &home,
            &[
                "eventbus",
                "poll",
                "--base-url",
                "http://127.0.0.1:8787",
                "--agent-id",
                "agent-missing-target",
                "--webhook-token",
                "polling-token",
            ],
        )?;

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("missing local delivery target"));
        Ok(())
    }

    #[test]
    fn eventbus_poll_rejects_conflicting_delivery_target() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-conflicting-target")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let output = run_corall(
            &home,
            &[
                "eventbus",
                "poll",
                "--base-url",
                "http://127.0.0.1:8787",
                "--agent-id",
                "agent-conflicting-target",
                "--webhook-token",
                "polling-token",
                "--hook-url",
                "http://127.0.0.1:9000/hooks/agent",
                "--exec",
                "true",
            ],
        )?;

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("choose exactly one local delivery target"));
        Ok(())
    }

    #[test]
    fn eventbus_poll_requires_saved_or_explicit_token() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-missing-token")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let output = run_corall(
            &home,
            &[
                "eventbus",
                "poll",
                "--base-url",
                "http://127.0.0.1:8787",
                "--agent-id",
                "agent-missing-token",
                "--exec",
                "true",
            ],
        )?;

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("polling token is required"));
        Ok(())
    }

    #[test]
    fn eventbus_poll_invalid_json_retries_without_ack() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-invalid-json")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let agent_id = unique_id("agent_invalid_json");
        let eventbus = FakeEventbusServer::start_with_poll_responses(
            &agent_id,
            "polling-secret",
            vec![
                PollResponse::raw(200, "application/json", "not-json"),
                PollResponse::raw(200, "application/json", "still-not-json"),
            ],
        )?;

        let stdout_path = temp.path().join("poller.stdout.log");
        let stderr_path = temp.path().join("poller.stderr.log");
        let mut child = ChildGuard::spawn(
            env!("CARGO_BIN_EXE_corall"),
            &[
                "eventbus",
                "poll",
                "--base-url",
                &eventbus.base_url(),
                "--agent-id",
                &agent_id,
                "--webhook-token",
                "polling-secret",
                "--exec",
                "true",
                "--wait-ms",
                "5",
                "--request-timeout-ms",
                "200",
                "--ack-timeout-ms",
                "200",
                "--idle-delay-ms",
                "20",
                "--error-backoff-ms",
                "20",
                "--max-error-backoff-ms",
                "20",
            ],
            &home,
            &stdout_path,
            &stderr_path,
        )?;

        wait_until(Duration::from_secs(5), || {
            eventbus.poll_count() >= 2
                && fs::read_to_string(&stderr_path)
                    .map(|stderr| stderr.contains("eventbus poll response was not valid JSON"))
                    .unwrap_or(false)
        })?;

        assert_eq!(eventbus.total_acks(), 0);
        assert!(
            child.is_running()?,
            "poller exited on invalid poll JSON\nstdout:\n{}\nstderr:\n{}",
            fs::read_to_string(&stdout_path).unwrap_or_default(),
            fs::read_to_string(&stderr_path).unwrap_or_default()
        );

        child.kill();
        Ok(())
    }

    #[test]
    fn eventbus_poll_hook_non_2xx_exits_without_ack() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-hook-failure")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let agent_id = unique_id("agent_hook_fail");
        let polling_token = "hook-fail-token";
        let eventbus = FakeEventbusServer::start(
            &agent_id,
            polling_token,
            vec![json!({
                "id": "stream-hook-fail-1",
                "eventId": "order.paid:hook-fail-1",
                "hook": {
                    "message": "hook failure",
                    "name": "Corall",
                    "sessionKey": "hook:corall:hook-fail-1",
                    "deliver": false
                }
            })],
        )?;
        let hook_server = FakeHookServer::start(Some("expected-hook-token"))?;

        let output = run_corall(
            &home,
            &[
                "eventbus",
                "poll",
                "--base-url",
                &eventbus.base_url(),
                "--agent-id",
                &agent_id,
                "--webhook-token",
                polling_token,
                "--hook-url",
                &hook_server.url(),
                "--hook-token",
                "wrong-hook-token",
                "--wait-ms",
                "5",
                "--request-timeout-ms",
                "1000",
                "--ack-timeout-ms",
                "1000",
            ],
        )?;

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("local hook returned HTTP 401"));
        assert_eq!(hook_server.attempt_count(), 1);
        assert_eq!(eventbus.ack_count("stream-hook-fail-1"), 0);
        Ok(())
    }

    #[test]
    fn eventbus_poll_exec_failure_does_not_ack() -> Result<(), Box<dyn Error>> {
        let temp = TempDir::new("corall-eventbus-poll-exec-failure")?;
        let home = temp.path().join("home");
        fs::create_dir_all(&home)?;

        let agent_id = unique_id("agent_exec_fail");
        let polling_token = "exec-fail-token";
        let eventbus = FakeEventbusServer::start(
            &agent_id,
            polling_token,
            vec![json!({
                "id": "stream-exec-fail-1",
                "eventId": "order.paid:exec-fail-1",
                "hook": {
                    "message": "exec failure",
                    "name": "Corall",
                    "sessionKey": "hook:corall:exec-fail-1",
                    "deliver": false
                }
            })],
        )?;

        let output = run_corall(
            &home,
            &[
                "eventbus",
                "poll",
                "--base-url",
                &eventbus.base_url(),
                "--agent-id",
                &agent_id,
                "--webhook-token",
                polling_token,
                "--exec",
                "sh",
                "--exec-arg=-c",
                "--exec-arg",
                "cat >/dev/null; exit 17",
                "--wait-ms",
                "5",
                "--request-timeout-ms",
                "1000",
                "--ack-timeout-ms",
                "1000",
            ],
        )?;

        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            stderr.contains("local command `sh` exited with status"),
            "unexpected stderr: {stderr}"
        );
        assert_eq!(eventbus.ack_count("stream-exec-fail-1"), 0);
        Ok(())
    }

    struct ChildGuard {
        child: Child,
    }

    impl ChildGuard {
        fn spawn(
            binary: &str,
            args: &[&str],
            home: &Path,
            stdout_path: &Path,
            stderr_path: &Path,
        ) -> Result<Self, Box<dyn Error>> {
            let stdout = fs::File::create(stdout_path)?;
            let stderr = fs::File::create(stderr_path)?;
            let child = Command::new(binary)
                .args(args)
                .env("HOME", home)
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()?;
            Ok(Self { child })
        }

        fn is_running(&mut self) -> Result<bool, Box<dyn Error>> {
            Ok(self.child.try_wait()?.is_none())
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

    #[derive(Clone)]
    struct HookRequest {
        authorization: Option<String>,
        body: Value,
    }

    struct FakeHookServer {
        addr: SocketAddr,
        shutdown: Arc<AtomicBool>,
        attempts: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<HookRequest>>>,
        thread: Option<JoinHandle<()>>,
    }

    impl FakeHookServer {
        fn start(expected_token: Option<&str>) -> Result<Self, Box<dyn Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let addr = listener.local_addr()?;
            let shutdown = Arc::new(AtomicBool::new(false));
            let attempts = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let shutdown_flag = shutdown.clone();
            let attempts_ref = attempts.clone();
            let requests_ref = requests.clone();
            let expected_token = expected_token.map(str::to_owned);

            let thread = thread::spawn(move || {
                while !shutdown_flag.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            if let Ok(request) = read_http_request(&mut stream) {
                                attempts_ref.fetch_add(1, Ordering::SeqCst);
                                let auth = request.headers.get("authorization").cloned();
                                let status = if let Some(expected_token) = expected_token.as_deref()
                                {
                                    if auth.as_deref() == Some(&format!("Bearer {expected_token}"))
                                    {
                                        200
                                    } else {
                                        401
                                    }
                                } else {
                                    200
                                };
                                if status == 200 {
                                    if let Ok(body) = serde_json::from_slice::<Value>(&request.body)
                                    {
                                        requests_ref.lock().unwrap().push(HookRequest {
                                            authorization: auth,
                                            body,
                                        });
                                    }
                                }
                                let _ = write_json_response(
                                    &mut stream,
                                    status,
                                    &json!({ "ok": status == 200 }),
                                );
                            }
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
                attempts,
                requests,
                thread: Some(thread),
            })
        }

        fn url(&self) -> String {
            format!("http://{}/hooks/agent", self.addr)
        }

        fn request_count(&self) -> usize {
            self.requests.lock().unwrap().len()
        }

        fn attempt_count(&self) -> usize {
            self.attempts.load(Ordering::SeqCst)
        }

        fn requests(&self) -> Vec<HookRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for FakeHookServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::SeqCst);
            let _ =
                TcpStream::connect(self.addr).and_then(|stream| stream.shutdown(Shutdown::Both));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    struct FakeEventbusServer {
        addr: SocketAddr,
        shutdown: Arc<AtomicBool>,
        state: Arc<Mutex<EventbusState>>,
        thread: Option<JoinHandle<()>>,
    }

    #[derive(Clone)]
    enum PollResponse {
        Events(Vec<Value>),
        Raw {
            status: u16,
            content_type: String,
            body: String,
        },
    }

    impl PollResponse {
        fn raw(status: u16, content_type: &str, body: &str) -> Self {
            Self::Raw {
                status,
                content_type: content_type.to_string(),
                body: body.to_string(),
            }
        }
    }

    struct EventbusState {
        agent_id: String,
        polling_token: String,
        poll_responses: VecDeque<PollResponse>,
        poll_count: usize,
        poll_consumers: Vec<String>,
        ack_counts: HashMap<String, usize>,
    }

    impl FakeEventbusServer {
        fn start(
            agent_id: &str,
            polling_token: &str,
            events: Vec<Value>,
        ) -> Result<Self, Box<dyn Error>> {
            Self::start_with_poll_responses(
                agent_id,
                polling_token,
                vec![PollResponse::Events(events)],
            )
        }

        fn start_with_poll_responses(
            agent_id: &str,
            polling_token: &str,
            poll_responses: Vec<PollResponse>,
        ) -> Result<Self, Box<dyn Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            listener.set_nonblocking(true)?;
            let addr = listener.local_addr()?;
            let shutdown = Arc::new(AtomicBool::new(false));
            let state = Arc::new(Mutex::new(EventbusState {
                agent_id: agent_id.to_string(),
                polling_token: polling_token.to_string(),
                poll_responses: poll_responses.into(),
                poll_count: 0,
                poll_consumers: Vec::new(),
                ack_counts: HashMap::new(),
            }));
            let shutdown_flag = shutdown.clone();
            let state_ref = state.clone();

            let thread = thread::spawn(move || {
                while !shutdown_flag.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let response = match read_http_request(&mut stream) {
                                Ok(request) => handle_eventbus_request(request, &state_ref),
                                Err(err) => {
                                    json_response(500, &json!({ "error": err.to_string() }))
                                }
                            };
                            let _ = write_http_response(&mut stream, &response);
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
                state,
                thread: Some(thread),
            })
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.addr)
        }

        fn ack_count(&self, event_id: &str) -> usize {
            self.state
                .lock()
                .unwrap()
                .ack_counts
                .get(event_id)
                .copied()
                .unwrap_or(0)
        }

        fn total_acks(&self) -> usize {
            self.state
                .lock()
                .unwrap()
                .ack_counts
                .values()
                .copied()
                .sum()
        }

        fn poll_count(&self) -> usize {
            self.state.lock().unwrap().poll_count
        }

        fn poll_consumers(&self) -> Vec<String> {
            self.state.lock().unwrap().poll_consumers.clone()
        }
    }

    impl Drop for FakeEventbusServer {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::SeqCst);
            let _ =
                TcpStream::connect(self.addr).and_then(|stream| stream.shutdown(Shutdown::Both));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    struct HttpRequest {
        method: String,
        path: String,
        query: Option<String>,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    fn handle_eventbus_request(
        request: HttpRequest,
        state: &Arc<Mutex<EventbusState>>,
    ) -> HttpResponse {
        if request.method == "GET" && request.path == "/v1/eventbus/health" {
            return json_response(200, &json!({ "ok": true, "redis": "ok" }));
        }

        let auth = request.headers.get("authorization").cloned();
        let expected_bearer = {
            let state = state.lock().unwrap();
            format!("Bearer {}", state.polling_token)
        };
        if auth.as_deref() != Some(expected_bearer.as_str()) {
            return json_response(401, &json!({ "error": "unauthorized" }));
        }

        if request.method == "GET" && request.path.ends_with("/events") {
            let mut state = state.lock().unwrap();
            if !request
                .path
                .contains(&format!("/v1/agents/{}/events", state.agent_id))
            {
                return json_response(404, &json!({ "error": "not found" }));
            }
            let consumer_id = query_param(request.query.as_deref(), "consumerId")
                .unwrap_or_else(|| "missing-consumer".to_string());
            state.poll_consumers.push(consumer_id.clone());
            state.poll_count += 1;
            if let Some(response) = state.poll_responses.pop_front() {
                return match response {
                    PollResponse::Events(events) => json_response(
                        200,
                        &json!({
                            "consumerId": consumer_id,
                            "events": events,
                        }),
                    ),
                    PollResponse::Raw {
                        status,
                        content_type,
                        body,
                    } => HttpResponse {
                        status,
                        content_type,
                        body: body.into_bytes(),
                    },
                };
            }
            return json_response(
                200,
                &json!({
                    "consumerId": consumer_id,
                    "events": [],
                }),
            );
        }

        if request.method == "POST"
            && request.path.contains("/events/")
            && request.path.ends_with("/ack")
        {
            let mut state = state.lock().unwrap();
            let prefix = format!("/v1/agents/{}/events/", state.agent_id);
            if let Some(rest) = request.path.strip_prefix(&prefix) {
                if let Some(event_id) = rest.strip_suffix("/ack") {
                    let counter = state.ack_counts.entry(event_id.to_string()).or_insert(0);
                    *counter += 1;
                    let acked = if *counter == 1 { 1 } else { 0 };
                    return json_response(
                        200,
                        &json!({
                            "ok": true,
                            "acked": acked,
                            "eventId": event_id,
                        }),
                    );
                }
            }
        }

        json_response(404, &json!({ "error": "not found" }))
    }

    fn write_credentials(
        home: &Path,
        profile: &str,
        agent_id: &str,
        polling_token: &str,
    ) -> Result<(), Box<dyn Error>> {
        let credentials_dir = home.join(".corall/credentials");
        fs::create_dir_all(&credentials_dir)?;
        fs::write(
            credentials_dir.join(format!("{profile}.json")),
            serde_json::to_string_pretty(&json!({
                "site": "http://corall.test",
                "user": {
                    "id": "user-test",
                    "publicKey": "a".repeat(64)
                },
                "privateKeyPkcs8": "b".repeat(64),
                "agentId": agent_id,
                "pollingToken": polling_token
            }))?,
        )?;
        Ok(())
    }

    fn write_exec_worker(path: &Path) -> Result<(), Box<dyn Error>> {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;

        fs::write(
            path,
            r#"#!/usr/bin/env python3
import json
import os
import pathlib
import sys

payload_path = pathlib.Path(sys.argv[1])
env_path = pathlib.Path(sys.argv[2])
payload_path.write_bytes(sys.stdin.buffer.read())
env_path.write_text(json.dumps({
    "CORALL_AGENT_ID": os.environ.get("CORALL_AGENT_ID"),
    "CORALL_EVENT_ID": os.environ.get("CORALL_EVENT_ID"),
    "CORALL_EVENT_DEDUPE_ID": os.environ.get("CORALL_EVENT_DEDUPE_ID"),
    "CORALL_HOOK_NAME": os.environ.get("CORALL_HOOK_NAME"),
    "CORALL_HOOK_MESSAGE": os.environ.get("CORALL_HOOK_MESSAGE"),
    "CORALL_HOOK_SESSION_KEY": os.environ.get("CORALL_HOOK_SESSION_KEY"),
    "CORALL_HOOK_DELIVER": os.environ.get("CORALL_HOOK_DELIVER"),
}))
"#,
        )?;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
        Ok(())
    }

    fn process_is_alive(pid: u32) -> Result<bool, Box<dyn Error>> {
        Ok(Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()?
            .success())
    }

    fn run_corall(home: &Path, args: &[&str]) -> Result<std::process::Output, Box<dyn Error>> {
        Ok(Command::new(env!("CARGO_BIN_EXE_corall"))
            .args(args)
            .env("HOME", home)
            .output()?)
    }

    fn kill_pid(pid: u32) -> Result<(), Box<dyn Error>> {
        let _ = Command::new("kill").args([pid.to_string()]).status()?;
        Ok(())
    }

    fn wait_until<F>(timeout: Duration, mut predicate: F) -> Result<(), Box<dyn Error>>
    where
        F: FnMut() -> bool,
    {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err("timed out waiting for condition".into())
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
        let raw_path = parts.next().ok_or("request missing path")?;
        let (path, query) = raw_path
            .split_once('?')
            .map(|(path, query)| (path.to_string(), Some(query.to_string())))
            .unwrap_or_else(|| (raw_path.to_string(), None));

        let mut headers = HashMap::new();
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
            query,
            headers,
            body,
        })
    }

    struct HttpResponse {
        status: u16,
        content_type: String,
        body: Vec<u8>,
    }

    fn write_json_response(
        stream: &mut TcpStream,
        status: u16,
        body: &Value,
    ) -> Result<(), Box<dyn Error>> {
        let body = serde_json::to_vec(body)?;
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
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

    fn write_http_response(
        stream: &mut TcpStream,
        response: &HttpResponse,
    ) -> Result<(), Box<dyn Error>> {
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.status,
            reason_phrase(response.status),
            response.content_type,
            response.body.len()
        )?;
        stream.write_all(&response.body)?;
        stream.flush()?;
        Ok(())
    }

    fn json_response(status: u16, body: &Value) -> HttpResponse {
        HttpResponse {
            status,
            content_type: "application/json".to_string(),
            body: serde_json::to_vec(body).expect("json response should serialize"),
        }
    }

    fn reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            401 => "Unauthorized",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Error",
        }
    }

    fn query_param(raw_query: Option<&str>, key: &str) -> Option<String> {
        raw_query.and_then(|query| {
            query.split('&').find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name == key).then(|| percent_decode(value))
            })
        })
    }

    fn percent_decode(raw: &str) -> String {
        let bytes = raw.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'%' if index + 2 < bytes.len() => {
                    let hex = &raw[index + 1..index + 3];
                    if let Ok(value) = u8::from_str_radix(hex, 16) {
                        out.push(value);
                        index += 3;
                        continue;
                    }
                    out.push(bytes[index]);
                }
                b'+' => out.push(b' '),
                byte => out.push(byte),
            }
            index += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn path_str(path: &Path) -> Result<&str, Box<dyn Error>> {
        path.to_str()
            .ok_or_else(|| format!("path is not valid utf-8: {}", path.display()).into())
    }

    fn unique_id(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{prefix}-{}-{nanos}", std::process::id())
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
}
