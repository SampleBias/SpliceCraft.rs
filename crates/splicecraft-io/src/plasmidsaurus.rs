//! Plasmidsaurus zip grouping + REST client (offline by default).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use splicecraft_core::Record;
use splicecraft_persist::LibraryEntry;
use splicecraft_util::{natural_sort_key, sanitize_plasmid_name};

use crate::error::IoError;
use crate::zip::{
    ZIP_MAX_MEMBERS, extract_gbk_member, extract_zip_member, is_safe_zip_member_name,
    list_gbk_members_in_zip, normalize_zip_member, summarize_perbase_tsv,
};
use crate::{gb_text_to_record, record_to_library_entry};

/// Official API origin. Default tests never open a socket.
pub const PLASMIDSAURUS_API_HOST: &str = "app.plasmidsaurus.com";
/// HTTPS origin.
pub const PLASMIDSAURUS_API_URL: &str = "https://app.plasmidsaurus.com";
/// Server listing ceiling (no pagination).
pub const PLASMIDSAURUS_ITEMS_LIMIT: i32 = 1000;
/// Observed account rate limit.
pub const PLASMIDSAURUS_RATE_PER_MIN: u32 = 10;
/// Listing cache TTL.
pub const PLASMIDSAURUS_CACHE_TTL: Duration = Duration::from_secs(120);
/// Download cap (a touch above the zip parser's 500 MB).
pub const PLASMIDSAURUS_DOWNLOAD_MAX_BYTES: u64 = 600 * 1024 * 1024;
const SUMMARY_MAX: usize = 4 * 1024;
const SAMPLE_NAME_MAX: usize = 256;

static ORDERS_CACHE: Mutex<Option<OrdersCache>> = Mutex::new(None);

struct OrdersCache {
    key: u64,
    at: Instant,
    items: Vec<PsItem>,
    token: String,
}

/// One `/api/items` row (subset we actually use).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PsItem {
    /// Six-character item code.
    pub code: String,
    /// Vendor status (`complete`, `canceled`, …).
    pub status: String,
    /// ISO date when results landed; shipping labels are typically null.
    pub done_date: String,
    /// Product name (`plasmidsaurus`, `ups_shipping_label`, …).
    pub product_name: String,
    /// User-facing order label.
    pub order_name: String,
}

impl PsItem {
    /// Heuristic: complete + dated + not a known non-sequencing product.
    #[must_use]
    pub fn has_results(&self) -> bool {
        plasmidsaurus_item_has_results(self)
    }
}

/// One sample inside a results zip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PsSample {
    /// Canonical base name.
    pub base: String,
    /// Display name.
    pub name: String,
    /// `.gbk` member, if any.
    pub gbk: Option<String>,
    /// `.fasta` member.
    pub fasta: Option<String>,
    /// Summary member.
    pub summary: Option<String>,
    /// Inline summary body (≤ 4 KB).
    pub summary_text: String,
    /// Per-base TSV member.
    pub perbase: Option<String>,
    /// Coverage stats.
    pub perbase_coverage: BTreeMap<String, f64>,
    /// AB1 member names.
    pub ab1_files: Vec<String>,
}

/// Structured zip summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PsZip {
    /// Majority-vote run id.
    pub run_id: String,
    /// Per-sample rows.
    pub samples: Vec<PsSample>,
    /// Top-level extras.
    pub run_files: Vec<(String, u64, String)>,
    /// Count of regular files seen.
    pub total_files: usize,
    /// Sum of declared sizes.
    pub total_size: u64,
}

const CATEGORY_SUFFIXES: &[(&str, &str)] = &[
    ("_genbank-files", "gbk"),
    ("_fasta-files", "fasta"),
    ("_summary-files", "summary"),
    ("_per-base-data", "perbase"),
    ("_histograms", "histogram"),
    ("_coverage-plots", "coverage_plot"),
    ("_interactive-map", "interactive_map"),
    ("_ab1-files", "ab1"),
];

