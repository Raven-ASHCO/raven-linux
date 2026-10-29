//! The Calm chat: the 66-point header (avatar, name, Verified, status ·
//! path, the path pill, Routes or Members), the route line, the messages
//! (day lines, bubbles with their time, tick and path chip, group sender
//! names and receipt rings, the away card), the toast pill above the
//! composer ("New messages below" while you read older ones), and the
//! rounded composer with its round send button and the encryption line.
//!
//! This module builds and updates widgets only; what they show comes from
//! the main window (`main_window.rs`), which holds the engine's answers.

use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::{GroupMessage, Message, MessageState};
use raven_desktop::groupview;
use raven_desktop::icons::Icon;
use raven_desktop::present::{self, TickIcon, TickTone};
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::{avatar_index, tokens};
use raven_desktop::timefmt;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::calm::{self, Chip, Clamp, GroupRing, TickMark};
use super::route_line::RouteLine;
use super::style::Style;
use super::widgets::{self, after, hbox, label, line, vbox, Avatar, AvatarSize, OneShot};

/// Bubble text wraps at about this many characters (the tokens' 72 % of the
/// 760-point column).
const BUBBLE_CHARS: i32 = 46;

/// "13:26 (UTC+1)": the clock of a moment with this computer's zone (day lines say the day).
pub fn clock(at_ms: u64) -> String {
    timefmt::short_clock(at_ms, at_ms)
}

fn set_tone(w: &impl IsA<gtk::Widget>, tone: TickTone) {
    for c in ["ok", "wait", "bad"] {
        w.as_ref().remove_css_class(c);
    }
    if let Some(c) = tone.css() {
        w.as_ref().add_css_class(c);
    }
}

// ---------------------------------------------------------------- rows

/// An outgoing 1:1 message's tick: icon and text, a button that opens
/// Message details.
pub struct Tick {
    pub button: gtk::Button,
    mark: Rc<TickMark>,
    text: gtk::Label,
}

impl Tick {
    pub fn set(&self, m: &Message, friend: &str, arriving: bool) {
        let (text, tone) = present::tick(m, friend, timefmt::now_ms());
        self.text.set_text(&text);
        self.button.set_visible(!text.is_empty());
        set_tone(&self.button, tone);
        self.mark.set(present::tick_icon(m.state), arriving);
        super::a11y::set_label(&self.button, &text);
    }
}

/// One message on screen.
pub struct MsgRow {
    pub root: gtk::Box,
    pub tick: Option<Tick>,
    pub chip: Chip,
    /// Group messages of yours: the ring, "Delivered to 3 of 5", its note,
    /// and the members' lines under Details.
    pub group: Option<GroupMine>,
}

pub struct GroupMine {
    ring: Rc<GroupRing>,
    text: gtk::Label,
    note: gtk::Label,
    details: gtk::Revealer,
    toggle: gtk::Button,
    grid: gtk::Grid,
    /// The members' states last shown (to mark the one that changed).
    last: RefCell<Vec<raven_desktop::api::ReceiptState>>,
}

impl GroupMine {
    pub fn set(&self, m: &GroupMessage, style: &Rc<Style>) {
        self.ring.set(groupview::ring(m));
        let (text, tone, note) = groupview::tick(m);
        self.text.set_text(&text);
        set_tone(&self.text, tone);
        match note {
            Some(n) => {
                self.note.set_text(&format!("\u{00B7} {n}"));
                self.note.set_visible(true);
            }
            None => self.note.set_visible(false),
        }
        super::a11y::set_label(&self.ring.area, &text);
        // The members' lines.
        let before = self
            .last
            .replace(m.receipts.iter().map(|r| r.state).collect());
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        for (i, r) in m.receipts.iter().enumerate() {
            let who = hbox(8);
            who.add_css_class("r");
            let av = Avatar::sized(&r.name, &r.address, AvatarSize::Small);
            who.append(&av.root);
            who.append(&line(&r.name, &[]));
            let (state, tone) = groupview::receipt_line(r);
            let st = hbox(6);
            st.add_css_class("rs");
            set_tone(&st, tone);
            let mark = TickMark::new(style);
            mark.set(
                match r.state {
                    raven_desktop::api::ReceiptState::Delivered => TickIcon::Delivered,
                    raven_desktop::api::ReceiptState::Sent => TickIcon::Sent,
                    raven_desktop::api::ReceiptState::Failed => TickIcon::Problem,
                    _ => TickIcon::Waiting,
                },
                false,
            );
            st.append(&mark.root);
            st.append(&line(&state, &[]));
            if before.get(i).is_some_and(|b| *b != r.state) && style.motion_allowed() {
                st.add_css_class("new");
            }
            self.grid.attach(&who, 0, i as i32, 1, 1);
            self.grid.attach(&st, 1, i as i32, 1, 1);
        }
    }

