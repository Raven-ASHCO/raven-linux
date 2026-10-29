//! A friend's routes panel (API.md, `contacts.routes` /
//! `contacts.set_routes`): the saved routes with their state chips, what
//! the friend told this computer (route hints), who carries the friend's
//! mail, hide my IP from this friend, and the form that changes them.
//! Never the friend's security words or old code (the engine never sends
//! them). Pure and GTK-free.

use crate::api::{Contact, ContactCarriers, ContactRoutes, RoutesPatch};
use crate::netview::{short_address, Chip};
use crate::strings::{tr, trf};
use crate::timefmt;

/// One saved route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRow {
    /// `lan`, `internet`, `p2p` or `nearby`.
    pub kind: String,
    pub label: String,
    pub value: String,
    /// "through …", "next hop: …".
    pub extra: Vec<String>,
    pub chip: Chip,
    /// `contacts.set_routes {clear}` can remove it.
    pub removable: bool,
}

/// The whole panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutesView {
    pub title: String,
    pub routes: Vec<RouteRow>,
    /// "No saved route: …" when there is none.
    pub empty: Option<String>,
    pub hints_title: String,
    pub hints_chip: Chip,
    pub hints: Vec<String>,
    pub mail_title: String,
    pub mail: Vec<String>,
    pub hide_ip_label: String,
    pub hide_ip: bool,
    /// For a friend whose words were not checked yet.
    pub not_verified: Option<String>,
    /// Where Raven reaches the friend on the local network now, and when
    /// (`lan_now`; E1: only the old saved address was shown while every
    /// message went to the new one).
    pub lan_now: Option<String>,
    /// The saved LAN address is another, old one: said plainly.
    pub lan_stale: Option<String>,
    /// The address in use, when it can be saved ("Use the new address"):
    /// the saved one is old, or none is saved.
    pub lan_save: Option<String>,
}

fn name_for(contacts: &[Contact], address: Option<&str>, given: Option<&str>) -> String {
    given
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let a = address?;
            contacts
                .iter()
                .find(|c| c.address == a)
                .map(|c| c.name.clone())
                .or_else(|| Some(short_address(a)))
        })
        .unwrap_or_default()
}

/// How long a message written now could wait while the friend is away, by
/// the rule the terminal app and `raven contact list` use (`carriers.away`;
/// E1: this panel said "up to 7 days" where the terminal app said "until
/// tomorrow"), then how friends can carry it too (only when one can). A
/// friend Raven hasn't connected with yet gets the terminal app's
/// first-message rule; "meet first" is only for a secure connection that
/// ended. `now_ms`: when it is read (a deadline a week away has its date).
pub fn away_lines(c: &ContactCarriers, name: &str, now_ms: u64) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(away) = &c.away {
        if away.first_message == Some(true) {
            lines.push(trf("network.routes.awayFirstMessage", &[("name", name)]));
        } else if away.meet_first == Some(true) {
            lines.push(trf("network.routes.awayMeetFirst", &[("name", name)]));
        } else if let Some(until) = away.until_ms.filter(|u| *u > 0) {
            lines.push(trf(
                "network.routes.away",
                &[("name", name), ("time", &timefmt::deadline(now_ms, until))],
            ));
        }
    }
    if let Some(cov) = c
        .coverage
        .as_ref()
        .filter(|cov| matches!(cov.kind.as_str(), "mutual_friend" | "two_friends"))
    {
        lines.push(trf(
            "network.routes.coverage",
            &[("coverage", &cov.name), ("until", &cov.until)],
        ));
    }
    lines
}

