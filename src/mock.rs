//! `MockRaven`: an in-memory engine that follows `desktop/api/API.md`, for
//! reviewing the screens with demo friends and demo events. Compiled only in
//! debug builds (`cfg(debug_assertions)`), and used only when asked for
//! ([`demo_requested`]: `RAVEN_DEMO=1` or `RAVEN_MOCK_PROFILE`); otherwise
//! debug builds run on the real engine too.
//!
//! Demo data: friends Ali (verified, in touch), Mina (verified, one waiting
//! message) and Rena (not verified). After the first `events.poll` it replays
//! the demo events of the design mockup: a message from Ali, one from Rena,
//! a retry of Mina's waiting message, then Mina comes back and the waiting
//! message is delivered. A heartbeat (`status_ok`) follows every status-check
//! interval while the background program runs. Sending to a friend who is in
//! touch gives `send_started`, `state: sent` and a signed `receipt`.
//!
//! The mock goes through the same JSON envelope as the real engine
//! (`handle_request` has the shape of `raven_call()`).
//!
//! Security words use Raven's draft 256-word list and the real GF(2^8) check
//! word, so "typo" (the check word catches a misheard or swapped word) and
//! "mismatch" (real words, wrong key) behave like the design. The friends'
//! words here are demo values; the app never shows them.
//!
//! Error messages use the engine texts of strings.en.json (`error.*`); a few
//! mock-only situations have their own text.
//!
//! The old code: Rena's demo old code is `Tq7m-29Xa-hKe4` (the same in the
//! macOS and Windows mocks). The privacy pack is not here yet: `privacy.state`
//! says `available: false` and the privacy actions answer `NOT_AVAILABLE_YET`.

use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::api::*;
use crate::strings::{tr, trf};
use crate::{theme, timefmt, words};

mod groups;

/// Raven's draft security word list (index TAB word), shared with the
/// design prototype.
const WORDLIST_TXT: &str = include_str!("../core/desktop/design/raven-security-words-v1.txt");

/// The 256 words.
pub fn wordlist() -> &'static [&'static str] {
    static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| {
        WORDLIST_TXT
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| l.split('\t').nth(1))
            .map(str::trim)
            .collect()
    })
}

fn xtime(v: u8) -> u8 {
    let v = (v as u16) << 1;
    if v & 0x100 != 0 {
        (v ^ 0x11B) as u8
    } else {
        v as u8
    }
}

/// The check index for nine data indexes (GF(2^8) Horner, AES field).
pub fn gf_check(data: &[u8]) -> u8 {
    let mut acc = 0u8;
    for d in data {
        acc = xtime(acc) ^ d;
    }
    xtime(acc)
}

/// True when ten indexes fit together (the check word matches).
pub fn check_ok(indexes: &[u8]) -> bool {
    let mut acc = 0u8;
    for d in indexes {
        acc = xtime(acc) ^ d;
    }
    acc == 0
}

/// Resolve one typed token: the exact word, a plural, or a prefix of at
/// least three letters (unique by the list's design).
pub fn resolve_word(token: &str) -> Option<u8> {
    let t = token.trim().to_ascii_lowercase();
    if t.len() < 3 {
        return None;
    }
    let list = wordlist();
    let find = |w: &str| list.iter().position(|x| *x == w).map(|i| i as u8);
    find(&t)
        .or_else(|| t.strip_suffix("es").and_then(find))
        .or_else(|| t.strip_suffix('s').and_then(find))
        .or_else(|| {
            let mut hits = list
                .iter()
                .enumerate()
                .filter(|(_, w)| w.starts_with(t.as_str()));
            match (hits.next(), hits.next()) {
                (Some((i, _)), None) => Some(i as u8),
                _ => None,
            }
        })
}

fn words_for(data_words: &[&str]) -> [u8; 10] {
    let mut ix = [0u8; 10];
    for (i, w) in data_words.iter().enumerate().take(9) {
        ix[i] = resolve_word(w).expect("demo word is in the list");
    }
    ix[9] = gf_check(&ix[..9]);
    ix
}

/// Demo words for a new friend, derived from the address (FNV-1a).
fn words_from_address(address: &str) -> [u8; 10] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut ix = [0u8; 10];
    for (i, slot) in ix.iter_mut().take(9).enumerate() {
        for b in address.bytes().chain([i as u8]) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        *slot = (h >> 24) as u8;
    }
    ix[9] = gf_check(&ix[..9]);
    ix
}

fn names(ix: &[u8]) -> Vec<String> {
    ix.iter()
        .map(|i| wordlist()[*i as usize].to_string())
        .collect()
}

fn short(address: &str) -> String {
    if address.len() <= 14 {
        return address.to_string();
    }
    format!("{}\u{2026}{}", &address[..9], &address[address.len() - 4..])
}

/// Time source: the system clock, or a manual one for tests.
#[derive(Clone, Debug)]
pub enum Clock {
    System,
    Manual(Arc<AtomicU64>),
}

impl Clock {
    pub fn now(&self) -> u64 {
        match self {
            Clock::System => timefmt::now_ms(),
            Clock::Manual(t) => t.load(Ordering::SeqCst),
        }
    }
}

const ME_ADDRESS: &str = "rvn1qysluvwl5922yctzd0u9gpr06gn3k7ldfvecule0";
const ME_PUB_HEX: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const ME_OLD_CODE: &str = "If4x-36FU-omFi";
const ME_WORDS: [&str; 9] = [
    "turtle", "frog", "bread", "wolf", "paper", "laptop", "funnel", "mango", "honey",
];
const ME_LAN_IP: &str = "192.168.1.23";
const PORT: &str = "7420";
const MOCK_PID: u32 = 4242;
const ALI: &str = "rvn1qam4hgx8dewltxpwe6cras5ypnk8qfmzx";
const MINA: &str = "rvn1qmy2f5a9m0gw4ku8ch6mj4cxs00s3zz5x";
const RENA: &str = "rvn1qpf74lw30082lpdvr9dp6gsraf5xfauxp";
/// Rena's demo words (not verified yet): type these in "Check words" to see
/// a match. Documented in README.md for reviewers; never shown by the app.
pub const RENA_DEMO_WORDS: [&str; 9] = [
    "anchor", "basket", "candle", "eagle", "garden", "helmet", "island", "jacket", "kettle",
];
/// Rena's demo old code, for "Use the old code instead" (the same value in
/// the three desktop mocks). Documented in README.md; never shown by the app.
pub const RENA_DEMO_OLD_CODE: &str = "Tq7m-29Xa-hKe4";
const LAN_PATH: &str = "LAN direct";
/// The stable kind of [`LAN_PATH`] (API.md, Message `path_kind`).
const LAN_KIND: &str = "lan";
const WAIT_MS: u64 = 24 * 3_600_000;
const MAX_TEXT: usize = 4_000;

#[derive(Debug, Clone)]
enum Due {
    Incoming {
        address: String,
        text: String,
    },
    State {
        id: String,
        state: MessageState,
    },
    Receipt {
        id: String,
        ms: u64,
    },
    Retry {
        id: String,
    },
    Presence {
        address: String,
        presence: Presence,
    },
    NodeListening,
    /// A member's message in a group.
    GroupIncoming {
        group: String,
        from: String,
        text: String,
    },
    /// A member's copy of your group message was confirmed.
    GroupReceipt {
        group: String,
        id: String,
        member: String,
    },
}

struct MockContact {
    c: Contact,
    words: [u8; 10],
    /// The friend's old code (only the demo friend Rena has one).
    old_code: Option<&'static str>,
    /// The newest message read (`messages.mark_read`), as the engine's read
    /// marker.
    read_up_to: Option<String>,
}

/// One friend's routes in the mock (by address): Internet route, p2p
/// route, mail friends, hide my IP.
type MockRoutes = (String, Option<String>, Option<String>, Vec<String>, bool);

/// The mock's network settings (a release build's: only LAN direct is on).
#[derive(Default)]
struct MockNet {
    lan_off: bool,
    /// `raven lan on --for`: when LAN direct turns off again.
    lan_until_ms: Option<u64>,
    hide_ip: bool,
    /// The node policy flags: bridge, store, relay, and AUTO.
    flags: (bool, bool, bool, bool),
    routes: Vec<MockRoutes>,
    /// `RAVEN_MOCK_NETWORK=lab`: a developer build with lab switches set,
    /// so every screen state can be reviewed (lab ways, consent, restarts,
    /// Lock v2, security notices, updates). `None`: a release build's
    /// answers, as API.md's Mocks section says.
    lab: Option<MockLab>,
}

/// The lab build's own state.
#[derive(Default)]
struct MockLab {
    /// This computer has Bluetooth LE (`lab-no-bluetooth`: it doesn't).
    bluetooth: bool,
    /// The settings switched on, by their `network.set` name.
    on: Vec<&'static str>,
    /// The settings the person decided on (on or off) at least once.
    decided: Vec<&'static str>,
    /// What the background program started with: settings that apply at
    /// a restart are in use only once they are in here.
    started_with: Vec<&'static str>,
    proxy: String,
    /// Lock v2: the background program runs without its keys.
    locked: bool,
    wrong_passphrases: u32,
    wait_until_ms: u64,
}

impl MockLab {
    fn is_on(&self, setting: &str) -> bool {
        self.on.contains(&setting)
    }

    /// Switched on and, for a setting that applies at a restart, in use
    /// since the background program started.
    fn in_use(&self, setting: &str) -> bool {
        self.is_on(setting)
            && (!RESTART_SETTINGS.contains(&setting) || self.started_with.contains(&setting))
    }
}

struct State {
    profile: ProfileStateKind,
    settings: Settings,
    node: NodeStatus,
    net: MockNet,
    contacts: Vec<MockContact>,
    messages: Vec<Message>,
    events: Vec<Event>,
    seq: u64,
    scheduled: Vec<(u64, Due)>,
    demo_started: bool,
    last_status_ok: u64,
    next_id: u64,
    /// Groups (draft contract): the demo group.
    groups: groups::MockGroups,
}

pub struct MockRaven {
    state: Mutex<State>,
    clock: Clock,
}

/// The environment variable that runs a debug build on the mock.
pub const DEMO_ENV: &str = "RAVEN_DEMO";
/// `lab` (or `lab-no-bluetooth`): the mock answers the network ops as a
/// developer build with lab switches set ([`MockRaven::with_lab_network`]).
pub const MOCK_NETWORK_ENV: &str = "RAVEN_MOCK_NETWORK";
/// Lock v2's demo passphrase in the lab mock (documented in README.md; a
/// real passphrase only ever goes to raven-node).
pub const MOCK_PASSPHRASE: &str = "demo passphrase";
/// The mock's start state (`ready` or `locked`); setting it also asks for
/// the mock.
pub const MOCK_PROFILE_ENV: &str = "RAVEN_MOCK_PROFILE";

/// True when the app was asked to run on the mock instead of the real
/// engine: `RAVEN_DEMO` set to anything but empty or `0`, or a
/// `RAVEN_MOCK_PROFILE`. `var` reads one environment variable.
pub fn demo_requested(var: impl Fn(&str) -> Option<String>) -> bool {
    let on = |key: &str| var(key).is_some_and(|v| !matches!(v.trim(), "" | "0"));
    on(DEMO_ENV) || on(MOCK_PROFILE_ENV)
}

impl MockRaven {
    /// A first-run profile (Welcome and Setup are shown).
    pub fn new() -> Self {
        Self::with_clock(Clock::System, false)
    }

    /// `RAVEN_MOCK_PROFILE=ready` starts with an identity (skips Setup);
    /// `RAVEN_MOCK_PROFILE=locked` starts locked, to review the Locked
    /// window (unlocking answers `NOT_AVAILABLE_YET` until the privacy pack).
    pub fn from_env() -> Self {
        let profile = std::env::var(MOCK_PROFILE_ENV).unwrap_or_default();
        let mock = Self::with_clock(Clock::System, profile == "ready" || profile == "locked");
        if profile == "locked" {
            mock.state.lock().unwrap_or_else(|e| e.into_inner()).profile = ProfileStateKind::Locked;
        }
        match std::env::var(MOCK_NETWORK_ENV).unwrap_or_default().trim() {
            "lab" => mock.with_lab_network(true),
            "lab-no-bluetooth" => mock.with_lab_network(false),
            _ => mock,
        }
    }

    /// Answer the network ops as a developer build with lab switches set
    /// (`RAVEN_MOCK_NETWORK=lab`): the held ways open as `lab`, the three
    /// settings that need a yes answer `CONSENT_NEEDED` without it, the
    /// settings that apply at a restart say so, Lock v2 locks (the demo
    /// passphrase is [`MOCK_PASSPHRASE`]), and there is a security notice
    /// and an updates view. `bluetooth: false` is a computer without
    /// Bluetooth LE (nearby `not_on_this_os`).
    pub fn with_lab_network(self, bluetooth: bool) -> Self {
        {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.net.lab = Some(MockLab {
                bluetooth,
                proxy: "auto".into(),
                ..MockLab::default()
            });
            s.node.lab = MOCK_LAB_WAYS.iter().map(|w| w.to_string()).collect();
            s.node.lab_switches = MOCK_LAB_SWITCHES.iter().map(|w| w.to_string()).collect();
        }
        self
    }