/// Group a Plasmidsaurus results zip by sample.
pub fn parse_plasmidsaurus_zip(path: &Path) -> Result<PsZip, IoError> {
    let file =
        std::fs::File::open(path).map_err(|e| IoError::zip(format!("could not open zip: {e}")))?;
    let meta = file.metadata()?;
    if meta.len() > crate::zip::ZIP_MAX_BYTES {
        return Err(IoError::zip(format!(
            "zip too large ({} bytes; cap {})",
            meta.len(),
            crate::zip::ZIP_MAX_BYTES
        )));
    }
    let mut zf =
        zip::ZipArchive::new(file).map_err(|e| IoError::zip(format!("could not open zip: {e}")))?;
    let mut samples: BTreeMap<String, PsSample> = BTreeMap::new();
    let mut run_files = Vec::new();
    let mut total_files = 0usize;
    let mut total_size = 0u64;
    let mut prefix_votes: BTreeMap<String, i32> = BTreeMap::new();
    let n = zf.len().min(ZIP_MAX_MEMBERS);
    for i in 0..n {
        let item = zf
            .by_index(i)
            .map_err(|e| IoError::zip(format!("could not read zip: {e}")))?;
        if item.is_dir() {
            continue;
        }
        let raw_name = normalize_zip_member(item.name());
        if !is_safe_zip_member_name(&raw_name) {
            continue;
        }
        total_files += 1;
        total_size += item.size();
        let parts: Vec<&str> = raw_name.split('/').collect();
        if parts.len() < 2 {
            let base = parts[0];
            let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
            if let Some((pre, _)) = stem.split_once('_') {
                *prefix_votes.entry(pre.to_owned()).or_insert(0) += 1;
            }
            run_files.push((base.to_owned(), item.size(), "run".into()));
            continue;
        }
        let folder = parts[0];
        let mut matched: Option<&str> = None;
        for (suffix, field) in CATEGORY_SUFFIXES {
            if let Some(prefix) = folder.strip_suffix(suffix) {
                matched = Some(*field);
                if !prefix.is_empty() {
                    *prefix_votes.entry(prefix.to_owned()).or_insert(0) += 1;
                }
                break;
            }
        }
        let matched = match matched {
            Some(m) => m,
            None => {
                let leaf = parts.last().copied().unwrap_or("");
                let low = leaf.to_ascii_lowercase();
                if low.ends_with(".gbk") || low.ends_with(".gb") || low.ends_with(".genbank") {
                    "gbk"
                } else {
                    run_files.push((raw_name.clone(), item.size(), folder.into()));
                    continue;
                }
            }
        };
        let leaf = parts.last().copied().unwrap_or("");
        let mut base = leaf.to_owned();
        for _ in 0..4 {
            let Some((stem, ext)) = base.rsplit_once('.') else {
                break;
            };
            if stem.is_empty() {
                break;
            }
            let el = ext.to_ascii_lowercase();
            if matches!(
                el.as_str(),
                "gbk"
                    | "gb"
                    | "genbank"
                    | "fasta"
                    | "fa"
                    | "tsv"
                    | "txt"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "html"
                    | "ab1"
            ) {
                base = stem.to_owned();
                continue;
            }
            if matched == "ab1" && el.contains("-of-") {
                base = stem.to_owned();
                continue;
            }
            break;
        }
        let entry = samples.entry(base.clone()).or_insert_with(|| PsSample {
            base: base.clone(),
            name: base.clone(),
            ..PsSample::default()
        });
        match matched {
            "ab1" => entry.ab1_files.push(raw_name),
            "gbk" => entry.gbk = Some(raw_name),
            "fasta" => entry.fasta = Some(raw_name),
            "summary" => entry.summary = Some(raw_name),
            "perbase" => entry.perbase = Some(raw_name),
            _ => {}
        }
    }
    let names: Vec<String> = samples.values().filter_map(|s| s.summary.clone()).collect();
    for sm in names {
        if let Ok(raw) = extract_zip_member(path, &sm)
            && raw.len() <= SUMMARY_MAX
            && let Some(sample) = samples
                .values_mut()
                .find(|s| s.summary.as_deref() == Some(sm.as_str()))
        {
            sample.summary_text = String::from_utf8_lossy(&raw).into_owned();
        }
    }
    let pbs: Vec<String> = samples.values().filter_map(|s| s.perbase.clone()).collect();
    for pb in pbs {
        if let Ok(raw) = extract_zip_member(path, &pb)
            && let Some(sample) = samples
                .values_mut()
                .find(|s| s.perbase.as_deref() == Some(pb.as_str()))
        {
            let text = String::from_utf8_lossy(&raw);
            sample.perbase_coverage = summarize_perbase_tsv(&text);
        }
    }
    let run_id = prefix_votes
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(k, _)| k)
        .unwrap_or_default();
    let mut sample_list: Vec<PsSample> = samples.into_values().collect();
    sample_list.sort_by_key(|a| natural_sort_key(&a.name));
    Ok(PsZip {
        run_id,
        samples: sample_list,
        run_files,
        total_files,
        total_size,
    })
}

