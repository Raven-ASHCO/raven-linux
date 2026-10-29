//! What the Network page, the sidebar and the tray say about
//! `network.status` (API.md, Network; NETWORK_PARITY.md): one row per way
//! with its state chip, one plain line (what it does and why it is in that
//! state) and a switch where `network.set` can change it; then this
//! computer, router port opening, relays in use, the web proxy, Bluetooth
//! nearby (with what Linux needs: BlueZ and `Privacy = device`), who
//! carries mail both ways, the help-friends flags with their honest
//! effects, and the "Lab switches on" banner of developer builds.
//!
//! Pure and GTK-free: the GTK page only lays these out. Tested against
//! JSON fixtures and the mock (`cargo test --lib --no-default-features`).

use crate::api::{
    Build, CarrierState, CheckRow, Contact, NetworkCheck, NetworkStatus, TorState, Way,
};
use crate::strings::{tr, tr_list, trf};
use crate::timefmt;

/// How a way's state chip reads and looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    On,
    Off,
    Preview,
    Lab,
    NotOnThisOs,
    /// A state this app doesn't know yet: no chip, the engine's reason says it.
    Unknown,
}

impl Chip {
    pub fn of(state: CarrierState) -> Self {
        match state {
            CarrierState::On => Chip::On,
            CarrierState::Off => Chip::Off,
            CarrierState::Preview => Chip::Preview,
            CarrierState::Lab => Chip::Lab,
            CarrierState::NotOnThisOs => Chip::NotOnThisOs,
            CarrierState::Unknown => Chip::Unknown,
        }
    }

    /// The chip's text (empty for [`Chip::Unknown`]).
    pub fn label(self) -> &'static str {
        match self {
            Chip::On => tr("net.on"),
            Chip::Off => tr("net.wayOff"),
            Chip::Preview => tr("net.preview"),
            Chip::Lab => tr("net.lab"),
            Chip::NotOnThisOs => tr("net.notOnThisOs"),
            Chip::Unknown => "",
        }
    }

    /// The CSS class of the chip (`state-chip` plus this).
    pub fn css(self) -> &'static str {
        match self {
            Chip::On => "state-on",
            Chip::Off => "state-off",
            Chip::Preview => "state-preview",
            Chip::Lab => "state-lab",
            Chip::NotOnThisOs => "state-not-here",
            Chip::Unknown => "state-off",
        }
    }
}

/// A switch on the page: the `network.set` setting it changes, whether it
/// shows on, and whether the person can move it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toggle {
    pub setting: &'static str,
    pub active: bool,
    pub sensitive: bool,
}

/// The `network.set` setting that switches a way (API.md, Network). The
/// web proxy is not a switch (auto, off or a proxy: its own section).
pub fn setting_for_way(id: &str) -> Option<&'static str> {
    match id {
        "lan" => Some("lan"),
        "internet" => Some("internet"),
        "port_mapping" => Some("port_mapping"),
        "p2p" => Some("p2p"),
        "friend_relay" => Some("relay_for_friends"),
        "custody" => Some("friends_carry_mine"),
        "handoff" => Some("handoff"),
        "nearby" => Some("nearby"),
        _ => None,
    }
}

/// Turning a held way on is refused (NOT_AVAILABLE_YET) and turning
/// anything off is always allowed, so a held way's switch moves only while
/// it is still on.
fn toggle_for(way: &Way) -> Option<Toggle> {
    let setting = setting_for_way(&way.id).filter(|_| way.can_set)?;
    let chip = Chip::of(way.state);
    let active = way.setting.unwrap_or(matches!(chip, Chip::On | Chip::Lab));
    let sensitive = match chip {
        Chip::On | Chip::Off | Chip::Lab => true,
        Chip::Preview | Chip::NotOnThisOs => active,
        Chip::Unknown => false,
    };
    Some(Toggle {
        setting,
        active,
        sensitive,
    })
}

/// What a way does, in one plain sentence (`None` for a way this app
/// doesn't know yet: the engine's reason alone is shown).
pub fn way_does(id: &str) -> Option<&'static str> {
    Some(match id {
        "lan" => tr("network.way.lan"),
        "lan_beacon" => tr("network.way.lanBeacon"),
        "internet" => tr("network.way.internet"),
        "port_mapping" => tr("network.way.portMapping"),
        "p2p" => tr("network.way.p2p"),
        "friend_relay" => tr("network.way.friendRelay"),
        "websocket" => tr("network.way.websocket"),
        "proxy" => tr("network.way.proxy"),
        "custody" => tr("network.way.custody"),
        "letters" => tr("network.way.letters"),
        "handoff" => tr("network.way.handoff"),
        "route_hints" => tr("network.way.routeHints"),
        "nearby" => tr("network.way.nearby"),
        "tor" => tr("network.way.tor"),
        "random_port" => tr("network.way.randomPort"),
        "link_knock" => tr("network.way.linkKnock"),
        "link_padding" => tr("network.way.linkPadding"),
        "lock_v2" => tr("network.way.lockV2"),
        "security_events" => tr("network.way.securityEvents"),
        "updates_from_friends" => tr("network.way.updatesFromFriends"),
        _ => return None,
    })
}

/// One row of the Network page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WayRow {
    pub id: String,
    pub name: String,
    pub chip: Chip,
    /// What it does, then why it is in this state (the engine's reason).
    pub line: String,
    /// "In use now" / "Not in use right now", when the background program
    /// says so for a way that is on.
    pub running: Option<&'static str>,
    pub toggle: Option<Toggle>,
    /// Technical text for Details: the CLI's row, the gate and (in a
    /// developer build) the lab switches that open it.
    pub detail: String,
    /// The Tor way only: its state as raven-node reports it ([`tor_view`]).
    pub tor: Option<TorView>,
}

