//! Changing a network setting (API.md, `network.set`): the consent sheet
//! for the three settings that need the person's yes, the refusal of a
//! held way, and the "Restart the background program" prompt with the
//! restart itself (`node.stop` then `node.start`, honest about a login
//! service the app did not start).
//!
//! Every step is a function over `&dyn RavenEngine`: the GTK app runs it
//! on the engine thread and shows the [`SetOutcome`]; the tests run it
//! against the mock and canned engine answers.

use crate::api::{
    codes, ApiError, NetworkChange, NetworkPatch, NetworkStatus, NodeStatus, RavenEngine,
};
use crate::strings::tr;

/// The settings that need the person's yes before `agreed: true` is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// `carry_for_friends`: this computer holds mail for friends.
    CarryForFriends,
    /// `friends_carry_mine`: friends hold your mail.
    FriendsCarryMine,
    /// `handoff`: this computer passes mail on between friends.
    Handoff,
}

impl Consent {
    pub fn of(setting: &str) -> Option<Self> {
        match setting {
            "carry_for_friends" => Some(Consent::CarryForFriends),
            "friends_carry_mine" => Some(Consent::FriendsCarryMine),
            "handoff" => Some(Consent::Handoff),
            _ => None,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            Consent::CarryForFriends => "carry_for_friends",
            Consent::FriendsCarryMine => "friends_carry_mine",
            Consent::Handoff => "handoff",
        }
    }

    /// Title, what it does, what it costs, what others can see, what they can't.
    fn texts(self) -> [&'static str; 5] {
        match self {
            Consent::CarryForFriends => [
                tr("network.consent.carryForFriends.title"),
                tr("network.consent.carryForFriends.does"),
                tr("network.consent.carryForFriends.costs"),
                tr("network.consent.carryForFriends.sees"),
                tr("network.consent.carryForFriends.hidden"),
            ],
            Consent::FriendsCarryMine => [
                tr("network.consent.friendsCarryMine.title"),
                tr("network.consent.friendsCarryMine.does"),
                tr("network.consent.friendsCarryMine.costs"),
                tr("network.consent.friendsCarryMine.sees"),
                tr("network.consent.friendsCarryMine.hidden"),
            ],
            Consent::Handoff => [
                tr("network.consent.handoff.title"),
                tr("network.consent.handoff.does"),
                tr("network.consent.handoff.costs"),
                tr("network.consent.handoff.sees"),
                tr("network.consent.handoff.hidden"),
            ],
        }
    }

    /// The engine's own words for it (`NetworkStatus.texts`).
    fn engine_text(self, status: &NetworkStatus) -> &str {
        match self {
            Consent::CarryForFriends => &status.texts.carry_for_friends,
            Consent::FriendsCarryMine => &status.texts.friends_carry_mine,
            Consent::Handoff => &status.texts.handoff,
        }
    }
}

/// What the consent sheet shows before the person agrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentSheet {
    pub consent: Consent,
    pub title: String,
    pub does: String,
    pub costs: String,
    pub sees: String,
    pub hidden: String,
    /// `NetworkStatus.texts.<setting>`, as plain paragraphs (the terminal's
    /// line breaks joined, its `[y/N]` prompt left out). Shown in full.
    pub engine_text: String,
}

pub fn consent_sheet(consent: Consent, status: &NetworkStatus) -> ConsentSheet {
    let [title, does, costs, sees, hidden] = consent.texts();
    ConsentSheet {
        consent,
        title: title.to_string(),
        does: does.to_string(),
        costs: costs.to_string(),
        sees: sees.to_string(),
        hidden: hidden.to_string(),
        engine_text: plain_paragraphs(consent.engine_text(status)),
    }
}

/// The CLI's consent text for a wrapping label: lines wrapped for a
/// terminal are joined, a `- ` item or a line after a question or a colon
/// starts a new paragraph (items as `• `), and the terminal's own prompt
/// (`Turn on? [y/N]`) is dropped: the sheet has its own buttons.
pub fn plain_paragraphs(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut new_paragraph = true;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            new_paragraph = true;
            continue;
        }
        if line.contains("[y/N]") || line.contains("[Y/n]") {
            continue;
        }
        let item = line.strip_prefix("- ");
        match (item, paragraphs.last_mut()) {
            (Some(rest), _) => paragraphs.push(format!("\u{2022} {rest}")),
            (None, Some(last)) if !new_paragraph => {
                last.push(' ');
                last.push_str(line);
            }
            (None, _) => paragraphs.push(line.to_string()),
        }
        new_paragraph = line.ends_with('?') || line.ends_with(':');
    }
    paragraphs.join("\n")
}