/// Headless import: each `.gbk` becomes a library row tagged `plasmidsaurus:…`.
pub fn plasmidsaurus_zip_to_entries(
    path: &Path,
    run_id: &str,
) -> Result<(Vec<LibraryEntry>, Vec<String>), IoError> {
    let members = list_gbk_members_in_zip(path)?;
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for m in members {
        match extract_gbk_member(path, &m.name) {
            Ok(gb_text) => match gb_text_to_record(&gb_text) {
                Ok(rec) => {
                    let sample = Path::new(&m.name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("plasmid");
                    let display = sanitize_plasmid_name(sample, "plasmid", SAMPLE_NAME_MAX);
                    let mut entry = record_to_library_entry(&rec)?;
                    entry.name = display.clone();
                    entry.source = if run_id.is_empty() {
                        format!("plasmidsaurus:{sample}")
                    } else {
                        format!("plasmidsaurus:{run_id}:{sample}")
                    };
                    entries.push(entry);
                }
                Err(e) => warnings.push(format!("{}: {e}", m.name)),
            },
            Err(e) => warnings.push(format!("{}: {e}", m.name)),
        }
    }
    Ok((entries, warnings))
}

/// Load the first `.gbk` member as a record (Sequencing overlay).
pub fn first_gbk_record_from_zip(path: &Path) -> Result<Record, IoError> {
    let members = list_gbk_members_in_zip(path)?;
    let m = members
        .first()
        .ok_or_else(|| IoError::zip("zip contains no GenBank members"))?;
    let gb = extract_gbk_member(path, &m.name)?;
    gb_text_to_record(&gb)
}

/// `^[A-Z0-9]{6}$` after trim + upper.
#[must_use]
pub fn sanitize_plasmidsaurus_item_code(code: &str) -> Option<String> {
    let c = code.trim().to_ascii_uppercase();
    if c.len() == 6 && c.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Some(c)
    } else {
        None
    }
}

