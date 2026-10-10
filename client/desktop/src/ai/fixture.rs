// SPDX-License-Identifier: GPL-3.0-only
//! The client tests' support: the one-shot HTTP fixture (a local `127.0.0.1`
//! server that serves scripted raw responses in order, one per accepted
//! connection, and captures every request's raw bytes, the autodetect
//! fetch-test pattern extended to keep what it was served so the wire tests
//! can pin it byte-relevantly), the client and registry builders that point
//! the providers at it, and the ledger reader the row assertions go through.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use astra_ai::{AiStore, TierTable};

use super::LlmClient;

/// One captured request: the request line, the headers, and the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapturedRequest {
    /// The request line, for example `POST /chat/completions HTTP/1.1`.
    pub(crate) request_line: String,
    /// The headers in arrival order, names lowercased.
    pub(crate) headers: Vec<(String, String)>,
    /// The request body as UTF-8.
    pub(crate) body: String,
}

impl CapturedRequest {
    /// The first value of a header, by lowercased name.
    pub(crate) fn header(&self, name: &str) -> &str {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    }
}

/// The running fixture: its base URL and the requests it captured.
pub(crate) struct Fixture {
    /// The `http://127.0.0.1:port` base the providers' endpoint bases point
    /// at; the two protocols land on different paths of the same server.
    pub(crate) base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Fixture {
    /// The parsed capture of request `index`.
    pub(crate) fn request(&self, index: usize) -> CapturedRequest {
        let raw = self.requests.lock().expect("fixture capture poisoned");
        parse_request(&raw[index])
    }

    /// How many requests the fixture captured.
    pub(crate) fn request_count(&self) -> usize {
        self.requests
            .lock()
            .expect("fixture capture poisoned")
            .len()
    }
}

/// A canned raw HTTP response: status line, extra headers, and a body with a
/// correct `Content-Length`.
pub(crate) fn response(status_line: &str, extra_headers: &[(&str, &str)], body: &str) -> String {
    let mut head = format!(
        "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    format!("{head}\r\n{body}")
}

/// Binds the fixture server and returns it with its base URL. Each scripted
/// response is served to one accepted connection, in order; the server stops
/// once the script runs out.
pub(crate) fn serve(responses: Vec<String>) -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the fixture server binds");
    let base = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("the fixture server has an address")
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&requests);
    thread::spawn(move || {
        for reply in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let Some(raw) = read_request(&mut stream) else {
                continue;
            };
            captured.lock().expect("fixture capture poisoned").push(raw);
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    Fixture { base, requests }
}

/// Reads one whole HTTP request: the headers through the blank line, then the
/// `Content-Length` worth of body bytes.
fn read_request(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
        let Some(sep) = buf.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&buf[..sep]).to_string();
        let length = content_length(&headers);
        if buf.len() >= sep + 4 + length {
            break;
        }
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// The request's `Content-Length`, zero when absent or unparsable.
fn content_length(headers: &str) -> usize {
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

/// Splits one raw request into its request line, headers, and body.
fn parse_request(raw: &str) -> CapturedRequest {
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    CapturedRequest {
        request_line,
        headers,
        body: body.to_string(),
    }
}

/// Distinct temp-file names for the per-test ledger stores.
static STORE_COUNTER: AtomicU32 = AtomicU32::new(0);

/// One llm_calls row as the tests read it back. `latency_ms` and the row id
/// stay out: neither is deterministic.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RecordedRow {
    pub(crate) correlation: String,
    pub(crate) use_case: String,
    pub(crate) tier: String,
    pub(crate) model: String,
    pub(crate) prompt_tokens: i64,
    pub(crate) completion_tokens: i64,
    pub(crate) cost_micros: Option<i64>,
    pub(crate) outcome: String,
}

/// The expected row shape for a whole-ledger `assert_eq`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn row(
    correlation: &str,
    use_case: &str,
    tier: &str,
    model: &str,
    tokens: (i64, i64),
    cost: Option<i64>,
    outcome: &str,
) -> RecordedRow {
    RecordedRow {
        correlation: correlation.to_string(),
        use_case: use_case.to_string(),
        tier: tier.to_string(),
        model: model.to_string(),
        prompt_tokens: tokens.0,
        completion_tokens: tokens.1,
        cost_micros: cost,
        outcome: outcome.to_string(),
    }
}

/// One test's ledger: the file-backed store the client records into, its
/// file removed when the test ends. Declare it before the client so the
/// client drops first and its store handle is gone before the removal.
pub(crate) struct Ledger {
    store: Option<Arc<AiStore>>,
    path: PathBuf,
}

impl Ledger {
    /// Opens the file-backed store at its own temp file. The name carries
    /// the process id beside the counter, so a run whose cleanup could not
    /// remove its file (a panicking live test whose spawned driver still
    /// holds the store open on Windows) can never be opened by the next
    /// run: the stale rows stay with the dead process's name.
    pub(crate) fn open(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "astra-llm-client-tests-{}-{}-{tag}.sqlite",
            std::process::id(),
            STORE_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let store = Arc::new(AiStore::open(&path).expect("the test ledger opens"));
        Self {
            store: Some(store),
            path,
        }
    }