pub fn routes_view(r: &ContactRoutes, name: &str, contacts: &[Contact], now_ms: u64) -> RoutesView {
    let routes: Vec<RouteRow> = r
        .routes
        .iter()
        .map(|route| {
            let mut extra = Vec::new();
            if !route.via.is_empty() {
                extra.push(trf(
                    "network.routes.via",
                    &[("relays", &route.via.join(", "))],
                ));
            }
            if let Some(next) = &route.next {
                extra.push(trf("network.routes.next", &[("id", next)]));
            }
            RouteRow {
                kind: route.kind.clone(),
                label: route.label.clone(),
                value: route.value.clone(),
                extra,
                chip: Chip::of(route.state),
                removable: matches!(route.kind.as_str(), "lan" | "internet" | "p2p"),
            }
        })
        .collect();
    let empty = routes
        .is_empty()
        .then(|| tr("network.routes.none").to_string());

    let mut hints = Vec::new();
    match &r.learned {
        Some(l) => {
            hints.push(trf(
                "network.routes.hints.counts",
                &[
                    ("relays", &l.relays.to_string()),
                    ("direct", &l.direct.to_string()),
                    ("mail", &l.mail_friends.to_string()),
                    ("time", &timefmt::deadline(now_ms, l.expires_at_ms)),
                ],
            ));
            if let Some(id) = &l.peer_id {
                // Only a proven ID gets a word: "not proven yet" was nothing the user could act on (E1).
                let mut line = trf("network.routes.peerId", &[("id", id)]);
                if l.peer_id_proven {
                    line.push_str(" \u{b7} ");
                    line.push_str(tr("network.routes.peerId.proven"));
                }
                hints.push(line);
            }
        }
        None => hints.push(trf("network.routes.hints.none", &[("name", name)])),
    }

    let c = &r.carriers;
    let mut mail = Vec::new();
    // One line per list, the names joined by ", " (as on macOS and Windows).
    if !c.told.is_empty() {
        let friends: Vec<String> = c
            .told
            .iter()
            .map(|f| {
                let who = name_for(contacts, f.address.as_deref(), f.name.as_deref());
                if f.takes_handoffs == Some(true) {
                    trf("network.routes.takesHandoffs", &[("friend", &who)])
                } else {
                    who
                }
            })
            .collect();
        mail.push(trf(
            "network.routes.told",
            &[("name", name), ("friends", &friends.join(", "))],
        ));
    }
    if c.told_unknown > 0 {
        mail.push(trf(
            "network.routes.toldUnknown",
            &[("name", name), ("n", &c.told_unknown.to_string())],
        ));
    }
    if !c.listed.is_empty() {
        let friends: Vec<String> = c
            .listed
            .iter()
            .map(|f| {
                let who = name_for(contacts, f.address.as_deref(), f.name.as_deref());
                if f.usable == Some(false) {
                    trf("network.routes.notUsable", &[("friend", &who)])
                } else {
                    who
                }
            })
            .collect();
        mail.push(trf(
            "network.routes.listed",
            &[("name", name), ("friends", &friends.join(", "))],
        ));
    }
    mail.extend(away_lines(c, name, now_ms));
    if c.receipts_stranded {
        mail.push(trf("network.routes.stranded", &[("name", name)]));
    }

    let saved_lan = r
        .routes
        .iter()
        .find(|route| route.kind == "lan")
        .map(|route| route.value.clone())
        .unwrap_or_default();
    let lan = r.lan_now.as_ref().filter(|l| !l.addr.is_empty());
    let lan_now = lan.map(|l| {
        trf(
            "network.routes.lanNow",
            &[
                ("name", name),
                ("addr", &l.addr),
                ("when", &timefmt::relative_past(now_ms, l.at_ms)),
            ],
        )
    });
    let lan_stale = lan
        .filter(|l| l.saved_stale && !saved_lan.is_empty())
        .map(|_| {
            trf(
                "network.routes.lanStale",
                &[("addr", &saved_lan), ("name", name)],
            )
        });
    let lan_save = lan
        .filter(|_| lan_stale.is_some() || saved_lan.is_empty())
        .map(|l| l.addr.clone());

    RoutesView {
        title: trf("network.routes.title", &[("name", name)]),
        routes,
        empty,
        hints_title: trf("network.routes.hints", &[("name", name)]),
        hints_chip: Chip::of(c.hints),
        hints,
        mail_title: trf("network.routes.carriers", &[("name", name)]),
        mail,
        hide_ip_label: trf("network.routes.hideIp", &[("name", name)]),
        hide_ip: r.hide_ip,
        not_verified: (!r.verified).then(|| trf("network.routes.notVerified", &[("name", name)])),
        lan_now,
        lan_stale,
        lan_save,
    }
}