/// Env-first credentials, then optional settings fallback.
#[must_use]
pub fn plasmidsaurus_credentials(
    settings_id: Option<&str>,
    settings_secret: Option<&str>,
) -> (Option<String>, Option<String>) {
    let mut cid = std::env::var("PLASMIDSAURUS_CLIENT_ID")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    let mut sec = std::env::var("PLASMIDSAURUS_CLIENT_SECRET")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    if cid.is_none() {
        cid = settings_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    if sec.is_none() {
        sec = settings_secret
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    (cid, sec)
}

/// Shape diagnosis after a 400/401/403. Never includes credential values.
#[must_use]
pub fn plasmidsaurus_credential_hint(client_id: &str, client_secret: &str) -> String {
    let cid = client_id.trim();
    let sec = client_secret.trim();
    if cid.contains('@') {
        return " That Client ID looks like an email address — that's your Plasmidsaurus website login, not the API credentials. The Client ID and Secret are hex strings from your account's API page (Settings → Plasmidsaurus).".into();
    }
    let mut off = Vec::new();
    for (label, val, want) in [("Client ID", cid, 32usize), ("Client Secret", sec, 64)] {
        if val.is_empty() {
            continue;
        }
        if !val.bytes().all(|b| b.is_ascii_hexdigit()) {
            off.push(format!(
                "the {label} isn't hex (expected {want} hex characters)"
            ));
        } else if val.len() != want {
            off.push(format!(
                "the {label} is {} characters, expected {want}",
                val.len()
            ));
        }
    }
    if off.is_empty() {
        String::new()
    } else {
        format!(" Note: {}.", off.join("; "))
    }
}

/// Complete + dated + not a shipping label.
#[must_use]
pub fn plasmidsaurus_item_has_results(item: &PsItem) -> bool {
    if !item.status.trim().eq_ignore_ascii_case("complete") {
        return false;
    }
    if item.done_date.trim().is_empty() {
        return false;
    }
    !item
        .product_name
        .trim()
        .eq_ignore_ascii_case("ups_shipping_label")
}

/// Host allowlist for the API origin (no DNS).
pub fn assert_plasmidsaurus_host(host: &str) -> Result<(), IoError> {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if h == PLASMIDSAURUS_API_HOST {
        Ok(())
    } else {
        Err(IoError::HostNotAllowlisted(host.to_owned()))
    }
}

/// One mocked or real HTTP exchange.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    /// `GET` / `POST`.
    pub method: String,
    /// Full URL.
    pub url: String,
    /// Request body (token POST).
    pub body: Vec<u8>,
}

/// Transport response.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    /// Status code.
    pub status: u16,
    /// Body bytes.
    pub body: Vec<u8>,
}

/// Injected by tests; the default never opens a socket.
pub trait HttpTransport {
    /// Perform one request.
    fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError>;
}

/// Default: accession-style fail-closed egress.
#[derive(Clone, Copy, Debug, Default)]
pub struct OfflineTransport;

impl HttpTransport for OfflineTransport {
    fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError> {
        let _ = req;
        Err(IoError::NetworkDisabled)
    }
}

fn url_host(url: &str) -> Result<String, IoError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| IoError::plasmidsaurus("Plasmidsaurus URL must be https"))?;
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    Ok(host.to_owned())
}

fn check_api_url(url: &str) -> Result<(), IoError> {
    assert_plasmidsaurus_host(&url_host(url)?)
}

/// Redeem a bearer token (`POST /oauth/token`).
pub fn plasmidsaurus_oauth_token(
    transport: &impl HttpTransport,
    client_id: &str,
    client_secret: &str,
) -> Result<String, IoError> {
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(IoError::plasmidsaurus(
            "missing Plasmidsaurus client_id / client_secret",
        ));
    }
    let url = format!("{PLASMIDSAURUS_API_URL}/oauth/token");
    check_api_url(&url)?;
    let body = b"grant_type=client_credentials&scope=item:read".to_vec();
    let resp = transport.execute(&HttpRequest {
        method: "POST".into(),
        url,
        body,
    })?;
    match resp.status {
        200 => {}
        429 => {
            return Err(IoError::plasmidsaurus(
                "Plasmidsaurus rate limit reached (10 requests per minute) — wait about a minute and try again.",
            ));
        }
        400 | 401 | 403 => {
            return Err(IoError::plasmidsaurus(format!(
                "Plasmidsaurus rejected the API credentials (HTTP {}). Check PLASMIDSAURUS_CLIENT_ID / PLASMIDSAURUS_CLIENT_SECRET (or the Settings values).{}",
                resp.status,
                plasmidsaurus_credential_hint(client_id, client_secret)
            )));
        }
        other => {
            return Err(IoError::plasmidsaurus(format!(
                "Plasmidsaurus token request failed: HTTP {other}"
            )));
        }
    }
    let v: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| IoError::plasmidsaurus("Plasmidsaurus token response wasn't valid JSON"))?;
    v.get("access_token")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| IoError::plasmidsaurus("Plasmidsaurus token response had no access_token"))
}