    pub fn with_clock(clock: Clock, ready: bool) -> Self {
        let now = clock.now();
        let mut s = State {
            profile: ProfileStateKind::New,
            settings: Settings {
                display_name: None,
                receive: true,
                reduced_motion: false,
                when_quit: WhenQuit::Keep,
                keep_chats_days: Some(30),
                forget_addresses: true,
                lock_on_sleep: false,
            },
            node: NodeStatus {
                running: false,
                started_by_app: false,
                pid: None,
                listening: None,
                receive: true,
                address: Some(ME_LAN_IP.into()),
                last_ok_ms: 0,
                build: Build::Dev,
                carriers: Carriers {
                    lan: CarrierState::On,
                    internet: CarrierState::Preview,
                    p2p: CarrierState::Preview,
                    custody: CarrierState::Preview,
                    nearby: CarrierState::Preview,
                },
                lab: Vec::new(),
                lab_switches: Vec::new(),
            },
            net: MockNet {
                flags: (true, true, false, true),
                ..MockNet::default()
            },
            contacts: Vec::new(),
            messages: Vec::new(),
            events: Vec::new(),
            seq: 0,
            scheduled: Vec::new(),
            demo_started: false,
            last_status_ok: 0,
            next_id: 1,
            groups: groups::MockGroups::seed(now),
        };
        s.seed_demo(now);
        if ready {
            s.profile = ProfileStateKind::Ready;
            s.node.running = true;
            s.node.pid = Some(MOCK_PID);
            s.node.listening = Some(format!("0.0.0.0:{PORT}"));
        }
        Self {
            state: Mutex::new(s),
            clock,
        }
    }

    /// The `raven_call()` shape: request JSON in, response JSON out.
    pub fn handle_request(&self, request: &str) -> String {
        let response = match serde_json::from_str::<Value>(request) {
            Err(e) => Err(bad_request(format!("request is not JSON: {e}"))),
            Ok(req) => {
                let op = req
                    .get("op")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let args = req.get("args").cloned().unwrap_or_else(|| json!({}));
                if req.get("v").and_then(Value::as_u64) != Some(API_VERSION) {
                    Err(bad_request("unsupported contract version"))
                } else {
                    let now = self.clock.now();
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.advance(now);
                    s.dispatch(&op, &args, now)
                }
            }
        };
        match response {
            Ok(result) => json!({ "v": API_VERSION, "ok": true, "result": result }),
            Err(e) => json!({ "v": API_VERSION, "ok": false, "error": e }),
        }
        .to_string()
    }
}

impl Default for MockRaven {
    fn default() -> Self {
        Self::new()
    }
}

impl RavenEngine for MockRaven {
    fn call(&self, op: &str, args: Value) -> Result<Value, ApiError> {
        parse_response(&self.handle_request(&make_request(op, &args)))
    }
}

/// An unknown operation or bad arguments.
fn bad_request(detail: impl Into<String>) -> ApiError {
    ApiError::new(
        codes::BAD_REQUEST,
        tr("error.badRequest"),
        format!("mock: {}", detail.into()),
    )
}

/// A privacy action before the privacy pack lands.
fn not_available_yet(op: &str) -> ApiError {
    ApiError::new(
        codes::NOT_AVAILABLE_YET,
        tr("error.notAvailableYet"),
        format!("mock: {op} comes with the privacy pack"),
    )
}

/// A way this version holds (the network ops, API.md).
fn not_in_this_version(what: &str) -> ApiError {
    ApiError::new(
        codes::NOT_AVAILABLE_YET,
        tr("error.notAvailableYet"),
        format!("mock: {what} is held in this version"),
    )
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request(format!("missing string argument `{key}`")))
}

fn arg_id(args: &Value) -> Result<String, ApiError> {
    match args.get("id") {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(bad_request("missing argument `id`")),
    }
}

fn unknown_message() -> ApiError {
    bad_request("unknown message id")
}

fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, ApiError> {
    serde_json::to_value(v)
        .map_err(|e| ApiError::new(codes::INTERNAL, tr("error.internal"), e.to_string()))
}

fn not_a_contact() -> ApiError {
    ApiError::new(
        codes::NOT_A_CONTACT,
        tr("error.notAContact"),
        "mock: unknown address",
    )
}

impl State {
    fn seed_demo(&mut self, now: u64) {
        let contact =
            |address: &str, name: &str, verified, presence, last: Option<u64>, lan: &str| Contact {
                address: address.into(),
                name: name.into(),
                short_address: short(address),
                verified,
                presence,
                last_contact_ms: last,
                waiting: None,
                avatar_index: theme::avatar_index(address) as u8,
                lan: Some(lan.into()),
                unread: 0,
            };
        self.contacts = vec![
            MockContact {
                c: contact(
                    ALI,
                    "Ali",
                    true,
                    Presence::InTouch,
                    Some(now - 20_000),
                    "192.168.1.31:7420",
                ),
                words: words_for(&[
                    "airport", "almond", "apple", "apron", "baby", "badger", "ball", "bamboo",
                    "banana",
                ]),
                old_code: None,
                // Ali's question was read (and answered) before the demo.
                read_up_to: Some("d2".into()),
            },
            MockContact {
                c: contact(
                    MINA,
                    "Mina",
                    true,
                    Presence::Waiting,
                    Some(now - 3 * 3_600_000),
                    "192.168.1.35:7420",
                ),
                words: words_for(&[
                    "beach", "bed", "beetle", "belt", "bicycle", "barrel", "battery", "basket",
                    "anchor",
                ]),
                old_code: None,
                read_up_to: None,
            },
            MockContact {
                c: contact(
                    RENA,
                    "Rena",
                    false,
                    Presence::NoRecent,
                    None,
                    "192.168.1.40:7420",
                ),
                words: words_for(&RENA_DEMO_WORDS),
                old_code: Some(RENA_DEMO_OLD_CODE),
                read_up_to: None,
            },
        ];
        let msg =
            |id: &str, address: &str, direction, text: &str, at_ms, state, until_ms, attempts| {
                Message {
                    id: id.into(),
                    address: address.into(),
                    direction,
                    text: text.into(),
                    at_ms,
                    state,
                    until_ms,
                    path: if state == MessageState::Delivered {
                        Some(LAN_PATH.into())
                    } else {
                        None
                    },
                    path_kind: (state == MessageState::Delivered).then(|| LAN_KIND.into()),
                    delivered_at_ms: None,
                    attempts,
                }
            };
        let mina_until = now + 22 * 3_600_000;
        self.messages = vec![
            msg(
                "d1",
                ALI,
                Direction::In,
                "Are you coming to the meeting?",
                now - 10 * 60_000,
                MessageState::Delivered,
                None,
                None,
            ),
            msg(
                "d2",
                ALI,
                Direction::Out,
                "Yes, see you there",
                now - 9 * 60_000,
                MessageState::Delivered,
                None,
                Some(1),
            ),
            msg(
                "d3",
                MINA,
                Direction::Out,
                "Can you send me the address?",
                now - 2 * 3_600_000,
                MessageState::Waiting,
                Some(mina_until),
                Some(2),
            ),
        ];
        self.contacts[1].c.waiting = Some(Waiting {
            count: 1,
            until_ms: mina_until,
        });
    }