fn way_row(way: &Way, app_debug: bool) -> WayRow {
    let chip = Chip::of(way.state);
    // Relay for friends has no consent sheet (the engine asks none): what lending a relay
    // costs stands with what it does, beside its switch.
    let does = way_does(&way.id).map(|does| match way.id.as_str() {
        "friend_relay" => format!("{does} {}", tr("network.relayForFriends.effect")),
        _ => does.to_string(),
    });
    let line = match does {
        Some(does) if !way.reason.is_empty() => format!("{does} {}", way.reason),
        Some(does) => does,
        None => way.reason.clone(),
    };
    let running = match (way.running, chip) {
        (Some(true), _) => Some(tr("network.running.yes")),
        (Some(false), Chip::On | Chip::Lab) => Some(tr("network.running.no")),
        _ => None,
    };
    let mut detail: Vec<String> = way.detail.iter().cloned().collect();
    if !way.gate.is_empty() {
        detail.push(trf("network.gate", &[("gate", &way.gate)]));
        if app_debug && !way.lab_switches.is_empty() {
            detail.push(trf(
                "network.labSwitches",
                &[("switches", &way.lab_switches.join(", "))],
            ));
        }
    }
    let name = if way.name.is_empty() {
        way.id.clone()
    } else {
        way.name.clone()
    };
    WayRow {
        id: way.id.clone(),
        name,
        chip,
        line,
        running,
        toggle: toggle_for(way),
        detail: detail.join("\n"),
        tor: None,
    }
}

/// The rows of "Ways to reach friends", in the engine's order. The Tor
/// way carries its state from raven-node's Status ([`tor_view`]).
pub fn way_rows(status: &NetworkStatus, app_debug: bool) -> Vec<WayRow> {
    status
        .carriers
        .iter()
        .map(|w| {
            let mut row = way_row(w, app_debug);
            if w.id == "tor" {
                row.tor = tor_view(status);
            }
            row
        })
        .collect()
}

/// The Tor way's state as raven-node's Status reports it (NetworkStatus
/// `tor`): the phase chip's text and the lines under it (why, and the
/// onion service's outage history). States only: the engine never puts an
/// address here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorView {
    /// "Ready", "Starting (45 %)", "Blocked on this network"; raven-node's
    /// own words for a phase this app doesn't know.
    pub phase: String,
    /// Tor is ready (ready, publishing, or ready with the onion service
    /// unreachable).
    pub ready: bool,
    /// A phase that needs attention: blocked, could not start, the onion
    /// service unreachable.
    pub trouble: bool,
    /// "Why: …" (with "retrying in …" when raven-node says it), then
    /// "Outage history: …".
    pub lines: Vec<String>,
}

/// [`TorView`] of `status`; `None` while raven-node reports no Tor line
/// (its gate closed, a release build, or no background program).
pub fn tor_view(status: &NetworkStatus) -> Option<TorView> {
    status.tor.as_ref().map(tor_view_of)
}

fn tor_view_of(t: &TorState) -> TorView {
    let phase = match t.phase.as_str() {
        "ready" => tr("network.tor.phase.ready").to_string(),
        "publishing" => tr("network.tor.phase.publishing").to_string(),
        "unreachable" => tr("network.tor.phase.unreachable").to_string(),
        "starting" => trf(
            "network.tor.phase.starting",
            &[("percent", &t.percent.unwrap_or(0).to_string())],
        ),
        "restarting" => tr("network.tor.phase.restarting").to_string(),
        "blocked" => tr("network.tor.phase.blocked").to_string(),
        "could_not_start" => tr("network.tor.phase.couldNotStart").to_string(),
        "not_built" => tr("network.tor.phase.notBuilt").to_string(),
        "off" => tr("network.tor.phase.off").to_string(),
        _ => t.words.clone(),
    };
    let mut lines = Vec::new();
    let why: Vec<&str> = [t.why.as_deref(), t.retry.as_deref()]
        .into_iter()
        .flatten()
        .filter(|w| !w.is_empty())
        .collect();
    if !why.is_empty() {
        lines.push(trf("network.tor.why", &[("text", &why.join("; "))]));
    }
    if let Some(o) = t.outages.as_deref().filter(|o| !o.is_empty()) {
        lines.push(trf("network.tor.outages", &[("text", o)]));
    }
    TorView {
        phase,
        ready: matches!(t.phase.as_str(), "ready" | "publishing" | "unreachable"),
        trouble: matches!(
            t.phase.as_str(),
            "blocked" | "could_not_start" | "unreachable"
        ),
        lines,
    }
}

/// The rows of "Protections" (none has a switch).
pub fn protection_rows(status: &NetworkStatus, app_debug: bool) -> Vec<WayRow> {
    status
        .protections
        .iter()
        .map(|w| {
            let mut r = way_row(w, app_debug);
            r.toggle = None;
            r
        })
        .collect()
}

/// "Lab switches on: …" in a developer build when the engine sees any;
/// never in a release build (the app's or the engine's).
pub fn lab_banner(status: &NetworkStatus, app_debug: bool) -> Option<String> {
    if !app_debug || status.build == Build::Release || status.lab_switches.is_empty() {
        return None;
    }
    Some(trf(
        "network.labBanner",
        &[("switches", &status.lab_switches.join(", "))],
    ))
}

/// Every peer-to-peer address, this computer's own among them: under the
/// details only.
pub fn this_computer_details(status: &NetworkStatus) -> Vec<String> {
    status
        .p2p_addresses
        .iter()
        .map(|a| trf("network.thisComputer.p2pAddress", &[("addr", a)]))
        .collect()
}

/// A multiaddr (`/ip4/127.0.0.1/tcp/7423/...`) whose IP only this computer
/// or this link can reach: loopback, link-local or unspecified.
pub fn p2p_address_is_local_only(multiaddr: &str) -> bool {
    let mut parts = multiaddr.split('/').filter(|p| !p.is_empty());
    let (Some(proto), Some(ip)) = (parts.next(), parts.next()) else {
        return false;
    };
    let ip = ip.to_ascii_lowercase();
    match proto {
        "ip4" => ip.starts_with("127.") || ip.starts_with("169.254.") || ip == "0.0.0.0",
        "ip6" => {
            let bare = ip.split('%').next().unwrap_or_default();
            bare == "::1"
                || bare == "::"
                || ["fe8", "fe9", "fea", "feb"]
                    .iter()
                    .any(|p| bare.starts_with(p))
        }
        _ => false,
    }
}