/// A `network.set` patch that switches one setting.
pub fn patch_for(setting: &str, on: bool) -> NetworkPatch {
    let mut p = NetworkPatch::default();
    let v = Some(on);
    match setting {
        "lan" => p.lan = v,
        "internet" => p.internet = v,
        "p2p" => p.p2p = v,
        "port_mapping" => p.port_mapping = v,
        "nearby" => p.nearby = v,
        "carry_for_friends" => p.carry_for_friends = v,
        "friends_carry_mine" => p.friends_carry_mine = v,
        "handoff" => p.handoff = v,
        "relay_for_friends" => p.relay_for_friends = v,
        "bridge" => p.bridge = v,
        "store" => p.store = v,
        "relay" => p.relay = v,
        "hide_ip" => p.hide_ip = v,
        _ => {}
    }
    p
}

/// How long "Open on this network for a while" opens LAN direct
/// (`network.set {lan: true, lan_for}`), in the order of
/// `network.lanFor.options`.
pub const LAN_FOR: [&str; 5] = ["30m", "1h", "2h", "8h", "24h"];

/// `network.set {lan: true, lan_for}` for the dropdown's choice.
pub fn lan_for_patch(index: u32) -> Option<NetworkPatch> {
    let time = LAN_FOR.get(index as usize)?;
    Some(NetworkPatch {
        lan: Some(true),
        lan_for: Some(time.to_string()),
        ..NetworkPatch::default()
    })
}

/// `network.set {proxy}`.
pub fn proxy_patch(value: &str) -> NetworkPatch {
    NetworkPatch {
        proxy: Some(value.to_string()),
        ..NetworkPatch::default()
    }
}

/// The first setting the patch turns on that needs a yes.
pub fn consent_in(patch: &NetworkPatch) -> Option<Consent> {
    [
        (patch.carry_for_friends, Consent::CarryForFriends),
        (patch.friends_carry_mine, Consent::FriendsCarryMine),
        (patch.handoff, Consent::Handoff),
    ]
    .into_iter()
    .find(|(on, _)| *on == Some(true))
    .map(|(_, c)| c)
}

/// The "Restart the background program?" prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartPrompt {
    pub title: String,
    pub body: String,
    /// What waits for the restart (each change's `effect`).
    pub changes: Vec<String>,
    /// The running background program wasn't started by this app (a
    /// login service or the terminal may run it).
    pub not_ours: Option<String>,
}

pub fn restart_prompt(changes: &[NetworkChange], node: Option<&NodeStatus>) -> RestartPrompt {
    RestartPrompt {
        title: tr("network.restart.title").to_string(),
        body: tr("network.restart.body").to_string(),
        changes: changes
            .iter()
            .filter(|c| c.applies == "restart" && !c.effect.is_empty())
            .map(|c| c.effect.clone())
            .collect(),
        not_ours: node
            .filter(|n| n.running && !n.started_by_app)
            .map(|_| tr("network.restart.notOurs").to_string()),
    }
}

/// What a `network.set` came to.
#[derive(Debug, Clone, PartialEq)]
pub enum SetOutcome {
    /// Changed: the new status, what each change does, and the restart
    /// prompt when a change waits for the background program to restart.
    Applied {
        changes: Vec<NetworkChange>,
        status: Box<NetworkStatus>,
        restart: Option<RestartPrompt>,
    },
    /// The person's yes is needed first: show the sheet, then [`agree`].
    NeedsConsent(Box<ConsentSheet>),
    /// A held way (NOT_AVAILABLE_YET): nothing was changed.
    Refused(ApiError),
    /// Any other error: nothing or only part was changed (`detail` says).
    Failed(ApiError),
}

fn needs_consent(e: &dyn RavenEngine, consent: Consent) -> SetOutcome {
    match e.network_status() {
        Ok(status) => SetOutcome::NeedsConsent(Box::new(consent_sheet(consent, &status))),
        Err(err) => SetOutcome::Failed(err),
    }
}