    /// The friend's messages in the order the chat shows them.
    fn chat(&self, address: &str) -> Vec<Message> {
        let mut list: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| m.address == address)
            .cloned()
            .collect();
        list.sort_by_key(|m| m.at_ms);
        list
    }

    /// The friend's messages after the one last read (API.md, Contact
    /// `unread`); yours never count.
    fn unread_of(&self, address: &str) -> u32 {
        let theirs = self.chat(address);
        let marked = self
            .contacts
            .iter()
            .find(|c| c.c.address == address)
            .and_then(|c| c.read_up_to.as_deref());
        let start = marked
            .and_then(|id| theirs.iter().rposition(|m| m.id == id))
            .map_or(0, |at| at + 1);
        theirs[start..]
            .iter()
            .filter(|m| m.direction == Direction::In)
            .count() as u32
    }

    /// The friend's count again, with an `unread` event when it changed.
    fn recount_unread(&mut self, at: u64, address: &str) {
        let n = self.unread_of(address);
        let Some(c) = self.contacts.iter_mut().find(|c| c.c.address == address) else {
            return;
        };
        if c.c.unread == n {
            return;
        }
        c.c.unread = n;
        let address = address.to_string();
        self.emit(at, EventKind::Unread { address, unread: n });
    }

    fn emit(&mut self, at_ms: u64, kind: EventKind) {
        self.seq += 1;
        self.events.push(Event {
            seq: self.seq,
            at_ms,
            kind,
        });
        if self.events.len() > 1_000 {
            self.events.drain(..500);
        }
    }

    fn schedule(&mut self, at: u64, due: Due) {
        self.scheduled.push((at, due));
    }

    fn contact_mut(&mut self, address: &str) -> Option<&mut MockContact> {
        self.contacts.iter_mut().find(|c| c.c.address == address)
    }

    fn message_mut(&mut self, id: &str) -> Option<&mut Message> {
        self.messages.iter_mut().find(|m| m.id == id)
    }

    fn new_id(&mut self) -> String {
        let id = format!("m{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn recount_waiting(&mut self, address: &str) {
        let waiting: Vec<u64> = self
            .messages
            .iter()
            .filter(|m| m.address == address && m.state == MessageState::Waiting)
            .map(|m| m.until_ms.unwrap_or(0))
            .collect();
        if let Some(c) = self.contact_mut(address) {
            c.c.waiting = waiting.iter().max().map(|until| Waiting {
                count: waiting.len() as u32,
                until_ms: *until,
            });
        }
    }

    /// Apply everything due by `now`, and the heartbeat.
    fn advance(&mut self, now: u64) {
        if self.node.running && self.profile == ProfileStateKind::Ready {
            let interval = theme::tokens().durations.status_check_interval as u64;
            if self.last_status_ok == 0 || now.saturating_sub(self.last_status_ok) >= interval {
                self.last_status_ok = now;
                self.node.last_ok_ms = now;
                self.emit(now, EventKind::StatusOk);
            }
        }
        self.scheduled.sort_by_key(|(at, _)| *at);
        while self.scheduled.first().is_some_and(|(at, _)| *at <= now) {
            let (at, due) = self.scheduled.remove(0);
            self.apply(at, due);
        }
    }

    fn apply(&mut self, at: u64, due: Due) {
        match due {
            Due::Incoming { address, text } => {
                let id = self.new_id();
                let message = Message {
                    id,
                    address: address.clone(),
                    direction: Direction::In,
                    text,
                    at_ms: at,
                    state: MessageState::Delivered,
                    until_ms: None,
                    path: Some(LAN_PATH.into()),
                    path_kind: Some(LAN_KIND.into()),
                    delivered_at_ms: None,
                    attempts: None,
                };
                self.messages.push(message.clone());
                self.emit(at, EventKind::Incoming { message });
                self.recount_unread(at, &address);
                self.set_presence(at, &address, Presence::InTouch);
            }
            Due::State { id, state } => {
                let Some(m) = self.message_mut(&id) else {
                    return;
                };
                if m.state == MessageState::Delivered {
                    return; // a receipt already won
                }
                m.state = state;
                if state == MessageState::Waiting {
                    m.until_ms = Some(at + WAIT_MS);
                }
                let address = m.address.clone();
                self.emit(at, EventKind::State { id, state });
                self.recount_waiting(&address);
            }
            Due::Receipt { id, ms } => {
                let Some(m) = self.message_mut(&id) else {
                    return;
                };
                if m.state == MessageState::Failed {
                    return;
                }
                m.state = MessageState::Delivered;
                m.path = Some(LAN_PATH.into());
                m.path_kind = Some(LAN_KIND.into());
                m.until_ms = None;
                let address = m.address.clone();
                self.emit(
                    at,
                    EventKind::Receipt {
                        id,
                        path: LAN_PATH.into(),
                        path_kind: LAN_KIND.into(),
                        ms,
                        delivered_at_ms: None,
                    },
                );
                self.recount_waiting(&address);
                if let Some(c) = self.contact_mut(&address) {
                    c.c.last_contact_ms = Some(at);
                }
            }
            Due::Retry { id } => {
                let Some(m) = self.message_mut(&id) else {
                    return;
                };
                if m.state != MessageState::Waiting {
                    return;
                }
                let attempts = m.attempts.unwrap_or(0) + 1;
                m.attempts = Some(attempts);
                let address = m.address.clone();
                self.emit(
                    at,
                    EventKind::Retry {
                        id,
                        address,
                        attempts,
                    },
                );
            }
            Due::Presence { address, presence } => self.set_presence(at, &address, presence),
            Due::GroupIncoming { group, from, text } => {
                self.group_incoming(at, &group, &from, &text)
            }
            Due::GroupReceipt { group, id, member } => self.group_receipt(at, &group, &id, &member),
            Due::NodeListening => {
                if self.node.running && self.node.receive {
                    self.node.listening = Some(format!("0.0.0.0:{PORT}"));
                    let status = self.node.clone();
                    self.emit(at, EventKind::Node { status });
                }
            }
        }
    }

    fn set_presence(&mut self, at: u64, address: &str, presence: Presence) {
        let Some(c) = self.contact_mut(address) else {
            return;
        };
        if presence == Presence::InTouch {
            c.c.last_contact_ms = Some(at);
        }
        if c.c.presence != presence {
            c.c.presence = presence;
            self.emit(
                at,
                EventKind::Presence {
                    address: address.into(),
                    presence,
                },
            );
        }
    }

    /// The mockup's demo, relative to the first poll of the main window.
    fn start_demo(&mut self, now: u64) {
        self.demo_started = true;
        self.schedule(
            now + 1_200,
            Due::Incoming {
                address: ALI.into(),
                text: "Great. Can you bring the charger?".into(),
            },
        );
        self.schedule(
            now + 2_600,
            Due::Incoming {
                address: RENA.into(),
                text: "Hi, it's Rena. I added you on Raven.".into(),
            },
        );
        self.schedule(now + 6_000, Due::Retry { id: "d3".into() });
        self.schedule(
            now + 10_500,
            Due::Presence {
                address: MINA.into(),
                presence: Presence::InTouch,
            },
        );
        self.schedule(
            now + 10_500,
            Due::Receipt {
                id: "d3".into(),
                ms: 380,
            },
        );
        self.start_group_demo(now);
    }

    fn require_ready(&self) -> Result<(), ApiError> {
        match self.profile {
            ProfileStateKind::Ready => Ok(()),
            ProfileStateKind::Locked => Err(ApiError::new(
                codes::PROFILE_LOCKED,
                tr("error.profileLocked"),
                "mock: profile locked",
            )),
            _ => Err(ApiError::new(
                codes::NOT_INITIALIZED,
                tr("error.notInitialized"),
                "mock: no identity yet",
            )),
        }
    }

    fn me_words(&self) -> Vec<String> {
        names(&words_for(&ME_WORDS))
    }

    fn invite(&self, include_address: bool) -> (String, Option<String>) {
        let base = format!("raven:{ME_ADDRESS}:{ME_PUB_HEX}");
        match (&self.node.address, include_address) {
            (Some(ip), true) => {
                let addr = format!("{ip}:{PORT}");
                (format!("{base}?lan={addr}"), Some(addr))
            }
            _ => (base, None),
        }
    }

    fn card_text(&self, invite: &str) -> String {
        format!("Raven card\naddress  {ME_ADDRESS}\ninvite   {invite}")
    }

    /// Parse an invite line out of pasted text: (address, lan).
    fn parse_invite(&self, text: &str) -> Result<(String, Option<String>), ApiError> {
        let invalid = |detail: &str| {
            ApiError::new(
                codes::CARD_INVALID,
                tr("error.cardInvalid"),
                format!("mock: {detail}"),
            )
        };
        let start = text
            .find("raven:")
            .ok_or_else(|| invalid("no `raven:` invite found"))?;
        let line = text[start..].split_whitespace().next().unwrap_or_default();
        let mut parts = line.splitn(3, ':');
        let (_, address, rest) = (
            parts.next(),
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        let (key, query) = rest.split_once('?').unwrap_or((rest, ""));
        if !address.starts_with("rvn1")
            || address.len() < 12
            || !address.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(invalid("bad address"));
        }
        if key.len() < 16 || !key.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(invalid("bad key"));
        }
        if address == ME_ADDRESS {
            return Err(ApiError::new(
                codes::CARD_INVALID,
                "This is your own invite. Paste your friend's invite.",
                "mock: own address",
            ));
        }
        let lan = query
            .strip_prefix("lan=")
            .filter(|l| l.contains(':'))
            .map(str::to_string);
        Ok((address.to_string(), lan))
    }

    fn dispatch(&mut self, op: &str, args: &Value, now: u64) -> Result<Value, ApiError> {
        if let Some(r) = self.group_op(op, args, now) {
            return r;
        }
        match op {
            "app.info" => Ok(
                json!({ "version": env!("CARGO_PKG_VERSION"), "build": "dev", "platform": "linux" }),
            ),
            "profile.state" => to_value(&ProfileState {
                state: self.profile,
                reason: None,
            }),
            "identity.create" => {
                if self.profile != ProfileStateKind::New {
                    return Err(bad_request("this computer already has an identity"));
                }
                self.settings.display_name = args
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(Into::into);
                self.profile = ProfileStateKind::Ready;
                to_value(&NewIdentity {
                    address: ME_ADDRESS.into(),
                    words: self.me_words(),
                    old_code: ME_OLD_CODE.into(),
                })
            }
            "identity.me" => {
                self.require_ready()?;
                let (invite, _) = self.invite(false);
                to_value(&Me {
                    display_name: self.settings.display_name.clone(),
                    address: ME_ADDRESS.into(),
                    words: self.me_words(),
                    old_code: ME_OLD_CODE.into(),
                    card: self.card_text(&invite),
                    invite,
                })
            }
            "card.make" => {
                self.require_ready()?;
                let include = args
                    .get("include_address")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let (invite, address_included) = self.invite(include);
                to_value(&Card {
                    card: self.card_text(&invite),
                    invite,
                    address_included,
                })
            }
            "card.parse" => {
                self.require_ready()?;
                let (address, lan) = self.parse_invite(arg_str(args, "text")?)?;
                let mut routes = Vec::new();
                if lan.is_some() {
                    routes.push(Route {
                        kind: "lan".into(),
                        label: tr("net.lan").into(),
                        preview: false,
                    });
                }
                // The mock's invites carry no Internet or p2p route, so
                // nothing is left out.
                to_value(&ParsedCard {
                    short_address: short(&address),
                    has_address: lan.is_some(),
                    routes,
                    ignored_routes: Vec::new(),
                    note: None,
                })
            }
            "contacts.list" => {
                self.require_ready()?;
                let contacts: Vec<&Contact> = self.contacts.iter().map(|c| &c.c).collect();
                Ok(json!({ "contacts": contacts }))
            }
            "contacts.add" => {
                self.require_ready()?;
                let (address, lan) = self.parse_invite(arg_str(args, "text")?)?;
                let name = arg_str(args, "name")?.trim().to_string();
                if name.is_empty() {
                    return Err(ApiError::new(
                        codes::NAME_CLASH,
                        "Type a name for your friend.",
                        "mock: empty name",
                    ));
                }
                if self
                    .contacts
                    .iter()
                    .any(|c| c.c.name.eq_ignore_ascii_case(&name))
                {
                    return Err(ApiError::new(
                        codes::NAME_CLASH,
                        trf("add.name.clash", &[("name", &name)]),
                        "mock: name exists",
                    ));
                }
                if self.contacts.iter().any(|c| c.c.address == address) {
                    return Err(ApiError::new(
                        codes::CARD_INVALID,
                        "You already have this friend.",
                        "mock: address exists",
                    ));
                }
                let contact = Contact {
                    short_address: short(&address),
                    verified: false,
                    presence: if lan.is_some() {
                        Presence::NoRecent
                    } else {
                        Presence::NoAddress
                    },
                    last_contact_ms: None,
                    waiting: None,
                    avatar_index: theme::avatar_index(&address) as u8,
                    lan,
                    name,
                    address: address.clone(),
                    unread: 0,
                };
                self.contacts.push(MockContact {
                    words: words_from_address(&address),
                    old_code: None,
                    read_up_to: None,
                    c: contact.clone(),
                });
                Ok(json!({ "contact": contact }))
            }
            "contacts.rename" => {
                self.require_ready()?;
                let (address, name) = (
                    arg_str(args, "address")?,
                    arg_str(args, "name")?.trim().to_string(),
                );
                if self
                    .contacts
                    .iter()
                    .any(|c| c.c.address != address && c.c.name.eq_ignore_ascii_case(&name))
                {
                    return Err(ApiError::new(
                        codes::NAME_CLASH,
                        trf("add.name.clash", &[("name", &name)]),
                        "mock: name exists",
                    ));
                }
                let c = self.contact_mut(address).ok_or_else(not_a_contact)?;
                c.c.name = name;
                Ok(json!({ "contact": c.c }))
            }
            "contacts.remove" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?;
                let before = self.contacts.len();
                self.contacts.retain(|c| c.c.address != address);
                if self.contacts.len() == before {
                    return Err(not_a_contact());
                }
                self.messages.retain(|m| m.address != address);
                Ok(json!({}))
            }
            "contacts.set_address" => {
                self.require_ready()?;
                let (address, lan) = (arg_str(args, "address")?, arg_str(args, "lan")?.to_string());
                let c = self.contact_mut(address).ok_or_else(not_a_contact)?;
                c.c.lan = Some(lan);
                if c.c.presence == Presence::NoAddress {
                    c.c.presence = Presence::NoRecent;
                }
                Ok(json!({ "contact": c.c }))
            }
            "verify.check_words" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?;
                let typed: Vec<String> = args
                    .get("words")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let incomplete = |detail: String| {
                    ApiError::new(
                        codes::WORDS_INCOMPLETE,
                        tr("error.wordsIncomplete"),
                        format!("mock: {detail}"),
                    )
                };
                if typed.len() != 10 {
                    return Err(incomplete(format!("{} words", typed.len())));
                }
                let mut ix = [0u8; 10];
                for (i, w) in typed.iter().enumerate() {
                    ix[i] = resolve_word(w)
                        .ok_or_else(|| incomplete(format!("word {} is not a Raven word", i + 1)))?;
                }
                let c = self.contact_mut(address).ok_or_else(not_a_contact)?;
                let result = if !check_ok(&ix) {
                    CheckOutcome::Typo
                } else {
                    // Constant-time comparison (XOR/OR fold), as the design asks.
                    let diff = ix
                        .iter()
                        .zip(c.words.iter())
                        .fold(0u8, |acc, (a, b)| acc | (a ^ b));
                    if diff == 0 {
                        c.c.verified = true;
                        CheckOutcome::Match
                    } else {
                        CheckOutcome::Mismatch
                    }
                };
                Ok(json!({ "result": result }))
            }
            "verify.check_old_code" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?;
                let typed = words::normalise_old_code(arg_str(args, "code")?);
                if typed.is_empty() {
                    return Err(bad_request("empty old code"));
                }
                let c = self.contact_mut(address).ok_or_else(not_a_contact)?;
                // Case-sensitive, compared over the whole length (no early
                // exit), as the design asks for the old code.
                let result = match c.old_code.map(words::normalise_old_code) {
                    Some(expected) if expected.len() == typed.len() => {
                        let diff = expected
                            .bytes()
                            .zip(typed.bytes())
                            .fold(0u8, |acc, (a, b)| acc | (a ^ b));
                        if diff == 0 {
                            c.c.verified = true;
                            OldCodeOutcome::Match
                        } else {
                            OldCodeOutcome::Mismatch
                        }
                    }
                    _ => OldCodeOutcome::Mismatch,
                };
                Ok(json!({ "result": result }))
            }
            "verify.word_hint" => {
                let prefix = arg_str(args, "prefix")?;
                match resolve_word(prefix) {
                    Some(i) => Ok(json!({ "word": wordlist()[i as usize] })),
                    None => Ok(json!({})),
                }
            }
            "messages.history" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?;
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
                let mut list: Vec<&Message> = self
                    .messages
                    .iter()
                    .filter(|m| m.address == address)
                    .collect();
                list.sort_by_key(|m| m.at_ms);
                if let Some(before) = args.get("before").and_then(Value::as_str) {
                    if let Some(pos) = list.iter().position(|m| m.id == before) {
                        list.truncate(pos);
                    }
                }
                let skip = list.len().saturating_sub(limit);
                Ok(json!({ "messages": list[skip..] }))
            }
            "messages.send" => {
                self.require_ready()?;
                let (address, text) = (
                    arg_str(args, "address")?.to_string(),
                    arg_str(args, "text")?.to_string(),
                );
                if text.len() > MAX_TEXT {
                    return Err(ApiError::new(
                        codes::MESSAGE_TOO_LONG,
                        tr("error.messageTooLong"),
                        format!("mock: {} bytes", text.len()),
                    ));
                }
                if text.trim().is_empty() {
                    return Err(bad_request("empty message"));
                }
                let presence = self
                    .contacts
                    .iter()
                    .find(|c| c.c.address == address)
                    .ok_or_else(not_a_contact)?
                    .c
                    .presence;
                let id = self.new_id();
                let mut message = Message {
                    id: id.clone(),
                    address: address.clone(),
                    direction: Direction::Out,
                    text,
                    at_ms: now,
                    state: MessageState::Sending,
                    until_ms: None,
                    path: None,
                    path_kind: None,
                    delivered_at_ms: None,
                    attempts: Some(1),
                };
                if presence == Presence::NoAddress {
                    message.state = MessageState::NotSentMeetFirst;
                    self.messages.push(message.clone());
                    return Ok(json!({ "message": message }));
                }
                self.messages.push(message.clone());
                self.emit(now, EventKind::SendStarted { id: id.clone() });
                if presence == Presence::InTouch {
                    self.schedule(
                        now + 800,
                        Due::State {
                            id: id.clone(),
                            state: MessageState::Sent,
                        },
                    );
                    self.schedule(now + 1_000, Due::Receipt { id, ms: 1_000 });
                } else {
                    self.schedule(
                        now + 800,
                        Due::State {
                            id,
                            state: MessageState::Waiting,
                        },
                    );
                }
                Ok(json!({ "message": message }))
            }
            "messages.retry" => {
                self.require_ready()?;
                let id = arg_id(args)?;
                let m = self.message_mut(&id).ok_or_else(unknown_message)?;
                if m.state == MessageState::Waiting {
                    let attempts = m.attempts.unwrap_or(0) + 1;
                    m.attempts = Some(attempts);
                    let address = m.address.clone();
                    self.emit(now, EventKind::SendStarted { id: id.clone() });
                    self.emit(
                        now,
                        EventKind::Retry {
                            id: id.clone(),
                            address: address.clone(),
                            attempts,
                        },
                    );
                    let in_touch = self
                        .contacts
                        .iter()
                        .any(|c| c.c.address == address && c.c.presence == Presence::InTouch);
                    if in_touch {
                        self.schedule(
                            now + 700,
                            Due::Receipt {
                                id: id.clone(),
                                ms: 700,
                            },
                        );
                    }
                }
                let m = self.message_mut(&id).cloned();
                Ok(json!({ "message": m }))
            }
            "messages.mark_read" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?.to_string();
                let marked = self
                    .contact_mut(&address)
                    .ok_or_else(not_a_contact)?
                    .read_up_to
                    .clone();
                let theirs = self.chat(&address);
                let end = match args.get("id").and_then(Value::as_str) {
                    None => theirs.len().checked_sub(1),
                    Some(id) => Some(
                        theirs
                            .iter()
                            .position(|m| m.id == id)
                            .ok_or_else(unknown_message)?,
                    ),
                };
                let at = marked.and_then(|id| theirs.iter().rposition(|m| m.id == id));
                // Never back.
                if let Some(end) = end.filter(|end| at.is_none_or(|at| *end > at)) {
                    let id = theirs[end].id.clone();
                    if let Some(c) = self.contact_mut(&address) {
                        c.read_up_to = Some(id);
                    }
                }
                self.recount_unread(now, &address);
                Ok(json!({ "address": address, "unread": self.unread_of(&address) }))
            }
            "messages.stop" => {
                self.require_ready()?;
                let id = arg_id(args)?;
                let m = self.message_mut(&id).ok_or_else(unknown_message)?;
                if m.state == MessageState::Waiting {
                    m.state = MessageState::Failed;
                    m.until_ms = None;
                    let address = m.address.clone();
                    self.emit(
                        now,
                        EventKind::State {
                            id: id.clone(),
                            state: MessageState::Failed,
                        },
                    );
                    self.recount_waiting(&address);
                }
                let m = self.message_mut(&id).cloned();
                Ok(json!({ "message": m }))
            }
            "messages.details" => {
                self.require_ready()?;
                let id = arg_id(args)?;
                let m = self
                    .messages
                    .iter()
                    .find(|m| m.id == id)
                    .cloned()
                    .ok_or_else(unknown_message)?;
                let name = self
                    .contacts
                    .iter()
                    .find(|c| c.c.address == m.address)
                    .map(|c| c.c.name.clone())
                    .unwrap_or_default();
                let n = name.as_str();
                let (status_text, why_text, action_text) = match m.state {
                    MessageState::Waiting => {
                        let until = m.until_ms.unwrap_or(now);
                        (
                            trf(
                                "message.status.waiting",
                                &[
                                    ("name", n),
                                    ("time", &timefmt::deadline(now, until)),
                                    ("left", &timefmt::time_left(now, until)),
                                ],
                            ),
                            trf("message.why.unreachable", &[("name", n)]),
                            trf("message.action.wait", &[("name", n)]),
                        )
                    }
                    MessageState::Sent | MessageState::Sending => (
                        trf("message.status.sent", &[("name", n)]),
                        String::new(),
                        String::new(),
                    ),
                    MessageState::TimeRanOut => (
                        tr("message.timeRanOut").into(),
                        String::new(),
                        String::new(),
                    ),
                    MessageState::Delivered => {
                        (tr("tick.delivered").into(), String::new(), String::new())
                    }
                    MessageState::NotSentMeetFirst => (
                        trf("tick.notSent.meetFirst", &[("name", n)]),
                        String::new(),
                        String::new(),
                    ),
                    MessageState::Failed | MessageState::Unknown => {
                        (tr("tick.failed").into(), String::new(), String::new())
                    }
                };
                let detail = format!(
                    "id={} state={:?} attempts={} path={} at_ms={} until_ms={}",
                    m.id,
                    m.state,
                    m.attempts.unwrap_or(0),
                    m.path.as_deref().unwrap_or("-"),
                    m.at_ms,
                    m.until_ms
                        .map(|u| u.to_string())
                        .unwrap_or_else(|| "-".into())
                );
                to_value(&MessageDetails {
                    message: m,
                    status_text,
                    why_text,
                    action_text,
                    detail,
                })
            }
            "node.status" => {
                // A successful status call is a status check.
                if self.node.running {
                    self.node.last_ok_ms = now;
                }
                to_value(&self.node)
            }
            "node.start" => {
                self.require_ready()?;
                if !self.node.running {
                    self.node.running = true;
                    self.node.started_by_app = true;
                    self.node.pid = Some(MOCK_PID);
                    self.node.listening = None;
                    self.node.receive = self.settings.receive;
                    if let Some(lab) = &mut self.net.lab {
                        lab.started_with = lab.on.clone();
                    }
                    let status = self.node.clone();
                    self.emit(now, EventKind::Node { status });
                    if self.node.receive {
                        self.schedule(now + 1_500, Due::NodeListening);
                    }
                }
                to_value(&self.node)
            }
            "node.stop" => {
                let was_running = self.node.running;
                if was_running {
                    self.node.running = false;
                    self.node.started_by_app = false;
                    self.node.pid = None;
                    self.node.listening = None;
                    self.last_status_ok = 0;
                    if let Some(lab) = &mut self.net.lab {
                        lab.started_with.clear();
                        lab.locked = false;
                    }
                    let status = self.node.clone();
                    self.emit(now, EventKind::Node { status });
                }
                // What happened, as the engine says it (API.md, Node).
                let mut v = to_value(&self.node)?;
                v["stop"] = if was_running {
                    "stopped"
                } else {
                    "not_running"
                }
                .into();
                Ok(v)
            }
            "settings.get" => to_value(&self.settings),
            "settings.set" => {
                let s = &mut self.settings;
                if let Some(v) = args.get("display_name") {
                    s.display_name = v
                        .as_str()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .map(Into::into);
                }
                if let Some(v) = args.get("reduced_motion").and_then(Value::as_bool) {
                    s.reduced_motion = v;
                }
                if let Some(v) = args.get("when_quit") {
                    s.when_quit = serde_json::from_value(v.clone()).unwrap_or(s.when_quit);
                }
                if let Some(v) = args.get("keep_chats_days") {
                    s.keep_chats_days = v.as_u64().map(|d| d as u32);
                }
                if let Some(v) = args.get("forget_addresses").and_then(Value::as_bool) {
                    s.forget_addresses = v;
                }
                if let Some(v) = args.get("lock_on_sleep").and_then(Value::as_bool) {
                    s.lock_on_sleep = v;
                }
                if let Some(v) = args.get("receive").and_then(Value::as_bool) {
                    s.receive = v;
                    if self.node.receive != v {
                        self.node.receive = v;
                        if self.node.running {
                            self.node.listening = None;
                            if v {
                                self.schedule(now + 1_500, Due::NodeListening);
                            }
                            let status = self.node.clone();
                            self.emit(now, EventKind::Node { status });
                        }
                    }
                }
                to_value(&self.settings)
            }
            "events.poll" => {
                if self.profile == ProfileStateKind::Ready && !self.demo_started {
                    self.start_demo(now);
                }
                let after = args.get("after_seq").and_then(Value::as_u64).unwrap_or(0);
                let events: Vec<&Event> = self.events.iter().filter(|e| e.seq > after).collect();
                Ok(json!({ "seq": self.seq, "events": events }))
            }
            // The privacy pack is not in this release: the state says so and
            // every privacy action answers NOT_AVAILABLE_YET (API.md).
            "privacy.state" => to_value(&PrivacyState {
                available: false,
                encrypted: false,
                locked: self.profile == ProfileStateKind::Locked,
            }),
            "privacy.lock" | "privacy.unlock" | "privacy.encrypt" => Err(not_available_yet(op)),
            "privacy.erase" => {
                if args.get("confirm").and_then(Value::as_str) != Some("erase") {
                    return Err(bad_request("privacy.erase needs confirm: \"erase\""));
                }
                Err(not_available_yet(op))
            }
            "network.status" => {
                self.require_ready()?;
                Ok(self.network_status(now))
            }
            "network.set" => {
                self.require_ready()?;
                self.network_set(args, now)
            }
            "network.check" => {
                self.require_ready()?;
                Ok(self.network_check(args))
            }
            "contacts.routes" => {
                self.require_ready()?;
                let address = arg_str(args, "address")?.to_string();
                self.contact_routes(&address, now)
            }
            "contacts.set_routes" => {
                self.require_ready()?;
                self.set_contact_routes(args, now)
            }
            "security.status" => Ok(self.security_status()),
            "lock.state" => Ok(self.lock_state()),
            "lock.now" => self.lock_now(op),
            "lock.unlock" => self.lock_unlock(op, args, now),
            "updates.status" => Ok(self.updates_status()),
            other => Err(bad_request(format!("unknown operation `{other}`"))),
        }
    }

    // ------------------------------------------------------------ network
    //
    // A release build's answers by default (API.md, Mocks): LAN direct on,
    // every other way held (preview), Bluetooth nearby `held`; a held way is
    // never turned on. With `RAVEN_MOCK_NETWORK=lab` a developer build's
    // with lab switches set (`MockLab`): the ways those switches open are
    // `lab` (or `off` with `lab: true` until switched on).

    /// One way of `network.status` (API.md `Carrier`).
    fn way_json(&self, id: &str, name: &str) -> Value {
        let mut w = json!({
            "id": id, "name": name, "lab": false,
            "gate": if id == "tor" {
                "ONION_PRODUCTION_ENABLED".to_owned()
            } else {
                format!("{}_PRODUCTION_ENABLED", id.to_uppercase())
            },
            "lab_switches": [], "can_set": MOCK_SETTABLE.contains(&id),
        });
        let up = self.node.running;
        let lab = self
            .net
            .lab
            .as_ref()
            .filter(|_| MOCK_LAB_WAYS.contains(&id));
        let (state, reason, setting, running): (&str, &str, Option<bool>, Option<bool>) =
            if id == "lan" {
                w["detail"] = format!(
                    "LAN direct: {}",
                    if self.net.lan_off {
                        "off (raven lan on turns it on)"
                    } else {
                        "on"
                    }
                )
                .into();
                if self.net.lan_off {
                    (
                        "off",
                        "Turned off on this computer: Raven doesn't send or receive on this \
                         network until it is turned on again.",
                        Some(false),
                        up.then_some(false),
                    )
                } else {
                    (
                        "on",
                        "On. Works without the internet.",
                        Some(true),
                        up.then_some(true),
                    )
                }
            } else if let Some(lab) = lab {
                w["lab"] = true.into();
                w["lab_switches"] = json!(mock_lab_switches_of(id));
                match mock_way_setting(id) {
                    _ if id == "nearby" && !lab.bluetooth => (
                        "not_on_this_os",
                        "This computer can't use Bluetooth nearby (no Bluetooth LE support for \
                         it).",
                        Some(lab.is_on("nearby")),
                        None,
                    ),
                    Some(key) => {
                        let on = lab.is_on(key);
                        // Nearby: the adapter shows its fixed address, so the
                        // background program keeps Bluetooth off (BlueZ
                        // without `Privacy = device`).
                        let in_use = on
                            && lab.in_use(key)
                            && id != "nearby"
                            && (id != "port_mapping" || lab.in_use("p2p"));
                        let running = up.then_some(in_use);
                        if on {
                            ("lab", MOCK_LAB_REASON, Some(true), running)
                        } else if id == "custody" {
                            (
                                "off",
                                "Raven doesn't leave your messages with friends or collect mail \
                                 from them.",
                                Some(false),
                                running,
                            )
                        } else {
                            ("off", "Turned off.", Some(false), running)
                        }
                    }
                    // Tor (no switch): its state as the background program
                    // reports it, ready in the lab mock.
                    None if id == "tor" && up => {
                        w["detail"] = MOCK_TOR_WORDS.into();
                        ("lab", "Ready.", None, Some(true))
                    }
                    None => ("lab", MOCK_LAB_REASON, None, up.then_some(true)),
                }
            } else {
                let setting = mock_way_setting(id)
                    .filter(|_| MOCK_SETTABLE.contains(&id))
                    .map(|_| false);
                (
                    "preview",
                    "Not in this version of Raven yet.",
                    setting,
                    None,
                )
            };
        w["state"] = state.into();
        w["reason"] = reason.into();
        if let Some(on) = setting {
            w["setting"] = on.into();
        }
        if let Some(r) = running {
            w["running"] = r.into();
        }
        w
    }

    fn network_status(&self, now: u64) -> Value {
        let lab = self.net.lab.as_ref();
        let up = self.node.running;
        let is_on = |k: &str| lab.is_some_and(|l| l.is_on(k));
        let in_use = |k: &str| up && lab.is_some_and(|l| l.in_use(k));
        let decided = |k: &str| lab.is_some_and(|l| l.decided.contains(&k));
        let mut carriers = vec![self.way_json("lan", "Same network (Wi-Fi, cable or hotspot)")];
        for (id, name) in MOCK_HELD_WAYS {
            carriers.push(self.way_json(id, name));
        }
        let protections: Vec<Value> = MOCK_PROTECTIONS
            .iter()
            .map(|(id, name)| self.way_json(id, name))
            .collect();
        let mut listening: Vec<String> = self.node.listening.iter().cloned().collect();
        if in_use("internet") {
            listening.push(MOCK_INTERNET_LISTEN.into());
        }
        let relays = if in_use("p2p") {
            json!([
                {"relay": MOCK_RELAY, "state": "reserved", "source": "manual"},
                {"relay": MOCK_FRIEND_RELAY, "state": "reserved", "source": "offer",
                 "friend_address": ALI}
            ])
        } else {
            json!([])
        };
        let port_mapping = match lab {
            None => json!({"setting": "unset", "router": "", "nat_pmp_pcp": "preview"}),
            Some(_) => {
                let mapped = in_use("port_mapping") && in_use("p2p");
                let setting = if is_on("port_mapping") {
                    "on"
                } else if decided("port_mapping") {
                    "off"
                } else {
                    "unset"
                };
                let router = if mapped {
                    "mapped 7423"
                } else if in_use("p2p") {
                    "off"
                } else {
                    ""
                };
                let mut v = json!({"setting": setting, "router": router, "nat_pmp_pcp": "lab"});
                if mapped {
                    v["v4"] = "upnp".into();
                    v["v4_port"] = 7423.into();
                    v["v6"] = "none".into();
                }
                v
            }
        };
        let proxy = json!({
            "setting": lab.map_or("auto", |l| l.proxy.as_str()), "in_use": "", "lines": []
        });
        let nearby = match lab {
            Some(l) if l.bluetooth => json!({
                "state": if up { "live" } else { "not_running" },
                "saved": decided("nearby").then(|| is_on("nearby")),
                "on_this_os": true, "enabled": is_on("nearby"), "radio": "powered_on",
                "address_privacy": "public", "in_range": 0, "connected": 0,
                "rows": [{"label": "radio", "value": "powered on"},
                         {"label": "address", "value": "public (Privacy = device is off)"}]
            }),
            Some(_) => json!({
                "state": "held", "saved": null, "on_this_os": false, "radio": "unsupported",
                "rows": []
            }),
            None => json!({
                "state": "held", "saved": null, "on_this_os": cfg!(target_env = "gnu"), "rows": []
            }),
        };
        let mine = if is_on("friends_carry_mine") {
            "on"
        } else if decided("friends_carry_mine") {
            "off"
        } else {
            "not_decided"
        };
        let listed = self.net.routes.iter().filter(|r| !r.3.is_empty()).count();
        let carrying = is_on("carry_for_friends");
        let handoff = is_on("handoff");
        let lending = is_on("relay_for_friends");
        let offering = u32::from(lab.is_some());
        let carry = json!({
            "friends_carry_mine": {"setting": mine,
                "mail_friends": u32::from(mine == "on"),
                "chosen_by_hand": false, "offering": 2 * offering,
                "offering_over_internet": offering,
                "offering_reachable": if up { offering } else { 0 },
                "listed_by_hand": listed},
            "carry_for_friends": {"on": carrying,
                "held_messages": if carrying && up { 3 } else { 0 },
                "held_bytes": if carrying && up { 2_048 } else { 0 },
                "handoff": handoff,
                "handoff_lines": if handoff { vec!["passed on: 0 · held for others: 0"] } else { vec![] }},
            "letters_left": [],
            "relay_for_friends": {"setting": lending, "offering": lending && up,
                "offered_to": if lending && up { 2 } else { 0 }}
        });
        let (bridge, store, relay, automatic) = self.net.flags;
        let mut v = json!({
            "build": "dev",
            "node": if up { "running" } else { "not_running" },
            "carriers": carriers,
            "protections": protections,
            "listening": listening,
            "port": PORT.parse::<u16>().unwrap_or(7420),
            "port_mapping": port_mapping,
            "relays": relays,
            "proxy": proxy,
            "nearby": nearby,
            "carry": carry,
            "help": {"bridge": bridge, "store": store, "relay": relay, "automatic": automatic,
                     "effects": {"bridge": MOCK_EFFECTS[0], "store": MOCK_EFFECTS[1],
                                 "relay": MOCK_EFFECTS[2]}},
            "hide_ip": self.net.hide_ip,
            "lab_switches": if lab.is_some() { MOCK_LAB_SWITCHES.to_vec() } else { Vec::new() },
            "texts": {"carry_for_friends": MOCK_CONSENT_TEXTS[0],
                      "friends_carry_mine": MOCK_CONSENT_TEXTS[1],
                      "handoff": MOCK_CONSENT_TEXTS[2]}
        });
        // Tor (lab): raven-node's Status has its line only while it runs.
        if lab.is_some() && up {
            v["tor"] = json!({"phase": "ready", "words": MOCK_TOR_WORDS,
                              "outages": MOCK_TOR_OUTAGES});
        }
        if is_on("internet") {
            v["internet_listen"] = MOCK_INTERNET_LISTEN.into();
        }
        if is_on("p2p") {
            v["p2p_listen"] = "7423".into();
        }
        if in_use("p2p") {
            v["p2p_addresses"] = json!([MOCK_P2P_ADDRESS]);
        }
        if let Some(until) = self.net.lan_until_ms.filter(|u| *u > now) {
            v["lan_until_ms"] = until.into();
        }
        v
    }

    fn network_set(&mut self, args: &Value, now: u64) -> Result<Value, ApiError> {
        let given: Vec<(&'static str, Value)> = MOCK_SETTINGS
            .iter()
            .filter_map(|k| {
                args.get(*k)
                    .filter(|v| !v.is_null())
                    .map(|v| (*k, v.clone()))
            })
            .collect();
        if given.is_empty() {
            return Err(bad_request("network.set needs at least one setting"));
        }
        let agreed = args.get("agreed").and_then(Value::as_bool).unwrap_or(false);
        // Everything is checked first: a held way turned on, or a setting
        // turned on without the person's yes, changes nothing.
        for (key, value) in &given {
            let on = match *key {
                "proxy" => value
                    .as_str()
                    .ok_or_else(|| bad_request("\"proxy\" must be auto, off or http://host:port"))?
                    .trim()
                    .ne("off"),
                _ => value
                    .as_bool()
                    .ok_or_else(|| bad_request(format!("\"{key}\" must be true or false")))?,
            };
            if !on {
                continue;
            }
            let open = matches!(*key, "lan" | "bridge" | "store" | "relay" | "hide_ip")
                || (self.net.lab.is_some() && MOCK_LAB_SETTINGS.contains(key));
            if !open {
                return Err(not_in_this_version(&format!("network.set {key}")));
            }
            if MOCK_CONSENT_SETTINGS.contains(key) && !agreed {
                return Err(ApiError::new(
                    codes::CONSENT_NEEDED,
                    tr("error.consentNeeded"),
                    format!(
                        "mock: {key}: show network.status texts.{key} and pass \"agreed\": true"
                    ),
                ));
            }
        }
        let mut changes = Vec::new();
        let mut restart = false;
        for (key, value) in given {
            let on = value.as_bool().unwrap_or(false);
            let meeting = args
                .get("lan_for")
                .and_then(Value::as_str)
                .filter(|_| key == "lan" && on);
            let effect: String = match key {
                "lan" => {
                    self.net.lan_off = !on;
                    self.net.lan_until_ms =
                        meeting.and_then(mock_minutes).map(|m| now + m * 60_000);
                    match (on, meeting) {
                        (true, Some(t)) => format!(
                            "Raven sends and receives on this network for {t}, then turns it off \
                             by itself."
                        ),
                        (true, None) => "Raven sends and receives on this network again.".into(),
                        (false, _) => "Raven stops sending and receiving on this network.".into(),
                    }
                }
                "bridge" => {
                    self.net.flags.0 = on;
                    self.net.flags.3 = false;
                    MOCK_EFFECTS[0].into()
                }
                "store" => {
                    self.net.flags.1 = on;
                    self.net.flags.3 = false;
                    MOCK_EFFECTS[1].into()
                }
                "relay" => {
                    self.net.flags.2 = on;
                    self.net.flags.3 = false;
                    MOCK_EFFECTS[2].into()
                }
                "hide_ip" => {
                    self.net.hide_ip = on;
                    if on {
                        "Your contacts no longer learn this computer's IP address from Raven."
                            .into()
                    } else {
                        "Your contacts may reach you directly again.".into()
                    }
                }
                "proxy" => {
                    let text = value.as_str().unwrap_or("auto").trim().to_string();
                    if let Some(lab) = &mut self.net.lab {
                        lab.proxy = text;
                    }
                    "Raven dials relays directly, never through a web proxy.".into()
                }
                other => {
                    if let Some(lab) = &mut self.net.lab {
                        lab.on.retain(|k| *k != other);
                        if on {
                            lab.on.push(other);
                        }
                        if !lab.decided.contains(&other) {
                            lab.decided.push(other);
                        }
                    }
                    mock_effect(other, on).into()
                }
            };
            let applies = if RESTART_SETTINGS.contains(&key) {
                restart = true;
                "restart"
            } else {
                "now"
            };
            changes.push(json!({"setting": key, "value": value, "effect": effect,
                                "applies": applies, "detail": []}));
        }
        Ok(json!({
            "changes": changes,
            "restart_needed": restart && self.node.running,
            "status": self.network_status(now)
        }))
    }

    fn network_check(&self, args: &Value) -> Value {
        let lan = if self.net.lan_off {
            "off (raven lan on turns it back on)"
        } else {
            "on"
        };
        let p2p_up = self.node.running && self.net.lab.as_ref().is_some_and(|l| l.in_use("p2p"));
        let mut v = json!({
            "summary": [
                {"key": "identity", "label": "identity", "value": "OK"},
                {"key": "raven_node", "label": "raven-node",
                 "value": if self.node.running { "running; you can send and receive" } else { "NOT running" }},
                {"key": "lan", "label": "LAN", "value": lan},
                {"key": "contacts", "label": "contacts", "value": self.contacts.len().to_string()},
                {"key": "next_step", "label": "next step", "value": "nothing to do"}
            ],
            "next_step": "nothing to do",
            "rows": [
                {"key": "receiving", "value": if self.node.running { "yes" } else { "not_running" },
                 "plain": if self.node.running { "Raven receives messages on this network." }
                          else { "Raven's background program isn't running." }},
                if p2p_up {
                    json!({"key": "reach_class", "value": "relayed (2 relays reserved)"})
                } else {
                    json!({"key": "reach_class", "value": "off (p2p host not running)",
                           "plain": "Peer-to-peer isn't running, so only direct ways are used."})
                },
                {"key": "policy", "value": format!("bridge={} store={} relay={}",
                    self.net.flags.0, self.net.flags.1, self.net.flags.2)}
            ]
        });
        if args.get("router").and_then(Value::as_bool) == Some(true) {
            v["router"] = json!([
                {"key": "router_gateway_v4", "value": "found"},
                {"key": "router_gateway_v4", "value": "192.168.1.1"},
                {"key": "router_natpmp", "value": "no", "plain": "Your router didn't answer NAT-PMP."},
                {"key": "router_upnp_igd", "value": "none", "plain": "No UPnP router answered."}
            ]);
        }
        v
    }

    fn mock_routes(&self, address: &str) -> (Option<String>, Option<String>, Vec<String>, bool) {
        self.net
            .routes
            .iter()
            .find(|r| r.0 == address)
            .map(|r| (r.1.clone(), r.2.clone(), r.3.clone(), r.4))
            .unwrap_or_default()
    }

    fn contact_routes(&self, address: &str, now: u64) -> Result<Value, ApiError> {
        let c = self
            .contacts
            .iter()
            .find(|c| c.c.address == address)
            .ok_or_else(not_a_contact)?;
        let lab = self.net.lab.is_some();
        let gated = if lab { "lab" } else { "preview" };
        let (internet, p2p, listed, hide_ip) = self.mock_routes(address);
        let mut routes = Vec::new();
        if let Some(lan) = &c.c.lan {
            routes.push(
                json!({"kind": "lan", "label": "Same network (Wi-Fi, cable or hotspot)",
                               "value": lan, "via": [], "state": "on"}),
            );
        }
        if let Some(inet) = internet {
            routes.push(json!({"kind": "internet", "label": "Internet direct",
                               "value": inet, "via": [], "state": gated}));
        }
        if let Some(id) = p2p {
            let via: Vec<&str> = if lab { vec![MOCK_RELAY] } else { Vec::new() };
            routes.push(json!({"kind": "p2p", "label": "Peer-to-peer and relays",
                               "value": id, "via": via, "state": gated}));
        }
        if c.c.verified {
            routes.push(json!({"kind": "nearby", "label": "Bluetooth nearby",
                               "value": "", "via": [], "state": gated}));
        }
        let name_of = |a: &str| {
            self.contacts
                .iter()
                .find(|f| f.c.address == a)
                .map(|f| f.c.name.clone())
        };
        let listed: Vec<Value> = listed
            .iter()
            .map(|a| json!({"address": a, "name": name_of(a), "usable": true}))
            .collect();
        // In the lab build Ali told this computer about a mail friend
        // (Mina) and its own routes.
        let told_by_ali = lab && address == ALI;
        let told: Vec<Value> = if told_by_ali {
            vec![json!({"address": MINA, "name": name_of(MINA), "takes_handoffs": true})]
        } else {
            Vec::new()
        };
        let mut carriers = json!({"hints": gated, "told": told,
                                  "told_unknown": u32::from(told_by_ali),
                                  "listed": listed, "receipts_stranded": false});
        if told_by_ali {
            carriers["coverage"] =
                json!({"kind": "mutual_friend", "name": "Mina", "until": "for up to 24 hours"});
        } else if c.c.verified {
            carriers["coverage"] = json!({"kind": "none", "name": "nobody can carry it",
                                          "until": "it goes when you meet"});
        }
        // How long a message written now could wait (a release build: until
        // the secure connection with a verified friend ends; a friend not
        // verified was never connected with: a first message).
        carriers["away"] = if c.c.verified {
            json!({"until_ms": now + 20 * 3_600_000})
        } else {
            json!({"meet_first": true, "first_message": true})
        };
        let mut v = json!({"address": address, "verified": c.c.verified, "hide_ip": hide_ip,
                           "routes": routes, "carriers": carriers});
        if told_by_ali {
            v["learned"] = json!({"relays": 1, "direct": 1, "mail_friends": 1,
                                  "peer_id": MOCK_ALI_PEER_ID, "peer_id_proven": true,
                                  "lan_port": 7420, "expires_at_ms": now + 7 * 24 * 3_600_000});
        }
        Ok(v)
    }

    fn set_contact_routes(&mut self, args: &Value, now: u64) -> Result<Value, ApiError> {
        let address = arg_str(args, "address")?.to_string();
        if !self.contacts.iter().any(|c| c.c.address == address) {
            return Err(not_a_contact());
        }
        let (mut internet, mut p2p, mut listed, mut hide_ip) = self.mock_routes(&address);
        let mut clear_lan = false;
        if let Some(v) = args.get("internet").and_then(Value::as_str) {
            if !v.contains(':') {
                return Err(bad_request("internet must be host:port"));
            }
            internet = Some(v.to_string());
        }
        if let Some(v) = args.get("p2p").and_then(Value::as_str) {
            p2p = Some(v.to_string());
        }
        for c in args
            .get("clear")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            match c.as_str() {
                Some("internet") => internet = None,
                Some("p2p") => p2p = None,
                Some("lan") => clear_lan = true,
                _ => {}
            }
        }
        if let Some(list) = args.get("custodians").and_then(Value::as_array) {
            let picked: Vec<String> = list
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            for a in &picked {
                let ok = self
                    .contacts
                    .iter()
                    .any(|f| &f.c.address == a && f.c.verified && *a != address);
                if !ok || picked.len() > 2 {
                    return Err(bad_request(
                        "mail friends must be up to two verified friends",
                    ));
                }
            }
            listed = picked;
        }
        if let Some(v) = args.get("hide_ip").and_then(Value::as_bool) {
            hide_ip = v;
        }
        if clear_lan {
            if let Some(c) = self.contact_mut(&address) {
                c.c.lan = None;
            }
        }
        self.net.routes.retain(|r| r.0 != address);
        self.net
            .routes
            .push((address.clone(), internet, p2p, listed, hide_ip));
        let contact = self
            .contacts
            .iter()
            .find(|c| c.c.address == address)
            .map(|c| c.c.clone())
            .ok_or_else(not_a_contact)?;
        Ok(json!({"contact": contact, "routes": self.contact_routes(&address, now)?}))
    }

    // ------------------------------------------------------------ security, Lock, updates

    fn security_status(&self) -> Value {
        if self.net.lab.is_none() {
            return json!({"available": false, "lab": false, "kept": "off", "notices": [],
                          "counted": 0});
        }
        let mut v = json!({
            "available": true, "lab": true, "kept": "live", "counted": 4,
            "notices": [{
                "id": 1, "level": "warning", "address": RENA, "name": "Rena",
                "headline": "A message from Rena failed its check",
                "what_raven_did": "Raven dropped it; nothing reached your chats.",
                "what_to_do": "Nothing, unless it happens again: then compare security words \
                               with Rena.",
                "when": "today 10:12",
                "details": [{"label": "check", "value": "signature"},
                            {"label": "count", "value": "1"}]
            }]
        });
        if self.node.running {
            v["node"] = json!({"strangers_refused_1h": 2, "failed_handshakes_1h": 0,
                               "bans_active": 0, "link_tag_failures_24h": 0, "alerts_open": 0});
        }
        v
    }

    fn lock_state(&self) -> Value {
        match &self.net.lab {
            None => json!({
                "available": false, "lab": false, "supported": false,
                "locked": self.profile == ProfileStateKind::Locked
            }),
            Some(l) => json!({
                "available": true, "lab": true, "supported": self.node.running,
                "locked": l.locked && self.node.running, "has_passphrase": true
            }),
        }
    }

    fn not_served() -> ApiError {
        ApiError::new(
            codes::NOT_AVAILABLE_YET,
            tr("lock.notSupported"),
            "mock: no raven-node serves Lock v2; `raven lock` stops it instead",
        )
    }

    fn lock_now(&mut self, op: &str) -> Result<Value, ApiError> {
        let up = self.node.running;
        match &mut self.net.lab {
            None => return Err(not_in_this_version(op)),
            Some(_) if !up => return Err(Self::not_served()),
            Some(l) => l.locked = true,
        }
        Ok(self.lock_state())
    }

    fn lock_unlock(&mut self, op: &str, args: &Value, now: u64) -> Result<Value, ApiError> {
        let up = self.node.running;
        let passphrase = args.get("passphrase").and_then(Value::as_str).unwrap_or("");
        let note = {
            let Some(l) = &mut self.net.lab else {
                return Err(not_in_this_version(op));
            };
            if !up {
                return Err(Self::not_served());
            }
            if !l.locked {
                "Raven wasn't locked."
            } else if now < l.wait_until_ms {
                return Err(ApiError::new(
                    codes::UNLOCK_WAIT,
                    tr("error.unlockWait"),
                    "mock: three wrong passphrases; wait 30 s",
                ));
            } else if passphrase.is_empty() {
                return Err(ApiError::new(
                    codes::PASSPHRASE_NEEDED,
                    tr("error.passphraseNeeded"),
                    "mock: this profile has a passphrase",
                ));
            } else if passphrase != MOCK_PASSPHRASE {
                l.wrong_passphrases += 1;
                if l.wrong_passphrases >= 3 {
                    l.wrong_passphrases = 0;
                    l.wait_until_ms = now + 30_000;
                    return Err(ApiError::new(
                        codes::UNLOCK_WAIT,
                        tr("error.unlockWait"),
                        "mock: three wrong passphrases; wait 30 s",
                    ));
                }
                return Err(ApiError::new(
                    codes::PASSPHRASE_WRONG,
                    tr("error.passphraseWrong"),
                    "mock: wrong passphrase",
                ));
            } else {
                l.locked = false;
                l.wrong_passphrases = 0;
                "Unlocked: Raven's background program has its keys again."
            }
        };
        let mut v = self.lock_state();
        v["note"] = note.into();
        Ok(v)
    }

    fn updates_status(&self) -> Value {
        let install_text = "Install updates with `raven update install` in a terminal. The app \
                            shows what is ready; it doesn't install yet.";
        if self.net.lab.is_none() {
            return json!({
                "available": false, "bundles": false, "from_friends": false, "lab": false,
                "lines": ["Raven updates are not part of this version."],
                "install": "cli_only", "install_text": install_text
            });
        }
        json!({
            "available": true, "bundles": false, "from_friends": true, "lab": true,
            "lines": ["No update is waiting.", "Updates passed on by friends: on (lab)."],
            "summary": "updates: none waiting",
            "install": "cli_only", "install_text": install_text
        })
    }
}