    fn toggle(&self) {
        let open = !self.details.reveals_child();
        self.details.set_reveal_child(open);
        let text = if open {
            tr("common.hide")
        } else {
            tr("message.details")
        };
        self.toggle.set_label(text);
    }
}

fn bubble(text: &str) -> gtk::Box {
    let b = vbox(0);
    b.add_css_class("bubble");
    let l = label(text, &[]);
    l.set_selectable(true);
    l.set_max_width_chars(BUBBLE_CHARS);
    l.set_natural_wrap_mode_word();
    b.append(&l);
    b
}

trait NaturalWrap {
    fn set_natural_wrap_mode_word(&self);
}

impl NaturalWrap for gtk::Label {
    /// Wrap at words; very long words break anywhere (URLs, codes).
    fn set_natural_wrap_mode_word(&self) {
        self.set_wrap(true);
        self.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    }
}

fn row_root(out: bool) -> gtk::Box {
    let root = vbox(0);
    root.add_css_class("msg");
    root.add_css_class(if out { "out" } else { "in" });
    root.set_halign(if out {
        gtk::Align::End
    } else {
        gtk::Align::Start
    });
    root
}

fn meta_row(out: bool) -> gtk::Box {
    let meta = hbox(8);
    meta.add_css_class("meta");
    meta.set_halign(if out {
        gtk::Align::End
    } else {
        gtk::Align::Start
    });
    meta
}

/// A 1:1 message. `on_details` opens Message details (its tick is a button).
pub fn message_row(m: &Message, style: &Rc<Style>, on_details: impl Fn() + 'static) -> MsgRow {
    let out = m.direction == raven_desktop::api::Direction::Out;
    let root = row_root(out);
    root.append(&bubble(&m.text));
    let meta = meta_row(out);
    meta.append(&line(&clock(m.at_ms), &[]));
    let tick = out.then(|| {
        let mark = TickMark::new(style);
        let text = line("", &[]);
        let inner = hbox(5);
        inner.append(&mark.root);
        inner.append(&text);
        let button = gtk::Button::new();
        button.add_css_class("tick");
        button.set_child(Some(&inner));
        button.connect_clicked(move |_| on_details());
        meta.append(&button);
        Tick { button, mark, text }
    });
    let chip = Chip::new(false);
    meta.append(&chip.root);
    root.append(&meta);
    MsgRow {
        root,
        tick,
        chip,
        group: None,
    }
}

impl MsgRow {
    /// Update the tick, the path chip and the outline of a waiting bubble.
    /// `arriving`: a signed receipt just came (the check pops, the chip
    /// slides in).
    pub fn update(&self, m: &Message, friend: &str, details: bool, arriving: bool, style: &Style) {
        if let Some(t) = &self.tick {
            t.set(m, friend, arriving);
        }
        let pending = m.state == MessageState::Waiting;
        let problem = matches!(
            m.state,
            MessageState::Failed | MessageState::TimeRanOut | MessageState::NotSentMeetFirst
        );
        if pending {
            self.root.add_css_class("pending");
        } else {
            self.root.remove_css_class("pending");
        }
        if problem {
            self.root.add_css_class("problem");
        } else {
            self.root.remove_css_class("problem");
        }
        // The path chip once the way is known (never while sending).
        let show_chip = !matches!(m.state, MessageState::Sending | MessageState::Sent);
        match present::path_chip(m.path_kind.as_deref(), m.path.as_deref()).filter(|_| show_chip) {
            Some((short, words)) => {
                let text = if details {
                    m.path.clone().unwrap_or(short)
                } else {
                    short
                };
                let was = self.chip.root.is_visible();
                self.chip
                    .set(&text, present::path_style(m.path_kind.as_deref()), &words);
                if arriving && !was {
                    calm::play_class(&self.chip.root, style, "slide", tokens().calm.chip_slide);
                }
            }
            None => self.chip.hide(),
        }
    }
}