/// "This computer": the port, where it listens, the internet and
/// peer-to-peer ports, a LAN meeting's end (seen at `now_ms`).
pub fn this_computer(status: &NetworkStatus, now_ms: u64) -> Vec<String> {
    let mut lines = Vec::new();
    if status.node != "running" {
        lines.push(tr("network.thisComputer.nodeDown").to_string());
    }
    if status.port > 0 {
        lines.push(trf(
            "network.thisComputer.port",
            &[("port", &status.port.to_string())],
        ));
    }
    for addr in &status.listening {
        lines.push(trf("network.thisComputer.listening", &[("addr", addr)]));
    }
    if status.listening.is_empty() && status.node == "running" {
        lines.push(tr("network.thisComputer.notListening").to_string());
    }
    if let Some(at) = &status.internet_listen {
        lines.push(trf("network.thisComputer.internet", &[("addr", at)]));
    }
    if let Some(at) = &status.p2p_listen {
        lines.push(trf("network.thisComputer.p2p", &[("addr", at)]));
    }
    // Addresses friends could use; this computer's own (loopback,
    // link-local) only under the details (E1: six rows, loopback ones among
    // them).
    for addr in status
        .p2p_addresses
        .iter()
        .filter(|a| !p2p_address_is_local_only(a))
        .take(4)
    {
        lines.push(trf("network.thisComputer.p2pAddress", &[("addr", addr)]));
    }
    if let Some(until) = status.lan_until_ms {
        lines.push(trf(
            "network.thisComputer.lanUntil",
            &[("time", &timefmt::deadline(now_ms, until))],
        ));
    }
    if status.policy_unreadable {
        lines.push(tr("network.thisComputer.policyUnreadable").to_string());
    }
    lines
}

fn on_off(on: bool) -> &'static str {
    if on {
        tr("network.value.on")
    } else {
        tr("network.value.off")
    }
}

/// "Router port opening": what Raven asks and what the router answered.
pub fn port_mapping_lines(status: &NetworkStatus) -> Vec<String> {
    let pm = &status.port_mapping;
    let setting = match pm.setting.as_str() {
        "on" => tr("network.value.on"),
        "off" => tr("network.value.off"),
        _ => tr("network.value.notDecided"),
    };
    let router = if pm.router.is_empty() {
        tr("network.value.none").to_string()
    } else {
        pm.router.clone()
    };
    let nat = match Chip::of(pm.nat_pmp_pcp) {
        Chip::Unknown => tr("network.value.none"),
        chip => chip.label(),
    };
    let mut lines = vec![
        trf("network.portMapping.setting", &[("setting", setting)]),
        trf("network.portMapping.router", &[("result", &router)]),
        trf("network.portMapping.natPmpPcp", &[("state", nat)]),
    ];
    // v4 and v6 are raven-node's words (`upnp`, `none`, `open`, ...), the port the router forwards.
    if let Some(v4) = &pm.v4 {
        lines.push(match pm.v4_port {
            Some(port) if port > 0 => trf(
                "network.portMapping.v4Port",
                &[("how", v4), ("port", &port.to_string())],
            ),
            _ => trf("network.portMapping.v4", &[("how", v4)]),
        });
    }
    if let Some(v6) = &pm.v6 {
        lines.push(trf("network.portMapping.v6", &[("how", v6)]));
    }
    lines
}

/// The friend's name for an address, else its short form.
fn friend_name(contacts: &[Contact], address: &str) -> String {
    contacts
        .iter()
        .find(|c| c.address == address)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| short_address(address))
}

/// `rvn1qam4hgx8d…qfmzx` for an address the app doesn't know by name.
pub fn short_address(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    if chars.len() <= 16 {
        return address.to_string();
    }
    let head: String = chars[..10].iter().collect();
    let tail: String = chars[chars.len() - 5..].iter().collect();
    format!("{head}\u{2026}{tail}")
}

/// "Relays in use": each relay, its state and who chose it.
pub fn relay_lines(status: &NetworkStatus, contacts: &[Contact]) -> Vec<String> {
    if status.relays.is_empty() {
        return vec![tr("network.relays.none").to_string()];
    }
    status
        .relays
        .iter()
        .map(|r| {
            let line = trf(
                "network.relays.line",
                &[("relay", &r.relay), ("state", &r.state)],
            );
            let source = if r.source == "offer" {
                let who = match r.friend_address.as_deref() {
                    Some(a) => friend_name(contacts, a),
                    None => tr("network.relays.aFriend").to_string(),
                };
                trf("network.relays.offer", &[("name", &who)])
            } else {
                tr("network.relays.manual").to_string()
            };
            format!("{line} \u{b7} {source}")
        })
        .collect()
}

/// The web proxy setting as the dropdown shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyChoice {
    Auto,
    Off,
    Custom(String),
}

impl ProxyChoice {
    pub fn of(setting: &str) -> Self {
        match setting.trim() {
            "" | "auto" => ProxyChoice::Auto,
            s if s.eq_ignore_ascii_case("off") => ProxyChoice::Off,
            s => ProxyChoice::Custom(s.to_string()),
        }
    }

    /// Index in `network.proxy.options`.
    pub fn index(&self) -> u32 {
        match self {
            ProxyChoice::Auto => 0,
            ProxyChoice::Off => 1,
            ProxyChoice::Custom(_) => 2,
        }
    }
}

/// The dropdown's texts (automatic, off, this proxy).
pub fn proxy_options() -> Vec<&'static str> {
    tr_list("network.proxy.options")
}

/// The `network.set {proxy}` value for the dropdown's choice; a typed
/// proxy must be `http://host:port`, never with a user name or password.
pub fn proxy_value(index: u32, typed: &str) -> Result<String, String> {
    match index {
        0 => Ok("auto".into()),
        1 => Ok("off".into()),
        _ => {
            let t = typed.trim();
            let rest = t.strip_prefix("http://").unwrap_or("");
            let port_ok = rest
                .rsplit_once(':')
                .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
            if t.contains('@') || !port_ok {
                Err(tr("network.proxy.noCredentials").to_string())
            } else {
                Ok(t.to_string())
            }
        }
    }
}

/// "In use now: …" and the engine's proxy lines.
pub fn proxy_lines(status: &NetworkStatus) -> Vec<String> {
    let mut lines = Vec::new();
    if !status.proxy.in_use.is_empty() {
        lines.push(trf(
            "network.proxy.inUse",
            &[("proxy", &status.proxy.in_use)],
        ));
    }
    lines.extend(status.proxy.lines.iter().cloned());
    lines
}

