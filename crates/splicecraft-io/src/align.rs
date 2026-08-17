//! Myers / Hirschberg pairwise alignment, identity counting, and verification grades.

use splicecraft_bio::{iupac_compatible, normalize_dna_for_align};
use splicecraft_persist::AlignmentBadge;
use splicecraft_util::format_identity_pct;

use crate::error::IoError;

/// Cap per side (upstream `_PAIRWISE_MAX_LEN`).
pub const PAIRWISE_MAX_LEN: usize = 200_000;

/// Hirschberg base-case area (upstream `_MYERS_DP_AREA`).
const MYERS_DP_AREA: usize = 4096;

const IUPAC_NUC: &[char] = &[
    'A', 'C', 'G', 'T', 'U', 'M', 'R', 'W', 'S', 'Y', 'K', 'V', 'H', 'D', 'B', 'N',
];

/// Verified floor: ungapped identity.
pub const SEQ_STATUS_VERIFIED_UNGAPPED_PCT: f64 = 99.5;
/// Verified floor: coverage of the target.
pub const SEQ_STATUS_VERIFIED_COVERAGE_PCT: f64 = 99.0;
/// Verified requires zero gap columns.
pub const SEQ_STATUS_VERIFIED_MAX_GAPS: i64 = 0;
/// Near-match ungapped identity.
pub const SEQ_STATUS_NEAR_UNGAPPED_PCT: f64 = 95.0;
/// Near-match coverage.
pub const SEQ_STATUS_NEAR_COVERAGE_PCT: f64 = 80.0;

/// Global vs local pairwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignMode {
    /// Needleman–Wunsch (Myers / Hirschberg).
    Global,
    /// Smith–Waterman (small sequences only).
    Local,
}

impl AlignMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
        }
    }
}

/// One pairwise result. Aligned strings stay off logs.
#[derive(Clone, Debug, PartialEq)]
pub struct AlignResult {
    /// `global` or `local`.
    pub mode: &'static str,
    /// Affine-equivalent display score.
    pub score: f64,
    /// Gap-inclusive identity (BLAST style).
    pub identity_pct: f64,
    /// Identity over non-gap columns only.
    pub ungapped_identity_pct: f64,
    /// Gapped query.
    pub aligned_q: String,
    /// Gapped target.
    pub aligned_t: String,
    /// IUPAC-compatible columns.
    pub n_matches: i64,
    /// IUPAC-incompatible columns.
    pub n_mismatches: i64,
    /// Columns where either side is `-`.
    pub n_gap_cols: i64,
    /// Contiguous `-` runs in the query.
    pub n_gap_opens_q: i64,
    /// Contiguous `-` runs in the target.
    pub n_gap_opens_t: i64,
    /// Alias of [`Self::n_gap_cols`].
    pub n_gaps: i64,
    /// Ungapped query length.
    pub q_len: usize,
    /// Ungapped target length.
    pub t_len: usize,
}

/// Overlay paint state (worst-wins in bar mode).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignState {
    /// Both bases present and IUPAC-compatible.
    Match,
    /// Target base present, query gapped.
    Gap,
    /// Both bases present and incompatible.
    Mismatch,
}

impl AlignState {
    fn priority(self) -> u8 {
        match self {
            Self::Match => 0,
            Self::Gap => 1,
            Self::Mismatch => 2,
        }
    }

    /// Single-character linear-map glyph.
    #[must_use]
    pub fn glyph(self) -> char {
        match self {
            Self::Match => '=',
            Self::Gap => '.',
            Self::Mismatch => 'X',
        }
    }
}

/// Verification grade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqStatus {
    /// Clean consensus of the target.
    Verified,
    /// High identity, a few SNPs / indels.
    Near,
    /// Low coverage of the target.
    Partial,
    /// Different molecule.
    Divergent,
}

impl SeqStatus {
    /// Sort key: worst first for the report.
    #[must_use]
    pub fn report_priority(self) -> u8 {
        match self {
            Self::Divergent => 0,
            Self::Partial => 1,
            Self::Near => 2,
            Self::Verified => 3,
        }
    }

    /// Best-wins key for the library column.
    #[must_use]
    pub fn badge_priority(self) -> u8 {
        self.report_priority()
    }