/// A group message: a member's (their name in their avatar colour, a small
/// avatar beside the bubble, "Passed on by …" when another member brought
/// it) or yours (the ring and "Delivered to 3 of 5", Details).
pub fn group_row(m: &GroupMessage, style: &Rc<Style>) -> MsgRow {
    let root = row_root(m.mine);
    let chip = Chip::new(false);
    if !m.mine {
        let from = line(&m.from_name, &["from"]);
        from.add_css_class(&format!("fg-{}", avatar_index(&m.from_address)));
        from.set_margin_start(30);
        root.append(&from);
        let row = hbox(8);
        let av = Avatar::sized(&m.from_name, &m.from_address, AvatarSize::Small);
        av.root.set_valign(gtk::Align::End);
        row.append(&av.root);
        row.append(&bubble(&m.text));
        root.append(&row);
        let meta = meta_row(false);
        meta.set_margin_start(30);
        meta.append(&line(&clock(m.at_ms), &[]));
        if let Some(via) = m.via_name.as_deref().filter(|v| !v.is_empty()) {
            meta.append(&line(&trf("group.passedOn", &[("name", via)]), &[]));
        }
        root.append(&meta);
        return MsgRow {
            root,
            tick: None,
            chip,
            group: None,
        };
    }
    root.append(&bubble(&m.text));
    let meta = meta_row(true);
    meta.append(&line(&clock(m.at_ms), &[]));
    let ring = GroupRing::new(style);
    meta.append(&ring.area);
    let text = line("", &["strong"]);
    text.add_css_class("tick");
    meta.append(&text);
    let note = line("", &[]);
    meta.append(&note);
    let toggle = gtk::Button::with_label(tr("message.details"));
    toggle.add_css_class("disc");
    meta.append(&toggle);
    root.append(&meta);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(10);
    grid.set_row_spacing(4);
    grid.add_css_class("recips");
    let details = gtk::Revealer::new();
    details.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    details.set_transition_duration(style.duration(250));
    details.set_child(Some(&grid));
    details.set_halign(gtk::Align::End);
    root.append(&details);
    let group = GroupMine {
        ring,
        text,
        note,
        details,
        toggle: toggle.clone(),
        grid,
        last: RefCell::new(Vec::new()),
    };
    group.set(m, style);
    MsgRow {
        root,
        tick: None,
        chip,
        group: Some(group),
    }
}

impl MsgRow {
    /// Connect the Details toggle of a group message of yours.
    pub fn connect_group_details(row: &Rc<MsgRow>) {
        let Some(g) = &row.group else { return };
        let weak = Rc::downgrade(row);
        g.toggle.connect_clicked(move |_| {
            if let Some(r) = weak.upgrade() {
                if let Some(g) = &r.group {
                    g.toggle();
                }
            }
        });
    }
}

/// A day line: "Today", "Yesterday", "Wednesday", "Mon 26 Oct".
pub fn day_line(text: &str) -> gtk::Box {
    let b = hbox(12);
    b.add_css_class("day");
    for i in 0..2 {
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_hexpand(true);
        sep.set_valign(gtk::Align::Center);
        b.append(&sep);
        if i == 0 {
            let l = gtk::Label::new(Some(text));
            b.append(&l);
        }
    }
    b
}

/// "{name} is back · your waiting message was delivered".
pub fn back_note(text: &str, style: &Rc<Style>) -> gtk::Box {
    let b = hbox(8);
    b.add_css_class("backnote");
    let mark = TickMark::new(style);
    mark.set(TickIcon::Delivered, false);
    b.append(&mark.root);
    b.append(&label(text, &[]));
    b
}

