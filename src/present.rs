//! What the screens say, computed from engine data (pure, GTK-free, tested).
//! Every text comes from strings.en.json through `tr`/`trf`.

use crate::api::{Contact, Message, MessageState, NodeStatus, ParsedCard, Presence};
use crate::icons::Icon;
use crate::strings::{tr, trf};
use crate::timefmt;

/// The friend's status line in the sidebar. Priority: waiting messages, then
/// "Not verified", then presence.
pub fn contact_status_line(c: &Contact, now_ms: u64) -> String {
    if let Some(w) = c.waiting.filter(|w| w.count > 0) {
        return trf(
            "home.waiting",
            &[
                ("n", &w.count.to_string()),
                ("time", &timefmt::deadline(now_ms, w.until_ms)),
            ],
        );
    }
    if !c.verified {
        return tr("home.notVerified").to_string();
    }
    match c.presence {
        Presence::InTouch => {
            let when = timefmt::relative_past(now_ms, c.last_contact_ms.unwrap_or(now_ms));
            trf("home.inTouch", &[("when", &when)])
        }
        Presence::NoAddress => tr("home.noAddress").to_string(),
        Presence::Waiting | Presence::NoRecent | Presence::Unknown => {
            tr("home.noRecent").to_string()
        }
    }
}

/// CSS class of the presence dot.
pub fn presence_class(p: Presence) -> &'static str {
    match p {
        Presence::InTouch => "in-touch",
        Presence::Waiting => "waiting",
        Presence::NoRecent | Presence::NoAddress | Presence::Unknown => "none",
    }
}

/// The avatar letter: the first letter of the name, upper case.
pub fn avatar_initial(name: &str) -> String {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "?".into())
}

/// How a tick looks: its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickTone {
    Normal,
    Delivered,
    Waiting,
    Problem,
}

impl TickTone {
    /// The CSS class of the tick (`tick` plus this).
    pub fn css(self) -> Option<&'static str> {
        match self {
            TickTone::Normal => None,
            TickTone::Delivered => Some("ok"),
            TickTone::Waiting => Some("wait"),
            TickTone::Problem => Some("bad"),
        }
    }
}

/// The small icon in front of the tick text (Calm): a spinner while sending,
/// a ring with a check once sent, a filled check once a signed receipt came,
/// a dashed clock while waiting, a warning mark when it went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickIcon {
    None,
    Spinner,
    Sent,
    Delivered,
    Waiting,
    Problem,
}

pub fn tick_icon(state: MessageState) -> TickIcon {
    match state {
        MessageState::Sending => TickIcon::Spinner,
        MessageState::Sent => TickIcon::Sent,
        MessageState::Delivered => TickIcon::Delivered,
        MessageState::Waiting => TickIcon::Waiting,
        MessageState::TimeRanOut | MessageState::NotSentMeetFirst | MessageState::Failed => {
            TickIcon::Problem
        }
        MessageState::Unknown => TickIcon::None,
    }
}

/// The tick text under an outgoing message, with its tone. Never says
/// "Delivered" before the state is `delivered` (set only by a receipt).
pub fn tick(msg: &Message, friend: &str, now_ms: u64) -> (String, TickTone) {
    match msg.state {
        MessageState::Sending => (tr("tick.sending").to_string(), TickTone::Normal),
        MessageState::Sent => (tr("tick.sent").to_string(), TickTone::Normal),
        // A late delivery says when it came (E1: one that waited an hour
        // said only "Delivered").
        MessageState::Delivered => match msg.delivered_at_ms.filter(|at| *at > 0) {
            Some(at) => (
                trf(
                    "tick.deliveredAt",
                    &[("time", &timefmt::short_clock(now_ms, at))],
                ),
                TickTone::Delivered,
            ),
            None => (tr("tick.delivered").to_string(), TickTone::Delivered),
        },
        MessageState::Waiting => {
            let until = msg.until_ms.unwrap_or(now_ms);
            let text = if until.saturating_sub(now_ms) < 2 * 3_600_000 {
                trf(
                    "tick.waitingSoon",
                    &[
                        ("name", friend),
                        ("left", &timefmt::time_left(now_ms, until)),
                    ],
                )
            } else {
                trf(
                    "tick.waiting",
                    &[
                        ("name", friend),
                        ("time", &timefmt::deadline(now_ms, until)),
                    ],
                )
            };
            (text, TickTone::Waiting)
        }
        MessageState::TimeRanOut => (tr("tick.timeRanOut").to_string(), TickTone::Problem),
        MessageState::NotSentMeetFirst => (
            trf("tick.notSent.meetFirst", &[("name", friend)]),
            TickTone::Problem,
        ),
        MessageState::Failed => (tr("tick.failed").to_string(), TickTone::Problem),
        MessageState::Unknown => (String::new(), TickTone::Normal),
    }
}