    /// `verified` / `near` / `partial` / `divergent`.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Near => "near",
            Self::Partial => "partial",
            Self::Divergent => "divergent",
        }
    }

    /// Library-column glyph.
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Verified => "✓",
            Self::Near => "⚠",
            Self::Partial => "~",
            Self::Divergent => "✗",
        }
    }

    fn from_code(s: &str) -> Self {
        match s {
            "verified" => Self::Verified,
            "near" => Self::Near,
            "partial" => Self::Partial,
            _ => Self::Divergent,
        }
    }
}

/// One discrepancy in target coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlignVariant {
    /// `snp` / `insertion` / `deletion` / `truncated`.
    pub kind: &'static str,
    /// 0-based target position.
    pub target_pos: usize,
    /// Event length in bp.
    pub length: usize,
}

const MAX_VARIANTS: usize = 10_000;

/// Global (or small local) pairwise alignment with IUPAC counting.
pub fn pairwise_align(query: &str, target: &str, mode: AlignMode) -> Result<AlignResult, IoError> {
    pairwise_align_scored(query, target, mode, 2.0, -1.0, -2.0, -0.5)
}

fn pairwise_align_scored(
    query: &str,
    target: &str,
    mode: AlignMode,
    match_s: f64,
    mismatch_s: f64,
    open_gap: f64,
    extend_gap: f64,
) -> Result<AlignResult, IoError> {
    let q = normalize_dna_for_align(query).map_err(|e| IoError::align(e.to_string()))?;
    let t = normalize_dna_for_align(target).map_err(|e| IoError::align(e.to_string()))?;
    if q.is_empty() || t.is_empty() {
        return Err(IoError::align("query / target sequence is empty"));
    }
    if q.len() > PAIRWISE_MAX_LEN || t.len() > PAIRWISE_MAX_LEN {
        return Err(IoError::align(format!(
            "sequence too long for pairwise align (cap {PAIRWISE_MAX_LEN} bp per side)"
        )));
    }
    let (aligned_q, aligned_t) = match mode {
        AlignMode::Global => {
            let (aq, at) = myers_align_global(&q, &t);
            if aq.replace('-', "") != q || at.replace('-', "") != t {
                return Err(IoError::align("myers alignment failed round-trip check"));
            }
            (aq, at)
        }
        AlignMode::Local => smith_waterman(&q, &t)?,
    };
    if aligned_q.is_empty() || aligned_t.is_empty() {
        return Err(IoError::align("aligner produced empty rows"));
    }
    if aligned_q.len() != aligned_t.len() {
        return Err(IoError::align(format!(
            "aligned strings differ in length: q={} vs t={}",
            aligned_q.len(),
            aligned_t.len()
        )));
    }
    let mut n_matches = 0i64;
    let mut n_mismatches = 0i64;
    let mut n_gap_cols = 0i64;
    let mut n_gap_opens_q = 0i64;
    let mut n_gap_opens_t = 0i64;
    let mut in_q_gap = false;
    let mut in_t_gap = false;
    for (ch_q, ch_t) in aligned_q.chars().zip(aligned_t.chars()) {
        if ch_q == '-' {
            if !in_q_gap {
                n_gap_opens_q += 1;
                in_q_gap = true;
            }
        } else {
            in_q_gap = false;
        }
        if ch_t == '-' {
            if !in_t_gap {
                n_gap_opens_t += 1;
                in_t_gap = true;
            }
        } else {
            in_t_gap = false;
        }
        if ch_q == '-' || ch_t == '-' {
            n_gap_cols += 1;
            continue;
        }
        if iupac_compatible(ch_q, ch_t) {
            n_matches += 1;
        } else {
            n_mismatches += 1;
        }
    }
    let aligned_cols = n_matches + n_mismatches + n_gap_cols;
    let ungapped_cols = n_matches + n_mismatches;
    let identity_pct = if aligned_cols > 0 {
        100.0 * n_matches as f64 / aligned_cols as f64
    } else {
        0.0
    };
    let ungapped_identity_pct = if ungapped_cols > 0 {
        100.0 * n_matches as f64 / ungapped_cols as f64
    } else {
        0.0
    };
    let n_gap_opens = n_gap_opens_q + n_gap_opens_t;
    let score = match_s * n_matches as f64
        + mismatch_s * n_mismatches as f64
        + open_gap * n_gap_opens as f64
        + extend_gap * (n_gap_cols - n_gap_opens).max(0) as f64;
    Ok(AlignResult {
        mode: mode.as_str(),
        score,
        identity_pct,
        ungapped_identity_pct,
        aligned_q,
        aligned_t,
        n_matches,
        n_mismatches,
        n_gap_cols,
        n_gap_opens_q,
        n_gap_opens_t,
        n_gaps: n_gap_cols,
        q_len: q.len(),
        t_len: t.len(),
    })
}

