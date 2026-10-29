//! The desktop API contract v1 (`desktop/api/API.md`) in Rust.
//!
//! The app talks to the engine only through [`RavenEngine::call`]: an
//! operation name plus JSON arguments in, a JSON result or an [`ApiError`]
//! out. The typed wrappers decode results into the structs below and run the
//! display sanitiser over every text field, so nothing unsanitised reaches a
//! label. Unknown fields are ignored (forward compatibility); unknown enum
//! values decode to an `Unknown` variant instead of failing.

use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

use crate::sanitize;

/// Contract version this app speaks.
pub const API_VERSION: u64 = 1;

/// Error codes of the first set (API.md, "Error codes").
pub mod codes {
    pub const NOT_INITIALIZED: &str = "NOT_INITIALIZED";
    pub const PROFILE_UNREADABLE: &str = "PROFILE_UNREADABLE";
    pub const PROFILE_LOCKED: &str = "PROFILE_LOCKED";
    pub const KEYCHAIN_DENIED: &str = "KEYCHAIN_DENIED";
    pub const NODE_NOT_RUNNING: &str = "NODE_NOT_RUNNING";
    pub const NOT_A_CONTACT: &str = "NOT_A_CONTACT";
    pub const CONTACT_BLOCKED: &str = "CONTACT_BLOCKED";
    pub const NAME_CLASH: &str = "NAME_CLASH";
    pub const CARD_INVALID: &str = "CARD_INVALID";
    pub const WORDS_INCOMPLETE: &str = "WORDS_INCOMPLETE";
    pub const MESSAGE_TOO_LONG: &str = "MESSAGE_TOO_LONG";
    pub const NOT_SENT_MEET_FIRST: &str = "NOT_SENT_MEET_FIRST";
    pub const TIME_RAN_OUT: &str = "TIME_RAN_OUT";
    /// A privacy action before the privacy pack lands.
    pub const NOT_AVAILABLE_YET: &str = "NOT_AVAILABLE_YET";
    /// An unknown operation or bad arguments.
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    pub const INTERNAL: &str = "INTERNAL";
    /// `network.set`: a setting that needs the person's yes (`agreed`).
    pub const CONSENT_NEEDED: &str = "CONSENT_NEEDED";
    /// `lock.unlock`: that Raven passphrase is not right.
    pub const PASSPHRASE_WRONG: &str = "PASSPHRASE_WRONG";
    /// `lock.unlock`: this profile has a Raven passphrase and none came.
    pub const PASSPHRASE_NEEDED: &str = "PASSPHRASE_NEEDED";
    /// `lock.unlock`: too many wrong passphrases; wait, then try again.
    pub const UNLOCK_WAIT: &str = "UNLOCK_WAIT";
    /// Groups: no such group.
    pub const GROUP_NOT_FOUND: &str = "GROUP_NOT_FOUND";
    /// Groups: you left the group or were removed from it.
    pub const GROUP_NOT_MEMBER: &str = "GROUP_NOT_MEMBER";
    /// Groups: too many group copies still wait; try again later.
    pub const GROUP_QUEUE_FULL: &str = "GROUP_QUEUE_FULL";
    /// Groups: none of the other members is your friend.
    pub const GROUP_NO_ROUTE: &str = "GROUP_NO_ROUTE";
    /// Groups: only the admin adds and removes.
    pub const GROUP_NOT_ADMIN: &str = "GROUP_NOT_ADMIN";
    /// Groups: more members than a group may have (16, you included).
    pub const GROUP_TOO_LARGE: &str = "GROUP_TOO_LARGE";
    /// Groups: the admin can't leave.
    pub const GROUP_ADMIN_CANNOT_LEAVE: &str = "GROUP_ADMIN_CANNOT_LEAVE";
}

/// How often the apps pull `events.poll` (API.md, "Boundary"): every 500 ms
/// while a window is open, every 5 s when only the background part of the
/// app (a tray icon, later) is alive.
pub mod poll {
    pub const WINDOW_OPEN_MS: u64 = 500;
    pub const BACKGROUND_MS: u64 = 5_000;

    pub fn interval_ms(window_open: bool) -> u64 {
        if window_open {
            WINDOW_OPEN_MS
        } else {
            BACKGROUND_MS
        }
    }

    /// True when the next poll is due, `since_last_ms` after the last one
    /// (`None`: no poll yet).
    pub fn due(window_open: bool, since_last_ms: Option<u64>) -> bool {
        since_last_ms.is_none_or(|ms| ms >= interval_ms(window_open))
    }
}

/// A failed call: a code, a plain-English message naming one next step, and
/// technical detail shown only under "Details".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub detail: String,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            detail: detail.into(),
        }
    }

    /// The engine's answer can't be used (not JSON, wrong contract version,
    /// a secret-looking field, an unexpected shape). The specifics go into
    /// `detail`.
    pub fn bad_response(detail: impl Into<String>) -> Self {
        Self::new(
            codes::INTERNAL,
            crate::strings::tr("error.badResponse"),
            detail,
        )
    }

    /// Something unexpected on the app's side (for example the engine thread
    /// stopped). The specifics go into `detail`.
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            codes::INTERNAL,
            crate::strings::tr("error.internal"),
            detail,
        )
    }

    pub fn is(&self, code: &str) -> bool {
        self.code == code
    }

    fn sanitize(mut self) -> Self {
        sanitize::inline(&mut self.message);
        sanitize::multiline(&mut self.detail);
        self
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

// ---------------------------------------------------------------- envelope

/// Build the request envelope `{"v": 1, "op": ..., "args": {...}}`.
pub fn make_request(op: &str, args: &Value) -> String {
    json!({ "v": API_VERSION, "op": op, "args": args }).to_string()
}

/// Field names the engine must never return (API.md: "Secrets never cross the
/// boundary"). The app refuses a response that contains one anyway.
const SECRET_FIELD_MARKERS: &[&str] = &[
    "seed",
    "private_key",
    "privatekey",
    "plaintext",
    "recovery",
    "mnemonic",
];

/// True if any object key anywhere in `value` looks like a secret field.
pub fn contains_secret_field(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| {
            let k = k.to_ascii_lowercase();
            SECRET_FIELD_MARKERS.iter().any(|m| k.contains(m)) || contains_secret_field(v)
        }),
        Value::Array(items) => items.iter().any(contains_secret_field),
        _ => false,
    }
}

/// Parse a response envelope into the `result` value or an [`ApiError`].
pub fn parse_response(response: &str) -> Result<Value, ApiError> {
    let value: Value = serde_json::from_str(response)
        .map_err(|e| ApiError::bad_response(format!("response is not JSON: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| ApiError::bad_response("response is not a JSON object"))?;
    match obj.get("v").and_then(Value::as_u64) {
        Some(API_VERSION) => {}
        Some(v) => {
            return Err(ApiError::bad_response(format!(
                "engine speaks contract v{v}, this app speaks v{API_VERSION}"
            )))
        }
        None => return Err(ApiError::bad_response("response has no contract version")),
    }
    if contains_secret_field(&value) {
        return Err(ApiError::bad_response(
            "response contained a secret-looking field and was refused",
        ));
    }
    match obj.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(obj.get("result").cloned().unwrap_or_else(|| json!({}))),
        Some(false) => {
            let err = obj
                .get("error")
                .cloned()
                .and_then(|e| serde_json::from_value::<ApiError>(e).ok())
                .unwrap_or_else(|| {
                    ApiError::bad_response("failure response without a readable error")
                });
            Err(err.sanitize())
        }
        None => Err(ApiError::bad_response("response has no ok field")),
    }
}

/// Decode a result value into a typed struct.
pub fn decode<T: DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|e| ApiError::bad_response(format!("unexpected result shape: {e}")))
}

/// Message ids may be strings or numbers on the wire; the app keeps strings.
fn de_id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        S(String),
        N(u64),
    }
    Ok(match Id::deserialize(d)? {
        Id::S(s) => s,
        Id::N(n) => n.to_string(),
    })
}

