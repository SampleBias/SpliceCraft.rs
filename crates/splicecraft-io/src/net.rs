//! Accession sanitizer, SSRF helpers, and an offline-by-default NCBI fetch.

use std::net::IpAddr;

use crate::error::IoError;

/// NCBI Entrez / datasets hosts that public fetches may target.
pub const NCBI_ALLOWLIST: &[&str] = &[
    "eutils.ncbi.nlm.nih.gov",
    "www.ncbi.nlm.nih.gov",
    "ncbi.nlm.nih.gov",
    "api.ncbi.nlm.nih.gov",
];

/// Clamp a user-supplied accession. `None` if it is empty or has metacharacters.
#[must_use]
pub fn sanitize_accession(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() || s.len() > 32 {
        return None;
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        Some(s.to_owned())
    } else {
        None
    }
}

/// True for loopback, RFC1918, link-local, multicast, unspecified, reserved.
#[must_use]
pub fn ip_is_non_public(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v) => {
            v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_multicast()
                || v.is_unspecified()
                || v.is_broadcast()
                || v.is_documentation()
        }
        IpAddr::V6(v) => {
            v.is_loopback()
                || v.is_multicast()
                || v.is_unspecified()
                || v.is_unique_local()
                || is_ipv6_link_local(v)
                || v.to_ipv4_mapped()
                    .is_some_and(|m| ip_is_non_public(IpAddr::V4(m)))
        }
    }
}

fn is_ipv6_link_local(v: std::net::Ipv6Addr) -> bool {
    let o = v.octets();
    o[0] == 0xfe && (o[1] & 0xc0) == 0x80
}

/// Host must be an NCBI allowlist entry (no DNS). Used before any socket.
pub fn assert_ncbi_host(host: &str) -> Result<(), IoError> {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if NCBI_ALLOWLIST.iter().any(|ok| h == *ok) {
        Ok(())
    } else {
        Err(IoError::HostNotAllowlisted(host.to_owned()))
    }
}

/// Refuse a literal IP that is not globally routable.
pub fn assert_public_ip(addr: IpAddr) -> Result<(), IoError> {
    if ip_is_non_public(addr) {
        Err(IoError::NonPublicAddress(addr.to_string()))
    } else {
        Ok(())
    }
}

/// Entrez `efetch` URL for a nucleotide accession (not opened by default tests).
#[must_use]
pub fn ncbi_efetch_url(accession: &str) -> String {
    format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?\
         db=nuccore&id={accession}&rettype=gb&retmode=text"
    )
}

/// Fetch a GenBank record by accession.
///
/// Default builds (no `ncbi` feature) **never** open a socket. They validate
/// the accession and return [`IoError::NetworkDisabled`].
pub fn fetch_genbank(accession: &str) -> Result<crate::core::Record, IoError> {
    let acc = sanitize_accession(accession)
        .ok_or_else(|| IoError::InvalidAccession(accession.to_owned()))?;
    assert_ncbi_host("eutils.ncbi.nlm.nih.gov")?;
    // Build the allowlisted URL so a later increment can open it. Stage 03
    // never creates a socket — even with `--features ncbi`.
    let _prepared = ncbi_efetch_url(&acc);
    debug_assert!(
        _prepared.starts_with("https://eutils.ncbi.nlm.nih.gov/"),
        "efetch URL must stay on the NCBI allowlist"
    );
    Err(IoError::NetworkDisabled)
}