fn myers_align_global(query: &str, target: &str) -> (String, String) {
    if query == target {
        return (query.to_owned(), target.to_owned());
    }
    myers_align_core(query, target)
}

fn myers_align_core(a: &str, b: &str) -> (String, String) {
    let ab: Vec<char> = a.chars().collect();
    let bb: Vec<char> = b.chars().collect();
    myers_align_chars(&ab, &bb)
}

fn myers_align_chars(a: &[char], b: &[char]) -> (String, String) {
    let qn = a.len();
    let tn = b.len();
    let lim = qn.min(tn);
    let mut lo = 0;
    while lo < lim && (a[lo] == b[lo] || iupac_compatible(a[lo], b[lo])) {
        lo += 1;
    }
    let mut hi = 0;
    let lim2 = lim - lo;
    while hi < lim2
        && (a[qn - 1 - hi] == b[tn - 1 - hi] || iupac_compatible(a[qn - 1 - hi], b[tn - 1 - hi]))
    {
        hi += 1;
    }
    let pre_q: String = a[..lo].iter().collect();
    let pre_t: String = b[..lo].iter().collect();
    let suf_q: String = a[qn - hi..].iter().collect();
    let suf_t: String = b[tn - hi..].iter().collect();
    let a = &a[lo..qn - hi];
    let b = &b[lo..tn - hi];
    let m = a.len();
    let n = b.len();
    let (mid_q, mid_t) = if m == 0 {
        ("-".repeat(n), b.iter().collect())
    } else if n == 0 {
        (a.iter().collect(), "-".repeat(m))
    } else if m == 1 || n == 1 || m.saturating_mul(n) <= MYERS_DP_AREA {
        myers_dp_global(a, b)
    } else {
        let h = m / 2;
        let al = &a[..h];
        let ar = &a[h..];
        let peq_l = myers_build_peq(al);
        let fwd = myers_edit_profile(al, b, &peq_l);
        let mut ar_rev = ar.to_vec();
        ar_rev.reverse();
        let mut b_rev = b.to_vec();
        b_rev.reverse();
        let peq_r = myers_build_peq(&ar_rev);
        let bwd = myers_edit_profile(&ar_rev, &b_rev, &peq_r);
        let mut best_j = 0;
        let mut best = fwd[0] + bwd[n];
        for j in 1..=n {
            let s = fwd[j] + bwd[n - j];
            if s < best {
                best = s;
                best_j = j;
            }
        }
        let (lq, lt) = myers_align_chars(al, &b[..best_j]);
        let (rq, rt) = myers_align_chars(ar, &b[best_j..]);
        (format!("{lq}{rq}"), format!("{lt}{rt}"))
    };
    (
        format!("{pre_q}{mid_q}{suf_q}"),
        format!("{pre_t}{mid_t}{suf_t}"),
    )
}

fn myers_build_peq(pattern: &[char]) -> Vec<(char, Bits)> {
    let m = pattern.len();
    let mut map: Vec<(char, Bits)> = IUPAC_NUC.iter().map(|&c| (c, Bits::zero(m))).collect();
    for (j, &ch) in pattern.iter().enumerate() {
        for c in IUPAC_NUC {
            if iupac_compatible(ch, *c)
                && let Some((_, bits)) = map.iter_mut().find(|(k, _)| *k == *c)
            {
                bits.set_bit(j);
            }
        }
    }
    map
}

fn peq_get(peq: &[(char, Bits)], c: char, m: usize) -> Bits {
    peq.iter()
        .find(|(k, _)| *k == c)
        .map(|(_, b)| b.clone())
        .unwrap_or_else(|| Bits::zero(m))
}