// ---------------------------------------------------------------- the away card

/// The card under a friend's waiting messages: who waits and until when,
/// who can carry it (only when the engine says a friend can), Try now,
/// Details (the engine's own Status, Why and What you can do), Stop trying
/// (when exactly one message waits).
pub struct AwayCard {
    pub root: gtk::Box,
    pub try_now: gtk::Button,
    pub details: gtk::Button,
    pub stop: gtk::Button,
    det: gtk::Revealer,
    det_box: gtk::Box,
    trio: gtk::Box,
    coverage: gtk::Label,
    title: gtk::Label,
    body: gtk::Label,
}

impl AwayCard {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let root = vbox(14);
        root.add_css_class("away");
        let top = hbox(18);
        let texts = vbox(4);
        texts.set_hexpand(true);
        let title = calm::heading_label("", &["away-title"]);
        let body = label("", &["away-body"]);
        let coverage = label("", &["away-body"]);
        coverage.set_visible(false);
        texts.append(&title);
        texts.append(&body);
        texts.append(&coverage);
        top.append(&texts);
        let trio = hbox(0);
        trio.set_valign(gtk::Align::Center);
        trio.set_visible(false);
        top.append(&trio);
        root.append(&top);
        let acts = hbox(8);
        let try_now = widgets::button(tr("message.tryNow"));
        let details = widgets::button(tr("message.details"));
        let stop = widgets::quiet_button(tr("message.stopTrying"));
        acts.append(&try_now);
        acts.append(&details);
        acts.append(&stop);
        root.append(&acts);
        let det_box = vbox(4);
        det_box.add_css_class("away-det");
        let det = gtk::Revealer::new();
        det.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        det.set_transition_duration(style.duration(250));
        det.set_child(Some(&det_box));
        root.append(&det);
        let card = Rc::new(Self {
            root,
            try_now,
            details,
            stop,
            det,
            det_box,
            trio,
            coverage,
            title,
            body,
        });
        let weak = Rc::downgrade(&card);
        card.details.connect_clicked(move |_| {
            if let Some(c) = weak.upgrade() {
                let open = !c.det.reveals_child();
                c.det.set_reveal_child(open);
                c.details.set_label(if open {
                    tr("common.hide")
                } else {
                    tr("message.details")
                });
            }
        });
        card
    }

    pub fn set(&self, name: &str, until_ms: u64, count: u32) {
        let now = timefmt::now_ms();
        self.title.set_text(&trf("away.title", &[("name", name)]));
        self.body.set_text(&trf(
            "away.body",
            &[("time", &timefmt::deadline(now, until_ms)), ("name", name)],
        ));
        self.stop.set_visible(count == 1);
    }

    /// Who can carry the message (`contacts.routes` coverage): the trio
    /// You ── holder ··· friend, only with a named friend.
    pub fn set_coverage(
        &self,
        friend: &str,
        friend_address: &str,
        coverage: Option<(String, String, String)>,
    ) {
        while let Some(c) = self.trio.first_child() {
            self.trio.remove(&c);
        }
        let Some((kind, holder, until)) = coverage else {
            self.coverage.set_visible(false);
            self.trio.set_visible(false);
            return;
        };
        self.coverage.set_text(&trf(
            "network.routes.coverage",
            &[("coverage", &holder), ("until", &until)],
        ));
        self.coverage.set_visible(true);
        if kind == "none" {
            self.trio.set_visible(false);
            return;
        }
        let node = |name: &str, av: Rc<Avatar>, faded: bool| {
            let b = vbox(4);
            b.add_css_class("trio-node");
            b.set_size_request(56, -1);
            if faded {
                av.root.set_opacity(0.55);
            }
            b.append(&av.root);
            let l = line(name, &[]);
            l.set_xalign(0.5);
            b.append(&l);
            b
        };
        let link = |dashed: bool| {
            let l = hbox(0);
            l.add_css_class("trio-link");
            if dashed {
                l.add_css_class("dashed");
            }
            l.set_valign(gtk::Align::Center);
            l
        };
        self.trio
            .append(&node(tr("you.title"), Avatar::you(tr("you.title")), false));
        self.trio.append(&link(false));
        self.trio.append(&node(
            &holder,
            Avatar::sized(&holder, &holder, AvatarSize::Small),
            false,
        ));
        self.trio.append(&link(true));
        self.trio.append(&node(
            friend,
            Avatar::sized(friend, friend_address, AvatarSize::Small),
            true,
        ));
        self.trio.set_visible(true);
    }

    /// The engine's Status / Why / What you can do for the newest waiting
    /// message.
    pub fn set_details(&self, status: &str, why: &str, todo: &str) {
        while let Some(c) = self.det_box.first_child() {
            self.det_box.remove(&c);
        }
        for (title, text) in [
            (tr("away.status"), status),
            (tr("away.why"), why),
            (tr("away.todo"), todo),
        ] {
            if text.is_empty() {
                continue;
            }
            let l = hbox(6);
            let t = line(&format!("{title}."), &["strong"]);
            t.set_valign(gtk::Align::Start);
            l.append(&t);
            let body = label(text, &[]);
            body.set_hexpand(true);
            l.append(&body);
            self.det_box.append(&l);
        }
    }
}

