//! The Calm icons (16 × 16, stroked with round caps, as in the approved
//! prototype) as drawing operations, and the small SVG path reader they are
//! written in. GTK-free: the GUI draws the operations with cairo in the
//! widget's own text colour, so an icon follows light and dark and takes a
//! path's colour inside a chip.
//!
//! Only the shapes the icons use are read: `M L H V C S Q T A Z` (absolute
//! and relative), circles and rounded rectangles. Elliptical arcs become
//! cubic Béziers.

/// One drawing step, in icon units (the icon's view box).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Move(f64, f64),
    Line(f64, f64),
    Curve(f64, f64, f64, f64, f64, f64),
    Close,
}

/// A shape: its outline and whether it is filled (else stroked).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub ops: Vec<Op>,
    pub fill: bool,
}

/// An icon: shapes in a square view box of `size` units, stroked `stroke`
/// units wide.
#[derive(Debug, Clone, PartialEq)]
pub struct IconDef {
    pub size: f64,
    pub stroke: f64,
    pub shapes: Vec<Shape>,
}

/// The icons the app uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Lan,
    Nearby,
    Tor,
    Direct,
    Relay,
    Friend,
    Shield,
    ShieldCheck,
    Net,
    Lock,
    Gear,
    Words,
    Route,
    Hourglass,
    Check,
    Search,
    Plus,
    Send,
    Appearance,
    Updates,
    About,
    Sidebar,
    Menu,
    Back,
    Down,
}

/// Every icon, for tests and previews.
pub const ALL: [Icon; 25] = [
    Icon::Lan,
    Icon::Nearby,
    Icon::Tor,
    Icon::Direct,
    Icon::Relay,
    Icon::Friend,
    Icon::Shield,
    Icon::ShieldCheck,
    Icon::Net,
    Icon::Lock,
    Icon::Gear,
    Icon::Words,
    Icon::Route,
    Icon::Hourglass,
    Icon::Check,
    Icon::Search,
    Icon::Plus,
    Icon::Send,
    Icon::Appearance,
    Icon::Updates,
    Icon::About,
    Icon::Sidebar,
    Icon::Menu,
    Icon::Back,
    Icon::Down,
];

