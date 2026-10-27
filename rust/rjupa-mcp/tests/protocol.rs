//! Offline stdio MCP contract against a loopback calculator. No live-service tests.
#![cfg(feature = "test-origin")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

struct Mcp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    id: u32,
}

impl Mcp {
    fn new(origin: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rjupa-mcp"))
            .arg("serve")
            .env_clear()
            .env("RJUPA_TEST_ORIGIN", origin)
            .env("RJUPA_TEST_NOW", "2026-10-27T12:30:00Z")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut mcp = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            id: 0,
        };
        let init = mcp.request("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}));
        assert_eq!(init["result"]["serverInfo"]["name"], "rjupa-mcp");
        assert!(
            init["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("never an empty result")
        );
        writeln!(
            mcp.input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        mcp.input.flush().unwrap();
        mcp
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        loop {
            let mut line = String::new();
            assert!(self.output.read_line(&mut line).unwrap() > 0);
            let result: Value = serde_json::from_str(&line).unwrap();
            if result["id"] == self.id {
                return result;
            }
        }
    }

    fn call(&mut self, args: Value) -> Value {
        self.request(
            "tools/call",
            json!({"name":"get_degree_days","arguments":args}),
        )["result"]
            .clone()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn args() -> Value {
    json!({"start_date":"2026-10-26","end_date":"2026-10-26","latitude":64.1,"longitude":-21.9})
}

fn body() -> Value {
    let hourly: Vec<Value> = (0..24)
        .map(|h| json!({"t":format!("2026-10-26T{h:02}:00:00Z"),"temp":50,"origin":"vedur"}))
        .collect();
    json!({"startDate":"2026-10-26","endDate":"2026-10-26","location":{"latitude":64.1,"longitude":-21.9},"station":null,"total":50,"hourly":hourly,
        "quality":{"source":"vedur","expectedHours":24,"observedHours":24,"interpolatedHours":0,"modeledHours":0}})
}

type Seen = Arc<Mutex<Vec<Value>>>;

fn upstream(status: u16, response: String) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    std::thread::spawn(move || {
        // Tests make at most two calls; nonblocking accept avoids a permanently blocked thread.
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let Ok((stream, _)) = listener.accept() else {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            assert!(head.starts_with("POST /api/daggradur HTTP/1.1\r\n"));
            assert!(head.contains("rjupa-mcp/"));
            assert!(!head.to_lowercase().contains("cookie:"));
            let len: usize = head
                .lines()
                .find_map(|s| {
                    s.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap();
            let mut input = vec![0; len];
            reader.read_exact(&mut input).unwrap();
            record
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&input).unwrap());
            let stream = reader.get_mut();
            write!(stream,"HTTP/1.1 {status} Status\r\nContent-Type: application/json\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            stream.flush().unwrap();
        }
    });
    (origin, seen)
}

#[test]
fn strict_surface_post_contract_and_acknowledgement_roundtrip() {
    let (origin, seen) = upstream(200, body().to_string());
    let mut mcp = Mcp::new(&origin);
    let tools = mcp.request("tools/list", json!({}))["result"]["tools"].clone();
    assert_eq!(tools.as_array().unwrap().len(), 1);
    assert_eq!(tools[0]["name"], "get_degree_days");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[0]["inputSchema"]["additionalProperties"], false);
    assert!(tools[0]["outputSchema"].is_object());
    let result = mcp.call(args());
    assert_ne!(result["isError"], true, "{result}");
    let out = &result["structuredContent"];
    assert_eq!(out["pending_thresholds"], json!([35, 40, 45, 50]));
    assert_eq!(out["next_threshold"], 55);
    let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, *out);
    let mut acknowledged = args();
    acknowledged["last_notified_threshold"] = json!(50);
    assert_eq!(
        mcp.call(acknowledged)["structuredContent"]["pending_thresholds"],
        json!([])
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0],
        json!({"startDate":"2026-10-26","endDate":"2026-10-26","latitude":64.1,"longitude":-21.9})
    );
    assert!(
        seen[1].get("last_notified_threshold").is_none(),
        "notification state stays host-local"
    );
}

#[test]
fn invalid_arguments_do_not_reach_upstream() {
    let (origin, seen) = upstream(200, body().to_string());
    let mut mcp = Mcp::new(&origin);
    for input in [
        json!({}),
        json!({"start_date":"2026-02-30","latitude":64,"longitude":-21}),
        json!({"start_date":"2026-10-26","latitude":64,"longitude":-21,"extra":true}),
    ] {
        let result = mcp.call(input);
        assert_eq!(result["isError"], true, "{result}");
        assert!(result.get("structuredContent").is_none());
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn failures_redirects_and_malformed_bodies_are_sanitized_and_not_retried() {
    for (status, response) in [
        (302, "SECRET-UPSTREAM-TEXT".into()),
        (429, "SECRET-UPSTREAM-TEXT".into()),
        (500, "SECRET-UPSTREAM-TEXT".into()),
        (200, "not json SECRET-UPSTREAM-TEXT".into()),
        (200, json!({"total":100}).to_string()),
        (200, "x".repeat(4 * 1024 * 1024 + 1)),
    ] {
        let (origin, seen) = upstream(status, response);
        let mut mcp = Mcp::new(&origin);
        let result = mcp.call(args());
        assert_eq!(result["isError"], true, "{result}");
        assert!(result.get("structuredContent").is_none());
        assert!(!result.to_string().contains("SECRET-UPSTREAM-TEXT"));
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
