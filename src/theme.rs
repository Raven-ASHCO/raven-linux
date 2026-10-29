//! Design tokens (`desktop/design/tokens.json`, version 2: the Calm design,
//! embedded) and the GTK CSS generated from them: one stylesheet per colour
//! scheme, with or without motion.
//!
//! Colours become `@define-color raven_<name>` (camelCase names turned into
//! snake_case); the style classes use only those names plus the spacing,
//! radius, layout, shadow, type and motion tokens. Font sizes are in `rem`,
//! relative to the system font (Calm's body 14 = 1rem), so the user's font
//! size and scaling are kept.
//!
//! GTK 4 has no CSS variables and no `color-mix()` before 4.16, so tints are
//! `alpha(@colour, n)` and every path colour gets its own pair of rules.
//! Under reduced motion the stylesheet carries no `animation` or `transition`
//! declaration at all and ends with a rule that stops the theme's own.

use serde_json::Value;
use std::fmt::Write as _;
use std::sync::OnceLock;

pub const TOKENS_JSON: &str = include_str!("../core/desktop/design/tokens.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scheme {
    Light,
    Dark,
}

/// An sRGB colour with alpha, parsed from `#RRGGBB` or `#RRGGBBAA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn parse(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#')?;
        if !(h.len() == 6 || h.len() == 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if h.len() == 8 { byte(6)? } else { 255 },
        })
    }

    /// CSS text: `#rrggbb` when opaque, otherwise `rgba(r, g, b, a)`.
    pub fn css(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            let a = (self.a as f64 / 255.0 * 1000.0).round() / 1000.0;
            format!("rgba({}, {}, {}, {})", self.r, self.g, self.b, a)
        }
    }

    /// Channels as 0.0..=1.0 for cairo.
    pub fn to_f64(self) -> (f64, f64, f64, f64) {
        (
            self.r as f64 / 255.0,
            self.g as f64 / 255.0,
            self.b as f64 / 255.0,
            self.a as f64 / 255.0,
        )
    }

    /// The same colour with its alpha multiplied by `f` (0..1).
    pub fn with_alpha(self, f: f64) -> Self {
        Self {
            a: (self.a as f64 * f.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }

    /// Linear mix towards `other` (0 = self, 1 = other), alpha included.
    pub fn mix(self, other: Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        Self {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: m(self.a, other.a),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    pub light: Rgba,
    pub dark: Rgba,
}

impl Pair {
    pub fn get(&self, scheme: Scheme) -> Rgba {
        match scheme {
            Scheme::Light => self.light,
            Scheme::Dark => self.dark,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarColors {
    pub bg: Pair,
    pub fg: Pair,
}

/// One shadow layer (offsets and blur in points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayer {
    pub y: f64,
    pub blur: f64,
    pub color: Rgba,
}

/// A shadow for both schemes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shadow {
    pub light: Vec<ShadowLayer>,
    pub dark: Vec<ShadowLayer>,
}

impl Shadow {
    /// `box-shadow` text: every layer, comma separated.
    pub fn css(&self, scheme: Scheme) -> String {
        let layers = match scheme {
            Scheme::Light => &self.light,
            Scheme::Dark => &self.dark,
        };
        if layers.is_empty() {
            return "none".into();
        }
        layers
            .iter()
            .map(|l| {
                format!(
                    "0 {}px {}px {}",
                    num_css(l.y),
                    num_css(l.blur),
                    l.color.css()
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// A step of the type ramp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    /// Size in points (body = 14).
    pub size: f64,
    /// CSS weight: regular 400, medium 500, semibold 600, bold 700.
    pub weight: u16,
    /// Letter spacing in em (0 when the token has none).
    pub tracking: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeRamp {
    pub body: Font,
    pub bubble: Font,
    pub label: Font,
    pub caption: Font,
    pub meta: Font,
    pub small: Font,
    pub section_label: Font,
    pub title: Font,
    pub brand: Font,
    pub page_title: Font,
    pub display: Font,
    pub mono: Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Durations {
    pub bubble_in: u32,
    pub route_packet: u32,
    pub receipt_back: u32,
    pub heartbeat_pulse: u32,
    pub status_check_interval: u32,
    pub presence_breathe: u32,
    pub retry_ring: u32,
    pub logo_reveal: u32,
    pub word_reveal: u32,
    pub read_aloud_step: u32,
    pub toast_visible: u32,
}

/// The Calm motion durations (`motion.calmMs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalmDurations {
    pub bubble_lift: u32,
    pub check_pop: u32,
    pub tick_ripple: u32,
    pub chip_slide: u32,
    pub group_arc: u32,
    pub view_in: u32,
    pub section_out: u32,
    pub section_in: u32,
    pub settings_glide: u32,
    pub segment_thumb: u32,
    pub switch_knob: u32,
    pub switch_color: u32,
    pub radio_dot: u32,
    pub sheet_in: u32,
    pub dialog_in: u32,
    pub scrim_fade: u32,
    pub toast_in: u32,
    pub presence_blink: u32,
    pub incoming_highlight: u32,
    pub lock_shackle: u32,
    pub locked_fade: u32,
    pub unlock_fade: u32,
    pub shake: u32,
    pub hover: u32,
    pub press: u32,
    pub radar_pulse: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radii {
    pub control: u32,
    pub card: u32,
    pub group: u32,
    pub row: u32,
    pub bubble: u32,
    pub bubble_tail: u32,
    pub chip: u32,
    pub chip_small: u32,
    pub badge: u32,
    pub composer: u32,
    pub sheet: u32,
    pub dialog: u32,
    pub segment: u32,
    pub segment_thumb: u32,
    pub window: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub sidebar_width: u32,
    pub sidebar_min_width: u32,
    pub settings_nav_width: u32,
    pub min_window: (u32, u32),
    pub header_height: u32,
    pub route_line_height: u32,
    pub avatar_size: u32,
    pub avatar_small: u32,
    pub avatar_large: u32,
    pub presence_dot: u32,
    pub content_max_width: u32,
    pub page_max_width: u32,
    pub settings_max_width: u32,
    pub bubble_max_fraction: f64,
    pub chip_height: u32,
    pub pill_height: u32,
    pub button_height: u32,
    pub send_button: u32,
    pub switch: (u32, u32, u32),
    pub group_ring: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tokens {
    /// Named colours in file order (`background`, `surface`, …).
    pub colors: Vec<(String, Pair)>,
    pub avatars: Vec<AvatarColors>,
    pub shadow_card: Shadow,
    pub shadow_bubble_out: Shadow,
    pub shadow_sheet: Shadow,
    pub fonts: TypeRamp,
    /// xs sm md lg xl xxl
    pub spacing: [u32; 6],
    pub radius: Radii,
    pub layout: Layout,
    pub durations: Durations,
    pub calm: CalmDurations,
    /// Cubic-bezier curves by name (`lift`, `pop`, `glide`, …).
    pub curves: Vec<(String, [f64; 4])>,
    // Version 1 names, kept for the screens that use them.
    pub radius_control: u32,
    pub radius_card: u32,
    pub radius_bubble: u32,
    pub radius_bubble_tail: u32,
    pub radius_chip: u32,
    pub radius_window: u32,
    pub sidebar_width: u32,
    pub min_window: (u32, u32),
    pub route_line_height: u32,
    pub avatar_size: u32,
    pub presence_dot: u32,
}

fn weight(name: &str) -> u16 {
    match name {
        "medium" => 500,
        "semibold" => 600,
        "bold" => 700,
        "light" => 300,
        _ => 400,
    }
}

impl Tokens {
    pub fn parse(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let color = v
            .get("color")
            .and_then(Value::as_object)
            .ok_or("no color object")?;
        let pair = |p: &Value| -> Result<Pair, String> {
            let get = |k: &str| {
                p.get(k)
                    .and_then(Value::as_str)
                    .and_then(Rgba::parse)
                    .ok_or_else(|| format!("bad {k} colour in {p}"))
            };
            Ok(Pair {
                light: get("light")?,
                dark: get("dark")?,
            })
        };
        let mut colors = Vec::new();
        for (name, val) in color {
            if val.get("light").is_some() {
                colors.push((name.clone(), pair(val)?));
            }
        }
        let avatars = color
            .get("avatar")
            .and_then(Value::as_array)
            .ok_or("no avatar colours")?
            .iter()
            .map(|a| {
                Ok(AvatarColors {
                    bg: pair(&a["bg"])?,
                    fg: pair(&a["fg"])?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let at = |path: &[&str]| -> Result<&Value, String> {
            let mut cur = &v;
            for p in path {
                cur = cur
                    .get(p)
                    .ok_or_else(|| format!("missing {}", path.join(".")))?;
            }
            Ok(cur)
        };
        let num = |path: &[&str]| -> Result<f64, String> {
            at(path)?
                .as_f64()
                .ok_or_else(|| format!("{} is not a number", path.join(".")))
        };
        let u = |path: &[&str]| num(path).map(|n| n.round() as u32);
        let d = |name: &str| u(&["motion", "durationsMs", name]);
        let c = |name: &str| u(&["motion", "calmMs", name]);
        let r = |name: &str| u(&["radius", name]);
        let l = |name: &str| u(&["layout", name]);
        let shadow = |name: &str| -> Result<Shadow, String> {
            let layers = |scheme: &str| -> Result<Vec<ShadowLayer>, String> {
                at(&["shadow", name, scheme])?
                    .as_array()
                    .ok_or_else(|| format!("shadow.{name}.{scheme} is not a list"))?
                    .iter()
                    .map(|layer| {
                        Ok(ShadowLayer {
                            y: layer.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                            blur: layer.get("blur").and_then(Value::as_f64).unwrap_or(0.0),
                            color: layer
                                .get("color")
                                .and_then(Value::as_str)
                                .and_then(Rgba::parse)
                                .ok_or_else(|| format!("bad colour in shadow.{name}"))?,
                        })
                    })
                    .collect()
            };
            Ok(Shadow {
                light: layers("light")?,
                dark: layers("dark")?,
            })
        };
        let font = |name: &str| -> Result<Font, String> {
            let f = at(&["typography", name])?;
            Ok(Font {
                size: f
                    .get("size")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("typography.{name}.size"))?,
                weight: weight(f.get("weight").and_then(Value::as_str).unwrap_or("")),
                tracking: f.get("tracking").and_then(Value::as_f64).unwrap_or(0.0),
            })
        };
        let curves = at(&["motion", "curves"])?
            .as_object()
            .ok_or("motion.curves is not an object")?
            .iter()
            .filter_map(|(k, v)| {
                let a = v.as_array()?;
                if a.len() != 4 {
                    return None;
                }
                let p: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
                (p.len() == 4).then(|| (k.clone(), [p[0], p[1], p[2], p[3]]))
            })
            .collect();
        let radius = Radii {
            control: r("control")?,
            card: r("card")?,
            group: r("group")?,
            row: r("row")?,
            bubble: r("bubble")?,
            bubble_tail: r("bubbleTail")?,
            chip: r("chip")?,
            chip_small: r("chipSmall")?,
            badge: r("badge")?,
            composer: r("composer")?,
            sheet: r("sheet")?,
            dialog: r("dialog")?,
            segment: r("segment")?,
            segment_thumb: r("segmentThumb")?,
            window: r("window")?,
        };
        let layout = Layout {
            sidebar_width: l("sidebarWidth")?,
            sidebar_min_width: l("sidebarMinWidth")?,
            settings_nav_width: l("settingsNavWidth")?,
            min_window: (
                u(&["layout", "minWindow", "width"])?,
                u(&["layout", "minWindow", "height"])?,
            ),
            header_height: l("headerHeight")?,
            route_line_height: l("routeLineHeight")?,
            avatar_size: l("avatarSize")?,
            avatar_small: l("avatarSmall")?,
            avatar_large: l("avatarLarge")?,
            presence_dot: l("presenceDot")?,
            content_max_width: l("contentMaxWidth")?,
            page_max_width: l("pageMaxWidth")?,
            settings_max_width: l("settingsMaxWidth")?,
            bubble_max_fraction: num(&["layout", "bubbleMaxFraction"])?,
            chip_height: l("chipHeight")?,
            pill_height: l("pillHeight")?,
            button_height: l("buttonHeight")?,
            send_button: l("sendButton")?,
            switch: (
                u(&["layout", "switch", "width"])?,
                u(&["layout", "switch", "height"])?,
                u(&["layout", "switch", "knob"])?,
            ),
            group_ring: l("groupRing")?,
        };
        Ok(Self {
            colors,
            avatars,
            shadow_card: shadow("card")?,
            shadow_bubble_out: shadow("bubbleOut")?,
            shadow_sheet: shadow("sheet")?,
            fonts: TypeRamp {
                body: font("body")?,
                bubble: font("bubble")?,
                label: font("label")?,
                caption: font("caption")?,
                meta: font("meta")?,
                small: font("small")?,
                section_label: font("sectionLabel")?,
                title: font("title")?,
                brand: font("brand")?,
                page_title: font("pageTitle")?,
                display: font("display")?,
                mono: font("mono")?,
            },
            spacing: [
                u(&["spacing", "xs"])?,
                u(&["spacing", "sm"])?,
                u(&["spacing", "md"])?,
                u(&["spacing", "lg"])?,
                u(&["spacing", "xl"])?,
                u(&["spacing", "xxl"])?,
            ],
            durations: Durations {
                bubble_in: d("bubbleIn")?,
                route_packet: d("routePacket")?,
                receipt_back: d("receiptBack")?,
                heartbeat_pulse: d("heartbeatPulse")?,
                status_check_interval: d("statusCheckInterval")?,
                presence_breathe: d("presenceBreathe")?,
                retry_ring: d("retryRing")?,
                logo_reveal: d("logoReveal")?,
                word_reveal: d("wordReveal")?,
                read_aloud_step: d("readAloudStep")?,
                toast_visible: d("toastVisible")?,
            },
            calm: CalmDurations {
                bubble_lift: c("bubbleLift")?,
                check_pop: c("checkPop")?,
                tick_ripple: c("tickRipple")?,
                chip_slide: c("chipSlide")?,
                group_arc: c("groupArc")?,
                view_in: c("viewIn")?,
                section_out: c("sectionOut")?,
                section_in: c("sectionIn")?,
                settings_glide: c("settingsGlide")?,
                segment_thumb: c("segmentThumb")?,
                switch_knob: c("switchKnob")?,
                switch_color: c("switchColor")?,
                radio_dot: c("radioDot")?,
                sheet_in: c("sheetIn")?,
                dialog_in: c("dialogIn")?,
                scrim_fade: c("scrimFade")?,
                toast_in: c("toastIn")?,
                presence_blink: c("presenceBlink")?,
                incoming_highlight: c("incomingHighlight")?,
                lock_shackle: c("lockShackle")?,
                locked_fade: c("lockedFade")?,
                unlock_fade: c("unlockFade")?,
                shake: c("shake")?,
                hover: c("hover")?,
                press: c("press")?,
                radar_pulse: c("radarPulse")?,
            },
            curves,
            radius_control: radius.control,
            radius_card: radius.card,
            radius_bubble: radius.bubble,
            radius_bubble_tail: radius.bubble_tail,
            radius_chip: radius.chip,
            radius_window: radius.window,
            sidebar_width: layout.sidebar_width,
            min_window: layout.min_window,
            route_line_height: layout.route_line_height,
            avatar_size: layout.avatar_size,
            presence_dot: layout.presence_dot,
            radius,
            layout,
        })
    }

    /// A named colour (`"accent"`, `"presenceInTouch"`, …) for a scheme.
    pub fn color(&self, name: &str, scheme: Scheme) -> Option<Rgba> {
        self.colors
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.get(scheme))
    }

    /// A named curve's control points, or ease-out when the name is unknown.
    pub fn curve(&self, name: &str) -> [f64; 4] {
        self.curves
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| *c)
            .unwrap_or([0.0, 0.0, 0.58, 1.0])
    }

    /// CSS `cubic-bezier(…)` of a named curve.
    pub fn curve_css(&self, name: &str) -> String {
        let [a, b, c, d] = self.curve(name);
        format!(
            "cubic-bezier({}, {}, {}, {})",
            num_css(a),
            num_css(b),
            num_css(c),
            num_css(d)
        )
    }
}

static TOKENS: OnceLock<Tokens> = OnceLock::new();

/// The embedded tokens (parsed on first use).
pub fn tokens() -> &'static Tokens {
    TOKENS.get_or_init(|| Tokens::parse(TOKENS_JSON).expect("desktop/design/tokens.json is valid"))
}

/// `presenceInTouch` -> `presence_in_touch`.
pub fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Avatar colour index: (sum of the bytes of the friend's Raven address)
/// mod 5, so a friend keeps the same colour on every computer.
pub fn avatar_index(address: &str) -> usize {
    let n = tokens().avatars.len().max(1);
    (address.bytes().map(|b| b as u64).sum::<u64>() % n as u64) as usize
}

/// A number for CSS: no trailing zeros ("1.5", "14", "0.04").
fn num_css(v: f64) -> String {
    let s = format!("{:.3}", (v * 1000.0).round() / 1000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// A font size relative to the system font (Calm's body = 1rem).
fn rem(size: f64, body: f64) -> String {
    format!("{}rem", num_css(size / body))
}

/// `font-size` and `font-weight` (and `letter-spacing` when set) for a step.
fn font_css(f: &Font, body: f64) -> String {
    let mut s = format!(
        "font-size: {}; font-weight: {};",
        rem(f.size, body),
        f.weight
    );
    if f.tracking != 0.0 {
        let _ = write!(s, " letter-spacing: {}em;", num_css(f.tracking));
    }
    s
}

/// Path kinds with a colour of their own (tokens' `pathRule`): the CSS class
/// suffix and the colour token.
pub const PATH_COLOURS: [(&str, &str); 6] = [
    ("lan", "pathLan"),
    ("nearby", "pathNearby"),
    ("tor", "pathTor"),
    ("direct", "pathDirect"),
    ("relay", "pathRelay"),
    ("friend", "pathFriend"),
];

/// Settings sections and their colour tokens (the square icon behind each).
pub const SECTION_COLOURS: [(&str, &str); 7] = [
    ("general", "sectionGeneral"),
    ("appearance", "sectionAppearance"),
    ("privacy", "sectionPrivacy"),
    ("lock", "sectionLock"),
    ("network", "sectionNetwork"),
    ("updates", "sectionUpdates"),
    ("about", "sectionAbout"),
];

/// Replace `${name}` placeholders. A placeholder without a value is left as
/// it is (the tests catch that).
fn subst(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len() + 1024);
    let mut rest = template;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push_str("${");
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Drop every `animation…` and `transition…` declaration (reduced motion).
fn strip_motion(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut depth = 0usize;
    let mut decl = String::new();
    for ch in css.chars() {
        match ch {
            '{' => {
                depth += 1;
                out.push_str(&decl);
                decl.clear();
                out.push(ch);
            }
            '}' => {
                let d = decl.trim_start();
                if !(d.starts_with("animation") || d.starts_with("transition")) {
                    out.push_str(&decl);
                }
                decl.clear();
                depth = depth.saturating_sub(1);
                out.push(ch);
            }
            ';' if depth > 0 => {
                decl.push(ch);
                let d = decl.trim_start();
                if !(d.starts_with("animation") || d.starts_with("transition")) {
                    out.push_str(&decl);
                }
                decl.clear();
            }
            _ => decl.push(ch),
        }
    }
    out.push_str(&decl);
    out
}

/// The Calm stylesheet: every rule of the app, with `${…}` filled from the
/// tokens by [`generate_css`].
const CALM_CSS: &str = r#"
/* ---------- window ---------- */
window.raven-window, window.raven-window > .background { background-color: @raven_background; color: @raven_text_primary; }
.raven-surface { background-color: @raven_surface; }
.raven-main { background-color: @raven_background; }
headerbar.raven-header { background-image: none; background-color: @raven_background; border-bottom: 1px solid @raven_border; box-shadow: none; min-height: 38px; }
headerbar.raven-header.split { background-image: linear-gradient(to right, @raven_sidebar ${sidebar}px, @raven_border ${sidebar}px, @raven_border ${sidebar1}px, @raven_background ${sidebar1}px); }
headerbar.raven-header .ttl { ${f_caption} font-weight: 600; color: @raven_text_secondary; }
.raven-sidebar { background-color: @raven_sidebar; border-right: 1px solid @raven_border; padding: 4px 12px 12px 12px; }

/* ---------- type ---------- */
.t-body { ${f_body} }
.t-bubble { ${f_bubble} }
.t-label { ${f_label} }
.t-caption, .small { ${f_caption} }
.t-meta { ${f_meta} }
.t-small { ${f_small} }
.t-section { ${f_section} color: @raven_text_tertiary; }
.t-title, .title { ${f_title} }
.t-brand { ${f_brand} }
.t-page { ${f_page} }
.display { ${f_display_size} font-weight: 600; letter-spacing: ${display_tracking}px; }
.mono { font-family: monospace; ${f_mono_size} }
.dim { color: @raven_text_secondary; }
.faint { color: @raven_text_tertiary; }
.success { color: @raven_success; }
.warning { color: @raven_warning; }
.danger { color: @raven_danger; }
.accent { color: @raven_accent; }
.strong { font-weight: 600; }
.lead { color: @raven_text_secondary; }

/* ---------- buttons and fields ---------- */
button.raven-btn { min-height: ${button_h}px; padding: 0 12px; border-radius: ${r_control}px; border: 1px solid @raven_border_strong; background-image: none; background-color: @raven_surface; color: @raven_text_primary; box-shadow: 0 1px 1px rgba(0, 0, 0, 0.04); ${f_caption} font-weight: 500; transition: background-color ${hover}ms ease, transform ${press}ms ease; }
button.raven-btn:hover { background-color: mix(@raven_surface, @raven_text_primary, 0.06); }
button.raven-btn:active { transform: scale(0.97); }
button.raven-btn:disabled { opacity: 0.45; }
button.raven-btn:focus-visible, button.raven-primary:focus-visible { outline: 2px solid @raven_focus; outline-offset: 1px; }
button.raven-primary { min-height: ${button_h}px; padding: 0 12px; border-radius: ${r_control}px; border: 1px solid transparent; background-image: none; background-color: @raven_accent_fill; color: @raven_on_accent; box-shadow: none; ${f_caption} font-weight: 600; transition: background-color ${hover}ms ease, transform ${press}ms ease; }
button.raven-primary:hover { background-color: mix(@raven_accent_fill, #ffffff, 0.08); }
button.raven-primary:active { transform: scale(0.97); }
button.raven-primary:disabled { opacity: 0.45; }
button.raven-primary.danger-fill { background-color: @raven_danger_fill; }
button.raven-quiet { background-image: none; background-color: transparent; border: none; box-shadow: none; color: @raven_accent; padding: 0 4px; min-height: 0; ${f_caption} font-weight: 500; }
button.raven-quiet:hover { background-color: alpha(@raven_accent, 0.08); }
button.raven-danger { color: @raven_danger_fill; border-color: alpha(@raven_danger_fill, 0.35); }
button.raven-flat { background-image: none; background-color: transparent; border: none; box-shadow: none; }
button.raven-flat:hover { background-color: alpha(@raven_text_primary, 0.05); }
entry.raven-field { min-height: ${button_h}px; padding: 0 10px; border-radius: ${r_control}px; border: none; background-image: none; background-color: @raven_background; box-shadow: inset 0 0 0 1px @raven_border_strong; outline: none; ${f_caption} transition: box-shadow ${hover}ms ease; }
entry.raven-field:focus-within { box-shadow: inset 0 0 0 2px alpha(@raven_accent_fill, 0.6); }
textview.raven-field { border-radius: ${r_control}px; }
.raven-field.error { box-shadow: inset 0 0 0 2px alpha(@raven_danger_fill, 0.6); }

/* ---------- sidebar ---------- */
.brand { padding: 2px 6px 8px 6px; }
.brand .mark { color: @raven_text_primary; }
.heart { min-width: 7px; min-height: 7px; border-radius: 9999px; background-color: @raven_success_fill; }
.heart.down { background-color: @raven_presence_none; }
.heart.beat { animation: raven-beat ${beat}ms ease-out 1; }
entry.raven-search { min-height: 32px; padding: 0 10px; border-radius: 9px; border: none; background-image: none; box-shadow: none; outline: none; background-color: alpha(@raven_text_primary, 0.06); color: @raven_text_primary; margin: 2px 0 4px 0; ${f_caption} }
entry.raven-search:focus-within { box-shadow: inset 0 0 0 2px alpha(@raven_accent_fill, 0.45); }
entry.raven-search image { color: @raven_text_secondary; }
.section-label { ${f_section} color: @raven_text_tertiary; padding: 8px 8px 2px 8px; }
list.raven-list, list.raven-nav { background-color: transparent; }
list.raven-list > row, list.raven-nav > row { border-radius: ${r_row}px; margin: 1px 0; padding: 8px; background-color: transparent; transition: background-color ${hover}ms ease, box-shadow ${hover}ms ease; }
list.raven-nav > row { padding: 6px 8px; }
list.raven-list > row:hover, list.raven-nav > row:hover { background-color: alpha(@raven_text_primary, 0.05); }
list.raven-list > row:selected, list.raven-nav > row:selected { background-color: @raven_surface; color: @raven_text_primary; box-shadow: ${shadow_card}; }
list.raven-list > row:focus-visible, list.raven-nav > row:focus-visible { outline: 2px solid @raven_focus; outline-offset: -2px; }
list.raven-nav > row .icon { color: @raven_text_secondary; }
list.raven-nav > row:selected .icon { color: @raven_accent; }
.row-name { ${f_label} }
.row-status { font-size: ${rem12}; color: @raven_text_secondary; }
.row-status.wait { color: @raven_warning; }
.row-meta { ${f_small} color: @raven_text_tertiary; }
.via { color: @raven_text_tertiary; }
.unverified-tag { font-size: ${rem10_5}; font-weight: 600; color: @raven_warning; }
.badge { min-width: 18px; min-height: 18px; padding: 0 5px; border-radius: ${r_badge}px; background-color: @raven_accent_fill; color: @raven_on_accent; ${f_small} font-weight: 600; }
button.add-friend { background-image: none; background-color: transparent; border: none; box-shadow: none; color: @raven_accent; padding: 7px 8px; border-radius: 9px; font-size: ${rem13}; font-weight: 500; }
button.add-friend:hover { background-color: alpha(@raven_accent, 0.08); }
.nav-top { border-top: 1px solid @raven_border; padding-top: 8px; }
button.sidebar-toggle { min-width: 28px; min-height: 28px; padding: 0; }

/* ---------- avatars and presence ---------- */
.avatar { min-width: ${av}px; min-height: ${av}px; border-radius: 9999px; font-weight: 600; font-size: ${rem14}; }
.avatar.sm { min-width: ${av_sm}px; min-height: ${av_sm}px; font-size: ${rem10}; }
.avatar.lg { min-width: ${av_lg}px; min-height: ${av_lg}px; font-size: ${rem15}; }
.avatar.xs { min-width: 21px; min-height: 21px; font-size: ${rem9_5}; }
.avatar.stacked { box-shadow: 0 0 0 2px @raven_sidebar; }
list.raven-list > row:selected .avatar.stacked { box-shadow: 0 0 0 2px @raven_surface; }
.avatar-you { background-color: @raven_accent_soft; color: @raven_accent; }
${avatar_rules}
.presence { min-width: ${dot}px; min-height: ${dot}px; border-radius: 9999px; border: 2px solid @raven_sidebar; background-color: @raven_presence_none; transition: background-color 400ms ease; }
list.raven-list > row:selected .presence { border-color: @raven_surface; }
.chat-head .presence { border-color: @raven_background; }
.presence.in-touch { background-color: @raven_presence_in_touch; }
.presence.waiting { background-color: @raven_presence_waiting; }
.presence.none { background-color: @raven_presence_none; }
.presence.breathe { animation: raven-breathe ${breathe}ms ease-in-out infinite; }
.presence.blink { animation: raven-blink ${blink}ms ease-out 2; }
.ring { min-width: ${av}px; min-height: ${av}px; border-radius: 9999px; border: 2px solid @raven_warning_fill; opacity: 0; }
.ring.go { animation: raven-ring ${ring}ms ease-out 1; }

/* ---------- chat ---------- */
.chat-head { min-height: ${header_h}px; padding: 0 22px; border-bottom: 1px solid @raven_border; background-color: @raven_background; }
.chat-title { ${f_title} }
.chat-sub { font-size: ${rem12_5}; color: @raven_text_secondary; }
.chat-sub.away { color: @raven_warning; font-weight: 600; }
.verified { font-size: ${rem12}; font-weight: 600; color: @raven_success; }
.unverified { font-size: ${rem12}; font-weight: 600; color: @raven_warning; }
.route { min-height: ${route_h}px; padding: 0 22px; border-bottom: 1px solid @raven_border; ${f_meta} color: @raven_text_secondary; }
.route-label { background-color: @raven_background; padding: 0 10px; font-weight: 500; }
.pill { min-height: ${pill_h}px; padding: 0 10px; border-radius: ${r_chip}px; font-size: ${rem12}; font-weight: 500; color: @raven_accent; background-color: alpha(@raven_accent, 0.11); }
.chip { min-height: ${chip_h}px; padding: 0 8px 0 6px; border-radius: ${r_chip_sm}px; ${f_small} font-weight: 500; color: @raven_text_secondary; background-color: alpha(@raven_text_secondary, 0.11); }
${path_rules}
.chip.slide { animation: raven-chip-in ${chip_ms}ms ${c_chip} 1; }
.day { margin: 14px 0 8px 0; ${f_meta} font-weight: 600; color: @raven_text_tertiary; }
.day separator { background-color: @raven_border; min-height: 1px; }
.msgs { padding: 18px 28px 22px 28px; }
.msg { margin: 3px 0; }
.from { font-size: ${rem12}; font-weight: 600; margin: 0 0 3px 12px; }
.bubble { padding: 9px 14px; border-radius: ${r_bubble}px ${r_bubble}px ${r_bubble}px ${r_tail}px; background-color: @raven_surface; color: @raven_text_primary; box-shadow: ${shadow_card}; transition: background-color 500ms ease, color 500ms ease, box-shadow 500ms ease; }
.bubble label { ${f_bubble} }
.msg.out .bubble { border-radius: ${r_bubble}px ${r_bubble}px ${r_tail}px ${r_bubble}px; background-image: linear-gradient(to bottom, @raven_accent_fill_top, @raven_accent_fill); color: @raven_on_accent; box-shadow: ${shadow_bubble}; }
.msg.out .bubble label selection { background-color: alpha(#ffffff, 0.3); color: @raven_on_accent; }
.msg.out.pending .bubble { background-image: none; background-color: transparent; color: @raven_accent; box-shadow: inset 0 0 0 1.5px alpha(@raven_accent_fill, 0.55); }
.msg.out.problem .bubble { background-image: none; background-color: transparent; color: @raven_text_primary; box-shadow: inset 0 0 0 1.5px alpha(@raven_danger_fill, 0.5); }
.msg.in.fresh .bubble { box-shadow: 0 0 0 3px alpha(@raven_accent_fill, 0.25), ${shadow_card}; }
.msg.lift .bubble { animation: raven-lift ${lift}ms ${c_lift} 1; }
.msg.incoming { animation: raven-in-up ${bubble_in}ms ${c_arrive} 1; }
.meta { margin: 5px 4px 0 4px; ${f_meta} color: @raven_text_secondary; min-height: 20px; }
.tick.ok { color: @raven_success; }
.tick.wait { color: @raven_warning; }
.tick.bad { color: @raven_danger; }
button.tick { background-image: none; background-color: transparent; border: none; box-shadow: none; padding: 0 2px; min-height: 0; ${f_meta} }
button.tick:hover { background-color: alpha(@raven_text_primary, 0.05); }
.st-ic.spin { animation: raven-spin 900ms linear infinite; }
.st-ic.pop { animation: raven-pop ${pop}ms ${c_pop} 1; }
.ripple { min-width: 14px; min-height: 14px; border-radius: 9999px; border: 1.5px solid @raven_success_fill; opacity: 0; }
.ripple.go { animation: raven-ripple ${ripple}ms ease-out 1; }
.gring { min-width: ${ring_px}px; min-height: ${ring_px}px; }
.recips { margin: 8px 4px 2px 4px; padding: 10px 12px; border-radius: 12px; background-color: @raven_surface; box-shadow: ${shadow_card}; font-size: ${rem12}; }
.recips .r { font-weight: 600; }
.recips .rs { color: @raven_text_secondary; }
.recips .rs.ok { color: @raven_success; }
.recips .rs.wait { color: @raven_warning; }
.recips .rs.bad { color: @raven_danger; }
.recips .new { animation: raven-chip-in 400ms ${c_chip} 1; }
button.disc { background-image: none; background-color: transparent; border: none; box-shadow: none; padding: 0 2px; min-height: 0; color: @raven_accent; ${f_meta} font-weight: 500; }
button.disc:hover { background-color: alpha(@raven_accent, 0.08); }
.away { margin: 14px 0 6px 0; padding: 16px 18px; border-radius: ${r_card}px; background-color: @raven_warning_soft; border: 1px solid alpha(@raven_warning_fill, 0.35); }
.away-title { font-size: ${rem14_5}; font-weight: 700; }
.away-body { font-size: ${rem13}; color: @raven_text_secondary; }
.away-det { border-top: 1px solid alpha(@raven_warning_fill, 0.25); padding-top: 6px; font-size: ${rem12_5}; color: @raven_text_secondary; }
.away-det .strong { color: @raven_text_primary; }
.trio-node { ${f_small} color: @raven_text_secondary; }
.trio-link { min-height: 2px; min-width: 44px; background-color: @raven_border_strong; margin-bottom: 18px; }
.trio-link.dashed { background-image: linear-gradient(to right, @raven_warning_fill 50%, transparent 50%); background-size: 8px 2px; background-color: transparent; opacity: 0.7; }
.backnote { margin: 12px 0 2px 0; padding: 10px 14px; border-radius: 12px; background-color: alpha(@raven_success_fill, 0.12); color: @raven_success; font-size: ${rem13}; font-weight: 500; animation: raven-in-up 400ms ease-out 1; }
.calm-banner { margin: 12px 0 4px 0; padding: 12px 14px; border-radius: 14px; background-color: @raven_warning_soft; font-size: ${rem13}; }
.group-banner { margin: 12px 0 4px 0; padding: 12px 14px; border-radius: 14px; background-color: alpha(@raven_text_primary, 0.06); color: @raven_text_secondary; font-size: ${rem13}; }
.empty-chat { color: @raven_text_secondary; margin-top: 24px; }

/* ---------- composer and toast ---------- */
.composer-wrap { padding: 10px 22px 16px 22px; }
.composer { padding: 6px 6px 6px 16px; border-radius: ${r_composer}px; background-color: @raven_surface; box-shadow: ${shadow_card}, 0 0 0 1px @raven_border; transition: box-shadow ${hover}ms ease; }
.composer:focus-within { box-shadow: ${shadow_card}, 0 0 0 2px alpha(@raven_accent_fill, 0.45); }
entry.composer-entry { min-height: 32px; border: none; background-image: none; background-color: transparent; box-shadow: none; outline: none; padding: 0; ${f_body} }
entry.composer-entry text placeholder { color: @raven_text_tertiary; }
button.send { min-width: ${send}px; min-height: ${send}px; padding: 0; border-radius: 9999px; border: none; background-image: none; background-color: @raven_accent_fill; color: #ffffff; box-shadow: none; transition: transform 150ms ease, background-color ${hover}ms ease; }
button.send:hover { transform: scale(1.06); }
button.send:active { transform: scale(0.94); }
button.send:disabled { opacity: 0.45; }
button.send:focus-visible { outline: 2px solid @raven_focus; outline-offset: 2px; }
.e2e { ${f_small} color: @raven_text_tertiary; margin-top: 6px; }
.toast { padding: 7px 14px; border-radius: 9999px; font-size: ${rem12}; background-color: @raven_surface; box-shadow: ${shadow_card}, 0 0 0 1px @raven_border; color: @raven_text_secondary; margin-bottom: 6px; }
button.toast { background-image: none; border: none; min-height: 0; }
button.toast:hover { background-color: mix(@raven_surface, @raven_text_primary, 0.04); }
.toast.ok { color: @raven_success; }
.toast.wait { color: @raven_warning; }
.toast.bad { color: @raven_danger; }
.toast.jump { color: @raven_accent; font-weight: 600; }

/* ---------- pages and cards ---------- */
.view-in { animation: raven-view-in ${view_in}ms ${c_arrive} 1; }
.page { padding: 26px 32px 40px 32px; }
.page-title { ${f_page} }
.intro { color: @raven_text_secondary; margin-bottom: 12px; }
.card { background-color: @raven_surface; border-radius: ${r_card}px; box-shadow: ${shadow_card}, 0 0 0 1px @raven_border; padding: 16px 18px; }
.card.flat-card { padding: 0; }
.h2 { ${f_meta} font-weight: 600; letter-spacing: 0.05em; color: @raven_text_secondary; margin: 14px 0 4px 0; }
.this-card .mark { color: @raven_text_primary; }
.radar-ring { min-width: 64px; min-height: 64px; border-radius: 9999px; border: 1.5px solid @raven_success_fill; opacity: 0; }
.radar-ring.go { animation: raven-radar ${radar}ms ease-out 1; }
.radar-ring.go.late { animation: raven-radar ${radar}ms ease-out 350ms 1; }
.way { transition: opacity 300ms ease; }
.way.off { opacity: 0.55; filter: grayscale(0.8); }
.way-ic { min-width: 36px; min-height: 36px; border-radius: 10px; }
.way-title { font-size: ${rem14}; font-weight: 600; }
.way-sub { font-size: ${rem12}; color: @raven_text_tertiary; }
.way-desc { font-size: ${rem12_5}; color: @raven_text_secondary; }
.way-state { font-size: ${rem12}; color: @raven_text_secondary; }
.live { min-width: 7px; min-height: 7px; border-radius: 9999px; background-color: @raven_success_fill; }
.live.go { animation: raven-beat 600ms ease-out 1; }
.state-chip { ${f_small} font-weight: 600; padding: 2px 8px; border-radius: 9999px; background-color: alpha(@raven_text_primary, 0.07); color: @raven_text_secondary; }
.state-chip.state-on { background-color: alpha(@raven_success_fill, 0.14); color: @raven_success; }
.state-chip.state-off { color: @raven_text_secondary; }
.state-chip.state-preview { background-color: @raven_warning_soft; color: @raven_warning; }
.state-chip.state-lab { background-color: @raven_accent_soft; color: @raven_accent; }
.state-chip.state-not-here { color: @raven_text_tertiary; }
.lab-banner { background-color: @raven_accent_soft; color: @raven_accent; font-weight: 600; padding: 8px 12px; border-radius: ${r_control}px; }
.prot-row { padding: 12px 0; border-bottom: 1px solid @raven_border; }
.prot-row.last { border-bottom: none; }
.row-line { padding: 12px 0; border-bottom: 1px solid @raven_border; }

/* ---------- settings ---------- */
.setnav { min-width: ${setnav}px; padding: 22px 12px; border-right: 1px solid @raven_border; }
.setnav-title { font-size: ${rem22}; font-weight: 600; letter-spacing: -0.01em; margin: 0 10px 14px 10px; }
button.setnav-item { min-height: 38px; padding: 0 10px; border-radius: ${r_row}px; border: none; background-image: none; background-color: transparent; box-shadow: none; color: @raven_text_secondary; font-size: ${rem13_5}; transition: color 200ms ease; }
button.setnav-item:hover { color: @raven_text_primary; }
button.setnav-item.current { color: @raven_text_primary; font-weight: 600; }
button.setnav-item:focus-visible { outline: 2px solid @raven_focus; outline-offset: -2px; }
.glider { border-radius: ${r_row}px; background-color: @raven_surface; box-shadow: ${shadow_card}, 0 0 0 1px @raven_border; }
.sec-ic { min-width: 26px; min-height: 26px; border-radius: 8px; color: #ffffff; }
${section_rules}
.sec-in { padding: 26px 34px 48px 34px; }
.sec-in.from-below { animation: raven-section-below ${section_in}ms ${c_section} 1; }
.sec-in.from-above { animation: raven-section-above ${section_in}ms ${c_section} 1; }
.sec-title { ${f_page} }
.glbl { ${f_meta} font-weight: 600; letter-spacing: 0.05em; color: @raven_text_tertiary; margin: 0 0 8px 4px; }
.grp { background-color: @raven_surface; border-radius: ${r_group}px; box-shadow: ${shadow_card}, 0 0 0 1px @raven_border; margin-bottom: 22px; }
.item { padding: 13px 16px; border-top: 1px solid @raven_border; }
.item.first { border-top: none; }
.item-title { font-size: ${rem13_5}; font-weight: 500; }
.item-desc { font-size: ${rem12_5}; color: @raven_text_secondary; }
.item.dis .item-title, .item.dis .item-desc { opacity: 0.6; }
.seg { padding: 3px; border-radius: ${r_segment}px; background-color: alpha(@raven_text_primary, 0.07); }
.seg button { padding: 5px 12px; min-height: 0; border-radius: ${r_thumb}px; border: none; background-image: none; background-color: transparent; box-shadow: none; color: @raven_text_secondary; font-size: ${rem12_5}; transition: color 250ms ease; }
.seg button:checked { color: @raven_text_primary; font-weight: 600; background-color: transparent; }
.seg button:focus-visible { outline: 2px solid @raven_focus; outline-offset: -2px; }
.seg-thumb { border-radius: ${r_thumb}px; background-color: @raven_surface; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.14), 0 0 0 1px @raven_border; }
switch { min-height: ${sw_h}px; padding: 0; border: none; border-radius: 9999px; background-image: none; background-color: @raven_border_strong; box-shadow: none; outline: none; transition: background-color ${sw_color}ms ease; }
switch:checked { background-color: @raven_success_fill; }
switch:disabled { opacity: 0.55; }
switch:focus-visible { outline: 2px solid @raven_focus; outline-offset: 2px; }
switch slider { min-width: ${knob}px; min-height: ${knob}px; margin: ${knob_m}px; border: none; border-radius: 9999px; background-image: none; background-color: #ffffff; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25); transition: min-width ${press}ms ease; }
switch:active slider { min-width: ${knob_wide}px; }
switch image { color: transparent; }
checkbutton.raven-radio { padding: 6px 4px; border-radius: ${r_control}px; ${f_caption} }
checkbutton.raven-radio radio { min-width: 16px; min-height: 16px; border-radius: 9999px; border: none; background-image: none; background-color: transparent; box-shadow: inset 0 0 0 1.5px @raven_border_strong; -gtk-icon-source: none; transition: box-shadow 250ms ease; }
checkbutton.raven-radio radio:checked { box-shadow: inset 0 0 0 1.5px @raven_accent_fill, inset 0 0 0 4px @raven_surface, inset 0 0 0 8px @raven_accent_fill; }
checkbutton.raven-radio:focus-visible { outline: 2px solid @raven_focus; }
.stat { font-size: ${rem12}; font-weight: 600; padding: 3px 9px; border-radius: 9999px; background-color: alpha(@raven_success_fill, 0.14); color: @raven_success; }
.stat.n { background-color: alpha(@raven_text_primary, 0.07); color: @raven_text_secondary; }
.stat.w { background-color: @raven_warning_soft; color: @raven_warning; }
.ldot { min-width: 6px; min-height: 6px; border-radius: 9999px; background-color: currentColor; }
.pend { padding: 12px 16px; margin: 0 0 14px 0; border-radius: 12px; background-color: @raven_accent_soft; color: @raven_accent; font-size: ${rem13}; animation: raven-in-up 350ms ease-out 1; }
.pend.ok { background-color: alpha(@raven_success_fill, 0.13); color: @raven_success; }
.pend.bad { background-color: @raven_warning_soft; color: @raven_warning; }
.lockrow { padding: 16px; }
.lk { min-width: 42px; min-height: 42px; border-radius: 12px; background-color: alpha(@raven_success_fill, 0.14); color: @raven_success; }
.lk.locked { background-color: @raven_accent_soft; color: @raven_accent; }
.about { padding: 26px 16px 22px 16px; }
.about .mark { color: @raven_text_primary; }
.about-name { font-size: ${rem20}; font-weight: 700; }
.stored-row { padding: 0 0 0 0; font-size: ${rem13}; color: @raven_text_secondary; }
.stored-row .e { color: @raven_success; }
.stored-row .p { color: @raven_text_tertiary; }
.preview-box { padding: 18px 20px; }

/* ---------- sheets, the locked window, setup ---------- */
window.raven-sheet, window.raven-sheet > .background { background-color: @raven_surface; }
.sheet-body { padding: 24px 26px 20px 26px; }
.sheet-body.sheet-in { animation: raven-sheet-in ${sheet_in}ms ${c_sheet} 1; }
.sheet-title { font-size: ${rem17}; font-weight: 700; }
.sheet-text { font-size: ${rem13}; color: @raven_text_secondary; }
.lkic { min-width: 48px; min-height: 48px; border-radius: 14px; background-color: @raven_accent_soft; color: @raven_accent; }
.lkic.danger { background-color: alpha(@raven_danger_fill, 0.12); color: @raven_danger_fill; }
.locked-card { padding: 24px; }
.locked-card.fade-in { animation: raven-sheet-in ${locked_fade}ms ${c_sheet} 1; }
.locked-card.shake { animation: raven-shake ${shake}ms ease-in-out 1; }
.locked-card .mark { color: @raven_text_primary; }
.locked-title { font-size: ${rem20}; font-weight: 700; }
.locked-err { min-height: 18px; font-size: ${rem12}; color: @raven_warning; }
.steps { ${f_meta} color: @raven_text_tertiary; }
.step-i { min-width: 18px; min-height: 18px; border-radius: 9999px; font-size: ${rem10}; font-weight: 700; background-color: @raven_border; color: @raven_text_secondary; transition: background-color 300ms ease, color 300ms ease; }
.step.done .step-i { background-color: @raven_success_fill; color: #ffffff; }
.step.now { color: @raven_text_primary; font-weight: 600; }
.step.now .step-i { background-color: @raven_accent_fill; color: #ffffff; }
.step-bar { min-height: 2px; border-radius: 1px; background-color: @raven_border; transition: background-color 400ms ease; }
.step-bar.done { background-color: @raven_success_fill; }
.logo.reveal { animation: raven-logo ${logo}ms ease-out 1; }
.w { padding: 10px 10px 9px 10px; border-radius: 12px; background-color: @raven_background; box-shadow: inset 0 0 0 1px @raven_border; transition: background-color 250ms ease, box-shadow 250ms ease, transform 250ms ease; }
.w-n { font-size: ${rem10_5}; font-weight: 600; color: @raven_text_tertiary; }
.w-word { font-size: ${rem15_5}; font-weight: 600; letter-spacing: 0.01em; }
.w.check { box-shadow: inset 0 0 0 1.5px alpha(@raven_path_friend, 0.55); }
.w-ck { font-size: ${rem9_5}; font-weight: 700; color: @raven_path_friend; letter-spacing: 0.05em; }
.w.reveal { animation: raven-word-in 300ms ${c_toast} 1; }
.w.say { background-color: @raven_accent_fill; box-shadow: 0 6px 18px alpha(@raven_accent_fill, 0.35); transform: translateY(-2px); }
.w.say label { color: #ffffff; }
.sayline { font-size: ${rem14}; color: @raven_text_secondary; min-height: 30px; }
.fine { border-top: 1px solid @raven_border; padding-top: 12px; margin-top: 14px; font-size: ${rem12_5}; color: @raven_text_secondary; }
.forge { padding: 12px 0 4px 0; }

/* Old helpers some dialogs still use. */
.banner { background-color: @raven_accent_soft; color: @raven_accent; padding: 8px 12px; border-radius: ${r_control}px; }
.step { color: @raven_text_secondary; padding: 4px 8px; border-radius: ${r_control}px; }
.step.current { color: @raven_accent; background-color: @raven_accent_soft; font-weight: 600; }
.chip.hl { background-color: @raven_accent_soft; color: @raven_accent; }
.chip .check-mark { ${f_small} color: @raven_warning; margin-left: 4px; }
.path-chip { ${f_small} color: @raven_text_secondary; border: 1px solid @raven_border; border-radius: 10px; padding: 0 6px; }
.way-row { padding: 8px 0; border-bottom: 1px solid @raven_border; }
.raven-page { padding: ${xl}px; }

@keyframes raven-beat { 0% { transform: scale(1); } 40% { transform: scale(1.35); } 100% { transform: scale(1); } }
@keyframes raven-breathe { 0% { box-shadow: 0 0 0 0 alpha(@raven_success_fill, 0); } 50% { box-shadow: 0 0 0 3px alpha(@raven_success_fill, 0.22); } 100% { box-shadow: 0 0 0 0 alpha(@raven_success_fill, 0); } }
@keyframes raven-blink { 0% { transform: scale(1); } 50% { transform: scale(1.5); } 100% { transform: scale(1); } }
@keyframes raven-ring { 0% { opacity: 0.9; transform: scale(1); } 100% { opacity: 0; transform: scale(1.8); } }
@keyframes raven-lift { 0% { opacity: 0.2; transform: translateY(${lift_dy}px) scale(0.92); } 100% { opacity: 1; transform: translateY(0) scale(1); } }
@keyframes raven-in-up { 0% { opacity: 0; transform: translateY(10px); } 100% { opacity: 1; transform: translateY(0); } }
@keyframes raven-chip-in { 0% { opacity: 0; transform: translateX(-8px) scale(0.9); } 100% { opacity: 1; transform: translateX(0) scale(1); } }
@keyframes raven-spin { 0% { transform: rotate(0deg); } 100% { transform: rotate(360deg); } }
@keyframes raven-pop { 0% { transform: scale(0.4); } 100% { transform: scale(1); } }
@keyframes raven-ripple { 0% { opacity: 0.9; transform: scale(0.6); } 100% { opacity: 0; transform: scale(2.2); } }
@keyframes raven-view-in { 0% { opacity: 0; transform: translateY(6px); } 100% { opacity: 1; transform: translateY(0); } }
@keyframes raven-section-below { 0% { opacity: 0; transform: translateY(16px); } 100% { opacity: 1; transform: translateY(0); } }
@keyframes raven-section-above { 0% { opacity: 0; transform: translateY(-16px); } 100% { opacity: 1; transform: translateY(0); } }
@keyframes raven-radar { 0% { opacity: 0.8; transform: scale(0.4); } 100% { opacity: 0; transform: scale(1.15); } }
@keyframes raven-sheet-in { 0% { opacity: 0; transform: translateY(18px) scale(0.98); } 100% { opacity: 1; transform: translateY(0) scale(1); } }
@keyframes raven-shake { 0% { transform: translateX(0); } 20% { transform: translateX(-8px); } 40% { transform: translateX(8px); } 60% { transform: translateX(-8px); } 80% { transform: translateX(8px); } 100% { transform: translateX(0); } }
@keyframes raven-word-in { 0% { opacity: 0; transform: translateY(8px) scale(0.94); } 100% { opacity: 1; transform: translateY(0) scale(1); } }
@keyframes raven-logo { 0% { opacity: 0; transform: scale(0.92); } 100% { opacity: 1; transform: scale(1); } }
"#;

/// The full stylesheet for one scheme. `motion: false` (reduced motion)
/// leaves out every animation and transition and stops the theme's own.
pub fn generate_css(t: &Tokens, scheme: Scheme, motion: bool) -> String {
    let mut c = String::with_capacity(48 * 1024);
    let _ = writeln!(
        c,
        "/* Generated from desktop/design/tokens.json ({scheme:?}, motion {}). Do not edit. */",
        if motion { "on" } else { "off" }
    );
    for (name, pair) in &t.colors {
        let _ = writeln!(
            c,
            "@define-color raven_{} {};",
            snake(name),
            pair.get(scheme).css()
        );
    }
    for (i, a) in t.avatars.iter().enumerate() {
        let _ = writeln!(
            c,
            "@define-color raven_avatar_bg_{i} {};",
            a.bg.get(scheme).css()
        );
        let _ = writeln!(
            c,
            "@define-color raven_avatar_fg_{i} {};",
            a.fg.get(scheme).css()
        );
    }
    let f = &t.fonts;
    let body = f.body.size;
    let r = |px: f64| rem(px, body);
    let avatar_rules = (0..t.avatars.len())
        .map(|i| {
            format!(
                ".avatar-{i} {{ background-color: @raven_avatar_bg_{i}; color: @raven_avatar_fg_{i}; }}\n.fg-{i} {{ color: @raven_avatar_fg_{i}; }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let path_rules = PATH_COLOURS
        .iter()
        .map(|(class, token)| {
            let v = snake(token);
            format!(
                ".pill.p-{class}, .chip.p-{class} {{ color: @raven_{v}; background-color: alpha(@raven_{v}, 0.11); }}\n.p-{class} .way-ic {{ color: @raven_{v}; background-color: alpha(@raven_{v}, 0.12); }}\n.route-label.p-{class}, .icon.p-{class} {{ color: @raven_{v}; }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let section_rules = SECTION_COLOURS
        .iter()
        .map(|(class, token)| {
            format!(
                ".sec-ic.sec-{class} {{ background-color: @raven_{}; }}",
                snake(token)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let l = &t.layout;
    let d = &t.durations;
    let m = &t.calm;
    let rd = &t.radius;
    let (sw_w, sw_h, knob) = l.switch;
    let _ = sw_w; // GTK's switch is as wide as two knobs: its width follows the knob.
    let knob_m = sw_h.saturating_sub(knob) / 2;
    let px = |v: u32| v.to_string();
    let vars: Vec<(&str, String)> = vec![
        ("sidebar", px(l.sidebar_width)),
        ("sidebar1", px(l.sidebar_width + 1)),
        ("f_body", font_css(&f.body, body)),
        ("f_bubble", font_css(&f.bubble, body)),
        ("f_label", font_css(&f.label, body)),
        ("f_caption", font_css(&f.caption, body)),
        ("f_meta", font_css(&f.meta, body)),
        ("f_small", font_css(&f.small, body)),
        ("f_section", font_css(&f.section_label, body)),
        ("f_title", font_css(&f.title, body)),
        ("f_brand", font_css(&f.brand, body)),
        ("f_page", font_css(&f.page_title, body)),
        (
            "f_display_size",
            format!("font-size: {};", r(f.display.size)),
        ),
        ("display_tracking", num_css(f.display.tracking)),
        ("f_mono_size", format!("font-size: {};", r(f.mono.size))),
        ("rem9_5", r(9.5)),
        ("rem10", r(10.0)),
        ("rem10_5", r(10.5)),
        ("rem12", r(12.0)),
        ("rem12_5", r(12.5)),
        ("rem13", r(13.0)),
        ("rem13_5", r(13.5)),
        ("rem14", r(14.0)),
        ("rem14_5", r(14.5)),
        ("rem15", r(15.0)),
        ("rem15_5", r(15.5)),
        ("rem17", r(17.0)),
        ("rem20", r(20.0)),
        ("rem22", r(22.0)),
        ("button_h", px(l.button_height)),
        ("header_h", px(l.header_height)),
        ("route_h", px(l.route_line_height)),
        ("pill_h", px(l.pill_height)),
        ("chip_h", px(l.chip_height)),
        ("send", px(l.send_button)),
        ("av", px(l.avatar_size)),
        ("av_sm", px(l.avatar_small)),
        ("av_lg", px(l.avatar_large)),
        ("dot", px(l.presence_dot)),
        ("ring_px", px(l.group_ring)),
        ("setnav", px(l.settings_nav_width)),
        ("sw_h", px(sw_h)),
        ("knob", px(knob)),
        ("knob_m", px(knob_m)),
        ("knob_wide", px(knob + 5)),
        ("xl", px(t.spacing[4])),
        ("r_control", px(rd.control)),
        ("r_card", px(rd.card)),
        ("r_group", px(rd.group)),
        ("r_row", px(rd.row)),
        ("r_bubble", px(rd.bubble)),
        ("r_tail", px(rd.bubble_tail)),
        ("r_chip", px(rd.chip)),
        ("r_chip_sm", px(rd.chip_small)),
        ("r_badge", px(rd.badge)),
        ("r_composer", px(rd.composer)),
        ("r_segment", px(rd.segment)),
        ("r_thumb", px(rd.segment_thumb)),
        ("shadow_card", t.shadow_card.css(scheme)),
        ("shadow_bubble", t.shadow_bubble_out.css(scheme)),
        ("beat", px(d.heartbeat_pulse)),
        ("breathe", px(d.presence_breathe)),
        ("ring", px(d.retry_ring)),
        ("logo", px(d.logo_reveal)),
        ("bubble_in", px(d.bubble_in)),
        ("blink", px(m.presence_blink)),
        ("lift", px(m.bubble_lift)),
        ("lift_dy", "34".into()),
        ("pop", px(m.check_pop)),
        ("ripple", px(m.tick_ripple)),
        ("chip_ms", px(m.chip_slide)),
        ("view_in", px(m.view_in)),
        ("section_in", px(m.section_in)),
        ("sheet_in", px(m.sheet_in)),
        ("locked_fade", px(m.locked_fade)),
        ("shake", px(m.shake)),
        ("hover", px(m.hover)),
        ("press", px(m.press)),
        ("radar", px(m.radar_pulse)),
        ("sw_color", px(m.switch_color)),
        ("c_lift", t.curve_css("lift")),
        ("c_arrive", t.curve_css("arrive")),
        ("c_pop", t.curve_css("pop")),
        ("c_chip", t.curve_css("chip")),
        ("c_sheet", t.curve_css("sheet")),
        ("c_toast", t.curve_css("toast")),
        ("c_section", t.curve_css("section")),
        ("avatar_rules", avatar_rules),
        ("path_rules", path_rules),
        ("section_rules", section_rules),
    ];
    let body_css = subst(CALM_CSS, &vars);
    if motion {
        c.push_str(&body_css);
    } else {
        c.push_str(&strip_motion(&body_css));
        // GTK's own theme moves too (button hovers, switches): stop it.
        c.push_str("\n* { transition: none; animation: none; }\n");
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_parse() {
        let t = tokens();
        assert_eq!(t.colors.len(), 38, "the Calm palette");
        assert_eq!(t.avatars.len(), 5);
        assert_eq!(t.spacing, [4, 8, 12, 16, 24, 32]);
        assert_eq!(t.sidebar_width, 272);
        assert_eq!(t.layout.settings_nav_width, 228);
        assert_eq!(t.layout.header_height, 66);
        assert_eq!(t.layout.content_max_width, 760);
        assert_eq!(t.layout.switch, (36, 22, 16));
        assert_eq!(t.min_window, (560, 480));
        assert_eq!(t.durations.route_packet, 700);
        assert_eq!(t.durations.receipt_back, 600);
        assert_eq!(t.durations.toast_visible, 5000);
        assert_eq!(t.calm.bubble_lift, 520);
        assert_eq!(t.calm.settings_glide, 450);
        assert_eq!(t.calm.check_pop, 450);
        assert_eq!(t.radius.bubble, 18);
        assert_eq!(t.radius.bubble_tail, 6);
        assert_eq!(t.fonts.body.size, 14.0);
        assert_eq!(t.fonts.section_label.weight, 600);
        assert_eq!(t.fonts.page_title.tracking, -0.01);
        assert_eq!(t.curve("lift"), [0.2, 0.9, 0.25, 1.12]);
        assert_eq!(t.curve("nope"), [0.0, 0.0, 0.58, 1.0]);
        assert_eq!(t.color("accent", Scheme::Light), Rgba::parse("#185FA5"));
        assert_eq!(t.color("accent", Scheme::Dark), Rgba::parse("#85B7EB"));
        assert_eq!(t.color("sidebar", Scheme::Dark), Rgba::parse("#222220"));
        assert_eq!(t.shadow_card.light.len(), 2);
        assert_eq!(t.shadow_sheet.dark.len(), 1);
    }

    #[test]
    fn colour_parsing_and_css() {
        assert_eq!(
            Rgba::parse("#1F1E1C26"),
            Some(Rgba {
                r: 0x1f,
                g: 0x1e,
                b: 0x1c,
                a: 0x26
            })
        );
        assert_eq!(Rgba::parse("#FFF"), None);
        assert_eq!(Rgba::parse("1F1E1C"), None);
        assert_eq!(Rgba::parse("#GG0000"), None);
        assert_eq!(Rgba::parse("#185FA5").unwrap().css(), "#185fa5");
        assert_eq!(
            Rgba::parse("#1F1E1C26").unwrap().css(),
            "rgba(31, 30, 28, 0.149)"
        );
        assert_eq!(
            Rgba::parse("#FFFFFF").unwrap().to_f64(),
            (1.0, 1.0, 1.0, 1.0)
        );
        let w = Rgba::parse("#FFFFFF").unwrap();
        let b = Rgba::parse("#000000").unwrap();
        assert_eq!(w.mix(b, 0.5).r, 128);
        assert_eq!(w.with_alpha(0.5).a, 128);
    }

    #[test]
    fn shadows_and_numbers() {
        let t = tokens();
        assert_eq!(
            t.shadow_card.css(Scheme::Light),
            "0 1px 2px rgba(31, 30, 28, 0.059), 0 4px 14px rgba(31, 30, 28, 0.051)"
        );
        assert_eq!(num_css(1.0), "1");
        assert_eq!(num_css(0.04), "0.04");
        assert_eq!(num_css(-0.01), "-0.01");
        assert_eq!(rem(14.5, 14.0), "1.036rem");
        assert_eq!(t.curve_css("pop"), "cubic-bezier(0.3, 1.6, 0.5, 1)");
    }

    fn undefined_colours(css: &str) -> Vec<String> {
        let mut missing = Vec::new();
        let mut rest = css;
        while let Some(i) = rest.find("@raven_") {
            let tail = &rest[i + 1..];
            let end = tail
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(tail.len());
            let name = &tail[..end];
            if !css.contains(&format!("@define-color {name} ")) {
                missing.push(name.to_string());
            }
            rest = &tail[end..];
        }
        missing
    }

    #[test]
    fn css_has_every_token_for_both_schemes() {
        let t = tokens();
        for scheme in [Scheme::Light, Scheme::Dark] {
            let css = generate_css(t, scheme, true);
            for (name, pair) in &t.colors {
                let line = format!(
                    "@define-color raven_{} {};",
                    snake(name),
                    pair.get(scheme).css()
                );
                assert!(css.contains(&line), "{scheme:?}: {line}");
            }
            for i in 0..5 {
                assert!(css.contains(&format!(".avatar-{i} {{")));
                assert!(css.contains(&format!(".fg-{i} {{")));
                assert!(css.contains(&format!("@define-color raven_avatar_bg_{i} ")));
            }
            for (class, _) in PATH_COLOURS {
                assert!(css.contains(&format!(".chip.p-{class}")), "{class}");
            }
            for (class, _) in SECTION_COLOURS {
                assert!(css.contains(&format!(".sec-ic.sec-{class}")), "{class}");
            }
            assert!(
                css.contains("border-radius: 18px 18px 18px 6px"),
                "bubble radii from tokens"
            );
            assert!(
                css.contains("letter-spacing: 8px"),
                "wordmark tracking from tokens"
            );
            assert!(
                css.contains("raven-ring 1000ms"),
                "retry ring duration from tokens"
            );
            assert!(
                css.contains("raven-breathe 3000ms"),
                "presence breathe from tokens"
            );
            assert!(
                css.contains("raven-lift 520ms cubic-bezier(0.2, 0.9, 0.25, 1.12)"),
                "bubble lift from tokens"
            );
            assert!(css.contains(
                "linear-gradient(to bottom, @raven_accent_fill_top, @raven_accent_fill)"
            ));
            for class in [
                ".state-chip.state-lab",
                ".lab-banner",
                ".chip.slide",
                "list.raven-nav",
                ".glider",
                ".seg-thumb",
                ".away",
                ".recips",
                ".locked-card.shake",
            ] {
                assert!(css.contains(class), "{class}");
            }
            assert!(!css.contains("${"), "unfilled placeholder");
            assert_eq!(undefined_colours(&css), Vec::<String>::new());
            // Braces balance and no 8-digit hex colours reach GTK.
            assert_eq!(css.matches('{').count(), css.matches('}').count());
            assert!(!css
                .split(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == ';')
                .any(|w| w.starts_with('#') && w.len() == 9));
        }
        assert_ne!(
            generate_css(t, Scheme::Light, true),
            generate_css(t, Scheme::Dark, true)
        );
    }

    #[test]
    fn reduced_motion_css_has_no_motion() {
        let t = tokens();
        for scheme in [Scheme::Light, Scheme::Dark] {
            let still = generate_css(t, scheme, false);
            let body = still
                .rsplit_once("* { transition: none; animation: none; }")
                .map(|(a, _)| a)
                .expect("the stop rule ends the sheet");
            // No declaration starts an animation or a transition.
            for decl in body.split([';', '{']) {
                let d = decl.trim_start();
                assert!(
                    !(d.starts_with("animation") || d.starts_with("transition")),
                    "{d}"
                );
            }
            // Everything else is still there.
            assert!(still.contains(".bubble {"));
            assert!(still.contains("@keyframes raven-lift"));
            assert_eq!(still.matches('{').count(), still.matches('}').count());
        }
        assert_eq!(
            strip_motion(".a { color: red; transition: color 1s; }"),
            ".a { color: red; }"
        );
        assert_eq!(
            strip_motion(".a { animation: x 1s }"),
            ".a {}",
            "the last declaration may have no semicolon"
        );
    }

    #[test]
    fn substitution() {
        assert_eq!(
            subst("a ${x} b ${y}", &[("x", "1".into()), ("y", "2".into())]),
            "a 1 b 2"
        );
        assert_eq!(subst("${nope}", &[]), "${nope}");
        assert_eq!(subst("tail ${", &[]), "tail ${");
    }

    #[test]
    fn snake_case_names() {
        assert_eq!(snake("presenceInTouch"), "presence_in_touch");
        assert_eq!(snake("background"), "background");
        assert_eq!(snake("onAccent"), "on_accent");
        assert_eq!(snake("accentFillTop"), "accent_fill_top");
    }

    #[test]
    fn avatar_rule() {
        // (sum of address bytes) mod 5.
        assert_eq!(avatar_index("rvn1qam4hgx8dewltxpwe6cras5ypnk8qfmzx"), 0);
        assert_eq!(avatar_index("rvn1qmy2f5a9m0gw4ku8ch6mj4cxs00s3zz5x"), 1);
        assert_eq!(avatar_index("rvn1qpf74lw30082lpdvr9dp6gsraf5xfauxp"), 2);
        assert_eq!(avatar_index("A"), 65 % 5);
        assert_eq!(avatar_index(""), 0);
    }
}