// ---------------------------------------------------------------- types

/// Types that carry display text clean it with this.
pub trait SanitizeDisplay {
    fn sanitize_display(&mut self);
}

fn sanitized<T: SanitizeDisplay>(mut v: T) -> T {
    v.sanitize_display();
    v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Build {
    Release,
    #[default]
    Dev,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub platform: String,
}

impl SanitizeDisplay for AppInfo {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.version);
        sanitize::inline(&mut self.platform);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStateKind {
    New,
    Ready,
    Locked,
    Unreadable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileState {
    pub state: ProfileStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SanitizeDisplay for ProfileState {
    fn sanitize_display(&mut self) {
        sanitize::inline_opt(&mut self.reason);
    }
}

/// Result of `identity.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewIdentity {
    pub address: String,
    pub words: Vec<String>,
    pub old_code: String,
}

impl SanitizeDisplay for NewIdentity {
    fn sanitize_display(&mut self) {
        self.words.iter_mut().for_each(sanitize::inline);
        sanitize::inline(&mut self.old_code);
    }
}

/// Result of `identity.me`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Me {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub address: String,
    pub words: Vec<String>,
    pub old_code: String,
    #[serde(default)]
    pub card: String,
    #[serde(default)]
    pub invite: String,
}

impl SanitizeDisplay for Me {
    fn sanitize_display(&mut self) {
        sanitize::inline_opt(&mut self.display_name);
        self.words.iter_mut().for_each(sanitize::inline);
        sanitize::inline(&mut self.old_code);
        sanitize::multiline(&mut self.card);
        sanitize::inline(&mut self.invite);
    }
}

/// Result of `card.make`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub card: String,
    pub invite: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_included: Option<String>,
}

impl SanitizeDisplay for Card {
    fn sanitize_display(&mut self) {
        sanitize::multiline(&mut self.card);
        sanitize::inline(&mut self.invite);
        sanitize::inline_opt(&mut self.address_included);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub preview: bool,
}

/// Result of `card.parse`. It never carries the friend's security words.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedCard {
    pub short_address: String,
    pub has_address: bool,
    #[serde(default)]
    pub routes: Vec<Route>,
}

impl SanitizeDisplay for ParsedCard {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.short_address);
        for r in &mut self.routes {
            sanitize::inline(&mut r.label);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    InTouch,
    Waiting,
    /// `"none"` on the wire: no recent contact.
    #[serde(rename = "none")]
    #[default]
    NoRecent,
    NoAddress,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub count: u32,
    pub until_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub address: String,
    pub name: String,
    #[serde(default)]
    pub short_address: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub presence: Presence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contact_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<Waiting>,
    #[serde(default)]
    pub avatar_index: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan: Option<String>,
    /// The friend's messages after the one last read on this computer, in
    /// this app or the terminal app (the engine counts them; API.md, "Read
    /// and unread").
    #[serde(default)]
    pub unread: u32,
}

impl SanitizeDisplay for Contact {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.name);
        sanitize::inline(&mut self.short_address);
        sanitize::inline_opt(&mut self.lan);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Sending,
    Sent,
    Delivered,
    Waiting,
    TimeRanOut,
    NotSentMeetFirst,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    pub address: String,
    pub direction: Direction,
    pub text: String,
    pub at_ms: u64,
    pub state: MessageState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The stable kind of `path` (API.md: `lan`, `internet`, `p2p_relayed`,
    /// `websocket`, `nearby`, `friend`, `handoff`, `letter`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_kind: Option<String>,
    /// When its receipt came, given only for a delivery more than a minute
    /// after it was written (API.md): the tick then says "Delivered 15:10".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempts: Option<u32>,
}

impl SanitizeDisplay for Message {
    fn sanitize_display(&mut self) {
        sanitize::multiline(&mut self.text);
        sanitize::inline_opt(&mut self.path);
    }
}

/// Result of `messages.details`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDetails {
    pub message: Message,
    #[serde(default)]
    pub status_text: String,
    #[serde(default)]
    pub why_text: String,
    #[serde(default)]
    pub action_text: String,
    #[serde(default)]
    pub detail: String,
}