// ---------------------------------------------------------------- toast

/// The pill that says what just happened for a few seconds (bottom centre,
/// above the composer in a chat): "Delivered to Ali via LAN direct in
/// 0.4 s · receipt signed by Ali". In a chat it also says "New messages
/// below ↓" while the person reads older messages; a click then scrolls down.
pub struct Toast {
    pub revealer: gtk::Revealer,
    button: gtk::Button,
    mark: Rc<TickMark>,
    text: gtk::Label,
    timer: OneShot,
    /// The pill says "New messages below".
    pub jump: Cell<bool>,
}

impl Toast {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let mark = TickMark::new(style);
        let text = line("", &[]);
        let inner = hbox(7);
        inner.append(&mark.root);
        inner.append(&text);
        let button = gtk::Button::new();
        button.add_css_class("toast");
        button.set_child(Some(&inner));
        button.set_focus_on_click(false);
        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        revealer.set_transition_duration(style.duration(tokens().calm.toast_in));
        revealer.set_child(Some(&button));
        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::End);
        let t = Rc::new(Self {
            revealer,
            button,
            mark,
            text,
            timer: OneShot::default(),
            jump: Cell::new(false),
        });
        let weak = Rc::downgrade(&t);
        t.button.connect_clicked(move |_| {
            if let Some(t) = weak.upgrade() {
                if !t.jump.get() {
                    t.hide();
                }
            }
        });
        t
    }

    /// `tone`: "ok", "wait", "bad" or "" (with the matching icon).
    pub fn show(&self, text: &str, tone: &str) {
        self.jump.set(false);
        for c in ["ok", "wait", "bad", "jump"] {
            self.button.remove_css_class(c);
        }
        if !tone.is_empty() {
            self.button.add_css_class(tone);
        }
        let icon = match tone {
            "ok" => TickIcon::Delivered,
            "wait" => TickIcon::Waiting,
            "bad" => TickIcon::Problem,
            _ => TickIcon::None,
        };
        self.mark.set(icon, false);
        self.text.set_text(text);
        super::a11y::set_label(&self.button, text);
        self.revealer.set_reveal_child(true);
        let r = self.revealer.downgrade();
        self.timer.start(tokens().durations.toast_visible, move || {
            if let Some(r) = r.upgrade() {
                r.set_reveal_child(false);
            }
        });
    }

    /// "New messages below ↓": stays until the person scrolls down or
    /// clicks it.
    pub fn show_jump(&self) {
        self.timer.cancel();
        self.jump.set(true);
        for c in ["ok", "wait", "bad"] {
            self.button.remove_css_class(c);
        }
        self.button.add_css_class("jump");
        self.mark.set(TickIcon::None, false);
        self.text
            .set_text(&format!("{} \u{2193}", tr("chat.newMessages")));
        super::a11y::set_label(&self.button, tr("chat.newMessages"));
        self.revealer.set_reveal_child(true);
    }

    pub fn hide(&self) {
        self.timer.cancel();
        self.jump.set(false);
        self.revealer.set_reveal_child(false);
    }

    pub fn connect_clicked(&self, f: impl Fn() + 'static) {
        self.button.connect_clicked(move |_| f());
    }
}