/// The calm line under the chat when nothing is happening.
pub fn quiet_line(node: &NodeStatus, friends: usize) -> String {
    if node.running && node.listening.is_some() && !node.is_local_only() {
        trf("ticker.quiet", &[("n", &friends.to_string())])
    } else {
        tr("tray.notListening").to_string()
    }
}

/// The network chip in the chat toolbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeChip {
    /// Spinner plus "Starting Raven…".
    Starting(String),
    /// Heart plus "your node: ok · now".
    Ok(String),
    /// "Raven's background program isn't responding".
    Down(String),
}

/// A status check is overdue after three missed intervals.
pub fn node_chip(
    node: &NodeStatus,
    last_ok_ms: u64,
    now_ms: u64,
    check_interval_ms: u64,
) -> NodeChip {
    if node.is_starting() {
        return NodeChip::Starting(tr("net.starting").to_string());
    }
    let overdue = now_ms.saturating_sub(last_ok_ms) > 3 * check_interval_ms;
    if !node.running || last_ok_ms == 0 || overdue {
        NodeChip::Down(tr("home.heartbeat.down").to_string())
    } else {
        NodeChip::Ok(trf(
            "home.heartbeat.ok",
            &[("when", &timefmt::heartbeat_age(now_ms, last_ok_ms))],
        ))
    }
}

/// "This computer" line of the Network page.
pub fn listening_line(node: &NodeStatus) -> String {
    if node.is_starting() {
        tr("net.starting").to_string()
    } else if !node.receive || !node.running {
        tr("net.off").to_string()
    } else if node.is_local_only() {
        tr("net.localOnly").to_string()
    } else {
        trf(
            "net.listening",
            &[("port", node.listening_port().unwrap_or("7420"))],
        )
    }
}

/// "Background program: …" line of the Network page.
pub fn process_line(node: &NodeStatus) -> String {
    match (node.running, node.pid) {
        (true, Some(pid)) if node.started_by_app => {
            trf("net.process.app", &[("pid", &pid.to_string())])
        }
        (true, Some(pid)) => trf("net.process.earlier", &[("pid", &pid.to_string())]),
        (true, None) => trf("net.process.earlier", &[("pid", "?")]),
        (false, _) => tr("net.process.none").to_string(),
    }
}

/// The path chip on a message (API.md, Message `path_kind`): a short
/// label ("LAN", "Relay", "443", "Letter") and the accessible text with
/// the engine's full path words. A kind this app doesn't know shows the
/// engine's words; no path, no chip.
pub fn path_chip(path_kind: Option<&str>, path: Option<&str>) -> Option<(String, String)> {
    let label = match path_kind.unwrap_or("") {
        "lan" => tr("network.path.lan"),
        "internet" => tr("network.path.internet"),
        "p2p" => tr("network.path.p2p"),
        "p2p_direct" => tr("network.path.p2pDirect"),
        "p2p_reversed" => tr("network.path.p2pReversed"),
        "p2p_relayed" => tr("network.path.p2pRelayed"),
        "p2p_hole_punched" => tr("network.path.p2pHolePunched"),
        "websocket" => tr("network.path.websocket"),
        "nearby" => tr("network.path.nearby"),
        "onion" => tr("network.path.onion"),
        "friend" => tr("network.path.friend"),
        "handoff" => tr("network.path.handoff"),
        "letter" => tr("network.path.letter"),
        _ => path.filter(|p| !p.is_empty())?,
    };
    let words = path.filter(|p| !p.is_empty()).unwrap_or(label);
    Some((
        label.to_string(),
        trf("network.path.a11y", &[("path", words)]),
    ))
}