fn myers_edit_profile(pattern: &[char], text: &[char], peq: &[(char, Bits)]) -> Vec<i32> {
    let m = pattern.len();
    if m == 0 {
        return (0..=text.len() as i32).collect();
    }
    let mask = Bits::ones(m);
    let mut vp = mask.clone();
    let mut vn = Bits::zero(m);
    let mut score = m as i32;
    let mut out = vec![score];
    for &c in text {
        let eq = peq_get(peq, c, m);
        let xv = eq.or(&vn);
        let xh = eq.and(&vp).add(&vp).and(&mask).xor(&vp).or(&eq);
        let mut ph = vn.or(&xh.or(&vp).not_and_mask());
        let mh = vp.and(&xh);
        if ph.test(m - 1) {
            score += 1;
        } else if mh.test(m - 1) {
            score -= 1;
        }
        ph = ph.shl1_set_low();
        let mh = mh.shl1();
        vp = mh.or(&xv.or(&ph).not_and_mask());
        vn = ph.and(&xv);
        out.push(score);
    }
    out
}

fn myers_dp_global(a: &[char], b: &[char]) -> (String, String) {
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return ("-".repeat(n), b.iter().collect());
    }
    if n == 0 {
        return (a.iter().collect(), "-".repeat(m));
    }
    let mut dp = vec![0i32; (m + 1) * (n + 1)];
    let idx = |i: usize, j: usize| i * (n + 1) + j;
    for i in 1..=m {
        dp[idx(i, 0)] = i as i32;
    }
    for j in 1..=n {
        dp[idx(0, j)] = j as i32;
    }
    for i in 1..=m {
        for j in 1..=n {
            let sub = dp[idx(i - 1, j - 1)]
                + if a[i - 1] == b[j - 1] || iupac_compatible(a[i - 1], b[j - 1]) {
                    0
                } else {
                    1
                };
            dp[idx(i, j)] = sub.min(dp[idx(i - 1, j)] + 1).min(dp[idx(i, j - 1)] + 1);
        }
    }
    let mut i = m;
    let mut j = n;
    let mut ga = Vec::new();
    let mut gb = Vec::new();
    while i > 0 && j > 0 {
        let here = dp[idx(i, j)];
        let ai = a[i - 1];
        let bj = b[j - 1];
        let diag = dp[idx(i - 1, j - 1)]
            + if ai == bj || iupac_compatible(ai, bj) {
                0
            } else {
                1
            };
        if here == diag {
            ga.push(ai);
            gb.push(bj);
            i -= 1;
            j -= 1;
        } else if here == dp[idx(i - 1, j)] + 1 {
            ga.push(ai);
            gb.push('-');
            i -= 1;
        } else {
            ga.push('-');
            gb.push(bj);
            j -= 1;
        }
    }
    while i > 0 {
        ga.push(a[i - 1]);
        gb.push('-');
        i -= 1;
    }
    while j > 0 {
        ga.push('-');
        gb.push(b[j - 1]);
        j -= 1;
    }
    ga.reverse();
    gb.reverse();
    (ga.into_iter().collect(), gb.into_iter().collect())
}

fn smith_waterman(q: &str, t: &str) -> Result<(String, String), IoError> {
    let a: Vec<char> = q.chars().collect();
    let b: Vec<char> = t.chars().collect();
    let m = a.len();
    let n = b.len();
    if m.saturating_mul(n) > 1_000_000 {
        return Err(IoError::align(
            "local alignment is only implemented for sequences whose product is ≤ 1e6 cells",
        ));
    }
    let mut dp = vec![0i32; (m + 1) * (n + 1)];
    let idx = |i: usize, j: usize| i * (n + 1) + j;
    let mut best = 0i32;
    let mut bi = 0usize;
    let mut bj = 0usize;
    for i in 1..=m {
        for j in 1..=n {
            let s = if a[i - 1] == b[j - 1] || iupac_compatible(a[i - 1], b[j - 1]) {
                2
            } else {
                -1
            };
            let v = (dp[idx(i - 1, j - 1)] + s)
                .max(dp[idx(i - 1, j)] - 2)
                .max(dp[idx(i, j - 1)] - 2)
                .max(0);
            dp[idx(i, j)] = v;
            if v > best {
                best = v;
                bi = i;
                bj = j;
            }
        }
    }
    if best == 0 {
        return Err(IoError::align("no local alignment produced"));
    }
    let mut i = bi;
    let mut j = bj;
    let mut ga = Vec::new();
    let mut gb = Vec::new();
    while i > 0 && j > 0 && dp[idx(i, j)] > 0 {
        let s = if a[i - 1] == b[j - 1] || iupac_compatible(a[i - 1], b[j - 1]) {
            2
        } else {
            -1
        };
        if dp[idx(i, j)] == dp[idx(i - 1, j - 1)] + s {
            ga.push(a[i - 1]);
            gb.push(b[j - 1]);
            i -= 1;
            j -= 1;
        } else if dp[idx(i, j)] == dp[idx(i - 1, j)] - 2 {
            ga.push(a[i - 1]);
            gb.push('-');
            i -= 1;
        } else {
            ga.push('-');
            gb.push(b[j - 1]);
            j -= 1;
        }
    }
    ga.reverse();
    gb.reverse();
    Ok((ga.into_iter().collect(), gb.into_iter().collect()))
}