/// Friends that can carry this friend's mail: verified, not the friend.
pub fn mail_friend_choices(contacts: &[Contact], friend: &str) -> Vec<(String, String)> {
    contacts
        .iter()
        .filter(|c| c.verified && c.address != friend)
        .map(|c| (c.address.clone(), c.name.clone()))
        .collect()
}

/// The mail friends chosen by hand now (addresses).
pub fn listed_now(r: &ContactRoutes) -> Vec<String> {
    r.carriers
        .listed
        .iter()
        .filter_map(|f| f.address.clone())
        .collect()
}

/// What the "Change routes" form holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RoutesForm {
    /// `host:port`; empty: unchanged.
    pub internet: String,
    /// A PeerId or multiaddr; empty: unchanged.
    pub p2p: String,
    /// Relays for the p2p route, one per line (commas and spaces too).
    pub relays: String,
    /// Mail friends by address.
    pub mail_friends: Vec<String>,
}

/// The `contacts.set_routes` patch for the form, `Ok(None)` when nothing
/// changed. The engine checks every route before saving anything; the
/// app only keeps to "at most two mail friends".
pub fn form_patch(
    address: &str,
    form: &RoutesForm,
    current: &ContactRoutes,
) -> Result<Option<RoutesPatch>, String> {
    if form.mail_friends.len() > 2 {
        return Err(tr("network.routes.mailFriends.tooMany").to_string());
    }
    let mut patch = RoutesPatch {
        address: address.to_string(),
        ..RoutesPatch::default()
    };
    let internet = form.internet.trim();
    if !internet.is_empty() {
        patch.internet = Some(internet.to_string());
    }
    let p2p = form.p2p.trim();
    if !p2p.is_empty() {
        patch.p2p = Some(p2p.to_string());
    }
    patch.via = form
        .relays
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let mut chosen = form.mail_friends.clone();
    chosen.sort();
    let mut before = listed_now(current);
    before.sort();
    if chosen != before {
        patch.custodians = Some(form.mail_friends.clone());
    }
    let nothing = patch.internet.is_none()
        && patch.p2p.is_none()
        && patch.via.is_empty()
        && patch.custodians.is_none();
    Ok((!nothing).then_some(patch))
}

/// Remove one saved route.
pub fn remove_patch(address: &str, kind: &str) -> RoutesPatch {
    RoutesPatch {
        address: address.to_string(),
        clear: vec![kind.to_string()],
        ..RoutesPatch::default()
    }
}