/// The ways the mock holds, as a release build does.
const MOCK_HELD_WAYS: [(&str, &str); 13] = [
    ("lan_beacon", "Finding friends on a new network"),
    ("internet", "Internet direct"),
    ("port_mapping", "Router port opening (UPnP, NAT-PMP, PCP)"),
    ("p2p", "Peer-to-peer and relays"),
    ("friend_relay", "Relay for friends"),
    ("websocket", "Relays over WebSocket (port 443)"),
    ("proxy", "Web proxy for relays"),
    ("custody", "Friends carry your messages"),
    ("letters", "Sealed letters with friends"),
    ("handoff", "Passing messages on between friends"),
    ("route_hints", "Routes friends tell each other"),
    ("nearby", "Bluetooth nearby"),
    ("tor", "Tor"),
];

const MOCK_PROTECTIONS: [(&str, &str); 6] = [
    ("random_port", "A port of its own for this profile"),
    ("link_knock", "Silent to strangers (link knock)"),
    ("link_padding", "Padding that hides message sizes"),
    ("lock_v2", "Lock while running"),
    ("security_events", "Security notices"),
    ("updates_from_friends", "Updates passed on by friends"),
];

const MOCK_SETTABLE: [&str; 9] = [
    "lan",
    "internet",
    "port_mapping",
    "p2p",
    "friend_relay",
    "proxy",
    "custody",
    "handoff",
    "nearby",
];