/// The Bluetooth nearby section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearbyView {
    pub lines: Vec<String>,
    /// What Linux needs (BlueZ and `Privacy = device`), on a computer that
    /// can have it.
    pub linux_note: Option<&'static str>,
    /// The adapter advertises from its fixed address: Raven keeps it off.
    pub warning: Option<&'static str>,
}

pub fn nearby_view(status: &NetworkStatus) -> NearbyView {
    let n = &status.nearby;
    let not_here = !n.on_this_os
        || status
            .way("nearby")
            .is_some_and(|w| w.state == CarrierState::NotOnThisOs);
    if not_here {
        return NearbyView {
            lines: vec![tr("network.nearby.notHere").to_string()],
            linux_note: None,
            warning: None,
        };
    }
    let mut lines = Vec::new();
    let state = match n.state.as_str() {
        "held" => Some(tr("network.nearby.state.held")),
        "node_held" => Some(tr("network.nearby.state.nodeHeld")),
        "not_running" => Some(tr("network.nearby.state.notRunning")),
        "live" => Some(tr("network.nearby.state.live")),
        _ => None,
    };
    lines.extend(state.map(str::to_string));
    let saved = match n.saved {
        Some(on) => on_off(on),
        None => tr("network.value.notDecided"),
    };
    lines.push(trf("network.nearby.saved", &[("value", saved)]));
    if let Some(radio) = &n.radio {
        lines.push(trf("network.nearby.radio", &[("value", radio)]));
    }
    if let Some(p) = &n.address_privacy {
        lines.push(trf("network.nearby.address", &[("value", p)]));
    }
    if n.in_range.is_some() || n.connected.is_some() {
        lines.push(trf(
            "network.nearby.inRange",
            &[
                ("n", &n.in_range.unwrap_or(0).to_string()),
                ("connected", &n.connected.unwrap_or(0).to_string()),
            ],
        ));
    }
    for r in &n.rows {
        lines.push(format!("{}: {}", r.label, r.value));
    }
    NearbyView {
        lines,
        linux_note: Some(tr("network.nearby.linux")),
        warning: (n.address_privacy.as_deref() == Some("public"))
            .then(|| tr("network.nearby.publicAddress")),
    }
}

/// A byte count in words ("2 KB").
pub fn size_text(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        trf("network.size.bytes", &[("n", &format!("{}", bytes.max(0)))])
    } else if b < 1024.0 * 1024.0 {
        trf("network.size.kb", &[("n", &format!("{:.1}", b / 1024.0))])
    } else {
        trf(
            "network.size.mb",
            &[("n", &format!("{:.1}", b / (1024.0 * 1024.0)))],
        )
    }
}

/// "Who carries mail": for you, for your friends, the relay you lend,
/// and the two switches of this section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailView {
    pub lines: Vec<String>,
    /// "Carry mail for friends" (`carry_for_friends`, needs a yes).
    pub carry_for_friends: Toggle,
    /// "Hide this computer's IP address from friends" (`hide_ip`).
    pub hide_ip: Toggle,
}

pub fn mail_view(status: &NetworkStatus) -> MailView {
    let c = &status.carry;
    let mut lines = Vec::new();
    let mine = &c.friends_carry_mine;
    lines.push(match mine.setting.as_str() {
        "on" => trf(
            "network.mail.mine.on",
            &[("n", &mine.mail_friends.to_string())],
        ),
        "off" => tr("network.mail.mine.off").to_string(),
        _ => tr("network.mail.mine.notDecided").to_string(),
    });
    if mine.offering > 0 {
        lines.push(trf(
            "network.mail.mine.offering",
            &[
                ("n", &mine.offering.to_string()),
                ("internet", &mine.offering_over_internet.to_string()),
                ("reachable", &mine.offering_reachable.to_string()),
            ],
        ));
    }
    if mine.listed_by_hand > 0 {
        lines.push(trf(
            "network.mail.mine.byHand",
            &[("n", &mine.listed_by_hand.to_string())],
        ));
    }
    let theirs = &c.carry_for_friends;
    lines.push(if theirs.on {
        trf(
            "network.mail.theirs.on",
            &[
                ("n", &theirs.held_messages.max(0).to_string()),
                ("size", &size_text(theirs.held_bytes)),
            ],
        )
    } else {
        tr("network.mail.theirs.off").to_string()
    });
    if theirs.handoff {
        lines.push(tr("network.mail.handoff").to_string());
        lines.extend(theirs.handoff_lines.iter().cloned());
    }
    if !c.letters_left.is_empty() {
        lines.push(tr("network.carry.letters").to_string());
        lines.extend(c.letters_left.iter().cloned());
    }
    let relay = &c.relay_for_friends;
    lines.push(match (relay.setting, relay.offering) {
        (_, true) => trf(
            "network.mail.relay.on",
            &[("n", &relay.offered_to.to_string())],
        ),
        (true, false) => tr("network.mail.relay.waiting").to_string(),
        (false, false) => tr("network.mail.relay.off").to_string(),
    });
    // The custody gate decides whether carrying for friends can be turned
    // on; turning it off always works.
    let custody_open = status
        .way("custody")
        .is_some_and(|w| matches!(Chip::of(w.state), Chip::On | Chip::Off | Chip::Lab));
    MailView {
        lines,
        carry_for_friends: Toggle {
            setting: "carry_for_friends",
            active: theirs.on,
            sensitive: custody_open || theirs.on,
        },
        hide_ip: Toggle {
            setting: "hide_ip",
            active: status.hide_ip,
            sensitive: true,
        },
    }
}

/// One help-friends flag with what it really does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpRow {
    pub label: &'static str,
    pub toggle: Toggle,
    pub effect: String,
}