/// Collapse gapped rows into target-coordinate runs.
pub fn alignment_to_target_segments(
    aligned_q: &str,
    aligned_t: &str,
    t_start: usize,
) -> Result<Vec<(usize, usize, AlignState)>, IoError> {
    if aligned_q.len() != aligned_t.len() {
        return Err(IoError::align(format!(
            "aligned strings differ in length: q={} vs t={}",
            aligned_q.len(),
            aligned_t.len()
        )));
    }
    let mut segs = Vec::new();
    let mut t_pos = t_start;
    let mut cur_state: Option<AlignState> = None;
    let mut cur_start = t_pos;
    for (q, t) in aligned_q
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .zip(aligned_t.chars().map(|c| c.to_ascii_uppercase()))
    {
        if t == '-' {
            continue;
        }
        let state = if q == '-' {
            AlignState::Gap
        } else if iupac_compatible(q, t) {
            AlignState::Match
        } else {
            AlignState::Mismatch
        };
        if Some(state) != cur_state {
            if let Some(prev) = cur_state {
                segs.push((cur_start, t_pos, prev));
            }
            cur_state = Some(state);
            cur_start = t_pos;
        }
        t_pos += 1;
    }
    if let Some(prev) = cur_state {
        segs.push((cur_start, t_pos, prev));
    }
    Ok(segs)
}

/// Bar-mode collapse: each column keeps the worst state (mismatch > gap > match).
pub fn alignment_bar_columns(
    segments: &[(usize, usize, AlignState)],
    view_s: usize,
    view_e: usize,
    cols: usize,
    total_bp: usize,
) -> Vec<Option<AlignState>> {
    if cols == 0 || total_bp == 0 || view_s >= view_e {
        return vec![None; cols];
    }
    let mut worst = vec![None; cols];
    let bp_to_col = |bp: usize| -> usize {
        let span = view_e.saturating_sub(view_s).max(1);
        ((bp.saturating_sub(view_s)) * cols) / span
    };
    for &(seg_s, seg_e, state) in segments {
        let s = seg_s.max(view_s);
        let e = seg_e.min(view_e);
        if s >= e {
            continue;
        }
        let mut c0 = bp_to_col(s);
        let mut c1 = bp_to_col(e);
        if c1 <= c0 {
            c1 = c0 + 1;
        }
        c0 = c0.min(cols);
        c1 = c1.min(cols);
        if c1 <= c0 {
            continue;
        }
        let pr = state.priority();
        for slot in worst.iter_mut().take(c1).skip(c0) {
            if slot.is_none_or(|p: AlignState| pr > p.priority()) {
                *slot = Some(state);
            }
        }
    }
    worst
}

/// Linear-map overlay line (`=` match, `X` mismatch, `.` gap).
#[must_use]
pub fn render_alignment_bar(
    segments: &[(usize, usize, AlignState)],
    total_bp: usize,
    width: usize,
) -> String {
    let cols = alignment_bar_columns(segments, 0, total_bp.max(1), width.max(1), total_bp.max(1));
    cols.into_iter()
        .map(|s| s.map(AlignState::glyph).unwrap_or(' '))
        .collect()
}

/// Coverage of the target, clamped to 100%.
#[must_use]
pub fn coverage_pct_from_result(result: &AlignResult, target_len: usize) -> f64 {
    if target_len == 0 {
        return 0.0;
    }
    let aligned_bp = result.n_matches + result.n_mismatches;
    if aligned_bp <= 0 {
        return 0.0;
    }
    (100.0 * aligned_bp as f64 / target_len as f64).min(100.0)
}

/// Indel events (gap runs), not gapped bp.
#[must_use]
pub fn alignment_indel_events(result: &AlignResult) -> i64 {
    result.n_gap_opens_q.max(0) + result.n_gap_opens_t.max(0)
}