/// A piece of an icon as written in the prototype's SVG.
enum Part {
    /// `<path d=…>` (stroked).
    Path(&'static str),
    /// `<path d=…>` filled with the text colour.
    FillPath(&'static str),
    /// `<circle cx cy r>` (stroked).
    Circle(f64, f64, f64),
    /// A filled `<circle>`.
    Dot(f64, f64, f64),
    /// `<rect x y width height rx>` (stroked).
    Rect(f64, f64, f64, f64, f64),
}

fn parts(icon: Icon) -> (f64, f64, Vec<Part>) {
    use Part::*;
    let p16 = |v: Vec<Part>| (16.0, 1.6, v);
    match icon {
        Icon::Lan => p16(vec![
            Path("M2 6.2a8.6 8.6 0 0 1 12 0M4.2 8.6a5.4 5.4 0 0 1 7.6 0M6.4 11a2.3 2.3 0 0 1 3.2 0"),
            Dot(8.0, 13.0, 0.6),
        ]),
        Icon::Nearby => p16(vec![Path("M4.5 5 11 10.5 8 13V3l3 2.5L4.5 11")]),
        Icon::Tor => p16(vec![
            Circle(8.0, 9.0, 5.2),
            Path("M8 3.8c-1.9 1.4-2.9 3.2-2.9 5.2S6.1 12.6 8 14.2M8 3.8c1.9 1.4 2.9 3.2 2.9 5.2S9.9 12.6 8 14.2M8 3.8V2"),
        ]),
        Icon::Direct => p16(vec![
            Circle(8.0, 8.0, 5.8),
            Path("M2.2 8h11.6M8 2.2c1.7 1.6 2.5 3.5 2.5 5.8s-.8 4.2-2.5 5.8C6.3 12.2 5.5 10.3 5.5 8S6.3 3.8 8 2.2"),
        ]),
        Icon::Relay => p16(vec![
            Circle(2.8, 8.0, 1.5),
            Circle(13.2, 8.0, 1.5),
            Rect(6.0, 5.5, 4.0, 5.0, 1.2),
            Path("M4.3 8H6M10 8h1.7"),
        ]),
        Icon::Friend => p16(vec![
            Rect(2.2, 4.0, 11.6, 8.4, 1.6),
            Path("m2.6 4.6 5.4 4.2 5.4-4.2"),
        ]),
        Icon::Shield => p16(vec![Path("M8 1.8 3 3.7v4c0 3 2.1 5.3 5 6.5 2.9-1.2 5-3.5 5-6.5v-4z")]),
        Icon::ShieldCheck => p16(vec![
            Path("M8 1.8 3 3.7v4c0 3 2.1 5.3 5 6.5 2.9-1.2 5-3.5 5-6.5v-4z"),
            Path("m5.8 8 1.6 1.6 3-3.2"),
        ]),
        Icon::Net => p16(vec![
            Circle(8.0, 3.5, 1.7),
            Circle(3.2, 12.0, 1.7),
            Circle(12.8, 12.0, 1.7),
            Path("m7.1 5-3 5.5M8.9 5l3 5.5M5 12h6"),
        ]),
        Icon::Lock => p16(vec![
            Rect(3.5, 7.0, 9.0, 6.5, 1.5),
            Path("M5.5 7V5a2.5 2.5 0 0 1 5 0v2"),
        ]),
        Icon::Gear => p16(vec![
            Circle(8.0, 8.0, 2.2),
            Path("M8 1.8v2M8 12.2v2M1.8 8h2M12.2 8h2M3.6 3.6 5 5M11 11l1.4 1.4M3.6 12.4 5 11M11 5l1.4-1.4"),
        ]),
        Icon::Words => p16(vec![Path("M2.5 4h11M2.5 8h7M2.5 12h9")]),
        Icon::Route => p16(vec![
            Circle(3.5, 12.5, 1.6),
            Circle(12.5, 3.5, 1.6),
            Path("M5 12.5h3.5a2.5 2.5 0 0 0 0-5h-1a2.5 2.5 0 0 1 0-5H11"),
        ]),
        Icon::Hourglass => p16(vec![Path(
            "M4 2h8M4 14h8M5 2c0 3 3 4 3 6s-3 3-3 6M11 2c0 3-3 4-3 6s3 3 3 6",
        )]),
        Icon::Check => (12.0, 2.0, vec![Path("m2.6 6.2 2.2 2.2 4.6-4.8")]),
        Icon::Search => p16(vec![Circle(7.0, 7.0, 4.5), Path("M10.5 10.5 14 14")]),
        Icon::Plus => p16(vec![Path("M8 3v10M3 8h10")]),
        Icon::Send => p16(vec![Path("M8 13V3M3.5 7.5 8 3l4.5 4.5")]),
        Icon::Appearance => p16(vec![
            Circle(8.0, 8.0, 5.5),
            FillPath("M8 2.5v11a5.5 5.5 0 0 0 0-11z"),
        ]),
        Icon::Updates => p16(vec![
            Circle(8.0, 8.0, 5.8),
            Path("M8 4.8v6M5.4 8.6 8 11.2l2.6-2.6"),
        ]),
        Icon::About => p16(vec![Circle(8.0, 8.0, 5.8), Path("M8 7.4v4M8 5h0")]),
        Icon::Sidebar => p16(vec![Rect(2.0, 3.0, 12.0, 10.0, 2.0), Path("M6.5 3v10")]),
        Icon::Menu => p16(vec![Path("M3 4.5h10M3 8h10M3 11.5h10")]),
        Icon::Back => p16(vec![Path("M10 3.5 5.5 8l4.5 4.5")]),
        Icon::Down => p16(vec![Path("M8 3v10M3.5 8.5 8 13l4.5-4.5")]),
    }
}

/// The icon's drawing operations.
pub fn icon(icon: Icon) -> IconDef {
    let (size, stroke, list) = parts(icon);
    let shapes = list
        .into_iter()
        .map(|p| match p {
            Part::Path(d) => Shape {
                ops: parse_path(d),
                fill: false,
            },
            Part::FillPath(d) => Shape {
                ops: parse_path(d),
                fill: true,
            },
            Part::Circle(cx, cy, r) => Shape {
                ops: circle(cx, cy, r),
                fill: false,
            },
            Part::Dot(cx, cy, r) => Shape {
                ops: circle(cx, cy, r),
                fill: true,
            },
            Part::Rect(x, y, w, h, r) => Shape {
                ops: rounded_rect(x, y, w, h, r),
                fill: false,
            },
        })
        .collect();
    IconDef {
        size,
        stroke,
        shapes,
    }
}

/// A full circle as four cubic arcs.
pub fn circle(cx: f64, cy: f64, r: f64) -> Vec<Op> {
    let k = 0.552_284_749_831 * r;
    vec![
        Op::Move(cx + r, cy),
        Op::Curve(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
        Op::Curve(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
        Op::Curve(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
        Op::Curve(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
        Op::Close,
    ]
}

/// A rectangle with rounded corners.
pub fn rounded_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> Vec<Op> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_749_831 * r;
    let (x2, y2) = (x + w, y + h);
    vec![
        Op::Move(x + r, y),
        Op::Line(x2 - r, y),
        Op::Curve(x2 - r + k, y, x2, y + r - k, x2, y + r),
        Op::Line(x2, y2 - r),
        Op::Curve(x2, y2 - r + k, x2 - r + k, y2, x2 - r, y2),
        Op::Line(x + r, y2),
        Op::Curve(x + r - k, y2, x, y2 - r + k, x, y2 - r),
        Op::Line(x, y + r),
        Op::Curve(x, y + r - k, x + r - k, y, x + r, y),
        Op::Close,
    ]
}

/// Numbers and command letters of a path, in order.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Cmd(char),
    Num(f64),
}

fn tokenize(d: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            out.push(Tok::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            let start = i;
            let mut seen_dot = false;
            let mut seen_exp = false;
            if c == '-' || c == '+' {
                i += 1;
            }
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot && !seen_exp {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && !seen_exp {
                    seen_exp = true;
                    i += 1;
                    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(v) = d[start..i].parse::<f64>() {
                out.push(Tok::Num(v));
            }
        } else {
            i += 1; // spaces and commas
        }
    }
    out
}

/// Read an SVG path (`d` attribute) into drawing operations.
pub fn parse_path(d: &str) -> Vec<Op> {
    let toks = tokenize(d);
    let mut ops = Vec::new();
    let mut i = 0;
    let (mut x, mut y) = (0.0_f64, 0.0_f64);
    let (mut sx, mut sy) = (0.0_f64, 0.0_f64);
    // The last control point, for S and T.
    let mut last_c: Option<(f64, f64)> = None;
    let mut last_q: Option<(f64, f64)> = None;
    let mut cmd = 'M';
    let num = |i: &mut usize| -> Option<f64> {
        match toks.get(*i) {
            Some(Tok::Num(v)) => {
                *i += 1;
                Some(*v)
            }
            _ => None,
        }
    };
    while i < toks.len() {
        if let Tok::Cmd(c) = toks[i] {
            cmd = c;
            i += 1;
            if c == 'Z' || c == 'z' {
                ops.push(Op::Close);
                x = sx;
                y = sy;
                last_c = None;
                last_q = None;
                continue;
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        let before = i;
        match cmd.to_ascii_uppercase() {
            'M' => {
                let (Some(a), Some(b)) = (num(&mut i), num(&mut i)) else {
                    break;
                };
                x = ox + a;
                y = oy + b;
                sx = x;
                sy = y;
                ops.push(Op::Move(x, y));
                // Pairs after a moveto are linetos.
                cmd = if rel { 'l' } else { 'L' };
                last_c = None;
                last_q = None;
            }
            'L' => {
                let (Some(a), Some(b)) = (num(&mut i), num(&mut i)) else {
                    break;
                };
                x = ox + a;
                y = oy + b;
                ops.push(Op::Line(x, y));
                last_c = None;
                last_q = None;
            }
            'H' => {
                let Some(a) = num(&mut i) else { break };
                x = ox + a;
                ops.push(Op::Line(x, y));
                last_c = None;
                last_q = None;
            }
            'V' => {
                let Some(b) = num(&mut i) else { break };
                y = oy + b;
                ops.push(Op::Line(x, y));
                last_c = None;
                last_q = None;
            }
            'C' => {
                let v: Vec<f64> = (0..6).filter_map(|_| num(&mut i)).collect();
                if v.len() < 6 {
                    break;
                }
                let (x1, y1, x2, y2) = (ox + v[0], oy + v[1], ox + v[2], oy + v[3]);
                x = ox + v[4];
                y = oy + v[5];
                ops.push(Op::Curve(x1, y1, x2, y2, x, y));
                last_c = Some((x2, y2));
                last_q = None;
            }
            'S' => {
                let v: Vec<f64> = (0..4).filter_map(|_| num(&mut i)).collect();
                if v.len() < 4 {
                    break;
                }
                let (x1, y1) = match last_c {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                let (x2, y2) = (ox + v[0], oy + v[1]);
                x = ox + v[2];
                y = oy + v[3];
                ops.push(Op::Curve(x1, y1, x2, y2, x, y));
                last_c = Some((x2, y2));
                last_q = None;
            }
            'Q' => {
                let v: Vec<f64> = (0..4).filter_map(|_| num(&mut i)).collect();
                if v.len() < 4 {
                    break;
                }
                let (qx, qy) = (ox + v[0], oy + v[1]);
                let (ex, ey) = (ox + v[2], oy + v[3]);
                ops.push(quad(x, y, qx, qy, ex, ey));
                x = ex;
                y = ey;
                last_q = Some((qx, qy));
                last_c = None;
            }
            'T' => {
                let v: Vec<f64> = (0..2).filter_map(|_| num(&mut i)).collect();
                if v.len() < 2 {
                    break;
                }
                let (qx, qy) = match last_q {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                let (ex, ey) = (ox + v[0], oy + v[1]);
                ops.push(quad(x, y, qx, qy, ex, ey));
                x = ex;
                y = ey;
                last_q = Some((qx, qy));
                last_c = None;
            }
            'A' => {
                let v: Vec<f64> = (0..7).filter_map(|_| num(&mut i)).collect();
                if v.len() < 7 {
                    break;
                }
                let (ex, ey) = (ox + v[5], oy + v[6]);
                ops.extend(arc(
                    x,
                    y,
                    v[0],
                    v[1],
                    v[2],
                    v[3] != 0.0,
                    v[4] != 0.0,
                    ex,
                    ey,
                ));
                x = ex;
                y = ey;
                last_c = None;
                last_q = None;
            }
            _ => {
                i += 1;
            }
        }
        if i == before {
            break; // a malformed path: stop rather than loop
        }
    }
    ops
}

/// A quadratic Bézier as a cubic.
fn quad(x0: f64, y0: f64, qx: f64, qy: f64, x: f64, y: f64) -> Op {
    Op::Curve(
        x0 + 2.0 / 3.0 * (qx - x0),
        y0 + 2.0 / 3.0 * (qy - y0),
        x + 2.0 / 3.0 * (qx - x),
        y + 2.0 / 3.0 * (qy - y),
        x,
        y,
    )
}

/// An SVG elliptical arc (endpoint form) as cubic Béziers of at most 90°
/// each (SVG 1.1, appendix F.6.5 and F.6.6).
#[allow(clippy::too_many_arguments)]
fn arc(
    x1: f64,
    y1: f64,
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    large: bool,
    sweep: bool,
    x2: f64,
    y2: f64,
) -> Vec<Op> {
    if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        return Vec::new();
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-9 || ry < 1e-9 {
        return vec![Op::Line(x2, y2)];
    }
    let phi = rotation_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    // Radii too small: scale them up (F.6.6).
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    let tau = std::f64::consts::TAU;
    if !sweep && delta > 0.0 {
        delta -= tau;
    } else if sweep && delta < 0.0 {
        delta += tau;
    }
    let n = (delta.abs() / (std::f64::consts::FRAC_PI_2))
        .ceil()
        .max(1.0) as usize;
    let step = delta / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            cx + rx * c * cos_phi - ry * s * sin_phi,
            cy + rx * c * sin_phi + ry * s * cos_phi,
        )
    };
    let deriv = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            -rx * s * cos_phi - ry * c * sin_phi,
            -rx * s * sin_phi + ry * c * cos_phi,
        )
    };
    let mut ops = Vec::with_capacity(n);
    let mut t = theta1;
    for seg in 0..n {
        let t2 = t + step;
        let (px, py) = point(t);
        let (dx1, dy1) = deriv(t);
        let (qx, qy) = if seg + 1 == n { (x2, y2) } else { point(t2) };
        let (dx2, dy2) = deriv(t2);
        ops.push(Op::Curve(
            px + k * dx1,
            py + k * dy1,
            qx - k * dx2,
            qy - k * dy2,
            qx,
            qy,
        ));
        t = t2;
    }
    ops
}

/// The end points of every operation (for bounds and tests).
pub fn points(ops: &[Op]) -> Vec<(f64, f64)> {
    ops.iter()
        .filter_map(|op| match *op {
            Op::Move(x, y) | Op::Line(x, y) | Op::Curve(_, _, _, _, x, y) => Some((x, y)),
            Op::Close => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn numbers_and_commands() {
        assert_eq!(
            tokenize("M2 6.2a8.6 8.6 0 0 1 12 0"),
            vec![
                Tok::Cmd('M'),
                Tok::Num(2.0),
                Tok::Num(6.2),
                Tok::Cmd('a'),
                Tok::Num(8.6),
                Tok::Num(8.6),
                Tok::Num(0.0),
                Tok::Num(0.0),
                Tok::Num(1.0),
                Tok::Num(12.0),
                Tok::Num(0.0),
            ]
        );
        // "-.8" and ".5.5" style numbers.
        assert_eq!(
            tokenize("s-.8 4.2-2.5 5.8"),
            vec![
                Tok::Cmd('s'),
                Tok::Num(-0.8),
                Tok::Num(4.2),
                Tok::Num(-2.5),
                Tok::Num(5.8)
            ]
        );
        assert_eq!(tokenize(".5.5"), vec![Tok::Num(0.5), Tok::Num(0.5)]);
        assert_eq!(tokenize("1e-1"), vec![Tok::Num(0.1)]);
    }

    #[test]
    fn lines_relative_and_implicit() {
        let ops = parse_path("M4.5 5 11 10.5 8 13V3l3 2.5L4.5 11");
        assert_eq!(ops[0], Op::Move(4.5, 5.0));
        assert_eq!(ops[1], Op::Line(11.0, 10.5), "pairs after M are lines");
        assert_eq!(ops[3], Op::Line(8.0, 3.0));
        assert!(close(points(&ops[4..5])[0], (11.0, 5.5)));
        assert_eq!(*ops.last().unwrap(), Op::Line(4.5, 11.0));
        let rel = parse_path("m2.6 4.6 5.4 4.2 5.4-4.2");
        assert!(close(points(&rel)[1], (8.0, 8.8)));
        assert!(close(points(&rel)[2], (13.4, 4.6)));
    }

    #[test]
    fn arcs_end_where_asked() {
        // A half circle from (2, 6.2) to (14, 6.2).
        let ops = parse_path("M2 6.2a8.6 8.6 0 0 1 12 0");
        assert!(ops.len() >= 2);
        let end = *points(&ops).last().unwrap();
        assert!(close(end, (14.0, 6.2)), "{end:?}");
        // The arc bulges upwards (sweep 1 from left to right).
        if let Op::Curve(_, y1, _, _, _, _) = ops[1] {
            assert!(y1 < 6.2);
        } else {
            panic!("a curve");
        }
        // A degenerate arc is nothing; zero radii are a line.
        assert!(arc(1.0, 1.0, 2.0, 2.0, 0.0, false, true, 1.0, 1.0).is_empty());
        assert_eq!(
            arc(0.0, 0.0, 0.0, 2.0, 0.0, false, true, 3.0, 0.0),
            vec![Op::Line(3.0, 0.0)]
        );
    }

    #[test]
    fn smooth_curves_mirror_their_control_point() {
        let ops = parse_path("M8 3.8c-1.9 1.4-2.9 3.2-2.9 5.2S6.1 12.6 8 14.2");
        match (ops[1], ops[2]) {
            (Op::Curve(_, _, c2x, c2y, x, y), Op::Curve(m1x, m1y, _, _, ex, ey)) => {
                assert!(close((m1x, m1y), (2.0 * x - c2x, 2.0 * y - c2y)));
                assert!(close((ex, ey), (8.0, 14.2)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_icon_stays_in_its_box() {
        for i in ALL {
            let def = icon(i);
            assert!(!def.shapes.is_empty(), "{i:?}");
            for s in &def.shapes {
                assert!(!s.ops.is_empty(), "{i:?}");
                assert!(matches!(s.ops[0], Op::Move(..)), "{i:?} starts with a move");
                for (x, y) in points(&s.ops) {
                    assert!(
                        (-0.5..=def.size + 0.5).contains(&x)
                            && (-0.5..=def.size + 0.5).contains(&y),
                        "{i:?}: ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn shapes() {
        let c = circle(8.0, 8.0, 2.0);
        assert_eq!(c[0], Op::Move(10.0, 8.0));
        assert!(close(points(&c)[2], (6.0, 8.0)));
        let r = rounded_rect(0.0, 0.0, 10.0, 4.0, 3.0);
        // The radius is capped at half the height.
        assert_eq!(r[0], Op::Move(2.0, 0.0));
        assert_eq!(parse_path(""), Vec::new());
        assert_eq!(parse_path("M1 1 Z"), vec![Op::Move(1.0, 1.0), Op::Close]);
        // Garbage ends the path instead of looping.
        assert_eq!(parse_path("M1"), Vec::new());
    }
}