/// "Helping other devices": bridge, store and relay, and whether Raven
/// still chooses them by itself.
pub fn help_rows(status: &NetworkStatus) -> (Vec<HelpRow>, &'static str) {
    let h = &status.help;
    let row = |label, setting, active, effect: &str| HelpRow {
        label,
        toggle: Toggle {
            setting,
            active,
            sensitive: true,
        },
        effect: effect.to_string(),
    };
    let rows = vec![
        row(
            tr("network.help.bridge"),
            "bridge",
            h.bridge,
            &h.effects.bridge,
        ),
        row(tr("network.help.store"), "store", h.store, &h.effects.store),
        row(tr("network.help.relay"), "relay", h.relay, &h.effects.relay),
    ];
    let how = if h.automatic {
        tr("network.help.automatic")
    } else {
        tr("network.help.manual")
    };
    (rows, how)
}

/// A short name for a way in the tray and the sidebar ("LAN", "p2p").
pub fn short_name(way: &Way) -> String {
    let key = match way.id.as_str() {
        "lan" => tr("network.path.lan"),
        "internet" => tr("network.path.internet"),
        "p2p" => tr("network.path.p2p"),
        "websocket" => tr("network.path.websocket"),
        "nearby" => tr("network.path.nearby"),
        "custody" => tr("network.path.friend"),
        "handoff" => tr("network.path.handoff"),
        "letters" => tr("network.path.letter"),
        "tor" => tr("network.tor.name"),
        _ => return way.name.clone(),
    };
    key.to_string()
}

/// "Ways on: LAN · p2p (lab)": the ways to reach friends that are on now,
/// for the tray menu and the sidebar entry.
pub fn ways_on_line(status: &NetworkStatus) -> String {
    let ways: Vec<String> = status
        .carriers
        .iter()
        .filter_map(|w| match Chip::of(w.state) {
            Chip::On => Some(short_name(w)),
            Chip::Lab => Some(trf("network.tray.labWay", &[("name", &short_name(w))])),
            _ => None,
        })
        .collect();
    if ways.is_empty() {
        tr("network.tray.noWays").to_string()
    } else {
        trf("network.tray.ways", &[("ways", &ways.join(" \u{00B7} "))])
    }
}

/// What the tray shows about the network.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkSummary {
    /// "Ways on: …".
    pub ways: String,
    /// How many ways to reach friends are on (or on for testing): the
    /// sidebar's "n ways on".
    pub on: usize,
    /// A developer build with lab switches set.
    pub lab: bool,
}

pub fn summary(status: &NetworkStatus, app_debug: bool) -> NetworkSummary {
    NetworkSummary {
        ways: ways_on_line(status),
        on: status
            .carriers
            .iter()
            .filter(|w| matches!(Chip::of(w.state), Chip::On | Chip::Lab))
            .count(),
        lab: lab_banner(status, app_debug).is_some(),
    }
}

/// The colour and icon of a way's card on the Network page (the path
/// colours of the tokens: LAN, Bluetooth, Tor, the internet, relays and
/// friends who carry mail; anything else stays neutral).
pub fn way_style(id: &str) -> crate::present::PathStyle {
    use crate::icons::Icon;
    let (class, icon) = match id {
        "lan" | "lan_beacon" | "random_port" => (Some("lan"), Icon::Lan),
        "nearby" => (Some("nearby"), Icon::Nearby),
        "tor" | "onion" => (Some("tor"), Icon::Tor),
        "internet" | "port_mapping" => (Some("direct"), Icon::Direct),
        "p2p" | "friend_relay" | "websocket" | "proxy" => (Some("relay"), Icon::Relay),
        "custody" | "letters" | "handoff" | "route_hints" => (Some("friend"), Icon::Friend),
        "link_knock" | "link_padding" | "security_events" => (None, Icon::Shield),
        "lock_v2" => (None, Icon::Lock),
        "updates_from_friends" => (None, Icon::Updates),
        _ => (None, Icon::Net),
    };
    crate::present::PathStyle { class, icon }
}

/// The Check network page (`network.check`, `raven doctor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckView {
    /// `raven doctor`'s summary as label and value.
    pub summary: Vec<(String, String)>,
    pub next_step: Option<String>,
    /// The network lines: Raven's plain sentence where it has one, else
    /// `key: value`; a line under the one before it (same key) is indented.
    pub rows: Vec<String>,
    /// The router's answers, after "Ask my router".
    pub router: Option<Vec<String>>,
}

fn check_lines(rows: &[CheckRow]) -> Vec<String> {
    let mut out = Vec::new();
    let mut last_key = "";
    for r in rows {
        let text = match (&r.plain, r.key.is_empty()) {
            (Some(p), _) => p.clone(),
            (None, true) => r.value.clone(),
            (None, false) if r.key == last_key => format!("    {}", r.value),
            (None, false) => format!("{}: {}", r.key, r.value),
        };
        last_key = &r.key;
        out.push(text);
    }
    out
}

pub fn check_view(c: &NetworkCheck) -> CheckView {
    CheckView {
        summary: c
            .summary
            .iter()
            .filter(|r| r.key != "next_step")
            .map(|r| (r.label.clone(), r.value.clone()))
            .collect(),
        next_step: (!c.next_step.is_empty())
            .then(|| trf("network.check.nextStep", &[("step", &c.next_step)])),
        rows: check_lines(&c.rows),
        router: c.router.as_deref().map(check_lines),
    }
}

#[cfg(test)]
mod style_tests {
    use super::*;

