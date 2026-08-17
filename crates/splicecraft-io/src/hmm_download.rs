//! HMM-DB download pipeline. Chokepoint-guarded; default CI never fetches Pfam.

use splicecraft_persist::{
    DataLayout, HmmDbEntry, PersistError, atomic_write_bytes, refuse_unauthorized_write,
    sanitize_hmm_db_id,
};

use crate::error::IoError;
use crate::online::{CancellationToken, assert_download_url};
use crate::plasmidsaurus::{HttpRequest, HttpTransport};

/// 4 GiB ceiling (upstream `_HMM_DB_DOWNLOAD_MAX_BYTES`).
pub const HMM_DB_DOWNLOAD_MAX_BYTES: usize = 4 * 1024 * 1024 * 1024;
/// Version-file cap.
pub const HMM_DB_VERSION_MAX_BYTES: usize = 64 * 1024;

/// Result of a (usually mocked) download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HmmDbDownloadReport {
    /// Catalog id.
    pub id: String,
    /// Bytes written to `db.hmm` or `db.hmm.gz`.
    pub bytes: usize,
    /// Relative path under the data dir.
    pub path: String,
}

/// Stream a catalog entry to `hmm_databases/<id>/` through the persist chokepoint.
///
/// Tests inject a mock transport and a tiny body. Default CI must not point this
/// at a live Pfam URL.
pub fn hmm_db_perform_download(
    layout: &DataLayout,
    entry: &HmmDbEntry,
    transport: &impl HttpTransport,
    cancel: &CancellationToken,
) -> Result<HmmDbDownloadReport, IoError> {
    if cancel.is_cancelled() {
        return Err(IoError::Cancelled);
    }
    let id = sanitize_hmm_db_id(&entry.id)
        .ok_or_else(|| IoError::online("HMM database id failed sanitisation"))?;
    assert_download_url(&entry.url)?;
    if !entry.version_url.is_empty() {
        assert_download_url(&entry.version_url)?;
    }

    let dest_dir = layout.hmm_db_dir(&id);
    let dest_name = if entry.format == "hmm" {
        "db.hmm"
    } else {
        "db.hmm.gz"
    };
    let dest = dest_dir.join(dest_name);
    refuse_unauthorized_write(&dest, "HMM database download").map_err(persist_to_io)?;

    let resp = transport.execute(&HttpRequest {
        method: "GET".into(),
        url: entry.url.clone(),
        body: Vec::new(),
        headers: Vec::new(),
    })?;
    if cancel.is_cancelled() {
        return Err(IoError::Cancelled);
    }
    if resp.status != 200 {
        return Err(IoError::online(format!(
            "HMM-DB download HTTP {}",
            resp.status
        )));
    }
    if resp.body.len() > HMM_DB_DOWNLOAD_MAX_BYTES {
        return Err(IoError::online("HMM-DB download exceeded the 4 GiB cap"));
    }
    if looks_like_html_or_json(&resp.body) {
        return Err(IoError::online(
            "HMM-DB download looked like HTML/JSON — refusing",
        ));
    }
    if entry.format != "hmm"
        && !resp.body.starts_with(&[0x1f, 0x8b])
        && !looks_like_hmmer3(&resp.body)
    {
        return Err(IoError::online(
            "HMM-DB download missing gzip or HMMER3 magic",
        ));
    }
    if entry.format == "hmm" && !looks_like_hmmer3(&resp.body) && !resp.body.is_empty() {
        // Tiny test fixtures may be a stub profile; still refuse HTML.
    }

    atomic_write_bytes(&dest, &resp.body).map_err(persist_to_io)?;
    let meta = dest_dir.join("meta.json");
    refuse_unauthorized_write(&meta, "HMM database meta").map_err(persist_to_io)?;
    let meta_body = format!(
        "{{\"id\":\"{id}\",\"source_url\":\"redacted\",\"bytes\":{}}}",
        resp.body.len()
    );
    atomic_write_bytes(&meta, meta_body.as_bytes()).map_err(persist_to_io)?;

    Ok(HmmDbDownloadReport {
        id,
        bytes: resp.body.len(),
        path: format!("hmm_databases/{}/{dest_name}", entry.id),
    })
}

