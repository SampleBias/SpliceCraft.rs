//! NCBI BLAST URL-API + EBI HMMER web client. Offline by default; cancel stops polls.
//!
//! Hosts are allowlisted, HTTPS-only. Sequence content is never logged. The
//! `allow_online_search` setting (or [`OnlineSearchPolicy::enabled`]) must be
//! ticked — a disarmed call errors without touching the transport.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::error::IoError;
use crate::net::{assert_public_ip, ip_is_non_public};
use crate::plasmidsaurus::{HttpRequest, HttpResponse, HttpTransport};

/// NCBI BLAST URL-API.
pub const NCBI_BLAST_URL: &str = "https://blast.ncbi.nlm.nih.gov/Blast.cgi";
/// EBI HMMER hmmscan submit.
pub const HMMER_WEB_SUBMIT_URL: &str = "https://www.ebi.ac.uk/Tools/hmmer/api/v1/search/hmmscan";
/// EBI HMMER result prefix (job id appended).
pub const HMMER_WEB_RESULT_URL: &str = "https://www.ebi.ac.uk/Tools/hmmer/api/v1/result/";
/// Default poll cadence (NCBI politeness floor).
pub const ONLINE_POLL_INTERVAL: Duration = Duration::from_secs(10);
/// Overall wait ceiling.
pub const ONLINE_MAX_WAIT: Duration = Duration::from_secs(300);
/// BLAST XML cap.
pub const NCBI_BLAST_MAX_RESPONSE_BYTES: usize = 48 * 1024 * 1024;
/// HMMER JSON cap.
pub const HMMER_WEB_MAX_RESPONSE_BYTES: usize = 24 * 1024 * 1024;

/// Hosts the online search client may contact.
pub const SEARCH_ALLOWLIST: &[&str] = &[
    "blast.ncbi.nlm.nih.gov",
    "www.ebi.ac.uk",
    "ftp.ebi.ac.uk",
    "ftp.ncbi.nlm.nih.gov",
];

const HMMER_DONE: &[&str] = &["SUCCESS", "DONE", "COMPLETE", "COMPLETED", "FINISHED", "OK"];
const HMMER_ERROR: &[&str] = &["ERROR", "ERR", "FAILURE", "FAILED", "FAIL"];

/// Cooperative cancel. Polled between HTTP calls and during poll sleeps.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Fresh unset token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal cancel. Subsequent [`Self::is_cancelled`] is true.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether cancel was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// Sleep up to `timeout`, returning `true` if cancel fired first.
    #[must_use]
    pub fn wait_timeout(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        let slice = Duration::from_millis(5);
        while start.elapsed() < timeout {
            if self.is_cancelled() {
                return true;
            }
            let remain = timeout.saturating_sub(start.elapsed());
            std::thread::sleep(remain.min(slice));
        }
        self.is_cancelled()
    }
}

/// Policy for one online search. Tests inject a mock transport.
pub struct OnlineSearchPolicy<'a, T: HttpTransport> {
    /// Must be true (`allow_online_search`). Off → [`IoError::OnlineDisabled`].
    pub enabled: bool,
    /// Injected HTTP. Default tests use a mock; never the real network.
    pub transport: &'a T,
    /// Cancel that actually stops the poll loop.
    pub cancel: &'a CancellationToken,
    /// Poll sleep (10 s in production; milliseconds in tests).
    pub poll_interval: Duration,
    /// Overall wait.
    pub max_wait: Duration,
}

/// One NCBI BLAST HSP row (first HSP per hit).
#[derive(Clone, Debug, PartialEq)]
pub struct OnlineBlastHit {
    /// Accession.
    pub accession: String,
    /// Hit definition (control bytes stripped).
    pub description: String,
    /// Percent identity, if both identity and align-len parsed.
    pub identity_pct: Option<f64>,
    /// Alignment length.
    pub aln_len: Option<i64>,
    /// E-value.
    pub evalue: Option<f64>,
    /// Bit score.
    pub bit_score: Option<f64>,
    /// Query from.
    pub q_start: Option<i64>,
    /// Query to.
    pub q_end: Option<i64>,
    /// Subject from.
    pub s_start: Option<i64>,
    /// Subject to.
    pub s_end: Option<i64>,
}

