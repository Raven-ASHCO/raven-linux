//! Raven's real engine (raven-client) through the app's own code: the typed
//! wrappers of `RavenEngine`, `ClientEngine` and the engine threads
//! (`EngineWorker`), as the GTK front end uses them.
//!
//! Every test runs on one throwaway profile with the file-backed secret
//! stores (`RAVEN_*_BACKEND=locked-file`), set before the engine's first call:
//! never `~/.raven`, never the OS keychain. `receive` stays off and nothing
//! here starts raven-node, so no port is opened.
//!
//! ```sh
//! cargo test --no-default-features --features engine --test engine
//! ```

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use raven_desktop::api::{
    codes, ApiError, Build, CarrierState, NetworkPatch, NewIdentity, ProfileState,
    ProfileStateKind, RavenEngine, SettingsPatch, WhenQuit,
};
use raven_desktop::client::ClientEngine;
use raven_desktop::netflow::{self, Consent, SetOutcome};
use raven_desktop::netview::{self, Chip};
use raven_desktop::secview;
use raven_desktop::worker::EngineWorker;
use serde_json::json;

/// The display name given at setup.
const NAME: &str = "Desk Test";

/// The lab profile, and what the engine said about it before and while the
/// identity was made (each test checks its own part).
struct Lab {
    profile: PathBuf,
    first_state: Result<ProfileState, ApiError>,
    created: Result<NewIdentity, ApiError>,
    /// `identity.create` once more, before any test runs (it writes the
    /// settings again, so it must not race a test's `settings.set`).
    created_again: Result<NewIdentity, ApiError>,
}

/// The process's engine reads the profile folder and the secret-store choice
/// on its first call: point both at a throwaway profile first, then make the
/// identity once for every test of this binary.
fn lab() -> &'static Lab {
    static LAB: OnceLock<Lab> = OnceLock::new();
    LAB.get_or_init(|| {
        // A release engine refuses the file-backed stores (API.md, "Profile
        // and raven-node"): these tests need the debug build of raven-client.
        if !cfg!(debug_assertions) {
            panic!(
                "the engine tests need a debug build of raven-client: run them without --release"
            );
        }
        for key in [
            "RAVEN_IDENTITY_BACKEND",
            "RAVEN_SESSION_BACKEND",
            "RAVEN_PREKEY_BACKEND",
            "RAVEN_CHAT_HISTORY_BACKEND",
        ] {
            std::env::set_var(key, "locked-file");
        }
        std::env::set_var("RAVEN_ALLOW_EPHEMERAL_DATA_DIR", "1");
        // Should anything start raven-node: loopback and a free port only.
        std::env::set_var("RAVEN_SERVICE_LAN_LISTEN", "127.0.0.1:0");
        for key in [
            "ASH_DATA_DIR",
            "RAVEN_PEER",
            "ASH_LAN_DIAL",
            "RAVEN_VERBOSE",
            "ASH_VERBOSE",
            "RAVEN_LAB_TEST_A",
            "RAVEN_INTERNET_LISTEN",
            "RAVEN_P2P_LISTEN",
            "RAVEN_P2P_RELAYS",
            "RAVEN_P2P_RELAY",
            "RAVEN_TRACE_LOG",
            // The privacy pack's lab switches (raven-core's
            // PRIVACY_LAB_SWITCHES): a plain profile, whatever the suite runs
            // with (raven-client's own tests cover sealed ones).
            "RAVEN_LAB_AT_REST",
            "RAVEN_LAB_ERASE_ROOT",
            "RAVEN_LAB_AT_REST_KEY_FAULT",
            "RAVEN_LAB_RETENTION_GRACE_MS",
            "RAVEN_LAB_LOGIN_ITEM_ROOT",
            "RAVEN_LAB_LOGIN_ITEM_TOOLS",
        ] {
            std::env::remove_var(key);
        }
        let profile = scratch_dir().join("profile");
        std::env::set_var("RAVEN_DATA_DIR", &profile);

        let engine = ClientEngine::new();
        let first_state = engine.profile_state();
        let created = engine.identity_create(Some(NAME));
        let created_again = engine.identity_create(None);
        Lab {
            profile,
            first_state,
            created,
            created_again,
        }
    })
}

