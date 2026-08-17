//! Stage 14 acceptance: healthz, tools+schema, sandboxed read, unauth write,
//! bind 127.0.0.1, no wipe, online refuse.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::time::Duration;

use serde_json::{Value, json};

use splicecraft_core::Record;
use splicecraft_io::record_to_library_entry;
use splicecraft_persist::{self as persist, DataLayout, LibraryStore};

use crate::dispatch::dispatch;
use crate::http::{BIND_HOST, ServerConfig, host_is_loopback, start_server};
use crate::registry::{FORBIDDEN_ENDPOINT_NAMES, builtin, is_forbidden_name};
use crate::session::AgentSession;
use crate::{IMPLEMENTATION_STAGE, crate_name};

fn sandbox() -> (tempfile::TempDir, DataLayout) {
    let tmp = tempfile::tempdir().expect("tempdir");
    persist::authorize_writes_for_sandbox(tmp.path()).expect("sandbox auth");
    let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
    assert!(
        layout.root.starts_with(tmp.path()),
        "layout {} not under {}",
        layout.root.display(),
        tmp.path().display()
    );
    assert_eq!(
        layout.root.file_name().and_then(|s| s.to_str()),
        Some(persist::XDG_DATA_DIR_LEAF)
    );
    (tmp, layout)
}

fn orf_record() -> Record {
    // 40× AAA + ATG/TAA → 42 aa including start, 40 coded if we count AAA only?
    // ATG + 40×AAA + TAA = 42 codons, 41 aa excluding stop? length_aa excludes stop.
    // ATG + 40 AAA = 41 residues + stop. min_aa default 30. Good.
    let seq = format!("ATG{}TAA", "AAA".repeat(40));
    Record::new("pORF", seq, true)
}

fn seed_library(layout: &DataLayout, rec: &Record) {
    let entry = record_to_library_entry(rec).expect("gb");
    let mut store = LibraryStore::load(layout);
    store.keep(entry, None);
    store.persist(layout).expect("persist library");
}

fn session_from(layout: DataLayout) -> AgentSession {
    let mut s = AgentSession::load(layout);
    s.headless = true;
    s
}

fn http_exchange(addr: std::net::SocketAddr, request: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(request.as_bytes()).expect("write");
    stream.flush().ok();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let json: Value = serde_json::from_str(body.trim()).unwrap_or(json!({
        "raw": body,
        "head": head,
        "n": buf.len(),
    }));
    (status, json)
}

#[test]
fn crate_name_matches() {
    assert_eq!(crate_name(), "splicecraft-agent");
    assert_eq!(IMPLEMENTATION_STAGE, 14);
}

#[test]
fn bind_host_is_loopback_never_unspecified() {
    assert_eq!(BIND_HOST, Ipv4Addr::LOCALHOST);
    assert!(BIND_HOST.is_loopback());
    assert_ne!(BIND_HOST, Ipv4Addr::UNSPECIFIED);
    assert!(host_is_loopback("127.0.0.1:6701"));
    assert!(host_is_loopback("localhost"));
    assert!(host_is_loopback("[::1]:6701"));
    assert!(!host_is_loopback("evil.example:6701"));
    assert!(!host_is_loopback("0.0.0.0:6701"));
}

#[test]
fn registry_has_schemas_and_no_wipe() {
    let reg = builtin();
    let names = reg.names();
    assert!(names.contains(&"list-library"));
    assert!(names.contains(&"find-orfs"));
    assert!(names.contains(&"blast-online"));
    assert!(names.contains(&"add-current-to-library"));
    for name in &names {
        assert!(
            !is_forbidden_name(name),
            "forbidden endpoint registered: {name}"
        );
    }
    for banned in FORBIDDEN_ENDPOINT_NAMES {
        assert!(reg.get(banned).is_none(), "registered {banned}");
    }
    let doc = reg.tools_document();
    let endpoints = doc["endpoints"].as_array().expect("endpoints");
    assert!(!endpoints.is_empty());
    for ep in endpoints {
        assert!(
            ep.get("schema").is_some(),
            "missing schema on {}",
            ep["name"]
        );
        assert!(ep.get("doc").is_some());
        assert!(ep.get("doc_full").is_some());
        assert!(ep.get("write").is_some());
    }
}

#[test]
fn read_list_library_from_sandboxed_store() {
    let (_tmp, layout) = sandbox();
    seed_library(&layout, &orf_record());
    persist::revoke_thread_writes();
    let mut session = session_from(layout);
    let resp = dispatch(builtin(), &mut session, "list-library", &json!({}));
    assert_eq!(resp.status, 200, "{}", resp.body);
    let lib = resp.body["library"].as_array().expect("library");
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0]["name"], "pORF");
    assert!(lib[0]["length"].as_u64().unwrap() > 0);
}