impl SanitizeDisplay for MessageDetails {
    fn sanitize_display(&mut self) {
        self.message.sanitize_display();
        sanitize::inline(&mut self.status_text);
        sanitize::inline(&mut self.why_text);
        sanitize::inline(&mut self.action_text);
        sanitize::multiline(&mut self.detail);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CarrierState {
    On,
    #[default]
    Preview,
    Off,
    /// On only through a developer build's lab switch.
    Lab,
    /// This computer can't have it (Bluetooth nearby without a backend).
    NotOnThisOs,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Carriers {
    #[serde(default)]
    pub lan: CarrierState,
    #[serde(default)]
    pub internet: CarrierState,
    #[serde(default)]
    pub p2p: CarrierState,
    #[serde(default)]
    pub custody: CarrierState,
    #[serde(default)]
    pub nearby: CarrierState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeStatus {
    pub running: bool,
    #[serde(default)]
    pub started_by_app: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listening: Option<String>,
    #[serde(default)]
    pub receive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default)]
    pub last_ok_ms: u64,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub carriers: Carriers,
    /// The ways a developer build's lab switch turned on (empty in a
    /// release build and in the mock).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lab: Vec<String>,
    /// The `RAVEN_LAB_*` switches the engine sees (names only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lab_switches: Vec<String>,
}

impl NodeStatus {
    /// The background program runs and should listen, but is not listening
    /// yet: the "Starting Raven…" state with the spinner.
    pub fn is_starting(&self) -> bool {
        self.running && self.receive && self.listening.is_none()
    }

    /// Port of the listening address (`"0.0.0.0:7420"` gives `"7420"`).
    pub fn listening_port(&self) -> Option<&str> {
        self.listening
            .as_deref()
            .and_then(|l| l.rsplit_once(':'))
            .map(|(_, p)| p)
    }

    /// True when it listens on loopback only: friends can't reach it.
    pub fn is_local_only(&self) -> bool {
        self.listening
            .as_deref()
            .is_some_and(|l| l.starts_with("127.") || l.starts_with("[::1]"))
    }
}

impl SanitizeDisplay for NodeStatus {
    fn sanitize_display(&mut self) {
        sanitize::inline_opt(&mut self.listening);
        sanitize::inline_opt(&mut self.address);
    }
}

/// Result of `node.stop`: the status afterwards plus what happened
/// (`stop`: `stopped`, `still_running`, `advice`, `managed`, `not_running`
/// or `failed`; `note` says how to stop it by hand, or why it failed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NodeStop {
    #[serde(flatten)]
    pub status: NodeStatus,
    /// Empty from an engine that doesn't say.
    #[serde(default)]
    pub stop: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl SanitizeDisplay for NodeStop {
    fn sanitize_display(&mut self) {
        self.status.sanitize_display();
        sanitize::inline(&mut self.stop);
        sanitize::inline_opt(&mut self.note);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhenQuit {
    #[default]
    Keep,
    StopIfStarted,
    Ask,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub receive: bool,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub when_quit: WhenQuit,
    /// 7, 30 or 90; `None` keeps chats always.
    #[serde(default)]
    pub keep_chats_days: Option<u32>,
    #[serde(default)]
    pub forget_addresses: bool,
    #[serde(default)]
    pub lock_on_sleep: bool,
}

impl SanitizeDisplay for Settings {
    fn sanitize_display(&mut self) {
        sanitize::inline_opt(&mut self.display_name);
    }
}

/// A partial `settings.set`. Only fields that are `Some` are sent;
/// `keep_chats_days: Some(None)` sends `null` ("Always").
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduced_motion: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_quit: Option<WhenQuit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_chats_days: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forget_addresses: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_on_sleep: Option<bool>,
}

/// One engine event. Animations are driven by these only (DESIGN.md,
/// "Honest motion").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    #[serde(default)]
    pub at_ms: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// A successful status check: the heartbeat pulses.
    StatusOk,
    /// A send attempt began: a packet travels out.
    SendStarted {
        #[serde(deserialize_with = "de_id")]
        id: String,
    },
    /// A signed receipt arrived: the receipt travels back, "Delivered".
    Receipt {
        #[serde(deserialize_with = "de_id")]
        id: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        path_kind: String,
        #[serde(default)]
        ms: u64,
        /// When it came, only for a late delivery (as `Message::delivered_at_ms`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delivered_at_ms: Option<u64>,
    },
    /// A message arrived: a packet travels in, the bubble slides in.
    Incoming { message: Message },
    /// Yours, sent by another program on this profile (the terminal app,
    /// the CLI): the bubble appears in the open chat.
    Outgoing { message: Message },
    /// A waiting message was tried again: a ring around the friend's dot.
    Retry {
        #[serde(deserialize_with = "de_id")]
        id: String,
        address: String,
        #[serde(default)]
        attempts: u32,
    },
    /// A friend's presence changed: the dot changes with a short blink.
    Presence { address: String, presence: Presence },
    /// A message changed state: the tick text changes.
    State {
        #[serde(deserialize_with = "de_id")]
        id: String,
        state: MessageState,
    },
    /// The background program changed: the network chip changes.
    Node { status: NodeStatus },
    /// A friend's unread count changed (a message came, or the chat was read
    /// here or in the terminal app): the badge follows.
    Unread {
        address: String,
        #[serde(default)]
        unread: u32,
    },
    /// A member's message came in a group (groups draft): a bubble in the
    /// group chat.
    GroupMessage {
        group: String,
        message: GroupMessage,
    },
    /// A group's members, status, unread count or a receipt changed (also a
    /// group that appeared): read it again.
    GroupChanged { group: String },
    /// A kind this app doesn't know yet: ignored.
    #[serde(other)]
    Unknown,
}

impl SanitizeDisplay for Event {
    fn sanitize_display(&mut self) {
        match &mut self.kind {
            EventKind::Receipt {
                path, path_kind, ..
            } => {
                sanitize::inline(path);
                sanitize::inline(path_kind);
            }
            EventKind::Incoming { message } | EventKind::Outgoing { message } => {
                message.sanitize_display()
            }
            EventKind::GroupMessage { message, .. } => message.sanitize_display(),
            EventKind::Node { status } => status.sanitize_display(),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- groups
//
// Group chats (desktop/api/API.md, "Groups"): `groups.list`, `groups.get`,
// `groups.create`, `groups.add`, `groups.remove`, `groups.leave`,
// `groups.history`, `groups.send`, `groups.mark_read`, the events
// `group_message` and `group_changed`. Lab only: a release build answers
// `groups.list` with `available: false` (the app then shows no Groups
// section) and every other op with NOT_AVAILABLE_YET; an engine without the
// ops at all answers an error, which the app treats the same way.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupStatus {
    #[default]
    Active,
    /// You left (`groups.leave`): the history stays, sending is refused.
    Left,
    /// The admin removed you.
    Removed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// 32 hex characters, opaque to the app.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub short_id: String,
    #[serde(default)]
    pub status: GroupStatus,
    /// You are its admin.
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub admin_address: String,
    #[serde(default)]
    pub admin_name: String,
    /// Members, you included.
    #[serde(default)]
    pub members: u32,
    #[serde(default)]
    pub unread: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_text: Option<String>,
    /// The newest message's author ("You" for yours).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_from: Option<String>,
}

impl SanitizeDisplay for Group {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.name);
        sanitize::inline(&mut self.short_id);
        sanitize::inline(&mut self.admin_name);
        sanitize::inline_opt(&mut self.last_text);
        sanitize::inline_opt(&mut self.last_from);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupMember {
    pub address: String,
    #[serde(default)]
    pub short_address: String,
    /// Your name for a friend, else the name the admin gave them.
    pub name: String,
    #[serde(default)]
    pub label: String,
    /// One of your friends (members who aren't are never added by the group).
    #[serde(default)]
    pub friend: bool,
    #[serde(default)]
    pub admin: bool,
    /// You.
    #[serde(default)]
    pub me: bool,
}

impl SanitizeDisplay for GroupMember {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.name);
        sanitize::inline(&mut self.label);
        sanitize::inline(&mut self.short_address);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    /// Still on this computer, or passed on by another member and not
    /// confirmed yet.
    #[default]
    Waiting,
    /// Left this computer, no receipt yet.
    Sent,
    /// That member's Raven confirmed it (a signed receipt).
    Delivered,
    /// Given up.
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupReceipt {
    pub address: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: ReceiptState,
    /// `direct` or `relay` (reached through another member).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupMessage {
    #[serde(deserialize_with = "de_id")]
    pub id: String,
    pub group: String,
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub mine: bool,
    pub text: String,
    pub at_ms: u64,
    /// The member that passed it on, when it was not its author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_name: Option<String>,
    #[serde(default)]
    pub delivered: u32,
    #[serde(default)]
    pub total: u32,
    /// One per member it was for (only on yours).
    #[serde(default)]
    pub receipts: Vec<GroupReceipt>,
}

impl SanitizeDisplay for GroupMessage {
    fn sanitize_display(&mut self) {
        sanitize::multiline(&mut self.text);
        sanitize::inline(&mut self.from_name);
        sanitize::inline_opt(&mut self.via_name);
        for r in &mut self.receipts {
            sanitize::inline(&mut r.name);
        }
    }
}

/// Result of `groups.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GroupsList {
    /// False in a build without groups: no Groups section.
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// Result of `groups.get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupDetail {
    pub group: Group,
    #[serde(default)]
    pub members: Vec<GroupMember>,
}

impl SanitizeDisplay for GroupDetail {
    fn sanitize_display(&mut self) {
        self.group.sanitize_display();
        for m in &mut self.members {
            m.sanitize_display();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Poll {
    pub seq: u64,
    #[serde(default)]
    pub events: Vec<Event>,
    /// The engine dropped events after `after_seq` (its buffer is bounded),
    /// or `after_seq` is from before it started: reload what is on screen
    /// instead of trusting the events alone (raven-client).
    #[serde(default)]
    pub reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Match,
    Typo,
    Mismatch,
    #[serde(other)]
    Unknown,
}

/// Result of `verify.check_old_code` (no "typo": the old code has no check
/// character).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OldCodeOutcome {
    Match,
    Mismatch,
    #[serde(other)]
    Unknown,
}

/// Result of `privacy.state`. `available` is false until the privacy pack
/// lands; the app then shows the privacy actions with the Preview label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PrivacyState {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default)]
    pub locked: bool,
}

// ---------------------------------------------------------------- network

/// One way to reach friends, or a protection, in `network.status`
/// (API.md, Network).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Way {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: CarrierState,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub lab: bool,
    #[serde(default)]
    pub gate: String,
    #[serde(default)]
    pub lab_switches: Vec<String>,
    #[serde(default)]
    pub can_set: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setting: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl SanitizeDisplay for Way {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.name);
        sanitize::inline(&mut self.reason);
        sanitize::inline_opt(&mut self.detail);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PortMapping {
    #[serde(default)]
    pub setting: String,
    #[serde(default)]
    pub router: String,
    #[serde(default)]
    pub nat_pmp_pcp: CarrierState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v4: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v4_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v6: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RelayInUse {
    #[serde(default)]
    pub relay: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friend_address: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProxyState {
    #[serde(default)]
    pub setting: String,
    #[serde(default)]
    pub in_use: String,
    #[serde(default)]
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LabelValue {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NearbyState {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub saved: Option<bool>,
    #[serde(default)]
    pub on_this_os: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_privacy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_range: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected: Option<u32>,
    #[serde(default)]
    pub rows: Vec<LabelValue>,
}

/// The Tor onion carrier as raven-node's Status reports it (API.md,
/// NetworkStatus `tor`): states only, never an address. Present only while
/// raven-node's Status has its Tor line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TorState {
    /// `ready`, `publishing`, `unreachable`, `starting`, `restarting`,
    /// `blocked`, `could_not_start`, `not_built`, `off` or `unknown`.
    #[serde(default)]
    pub phase: String,
    /// raven-node's line after "Tor: ".
    #[serde(default)]
    pub words: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<String>,
    /// The onion service's outage history since raven-node started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outages: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FriendsCarryMine {
    #[serde(default)]
    pub setting: String,
    #[serde(default)]
    pub mail_friends: u32,
    #[serde(default)]
    pub chosen_by_hand: bool,
    #[serde(default)]
    pub offering: u32,
    #[serde(default)]
    pub offering_over_internet: u32,
    #[serde(default)]
    pub offering_reachable: u32,
    #[serde(default)]
    pub listed_by_hand: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CarryForFriends {
    #[serde(default)]
    pub on: bool,
    #[serde(default)]
    pub held_messages: i64,
    #[serde(default)]
    pub held_bytes: i64,
    #[serde(default)]
    pub handoff: bool,
    #[serde(default)]
    pub handoff_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RelayForFriends {
    #[serde(default)]
    pub setting: bool,
    #[serde(default)]
    pub offering: bool,
    #[serde(default)]
    pub offered_to: u32,
}

/// Who carries mail, both ways, and the relay this computer lends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Carry {
    #[serde(default)]
    pub friends_carry_mine: FriendsCarryMine,
    #[serde(default)]
    pub carry_for_friends: CarryForFriends,
    #[serde(default)]
    pub letters_left: Vec<String>,
    #[serde(default)]
    pub relay_for_friends: RelayForFriends,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HelpEffects {
    #[serde(default)]
    pub bridge: String,
    #[serde(default)]
    pub store: String,
    #[serde(default)]
    pub relay: String,
}

/// The node policy flags `raven node bridge|store|relay` sets, with what
/// each really does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HelpFlags {
    #[serde(default)]
    pub bridge: bool,
    #[serde(default)]
    pub store: bool,
    #[serde(default)]
    pub relay: bool,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub effects: HelpEffects,
}

/// What the settings that need a yes mean (shown before `agreed: true`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkTexts {
    #[serde(default)]
    pub carry_for_friends: String,
    #[serde(default)]
    pub friends_carry_mine: String,
    #[serde(default)]
    pub handoff: String,
}

/// Result of `network.status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkStatus {
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub node: String,
    #[serde(default)]
    pub carriers: Vec<Way>,
    #[serde(default)]
    pub protections: Vec<Way>,
    #[serde(default)]
    pub listening: Vec<String>,
    #[serde(default)]
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet_listen: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p2p_listen: Option<String>,
    #[serde(default)]
    pub p2p_addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_until_ms: Option<u64>,
    #[serde(default)]
    pub port_mapping: PortMapping,
    #[serde(default)]
    pub relays: Vec<RelayInUse>,
    #[serde(default)]
    pub proxy: ProxyState,
    #[serde(default)]
    pub nearby: NearbyState,
    /// The Tor onion carrier's state, while raven-node reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tor: Option<TorState>,
    #[serde(default)]
    pub carry: Carry,
    #[serde(default)]
    pub help: HelpFlags,
    #[serde(default)]
    pub hide_ip: bool,
    #[serde(default)]
    pub lab_switches: Vec<String>,
    #[serde(default)]
    pub texts: NetworkTexts,
    /// The profile's `node_policy.json` could not be read: the defaults
    /// are shown.
    #[serde(default)]
    pub policy_unreadable: bool,
}

impl NetworkStatus {
    /// One way (or protection) by its id.
    pub fn way(&self, id: &str) -> Option<&Way> {
        self.carriers
            .iter()
            .chain(self.protections.iter())
            .find(|w| w.id == id)
    }
}

impl SanitizeDisplay for NetworkStatus {
    fn sanitize_display(&mut self) {
        self.carriers
            .iter_mut()
            .for_each(SanitizeDisplay::sanitize_display);
        self.protections
            .iter_mut()
            .for_each(SanitizeDisplay::sanitize_display);
        self.listening.iter_mut().for_each(sanitize::inline);
        sanitize::inline_opt(&mut self.internet_listen);
        sanitize::inline_opt(&mut self.p2p_listen);
        self.p2p_addresses.iter_mut().for_each(sanitize::inline);
        sanitize::inline(&mut self.port_mapping.setting);
        sanitize::inline(&mut self.port_mapping.router);
        sanitize::inline_opt(&mut self.port_mapping.v4);
        sanitize::inline_opt(&mut self.port_mapping.v6);
        for r in &mut self.relays {
            sanitize::inline(&mut r.relay);
            sanitize::inline(&mut r.state);
            sanitize::inline(&mut r.source);
            sanitize::inline_opt(&mut r.friend_address);
        }
        sanitize::inline(&mut self.proxy.setting);
        sanitize::inline(&mut self.proxy.in_use);
        self.proxy.lines.iter_mut().for_each(sanitize::inline);
        sanitize::inline(&mut self.nearby.state);
        sanitize::inline_opt(&mut self.nearby.radio);
        sanitize::inline_opt(&mut self.nearby.address_privacy);
        for row in &mut self.nearby.rows {
            sanitize::inline(&mut row.label);
            sanitize::inline(&mut row.value);
        }
        if let Some(tor) = &mut self.tor {
            sanitize::inline(&mut tor.phase);
            sanitize::inline(&mut tor.words);
            sanitize::inline_opt(&mut tor.why);
            sanitize::inline_opt(&mut tor.retry);
            sanitize::inline_opt(&mut tor.outages);
        }
        self.carry
            .letters_left
            .iter_mut()
            .for_each(sanitize::inline);
        self.carry
            .carry_for_friends
            .handoff_lines
            .iter_mut()
            .for_each(sanitize::inline);
        sanitize::multiline(&mut self.texts.carry_for_friends);
        sanitize::multiline(&mut self.texts.friends_carry_mine);
        sanitize::multiline(&mut self.texts.handoff);
    }
}

/// What `network.set` changes; only the given fields are sent.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct NetworkPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lan: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lan_for: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internet: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internet_listen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p2p: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p2p_listen: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relays: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_mapping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carry_for_friends: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friends_carry_mine: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_for_friends: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_ip: Option<bool>,
    /// The person said yes to `NetworkStatus::texts` for this change.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub agreed: bool,
}

/// One change `network.set` made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkChange {
    #[serde(default)]
    pub setting: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub effect: String,
    /// `now` or `restart`.
    #[serde(default)]
    pub applies: String,
    #[serde(default)]
    pub detail: Vec<String>,
}

/// Result of `network.set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkSetResult {
    #[serde(default)]
    pub changes: Vec<NetworkChange>,
    #[serde(default)]
    pub restart_needed: bool,
    #[serde(default)]
    pub status: NetworkStatus,
}

impl SanitizeDisplay for NetworkSetResult {
    fn sanitize_display(&mut self) {
        for c in &mut self.changes {
            sanitize::inline(&mut c.effect);
            c.detail.iter_mut().for_each(sanitize::inline);
        }
        self.status.sanitize_display();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SummaryRow {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CheckRow {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain: Option<String>,
}

/// Result of `network.check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct NetworkCheck {
    #[serde(default)]
    pub summary: Vec<SummaryRow>,
    #[serde(default)]
    pub next_step: String,
    #[serde(default)]
    pub rows: Vec<CheckRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub router: Option<Vec<CheckRow>>,
}

impl SanitizeDisplay for NetworkCheck {
    fn sanitize_display(&mut self) {
        for r in &mut self.summary {
            sanitize::inline(&mut r.label);
            sanitize::inline(&mut r.value);
        }
        sanitize::inline(&mut self.next_step);
        for r in self.rows.iter_mut().chain(self.router.iter_mut().flatten()) {
            sanitize::inline(&mut r.value);
            sanitize::inline_opt(&mut r.plain);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RouteEntry {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub via: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    #[serde(default)]
    pub state: CarrierState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LearnedRoutes {
    #[serde(default)]
    pub relays: u32,
    #[serde(default)]
    pub direct: u32,
    #[serde(default)]
    pub mail_friends: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_id: Option<String>,
    #[serde(default)]
    pub peer_id_proven: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_port: Option<u16>,
    #[serde(default)]
    pub expires_at_ms: u64,
}

/// A friend named in a contact's mail friends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FriendRef {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub takes_handoffs: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Coverage {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub until: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContactCarriers {
    #[serde(default)]
    pub hints: CarrierState,
    #[serde(default)]
    pub told: Vec<FriendRef>,
    #[serde(default)]
    pub told_unknown: u32,
    #[serde(default)]
    pub listed: Vec<FriendRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// How long a message written now could wait while the friend is away,
    /// by the rule the terminal app and `raven contact list` use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub away: Option<AwayWait>,
    #[serde(default)]
    pub receipts_stranded: bool,
}

/// `carriers.away`: until a time, or not at all (meet first).
/// `first_message`: none can because Raven hasn't connected with the friend
/// yet (a first message needs you both online).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AwayWait {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meet_first: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_message: Option<bool>,
}

/// `lan_now`: where Raven reaches the friend on the local network now, and
/// whether the saved LAN address is another, old one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LanNow {
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub at_ms: u64,
    #[serde(default)]
    pub saved_stale: bool,
}

/// Result of `contacts.routes`: never the friend's words or old code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ContactRoutes {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub hide_ip: bool,
    #[serde(default)]
    pub routes: Vec<RouteEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learned: Option<LearnedRoutes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lan_now: Option<LanNow>,
    #[serde(default)]
    pub carriers: ContactCarriers,
}

impl SanitizeDisplay for ContactRoutes {
    fn sanitize_display(&mut self) {
        for r in &mut self.routes {
            sanitize::inline(&mut r.label);
            sanitize::inline(&mut r.value);
            r.via.iter_mut().for_each(sanitize::inline);
            sanitize::inline_opt(&mut r.next);
        }
        if let Some(l) = &mut self.learned {
            sanitize::inline_opt(&mut l.peer_id);
        }
        if let Some(l) = &mut self.lan_now {
            sanitize::inline(&mut l.addr);
        }
        if let Some(c) = &mut self.carriers.coverage {
            sanitize::inline(&mut c.kind);
            sanitize::inline(&mut c.name);
            sanitize::inline(&mut c.until);
        }
        for f in self
            .carriers
            .told
            .iter_mut()
            .chain(self.carriers.listed.iter_mut())
        {
            sanitize::inline_opt(&mut f.name);
        }
    }
}

/// What `contacts.set_routes` changes for one friend.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RoutesPatch {
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p2p: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clear: Vec<String>,
    /// Mail friends by address; `Some(vec![])` clears them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custodians: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_ip: Option<bool>,
}

/// Result of `contacts.set_routes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetRoutesResult {
    pub contact: Contact,
    pub routes: ContactRoutes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SecurityNotice {
    #[serde(default)]
    pub id: i64,
    /// `alert` or `warning`.
    #[serde(default)]
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub what_raven_did: String,
    #[serde(default)]
    pub what_to_do: String,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub details: Vec<LabelValue>,
}

/// Result of `security.status` (`raven security`, read-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SecurityStatus {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub lab: bool,
    /// `off`, `unsealed` or `live`.
    #[serde(default)]
    pub kept: String,
    #[serde(default)]
    pub notices: Vec<SecurityNotice>,
    #[serde(default)]
    pub counted: u32,
    /// What the running background program counted (when it answers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<SecurityCounts>,
}

/// `SecurityStatus.node`: the background program's own counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SecurityCounts {
    #[serde(default)]
    pub strangers_refused_1h: u64,
    #[serde(default)]
    pub failed_handshakes_1h: u64,
    #[serde(default)]
    pub bans_active: u64,
    #[serde(default)]
    pub link_tag_failures_24h: u64,
    #[serde(default)]
    pub alerts_open: u64,
}

impl SanitizeDisplay for SecurityStatus {
    fn sanitize_display(&mut self) {
        sanitize::inline(&mut self.kept);
        for n in &mut self.notices {
            sanitize::inline(&mut n.level);
            sanitize::inline(&mut n.name);
            sanitize::inline(&mut n.headline);
            sanitize::inline(&mut n.what_raven_did);
            sanitize::inline(&mut n.what_to_do);
            sanitize::inline(&mut n.when);
            for d in &mut n.details {
                sanitize::inline(&mut d.label);
                sanitize::inline(&mut d.value);
            }
        }
    }
}

/// Result of `lock.state`, `lock.now` and `lock.unlock` (Lock v2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LockState {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub lab: bool,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_passphrase: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl SanitizeDisplay for LockState {
    fn sanitize_display(&mut self) {
        sanitize::inline_opt(&mut self.note);
    }
}

/// Result of `updates.status` (`raven update status`, read-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UpdatesStatus {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub bundles: bool,
    #[serde(default)]
    pub from_friends: bool,
    #[serde(default)]
    pub lab: bool,
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offer: Option<Vec<String>>,
    /// Always `cli_only` in this version.
    #[serde(default)]
    pub install: String,
    #[serde(default)]
    pub install_text: String,
}

