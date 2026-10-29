//! The Raven mark as dots: the braille drawing in
//! `desktop/design/assets/raven-logo-braille.txt` (embedded), one dot per
//! raised braille point, laid out like the approved prototype's mark (cells
//! 24 units wide and 40 high, two dot columns 15 apart, four dot rows 9
//! apart). The GUI draws them with cairo in the text colour, at any size,
//! in light and dark; setup reveals them while the identity is being made.

use std::sync::OnceLock;

/// The braille drawing of the mark.
pub const MARK_BRAILLE: &str = include_str!("../core/desktop/design/assets/raven-logo-braille.txt");

/// Dot positions of the mark in its own units, plus its size.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub dots: Vec<(f64, f64)>,
    pub width: f64,
    pub height: f64,
    /// The dot radius in the same units.
    pub radius: f64,
}

/// Braille bit -> (column, row) inside a cell (Unicode braille patterns).
const BITS: [(u32, u32, u32); 8] = [
    (0x01, 0, 0),
    (0x02, 0, 1),
    (0x04, 0, 2),
    (0x08, 1, 0),
    (0x10, 1, 1),
    (0x20, 1, 2),
    (0x40, 0, 3),
    (0x80, 1, 3),
];

/// The dots of a braille drawing.
pub fn dots_of(braille: &str) -> Mark {
    let mut dots = Vec::new();
    for (row, line) in braille.lines().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let code = ch as u32;
            if !(0x2800..=0x28FF).contains(&code) {
                continue;
            }
            let bits = code - 0x2800;
            for (bit, dx, dy) in BITS {
                if bits & bit != 0 {
                    dots.push((
                        col as f64 * 24.0 + 4.0 + 15.0 * dx as f64,
                        row as f64 * 40.0 - 5.0 + 9.0 * dy as f64,
                    ));
                }
            }
        }
    }
    // Trim to the dots' own box (with room for the dot radius).
    let radius = 10.0;
    let min_x = dots.iter().map(|d| d.0).fold(f64::INFINITY, f64::min);
    let min_y = dots.iter().map(|d| d.1).fold(f64::INFINITY, f64::min);
    let max_x = dots.iter().map(|d| d.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = dots.iter().map(|d| d.1).fold(f64::NEG_INFINITY, f64::max);
    if dots.is_empty() {
        return Mark {
            dots,
            width: 1.0,
            height: 1.0,
            radius,
        };
    }
    for d in &mut dots {
        d.0 -= min_x - radius;
        d.1 -= min_y - radius;
    }
    Mark {
        dots,
        width: max_x - min_x + 2.0 * radius,
        height: max_y - min_y + 2.0 * radius,
        radius,
    }
}

/// The Raven mark (parsed once).
pub fn mark() -> &'static Mark {
    static MARK: OnceLock<Mark> = OnceLock::new();
    MARK.get_or_init(|| dots_of(MARK_BRAILLE))
}

/// The order in which setup reveals the dots: a fixed scatter (the same
/// on every run), so the mark assembles evenly rather than line by line.
pub fn reveal_order(n: usize) -> Vec<usize> {
    let mut keyed: Vec<(u64, usize)> = (0..n)
        .map(|i| {
            // A small integer hash (splitmix64) of the index.
            let mut z = (i as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31), i)
        })
        .collect();
    keyed.sort_unstable();
    keyed.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_has_its_dots() {
        let m = mark();
        // One dot per raised braille point of the asset.
        let raised: u32 = MARK_BRAILLE
            .chars()
            .filter(|c| (0x2800..=0x28FF).contains(&(*c as u32)))
            .map(|c| (c as u32 - 0x2800).count_ones())
            .sum();
        assert_eq!(m.dots.len() as u32, raised);
        assert!(m.dots.len() > 1000, "{}", m.dots.len());
        // A little wider than high, like the logo.
        assert!(
            m.width > m.height && m.width < 1.3 * m.height,
            "{} x {}",
            m.width,
            m.height
        );
        for (x, y) in &m.dots {
            assert!(*x >= m.radius - 1e-9 && *x <= m.width - m.radius + 1e-9);
            assert!(*y >= m.radius - 1e-9 && *y <= m.height - m.radius + 1e-9);
        }
    }

    #[test]
    fn braille_cells() {
        // ⠁ is dot 1 (top left), ⢀ is dot 8 (bottom right).
        let m = dots_of("\u{2801}\u{2880}");
        assert_eq!(m.dots.len(), 2);
        let (a, b) = (m.dots[0], m.dots[1]);
        assert_eq!(b.0 - a.0, 24.0 + 15.0);
        assert_eq!(b.1 - a.1, 27.0);
        assert_eq!(dots_of("abc").dots.len(), 0);
    }

    #[test]
    fn reveal_order_is_a_fixed_permutation() {
        let o = reveal_order(50);
        let mut sorted = o.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_eq!(o, reveal_order(50));
        assert_ne!(o, (0..50).collect::<Vec<_>>());
    }
}
