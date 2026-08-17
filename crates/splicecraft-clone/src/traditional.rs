//! Two-enzyme directional ligation. Both orientations; refuse silent drops.

use crate::fragment::{
    Fragment, close_circular, ends_compatible, label_disrupted_split_features, ligate_fragments,
    rc_fragment,
};

/// One orientation of a traditional clone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrientationProduct {
    /// Concatenated top strand (vector + insert).
    pub top_seq: String,
    /// Vector features plus shifted insert features.
    pub features: Vec<crate::fragment::FragFeature>,
    /// True when both junctions actually ligate and the circle closes.
    pub compatible: bool,
}

/// Result of trying both insert orientations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraditionalResult {
    /// Vector + insert as supplied.
    pub forward: OrientationProduct,
    /// Vector + reverse-complement insert.
    pub reverse: OrientationProduct,
    /// Directional / ambiguous notes.
    pub warnings: Vec<String>,
    /// Neither orientation ligates.
    pub errors: Vec<String>,
}

/// Ligate `insert` into `vector` in both orientations.
#[must_use]
pub fn simulate_traditional_cloning(insert: &Fragment, vector: &Fragment) -> TraditionalResult {
    let insert_rc = rc_fragment(insert);
    let fwd_linear = ligate_fragments(vector, insert);
    let rev_linear = ligate_fragments(vector, &insert_rc);
    let fwd_compat = fwd_linear
        .as_ref()
        .is_some_and(|f| ends_compatible(&f.right, &f.left));
    let rev_compat = rev_linear
        .as_ref()
        .is_some_and(|f| ends_compatible(&f.right, &f.left));
    let fwd_seq = fwd_linear
        .as_ref()
        .map(|f| f.top_seq.clone())
        .unwrap_or_else(|| format!("{}{}", vector.top_seq, insert.top_seq));
    let rev_seq = rev_linear
        .as_ref()
        .map(|f| f.top_seq.clone())
        .unwrap_or_else(|| format!("{}{}", vector.top_seq, insert_rc.top_seq));

    let junction_enz: Vec<&str> = [
        insert.left.enzyme.as_str(),
        insert.right.enzyme.as_str(),
        vector.left.enzyme.as_str(),
        vector.right.enzyme.as_str(),
    ]
    .into_iter()
    .filter(|e| !e.is_empty())
    .collect();

    let mut fwd_feats = vector.features.clone();
    let shift = vector.top_seq.len();
    for f in &insert.features {
        let mut g = f.clone();
        g.start += shift;
        g.end += shift;
        fwd_feats.push(g);
    }
    let mut rev_feats = vector.features.clone();
    for f in &insert_rc.features {
        let mut g = f.clone();
        g.start += shift;
        g.end += shift;
        rev_feats.push(g);
    }
    label_disrupted_split_features(&mut fwd_feats, &junction_enz);
    label_disrupted_split_features(&mut rev_feats, &junction_enz);

    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    if fwd_compat && rev_compat {
        warnings.push(
            "Ambiguous orientation: both forward and reverse ligation are chemically compatible. \
             The cloning reaction will yield a mixture; pick by sequencing."
                .into(),
        );
    } else if fwd_compat && !rev_compat {
        warnings.push(
            "Directional cloning: only the forward orientation is biologically achievable. \
             Reverse is rendered for reference but cannot ligate."
                .into(),
        );
    } else if rev_compat && !fwd_compat {
        warnings.push(
            "Directional cloning: only the reverse orientation is biologically achievable. \
             Forward is rendered for reference but cannot ligate."
                .into(),
        );
    } else {
        errors.push(
            "Neither orientation has matching overhangs at both junctions. \
             Check that the insert and vector were cut with the same enzyme(s)."
                .into(),
        );
    }

    TraditionalResult {
        forward: OrientationProduct {
            top_seq: fwd_seq,
            features: fwd_feats,
            compatible: fwd_compat,
        },
        reverse: OrientationProduct {
            top_seq: rev_seq,
            features: rev_feats,
            compatible: rev_compat,
        },
        warnings,
        errors,
    }
}

/// Compatible orientation closed into a circle, or `None` if that orientation cannot ligate.
#[must_use]
pub fn traditional_closed(
    insert: &Fragment,
    vector: &Fragment,
    reverse: bool,
) -> Option<crate::fragment::ClosedProduct> {
    let insert = if reverse {
        rc_fragment(insert)
    } else {
        insert.clone()
    };
    let linear = ligate_fragments(vector, &insert)?;
    close_circular(&linear)
}