/// Change network settings. A setting that needs a yes is not sent
/// without one: the sheet comes first (and a CONSENT_NEEDED from the
/// engine leads to the same sheet).
pub fn set(e: &dyn RavenEngine, patch: NetworkPatch) -> SetOutcome {
    let consent = consent_in(&patch);
    if let (Some(c), false) = (consent, patch.agreed) {
        return needs_consent(e, c);
    }
    match e.network_set(&patch) {
        Ok(r) => {
            let restart = r.restart_needed.then(|| {
                let node = e.node_status().ok();
                restart_prompt(&r.changes, node.as_ref())
            });
            SetOutcome::Applied {
                changes: r.changes,
                status: Box::new(r.status),
                restart,
            }
        }
        Err(err) if err.is(codes::CONSENT_NEEDED) => match consent {
            Some(c) => needs_consent(e, c),
            None => SetOutcome::Failed(err),
        },
        Err(err) if err.is(codes::NOT_AVAILABLE_YET) => SetOutcome::Refused(err),
        Err(err) => SetOutcome::Failed(err),
    }
}

/// The person agreed on the sheet: turn it on with `agreed: true`.
pub fn agree(e: &dyn RavenEngine, consent: Consent) -> SetOutcome {
    let mut patch = patch_for(consent.setting(), true);
    patch.agreed = true;
    set(e, patch)
}

/// How a restart went.
#[derive(Debug, Clone, PartialEq)]
pub struct RestartResult {
    /// The background program runs again with the changes.
    pub done: bool,
    pub text: String,
    /// A command to run by hand (a login service on Linux).
    pub hint: Option<&'static str>,
    /// Technical text for Details.
    pub detail: String,
    /// The status afterwards, when known.
    pub node: Option<NodeStatus>,
}

impl RestartResult {
    fn not_done(text: &str, detail: String, node: Option<NodeStatus>) -> Self {
        Self {
            done: false,
            text: text.to_string(),
            hint: None,
            detail,
            node,
        }
    }
}