const MOCK_SETTINGS: [&str; 14] = [
    "lan",
    "internet",
    "p2p",
    "port_mapping",
    "proxy",
    "nearby",
    "carry_for_friends",
    "friends_carry_mine",
    "handoff",
    "relay_for_friends",
    "bridge",
    "store",
    "relay",
    "hide_ip",
];

/// The ways the lab mock's switches open (the rest stay `preview`).
const MOCK_LAB_WAYS: [&str; 14] = [
    "lan_beacon",
    "internet",
    "port_mapping",
    "p2p",
    "friend_relay",
    "custody",
    "handoff",
    "route_hints",
    "nearby",
    "tor",
    "random_port",
    "lock_v2",
    "security_events",
    "updates_from_friends",
];

/// The `RAVEN_LAB_*` switches the lab mock says it sees (names only).
const MOCK_LAB_SWITCHES: [&str; 11] = [
    "RAVEN_LAB_TEST_A",
    "RAVEN_LAB_LAN_BEACON",
    "RAVEN_LAB_FRIEND_RELAY",
    "RAVEN_LAB_CUSTODY_HANDOFF",
    "RAVEN_LAB_ROUTE_HINTS",
    "RAVEN_LAB_NEARBY",
    "RAVEN_LAB_TOR",
    "RAVEN_LAB_RANDOM_PORT",
    "RAVEN_LAB_LOCAL_PROTECTION",
    "RAVEN_LAB_SECURITY_EVENTS",
    "RAVEN_LAB_MESH_UPDATE",
];