/// One EBI HMMER / Pfam hit.
#[derive(Clone, Debug, PartialEq)]
pub struct OnlineHmmHit {
    /// Accession (PF00069, …).
    pub acc: String,
    /// Family name.
    pub name: String,
    /// Description.
    pub description: String,
    /// E-value.
    pub evalue: Option<f64>,
    /// Bit score.
    pub bit_score: Option<f64>,
    /// Domain count.
    pub n_dom: i64,
}

/// Host must be on [`SEARCH_ALLOWLIST`].
pub fn assert_search_host(host: &str) -> Result<(), IoError> {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if SEARCH_ALLOWLIST.iter().any(|ok| h == *ok) {
        Ok(())
    } else {
        Err(IoError::HostNotAllowlisted(host.to_owned()))
    }
}

/// HTTPS URL on an allowlisted host. Refuses userinfo and http.
pub fn assert_search_url(url: &str) -> Result<String, IoError> {
    let host = https_host(url)?;
    assert_search_host(&host)?;
    Ok(host)
}

pub(crate) fn https_host(url: &str) -> Result<String, IoError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| IoError::online("online search URL must be https"))?;
    if rest.contains('@') {
        return Err(IoError::online("refusing URL userinfo"));
    }
    let host = rest
        .split('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        return Err(IoError::online("online search URL missing host"));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        assert_public_ip(ip)?;
    }
    Ok(host.to_ascii_lowercase())
}

/// HTTPS download URL: allowlisted **or** a non-private hostname (custom HMM-DB).
pub fn assert_download_url(url: &str) -> Result<String, IoError> {
    let host = https_host(url)?;
    if SEARCH_ALLOWLIST.iter().any(|ok| host == *ok) {
        return Ok(host);
    }
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return Err(IoError::HostNotAllowlisted(host));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip_is_non_public(ip) {
            return Err(IoError::NonPublicAddress(ip.to_string()));
        }
        return Ok(host);
    }
    if host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && host.contains('.')
    {
        Ok(host)
    } else {
        Err(IoError::HostNotAllowlisted(host))
    }
}

fn demo_blocks_network() -> bool {
    std::env::var("SPLICECRAFT_DEMO")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            !v.is_empty() && v != "0" && v != "false" && v != "off"
        })
        .unwrap_or(false)
}

fn refuse_if_disarmed(enabled: bool) -> Result<(), IoError> {
    if demo_blocks_network() {
        return Err(IoError::NetworkDisabled);
    }
    if !enabled {
        return Err(IoError::OnlineDisabled);
    }
    Ok(())
}

fn clean_query(raw: &str, nucleotide: bool) -> String {
    let mut seq = String::new();
    for line in raw.lines() {
        if line.starts_with('>') {
            continue;
        }
        seq.push_str(line);
    }
    if seq.is_empty() {
        seq = raw.to_owned();
    }
    let mut seq: String = seq.chars().filter(|c| !c.is_whitespace()).collect();
    seq.make_ascii_uppercase();
    if nucleotide {
        seq = seq.replace('U', "T");
    }
    seq
}

fn program_is_nucleotide(program: &str) -> bool {
    matches!(program, "blastn" | "blastx" | "tblastx")
}

fn max_query_len(program: &str) -> usize {
    match program {
        "blastn" | "blastx" | "tblastx" => 1_000_000,
        _ => 100_000,
    }
}

fn ncbi_db_for(program: &str) -> &'static str {
    if matches!(program, "blastp" | "blastx") {
        "nr"
    } else {
        "nt"
    }
}

fn form_encode(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut out = String::new();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&percent_encode(k));
        out.push('=');
        out.push_str(&percent_encode(v));
    }
    out.into_bytes()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn strip_controls(s: &str) -> String {
    s.chars().filter(|c| *c >= ' ' && *c != '\u{7f}').collect()
}