impl SanitizeDisplay for UpdatesStatus {
    fn sanitize_display(&mut self) {
        self.lines.iter_mut().for_each(sanitize::inline);
        sanitize::inline_opt(&mut self.summary);
        self.offer.iter_mut().flatten().for_each(sanitize::inline);
        sanitize::inline(&mut self.install_text);
    }
}

#[derive(Deserialize)]
struct ContactsResult {
    contacts: Vec<Contact>,
}

#[derive(Deserialize)]
struct ContactResult {
    contact: Contact,
}

#[derive(Deserialize)]
struct MessagesResult {
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct MessageResult {
    message: Message,
}

#[derive(Deserialize)]
struct GroupMessagesResult {
    messages: Vec<GroupMessage>,
}

#[derive(Deserialize)]
struct GroupMessageResult {
    message: GroupMessage,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct GroupResult {
    group: Group,
}

#[derive(Deserialize)]
struct MarkReadResult {
    #[serde(default)]
    unread: u32,
}

#[derive(Deserialize)]
struct HintResult {
    #[serde(default)]
    word: Option<String>,
}

#[derive(Deserialize)]
struct CheckResult {
    result: CheckOutcome,
}

#[derive(Deserialize)]
struct OldCodeResult {
    result: OldCodeOutcome,
}

// ---------------------------------------------------------------- engine

/// The engine boundary. `call` is the whole contract; the typed wrappers are
/// conveniences that build the arguments, decode the result and sanitise its
/// display text.
pub trait RavenEngine: Send + Sync {
    /// One request: `op` with `args` (a JSON object). Returns the `result`
    /// object or the engine's error.
    fn call(&self, op: &str, args: Value) -> Result<Value, ApiError>;