/// The settings the lab mock's switches let `network.set` turn on.
const MOCK_LAB_SETTINGS: [&str; 8] = [
    "internet",
    "p2p",
    "port_mapping",
    "nearby",
    "carry_for_friends",
    "friends_carry_mine",
    "handoff",
    "relay_for_friends",
];

/// Turned on, these need the person's yes (`agreed: true`).
const MOCK_CONSENT_SETTINGS: [&str; 3] = ["carry_for_friends", "friends_carry_mine", "handoff"];

/// These apply when the background program starts again.
const RESTART_SETTINGS: [&str; 4] = ["internet", "p2p", "port_mapping", "proxy"];

/// The lab mock's Tor line after "Tor: " (states only, as raven-node says it).
const MOCK_TOR_WORDS: &str = "ready (onion service outages since start: 1, the last lasted 2 min \
     and ended 1 h ago)";
const MOCK_TOR_OUTAGES: &str =
    "onion service outages since start: 1, the last lasted 2 min and ended 1 h ago";

const MOCK_LAB_REASON: &str = "On for testing only: a developer build's lab switch turned it on.";
const MOCK_INTERNET_LISTEN: &str = "0.0.0.0:7422";
const MOCK_RELAY: &str = "/ip4/203.0.113.7/tcp/4001/p2p/12D3KooWRelayDemo";
const MOCK_FRIEND_RELAY: &str = "/ip4/192.168.1.31/tcp/7423/p2p/12D3KooWAliDemo";
const MOCK_P2P_ADDRESS: &str = "/ip4/192.168.1.23/tcp/7423/p2p/12D3KooWAliceDemo";
const MOCK_ALI_PEER_ID: &str = "12D3KooWAliDemo";

/// The way a `network.set` setting belongs to.
fn mock_way_setting(id: &str) -> Option<&'static str> {
    match id {
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

/// The lab switches that open a way in the lab mock.
fn mock_lab_switches_of(id: &str) -> Vec<&'static str> {
    match id {
        "lan_beacon" => vec!["RAVEN_LAB_LAN_BEACON"],
        "internet" | "p2p" | "port_mapping" | "custody" => vec!["RAVEN_LAB_TEST_A"],
        "friend_relay" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_FRIEND_RELAY"],
        "handoff" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_CUSTODY_HANDOFF"],
        "route_hints" => vec!["RAVEN_LAB_ROUTE_HINTS"],
        "nearby" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_NEARBY"],
        "tor" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_TOR"],
        "random_port" => vec!["RAVEN_LAB_RANDOM_PORT"],
        "lock_v2" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_LOCAL_PROTECTION"],
        "security_events" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_SECURITY_EVENTS"],
        "updates_from_friends" => vec!["RAVEN_LAB_TEST_A", "RAVEN_LAB_MESH_UPDATE"],
        _ => Vec::new(),
    }
}

/// `raven lan on --for`'s time in minutes (`30m`, `1h` … `24h`).
fn mock_minutes(text: &str) -> Option<u64> {
    let t = text.trim().to_ascii_lowercase();
    let (n, per) = match t.strip_suffix('m') {
        Some(n) => (n.to_string(), 1),
        None => (t.strip_suffix('h')?.to_string(), 60),
    };
    n.parse::<u64>()
        .ok()
        .map(|n| n * per)
        .filter(|m| (1..=24 * 60).contains(m))
}

/// What a change does, in the engine's words.
fn mock_effect(setting: &str, on: bool) -> &'static str {
    match (setting, on) {
        ("internet", true) => {
            "Raven listens for friends over the internet on 0.0.0.0:7422 once its background \
             program restarts. Only your verified friends get an answer, but anyone who scans \
             that port can see that something listens there."
        }
        ("internet", false) => {
            "Raven closes its internet port when its background program restarts."
        }
        ("p2p", true) => {
            "Raven reaches friends across networks, directly or through relays, once its \
             background program restarts. The relays you use learn your network ID, your IP \
             address and when you talk."
        }
        ("p2p", false) => {
            "Raven stops using peer-to-peer and relays when its background program restarts."
        }
        ("port_mapping", true) => {
            "Raven asks your router to open its peer-to-peer port (UPnP, NAT-PMP or PCP) when \
             its background program restarts. Your router may not support it."
        }
        ("port_mapping", false) => "Raven won't ask your router to open a port.",
        ("nearby", true) => {
            "Raven may use Bluetooth to reach your verified friends when they are in range. It \
             advertises only while it has something to send, under a random name only they can \
             recognise."
        }
        ("nearby", false) => "Raven stops using Bluetooth.",
        ("carry_for_friends", true) => {
            "This computer holds sealed messages for your verified friends while they are \
             offline. It can't read them, but it sees who wrote to whom, and when."
        }
        ("carry_for_friends", false) => {
            "This computer stops taking messages for friends. What it holds stays until it is \
             collected or expires."
        }
        ("friends_carry_mine", true) => {
            "When a friend can't be reached, Raven may leave a sealed copy with a friend you both \
             verified, and collect mail friends hold for you."
        }
        ("friends_carry_mine", false) => {
            "Raven doesn't leave your messages with friends or collect mail from them."
        }
        ("handoff", true) => {
            "This computer may pass sealed messages on between your verified friends, and hold \
             ones a friend passes on. It can't read them."
        }
        ("handoff", false) => {
            "New handoffs are refused; what this computer already took on is still passed on or \
             kept until it expires."
        }
        ("relay_for_friends", true) => {
            "This computer lends a relay to your verified friends, so friends behind other \
             routers can reach each other through it."
        }
        ("relay_for_friends", false) => "This computer stops relaying for friends.",
        _ => "Turned off.",
    }
}

/// What the three settings that need a yes mean, as the CLI asks it (the
/// mailbox question ends with the terminal's prompt, as the real text does).
const MOCK_CONSENT_TEXTS: [&str; 3] = [
    "Hold sealed messages for your friends while they are offline?\n\
     - Only between two people who are both your verified contacts.\n\
     - What this computer keeps for them is sealed: you can't read it.\n\
     - It does learn, for every message it holds, who sent it and who it is for\n  \
     (both Raven identities), when, how big, and when it was collected.",
    "Use friends as mailboxes?\n\
     When a contact cannot be reached, raven-node can leave a sealed copy of your\n\
     message with a friend you have both verified. A friend's Raven can't read your\n\
     messages, but it learns that you wrote to someone, to whom, and when.\n\
     Turn on? [y/N] ",
    "This node may pass sealed messages on from a verified contact to another\n\
     verified contact (your friend). You cannot read them.",
];

