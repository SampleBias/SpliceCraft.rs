//! Standard agarose-gel size ladders (largest first).

/// Named ladders: NEB-style 1 kb Plus / 1 kb / 100 bp and Lambda/HindIII.
pub const GEL_LADDERS: &[(&str, &[u32])] = &[
    (
        "1 kb Plus",
        &[
            15000, 10000, 7000, 5000, 4000, 3000, 2000, 1500, 1000, 850, 650, 500, 400, 300, 200,
            100,
        ],
    ),
    (
        "1 kb",
        &[
            10000, 8000, 6000, 5000, 4000, 3000, 2500, 2000, 1500, 1000, 750, 500, 250,
        ],
    ),
    (
        "100 bp",
        &[
            1517, 1200, 1000, 900, 800, 700, 600, 500, 400, 300, 200, 100,
        ],
    ),
    (
        "Lambda/HindIII",
        &[23130, 9416, 6557, 4361, 2322, 2027, 564, 125],
    ),
];

/// Ladder display names in declaration order.
#[must_use]
pub fn ladder_names() -> Vec<&'static str> {
    GEL_LADDERS.iter().map(|(n, _)| *n).collect()
}

/// Bands for `name`, or the first ladder when unknown.
#[must_use]
pub fn ladder_bands(name: &str) -> &'static [u32] {
    GEL_LADDERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
        .unwrap_or(GEL_LADDERS[0].1)
}