    fn app_info(&self) -> Result<AppInfo, ApiError> {
        decode(self.call("app.info", json!({}))?).map(sanitized)
    }

    fn profile_state(&self) -> Result<ProfileState, ApiError> {
        decode(self.call("profile.state", json!({}))?).map(sanitized)
    }

    fn identity_create(&self, display_name: Option<&str>) -> Result<NewIdentity, ApiError> {
        let args = match display_name {
            Some(n) => json!({ "display_name": n }),
            None => json!({}),
        };
        decode(self.call("identity.create", args)?).map(sanitized)
    }

    fn identity_me(&self) -> Result<Me, ApiError> {
        decode(self.call("identity.me", json!({}))?).map(sanitized)
    }

    fn card_make(&self, include_address: bool) -> Result<Card, ApiError> {
        decode(self.call("card.make", json!({ "include_address": include_address }))?)
            .map(sanitized)
    }

    fn card_parse(&self, text: &str) -> Result<ParsedCard, ApiError> {
        decode(self.call("card.parse", json!({ "text": text }))?).map(sanitized)
    }

    fn contacts_list(&self) -> Result<Vec<Contact>, ApiError> {
        let r: ContactsResult = decode(self.call("contacts.list", json!({}))?)?;
        Ok(r.contacts.into_iter().map(sanitized).collect())
    }