/// How a path kind looks (the tokens' `pathRule`): the CSS class suffix
/// (`p-lan`, …; `None` keeps the secondary text colour) and its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathStyle {
    pub class: Option<&'static str>,
    pub icon: Icon,
}

pub fn path_style(kind: Option<&str>) -> PathStyle {
    let (class, icon) = match kind.unwrap_or("") {
        "lan" => (Some("lan"), Icon::Lan),
        "nearby" => (Some("nearby"), Icon::Nearby),
        "tor" | "onion" => (Some("tor"), Icon::Tor),
        "internet" | "p2p_hole_punched" | "websocket" => (Some("direct"), Icon::Direct),
        "p2p" | "p2p_direct" | "p2p_reversed" | "p2p_relayed" | "relay" => {
            (Some("relay"), Icon::Relay)
        }
        "friend" | "handoff" | "letter" | "custody" => (Some("friend"), Icon::Friend),
        _ => (None, Icon::Route),
    };
    PathStyle { class, icon }
}

/// The chat header's second line: the friend's status, then how the last
/// message travelled (the engine's own words) when the friend is in touch.
pub fn header_line(c: &Contact, last_path: Option<&str>, now_ms: u64) -> String {
    let status = contact_status_line(c, now_ms);
    match last_path.filter(|p| !p.is_empty()) {
        Some(p) if c.presence == Presence::InTouch && c.waiting.is_none_or(|w| w.count == 0) => {
            format!("{status} \u{00B7} {p}")
        }
        _ => status,
    }
}

/// `ip:port` to show in the include-address toggle.
pub fn own_network_address(node: &NodeStatus) -> Option<String> {
    let ip = node.address.as_deref()?;
    let port = node.listening_port().unwrap_or("7420");
    Some(format!("{ip}:{port}"))
}