/// A new, empty folder for this test process under the target directory
/// (`target/tmp/engine-lab/`, so throwaway identities never land in a shared
/// temp folder), owner-only on Unix. Folders older runs left there are
/// removed first.
fn scratch_dir() -> PathBuf {
    let base = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("engine-lab");
    std::fs::create_dir_all(&base).expect("target/tmp/engine-lab");
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(600));
            if old {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let dir = base.join(format!("{}-{nanos:x}", std::process::id()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&dir).expect("a new scratch folder");
    dir
}

fn created() -> &'static NewIdentity {
    lab()
        .created
        .as_ref()
        .expect("identity.create on the lab profile")
}

fn engine() -> ClientEngine {
    lab();
    ClientEngine::new()
}

#[test]
fn the_engine_is_linked_in() {
    assert!(ClientEngine::linked());
}

#[test]
fn app_info_names_this_build_and_platform() {
    let info = engine().app_info().expect("app.info");
    assert!(!info.version.is_empty());
    let build = if cfg!(debug_assertions) {
        Build::Dev
    } else {
        Build::Release
    };
    assert_eq!(info.build, build);
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        std::env::consts::OS
    };
    assert_eq!(info.platform, platform);
}

#[test]
fn a_new_profile_then_an_identity() {
    let lab = lab();
    let first = lab
        .first_state
        .as_ref()
        .expect("profile.state before setup");
    assert_eq!(first.state, ProfileStateKind::New, "{first:?}");

    let me = created();
    assert!(me.address.starts_with("rvn1"), "{}", me.address);
    assert_eq!(me.words.len(), 10, "{:?}", me.words);
    assert!(me.words.iter().all(|w| !w.trim().is_empty()));
    assert!(!me.old_code.is_empty());

    let now = engine().profile_state().expect("profile.state after setup");
    assert_eq!(now.state, ProfileStateKind::Ready);
    // The profile went where RAVEN_DATA_DIR points, not to ~/.raven.
    assert!(lab.profile.is_dir(), "{}", lab.profile.display());
    assert!(std::fs::read_dir(&lab.profile).unwrap().next().is_some());

    // A second identity.create keeps the identity.
    let again = lab.created_again.as_ref().expect("identity.create again");
    assert_eq!(again.address, me.address);
    assert_eq!(again.words, me.words);
}

#[test]
fn identity_me_is_the_identity_that_was_made() {
    let made = created();
    let me = engine().identity_me().expect("identity.me");
    assert_eq!(me.address, made.address);
    assert_eq!(me.words, made.words);
    assert_eq!(me.old_code, made.old_code);
    assert_eq!(me.display_name.as_deref(), Some(NAME));
    assert!(!me.card.trim().is_empty());
    assert!(me.invite.starts_with("raven:"), "{}", me.invite);
}

#[test]
fn settings_round_trip_into_the_profile() {
    created();
    let e = engine();
    let set = e
        .settings_set(&SettingsPatch {
            receive: Some(false),
            reduced_motion: Some(true),
            when_quit: Some(WhenQuit::Ask),
            keep_chats_days: Some(Some(7)),
            lock_on_sleep: Some(true),
            ..SettingsPatch::default()
        })
        .expect("settings.set");
    assert!(!set.receive);
    assert!(set.reduced_motion);
    assert_eq!(set.when_quit, WhenQuit::Ask);
    assert_eq!(set.keep_chats_days, Some(7));
    // Raven always forgets unused friends' addresses (privacy pack PP.7).
    assert!(set.forget_addresses);
    assert!(set.lock_on_sleep);
    assert_eq!(set.display_name.as_deref(), Some(NAME), "kept");
    assert_eq!(e.settings_get().expect("settings.get"), set);

    // "Always" keeps chats: null on the wire; only that field changes.
    let always = e
        .settings_set(&SettingsPatch {
            keep_chats_days: Some(None),
            ..SettingsPatch::default()
        })
        .expect("settings.set keep_chats_days null");
    assert_eq!(always.keep_chats_days, None);
    assert!(always.reduced_motion && !always.receive);

    // Written into the profile (owner-only on Unix).
    let file = lab().profile.join("desktop_settings.json");
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("desktop_settings.json")).unwrap();
    assert_eq!(saved["when_quit"], "ask");
    assert_eq!(saved["receive"], false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }

    // The name and the chat-keeping time are not in it: the name has its
    // own file (sealed once the profile is), the time is the privacy pack's
    // setting.
    assert!(saved.get("display_name").is_none(), "{saved}");
    assert!(saved.get("keep_chats_days").is_none(), "{saved}");

    // A value outside the contract is refused and changes nothing.
    let e2 = e
        .call("settings.set", json!({ "when_quit": "never" }))
        .unwrap_err();
    assert!(e2.is(codes::BAD_REQUEST), "{e2:?}");
    assert_eq!(e.settings_get().unwrap().when_quit, WhenQuit::Ask);
    // Forgetting addresses can't be turned off.
    let e3 = e
        .settings_set(&SettingsPatch {
            forget_addresses: Some(false),
            ..SettingsPatch::default()
        })
        .unwrap_err();
    assert!(e3.is(codes::NOT_AVAILABLE_YET), "{e3:?}");
    assert!(e.settings_get().unwrap().forget_addresses);
}