/// Hide (or show) this computer's IP address to one friend.
pub fn hide_ip_patch(address: &str, on: bool) -> RoutesPatch {
    RoutesPatch {
        address: address.to_string(),
        hide_ip: Some(on),
        ..RoutesPatch::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::decode;
    use serde_json::json;

    fn friend(name: &str, verified: bool) -> Contact {
        Contact {
            address: format!("rvn1q{name}"),
            name: name.into(),
            short_address: String::new(),
            verified,
            presence: Default::default(),
            last_contact_ms: None,
            waiting: None,
            avatar_index: 0,
            lan: None,
            unread: 0,
        }
    }

    fn routes() -> ContactRoutes {
        decode(json!({
            "address": "rvn1qbob", "verified": true, "hide_ip": false,
            "routes": [
                {"kind": "lan", "label": "Same network", "value": "192.168.1.31:7420", "state": "on"},
                {"kind": "p2p", "label": "Peer-to-peer", "value": "12D3KooWBob",
                 "via": ["/ip4/203.0.113.7/tcp/4001"], "next": "12D3KooWBobNext", "state": "lab"},
                {"kind": "nearby", "label": "Bluetooth nearby", "value": "", "state": "preview"}
            ],
            "learned": {"relays": 1, "direct": 2, "mail_friends": 1, "peer_id": "12D3KooWBob",
                        "peer_id_proven": false, "expires_at_ms": 1_792_591_500_000u64},
            "carriers": {"hints": "lab",
                "told": [{"address": "rvn1qmax", "name": null, "takes_handoffs": true}],
                "told_unknown": 2,
                "listed": [{"address": "rvn1qlily", "name": "lily", "usable": false}],
                "coverage": {"kind": "mutual_friend", "name": "max", "until": "for up to 24 hours"},
                "away": {"until_ms": 1_792_591_500_000u64},
                "receipts_stranded": true}
        }))
        .unwrap()
    }

    #[test]
    fn the_panel_says_routes_hints_and_mail_friends() {
        crate::timefmt::set_local_offset_secs(0);
        let contacts = vec![
            friend("max", true),
            friend("lily", true),
            friend("alice", false),
        ];
        let v = routes_view(&routes(), "bob", &contacts, 1_792_505_100_000);
        assert_eq!(v.title, "How Raven reaches bob");
        assert_eq!(v.routes.len(), 3);
        assert_eq!(v.routes[0].chip, Chip::On);
        assert_eq!(v.routes[1].chip, Chip::Lab);
        assert_eq!(
            v.routes[1].extra,
            vec![
                "through /ip4/203.0.113.7/tcp/4001",
                "Next peer-to-peer ID: 12D3KooWBobNext"
            ]
        );
        assert!(v.routes[1].removable && !v.routes[2].removable);
        assert_eq!(v.empty, None);
        assert_eq!(
            v.hints[0],
            "Relays: 1 · direct addresses: 2 · mail friends: 1 · kept until Wed 14:05 (UTC)"
        );
        assert_eq!(v.hints[1], "Peer-to-peer ID: 12D3KooWBob");
        assert_eq!(v.hints_chip, Chip::Lab);
        assert_eq!(
            v.mail,
            vec![
                "Mail friends bob told you about: max (takes handoffs)",
                "Mail friends bob told you about that aren't your friends: 2",
                "Mail friends you chose for bob: lily (can't carry mail now)",
                "When bob is away: Raven keeps trying until Wed 14:05 (UTC).",
                "Friends can carry it too: max, for up to 24 hours",
                "Some delivery receipts from bob are still waiting with a friend."
            ]
        );
        assert_eq!(v.not_verified, None);
        assert_eq!(v.hide_ip_label, "Hide this computer's IP address from bob");
    }

    #[test]
    fn an_unverified_friend_without_routes() {
        let r: ContactRoutes = decode(json!({"address": "rvn1qalice", "verified": false,
            "routes": [], "carriers": {"hints": "preview"}}))
        .unwrap();
        let v = routes_view(&r, "alice", &[], 0);
        assert_eq!(v.empty.as_deref(), Some("No saved routes."));
        assert_eq!(v.hints, vec!["alice hasn't told this computer any routes."]);
        assert!(v.mail.is_empty());
        assert!(v.not_verified.unwrap().contains("alice"));
    }

    /// E1: the panel names the address in use, says plainly the saved one
    /// is old, and offers to save it; with nothing saved it offers it too.
    #[test]
    fn the_panel_says_where_the_friend_is_reached_now() {
        let now = 1_792_505_100_000u64;
        let r: ContactRoutes = decode(json!({"address": "rvn1qwinc", "verified": true,
            "routes": [{"kind": "lan", "label": "Same network", "value": "192.168.1.146:17420", "state": "on"}],
            "lan_now": {"addr": "192.168.0.24:17420", "at_ms": now - 300_000, "saved_stale": true},
            "carriers": {"hints": "lab", "away": {"meet_first": true}}}))
        .unwrap();
        let v = routes_view(&r, "win-c", &[], now);
        assert_eq!(
            v.lan_now.as_deref(),
            Some("Raven last reached win-c at 192.168.0.24:17420 (5 min ago).")
        );
        assert_eq!(
            v.lan_stale.as_deref(),
            Some("The saved address 192.168.1.146:17420 is old: Raven reaches win-c at the new one now.")
        );
        assert_eq!(v.lan_save.as_deref(), Some("192.168.0.24:17420"));
        assert_eq!(
            v.mail.last().map(String::as_str),
            Some("To write to win-c, meet first: Raven can't hold new messages for them now.")
        );
        let same: ContactRoutes = decode(json!({"address": "rvn1qwinc", "verified": true,
            "routes": [{"kind": "lan", "label": "Same network", "value": "192.168.0.24:17420", "state": "on"}],
            "lan_now": {"addr": "192.168.0.24:17420", "at_ms": now, "saved_stale": false}}))
        .unwrap();
        let v = routes_view(&same, "win-c", &[], now);
        assert!(v.lan_now.is_some() && v.lan_stale.is_none() && v.lan_save.is_none());
        let none: ContactRoutes = decode(json!({"address": "rvn1qwinc", "verified": true,
            "routes": [], "lan_now": {"addr": "192.168.0.24:17420", "at_ms": now, "saved_stale": false}}))
        .unwrap();
        assert_eq!(
            routes_view(&none, "win-c", &[], now).lan_save.as_deref(),
            Some("192.168.0.24:17420")
        );
    }

    /// Three-machine test: Routes said "meet first" for a friend Raven had
    /// never connected with, where the terminal app says a first message
    /// needs you both online. That friend gets the terminal app's sentence
    /// (`first_message`, next to `meet_first`); "meet first" is only for a
    /// secure connection that ended.
    #[test]
    fn a_first_message_is_not_meet_first() {
        let first: ContactCarriers =
            decode(json!({"away": {"meet_first": true, "first_message": true}})).unwrap();
        assert_eq!(
            away_lines(&first, "win-c", 0),
            vec!["A first message needs you and win-c both online (Raven hasn't connected with win-c yet)."]
        );
        let ended: ContactCarriers = decode(json!({"away": {"meet_first": true}})).unwrap();
        assert_eq!(
            away_lines(&ended, "win-c", 0),
            vec!["To write to win-c, meet first: Raven can't hold new messages for them now."]
        );
    }

    #[test]
    fn the_form_builds_one_patch() {
        let contacts = vec![
            friend("bob", true),
            friend("max", true),
            friend("lily", true),
            friend("alice", false),
        ];
        let choices = mail_friend_choices(&contacts, "rvn1qbob");
        let names: Vec<&str> = choices.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["max", "lily"],
            "verified friends, not bob himself"
        );
        let current = routes();
        let unchanged = RoutesForm {
            mail_friends: vec!["rvn1qlily".into()],
            ..RoutesForm::default()
        };
        assert_eq!(form_patch("rvn1qbob", &unchanged, &current), Ok(None));
        let form = RoutesForm {
            internet: " 203.0.113.5:7422 ".into(),
            p2p: String::new(),
            relays: "/ip4/a/tcp/1\n/ip4/b/tcp/2, ".into(),
            mail_friends: vec!["rvn1qmax".into(), "rvn1qlily".into()],
        };
        let p = form_patch("rvn1qbob", &form, &current).unwrap().unwrap();
        assert_eq!(p.internet.as_deref(), Some("203.0.113.5:7422"));
        assert_eq!(p.via, vec!["/ip4/a/tcp/1", "/ip4/b/tcp/2"]);
        assert_eq!(
            p.custodians,
            Some(vec!["rvn1qmax".to_string(), "rvn1qlily".to_string()])
        );
        let too_many = RoutesForm {
            mail_friends: vec!["a".into(), "b".into(), "c".into()],
            ..RoutesForm::default()
        };
        assert_eq!(
            form_patch("rvn1qbob", &too_many, &current),
            Err("Pick at most two mail friends.".to_string())
        );
        let cleared = RoutesForm::default();
        let p = form_patch("rvn1qbob", &cleared, &current).unwrap().unwrap();
        assert_eq!(p.custodians, Some(Vec::new()), "unticking clears them");
        assert_eq!(remove_patch("rvn1qbob", "p2p").clear, vec!["p2p"]);
        assert_eq!(hide_ip_patch("rvn1qbob", true).hide_ip, Some(true));
    }
}