    fn contacts_add(&self, text: &str, name: &str) -> Result<Contact, ApiError> {
        let r: ContactResult =
            decode(self.call("contacts.add", json!({ "text": text, "name": name }))?)?;
        Ok(sanitized(r.contact))
    }

    fn contacts_rename(&self, address: &str, name: &str) -> Result<Contact, ApiError> {
        let r: ContactResult = decode(self.call(
            "contacts.rename",
            json!({ "address": address, "name": name }),
        )?)?;
        Ok(sanitized(r.contact))
    }

    fn contacts_remove(&self, address: &str) -> Result<(), ApiError> {
        self.call("contacts.remove", json!({ "address": address }))
            .map(|_| ())
    }

    fn contacts_set_address(&self, address: &str, lan: &str) -> Result<Contact, ApiError> {
        let r: ContactResult = decode(self.call(
            "contacts.set_address",
            json!({ "address": address, "lan": lan }),
        )?)?;
        Ok(sanitized(r.contact))
    }

    fn verify_check_words(
        &self,
        address: &str,
        words: &[String],
    ) -> Result<CheckOutcome, ApiError> {
        let r: CheckResult = decode(self.call(
            "verify.check_words",
            json!({ "address": address, "words": words }),
        )?)?;
        Ok(r.result)
    }

    /// The old letters-and-digits code, for friends with an older Raven. On
    /// a match the engine pins the friend as verified, as with the words.
    fn verify_check_old_code(&self, address: &str, code: &str) -> Result<OldCodeOutcome, ApiError> {
        let r: OldCodeResult = decode(self.call(
            "verify.check_old_code",
            json!({ "address": address, "code": code }),
        )?)?;
        Ok(r.result)
    }

    fn verify_word_hint(&self, prefix: &str) -> Result<Option<String>, ApiError> {
        let r: HintResult = decode(self.call("verify.word_hint", json!({ "prefix": prefix }))?)?;
        Ok(r.word
            .map(|w| sanitize::sanitize_inline(&w))
            .filter(|w| !w.is_empty()))
    }