/// Classify one alignment for the library Seq column.
#[must_use]
pub fn alignment_quality_status(result: &AlignResult, target_len: usize) -> SeqStatus {
    if result.n_matches < 0
        || result.n_mismatches < 0
        || result.n_gaps < 0
        || result.ungapped_identity_pct < 0.0
    {
        return SeqStatus::Divergent;
    }
    let coverage_pct = coverage_pct_from_result(result, target_len);
    if result.ungapped_identity_pct >= SEQ_STATUS_VERIFIED_UNGAPPED_PCT
        && coverage_pct >= SEQ_STATUS_VERIFIED_COVERAGE_PCT
        && result.n_gaps <= SEQ_STATUS_VERIFIED_MAX_GAPS
        && result.n_mismatches == 0
    {
        return SeqStatus::Verified;
    }
    if result.ungapped_identity_pct >= SEQ_STATUS_NEAR_UNGAPPED_PCT
        && coverage_pct >= SEQ_STATUS_NEAR_COVERAGE_PCT
    {
        return SeqStatus::Near;
    }
    if coverage_pct < SEQ_STATUS_NEAR_COVERAGE_PCT {
        SeqStatus::Partial
    } else {
        SeqStatus::Divergent
    }
}

/// Best badge across stored alignments (verified wins).
#[must_use]
pub fn library_entry_alignment_summary(badges: &[AlignmentBadge]) -> Option<SeqStatus> {
    badges
        .iter()
        .map(|b| SeqStatus::from_code(&b.status))
        .max_by_key(|s| s.badge_priority())
}

/// Walk discrepancies; gap runs merge into one indel.
pub fn extract_variants_from_alignment(aligned_q: &str, aligned_t: &str) -> Vec<AlignVariant> {
    if aligned_q.is_empty() || aligned_t.is_empty() || aligned_q.len() != aligned_t.len() {
        return Vec::new();
    }
    let aq: Vec<char> = aligned_q.chars().collect();
    let at: Vec<char> = aligned_t.chars().collect();
    let mut variants = Vec::new();
    let mut target_pos = 0usize;
    let mut i = 0usize;
    let n = at.len();
    while i < n {
        if variants.len() >= MAX_VARIANTS {
            variants.push(AlignVariant {
                kind: "truncated",
                target_pos,
                length: 0,
            });
            break;
        }
        let ac = aq[i];
        let bc = at[i];
        if ac == '-' && bc == '-' {
            i += 1;
            continue;
        }
        if ac == '-' {
            let start = target_pos;
            let mut len = 0usize;
            while i < n && aq[i] == '-' && at[i] != '-' {
                len += 1;
                target_pos += 1;
                i += 1;
            }
            variants.push(AlignVariant {
                kind: "deletion",
                target_pos: start,
                length: len,
            });
            continue;
        }
        if bc == '-' {
            let start = target_pos;
            let mut len = 0usize;
            while i < n && at[i] == '-' && aq[i] != '-' {
                len += 1;
                i += 1;
            }
            variants.push(AlignVariant {
                kind: "insertion",
                target_pos: start,
                length: len,
            });
            continue;
        }
        if !iupac_compatible(ac.to_ascii_uppercase(), bc.to_ascii_uppercase()) {
            variants.push(AlignVariant {
                kind: "snp",
                target_pos,
                length: 1,
            });
        }
        target_pos += 1;
        i += 1;
    }
    variants
}

/// Compact badge for persist (no aligned DNA).
#[must_use]
pub fn badge_from_result(label: &str, result: &AlignResult, target_len: usize) -> AlignmentBadge {
    let status = alignment_quality_status(result, target_len);
    let first = extract_variants_from_alignment(&result.aligned_q, &result.aligned_t)
        .into_iter()
        .find(|v| v.kind != "truncated")
        .map(|v| v.target_pos);
    AlignmentBadge {
        label: label.to_owned(),
        status: status.code().into(),
        identity: format_identity_pct(Some(result.identity_pct), 1),
        n_mismatches: result.n_mismatches,
        n_indels: alignment_indel_events(result),
        first_variant_bp: first,
    }
}

/// One file from a bulk-align folder walk.
#[derive(Clone, Debug, PartialEq)]
pub struct BulkAlignRow {
    /// File stem / sample name.
    pub label: String,
    /// Alignment against the loaded target, if the file parsed.
    pub result: Option<AlignResult>,
    /// Failure reason (no DNA).
    pub error: Option<String>,
}