/// `node.stop` then `node.start`. A login service's background program
/// is never stopped (the engine leaves it running): the person is told to
/// restart that service. Nothing else is restarted behind their back.
pub fn restart(e: &dyn RavenEngine) -> RestartResult {
    let stop = match e.node_stop_report() {
        Ok(s) => s,
        Err(err) => return RestartResult::not_done(&err.message, err.detail, None),
    };
    let note = stop.note.clone().unwrap_or_default();
    let stopped = match stop.stop.as_str() {
        "stopped" | "not_running" => true,
        // An engine that doesn't say: believe the status.
        "" => !stop.status.running,
        _ => false,
    };
    if !stopped {
        let mut r = match stop.stop.as_str() {
            "managed" => RestartResult::not_done(tr("network.restart.managed"), note, None),
            "still_running" => {
                RestartResult::not_done(tr("network.restart.stillRunning"), note, None)
            }
            "failed" => RestartResult::not_done(tr("network.restart.failed"), note, None),
            _ => RestartResult::not_done(tr("network.restart.advice"), note, None),
        };
        if stop.stop == "managed" {
            r.hint = Some(tr("network.restart.managed.linux"));
        }
        r.node = Some(stop.status);
        return r;
    }
    match e.node_start() {
        Ok(n) => RestartResult {
            done: true,
            text: tr("network.restart.done").to_string(),
            hint: None,
            detail: String::new(),
            node: Some(n),
        },
        Err(err) => RestartResult::not_done(&err.message, err.detail, Some(stop.status)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::Mutex;

    /// An engine that answers each op from a list (the last answer repeats).
    struct Canned(Mutex<Vec<(&'static str, Result<Value, ApiError>)>>);

    impl RavenEngine for Canned {
        fn call(&self, op: &str, _args: Value) -> Result<Value, ApiError> {
            let mut list = self.0.lock().unwrap();
            let i = list
                .iter()
                .position(|(o, _)| *o == op)
                .unwrap_or_else(|| panic!("unexpected {op}"));
            if list.iter().filter(|(o, _)| *o == op).count() > 1 {
                list.remove(i).1
            } else {
                list[i].1.clone()
            }
        }
    }

    #[test]
    fn the_terminal_text_becomes_paragraphs() {
        let t = "Use friends as mailboxes?\nWhen a contact cannot be reached, raven-node can\n\
                 leave a sealed copy.\nTurn on? [y/N] ";
        assert_eq!(
            plain_paragraphs(t),
            "Use friends as mailboxes?\nWhen a contact cannot be reached, raven-node can leave \
             a sealed copy."
        );
        let t = "Hold mail?\n- Only between friends.\n- It learns who wrote to whom\n  \
                 and when.";
        assert_eq!(
            plain_paragraphs(t),
            "Hold mail?\n\u{2022} Only between friends.\n\u{2022} It learns who wrote to whom \
             and when."
        );
    }

    #[test]
    fn patches_and_consent() {
        let p = patch_for("relay_for_friends", true);
        assert_eq!(p.relay_for_friends, Some(true));
        assert_eq!(consent_in(&p), None);
        assert_eq!(
            consent_in(&patch_for("handoff", true)),
            Some(Consent::Handoff)
        );
        assert_eq!(
            consent_in(&patch_for("handoff", false)),
            None,
            "off needs no yes"
        );
        assert_eq!(
            Consent::of("friends_carry_mine").unwrap().setting(),
            "friends_carry_mine"
        );
        assert_eq!(Consent::of("bridge"), None);
        assert_eq!(proxy_patch("off").proxy.as_deref(), Some("off"));
        let p = lan_for_patch(1).unwrap();
        assert_eq!((p.lan, p.lan_for.as_deref()), (Some(true), Some("1h")));
        assert_eq!(lan_for_patch(5), None);
        assert_eq!(
            crate::strings::tr_list("network.lanFor.options").len(),
            LAN_FOR.len()
        );
    }

    #[test]
    fn restart_prompt_is_honest_about_a_program_it_did_not_start() {
        let changes = vec![
            NetworkChange {
                setting: "p2p".into(),
                effect: "p2p effect".into(),
                applies: "restart".into(),
                ..NetworkChange::default()
            },
            NetworkChange {
                setting: "store".into(),
                effect: "store effect".into(),
                applies: "now".into(),
                ..NetworkChange::default()
            },
        ];
        let node = NodeStatus {
            running: true,
            started_by_app: false,
            ..NodeStatus::default()
        };
        let p = restart_prompt(&changes, Some(&node));
        assert_eq!(p.changes, vec!["p2p effect"]);
        assert!(p.not_ours.is_some());
        let ours = NodeStatus {
            started_by_app: true,
            ..node
        };
        assert!(restart_prompt(&changes, Some(&ours)).not_ours.is_none());
    }

    #[test]
    fn restart_outcomes() {
        let node = json!({"running": true, "receive": true});
        let stop = |word: &str, note: Option<&str>| {
            let mut v = json!({"running": true, "receive": true, "stop": word});
            if let Some(n) = note {
                v["note"] = n.into();
            }
            v
        };
        let managed = Canned(Mutex::new(vec![(
            "node.stop",
            Ok(stop(
                "managed",
                Some("Raven's background program runs as a login service"),
            )),
        )]));
        let r = restart(&managed);
        assert!(!r.done);
        assert!(r.text.starts_with("A login service runs"));
        assert_eq!(
            r.hint,
            Some("In a terminal: systemctl --user restart raven-node")
        );
        assert!(r.detail.contains("login service"));
        for (word, starts) in [
            ("advice", "This app didn't start"),
            (
                "still_running",
                "Raven's background program was asked to stop",
            ),
            ("failed", "Raven's background program didn't restart"),
        ] {
            let e = Canned(Mutex::new(vec![(
                "node.stop",
                Ok(stop(word, Some("kill 42"))),
            )]));
            let r = restart(&e);
            assert!(!r.done && r.text.starts_with(starts), "{word}: {}", r.text);
            assert_eq!(r.hint, None);
        }
        let ok = Canned(Mutex::new(vec![
            ("node.stop", Ok(stop("stopped", None))),
            ("node.start", Ok(node.clone())),
        ]));
        let r = restart(&ok);
        assert!(r.done);
        assert_eq!(r.text, "Restarted. The changes are in use now.");
        let start_fails = Canned(Mutex::new(vec![
            ("node.stop", Ok(stop("not_running", None))),
            (
                "node.start",
                Err(ApiError::new(
                    codes::NODE_NOT_RUNNING,
                    "It didn't start.",
                    "port in use",
                )),
            ),
        ]));
        let r = restart(&start_fails);
        assert!(!r.done);
        assert_eq!(
            (r.text.as_str(), r.detail.as_str()),
            ("It didn't start.", "port in use")
        );
    }

    #[test]
    fn a_consent_needed_from_the_engine_leads_to_the_sheet() {
        let e = Canned(Mutex::new(vec![
            (
                "network.set",
                Err(ApiError::new(
                    codes::CONSENT_NEEDED,
                    "Raven needs your yes first.",
                    "",
                )),
            ),
            (
                "network.status",
                Ok(json!({"texts": {"handoff": "This node may pass sealed messages on."}})),
            ),
        ]));
        // Sent with `agreed` (an older sheet), the engine still asks: show it again.
        let mut p = patch_for("handoff", true);
        p.agreed = true;
        match set(&e, p) {
            SetOutcome::NeedsConsent(sheet) => {
                assert_eq!(sheet.consent, Consent::Handoff);
                assert_eq!(sheet.engine_text, "This node may pass sealed messages on.");
                assert_eq!(sheet.title, "Pass mail on between friends?");
            }
            other => panic!("{other:?}"),
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod with_mock {
    use super::*;
    use crate::api::CarrierState;
    use crate::mock::{Clock, MockRaven};
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
    fn a_held_way_is_refused_and_nothing_changes() {
        let m = mock(false);
        match set(&m, patch_for("p2p", true)) {
            SetOutcome::Refused(e) => {
                assert!(e.is(codes::NOT_AVAILABLE_YET));
                assert_eq!(
                    e.message,
                    "This isn't in this release of Raven yet. Nothing was changed."
                );
            }
            other => panic!("{other:?}"),
        }
        // A setting that needs a yes on a release build: the sheet first
        // (nothing is sent without a yes), then the engine refuses it.
        assert!(matches!(
            set(&m, patch_for("carry_for_friends", true)),
            SetOutcome::NeedsConsent(_)
        ));
        assert!(matches!(
            agree(&m, Consent::CarryForFriends),
            SetOutcome::Refused(_)
        ));
        // Turning off is always allowed.
        match set(&m, patch_for("store", false)) {
            SetOutcome::Applied {
                changes, restart, ..
            } => {
                assert_eq!(changes.len(), 1);
                assert!(restart.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lan_opens_for_a_meeting() {
        crate::timefmt::set_local_offset_secs(0);
        let m = mock(false);
        assert!(matches!(
            set(&m, patch_for("lan", false)),
            SetOutcome::Applied { .. }
        ));
        match set(&m, lan_for_patch(0).unwrap()) {
            SetOutcome::Applied {
                status, changes, ..
            } => {
                assert_eq!(status.way("lan").unwrap().state, CarrierState::On);
                assert!(status.lan_until_ms.is_some());
                assert!(changes[0].effect.contains("30m"), "{changes:?}");
                assert!(
                    crate::netview::this_computer(&status, crate::timefmt::now_ms())
                        .iter()
                        .any(|l| l.starts_with("Open on this network until"))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn consent_flow_on_a_lab_build() {
        let m = mock(true);
        // The engine refuses without the yes.
        let direct = m.network_set(&patch_for("friends_carry_mine", true));
        assert!(direct.unwrap_err().is(codes::CONSENT_NEEDED));
        let sheet = match set(&m, patch_for("friends_carry_mine", true)) {
            SetOutcome::NeedsConsent(s) => s,
            other => panic!("{other:?}"),
        };
        assert_eq!(sheet.title, "Let friends carry your mail?");
        assert!(sheet.engine_text.starts_with("Use friends as mailboxes?"));
        assert!(!sheet.engine_text.contains("[y/N]"));
        assert!(sheet.costs.contains("bandwidth"));
        // Still off until the person agrees.
        let custody = m.network_status().unwrap();
        assert_eq!(custody.way("custody").unwrap().state, CarrierState::Off);
        match agree(&m, sheet.consent) {
            SetOutcome::Applied {
                status, restart, ..
            } => {
                assert_eq!(status.way("custody").unwrap().state, CarrierState::Lab);
                assert_eq!(status.carry.friends_carry_mine.setting, "on");
                assert!(restart.is_none(), "applies now");
            }
            other => panic!("{other:?}"),
        }
        // Turning it off needs no yes.
        assert!(matches!(
            set(&m, patch_for("friends_carry_mine", false)),
            SetOutcome::Applied { .. }
        ));
    }

    #[test]
    fn restart_prompt_then_restart_on_a_lab_build() {
        let m = mock(true);
        let prompt = match set(&m, patch_for("p2p", true)) {
            SetOutcome::Applied {
                restart: Some(p),
                status,
                ..
            } => {
                assert_eq!(status.way("p2p").unwrap().running, Some(false));
                p
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(prompt.title, "Restart the background program?");
        assert_eq!(prompt.changes.len(), 1);
        assert!(
            prompt.not_ours.is_some(),
            "the demo profile's program was started earlier, not by this app"
        );
        let r = restart(&m);
        assert!(r.done, "{r:?}");
        assert!(r.node.unwrap().started_by_app);
        let after = m.network_status().unwrap();
        assert_eq!(after.way("p2p").unwrap().running, Some(true));
        // Now this app started it: the prompt says nothing about others.
        match set(&m, patch_for("internet", true)) {
            SetOutcome::Applied {
                restart: Some(p), ..
            } => assert!(p.not_ours.is_none()),
            other => panic!("{other:?}"),
        }
    }
}
