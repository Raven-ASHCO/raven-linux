//! The Security page (API.md): security notices (`security.status`, read
//! only), Lock while running (`lock.state` / `lock.now` / `lock.unlock`,
//! with the locked screen) and Updates (`updates.status`, read only:
//! installing stays in the terminal for now). Pure and GTK-free.

use crate::api::{codes, ApiError, LockState, SecurityStatus, UpdatesStatus};
use crate::strings::{tr, trf};

/// One security notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticeView {
    /// "Alert" or "Warning".
    pub level: &'static str,
    /// `danger` for an alert, `warning` otherwise.
    pub css: &'static str,
    pub headline: String,
    pub when: String,
    pub did: Option<String>,
    pub todo: Option<String>,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityView {
    /// Preview or lab, when that is the state.
    pub state: Option<&'static str>,
    pub kept: Option<&'static str>,
    pub counted: Option<String>,
    pub notices: Vec<NoticeView>,
    /// "No security notices." when there is none.
    pub empty: Option<&'static str>,
    /// The background program's counters.
    pub node: Option<String>,
    /// Resolving stays in the terminal.
    pub terminal: &'static str,
}

/// The background program's count of open alerts, labelled like the line
/// above it: they are the smaller notices, only counted, while no notice
/// needs you, and "alerts" is never said then (E1: "Open alerts: 1" beside
/// "Nothing needs you.").
pub fn open_notices_line(notices_need_you: bool, n: impl ToString) -> String {
    let key = if notices_need_you {
        "security.node.alerts"
    } else {
        "security.counted"
    };
    trf(key, &[("n", &n.to_string())])
}

pub fn security_view(s: &SecurityStatus) -> SecurityView {
    let state = if !s.available {
        Some(tr("security.preview"))
    } else if s.lab {
        Some(tr("network.labOnly"))
    } else {
        None
    };
    let kept = match s.kept.as_str() {
        "off" => Some(tr("security.kept.off")),
        "unsealed" => Some(tr("security.kept.unsealed")),
        "live" => Some(tr("security.kept.live")),
        _ => None,
    };
    let notices: Vec<NoticeView> = s
        .notices
        .iter()
        .map(|n| {
            let alert = n.level == "alert";
            NoticeView {
                level: if alert {
                    tr("security.alert")
                } else {
                    tr("security.warning")
                },
                css: if alert { "danger" } else { "warning" },
                headline: n.headline.clone(),
                when: n.when.clone(),
                did: (!n.what_raven_did.is_empty())
                    .then(|| trf("security.did", &[("text", &n.what_raven_did)])),
                todo: (!n.what_to_do.is_empty())
                    .then(|| trf("security.todo", &[("text", &n.what_to_do)])),
                details: n
                    .details
                    .iter()
                    .map(|d| format!("{}: {}", d.label, d.value))
                    .collect(),
            }
        })
        .collect();
    SecurityView {
        state,
        kept,
        counted: (s.counted > 0).then(|| trf("security.counted", &[("n", &s.counted.to_string())])),
        empty: (s.available && notices.is_empty()).then(|| tr("security.none")),
        notices,
        node: s.node.map(|c| {
            let counts = trf(
                "security.node",
                &[
                    ("strangers", &c.strangers_refused_1h.to_string()),
                    ("handshakes", &c.failed_handshakes_1h.to_string()),
                    ("bans", &c.bans_active.to_string()),
                    ("tags", &c.link_tag_failures_24h.to_string()),
                ],
            );
            format!(
                "{counts} {}",
                open_notices_line(!s.notices.is_empty(), c.alerts_open)
            )
        }),
        terminal: tr("security.resolve"),
    }
}

/// The Lock section and the locked screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockView {
    /// What the state is, in one line.
    pub status: &'static str,
    /// What Lock now does (when it can be used).
    pub body: Option<&'static str>,
    /// On for testing only.
    pub lab: Option<&'static str>,
    /// "Lock now" works.
    pub can_lock: bool,
    /// The background program runs without its keys: show the locked screen.
    pub locked: bool,
    /// Ask for the Raven passphrase on the locked screen (unless the
    /// profile has none).
    pub passphrase_field: bool,
    /// The profile has no passphrase: Unlock opens it as it is.
    pub no_passphrase: Option<&'static str>,
}

pub fn lock_view(l: &LockState) -> LockView {
    let status = if !l.available {
        tr("lock.notAvailable")
    } else if !l.supported {
        tr("lock.notSupported")
    } else if l.locked {
        tr("lock.isLocked")
    } else {
        tr("lock.isOpen")
    };
    let usable = l.available && l.supported;
    LockView {
        status,
        body: usable.then(|| tr("lock.v2.body")),
        lab: (l.available && l.lab).then(|| tr("network.labOnly")),
        can_lock: usable && !l.locked,
        locked: usable && l.locked,
        passphrase_field: l.has_passphrase != Some(false),
        no_passphrase: (l.has_passphrase == Some(false)).then(|| tr("lock.noPassphrase")),
    }
}