#[test]
fn privacy_state_and_the_actions_the_app_does_not_offer_yet() {
    let e = engine();
    // A plain profile (a debug build without the lab switch), not locked.
    let p = e.privacy_state().expect("privacy.state");
    assert!(!p.available && !p.encrypted && !p.locked, "{p:?}");
    // Unlock clears a `raven lock` (none here) and answers as profile.state.
    let unlocked = e.privacy_unlock().expect("privacy.unlock");
    assert_eq!(unlocked.state, ProfileStateKind::Ready);
    for err in [
        e.privacy_lock().map(|_| ()).unwrap_err(),
        e.privacy_encrypt().unwrap_err(),
        e.privacy_erase().unwrap_err(),
    ] {
        assert!(err.is(codes::NOT_AVAILABLE_YET), "{err:?}");
        assert!(!err.message.is_empty());
    }
}

/// The network ops through the app's types (API.md, Network): every way
/// decodes with a known state, a held way is refused and changes nothing,
/// the help-friends flags reach the profile's node policy, and the doctor,
/// security, Lock and updates views decode. No raven-node is started.
#[test]
fn network_ops_through_the_app_types() {
    created();
    let e = engine();
    let net = e.network_status().expect("network.status");
    let lan = net.way("lan").expect("the LAN way");
    assert_eq!(lan.state, CarrierState::On, "{lan:?}");
    for w in net.carriers.iter().chain(net.protections.iter()) {
        assert_ne!(w.state, CarrierState::Unknown, "{w:?}");
        assert!(!w.reason.is_empty(), "{w:?}");
    }
    assert!(net.help.effects.store.contains("changes nothing yet"));
    // Tor: a way of its own, held behind its gate, with no switch; its state
    // (NetworkStatus `tor`) only while raven-node's Status has its line,
    // and this test starts no raven-node.
    let tor = net.way("tor").expect("the Tor way");
    assert_eq!(tor.gate, "ONION_PRODUCTION_ENABLED", "{tor:?}");
    assert!(!tor.can_set, "{tor:?}");
    assert!(net.tor.is_none(), "{:?}", net.tor);
    if net.way("p2p").unwrap().state == CarrierState::Preview {
        let err = e
            .network_set(&NetworkPatch {
                p2p: Some(true),
                ..NetworkPatch::default()
            })
            .unwrap_err();
        assert!(err.is(codes::NOT_AVAILABLE_YET), "{err:?}");
    }
    let set = e
        .network_set(&NetworkPatch {
            store: Some(false),
            ..NetworkPatch::default()
        })
        .expect("network.set store");
    assert_eq!(set.changes.len(), 1);
    assert!(!set.status.help.store && !set.status.help.automatic);
    let policy = std::fs::read_to_string(lab().profile.join("node_policy.json")).unwrap();
    assert!(policy.contains("\"store\": false"), "{policy}");
    e.network_set(&NetworkPatch {
        store: Some(true),
        ..NetworkPatch::default()
    })
    .expect("network.set store back");
    let check = e.network_check(false).expect("network.check");
    assert!(
        check.summary.iter().any(|r| r.key == "identity"),
        "{check:?}"
    );
    assert!(check.router.is_none());
    e.security_status().expect("security.status");
    e.lock_state().expect("lock.state");
    let upd = e.updates_status().expect("updates.status");
    assert_eq!(upd.install, "cli_only");
}