/// The add-friend flow's line under a pasted card's summary: the engine's
/// note about the routes it left out of the card (`card.parse` `note`: no key
/// signs a card's addresses), without the terminal's "note:" prefix. `None`
/// when nothing was left out (or the engine predates the note).
pub fn card_note(card: &ParsedCard) -> Option<String> {
    let note = card.note.as_deref()?.trim();
    let note = match note.get(..5) {
        Some(prefix) if prefix.eq_ignore_ascii_case("note:") => note[5..].trim_start(),
        _ => note,
    };
    let mut chars = note.chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Direction, Waiting};

    const NOW: u64 = 1_792_505_100_000;

    fn parsed(note: Option<&str>) -> ParsedCard {
        ParsedCard {
            short_address: "rvn1qali".into(),
            has_address: false,
            routes: Vec::new(),
            ignored_routes: note.map(|_| vec!["internet".into()]).unwrap_or_default(),
            note: note.map(str::to_owned),
        }
    }

    #[test]
    fn a_card_note_is_the_engines_line_without_the_terminal_prefix() {
        assert_eq!(
            card_note(&parsed(Some(
                "note: Raven ignored this card's inet=: no key signs a card's addresses"
            )))
            .as_deref(),
            Some("Raven ignored this card's inet=: no key signs a card's addresses")
        );
        assert_eq!(
            card_note(&parsed(Some("  some other line  "))).as_deref(),
            Some("Some other line")
        );
        assert_eq!(card_note(&parsed(Some("NOTE:"))), None);
        assert_eq!(card_note(&parsed(Some("   "))), None);
        assert_eq!(card_note(&parsed(None)), None);
    }

    fn contact(verified: bool, presence: Presence) -> Contact {
        Contact {
            address: "rvn1qx".into(),
            name: "Ali".into(),
            short_address: "rvn1qx…".into(),
            verified,
            presence,
            last_contact_ms: Some(NOW - 5 * 60_000),
            waiting: None,
            avatar_index: 0,
            lan: None,
            unread: 0,
        }
    }

    fn msg(state: MessageState) -> Message {
        Message {
            id: "m1".into(),
            address: "rvn1qx".into(),
            direction: Direction::Out,
            text: "hi".into(),
            at_ms: NOW,
            state,
            until_ms: Some(NOW + 22 * 3_600_000),
            path: None,
            path_kind: None,
            delivered_at_ms: None,
            attempts: None,
        }
    }

    #[test]
    fn status_lines() {
        crate::timefmt::set_local_offset_secs(0);
        assert_eq!(
            contact_status_line(&contact(true, Presence::InTouch), NOW),
            "In touch 5 min ago"
        );
        assert_eq!(
            contact_status_line(&contact(false, Presence::InTouch), NOW),
            "Not verified"
        );
        assert_eq!(
            contact_status_line(&contact(true, Presence::NoRecent), NOW),
            "No recent contact"
        );
        assert_eq!(
            contact_status_line(&contact(true, Presence::NoAddress), NOW),
            "No address to reach them"
        );
        let mut c = contact(true, Presence::Waiting);
        c.waiting = Some(Waiting {
            count: 1,
            until_ms: NOW + 24 * 3_600_000,
        });
        assert_eq!(
            contact_status_line(&c, NOW),
            "1 waiting · until Wed 14:05 (UTC)"
        );
    }

    #[test]
    fn ticks_are_honest() {
        crate::timefmt::set_local_offset_secs(0);
        assert_eq!(tick(&msg(MessageState::Sending), "Ali", NOW).0, "Sending…");
        assert_eq!(
            tick(&msg(MessageState::Sent), "Ali", NOW).0,
            "Sent, not confirmed yet"
        );
        assert_eq!(
            tick(&msg(MessageState::Delivered), "Ali", NOW),
            ("Delivered".into(), TickTone::Delivered)
        );
        // A late delivery says when it came (E1): today the time, else the day too.
        let mut late = msg(MessageState::Delivered);
        late.delivered_at_ms = Some(NOW - 2 * 3_600_000);
        assert_eq!(tick(&late, "Ali", NOW).0, "Delivered 12:05 (UTC)");
        late.delivered_at_ms = Some(NOW - 24 * 3_600_000);
        assert_eq!(tick(&late, "Ali", NOW).0, "Delivered Mon 14:05 (UTC)");
        assert_eq!(
            tick(&msg(MessageState::Waiting), "Mina", NOW),
            (
                "Waiting for Mina · until Wed 12:05 (UTC)".into(),
                TickTone::Waiting
            )
        );
        let mut soon = msg(MessageState::Waiting);
        soon.until_ms = Some(NOW + 35 * 60_000);
        assert_eq!(
            tick(&soon, "Mina", NOW).0,
            "Waiting for Mina · about 35 min left"
        );
        assert_eq!(
            tick(&msg(MessageState::TimeRanOut), "Ali", NOW).1,
            TickTone::Problem
        );
        assert_eq!(
            tick(&msg(MessageState::NotSentMeetFirst), "Ali", NOW).0,
            "Not sent: Ali isn't reachable. A first message needs you both online."
        );
    }

    #[test]
    fn node_texts() {
        let mut n = NodeStatus {
            running: true,
            receive: true,
            ..Default::default()
        };
        assert_eq!(
            node_chip(&n, 0, NOW, 2000),
            NodeChip::Starting("Starting Raven…".into())
        );
        n.listening = Some("0.0.0.0:7420".into());
        assert_eq!(
            node_chip(&n, NOW - 1000, NOW, 2000),
            NodeChip::Ok("your node: ok · now".into())
        );
        assert_eq!(
            node_chip(&n, NOW - 7000, NOW, 2000),
            NodeChip::Down("Raven's background program isn't responding".into())
        );
        assert_eq!(
            listening_line(&n),
            "Listening on port 7420 on your network."
        );
        assert_eq!(
            quiet_line(&n, 3),
            "Listening on your Wi-Fi · 3 friends · all quiet"
        );
        n.pid = Some(4242);
        n.started_by_app = true;
        assert_eq!(
            process_line(&n),
            "Background program: running (process 4242), started by this app"
        );
        n.address = Some("192.168.1.23".into());
        assert_eq!(
            own_network_address(&n).as_deref(),
            Some("192.168.1.23:7420")
        );
        n.running = false;
        assert_eq!(process_line(&n), "Background program: not running");
        assert_eq!(quiet_line(&n, 3), "Raven isn't receiving");
    }

    #[test]
    fn path_chips_name_every_path_kind() {
        let kinds = [
            ("lan", "LAN"),
            ("internet", "Internet"),
            ("p2p", "p2p"),
            ("p2p_direct", "p2p direct"),
            ("p2p_reversed", "p2p reversed"),
            ("p2p_relayed", "Relay"),
            ("p2p_hole_punched", "Hole punched"),
            ("websocket", "Relay \u{00B7} 443"),
            ("nearby", "Nearby"),
            ("onion", "via Tor"),
            ("friend", "Friend"),
            ("handoff", "Handoff"),
            ("letter", "Letter"),
        ];
        for (kind, label) in kinds {
            assert_eq!(path_chip(Some(kind), None).unwrap().0, label, "{kind}");
        }
        assert_eq!(
            path_chip(
                Some("websocket"),
                Some("p2p \u{00B7} relayed over WebSocket")
            ),
            Some((
                "Relay \u{00B7} 443".into(),
                "Delivery path: p2p \u{00B7} relayed over WebSocket".into()
            ))
        );
        assert_eq!(
            path_chip(Some("carrier_pigeon"), Some("Pigeon")).unwrap().0,
            "Pigeon",
            "an unknown kind shows the engine's words"
        );
        assert_eq!(path_chip(None, None), None);
        assert_eq!(path_chip(Some(""), Some("")), None);
    }

    #[test]
    fn tick_icons_follow_the_state() {
        assert_eq!(tick_icon(MessageState::Sending), TickIcon::Spinner);
        assert_eq!(tick_icon(MessageState::Sent), TickIcon::Sent);
        assert_eq!(tick_icon(MessageState::Delivered), TickIcon::Delivered);
        assert_eq!(tick_icon(MessageState::Waiting), TickIcon::Waiting);
        assert_eq!(tick_icon(MessageState::Failed), TickIcon::Problem);
        assert_eq!(tick_icon(MessageState::Unknown), TickIcon::None);
        assert_eq!(TickTone::Delivered.css(), Some("ok"));
        assert_eq!(TickTone::Normal.css(), None);
    }

    #[test]
    fn path_colours_follow_the_path_rule() {
        let class = |k: &str| path_style(Some(k)).class;
        assert_eq!(class("lan"), Some("lan"));
        assert_eq!(class("nearby"), Some("nearby"));
        assert_eq!(class("tor"), Some("tor"));
        assert_eq!(class("onion"), Some("tor"));
        assert_eq!(path_style(Some("onion")).icon, Icon::Tor);
        for k in ["internet", "p2p_hole_punched", "websocket"] {
            assert_eq!(class(k), Some("direct"), "{k}");
        }
        for k in ["p2p", "p2p_direct", "p2p_reversed", "p2p_relayed"] {
            assert_eq!(class(k), Some("relay"), "{k}");
        }
        for k in ["friend", "handoff", "letter"] {
            assert_eq!(class(k), Some("friend"), "{k}");
        }
        assert_eq!(path_style(Some("carrier_pigeon")).class, None);
        assert_eq!(path_style(None).icon, Icon::Route);
        assert_eq!(path_style(Some("lan")).icon, Icon::Lan);
    }

    #[test]
    fn header_lines() {
        crate::timefmt::set_local_offset_secs(0);
        let c = contact(true, Presence::InTouch);
        assert_eq!(
            header_line(&c, Some("LAN direct"), NOW),
            "In touch 5 min ago \u{00B7} LAN direct"
        );
        assert_eq!(header_line(&c, None, NOW), "In touch 5 min ago");
        let away = contact(true, Presence::NoRecent);
        assert_eq!(
            header_line(&away, Some("LAN direct"), NOW),
            "No recent contact"
        );
    }

    #[test]
    fn initials_and_dots() {
        assert_eq!(avatar_initial("ali"), "A");
        assert_eq!(avatar_initial("  "), "?");
        assert_eq!(
            avatar_initial("\u{0645}\u{06CC}\u{0646}\u{0627}"),
            "\u{0645}"
        );
        assert_eq!(presence_class(Presence::InTouch), "in-touch");
        assert_eq!(presence_class(Presence::NoAddress), "none");
    }
}