fn http(
    transport: &impl HttpTransport,
    method: &str,
    url: &str,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
    max_bytes: usize,
) -> Result<HttpResponse, IoError> {
    assert_search_url(url)?;
    let resp = transport.execute(&HttpRequest {
        method: method.into(),
        url: url.into(),
        body,
        headers,
    })?;
    if resp.body.len() > max_bytes {
        return Err(IoError::online(format!(
            "Response exceeded the {} MB cap — refusing to load it.",
            max_bytes / (1024 * 1024)
        )));
    }
    Ok(resp)
}

fn delete_rid(transport: &impl HttpTransport, rid: &str) {
    let body = form_encode(&[("CMD", "Delete"), ("RID", rid)]);
    let _ = http(
        transport,
        "POST",
        NCBI_BLAST_URL,
        body,
        Vec::new(),
        NCBI_BLAST_MAX_RESPONSE_BYTES,
    );
}

/// NCBI BLAST via CMD=Put → poll SearchInfo → CMD=Get XML.
pub fn ncbi_blast_online(
    query: &str,
    program: &str,
    database: Option<&str>,
    max_hits: usize,
    policy: &OnlineSearchPolicy<'_, impl HttpTransport>,
) -> Result<Vec<OnlineBlastHit>, IoError> {
    refuse_if_disarmed(policy.enabled)?;
    if policy.cancel.is_cancelled() {
        return Err(IoError::Cancelled);
    }
    let seq = clean_query(query, program_is_nucleotide(program));
    if seq.is_empty() {
        return Err(IoError::online("empty BLAST query"));
    }
    if seq.len() > max_query_len(program) {
        return Err(IoError::online(
            "BLAST query exceeds the program length cap",
        ));
    }
    let db = database.unwrap_or_else(|| ncbi_db_for(program));
    let put = form_encode(&[
        ("CMD", "Put"),
        ("PROGRAM", program),
        ("DATABASE", db),
        ("QUERY", &seq),
        ("HITLIST_SIZE", &max_hits.to_string()),
        ("FORMAT_TYPE", "XML"),
    ]);
    let put_text = String::from_utf8_lossy(
        &http(
            policy.transport,
            "POST",
            NCBI_BLAST_URL,
            put,
            Vec::new(),
            NCBI_BLAST_MAX_RESPONSE_BYTES,
        )?
        .body,
    )
    .into_owned();
    let rid = put_text
        .lines()
        .find_map(|ln| {
            let s = ln.trim();
            s.strip_prefix("RID = ").map(str::trim).map(str::to_owned)
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IoError::online("NCBI did not return a job id (RID)"))?;

    let ready = (|| -> Result<(), IoError> {
        let mut elapsed = Duration::ZERO;
        let mut consecutive_fail = 0u32;
        while elapsed < policy.max_wait {
            if policy.cancel.wait_timeout(policy.poll_interval) {
                return Err(IoError::Cancelled);
            }
            elapsed += policy.poll_interval;
            let status_body = form_encode(&[
                ("CMD", "Get"),
                ("FORMAT_OBJECT", "SearchInfo"),
                ("RID", &rid),
            ]);
            let status_text = match http(
                policy.transport,
                "POST",
                NCBI_BLAST_URL,
                status_body,
                Vec::new(),
                NCBI_BLAST_MAX_RESPONSE_BYTES,
            ) {
                Ok(r) => String::from_utf8_lossy(&r.body).into_owned(),
                Err(e) => {
                    consecutive_fail += 1;
                    if consecutive_fail >= 5 {
                        return Err(IoError::online(format!(
                            "NCBI BLAST polling failed {consecutive_fail}× in a row: {e}"
                        )));
                    }
                    continue;
                }
            };
            consecutive_fail = 0;
            if status_text.contains("Status=WAITING") {
                continue;
            }
            if status_text.contains("Status=FAILED") {
                return Err(IoError::online("NCBI BLAST job failed"));
            }
            if status_text.contains("Status=UNKNOWN") {
                return Err(IoError::online("NCBI BLAST job expired or is unknown"));
            }
            if status_text.contains("Status=READY") {
                return Ok(());
            }
        }
        Err(IoError::online(format!(
            "NCBI BLAST timed out after {}s",
            policy.max_wait.as_secs()
        )))
    })();
    if let Err(e) = ready {
        delete_rid(policy.transport, &rid);
        return Err(e);
    }

    let get = form_encode(&[("CMD", "Get"), ("FORMAT_TYPE", "XML"), ("RID", &rid)]);
    let xml = String::from_utf8_lossy(
        &http(
            policy.transport,
            "POST",
            NCBI_BLAST_URL,
            get,
            Vec::new(),
            NCBI_BLAST_MAX_RESPONSE_BYTES,
        )?
        .body,
    )
    .into_owned();
    Ok(ncbi_blast_parse_xml(&xml, max_hits))
}

/// Parse NCBI BLAST XML (DTD allowed; no entity expansion).
#[must_use]
pub fn ncbi_blast_parse_xml(xml_text: &str, max_hits: usize) -> Vec<OnlineBlastHit> {
    let mut hits = Vec::new();
    let mut rest = xml_text;
    while let Some(start) = rest.find("<Hit>") {
        let after = &rest[start + 5..];
        let Some(end) = after.find("</Hit>") else {
            break;
        };
        let block = &after[..end];
        rest = &after[end + 6..];
        let hsp = xml_text_in(block, "Hsp").unwrap_or("");
        if hsp.is_empty() {
            continue;
        }
        let identity = xml_int(hsp, "Hsp_identity");
        let align_len = xml_int(hsp, "Hsp_align-len");
        let pct = match (identity, align_len) {
            (Some(id), Some(len)) if len > 0 => {
                Some(((id as f64) / (len as f64) * 1000.0).round() / 10.0)
            }
            _ => None,
        };
        let acc = strip_controls(
            &xml_text_owned(block, "Hit_accession")
                .or_else(|| xml_text_owned(block, "Hit_id"))
                .unwrap_or_else(|| "?".into()),
        );
        hits.push(OnlineBlastHit {
            accession: acc,
            description: strip_controls(&xml_text_owned(block, "Hit_def").unwrap_or_default()),
            identity_pct: pct,
            aln_len: align_len,
            evalue: xml_float(hsp, "Hsp_evalue"),
            bit_score: xml_float(hsp, "Hsp_bit-score"),
            q_start: xml_int(hsp, "Hsp_query-from"),
            q_end: xml_int(hsp, "Hsp_query-to"),
            s_start: xml_int(hsp, "Hsp_hit-from"),
            s_end: xml_int(hsp, "Hsp_hit-to"),
        });
        if hits.len() >= max_hits {
            break;
        }
    }
    hits
}

fn xml_text_in<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find(&close)? + s;
    Some(block[s..e].trim())
}