/// The Network, Check network and Security screens' view models built
/// from the real engine on the throwaway profile (no lab switch set): every
/// way has a state chip and a sentence this app knows, LAN direct has a
/// switch, there is no lab banner, a setting that needs a yes shows the
/// engine's own words first, and a held way is refused even with the yes.
/// No raven-node is started.
#[test]
fn the_network_screens_read_the_real_engine() {
    created();
    let e = engine();
    let net = e.network_status().expect("network.status");
    let rows = netview::way_rows(&net, true);
    assert!(rows.len() >= 13, "{rows:?}");
    for r in rows
        .iter()
        .chain(netview::protection_rows(&net, true).iter())
    {
        assert_ne!(r.chip, Chip::Unknown, "{r:?}");
        assert!(
            netview::way_does(&r.id).is_some(),
            "a way the app has no sentence for: {}",
            r.id
        );
        assert!(r.line.len() > 20, "{r:?}");
    }
    let lan = rows.iter().find(|r| r.id == "lan").expect("LAN direct");
    let toggle = lan.toggle.as_ref().expect("LAN direct has a switch");
    assert!(toggle.sensitive && toggle.active, "{toggle:?}");
    assert_eq!(netview::lab_banner(&net, true), None, "no lab switch set");
    assert!(
        netview::this_computer(&net, raven_desktop::timefmt::now_ms())
            .iter()
            .any(|l| l.starts_with("This profile's port: "))
    );
    assert!(!netview::mail_view(&net).lines.is_empty());
    assert!(!netview::nearby_view(&net).lines.is_empty());
    assert!(!netview::ways_on_line(&net).is_empty());

    match netflow::set(&e, netflow::patch_for("handoff", true)) {
        SetOutcome::NeedsConsent(sheet) => {
            assert_eq!(sheet.consent, Consent::Handoff);
            assert!(!sheet.engine_text.is_empty());
            assert!(!sheet.engine_text.contains("[y/N]"));
        }
        other => panic!("{other:?}"),
    }
    if net.way("handoff").unwrap().state == CarrierState::Preview {
        match netflow::agree(&e, Consent::Handoff) {
            SetOutcome::Refused(err) => assert!(err.is(codes::NOT_AVAILABLE_YET)),
            other => panic!("{other:?}"),
        }
    }

    let check = netview::check_view(&e.network_check(false).expect("network.check"));
    assert!(
        !check.summary.is_empty() && !check.rows.is_empty(),
        "{check:?}"
    );
    secview::security_view(&e.security_status().expect("security.status"));
    let lock = secview::lock_view(&e.lock_state().expect("lock.state"));
    assert!(
        !lock.locked && !lock.can_lock,
        "no raven-node runs here: {lock:?}"
    );
    let updates = secview::updates_view(&e.updates_status().expect("updates.status"));
    assert_eq!(updates.install, "Installing updates isn't in this app yet.");
}

/// Groups are lab only: without the lab switches (`RAVEN_LAB_GROUPS` and
/// the others, never set here) the engine says `available: false`, the app
/// shows no Groups section, and the other ops are NOT_AVAILABLE_YET.
#[test]
fn groups_are_not_offered_without_the_lab_switches() {
    let _ = created();
    let e = engine();
    let list = e.groups_list().expect("groups.list");
    assert!(!list.available, "{list:?}");
    assert!(list.groups.is_empty());
    let err = e
        .groups_send("00000000000000000000000000000000", "hi")
        .expect_err("groups.send without groups");
    assert!(err.is(codes::NOT_AVAILABLE_YET), "{err:?}");
    assert!(!err.message.is_empty());
}

#[test]
fn errors_come_back_as_api_errors() {
    let e = engine();
    let err = e.call("no.such.op", json!({})).unwrap_err();
    assert!(err.is(codes::BAD_REQUEST), "{err:?}");
    assert!(!err.message.is_empty());
    created();
    let err = e.messages_history("rvn1qnobodyhere", None, 10).unwrap_err();
    assert!(err.is(codes::NOT_A_CONTACT), "{err:?}");
    assert_eq!(e.contacts_list().expect("contacts.list"), vec![]);
}

#[test]
fn events_poll_on_the_engine_threads() {
    created();
    let worker = EngineWorker::start(|| Box::new(ClientEngine::new()));
    let first = block_on(worker.poll_events(0)).expect("events.poll");
    // A call on the calls lane and a poll on the events lane, together.
    let status = worker.call(|e| e.node_status());
    let second = block_on(worker.poll_events(first.seq)).expect("events.poll again");
    assert!(second.seq >= first.seq);
    assert!(second.events.iter().all(|ev| ev.seq > first.seq));
    let status = block_on(status).expect("node.status");
    assert!(!status.running, "no raven-node runs for the lab profile");
    assert!(!status.started_by_app);
    worker.shutdown();
}

/// A tiny `block_on` for the worker's replies (it never waits forever).
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct ThreadWaker(std::thread::Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let mut f = Box::pin(f);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let start = std::time::Instant::now();
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "no answer within 60 s"
        );
        std::thread::park_timeout(Duration::from_millis(50));
    }
}