// ---------------------------------------------------------------- the view

/// The widgets of the chat page.
pub struct ChatView {
    pub root: gtk::Box,
    // Header.
    pub avatar_slot: gtk::Box,
    pub name: gtk::Label,
    pub verified: gtk::Box,
    verified_icon: calm::IconView,
    verified_text: gtk::Label,
    pub sub: gtk::Label,
    pub pill: Chip,
    pub routes: gtk::Button,
    pub members: gtk::Button,
    // Route line.
    pub route: Rc<RouteLine>,
    // Messages.
    pub scroller: gtk::ScrolledWindow,
    pub list: gtk::Box,
    pub tail: gtk::Box,
    pub empty: gtk::Label,
    /// The pill above the composer.
    pub toast: Rc<Toast>,
    // Composer.
    pub banner: gtk::Revealer,
    pub banner_text: gtk::Label,
    pub banner_action: gtk::Button,
    pub closed: gtk::Label,
    pub composer_box: gtk::Box,
    pub entry: gtk::Entry,
    pub send: gtk::Button,
    pub e2e: gtk::Label,
    style: Rc<Style>,
}

impl ChatView {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let t = tokens();
        // Header.
        let head = hbox(12);
        head.add_css_class("chat-head");
        let avatar_slot = hbox(0);
        avatar_slot.set_valign(gtk::Align::Center);
        head.append(&avatar_slot);
        let titles = vbox(2);
        titles.set_valign(gtk::Align::Center);
        titles.set_hexpand(true);
        let title_row = hbox(8);
        let name = line("", &["chat-title"]);
        title_row.append(&name);
        let verified = hbox(4);
        let verified_icon = calm::IconView::new(Icon::ShieldCheck, 14);
        let verified_text = line("", &[]);
        verified.append(&verified_icon.area);
        verified.append(&verified_text);
        verified.set_valign(gtk::Align::Center);
        title_row.append(&verified);
        titles.append(&title_row);
        let sub = line("", &["chat-sub"]);
        titles.append(&sub);
        head.append(&titles);
        let pill = Chip::new(true);
        head.append(&pill.root);
        let routes = widgets::icon_button(Icon::Route, tr("network.routes.button"));
        routes.set_valign(gtk::Align::Center);
        head.append(&routes);
        let members = widgets::icon_button(Icon::Net, tr("group.membersButton"));
        members.set_valign(gtk::Align::Center);
        members.set_visible(false);
        head.append(&members);

        let route = RouteLine::new(style);