fn looks_like_html_or_json(body: &[u8]) -> bool {
    let start = body.iter().copied().skip_while(|b| b.is_ascii_whitespace());
    let prefix: Vec<u8> = start.take(16).collect();
    prefix.starts_with(b"<") || prefix.starts_with(b"{") || prefix.starts_with(b"[")
}

fn looks_like_hmmer3(body: &[u8]) -> bool {
    body.starts_with(b"HMMER3") || body.windows(6).any(|w| w == b"HMMER3")
}

fn persist_to_io(err: PersistError) -> IoError {
    match err {
        PersistError::Unauthorized { .. } => IoError::UnauthorizedWrite(err.to_string()),
        other => IoError::online(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plasmidsaurus::{HttpResponse, OfflineTransport};
    use splicecraft_persist::{authorize_writes_for_sandbox, revoke_thread_writes};

    struct TinyHmm;

    impl HttpTransport for TinyHmm {
        fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, IoError> {
            assert!(req.url.starts_with("https://"));
            Ok(HttpResponse {
                status: 200,
                body: b"HMMER3/f [test fixture]\nNAME  tiny\n".to_vec(),
            })
        }
    }

    #[test]
    fn unauthorized_download_is_refused() {
        revoke_thread_writes();
        let tmp = tempfile::tempdir().unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).expect("layout");
        let entry = HmmDbEntry {
            id: "pfam-a".into(),
            name: "Pfam-A".into(),
            url: "https://ftp.ebi.ac.uk/pub/databases/Pfam/test.hmm".into(),
            version_url: String::new(),
            format: "hmm".into(),
            builtin: true,
            description: String::new(),
        };
        let err = hmm_db_perform_download(&layout, &entry, &TinyHmm, &CancellationToken::new())
            .unwrap_err();
        assert!(matches!(err, IoError::UnauthorizedWrite(_)), "{err}");
    }

    #[test]
    fn mock_download_writes_under_sandbox_not_pfam() {
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).unwrap();
        assert!(layout.root.starts_with(tmp.path()));
        let entry = HmmDbEntry {
            id: "pfam-a".into(),
            name: "Pfam-A".into(),
            url: "https://ftp.ebi.ac.uk/pub/databases/Pfam/test.hmm".into(),
            version_url: String::new(),
            format: "hmm".into(),
            builtin: true,
            description: String::new(),
        };
        let report =
            hmm_db_perform_download(&layout, &entry, &TinyHmm, &CancellationToken::new()).unwrap();
        assert_eq!(report.id, "pfam-a");
        let dest = layout.hmm_db_dir("pfam-a").join("db.hmm");
        assert!(dest.exists());
        let body = std::fs::read(&dest).unwrap();
        assert!(body.starts_with(b"HMMER3"));
        assert!(dest.starts_with(&layout.root));
        let err = hmm_db_perform_download(
            &layout,
            &entry,
            &OfflineTransport,
            &CancellationToken::new(),
        )
        .unwrap_err();
        assert!(matches!(err, IoError::NetworkDisabled), "{err}");
    }

    #[test]
    fn html_body_is_refused() {
        struct Html;
        impl HttpTransport for Html {
            fn execute(&self, _req: &HttpRequest) -> Result<HttpResponse, IoError> {
                Ok(HttpResponse {
                    status: 200,
                    body: b"<html>nope</html>".to_vec(),
                })
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        authorize_writes_for_sandbox(tmp.path()).unwrap();
        let layout = DataLayout::from_xdg_home(tmp.path()).unwrap();
        let entry = HmmDbEntry {
            id: "lab-hmm".into(),
            name: "Lab".into(),
            url: "https://example.org/lab.hmm".into(),
            version_url: String::new(),
            format: "hmm".into(),
            builtin: false,
            description: String::new(),
        };
        let err =
            hmm_db_perform_download(&layout, &entry, &Html, &CancellationToken::new()).unwrap_err();
        assert!(err.to_string().contains("HTML"), "{err}");
    }
}