#[cfg(all(test, debug_assertions))]
mod with_mock {
    use super::*;
    use crate::api::RavenEngine;
    use crate::mock::{Clock, MockRaven};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    #[test]
    fn routes_through_the_mock() {
        let t = Arc::new(AtomicU64::new(1_792_505_100_000));
        let m = MockRaven::with_clock(Clock::Manual(t), true).with_lab_network(true);
        let contacts = m.contacts_list().unwrap();
        let ali = contacts.iter().find(|c| c.name == "Ali").unwrap();
        let r = m.contacts_routes(&ali.address).unwrap();
        let v = routes_view(&r, &ali.name, &contacts, 1_792_505_100_000);
        assert_eq!(v.routes[0].kind, "lan");
        assert!(v
            .mail
            .iter()
            .any(|l| l == "Mail friends Ali told you about: Mina (takes handoffs)"));
        assert!(v.hints[0].starts_with("Relays: 1"));
        // Save a route and a mail friend, then remove the route again.
        let mina = contacts.iter().find(|c| c.name == "Mina").unwrap();
        let form = RoutesForm {
            internet: "203.0.113.5:7422".into(),
            mail_friends: vec![mina.address.clone()],
            ..RoutesForm::default()
        };
        let patch = form_patch(&ali.address, &form, &r).unwrap().unwrap();
        let saved = m.contacts_set_routes(&patch).unwrap();
        let v = routes_view(&saved.routes, &ali.name, &contacts, 1_792_505_100_000);
        assert!(v
            .routes
            .iter()
            .any(|r| r.kind == "internet" && r.chip == Chip::Lab));
        assert!(v
            .mail
            .iter()
            .any(|l| l == "Mail friends you chose for Ali: Mina"));
        let removed = m
            .contacts_set_routes(&remove_patch(&ali.address, "internet"))
            .unwrap();
        assert!(!removed.routes.routes.iter().any(|r| r.kind == "internet"));
        // A bad route comes back as the engine's error, nothing saved.
        let bad = RoutesPatch {
            address: ali.address.clone(),
            internet: Some("no-port".into()),
            ..RoutesPatch::default()
        };
        assert!(m.contacts_set_routes(&bad).is_err());
    }

    /// The demo's friend who isn't verified was never connected with: the
    /// first-message rule, not meet first.
    #[test]
    fn a_friend_never_connected_with_gets_the_first_message_rule() {
        let now = 1_792_505_100_000;
        let m = MockRaven::with_clock(Clock::Manual(Arc::new(AtomicU64::new(now))), true);
        let contacts = m.contacts_list().unwrap();
        let rena = contacts.iter().find(|c| c.name == "Rena").unwrap();
        let r = m.contacts_routes(&rena.address).unwrap();
        let away = r.carriers.away.clone().unwrap();
        assert_eq!(
            (away.meet_first, away.first_message),
            (Some(true), Some(true))
        );
        let v = routes_view(&r, &rena.name, &contacts, now);
        assert!(v
            .mail
            .iter()
            .any(|l| l.starts_with("A first message needs you and Rena both online")));
        assert!(
            !v.mail.iter().any(|l| l.contains("meet first")),
            "{:?}",
            v.mail
        );
    }
}