/// Align every GenBank / FASTA in `folder` against `target`.
pub fn bulk_align_folder(
    folder: &std::path::Path,
    target: &str,
    _circular: bool,
) -> Vec<BulkAlignRow> {
    let mut rows = Vec::new();
    let Ok(rd) = std::fs::read_dir(folder) else {
        rows.push(BulkAlignRow {
            label: folder.display().to_string(),
            result: None,
            error: Some("could not read folder".into()),
        });
        return rows;
    };
    let mut paths: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths.into_iter().take(crate::bulk::BULK_IMPORT_MAX_FILES) {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if !matches!(
            ext.as_str(),
            "gb" | "gbk" | "genbank" | "fa" | "fasta" | "fna" | "ffn"
        ) {
            continue;
        }
        let label = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("read")
            .to_owned();
        match crate::load_path(&path) {
            Ok(rec) => match pairwise_align(&rec.sequence, target, AlignMode::Global) {
                Ok(result) => rows.push(BulkAlignRow {
                    label,
                    result: Some(result),
                    error: None,
                }),
                Err(e) => rows.push(BulkAlignRow {
                    label,
                    result: None,
                    error: Some(e.to_string()),
                }),
            },
            Err(e) => rows.push(BulkAlignRow {
                label,
                result: None,
                error: Some(e.to_string()),
            }),
        }
    }
    rows
}

/// Arbitrary-width bit vector for Myers (Python int equivalent).
#[derive(Clone, Debug)]
struct Bits {
    words: Vec<u64>,
    bits: usize,
}

impl Bits {
    fn zero(bits: usize) -> Self {
        let n = bits.div_ceil(64).max(1);
        Self {
            words: vec![0; n],
            bits,
        }
    }

    fn ones(bits: usize) -> Self {
        let mut b = Self::zero(bits);
        if bits == 0 {
            return b;
        }
        let full = bits / 64;
        for w in b.words.iter_mut().take(full) {
            *w = u64::MAX;
        }
        let rem = bits % 64;
        if rem > 0 {
            b.words[full] = (1u64 << rem) - 1;
        }
        b
    }

    fn set_bit(&mut self, j: usize) {
        if j >= self.bits {
            return;
        }
        self.words[j / 64] |= 1u64 << (j % 64);
    }

    fn test(&self, j: usize) -> bool {
        if j >= self.bits {
            return false;
        }
        (self.words[j / 64] >> (j % 64)) & 1 == 1
    }

    fn mask_in_place(&mut self) {
        let nwords = self.bits.div_ceil(64).max(1);
        self.words.resize(nwords, 0);
        if self.bits == 0 {
            self.words.fill(0);
            return;
        }
        let rem = self.bits % 64;
        if rem != 0
            && let Some(last) = self.words.last_mut()
        {
            *last &= (1u64 << rem) - 1;
        }
    }

    fn or(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (a, b) in out.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
        out
    }

    fn and(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (a, b) in out.words.iter_mut().zip(&other.words) {
            *a &= *b;
        }
        out
    }

    fn xor(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (a, b) in out.words.iter_mut().zip(&other.words) {
            *a ^= *b;
        }
        out
    }

    fn not_and_mask(&self) -> Self {
        let mut out = self.clone();
        for w in &mut out.words {
            *w = !*w;
        }
        out.mask_in_place();
        out
    }

    fn add(&self, other: &Self) -> Self {
        let mut out = Self::zero(self.bits);
        let mut carry = 0u64;
        for i in 0..self.words.len() {
            let a = self.words[i];
            let b = other.words.get(i).copied().unwrap_or(0);
            let (s1, c1) = a.overflowing_add(b);
            let (s2, c2) = s1.overflowing_add(carry);
            out.words[i] = s2;
            carry = u64::from(c1) + u64::from(c2);
        }
        out.mask_in_place();
        out
    }

    fn shl1(&self) -> Self {
        let mut out = Self::zero(self.bits);
        let mut carry = 0u64;
        for i in 0..self.words.len() {
            let w = self.words[i];
            out.words[i] = (w << 1) | carry;
            carry = w >> 63;
        }
        out.mask_in_place();
        out
    }

    fn shl1_set_low(&self) -> Self {
        let mut out = self.shl1();
        if !out.words.is_empty() {
            out.words[0] |= 1;
        }
        out.mask_in_place();
        out
    }
}