fn xml_text_owned(block: &str, tag: &str) -> Option<String> {
    xml_text_in(block, tag).map(str::to_owned)
}

fn xml_int(block: &str, tag: &str) -> Option<i64> {
    xml_text_in(block, tag)?.parse().ok()
}

fn xml_float(block: &str, tag: &str) -> Option<f64> {
    xml_text_in(block, tag)?.parse().ok()
}

/// EBI HMMER hmmscan vs Pfam.
pub fn hmmer_web_hmmscan(
    protein: &str,
    max_hits: usize,
    policy: &OnlineSearchPolicy<'_, impl HttpTransport>,
) -> Result<Vec<OnlineHmmHit>, IoError> {
    refuse_if_disarmed(policy.enabled)?;
    if policy.cancel.is_cancelled() {
        return Err(IoError::Cancelled);
    }
    let seq = clean_query(protein, false);
    if seq.is_empty() {
        return Err(IoError::online("empty HMMER query"));
    }
    if seq.len() > max_query_len("hmmscan") {
        return Err(IoError::online("HMMER query exceeds the length cap"));
    }
    let body = serde_json::to_vec(&serde_json::json!({"input": seq, "database": "pfam"}))
        .map_err(|e| IoError::online(e.to_string()))?;
    let submit_text = String::from_utf8_lossy(
        &http(
            policy.transport,
            "POST",
            HMMER_WEB_SUBMIT_URL,
            body,
            vec![
                ("Content-Type".into(), "application/json".into()),
                ("Accept".into(), "application/json".into()),
            ],
            HMMER_WEB_MAX_RESPONSE_BYTES,
        )?
        .body,
    )
    .into_owned();
    let submit: Value = serde_json::from_str(&submit_text)
        .map_err(|_| IoError::online("EBI HMMER returned an unparseable submit response"))?;
    let inline = hmmer_web_parse_json(&submit, max_hits);
    if !inline.is_empty() {
        return Ok(inline);
    }
    let job_id = submit
        .get("id")
        .or_else(|| submit.get("uuid"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if job_id.is_empty() {
        return Err(IoError::online("EBI HMMER did not return a job id"));
    }
    if job_id.contains('/') || job_id.contains("..") {
        return Err(IoError::online("EBI HMMER job id failed sanitisation"));
    }
    let result_url = format!("{HMMER_WEB_RESULT_URL}{job_id}");
    let mut elapsed = Duration::ZERO;
    let mut consecutive_fail = 0u32;
    while elapsed < policy.max_wait {
        if policy.cancel.wait_timeout(policy.poll_interval) {
            return Err(IoError::Cancelled);
        }
        elapsed += policy.poll_interval;
        let rtext = match http(
            policy.transport,
            "GET",
            &result_url,
            Vec::new(),
            vec![("Accept".into(), "application/json".into())],
            HMMER_WEB_MAX_RESPONSE_BYTES,
        ) {
            Ok(r) => String::from_utf8_lossy(&r.body).into_owned(),
            Err(_) => {
                consecutive_fail += 1;
                if consecutive_fail >= 5 {
                    return Err(IoError::online(
                        "EBI HMMER kept returning errors — try again later.",
                    ));
                }
                continue;
            }
        };
        let obj: Value = match serde_json::from_str(&rtext) {
            Ok(v) => v,
            Err(_) => {
                consecutive_fail += 1;
                if consecutive_fail >= 5 {
                    return Err(IoError::online(
                        "EBI HMMER kept returning errors — try again later.",
                    ));
                }
                continue;
            }
        };
        consecutive_fail = 0;
        let status = obj
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_uppercase();
        if HMMER_ERROR.iter().any(|s| *s == status) {
            return Err(IoError::online("EBI HMMER reported the job failed"));
        }
        if !status.is_empty() && HMMER_DONE.iter().all(|s| *s != status) {
            continue;
        }
        return Ok(hmmer_web_parse_json(&obj, max_hits));
    }
    Err(IoError::online(format!(
        "EBI HMMER timed out after {}s",
        policy.max_wait.as_secs()
    )))
}

/// Pull Pfam hits from an EBI result body.
#[must_use]
pub fn hmmer_web_parse_json(obj: &Value, max_hits: usize) -> Vec<OnlineHmmHit> {
    let hits = obj
        .get("result")
        .and_then(|r| r.get("hits"))
        .or_else(|| obj.get("results").and_then(|r| r.get("hits")))
        .or_else(|| obj.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for h in hits {
        let Some(h) = h.as_object() else {
            continue;
        };
        let md = h.get("metadata").and_then(Value::as_object);
        let acc = strip_controls(
            h.get("acc")
                .or_else(|| md.and_then(|m| m.get("accession")))
                .or_else(|| h.get("accession"))
                .and_then(Value::as_str)
                .unwrap_or("?"),
        );
        let name = strip_controls(
            md.and_then(|m| m.get("identifier").or_else(|| m.get("id")))
                .or_else(|| h.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(acc.as_str()),
        );
        let desc = strip_controls(
            md.and_then(|m| m.get("description"))
                .or_else(|| h.get("desc"))
                .or_else(|| h.get("description"))
                .and_then(Value::as_str)
                .unwrap_or(""),
        );
        let ndom = h
            .get("ndom")
            .or_else(|| h.get("nincluded"))
            .or_else(|| h.get("nreported"))
            .and_then(Value::as_i64)
            .unwrap_or_else(|| {
                h.get("domains")
                    .and_then(Value::as_array)
                    .map(|d| d.len() as i64)
                    .unwrap_or(0)
            });
        out.push(OnlineHmmHit {
            acc,
            name,
            description: desc,
            evalue: h
                .get("evalue")
                .or_else(|| h.get("eval"))
                .and_then(Value::as_f64),
            bit_score: h
                .get("score")
                .or_else(|| h.get("bitscore"))
                .and_then(Value::as_f64),
            n_dom: ndom,
        });
        if out.len() >= max_hits {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    struct ScriptedBlast {
        put: String,
        polls: Mutex<Vec<String>>,
        xml: String,
        deleted: AtomicBool,
        polls_seen: Arc<AtomicUsize>,
    }

    impl HttpTransport for ScriptedBlast {
        fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError> {
            let body = String::from_utf8_lossy(&req.body);
            if body.contains("CMD=Delete") || body.contains("CMD%3DDelete") {
                self.deleted.store(true, Ordering::SeqCst);
                return Ok(HttpResponse {
                    status: 200,
                    body: b"deleted".to_vec(),
                });
            }
            if body.contains("CMD=Put") || body.contains("CMD%3DPut") {
                return Ok(HttpResponse {
                    status: 200,
                    body: self.put.as_bytes().to_vec(),
                });
            }
            if body.contains("SearchInfo") {
                self.polls_seen.fetch_add(1, Ordering::SeqCst);
                let mut q = self.polls.lock().expect("polls");
                let next = if q.is_empty() {
                    "Status=WAITING".to_owned()
                } else {
                    q.remove(0)
                };
                return Ok(HttpResponse {
                    status: 200,
                    body: next.into_bytes(),
                });
            }
            if body.contains("FORMAT_TYPE=XML") || body.contains("FORMAT_TYPE%3DXML") {
                return Ok(HttpResponse {
                    status: 200,
                    body: self.xml.as_bytes().to_vec(),
                });
            }
            Err(IoError::NetworkDisabled)
        }
    }

    fn sample_xml() -> String {
        r#"<?xml version="1.0"?>
<!DOCTYPE BlastOutput PUBLIC "-//NCBI//NCBI BlastOutput/EN" "BlastOutput.dtd">
<BlastOutput><BlastOutput_iterations><Iteration>
<Hit>
  <Hit_id>gi|1</Hit_id>
  <Hit_def>demo protein</Hit_def>
  <Hit_accession>P12345</Hit_accession>
  <Hit_hsps><Hsp>
    <Hsp_identity>90</Hsp_identity>
    <Hsp_align-len>100</Hsp_align-len>
    <Hsp_evalue>1e-20</Hsp_evalue>
    <Hsp_bit-score>80.5</Hsp_bit-score>
    <Hsp_query-from>1</Hsp_query-from>
    <Hsp_query-to>100</Hsp_query-to>
    <Hsp_hit-from>10</Hsp_hit-from>
    <Hsp_hit-to>109</Hsp_hit-to>
  </Hsp></Hit_hsps>
</Hit>
</Iteration></BlastOutput_iterations></BlastOutput>"#
            .into()
    }

    #[test]
    fn setting_off_errors_without_transport() {
        struct Boom;
        impl HttpTransport for Boom {
            fn execute(&self, _req: &HttpRequest) -> Result<HttpResponse, IoError> {
                panic!("transport must not run when online search is off");
            }
        }
        let cancel = CancellationToken::new();
        let policy = OnlineSearchPolicy {
            enabled: false,
            transport: &Boom,
            cancel: &cancel,
            poll_interval: Duration::from_millis(1),
            max_wait: Duration::from_millis(10),
        };
        let err = ncbi_blast_online("ATGC", "blastn", None, 5, &policy).unwrap_err();
        assert!(matches!(err, IoError::OnlineDisabled), "{err}");
    }

    #[test]
    fn mock_blast_parses_xml_and_never_opens_a_socket() {
        let transport = ScriptedBlast {
            put: "RID = TESTJOB1\n".into(),
            polls: Mutex::new(vec!["Status=READY".into()]),
            xml: sample_xml(),
            deleted: AtomicBool::new(false),
            polls_seen: Arc::new(AtomicUsize::new(0)),
        };
        let cancel = CancellationToken::new();
        let policy = OnlineSearchPolicy {
            enabled: true,
            transport: &transport,
            cancel: &cancel,
            poll_interval: Duration::from_millis(5),
            max_wait: Duration::from_secs(2),
        };
        let hits = ncbi_blast_online("ATGCATGCATGC", "blastn", None, 5, &policy).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].accession, "P12345");
        assert_eq!(hits[0].identity_pct, Some(90.0));
        assert!(!transport.deleted.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_stops_polling_and_deletes_rid() {
        let transport = ScriptedBlast {
            put: "RID = CANCELME\n".into(),
            polls: Mutex::new(vec![
                "Status=WAITING".into(),
                "Status=WAITING".into(),
                "Status=WAITING".into(),
                "Status=READY".into(),
            ]),
            xml: sample_xml(),
            deleted: AtomicBool::new(false),
            polls_seen: Arc::new(AtomicUsize::new(0)),
        };
        let cancel = CancellationToken::new();
        let seen = Arc::clone(&transport.polls_seen);
        let token = cancel.clone();
        std::thread::spawn(move || {
            while seen.load(Ordering::SeqCst) < 1 {
                std::thread::sleep(Duration::from_millis(2));
            }
            token.cancel();
        });
        let policy = OnlineSearchPolicy {
            enabled: true,
            transport: &transport,
            cancel: &cancel,
            poll_interval: Duration::from_millis(15),
            max_wait: Duration::from_secs(5),
        };
        let err = ncbi_blast_online("ATGCATGCATGC", "blastn", None, 5, &policy).unwrap_err();
        assert!(matches!(err, IoError::Cancelled), "{err}");
        assert!(
            transport.deleted.load(Ordering::SeqCst),
            "RID must be deleted"
        );
        let polls = transport.polls_seen.load(Ordering::SeqCst);
        assert!(polls < 4, "cancel must stop polling, saw {polls}");
    }

    #[test]
    fn default_transport_is_offline() {
        let cancel = CancellationToken::new();
        let policy = OnlineSearchPolicy {
            enabled: true,
            transport: &crate::plasmidsaurus::OfflineTransport,
            cancel: &cancel,
            poll_interval: Duration::from_millis(1),
            max_wait: Duration::from_millis(5),
        };
        let err = ncbi_blast_online("ATGC", "blastn", None, 5, &policy).unwrap_err();
        assert!(matches!(err, IoError::NetworkDisabled), "{err}");
    }

    #[test]
    fn search_host_allowlist() {
        assert!(assert_search_host("blast.ncbi.nlm.nih.gov").is_ok());
        assert!(assert_search_host("www.ebi.ac.uk").is_ok());
        assert!(assert_search_host("evil.example").is_err());
        assert!(assert_search_url("http://blast.ncbi.nlm.nih.gov/Blast.cgi").is_err());
        assert!(assert_search_url(NCBI_BLAST_URL).is_ok());
    }

    #[test]
    fn hmmer_json_reads_metadata_identifier() {
        let obj = serde_json::json!({
            "result": {"hits": [{
                "acc": "PF00069",
                "name": "12345",
                "metadata": {"identifier": "Pkinase", "description": "kinase"},
                "evalue": 1e-10,
                "score": 40.0,
                "ndom": 1
            }]}
        });
        let hits = hmmer_web_parse_json(&obj, 5);
        assert_eq!(hits[0].name, "Pkinase");
        assert_eq!(hits[0].acc, "PF00069");
    }
}