/// `GET /api/items?limit=…`.
pub fn plasmidsaurus_list_items(
    transport: &impl HttpTransport,
    token: &str,
    limit: i32,
) -> Result<Vec<PsItem>, IoError> {
    let limit = limit.clamp(1, PLASMIDSAURUS_ITEMS_LIMIT);
    let url = format!("{PLASMIDSAURUS_API_URL}/api/items?limit={limit}");
    check_api_url(&url)?;
    let resp = transport.execute(&HttpRequest {
        method: "GET".into(),
        url,
        body: token.as_bytes().to_vec(),
    })?;
    match resp.status {
        200 => {}
        429 => {
            return Err(IoError::plasmidsaurus(
                "Plasmidsaurus rate limit reached (10 requests per minute) — wait about a minute and try again.",
            ));
        }
        other => {
            return Err(IoError::plasmidsaurus(format!(
                "Plasmidsaurus API GET failed: HTTP {other}"
            )));
        }
    }
    let v: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| IoError::plasmidsaurus("Plasmidsaurus API response wasn't valid JSON"))?;
    let arr = match v {
        Value::Array(a) => a,
        _ => return Ok(Vec::new()),
    };
    Ok(arr
        .into_iter()
        .filter_map(|item| {
            let code = item.get("code").and_then(Value::as_str).unwrap_or("");
            let code = sanitize_plasmidsaurus_item_code(code)?;
            Some(PsItem {
                code,
                status: item
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                done_date: item
                    .get("done_date")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                product_name: item
                    .get("product_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                order_name: item
                    .get("order_name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            })
        })
        .collect())
}

/// Cached listing (2 minutes, keyed on a hash of the credential pair).
pub fn plasmidsaurus_orders_cached(
    transport: &impl HttpTransport,
    client_id: &str,
    client_secret: &str,
    force: bool,
) -> Result<(Vec<PsItem>, String), IoError> {
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        client_id.hash(&mut h);
        client_secret.hash(&mut h);
        h.finish()
    };
    if !force {
        let guard = ORDERS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = guard.as_ref()
            && c.key == key
            && c.at.elapsed() < PLASMIDSAURUS_CACHE_TTL
        {
            return Ok((c.items.clone(), c.token.clone()));
        }
    }
    let token = plasmidsaurus_oauth_token(transport, client_id, client_secret)?;
    let items = plasmidsaurus_list_items(transport, &token, PLASMIDSAURUS_ITEMS_LIMIT)?;
    {
        let mut guard = ORDERS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(OrdersCache {
            key,
            at: Instant::now(),
            items: items.clone(),
            token: token.clone(),
        });
    }
    Ok((items, token))
}

/// Drop the process listing cache (tests).
pub fn clear_plasmidsaurus_orders_cache() {
    let mut guard = ORDERS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;
}

/// Default client: never opens a socket.
pub fn plasmidsaurus_list_items_offline() -> Result<Vec<PsItem>, IoError> {
    let (cid, sec) = plasmidsaurus_credentials(None, None);
    let (cid, sec) = match (cid, sec) {
        (Some(c), Some(s)) => (c, s),
        _ => {
            return Err(IoError::plasmidsaurus(
                "set PLASMIDSAURUS_CLIENT_ID / PLASMIDSAURUS_CLIENT_SECRET",
            ));
        }
    };
    plasmidsaurus_orders_cached(&OfflineTransport, &cid, &sec, false).map(|(i, _)| i)
}