    /// The store handle the client records through.
    pub(crate) fn store(&self) -> Arc<AiStore> {
        Arc::clone(
            self.store
                .as_ref()
                .expect("the ledger store lives until drop"),
        )
    }

    /// The ledger rows in append order, read through the test's own
    /// connection.
    pub(crate) fn rows(&self) -> Vec<RecordedRow> {
        let conn = rusqlite::Connection::open(&self.path).expect("the ledger opens for reading");
        conn.busy_timeout(Duration::from_secs(5))
            .expect("the read connection waits on a busy ledger");
        let mut statement = conn
            .prepare(
                "SELECT correlation_id, use_case, tier, model, prompt_tokens,
                        completion_tokens, cost_micros, outcome
                 FROM llm_calls ORDER BY id",
            )
            .expect("the ledger query prepares");
        let recorded = statement
            .query_map([], |row| {
                Ok(RecordedRow {
                    correlation: row.get(0)?,
                    use_case: row.get(1)?,
                    tier: row.get(2)?,
                    model: row.get(3)?,
                    prompt_tokens: row.get(4)?,
                    completion_tokens: row.get(5)?,
                    cost_micros: row.get(6)?,
                    outcome: row.get(7)?,
                })
            })
            .expect("the ledger query runs")
            .collect::<Result<Vec<_>, _>>()
            .expect("the ledger rows read back");
        recorded
    }
}

impl Drop for Ledger {
    fn drop(&mut self) {
        // The store handle closes before the removal so the file is not in
        // use on platforms that lock open files.
        self.store.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The tier registry from `(ASTRA_LLM_* variable, value)` pairs.
pub(crate) fn tier_table(lists: &[(&str, &str)]) -> TierTable {
    TierTable::from_lookup(|name| {
        lists
            .iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| (*value).to_string())
    })
    .expect("the test tier table parses")
}

/// The env the wire tests inject: both providers' endpoint bases at the
/// fixture, plus the given `(provider, key)` pairs.
pub(crate) fn env_at(base: &str, keys: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut env = vec![
        ("ASTRA_LLM_BASE_URL_OPENAI".to_string(), base.to_string()),
        ("ASTRA_LLM_BASE_URL_ANTHROPIC".to_string(), base.to_string()),
    ];
    for (provider, key) in keys {
        env.push((
            format!("ASTRA_LLM_API_KEY_{}", provider.to_ascii_uppercase()),
            (*key).to_string(),
        ));
    }
    env
}

/// A client over the tier lists and env, recording into `store`. The HTTP
/// client is built over the engine's TLS policy the way the host will build
/// the real one; the fixture is plain HTTP on the loopback, but the client
/// build installs the crypto provider reqwest needs.
pub(crate) fn llm_client(
    store: &Arc<AiStore>,
    tiers: &[(&str, &str)],
    env: &[(String, String)],
) -> LlmClient {
    let vars: HashMap<String, String> = env.iter().cloned().collect();
    LlmClient::new_with_lookup(http(), tier_table(tiers), Arc::clone(store), move |name| {
        vars.get(name).cloned()
    })
}

/// The tests' HTTP client, over the same TLS policy autodetection's fetcher
/// builds on.
pub(crate) fn http() -> reqwest::Client {
    let tls = engine_tls::client_config(&engine_tls::TlsPolicy::bundled_and_system())
        .expect("the test TLS policy builds");
    tls.reqwest_builder()
        .build()
        .expect("the test HTTP client builds")
}

/// The runtime the async tests block on, the same one the host builds.
pub(crate) fn runtime() -> crate::runtime::AppRuntime {
    crate::runtime::build_runtime().expect("the test runtime builds")
}