        // Messages, at most 760 points wide.
        let list = vbox(4);
        list.add_css_class("msgs");
        list.set_valign(gtk::Align::End);
        let empty = label("", &["empty-chat"]);
        empty.set_xalign(0.5);
        empty.set_justify(gtk::Justification::Center);
        empty.set_visible(false);
        let tail = vbox(0);
        let column = vbox(0);
        column.append(&empty);
        column.append(&list);
        column.append(&tail);
        column.set_valign(gtk::Align::End);
        column.set_vexpand(true);
        let clamp = Clamp::new(t.layout.content_max_width, &column);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&clamp));

        // The toast pill over the bottom of the messages.
        let toast = Toast::new(style);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scroller));
        overlay.add_overlay(&toast.revealer);
        overlay.set_vexpand(true);

        // Composer.
        let banner_text = label("", &[]);
        banner_text.set_hexpand(true);
        let banner_action = widgets::primary_button(tr("home.banner.notVerified.action"));
        banner_action.set_valign(gtk::Align::Center);
        let banner_box = hbox(12);
        banner_box.add_css_class("calm-banner");
        banner_box.append(&calm::icon(Icon::ShieldCheck, 16));
        banner_box.append(&banner_text);
        banner_box.append(&banner_action);
        let banner = gtk::Revealer::new();
        banner.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        banner.set_transition_duration(style.duration(250));
        banner.set_child(Some(&banner_box));
        let closed = label("", &["group-banner"]);
        closed.set_visible(false);
        let entry = gtk::Entry::new();
        entry.add_css_class("composer-entry");
        entry.set_hexpand(true);
        let send = gtk::Button::new();
        send.add_css_class("send");
        send.set_child(Some(&calm::icon(Icon::Send, 16)));
        send.set_tooltip_text(Some(tr("home.send")));
        super::a11y::set_label(&send, tr("home.send"));
        send.set_valign(gtk::Align::Center);
        let composer_box = hbox(10);
        composer_box.add_css_class("composer");
        composer_box.append(&entry);
        composer_box.append(&send);
        let e2e = line("", &[]);
        let e2e_row = hbox(6);
        e2e_row.add_css_class("e2e");
        e2e_row.set_halign(gtk::Align::Center);
        e2e_row.append(&calm::icon(Icon::Lock, 12));
        e2e_row.append(&e2e);
        let comp = vbox(0);
        comp.append(&banner);
        comp.append(&closed);
        comp.append(&composer_box);
        comp.append(&e2e_row);
        let comp_clamp = Clamp::new(t.layout.content_max_width, &comp);
        let comp_wrap = vbox(0);
        comp_wrap.add_css_class("composer-wrap");
        comp_wrap.append(&comp_clamp);

        let root = vbox(0);
        root.append(&head);
        root.append(&route.root);
        root.append(&overlay);
        root.append(&comp_wrap);

        Rc::new(Self {
            root,
            avatar_slot,
            name,
            verified,
            verified_icon,
            verified_text,
            sub,
            pill,
            routes,
            members,
            route,
            scroller,
            list,
            tail,
            empty,
            toast,
            banner,
            banner_text,
            banner_action,
            closed,
            composer_box,
            entry,
            send,
            e2e,
            style: style.clone(),
        })
    }

    pub fn set_verified(&self, verified: Option<bool>) {
        for c in ["verified", "unverified"] {
            self.verified.remove_css_class(c);
        }
        match verified {
            Some(true) => {
                self.verified.add_css_class("verified");
                self.verified_icon.area.set_visible(true);
                self.verified_text.set_text(tr("home.verified"));
                self.verified.set_visible(true);
            }
            Some(false) => {
                self.verified.add_css_class("unverified");
                self.verified_icon.area.set_visible(false);
                self.verified_text.set_text(tr("home.notVerified"));
                self.verified.set_visible(true);
            }
            None => self.verified.set_visible(false),
        }
    }

    /// Empty the message list (another chat opens).
    pub fn clear(&self) {
        widgets::clear_box(&self.list);
        widgets::clear_box(&self.tail);
        self.toast.hide();
    }

    /// Add a widget at the end of the messages.
    pub fn push(&self, w: &impl IsA<gtk::Widget>) {
        self.list.append(w);
    }

    /// Lift your new bubble out of the composer (bubbleLift, curve lift).
    pub fn lift(&self, row: &gtk::Box) {
        calm::play_class(row, &self.style, "lift", tokens().calm.bubble_lift);
    }

    /// An arriving bubble rises in and keeps a soft ring for a moment.
    pub fn arrive(&self, row: &gtk::Box) {
        if !self.style.motion_allowed() {
            return;
        }
        calm::play_class(row, &self.style, "incoming", tokens().durations.bubble_in);
        row.add_css_class("fresh");
        let r = row.downgrade();
        after(tokens().calm.incoming_highlight, move || {
            if let Some(r) = r.upgrade() {
                r.remove_css_class("fresh");
            }
        });
    }

    /// Restart the page's entrance (viewIn) when another chat opens.
    pub fn enter(&self) {
        calm::play_class(&self.root, &self.style, "view-in", tokens().calm.view_in);
    }
}

/// Run `f` once the next frame has laid the list out (to scroll to the
/// newest message after appending).
pub fn after_layout(w: &impl IsA<gtk::Widget>, f: impl FnOnce() + 'static) {
    let f = RefCell::new(Some(f));
    w.as_ref().add_tick_callback(move |_, _| {
        if let Some(f) = f.borrow_mut().take() {
            f();
        }
        glib::ControlFlow::Break
    });
}