/// The engine's honest words for the bridge, store and relay flags.
const MOCK_EFFECTS: [&str; 3] = [
    "Raven's background program passes on sealed envelopes other devices hand it on its bridge \
     listeners (the older mesh envelope path; a program started by Raven listens for these on \
     this computer only). Messages to your friends don't use it.",
    "Lets Raven's background program keep sealed envelopes until their next hop is ready. In \
     this version it always has a next hop, so this changes nothing yet.",
    "Reserved for relaying between Raven's own links. This version of Raven's background \
     program doesn't use it, so this changes nothing yet. To lend friends a relay, use Relay for \
     friends.",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn manual(ready: bool) -> (MockRaven, Arc<AtomicU64>) {
        let t = Arc::new(AtomicU64::new(1_792_505_100_000));
        (MockRaven::with_clock(Clock::Manual(t.clone()), ready), t)
    }

    fn advance(t: &AtomicU64, ms: u64) {
        t.fetch_add(ms, Ordering::SeqCst);
    }

    #[test]
    fn network_answers_like_a_release_build() {
        let (m, _) = manual(true);
        let net = m.network_status().unwrap();
        assert_eq!(net.way("lan").unwrap().state, CarrierState::On);
        for w in net.carriers.iter().chain(net.protections.iter()) {
            if w.id != "lan" {
                assert_eq!(w.state, CarrierState::Preview, "{}", w.id);
                assert!(!w.lab);
            }
        }
        assert!(net.lab_switches.is_empty());
        assert!(net.help.effects.store.contains("changes nothing yet"));
        // A held way is never turned on, and nothing else changes with it.
        let refused = m
            .network_set(&NetworkPatch {
                p2p: Some(true),
                store: Some(false),
                ..NetworkPatch::default()
            })
            .unwrap_err();
        assert!(refused.is(codes::NOT_AVAILABLE_YET));
        assert!(m.network_status().unwrap().help.store);
        let set = m
            .network_set(&NetworkPatch {
                store: Some(false),
                lan: Some(false),
                ..NetworkPatch::default()
            })
            .unwrap();
        assert_eq!(set.changes.len(), 2);
        assert!(!set.status.help.store && !set.status.help.automatic);
        assert_eq!(set.status.way("lan").unwrap().state, CarrierState::Off);
        let check = m.network_check(true).unwrap();
        assert!(check.summary.iter().any(|r| r.key == "lan"));
        assert!(check.router.is_some());
        assert!(!m.security_status().unwrap().available);
        assert!(!m.lock_state().unwrap().available);
        assert!(m.lock_now().unwrap_err().is(codes::NOT_AVAILABLE_YET));
        assert_eq!(m.updates_status().unwrap().install, "cli_only");
    }

    #[test]
    fn the_lab_network_opens_ways_asks_for_a_yes_and_restarts() {
        let (m, _) = manual(true);
        let m = m.with_lab_network(true);
        let net = m.network_status().unwrap();
        assert!(!net.lab_switches.is_empty());
        let p2p = net.way("p2p").unwrap();
        assert_eq!(
            (p2p.state, p2p.lab, p2p.setting),
            (CarrierState::Off, true, Some(false))
        );
        assert_eq!(net.way("route_hints").unwrap().state, CarrierState::Lab);
        assert_eq!(net.way("websocket").unwrap().state, CarrierState::Preview);
        // Consent: refused without the yes, nothing changed; then agreed.
        let patch = NetworkPatch {
            carry_for_friends: Some(true),
            ..NetworkPatch::default()
        };
        let e = m.network_set(&patch).unwrap_err();
        assert!(e.is(codes::CONSENT_NEEDED), "{e:?}");
        assert!(!m.network_status().unwrap().carry.carry_for_friends.on);
        let set = m
            .network_set(&NetworkPatch {
                agreed: true,
                ..patch
            })
            .unwrap();
        assert!(set.status.carry.carry_for_friends.on && !set.restart_needed);
        // A setting that applies at a restart: on, but not in use yet.
        let set = m
            .network_set(&NetworkPatch {
                p2p: Some(true),
                ..NetworkPatch::default()
            })
            .unwrap();
        assert!(set.restart_needed);
        assert_eq!(set.changes[0].applies, "restart");
        let p2p = set.status.way("p2p").unwrap();
        assert_eq!((p2p.state, p2p.running), (CarrierState::Lab, Some(false)));
        let stop = m.node_stop_report().unwrap();
        assert_eq!(stop.stop, "stopped");
        assert_eq!(m.node_stop_report().unwrap().stop, "not_running");
        m.node_start().unwrap();
        let net = m.network_status().unwrap();
        assert_eq!(net.way("p2p").unwrap().running, Some(true));
        assert_eq!(net.relays.len(), 2);
        // Still held in the lab build: a web proxy.
        let e = m
            .network_set(&NetworkPatch {
                proxy: Some("http://192.0.2.1:8080".into()),
                ..NetworkPatch::default()
            })
            .unwrap_err();
        assert!(e.is(codes::NOT_AVAILABLE_YET));
        // Bluetooth: the adapter shows its fixed address, so it isn't used.
        assert_eq!(net.nearby.address_privacy.as_deref(), Some("public"));
        let (m2, _) = manual(true);
        let no_radio = m2.with_lab_network(false).network_status().unwrap();
        assert_eq!(
            no_radio.way("nearby").unwrap().state,
            CarrierState::NotOnThisOs
        );
    }

    #[test]
    fn lab_lock_v2_security_and_updates() {
        let (m, t) = manual(true);
        let m = m.with_lab_network(true);
        let l = m.lock_state().unwrap();
        assert!(l.available && l.supported && !l.locked);
        assert!(m.lock_now().unwrap().locked);
        assert!(m
            .lock_unlock(None)
            .unwrap_err()
            .is(codes::PASSPHRASE_NEEDED));
        assert!(m
            .lock_unlock(Some("wrong"))
            .unwrap_err()
            .is(codes::PASSPHRASE_WRONG));
        assert!(m
            .lock_unlock(Some("wrong"))
            .unwrap_err()
            .is(codes::PASSPHRASE_WRONG));
        assert!(m
            .lock_unlock(Some("wrong"))
            .unwrap_err()
            .is(codes::UNLOCK_WAIT));
        assert!(m
            .lock_unlock(Some(MOCK_PASSPHRASE))
            .unwrap_err()
            .is(codes::UNLOCK_WAIT));
        advance(&t, 31_000);
        let open = m.lock_unlock(Some(MOCK_PASSPHRASE)).unwrap();
        assert!(!open.locked && open.note.is_some());
        let sec = m.security_status().unwrap();
        assert!(sec.available && sec.lab && sec.node.is_some());
        assert_eq!(sec.notices.len(), 1);
        let upd = m.updates_status().unwrap();
        assert!(upd.available && upd.lab);
        // Without a background program Lock v2 isn't served.
        m.node_stop().unwrap();
        assert!(m.lock_now().unwrap_err().is(codes::NOT_AVAILABLE_YET));
    }

    #[test]
    fn contact_routes_and_mail_friends() {
        let (m, _) = manual(true);
        let ali = m.contacts_routes(ALI).unwrap();
        assert_eq!(ali.routes[0].kind, "lan");
        assert_eq!(ali.routes.last().unwrap().kind, "nearby");
        let set = m
            .contacts_set_routes(&RoutesPatch {
                address: ALI.into(),
                internet: Some("203.0.113.5:7422".into()),
                custodians: Some(vec![MINA.into()]),
                ..RoutesPatch::default()
            })
            .unwrap();
        assert!(set.routes.routes.iter().any(|r| r.kind == "internet"));
        assert_eq!(set.routes.carriers.listed[0].address.as_deref(), Some(MINA));
        // Rena is not verified: no mail friend.
        assert!(m
            .contacts_set_routes(&RoutesPatch {
                address: ALI.into(),
                custodians: Some(vec![RENA.into()]),
                ..RoutesPatch::default()
            })
            .is_err());
    }

    #[test]
    fn the_mock_runs_only_when_asked_for() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(!demo_requested(env(&[])));
        assert!(!demo_requested(env(&[
            ("RAVEN_DEMO", ""),
            ("RAVEN_MOCK_PROFILE", " ")
        ])));
        assert!(!demo_requested(env(&[("RAVEN_DEMO", "0")])));
        assert!(!demo_requested(env(&[("RAVEN_DATA_DIR", "/tmp/x")])));
        assert!(demo_requested(env(&[("RAVEN_DEMO", "1")])));
        assert!(demo_requested(env(&[("RAVEN_DEMO", "yes")])));
        assert!(demo_requested(env(&[("RAVEN_MOCK_PROFILE", "ready")])));
        assert!(demo_requested(env(&[("RAVEN_MOCK_PROFILE", "locked")])));
    }

    fn kinds(p: &Poll) -> Vec<&EventKind> {
        p.events.iter().map(|e| &e.kind).collect()
    }

    #[test]
    fn wordlist_and_check_word() {
        assert_eq!(wordlist().len(), 256);
        // Test vector words_alice_001: data indexes and check index 248 (wagon).
        assert_eq!(gf_check(&[241, 80, 25, 253, 153, 114, 81, 125, 98]), 248);
        assert_eq!(gf_check(&[0; 9]), 0);
        assert_eq!(gf_check(&[255; 9]), 169);
        assert_eq!(gf_check(&[0, 1, 2, 3, 4, 5, 6, 7, 8]), 44);
        assert_eq!(
            names(&words_for(&ME_WORDS)).join(" "),
            "turtle frog bread wolf paper laptop funnel mango honey wagon"
        );
        assert!(check_ok(&words_from_address("rvn1qanything")));
    }

    #[test]
    fn word_hints_complete_after_three_letters() {
        let (m, _) = manual(false);
        assert_eq!(
            m.verify_word_hint("tur").unwrap().as_deref(),
            Some("turtle")
        );
        assert_eq!(
            m.verify_word_hint("TURTLE").unwrap().as_deref(),
            Some("turtle")
        );
        assert_eq!(
            m.verify_word_hint("turtles").unwrap().as_deref(),
            Some("turtle")
        );
        assert_eq!(m.verify_word_hint("tu").unwrap(), None);
        assert_eq!(m.verify_word_hint("xqz").unwrap(), None);
    }

    #[test]
    fn first_run_setup() {
        let (m, _) = manual(false);
        assert_eq!(m.profile_state().unwrap().state, ProfileStateKind::New);
        assert!(m.contacts_list().unwrap_err().is(codes::NOT_INITIALIZED));
        let id = m.identity_create(Some("Sam")).unwrap();
        assert_eq!(id.words.len(), 10);
        assert_eq!(id.words[9], "wagon");
        assert_eq!(id.old_code, ME_OLD_CODE);
        assert_eq!(m.profile_state().unwrap().state, ProfileStateKind::Ready);
        assert_eq!(
            m.settings_get().unwrap().display_name.as_deref(),
            Some("Sam")
        );
        assert!(m.identity_create(None).is_err());
        // Node: starting, then listening.
        let s = m.node_start().unwrap();
        assert!(s.is_starting());
        assert!(s.started_by_app);
    }

    #[test]
    fn demo_friends() {
        let (m, _) = manual(true);
        let c = m.contacts_list().unwrap();
        let by = |n: &str| c.iter().find(|x| x.name == n).unwrap().clone();
        let (ali, mina, rena) = (by("Ali"), by("Mina"), by("Rena"));
        assert!(ali.verified && ali.presence == Presence::InTouch);
        assert_eq!(mina.waiting.unwrap().count, 1);
        assert!(!rena.verified);
        for x in &c {
            assert_eq!(
                x.avatar_index as usize,
                theme::avatar_index(&x.address),
                "avatar rule"
            );
        }
        assert_eq!(
            (ali.avatar_index, mina.avatar_index, rena.avatar_index),
            (0, 1, 2)
        );
        let h = m.messages_history(&ali.address, None, 50).unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[1].state, MessageState::Delivered);
        let older = m
            .messages_history(&ali.address, Some(&h[1].id), 50)
            .unwrap();
        assert_eq!(older.len(), 1);
    }

    #[test]
    fn demo_event_stream() {
        let (m, t) = manual(true);
        let p = m.events_poll(0).unwrap();
        assert_eq!(
            kinds(&p),
            vec![&EventKind::StatusOk],
            "heartbeat right away"
        );
        advance(&t, 1_200);
        let p2 = m.events_poll(p.seq).unwrap();
        assert!(
            matches!(p2.events.first().map(|e| &e.kind), Some(EventKind::Incoming { message }) if message.address == ALI)
        );
        advance(&t, 1_400);
        let p3 = m.events_poll(p2.seq).unwrap();
        assert!(p3.events.iter().any(
            |e| matches!(&e.kind, EventKind::Incoming { message } if message.address == RENA)
        ));
        assert!(p3.events.iter().any(|e| matches!(&e.kind, EventKind::Presence { address, presence: Presence::InTouch } if address == RENA)));
        advance(&t, 3_400);
        let p4 = m.events_poll(p3.seq).unwrap();
        assert!(p4
            .events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::Retry { id, attempts: 3, .. } if id == "d3")));
        advance(&t, 4_500);
        let p5 = m.events_poll(p4.seq).unwrap();
        assert!(p5.events.iter().any(|e| matches!(&e.kind, EventKind::Presence { address, presence: Presence::InTouch } if address == MINA)));
        assert!(p5
            .events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::Receipt { id, .. } if id == "d3")));
        let mina = m
            .contacts_list()
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Mina")
            .unwrap();
        assert!(mina.waiting.is_none(), "the waiting message was delivered");
        // Sequence numbers only grow; polling again returns nothing new.
        let again = m.events_poll(p5.seq).unwrap();
        assert!(again.events.iter().all(|e| e.seq > p5.seq));
    }

    /// E1 round two: unread is the engine's count, shared with the terminal
    /// app; `messages.mark_read` never moves a marker back.
    #[test]
    fn unread_is_counted_and_marked_read_never_backwards() {
        let (m, t) = manual(true);
        let unread = |m: &MockRaven, a: &str| {
            let list = m.contacts_list().unwrap();
            list.iter().find(|c| c.address == a).unwrap().unread
        };
        assert!(m.contacts_list().unwrap().iter().all(|c| c.unread == 0));
        let seq = m.events_poll(0).unwrap().seq;
        advance(&t, 3_000); // Ali at 1.2 s, Rena at 2.6 s
        let p = m.events_poll(seq).unwrap();
        assert!(p.events.iter().any(|e| e.kind
            == EventKind::Unread {
                address: RENA.into(),
                unread: 1
            }));
        assert_eq!(unread(&m, RENA), 1);
        assert_eq!(m.messages_mark_read(RENA, None).unwrap(), 0);
        let p2 = m.events_poll(p.seq).unwrap();
        assert!(p2.events.iter().any(|e| e.kind
            == EventKind::Unread {
                address: RENA.into(),
                unread: 0
            }));
        // Ali's newest read, then his oldest: nothing moves back.
        let h = m.messages_history(ALI, None, 50).unwrap();
        assert_eq!(m.messages_mark_read(ALI, Some(&h[2].id)).unwrap(), 0);
        assert_eq!(m.messages_mark_read(ALI, Some(&h[0].id)).unwrap(), 0);
        assert_eq!(unread(&m, ALI), 0);
        // Yours are never unread; a message not in the chat is refused.
        m.messages_send(RENA, "hi").unwrap();
        assert_eq!(unread(&m, RENA), 0);
        assert!(m
            .messages_mark_read(ALI, Some("nope"))
            .unwrap_err()
            .is(codes::BAD_REQUEST));
    }

    #[test]
    fn sending_is_honest() {
        let (m, t) = manual(true);
        let seq = m.events_poll(0).unwrap().seq;
        let sent = m.messages_send(ALI, "Yes, I'll bring it").unwrap();
        assert_eq!(sent.state, MessageState::Sending);
        let p = m.events_poll(seq).unwrap();
        assert!(p.events.iter().any(|e| e.kind
            == EventKind::SendStarted {
                id: sent.id.clone()
            }));
        assert!(
            !p.events
                .iter()
                .any(|e| matches!(e.kind, EventKind::Receipt { .. })),
            "no receipt yet"
        );
        advance(&t, 800);
        let p2 = m.events_poll(p.seq).unwrap();
        assert!(p2.events.iter().any(|e| e.kind
            == EventKind::State {
                id: sent.id.clone(),
                state: MessageState::Sent
            }));
        advance(&t, 200);
        let p3 = m.events_poll(p2.seq).unwrap();
        assert!(p3.events.iter().any(
            |e| matches!(&e.kind, EventKind::Receipt { id, ms: 1_000, .. } if *id == sent.id)
        ));
        // Rena is not in touch: the message waits.
        let w = m.messages_send(RENA, "hello").unwrap();
        advance(&t, 800);
        let p4 = m.events_poll(p3.seq).unwrap();
        assert!(p4.events.iter().any(|e| e.kind
            == EventKind::State {
                id: w.id.clone(),
                state: MessageState::Waiting
            }));
        let d = m.messages_details(&w.id).unwrap();
        assert!(d.status_text.starts_with("Waiting for Rena until"));
        assert!(d.why_text.contains("Rena's Raven isn't reachable"));
        let stopped = m.messages_stop(&w.id).unwrap();
        assert_eq!(stopped.state, MessageState::Failed);
        assert!(m
            .messages_send(ALI, &"x".repeat(MAX_TEXT + 1))
            .unwrap_err()
            .is(codes::MESSAGE_TOO_LONG));
        assert!(m
            .messages_send("rvn1qnobody", "hi")
            .unwrap_err()
            .is(codes::NOT_A_CONTACT));
        assert!(m
            .messages_send(ALI, "  ")
            .unwrap_err()
            .is(codes::BAD_REQUEST));
    }

    #[test]
    fn verify_words_match_typo_mismatch() {
        let (m, _) = manual(true);
        let mut words: Vec<String> = names(&words_for(&RENA_DEMO_WORDS));
        assert_eq!(words[9], "milk");
        // A misheard word: the check word catches it.
        let mut typo = words.clone();
        typo[5] = "apple".into();
        assert_eq!(
            m.verify_check_words(RENA, &typo).unwrap(),
            CheckOutcome::Typo
        );
        // Two words swapped: also caught.
        let mut swapped = words.clone();
        swapped.swap(2, 3);
        assert_eq!(
            m.verify_check_words(RENA, &swapped).unwrap(),
            CheckOutcome::Typo
        );
        // Real Raven words that fit together, but someone else's key.
        let other = names(&words_for(&ME_WORDS));
        assert_eq!(
            m.verify_check_words(RENA, &other).unwrap(),
            CheckOutcome::Mismatch
        );
        // Prefixes and plurals are accepted, like typing.
        words[0] = "anc".into();
        words[1] = "baskets".into();
        assert_eq!(
            m.verify_check_words(RENA, &words).unwrap(),
            CheckOutcome::Match
        );
        let rena = m
            .contacts_list()
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Rena")
            .unwrap();
        assert!(rena.verified, "a match pins the friend as verified");
        // Nine words or a non-word is incomplete, never a right/wrong per word.
        assert!(m
            .verify_check_words(RENA, &words[..9])
            .unwrap_err()
            .is(codes::WORDS_INCOMPLETE));
        let mut bad = words.clone();
        bad[0] = "zzzz".into();
        assert!(m
            .verify_check_words(RENA, &bad)
            .unwrap_err()
            .is(codes::WORDS_INCOMPLETE));
    }

    #[test]
    fn add_a_friend() {
        let (m, _) = manual(true);
        let mine = m.card_make(true).unwrap();
        assert_eq!(mine.address_included.as_deref(), Some("192.168.1.23:7420"));
        assert!(
            m.card_parse(&mine.invite)
                .unwrap_err()
                .is(codes::CARD_INVALID),
            "own invite"
        );
        assert!(m.card_parse("hello").unwrap_err().is(codes::CARD_INVALID));
        let invite =
            "Here: raven:rvn1qsamdemo8x7k2v4m:0123456789abcdef0123?lan=192.168.1.50:7420 thanks";
        let parsed = m.card_parse(invite).unwrap();
        assert!(parsed.has_address);
        assert_eq!(parsed.routes.len(), 1);
        let clash = m.contacts_add(invite, "ali").unwrap_err();
        assert!(clash.is(codes::NAME_CLASH));
        assert!(clash.message.contains("ali"));
        let sam = m.contacts_add(invite, "Sam\u{202E}x").unwrap();
        assert_eq!(sam.name, "Samx", "names are sanitised on the way out");
        assert!(!sam.verified, "new friends are saved as not verified");
        assert_eq!(sam.lan.as_deref(), Some("192.168.1.50:7420"));
        let no_addr = m
            .contacts_add("raven:rvn1qnoaddr5x7k2v4m:0123456789abcdef0123", "Pat")
            .unwrap();
        assert_eq!(no_addr.presence, Presence::NoAddress);
        assert_eq!(
            m.messages_send(&no_addr.address, "hi").unwrap().state,
            MessageState::NotSentMeetFirst
        );
    }

    #[test]
    fn settings_round_trip() {
        let (m, _) = manual(true);
        let s = m
            .settings_set(&SettingsPatch {
                reduced_motion: Some(true),
                keep_chats_days: Some(None),
                when_quit: Some(WhenQuit::Ask),
                ..Default::default()
            })
            .unwrap();
        assert!(s.reduced_motion);
        assert_eq!(s.keep_chats_days, None);
        assert_eq!(s.when_quit, WhenQuit::Ask);
        assert!(s.forget_addresses, "untouched fields keep their value");
        let n = m
            .settings_set(&SettingsPatch {
                receive: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert!(!n.receive);
        assert!(m.node_status().unwrap().listening.is_none());
    }

    #[test]
    fn no_response_carries_a_secret_field() {
        let (m, t) = manual(false);
        let mut ops: Vec<(&str, Value)> = vec![
            ("app.info", json!({})),
            ("profile.state", json!({})),
            ("identity.create", json!({ "display_name": "Sam" })),
            ("identity.me", json!({})),
            ("card.make", json!({ "include_address": true })),
            ("contacts.list", json!({})),
            ("node.start", json!({})),
            ("settings.get", json!({})),
            ("messages.send", json!({ "address": ALI, "text": "hi" })),
            ("messages.history", json!({ "address": ALI, "limit": 50 })),
            ("messages.details", json!({ "id": "d3" })),
            (
                "verify.check_old_code",
                json!({ "address": RENA, "code": "x" }),
            ),
            ("privacy.state", json!({})),
        ];
        ops.push(("events.poll", json!({ "after_seq": 0 })));
        for (op, args) in ops {
            advance(&t, 3_000);
            let raw = m.handle_request(&make_request(op, &args));
            let v: Value = serde_json::from_str(&raw).unwrap();
            assert!(!contains_secret_field(&v), "{op}: {raw}");
            assert_eq!(v["ok"], true, "{op}: {raw}");
        }
    }

    #[test]
    fn bad_requests() {
        let (m, _) = manual(true);
        let e = m.call("no.such.op", json!({})).unwrap_err();
        assert!(e.is(codes::BAD_REQUEST));
        assert_eq!(e.message, tr("error.badRequest"));
        assert!(e.detail.contains("no.such.op"));
        let raw = m.handle_request("not json");
        assert!(parse_response(&raw).unwrap_err().is(codes::BAD_REQUEST));
        let raw = m.handle_request(r#"{"v":7,"op":"app.info"}"#);
        assert!(parse_response(&raw).unwrap_err().is(codes::BAD_REQUEST));
        assert!(m
            .call("contacts.rename", json!({ "address": ALI }))
            .unwrap_err()
            .is(codes::BAD_REQUEST));
        assert!(m
            .messages_details("no-such-id")
            .unwrap_err()
            .is(codes::BAD_REQUEST));
    }

    #[test]
    fn errors_use_the_engine_texts() {
        let (m, _) = manual(false);
        assert_eq!(
            m.contacts_list().unwrap_err().message,
            tr("error.notInitialized")
        );
        let (m, _) = manual(true);
        assert_eq!(
            m.messages_send("rvn1qnobody", "hi").unwrap_err().message,
            tr("error.notAContact")
        );
        assert_eq!(
            m.card_parse("hello").unwrap_err().message,
            tr("error.cardInvalid")
        );
        assert_eq!(
            m.verify_check_words(RENA, &["turtle".to_string()])
                .unwrap_err()
                .message,
            tr("error.wordsIncomplete")
        );
        assert_eq!(
            m.messages_send(ALI, &"x".repeat(MAX_TEXT + 1))
                .unwrap_err()
                .message,
            tr("error.messageTooLong")
        );
    }

    #[test]
    fn privacy_is_not_available_yet() {
        let (m, _) = manual(true);
        assert_eq!(m.privacy_state().unwrap(), PrivacyState::default());
        for e in [
            m.privacy_lock().unwrap_err(),
            m.privacy_unlock().unwrap_err(),
            m.privacy_encrypt().unwrap_err(),
            m.privacy_erase().unwrap_err(),
        ] {
            assert!(e.is(codes::NOT_AVAILABLE_YET), "{e:?}");
            assert_eq!(e.message, tr("error.notAvailableYet"));
        }
        // Erase without the typed confirmation is a bad request.
        assert!(m
            .call("privacy.erase", json!({ "confirm": "yes" }))
            .unwrap_err()
            .is(codes::BAD_REQUEST));
        assert!(m
            .call("privacy.erase", json!({}))
            .unwrap_err()
            .is(codes::BAD_REQUEST));
        // Nothing changed: still ready, friends still there.
        assert_eq!(m.profile_state().unwrap().state, ProfileStateKind::Ready);
        assert_eq!(m.contacts_list().unwrap().len(), 3);
    }

    #[test]
    fn locked_profile_for_review() {
        let (m, _) = manual(true);
        m.state.lock().unwrap().profile = ProfileStateKind::Locked;
        assert!(m.contacts_list().unwrap_err().is(codes::PROFILE_LOCKED));
        assert_eq!(
            m.contacts_list().unwrap_err().message,
            tr("error.profileLocked")
        );
        assert!(m.privacy_state().unwrap().locked);
        assert!(m.privacy_unlock().unwrap_err().is(codes::NOT_AVAILABLE_YET));
    }

    #[test]
    fn old_code_match_and_mismatch() {
        let (m, _) = manual(true);
        let rena = || {
            m.contacts_list()
                .unwrap()
                .into_iter()
                .find(|c| c.name == "Rena")
                .unwrap()
        };
        assert!(!rena().verified);
        // Another case is a different code: no match, and nothing is pinned.
        assert_eq!(
            m.verify_check_old_code(RENA, "tq7m-29xa-hke4").unwrap(),
            OldCodeOutcome::Mismatch
        );
        assert_eq!(
            m.verify_check_old_code(RENA, "Tq7m-29Xa-hKe").unwrap(),
            OldCodeOutcome::Mismatch,
            "one character short"
        );
        // Your own code is someone else's key.
        assert_eq!(
            m.verify_check_old_code(RENA, ME_OLD_CODE).unwrap(),
            OldCodeOutcome::Mismatch
        );
        assert!(!rena().verified);
        // Friends without a known old code never match.
        assert_eq!(
            m.verify_check_old_code(ALI, RENA_DEMO_OLD_CODE).unwrap(),
            OldCodeOutcome::Mismatch
        );
        // Unknown friend, empty code.
        assert!(m
            .verify_check_old_code("rvn1qnobody", RENA_DEMO_OLD_CODE)
            .unwrap_err()
            .is(codes::NOT_A_CONTACT));
        assert!(m
            .verify_check_old_code(RENA, " - ")
            .unwrap_err()
            .is(codes::BAD_REQUEST));
        // Spaces instead of dashes, Persian and Arabic-Indic digits: a match,
        // and the friend is pinned as verified, as with the words.
        assert_eq!(
            m.verify_check_old_code(RENA, "Tq\u{06F7}m \u{0662}9Xa hKe\u{06F4}")
                .unwrap(),
            OldCodeOutcome::Match
        );
        assert!(rena().verified);
    }
}