#[test]
fn write_without_persist_authorisation_fails() {
    let (_tmp, layout) = sandbox();
    persist::revoke_thread_writes();
    assert!(!persist::writes_authorized());
    let mut session = session_from(layout);
    session.record = Some(orf_record());
    session.dirty = false;
    let resp = dispatch(
        builtin(),
        &mut session,
        "add-current-to-library",
        &json!({}),
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    assert!(
        resp.body["error"]
            .as_str()
            .unwrap_or("")
            .contains("not authorised")
    );
}

#[test]
fn dirty_guard_requires_force_in_body() {
    let (_tmp, layout) = sandbox();
    let mut session = session_from(layout);
    session.record = Some(orf_record());
    session.dirty = true;
    let blocked = dispatch(builtin(), &mut session, "save", &json!({"create": true}));
    assert_eq!(blocked.status, 409, "{}", blocked.body);
    assert_eq!(blocked.body["dirty"], true);
}

#[test]
fn find_orfs_rejects_min_length_and_reports_length_aa() {
    let (_tmp, layout) = sandbox();
    persist::revoke_thread_writes();
    let mut session = session_from(layout);
    session.record = Some(orf_record());
    let bad = dispatch(
        builtin(),
        &mut session,
        "find-orfs",
        &json!({"min_length": 90}),
    );
    assert_eq!(bad.status, 400, "{}", bad.body);
    let ok = dispatch(builtin(), &mut session, "find-orfs", &json!({"min_aa": 30}));
    assert_eq!(ok.status, 200, "{}", ok.body);
    let orfs = ok.body["orfs"].as_array().expect("orfs");
    assert!(!orfs.is_empty());
    assert!(orfs[0]["length_aa"].as_u64().unwrap() >= 30);
    assert!(orfs[0].get("nt_len").is_some());
    assert!(orfs[0].get("exceeds_one_lap").is_some());
}

#[test]
fn blast_online_refuses_when_setting_off() {
    let (_tmp, layout) = sandbox();
    persist::revoke_thread_writes();
    let mut session = session_from(layout);
    assert!(!persist::allow_online_search(&session.layout));
    let resp = dispatch(
        builtin(),
        &mut session,
        "blast-online",
        &json!({"query": "ATGCATGCATGCATGCATGC"}),
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    assert!(
        resp.body["error"]
            .as_str()
            .unwrap_or("")
            .contains("allow_online_search")
    );
}

#[test]
fn set_setting_cannot_enable_online_search() {
    let (_tmp, layout) = sandbox();
    let mut session = session_from(layout);
    let resp = dispatch(
        builtin(),
        &mut session,
        "set-setting",
        &json!({"key": "allow_online_search", "value": true}),
    );
    assert_eq!(resp.status, 403, "{}", resp.body);
    persist::revoke_thread_writes();
    assert!(!persist::allow_online_search(&session.layout));
}

#[test]
fn path_sanitiser_refuses_other_user_and_dotdot() {
    assert!(crate::sanitize_agent_path("~other/file").is_err());
    assert!(crate::sanitize_agent_path("../etc/passwd").is_err());
    assert!(crate::sanitize_agent_path("/tmp/ok.gb").is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn healthz_tools_and_bind_loopback() {
    let (_tmp, layout) = sandbox();
    persist::revoke_thread_writes();
    let session = session_from(layout);
    let handle = start_server(
        session,
        ServerConfig {
            port: 0,
            headless: true,
            write_token_file: false,
            authorize_process_writes: false,
        },
    )
    .await
    .expect("bind");
    assert!(handle.addr.ip().is_loopback());
    assert_eq!(handle.addr.ip(), Ipv4Addr::LOCALHOST);
    tokio::time::sleep(Duration::from_millis(20)).await;

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 200, "{body}");
    assert_eq!(body["status"], "ready");
    assert_eq!(body["headless"], true);
    assert!(body.get("data").is_none());

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /tools HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 200, "{body}");
    let eps = body["endpoints"].as_array().expect("endpoints");
    assert!(eps.iter().any(|e| e["name"] == "list-library"));
    assert!(eps.iter().all(|e| e.get("schema").is_some()));

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /healthz HTTP/1.1\r\nHost: evil.example:{}\r\nConnection: close\r\n\r\n",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 403, "{body}");

    handle.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_without_bearer_is_401_and_read_needs_token() {
    let (_tmp, layout) = sandbox();
    seed_library(&layout, &orf_record());
    persist::revoke_thread_writes();
    let session = session_from(layout);
    let handle = start_server(
        session,
        ServerConfig {
            port: 0,
            headless: true,
            write_token_file: false,
            authorize_process_writes: false,
        },
    )
    .await
    .expect("bind");
    tokio::time::sleep(Duration::from_millis(20)).await;

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 200, "healthz in 401 test: {body}");

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /list-library HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 401, "{body}");

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "POST /save HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",
            handle.addr.port()
        ),
    );
    assert_eq!(st, 401, "{body}");

    let (st, body) = http_exchange(
        handle.addr,
        &format!(
            "GET /list-library HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            handle.addr.port(),
            handle.token
        ),
    );
    assert_eq!(st, 200, "{body}");
    assert_eq!(body["library"][0]["name"], "pORF");

    handle.shutdown().await;
}