/// What the locked screen says after a failed unlock: the engine's plain
/// words, else the shared texts for the unlock codes.
pub fn unlock_error(e: &ApiError) -> String {
    if !e.message.is_empty() {
        return e.message.clone();
    }
    match e.code.as_str() {
        codes::PASSPHRASE_WRONG => tr("error.passphraseWrong"),
        codes::PASSPHRASE_NEEDED => tr("error.passphraseNeeded"),
        codes::UNLOCK_WAIT => tr("error.unlockWait"),
        codes::NOT_AVAILABLE_YET => tr("lock.notAvailable"),
        _ => tr("error.internal"),
    }
    .to_string()
}

/// The passphrase to send: `None` for an empty field.
pub fn passphrase_arg(typed: &str) -> Option<&str> {
    (!typed.is_empty()).then_some(typed)
}

/// The Updates section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdatesView {
    pub state: Option<&'static str>,
    pub summary: Option<String>,
    pub lines: Vec<String>,
    pub offer: Vec<String>,
    /// Installing stays in the terminal for now.
    pub install: &'static str,
    /// The engine's own sentence about installing.
    pub install_text: Option<String>,
}

pub fn updates_view(u: &UpdatesStatus) -> UpdatesView {
    let state = if !u.available {
        Some(tr("updates.notAvailable"))
    } else if u.lab {
        Some(tr("network.labOnly"))
    } else {
        None
    };
    let offer = u.offer.clone().unwrap_or_default();
    let mut lines = u.lines.clone();
    if !offer.is_empty() {
        lines.push(tr("updates.offer").to_string());
    }
    UpdatesView {
        state,
        summary: u.summary.clone(),
        lines,
        offer,
        install: tr("updates.install"),
        install_text: (!u.install_text.is_empty()).then(|| u.install_text.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::decode;
    use serde_json::json;

    #[test]
    fn notices_and_counters() {
        let s: SecurityStatus = decode(json!({
            "available": true, "lab": true, "kept": "live", "counted": 3,
            "notices": [
                {"id": 1, "level": "alert", "name": "max", "headline": "max's key changed",
                 "what_raven_did": "Raven stopped sending to max.",
                 "what_to_do": "Compare security words with max.", "when": "today 09:00",
                 "details": [{"label": "old key", "value": "ab12"}]},
                {"id": 2, "level": "warning", "name": "", "headline": "Strangers knocked",
                 "what_raven_did": "", "what_to_do": "", "when": "now", "details": []}
            ],
            "node": {"strangers_refused_1h": 5, "failed_handshakes_1h": 1, "bans_active": 0,
                     "link_tag_failures_24h": 2, "alerts_open": 1}
        }))
        .unwrap();
        let v = security_view(&s);
        assert_eq!(
            v.state,
            Some("On for testing only: a developer build's lab switch turned it on.")
        );
        assert_eq!(
            v.kept,
            Some("Raven keeps security notices on this computer.")
        );
        assert_eq!(
            v.counted.as_deref(),
            Some("Smaller notices, only counted: 3")
        );
        assert_eq!(v.notices[0].level, "Alert");
        assert_eq!(v.notices[0].css, "danger");
        assert_eq!(
            v.notices[0].did.as_deref(),
            Some("What Raven did: Raven stopped sending to max.")
        );
        assert_eq!(v.notices[0].details, vec!["old key: ab12"]);
        assert_eq!(v.notices[1].level, "Warning");
        assert_eq!(v.notices[1].did, None);
        assert_eq!(v.empty, None);
        let node = v.node.unwrap();
        assert!(node.starts_with("Last hour: 5 strangers refused"));
        // Notices need you: their count is "open notices"; never "alerts" (E1).
        assert!(node.ends_with(" Open notices: 1"), "{node}");
        assert!(!node.to_lowercase().contains("alert"), "{node}");
        assert_eq!(
            open_notices_line(false, 1),
            "Smaller notices, only counted: 1"
        );
        assert!(v.terminal.contains("raven security resolve"));
        let off: SecurityStatus =
            decode(json!({"available": false, "kept": "off", "notices": []})).unwrap();
        let v = security_view(&off);
        assert_eq!(
            v.state,
            Some("Security notices aren't in this version of Raven yet.")
        );
        assert_eq!(
            v.empty, None,
            "no 'no notices' line while it isn't there at all"
        );
    }

    #[test]
    fn lock_states() {
        let l = |v: serde_json::Value| lock_view(&decode::<LockState>(v).unwrap());
        let held = l(json!({"available": false, "supported": false, "locked": false}));
        assert!(!held.can_lock && !held.locked);
        assert!(held.status.contains("raven lock"));
        let no_node = l(json!({"available": true, "supported": false, "locked": false}));
        assert!(!no_node.can_lock);
        assert!(no_node
            .status
            .starts_with("Raven's background program can't lock now"));
        let open = l(
            json!({"available": true, "lab": true, "supported": true, "locked": false,
                            "has_passphrase": true}),
        );
        assert!(open.can_lock && !open.locked && open.lab.is_some() && open.body.is_some());
        assert_eq!(open.status, "Raven is open.");
        let locked = l(json!({"available": true, "supported": true, "locked": true,
                              "has_passphrase": false}));
        assert!(locked.locked && !locked.can_lock);
        assert!(!locked.passphrase_field);
        assert!(locked.no_passphrase.is_some());
        let unknown = l(json!({"available": true, "supported": true, "locked": true}));
        assert!(unknown.passphrase_field, "ask when Raven can't tell");
        assert_eq!(passphrase_arg(""), None);
        assert_eq!(passphrase_arg(" x"), Some(" x"), "sent exactly as typed");
    }

    #[test]
    fn unlock_errors_use_plain_words() {
        for (code, text) in [
            (
                codes::PASSPHRASE_WRONG,
                "That isn't your Raven passphrase. Try again.",
            ),
            (
                codes::PASSPHRASE_NEEDED,
                "Type your Raven passphrase to unlock.",
            ),
            (
                codes::UNLOCK_WAIT,
                "Too many wrong passphrases. Wait a little, then try again.",
            ),
        ] {
            assert_eq!(unlock_error(&ApiError::new(code, "", "")), text);
        }
        assert_eq!(
            unlock_error(&ApiError::new(
                codes::PASSPHRASE_WRONG,
                "Wrong; 2 tries left.",
                ""
            )),
            "Wrong; 2 tries left.",
            "the engine's own words first"
        );
    }

    #[test]
    fn updates_are_read_only() {
        let u: UpdatesStatus = decode(json!({
            "available": true, "lab": false, "lines": ["A signed update is ready."],
            "offer": ["raven 2.1.0"], "summary": "updates: 1 ready",
            "install": "cli_only", "install_text": "Install updates with raven update install."
        }))
        .unwrap();
        let v = updates_view(&u);
        assert_eq!(v.state, None);
        assert_eq!(
            v.lines,
            vec!["A signed update is ready.", "A friend offered a release"]
        );
        assert_eq!(v.offer, vec!["raven 2.1.0"]);
        // Plain words; the command stays out of the page (E1).
        assert_eq!(v.install, "Installing updates isn't in this app yet.");
        let held: UpdatesStatus =
            decode(json!({"available": false, "install": "cli_only"})).unwrap();
        assert_eq!(
            updates_view(&held).state,
            Some("Updates passed on by friends aren't in this version of Raven yet.")
        );
    }
}

#[cfg(all(test, debug_assertions))]
mod with_mock {
    use super::*;
    use crate::api::RavenEngine;
    use crate::mock::{Clock, MockRaven, MOCK_PASSPHRASE};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn mock(lab: bool) -> MockRaven {
        let t = Arc::new(AtomicU64::new(1_792_505_100_000));
        let m = MockRaven::with_clock(Clock::Manual(t), true);
        if lab {
            m.with_lab_network(true)
        } else {
            m
        }
    }

    #[test]
    fn a_release_build_has_none_of_it_yet() {
        let m = mock(false);
        let s = security_view(&m.security_status().unwrap());
        assert!(s.state.is_some() && s.notices.is_empty());
        let l = lock_view(&m.lock_state().unwrap());
        assert!(!l.can_lock && !l.locked);
        let e = m.lock_now().unwrap_err();
        assert!(e.is(codes::NOT_AVAILABLE_YET));
        assert_eq!(
            unlock_error(&e),
            "This isn't in this release of Raven yet. Nothing was changed."
        );
        let u = updates_view(&m.updates_status().unwrap());
        assert!(u.state.is_some());
        assert_eq!(u.install, "Installing updates isn't in this app yet.");
    }

    #[test]
    fn lock_now_then_the_locked_screen_then_unlock() {
        let m = mock(true);
        let v = lock_view(&m.lock_state().unwrap());
        assert!(v.can_lock);
        let v = lock_view(&m.lock_now().unwrap());
        assert!(v.locked && v.passphrase_field);
        let e = m.lock_unlock(passphrase_arg("")).unwrap_err();
        assert_eq!(unlock_error(&e), "Type your Raven passphrase to unlock.");
        let e = m.lock_unlock(passphrase_arg("not it")).unwrap_err();
        assert_eq!(
            unlock_error(&e),
            "That isn't your Raven passphrase. Try again."
        );
        let open = m.lock_unlock(passphrase_arg(MOCK_PASSPHRASE)).unwrap();
        assert!(!lock_view(&open).locked);
        assert!(open.note.is_some());
        let s = security_view(&m.security_status().unwrap());
        assert_eq!(s.notices.len(), 1);
        assert!(s.node.is_some());
    }
}