    #[test]
    fn way_cards_take_the_path_colours() {
        assert_eq!(way_style("lan").class, Some("lan"));
        assert_eq!(way_style("custody").class, Some("friend"));
        assert_eq!(way_style("p2p").class, Some("relay"));
        assert_eq!(way_style("internet").class, Some("direct"));
        assert_eq!(way_style("brand_new").class, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::decode;
    use serde_json::json;

    fn status(carriers: serde_json::Value) -> NetworkStatus {
        decode(json!({
            "build": "dev", "node": "running", "carriers": carriers, "protections": [],
            "listening": ["0.0.0.0:7420"], "port": 7431,
            "port_mapping": {"setting": "on", "router": "mapped 7423", "nat_pmp_pcp": "lab",
                             "v4": "upnp", "v4_port": 7423},
            "relays": [{"relay": "/ip4/203.0.113.7/tcp/4001", "state": "reserved", "source": "manual"},
                       {"relay": "/ip4/192.0.2.9/tcp/7423", "state": "reserved", "source": "offer",
                        "friend_address": "rvn1qlilyaddress0000000000000000000"}],
            "nearby": {"state": "live", "saved": true, "on_this_os": true, "radio": "powered_on",
                       "address_privacy": "public", "in_range": 1, "connected": 0, "rows": []},
            "carry": {"friends_carry_mine": {"setting": "on", "mail_friends": 2, "offering": 3,
                        "offering_over_internet": 1, "offering_reachable": 2},
                      "carry_for_friends": {"on": true, "held_messages": 4, "held_bytes": 3072,
                        "handoff": true, "handoff_lines": ["passed on: 1"]},
                      "letters_left": [], "relay_for_friends": {"setting": true, "offering": false}},
            "help": {"bridge": true, "store": false, "relay": false, "automatic": false,
                     "effects": {"bridge": "b", "store": "s", "relay": "r"}},
            "lab_switches": ["RAVEN_LAB_TEST_A"]
        }))
        .unwrap()
    }

    #[test]
    fn the_tor_way_says_raven_nodes_state_why_and_outage_history() {
        let mut s = status(json!([way("tor", "lab", None, Some(false))]));
        let row = &way_rows(&s, true)[0];
        assert!(row.tor.is_none(), "no Tor line in Status: nothing said");
        assert!(row.line.starts_with(tr("network.way.tor")));
        assert_eq!(short_name(s.way("tor").unwrap()), "Tor");
        assert_eq!(way_style("tor").class, Some("tor"));
        let state = |phase: &str| TorState {
            phase: phase.into(),
            words: format!("{phase} (as raven-node says it)"),
            ..TorState::default()
        };
        s.tor = Some(TorState {
            percent: Some(45),
            ..state("starting")
        });
        let t = way_rows(&s, true)[0].tor.clone().unwrap();
        assert_eq!(t.phase, "Starting (45 %)");
        assert!(!t.ready && !t.trouble && t.lines.is_empty());
        s.tor = Some(TorState {
            why: Some("self-check did not reach it".into()),
            outages: Some("onion service outages since start: 2".into()),
            ..state("unreachable")
        });
        let t = way_rows(&s, true)[0].tor.clone().unwrap();
        assert_eq!(t.phase, "Onion service unreachable");
        assert!(t.ready && t.trouble);
        assert_eq!(
            t.lines,
            vec![
                "Why: self-check did not reach it".to_string(),
                "Outage history: onion service outages since start: 2".to_string()
            ]
        );
        s.tor = Some(TorState {
            why: Some("bridges needed".into()),
            retry: Some("retrying in 2 min".into()),
            ..state("blocked")
        });
        let t = way_rows(&s, true)[0].tor.clone().unwrap();
        assert_eq!(t.phase, "Blocked on this network");
        assert!(!t.ready && t.trouble);
        assert_eq!(
            t.lines,
            vec!["Why: bridges needed; retrying in 2 min".to_string()]
        );
        for (phase, words) in [
            ("ready", "Ready"),
            ("publishing", "Publishing"),
            ("restarting", "Restarting"),
            ("could_not_start", "Could not start"),
            ("not_built", "Not built in"),
            ("off", "Off"),
            ("something_new", "something_new (as raven-node says it)"),
        ] {
            s.tor = Some(state(phase));
            assert_eq!(way_rows(&s, true)[0].tor.clone().unwrap().phase, words);
        }
    }

    fn way(
        id: &str,
        state: &str,
        setting: Option<bool>,
        running: Option<bool>,
    ) -> serde_json::Value {
        let mut w = json!({"id": id, "name": id, "state": state, "reason": "Why.",
                           "gate": "G", "lab_switches": ["RAVEN_LAB_X"], "can_set": true});
        if let Some(s) = setting {
            w["setting"] = s.into();
        }
        if let Some(r) = running {
            w["running"] = r.into();
        }
        w
    }

    #[test]
    fn every_state_has_its_chip_and_switch() {
        let s = status(json!([
            way("lan", "on", Some(true), Some(true)),
            way("internet", "off", Some(false), None),
            way("p2p", "preview", Some(false), None),
            way("nearby", "not_on_this_os", Some(false), None),
            way("handoff", "lab", Some(true), Some(false)),
            way("friend_relay", "preview", Some(true), None),
            way("custody", "brand_new", Some(false), None),
            way("websocket", "on", None, Some(true)),
        ]));
        let rows = way_rows(&s, true);
        let chips: Vec<Chip> = rows.iter().map(|r| r.chip).collect();
        assert_eq!(
            chips,
            vec![
                Chip::On,
                Chip::Off,
                Chip::Preview,
                Chip::NotOnThisOs,
                Chip::Lab,
                Chip::Preview,
                Chip::Unknown,
                Chip::On
            ]
        );
        assert_eq!(Chip::On.label(), "On");
        assert_eq!(Chip::Off.label(), "Off");
        assert_eq!(Chip::Preview.label(), "Preview, not in this release");
        assert_eq!(Chip::Lab.label(), "Lab only");
        assert_eq!(Chip::NotOnThisOs.label(), "Not on this computer");
        assert_eq!(Chip::Unknown.label(), "");
        let t = |i: usize| rows[i].toggle.clone().unwrap();
        assert_eq!(
            (t(0).setting, t(0).active, t(0).sensitive),
            ("lan", true, true)
        );
        assert_eq!(
            (t(1).active, t(1).sensitive),
            (false, true),
            "off: can turn on"
        );
        assert_eq!(
            (t(2).active, t(2).sensitive),
            (false, false),
            "held: can't turn on"
        );
        assert!(!t(3).sensitive, "not on this computer");
        assert_eq!(
            (t(4).setting, t(4).active, t(4).sensitive),
            ("handoff", true, true)
        );
        assert_eq!(
            (t(5).setting, t(5).active, t(5).sensitive),
            ("relay_for_friends", true, true),
            "a held way that is on can still be turned off"
        );
        assert!(!t(6).sensitive, "an unknown state is never switched");
        assert!(rows[7].toggle.is_none(), "websocket has no setting");
        // The line: what it does, then why.
        assert_eq!(
            rows[0].line,
            "Reaches friends on the same Wi-Fi, cable or hotspot, even without the internet. Why."
        );
        assert_eq!(rows[0].running, Some("In use now"));
        assert_eq!(rows[4].running, Some("Not in use now"));
        assert_eq!(rows[1].running, None);
        assert!(rows[4].detail.contains("RAVEN_LAB_X"));
        assert!(
            !way_rows(&s, false)[4].detail.contains("RAVEN_LAB_X"),
            "release builds don't name lab switches"
        );
    }

    #[test]
    fn the_lab_banner_is_for_developer_builds_only() {
        let mut s = status(json!([]));
        assert_eq!(
            lab_banner(&s, true).as_deref(),
            Some(
                "Lab switches on: RAVEN_LAB_TEST_A. Ways marked Lab run for testing only, in \
                 this developer build."
            )
        );
        assert_eq!(lab_banner(&s, false), None, "never in a release app");
        s.build = Build::Release;
        assert_eq!(lab_banner(&s, true), None, "never with a release engine");
        s.build = Build::Dev;
        s.lab_switches.clear();
        assert_eq!(lab_banner(&s, true), None, "only when a switch is set");
    }

    /// E1: the page lists the peer-to-peer addresses friends could use;
    /// this computer's own (loopback, link-local) only under the details.
    #[test]
    fn local_only_peer_to_peer_addresses_are_details() {
        let mut s = status(json!([]));
        s.p2p_addresses = vec![
            "/ip4/127.0.0.1/tcp/17423/p2p/12D3".into(),
            "/ip4/192.168.0.16/tcp/17423/p2p/12D3".into(),
            "/ip6/::1/tcp/17423/p2p/12D3".into(),
            "/ip6/fe80::1%en0/tcp/17423".into(),
            "/ip4/169.254.3.4/udp/17423/quic-v1".into(),
            "/ip6/2001:db8::7/tcp/17423".into(),
        ];
        let shown: Vec<String> = this_computer(&s, 0)
            .into_iter()
            .filter(|l| l.starts_with("Peer-to-peer address"))
            .collect();
        assert_eq!(
            shown,
            vec![
                "Peer-to-peer address: /ip4/192.168.0.16/tcp/17423/p2p/12D3",
                "Peer-to-peer address: /ip6/2001:db8::7/tcp/17423"
            ]
        );
        assert_eq!(this_computer_details(&s).len(), 6);
        assert!(!tr("network.way.randomPort").contains("7420"));
    }

    #[test]
    fn sections_say_what_the_engine_says() {
        crate::timefmt::set_local_offset_secs(0);
        let mut s = status(json!([way("custody", "lab", Some(true), Some(true))]));
        s.internet_listen = Some("0.0.0.0:7422".into());
        let here = this_computer(&s, 0);
        assert_eq!(here[0], "This profile's port: 7431");
        assert_eq!(here[1], "Listening on 0.0.0.0:7420");
        assert_eq!(here[2], "Internet direct listens on 0.0.0.0:7422");
        let pm = port_mapping_lines(&s);
        assert_eq!(
            pm,
            vec![
                "Router port opening: on",
                "Router's answer: mapped 7423",
                "NAT-PMP and PCP: Lab only",
                "IPv4: upnp, port 7423"
            ]
        );
        let lily = Contact {
            address: "rvn1qlilyaddress0000000000000000000".into(),
            name: "lily".into(),
            short_address: String::new(),
            verified: true,
            presence: Default::default(),
            last_contact_ms: None,
            waiting: None,
            avatar_index: 0,
            lan: None,
            unread: 0,
        };
        let relays = relay_lines(&s, &[lily]);
        assert_eq!(
            relays[0],
            "/ip4/203.0.113.7/tcp/4001 · reserved · added by you"
        );
        assert_eq!(
            relays[1],
            "/ip4/192.0.2.9/tcp/7423 · reserved · lent by lily"
        );
        assert_eq!(
            relay_lines(&s, &[])[1],
            "/ip4/192.0.2.9/tcp/7423 · reserved · lent by rvn1qlilya…00000"
        );
        let mail = mail_view(&s);
        assert_eq!(
            mail.lines[0],
            "Friends may carry your mail: 2 mail friends."
        );
        assert!(mail
            .lines
            .iter()
            .any(|l| l.contains("4 messages held (3.0 KB)")));
        assert!(mail.lines.iter().any(|l| l == "passed on: 1"));
        assert!(mail
            .lines
            .last()
            .unwrap()
            .starts_with("Relay for friends is on"));
        assert!(mail.carry_for_friends.active && mail.carry_for_friends.sensitive);
        let (help, how) = help_rows(&s);
        assert_eq!(help[0].toggle.setting, "bridge");
        assert!(help[0].toggle.active && !help[1].toggle.active);
        assert_eq!(how, "Set by hand.");
    }

    #[test]
    fn nearby_explains_bluez_on_linux() {
        let s = status(json!([way("nearby", "lab", Some(true), Some(false))]));
        let n = nearby_view(&s);
        assert!(n.linux_note.unwrap().contains("Privacy = device"));
        assert!(n.linux_note.unwrap().contains("/etc/bluetooth/main.conf"));
        assert!(n.warning.is_some(), "a public address keeps nearby off");
        assert!(n
            .lines
            .contains(&"Friends in range: 1 · connected: 0".to_string()));
        let gone = status(json!([way("nearby", "not_on_this_os", Some(false), None)]));
        let n = nearby_view(&gone);
        assert_eq!(
            n.lines,
            vec!["This computer has no Bluetooth LE that Raven can use."]
        );
        assert!(n.linux_note.is_none() && n.warning.is_none());
    }

    #[test]
    fn the_proxy_is_auto_off_or_host_and_port() {
        assert_eq!(ProxyChoice::of("auto"), ProxyChoice::Auto);
        assert_eq!(ProxyChoice::of("OFF").index(), 1);
        assert_eq!(
            ProxyChoice::of("http://192.0.2.1:8080"),
            ProxyChoice::Custom("http://192.0.2.1:8080".into())
        );
        assert_eq!(proxy_value(0, "").unwrap(), "auto");
        assert_eq!(proxy_value(1, "").unwrap(), "off");
        assert_eq!(
            proxy_value(2, " http://192.0.2.1:8080 ").unwrap(),
            "http://192.0.2.1:8080"
        );
        assert!(proxy_value(2, "http://max:secret@192.0.2.1:8080").is_err());
        assert!(proxy_value(2, "socks5://192.0.2.1:1080").is_err());
        assert!(proxy_value(2, "http://192.0.2.1").is_err());
        assert_eq!(proxy_options().len(), 3);
    }

    #[test]
    fn check_network_reads_like_raven_doctor() {
        let c: NetworkCheck = decode(json!({
            "summary": [{"key": "identity", "label": "identity", "value": "OK"},
                        {"key": "next_step", "label": "next step", "value": "nothing to do"}],
            "next_step": "nothing to do",
            "rows": [{"key": "receiving", "value": "yes", "plain": "Raven receives messages."},
                     {"key": "relay_1", "value": "reserved"},
                     {"key": "relay_1", "value": "via tcp"},
                     {"key": "", "value": "a loose line"}]
        }))
        .unwrap();
        let v = check_view(&c);
        assert_eq!(v.summary, vec![("identity".to_string(), "OK".to_string())]);
        assert_eq!(v.next_step.as_deref(), Some("Next step: nothing to do"));
        assert_eq!(
            v.rows,
            vec![
                "Raven receives messages.",
                "relay_1: reserved",
                "    via tcp",
                "a loose line"
            ]
        );
        assert_eq!(v.router, None);
    }

    #[test]
    fn the_tray_line_names_the_ways_that_are_on() {
        let s = status(json!([
            way("lan", "on", Some(true), Some(true)),
            way("p2p", "lab", Some(true), Some(true)),
            way("internet", "off", Some(false), None),
            way("nearby", "preview", Some(false), None),
        ]));
        assert_eq!(ways_on_line(&s), "Ways on: LAN \u{00B7} p2p (lab)");
        let sum = summary(&s, true);
        assert!(sum.lab);
        assert!(!summary(&s, false).lab);
        let none = status(json!([way("lan", "off", Some(false), None)]));
        assert_eq!(ways_on_line(&none), "No way to reach friends is on");
        assert_eq!(size_text(10), "10 bytes");
        assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");
    }
}

#[cfg(all(test, debug_assertions))]
mod with_mock {
    use super::*;
    use crate::api::RavenEngine;
    use crate::mock::{Clock, MockRaven};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn mock(lab: Option<bool>) -> MockRaven {
        let t = Arc::new(AtomicU64::new(1_792_505_100_000));
        let m = MockRaven::with_clock(Clock::Manual(t), true);
        match lab {
            Some(bluetooth) => m.with_lab_network(bluetooth),
            None => m,
        }
    }

    #[test]
    fn a_release_build_shows_lan_on_and_every_other_way_as_preview() {
        let s = mock(None).network_status().unwrap();
        let rows = way_rows(&s, true);
        assert_eq!(rows[0].id, "lan");
        assert_eq!(rows[0].chip, Chip::On);
        assert!(rows[0].toggle.as_ref().unwrap().sensitive);
        for r in &rows[1..] {
            assert_eq!(r.chip, Chip::Preview, "{}", r.id);
            assert!(r.toggle.as_ref().is_none_or(|t| !t.sensitive), "{}", r.id);
            assert!(
                r.line.ends_with("Not in this version of Raven yet."),
                "{}",
                r.line
            );
        }
        assert!(protection_rows(&s, true).iter().all(|r| r.toggle.is_none()));
        assert_eq!(lab_banner(&s, true), None);
        assert_eq!(ways_on_line(&s), "Ways on: LAN");
        assert!(!mail_view(&s).carry_for_friends.sensitive);
    }

    #[test]
    fn the_lab_mock_shows_tor_ready_with_its_outage_history_while_the_node_runs() {
        let m = mock(Some(true));
        let s = m.network_status().unwrap();
        let tor = way_rows(&s, true)
            .into_iter()
            .find(|r| r.id == "tor")
            .unwrap();
        assert_eq!(tor.chip, Chip::Lab);
        assert!(tor.toggle.is_none(), "Tor has no switch");
        assert!(tor.detail.contains("ONION_PRODUCTION_ENABLED"));
        if s.node != "running" {
            assert!(tor.tor.is_none(), "no background program, no Tor line");
            m.node_start().unwrap();
        }
        let s = m.network_status().unwrap();
        let tor = way_rows(&s, true)
            .into_iter()
            .find(|r| r.id == "tor")
            .unwrap();
        let t = tor.tor.unwrap();
        assert_eq!(t.phase, "Ready");
        assert!(t.ready && !t.trouble);
        assert_eq!(
            t.lines,
            vec![
                "Outage history: onion service outages since start: 1, the last lasted 2 min \
                 and ended 1 h ago"
                    .to_string()
            ]
        );
        assert_eq!(tor.running, Some(tr("network.running.yes")));
        // A release build: Tor held like every other way, no state.
        let s = mock(None).network_status().unwrap();
        let tor = way_rows(&s, true)
            .into_iter()
            .find(|r| r.id == "tor")
            .unwrap();
        assert_eq!(tor.chip, Chip::Preview);
        assert!(tor.tor.is_none());
    }

    #[test]
    fn a_lab_build_shows_lab_and_off_ways_and_the_banner() {
        let m = mock(Some(true));
        let s = m.network_status().unwrap();
        let rows = way_rows(&s, true);
        let by = |id: &str| rows.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(by("p2p").chip, Chip::Off);
        assert!(
            by("p2p").toggle.unwrap().sensitive,
            "a lab way can be turned on"
        );
        assert_eq!(by("route_hints").chip, Chip::Lab);
        assert_eq!(by("websocket").chip, Chip::Preview);
        assert!(lab_banner(&s, true).unwrap().contains("RAVEN_LAB_TEST_A"));
        assert!(mail_view(&s).carry_for_friends.sensitive);
        assert!(nearby_view(&s).warning.is_some());
        let check = check_view(&m.network_check(true).unwrap());
        assert!(check
            .router
            .unwrap()
            .contains(&"No UPnP router answered.".to_string()));
        let s = mock(Some(false)).network_status().unwrap();
        let nearby = way_rows(&s, true)
            .into_iter()
            .find(|r| r.id == "nearby")
            .unwrap();
        assert_eq!(nearby.chip, Chip::NotOnThisOs);
        assert!(!nearby.toggle.unwrap().sensitive);
    }
}