    fn messages_history(
        &self,
        address: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Message>, ApiError> {
        let mut args = json!({ "address": address, "limit": limit });
        if let Some(b) = before {
            args["before"] = json!(b);
        }
        let r: MessagesResult = decode(self.call("messages.history", args)?)?;
        Ok(r.messages.into_iter().map(sanitized).collect())
    }

    fn messages_send(&self, address: &str, text: &str) -> Result<Message, ApiError> {
        let r: MessageResult =
            decode(self.call("messages.send", json!({ "address": address, "text": text }))?)?;
        Ok(sanitized(r.message))
    }

    fn messages_retry(&self, id: &str) -> Result<Message, ApiError> {
        let r: MessageResult = decode(self.call("messages.retry", json!({ "id": id }))?)?;
        Ok(sanitized(r.message))
    }

    fn messages_stop(&self, id: &str) -> Result<Message, ApiError> {
        let r: MessageResult = decode(self.call("messages.stop", json!({ "id": id }))?)?;
        Ok(sanitized(r.message))
    }

    /// `messages.mark_read`: the friend's chat is read up to `up_to` (default
    /// its newest message), here and in the terminal app; never moved back.
    /// The friend's unread count afterwards.
    fn messages_mark_read(&self, address: &str, up_to: Option<&str>) -> Result<u32, ApiError> {
        let mut args = json!({ "address": address });
        if let Some(id) = up_to {
            args["id"] = json!(id);
        }
        let r: MarkReadResult = decode(self.call("messages.mark_read", args)?)?;
        Ok(r.unread)
    }

    fn messages_details(&self, id: &str) -> Result<MessageDetails, ApiError> {
        decode(self.call("messages.details", json!({ "id": id }))?).map(sanitized)
    }

    fn node_status(&self) -> Result<NodeStatus, ApiError> {
        decode(self.call("node.status", json!({}))?).map(sanitized)
    }

    fn node_start(&self) -> Result<NodeStatus, ApiError> {
        decode(self.call("node.start", json!({}))?).map(sanitized)
    }

    fn node_stop(&self) -> Result<NodeStatus, ApiError> {
        decode(self.call("node.stop", json!({}))?).map(sanitized)
    }

    /// `node.stop` with what happened (`stop`, `note`), for the restart
    /// prompt: a login service's background program is left running.
    fn node_stop_report(&self) -> Result<NodeStop, ApiError> {
        decode(self.call("node.stop", json!({}))?).map(sanitized)
    }

    fn settings_get(&self) -> Result<Settings, ApiError> {
        decode(self.call("settings.get", json!({}))?).map(sanitized)
    }

    fn settings_set(&self, patch: &SettingsPatch) -> Result<Settings, ApiError> {
        let args = serde_json::to_value(patch).map_err(|e| ApiError::internal(e.to_string()))?;
        decode(self.call("settings.set", args)?).map(sanitized)
    }

    fn events_poll(&self, after_seq: u64) -> Result<Poll, ApiError> {
        let mut p: Poll = decode(self.call("events.poll", json!({ "after_seq": after_seq }))?)?;
        p.events
            .iter_mut()
            .for_each(SanitizeDisplay::sanitize_display);
        Ok(p)
    }

    fn privacy_lock(&self) -> Result<ProfileState, ApiError> {
        decode(self.call("privacy.lock", json!({}))?).map(sanitized)
    }

    fn privacy_unlock(&self) -> Result<ProfileState, ApiError> {
        decode(self.call("privacy.unlock", json!({}))?).map(sanitized)
    }

    fn privacy_state(&self) -> Result<PrivacyState, ApiError> {
        decode(self.call("privacy.state", json!({}))?)
    }

    /// Encrypt everything Raven stores on this computer. Read
    /// [`RavenEngine::privacy_state`] afterwards for the new state.
    fn privacy_encrypt(&self) -> Result<(), ApiError> {
        self.call("privacy.encrypt", json!({})).map(|_| ())
    }

    /// Remove Raven's data from this computer (`profile.state` is `new`
    /// afterwards). The user confirmed by typing "erase".
    fn privacy_erase(&self) -> Result<(), ApiError> {
        self.call("privacy.erase", json!({ "confirm": "erase" }))
            .map(|_| ())
    }

    /// Every way to reach friends and what the background program does.
    fn network_status(&self) -> Result<NetworkStatus, ApiError> {
        decode(self.call("network.status", json!({}))?).map(sanitized)
    }

    /// Changes network settings (checked first: a held way is
    /// NOT_AVAILABLE_YET, a setting without the person's yes is
    /// CONSENT_NEEDED).
    fn network_set(&self, patch: &NetworkPatch) -> Result<NetworkSetResult, ApiError> {
        let args = serde_json::to_value(patch).map_err(|e| ApiError::internal(e.to_string()))?;
        decode(self.call("network.set", args)?).map(sanitized)
    }

    /// `raven doctor`'s summary and network rows; `router` also asks the
    /// router (read-only, up to about 4 s).
    fn network_check(&self, router: bool) -> Result<NetworkCheck, ApiError> {
        decode(self.call("network.check", json!({ "router": router }))?).map(sanitized)
    }

    fn contacts_routes(&self, address: &str) -> Result<ContactRoutes, ApiError> {
        decode(self.call("contacts.routes", json!({ "address": address }))?).map(sanitized)
    }

    fn contacts_set_routes(&self, patch: &RoutesPatch) -> Result<SetRoutesResult, ApiError> {
        let args = serde_json::to_value(patch).map_err(|e| ApiError::internal(e.to_string()))?;
        let mut r: SetRoutesResult = decode(self.call("contacts.set_routes", args)?)?;
        r.contact.sanitize_display();
        r.routes.sanitize_display();
        Ok(r)
    }

    fn security_status(&self) -> Result<SecurityStatus, ApiError> {
        decode(self.call("security.status", json!({}))?).map(sanitized)
    }

    fn lock_state(&self) -> Result<LockState, ApiError> {
        decode(self.call("lock.state", json!({}))?).map(sanitized)
    }

    /// Lock v2: the background program keeps running without its keys.
    fn lock_now(&self) -> Result<LockState, ApiError> {
        decode(self.call("lock.now", json!({}))?).map(sanitized)
    }

    /// Lock v2's unlock; the passphrase goes only to the background program.
    fn lock_unlock(&self, passphrase: Option<&str>) -> Result<LockState, ApiError> {
        let args = match passphrase {
            Some(p) => json!({ "passphrase": p }),
            None => json!({}),
        };
        decode(self.call("lock.unlock", args)?).map(sanitized)
    }

    /// `raven update status`; installing stays in the CLI.
    fn updates_status(&self) -> Result<UpdatesStatus, ApiError> {
        decode(self.call("updates.status", json!({}))?).map(sanitized)
    }

    /// `groups.list` (groups draft). An engine without the op answers an
    /// error: the caller treats that as "no groups in this build".
    fn groups_list(&self) -> Result<GroupsList, ApiError> {
        let mut r: GroupsList = decode(self.call("groups.list", json!({}))?)?;
        for g in &mut r.groups {
            g.sanitize_display();
        }
        Ok(r)
    }

    fn groups_get(&self, group: &str) -> Result<GroupDetail, ApiError> {
        decode(self.call("groups.get", json!({ "group": group }))?).map(sanitized)
    }

    fn groups_history(
        &self,
        group: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<Vec<GroupMessage>, ApiError> {
        let mut args = json!({ "group": group, "limit": limit });
        if let Some(b) = before {
            args["before"] = json!(b);
        }
        let r: GroupMessagesResult = decode(self.call("groups.history", args)?)?;
        Ok(r.messages.into_iter().map(sanitized).collect())
    }

    /// `groups.send`: the message (returned at once; raven-node delivers
    /// each copy) and, when raven-node could not be asked to send now, why
    /// (the copies wait).
    fn groups_send(
        &self,
        group: &str,
        text: &str,
    ) -> Result<(GroupMessage, Option<String>), ApiError> {
        let r: GroupMessageResult =
            decode(self.call("groups.send", json!({ "group": group, "text": text }))?)?;
        let note = r.note.map(|mut n| {
            sanitize::inline(&mut n);
            n
        });
        Ok((sanitized(r.message), note.filter(|n| !n.is_empty())))
    }

    /// `groups.create`: you become its admin; members are your friends.
    fn groups_create(&self, name: &str, members: &[String]) -> Result<GroupDetail, ApiError> {
        decode(self.call("groups.create", json!({ "name": name, "members": members }))?)
            .map(sanitized)
    }

    /// `groups.add` (admin only).
    fn groups_add(&self, group: &str, members: &[String]) -> Result<GroupDetail, ApiError> {
        decode(self.call("groups.add", json!({ "group": group, "members": members }))?)
            .map(sanitized)
    }

    /// `groups.remove` (admin only).
    fn groups_remove(&self, group: &str, member: &str) -> Result<GroupDetail, ApiError> {
        decode(self.call("groups.remove", json!({ "group": group, "member": member }))?)
            .map(sanitized)
    }

    /// `groups.leave` (not the admin): the history stays.
    fn groups_leave(&self, group: &str) -> Result<Group, ApiError> {
        let r: GroupResult = decode(self.call("groups.leave", json!({ "group": group }))?)?;
        Ok(sanitized(r.group))
    }

    fn groups_mark_read(&self, group: &str) -> Result<(), ApiError> {
        self.call("groups.mark_read", json!({ "group": group }))
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_envelope() {
        let r: Value = serde_json::from_str(&make_request("contacts.list", &json!({}))).unwrap();
        assert_eq!(r, json!({ "v": 1, "op": "contacts.list", "args": {} }));
    }

    #[test]
    fn success_and_failure_envelopes() {
        let ok =
            parse_response(r#"{"v":1,"ok":true,"result":{"x":1},"future_field":true}"#).unwrap();
        assert_eq!(ok, json!({ "x": 1 }));
        let ok_empty = parse_response(r#"{"v":1,"ok":true}"#).unwrap();
        assert_eq!(ok_empty, json!({}));

        let err = parse_response(
            r#"{"v":1,"ok":false,"error":{"code":"NAME_CLASH","message":"Pick\u202e another name.","detail":"d"}}"#,
        )
        .unwrap_err();
        assert_eq!(err.code, codes::NAME_CLASH);
        assert_eq!(err.message, "Pick another name.", "error text is sanitised");
        assert!(err.is(codes::NAME_CLASH));
    }

    #[test]
    fn malformed_envelopes_are_bad_response_errors() {
        for bad in [
            "",
            "not json",
            "[]",
            r#"{"ok":true}"#,
            r#"{"v":2,"ok":true}"#,
            r#"{"v":1}"#,
            r#"{"v":1,"ok":false}"#,
        ] {
            let e = parse_response(bad).unwrap_err();
            assert_eq!(e.code, codes::INTERNAL, "{bad}");
            assert_eq!(e.message, crate::strings::tr("error.badResponse"), "{bad}");
        }
    }

    #[test]
    fn secret_fields_are_refused() {
        assert!(contains_secret_field(&json!({ "seed": "x" })));
        assert!(contains_secret_field(
            &json!({ "a": [{ "b": { "Private_Key": 1 } }] })
        ));
        assert!(contains_secret_field(&json!({ "recovery_phrase": "x" })));
        assert!(contains_secret_field(&json!({ "plaintext": "x" })));
        assert!(!contains_secret_field(
            &json!({ "words": ["turtle"], "old_code": "x", "invite": "raven:" })
        ));
        let e = parse_response(r#"{"v":1,"ok":true,"result":{"identity_seed":"00"}}"#).unwrap_err();
        assert_eq!(e.code, codes::INTERNAL);
    }

    #[test]
    fn group_results_decode() {
        let list: GroupsList = decode(json!({
            "available": true,
            "groups": [{ "id": "ab", "name": "Trip\u{202e}", "members": 6, "unread": 2,
                         "status": "brand_new", "last_from": "Ali", "last_text": "hi" }]
        }))
        .unwrap();
        assert!(list.available);
        assert_eq!(list.groups[0].status, GroupStatus::Unknown);
        assert_eq!(list.groups[0].members, 6);
        // An engine without groups: nothing listed.
        assert_eq!(
            decode::<GroupsList>(json!({})).unwrap(),
            GroupsList::default()
        );
        let m: GroupMessage = decode(json!({
            "id": 7, "group": "ab", "from_address": "rvn1me", "from_name": "You", "mine": true,
            "text": "see you", "at_ms": 5, "delivered": 1, "total": 2,
            "receipts": [
                { "address": "rvn1a", "name": "Ali", "state": "delivered", "via": "direct" },
                { "address": "rvn1s", "name": "Sara", "state": "teleported" }
            ]
        }))
        .unwrap();
        assert_eq!(m.id, "7");
        assert_eq!(m.receipts[0].state, ReceiptState::Delivered);
        assert_eq!(m.receipts[1].state, ReceiptState::Unknown);
        assert_eq!(m.receipts[1].via, None);
        let p: Poll = decode(json!({
            "seq": 3,
            "events": [
                { "seq": 1, "kind": "group_changed", "group": "ab", "at_ms": 1 },
                { "seq": 2, "kind": "group_message", "group": "ab", "at_ms": 2,
                  "message": { "id": "g1", "group": "ab", "from_name": "Sara", "text": "x", "at_ms": 2 } },
                { "seq": 3, "kind": "outgoing", "at_ms": 3, "message": { "id": "m9", "address": "rvn1a",
                  "direction": "out", "text": "from the terminal", "at_ms": 3, "state": "sending" } }
            ]
        }))
        .unwrap();
        assert_eq!(
            p.events[0].kind,
            EventKind::GroupChanged { group: "ab".into() }
        );
        assert!(
            matches!(&p.events[1].kind, EventKind::GroupMessage { message, .. } if message.from_name == "Sara")
        );
        assert!(matches!(&p.events[2].kind, EventKind::Outgoing { message } if message.id == "m9"));
        let mut g = list.groups[0].clone();
        g.sanitize_display();
        assert_eq!(g.name, "Trip");
    }

    #[test]
    fn events_decode_including_unknown_kinds() {
        let p: Poll = decode(json!({
            "seq": 9,
            "events": [
                { "seq": 1, "kind": "status_ok", "at_ms": 10 },
                { "seq": 2, "kind": "send_started", "id": 42, "at_ms": 11 },
                { "seq": 3, "kind": "receipt", "id": "m1", "path": "LAN direct", "ms": 412, "at_ms": 12 },
                { "seq": 4, "kind": "presence", "address": "rvn1q", "presence": "in_touch", "at_ms": 13 },
                { "seq": 5, "kind": "state", "id": "m1", "state": "brand_new_state", "at_ms": 14 },
                { "seq": 6, "kind": "something_new", "at_ms": 15, "extra": [1, 2] },
                { "seq": 7, "kind": "node", "status": { "running": true, "receive": true }, "at_ms": 16 }
            ]
        }))
        .unwrap();
        assert_eq!(p.events.len(), 7);
        assert_eq!(p.events[0].kind, EventKind::StatusOk);
        assert_eq!(p.events[1].kind, EventKind::SendStarted { id: "42".into() });
        assert_eq!(
            p.events[2].kind,
            EventKind::Receipt {
                id: "m1".into(),
                path: "LAN direct".into(),
                path_kind: String::new(),
                ms: 412,
                delivered_at_ms: None,
            }
        );
        assert_eq!(
            p.events[3].kind,
            EventKind::Presence {
                address: "rvn1q".into(),
                presence: Presence::InTouch
            }
        );
        assert_eq!(
            p.events[4].kind,
            EventKind::State {
                id: "m1".into(),
                state: MessageState::Unknown
            }
        );
        assert_eq!(p.events[5].kind, EventKind::Unknown);
        match &p.events[6].kind {
            EventKind::Node { status } => assert!(status.is_starting()),
            other => panic!("{other:?}"),
        }
        assert!(!p.reset, "no reset unless the engine says so");
        let p: Poll = decode(json!({ "seq": 700, "events": [], "reset": true })).unwrap();
        assert!(p.reset);
    }

    #[test]
    fn event_round_trip() {
        let e = Event {
            seq: 3,
            at_ms: 5,
            kind: EventKind::Retry {
                id: "m2".into(),
                address: "a".into(),
                attempts: 2,
            },
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "retry");
        assert_eq!(v["attempts"], 2);
        let back: Event = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn settings_patch_sends_only_given_fields() {
        let p = SettingsPatch {
            keep_chats_days: Some(None),
            receive: Some(true),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({ "receive": true, "keep_chats_days": null })
        );
        let p = SettingsPatch {
            when_quit: Some(WhenQuit::StopIfStarted),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({ "when_quit": "stop_if_started" })
        );
    }

    #[test]
    fn new_results_decode() {
        let p: PrivacyState =
            decode(json!({ "available": false, "encrypted": false, "locked": false, "x": 1 }))
                .unwrap();
        assert_eq!(p, PrivacyState::default());
        let p: PrivacyState = decode(json!({ "available": true, "encrypted": true })).unwrap();
        assert!(p.available && p.encrypted && !p.locked);
        let r: OldCodeResult = decode(json!({ "result": "match" })).unwrap();
        assert_eq!(r.result, OldCodeOutcome::Match);
        let r: OldCodeResult = decode(json!({ "result": "typo" })).unwrap();
        assert_eq!(
            r.result,
            OldCodeOutcome::Unknown,
            "unknown values are tolerated"
        );
    }

    #[test]
    fn polling_intervals() {
        assert_eq!(poll::interval_ms(true), 500);
        assert_eq!(poll::interval_ms(false), 5_000);
        assert!(poll::due(true, None));
        assert!(poll::due(true, Some(500)));
        assert!(!poll::due(true, Some(499)));
        assert!(!poll::due(false, Some(500)));
        assert!(poll::due(false, Some(5_000)));
    }

    #[test]
    fn network_results_decode() {
        let status: NetworkStatus = decode(json!({
            "build": "release",
            "node": "running",
            "carriers": [
                {"id": "lan", "name": "Same network", "state": "on", "reason": "On.",
                 "lab": false, "gate": "LAN_DIRECT_PRODUCTION_ENABLED", "lab_switches": [],
                 "can_set": true, "setting": true, "running": true},
                {"id": "p2p", "state": "lab", "reason": "On for testing only.", "lab": true},
                {"id": "nearby", "state": "not_on_this_os", "reason": "No Bluetooth."},
                {"id": "future", "state": "something_new", "reason": ""}
            ],
            "protections": [{"id": "link_knock", "state": "preview", "reason": "Held."}],
            "listening": ["127.0.0.1:5000"],
            "port": 7420,
            "port_mapping": {"setting": "unset", "router": "", "nat_pmp_pcp": "preview"},
            "relays": [{"relay": "/ip4/127.0.0.1/tcp/1/p2p/X", "state": "reserved", "source": "manual"}],
            "help": {"bridge": true, "store": true, "relay": false, "automatic": true,
                     "effects": {"bridge": "b", "store": "s", "relay": "r"}},
            "hide_ip": false,
            "lab_switches": ["RAVEN_LAB_TEST_A"],
            "a_newer_field": 1
        }))
        .unwrap();
        assert_eq!(status.way("lan").unwrap().state, CarrierState::On);
        assert_eq!(status.way("p2p").unwrap().state, CarrierState::Lab);
        assert_eq!(
            status.way("nearby").unwrap().state,
            CarrierState::NotOnThisOs
        );
        assert_eq!(status.way("future").unwrap().state, CarrierState::Unknown);
        assert_eq!(status.relays[0].source, "manual");
        assert_eq!(status.port, 7420);
        let patch = NetworkPatch {
            store: Some(false),
            carry_for_friends: Some(true),
            agreed: true,
            ..NetworkPatch::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            json!({"store": false, "carry_for_friends": true, "agreed": true})
        );
        let routes = RoutesPatch {
            address: "rvn1q".into(),
            custodians: Some(Vec::new()),
            ..RoutesPatch::default()
        };
        assert_eq!(
            serde_json::to_value(&routes).unwrap(),
            json!({"address": "rvn1q", "custodians": []})
        );
        let m: Message = decode(json!({"id": "m", "address": "a", "direction": "out",
            "text": "t", "at_ms": 1, "state": "delivered", "path": "Held by a friend",
            "path_kind": "friend"}))
        .unwrap();
        assert_eq!(m.path_kind.as_deref(), Some("friend"));
        let lock: LockState =
            decode(json!({"available": false, "supported": false, "locked": false})).unwrap();
        assert!(!lock.available);
        let upd: UpdatesStatus =
            decode(json!({"available": false, "install": "cli_only", "lines": ["x"]})).unwrap();
        assert_eq!(upd.install, "cli_only");
    }

    #[test]
    fn node_status_helpers() {
        let mut s = NodeStatus {
            running: true,
            receive: true,
            ..Default::default()
        };
        assert!(s.is_starting());
        s.listening = Some("0.0.0.0:7420".into());
        assert!(!s.is_starting());
        assert_eq!(s.listening_port(), Some("7420"));
        assert!(!s.is_local_only());
        s.listening = Some("127.0.0.1:7420".into());
        assert!(s.is_local_only());
    }
}
