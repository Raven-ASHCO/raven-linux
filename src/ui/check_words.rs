//! "Check it's really {name}": your words with the read-aloud highlight
//! (word 10 marked "check"); the friend's words typed as neutral chips, the
//! field completing a word after 3 letters. There is no right/wrong mark per
//! word (a chip only means "a Raven word"); the result comes only at the end
//! from the engine: match, typo ("one word doesn't fit") or mismatch. The
//! friend's words are never shown. After three mismatches the dialog stops
//! offering Check (blind-entry rule 4).
//!
//! For a friend with an older Raven, a small link switches to the old code:
//! your old code to read aloud and one plain field for theirs (no chips, no
//! completion, no per-character marks). `verify.check_old_code` answers
//! match or mismatch; a match verifies the friend exactly as the words do.
//! Mismatches count towards the same limit of three.
//!
//! Every engine call (word completion included) runs on the engine thread;
//! an answer for text that has changed since is dropped.

use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::{ApiError, CheckOutcome, Contact, Me, OldCodeOutcome};
use raven_desktop::strings::{tr, trf};
use raven_desktop::theme::tokens;
use raven_desktop::words::{self, TokenShape, WORD_COUNT};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::app::Ctx;
use super::widgets::{
    self, dialog, flat_button, hbox, heading, label, primary_button, sp, vbox, WordGrid, MD, SM, XS,
};

const MAX_MISMATCHES: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Words,
    OldCode,
}

/// One engine answer for a typed token (`None`: not looked up, the shape
/// already says what is wrong).
type Lookup = Option<Result<Option<String>, ApiError>>;

struct CheckWords {
    ctx: Rc<Ctx>,
    me: Weak<CheckWords>,
    window: gtk::Window,
    contact: Contact,
    mode: Cell<Mode>,
    intro: gtk::Label,
    words_box: gtk::Box,
    chips: RefCell<Vec<(String, gtk::FlowBoxChild)>>,
    flow: gtk::FlowBox,
    progress: gtk::Label,
    entry: gtk::Entry,
    hint: gtk::Label,
    status: gtk::Label,
    old_link: gtk::Button,
    old_box: gtk::Box,
    old_entry: gtk::Entry,
    words_link: gtk::Button,
    result: gtk::Revealer,
    result_title: gtk::Label,
    result_body: gtk::Label,
    check: gtk::Button,
    again: gtk::Button,
    later: gtk::Button,
    done: gtk::Button,
    guard: Cell<bool>,
    /// The completion on screen and the partial word it belongs to.
    hint_word: RefCell<Option<(String, String)>>,
    /// Counts edits of the words field; answers for older text are dropped.
    text_gen: Cell<u64>,
    checking: Cell<bool>,
    mismatches: Cell<u32>,
    on_verified: Box<dyn Fn()>,
    _grid: Option<Rc<WordGrid>>,
}

/// Open the dialog once your words (and old code) are here.
pub fn show(ctx: &Rc<Ctx>, parent: &gtk::Window, contact: &Contact, on_verified: Box<dyn Fn()>) {
    let (parent, contact) = (parent.downgrade(), contact.clone());
    ctx.call(
        |e| e.identity_me(),
        move |ctx, me| {
            if let Some(parent) = parent.upgrade() {
                build(ctx, &parent, &contact, me, on_verified);
            }
        },
    );
}

fn build(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    contact: &Contact,
    me: Result<Me, ApiError>,
    on_verified: Box<dyn Fn()>,
) {
    let name = contact.name.as_str();
    let (window, content) = dialog(
        Some(parent),
        &trf("check.title", &[("name", name)]),
        &ctx.style,
        560,
    );
    content.append(&heading(&trf("check.title", &[("name", name)])));
    let intro = label(
        &trf(words::check_intro_key(false), &[("name", name)]),
        &["dim"],
    );
    content.append(&intro);

    // ---- Words mode.
    let words_box = vbox(sp(SM));
    let yours = label(&trf("check.yours", &[("name", name)]), &["strong"]);
    yours.set_margin_top(sp(MD));
    words_box.append(&yours);
    let (grid, old_code) = match &me {
        Ok(me) => {
            let g = WordGrid::new(&me.words, &ctx.style);
            words_box.append(&g.root);
            (Some(g), me.old_code.clone())
        }
        Err(e) => {
            words_box.append(&label(&e.message, &["danger"]));
            (None, String::new())
        }
    };
    let theirs_row = hbox(sp(SM));
    theirs_row.set_margin_top(sp(MD));
    let theirs = label(&trf("check.theirs", &[("name", name)]), &["strong"]);
    theirs.set_hexpand(true);
    let progress = gtk::Label::new(Some(&trf("check.progress", &[("n", "0")])));
    progress.add_css_class("dim");
    theirs_row.append(&theirs);
    theirs_row.append(&progress);
    words_box.append(&theirs_row);
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(5);
    flow.set_min_children_per_line(2);
    flow.set_column_spacing(sp(XS) as u32);
    flow.set_row_spacing(sp(XS) as u32);
    words_box.append(&flow);
    let entry = gtk::Entry::new();
    entry.add_css_class("raven-field");
    let hint = label("", &["dim"]);
    hint.set_visible(false);
    let status = label("", &["warning"]);
    status.set_visible(false);
    words_box.append(&entry);
    words_box.append(&hint);
    words_box.append(&status);
    // A small secondary link for friends with an older Raven.
    let old_link = flat_button(tr("check.oldCode.link"));
    old_link.add_css_class("small");
    old_link.set_halign(gtk::Align::Start);
    words_box.append(&old_link);
    content.append(&words_box);

    // ---- Old-code mode.
    let old_box = vbox(sp(SM));
    old_box.set_visible(false);
    let old_yours = label(&trf("check.oldCode.yours", &[("name", name)]), &["strong"]);
    old_yours.set_margin_top(sp(MD));
    old_box.append(&old_yours);
    match &me {
        Ok(_) => {
            let code = label(&words::group_old_code(&old_code), &["mono"]);
            code.set_selectable(true);
            old_box.append(&code);
        }
        Err(e) => old_box.append(&label(&e.message, &["danger"])),
    }
    let old_theirs = label(&trf("check.oldCode.theirs", &[("name", name)]), &["strong"]);
    old_theirs.set_margin_top(sp(MD));
    old_box.append(&old_theirs);
    let old_entry = gtk::Entry::new();
    old_entry.add_css_class("raven-field");
    old_entry.add_css_class("mono");
    // GTK exposes the placeholder to screen readers (GTK 4.0 API).
    old_entry.set_placeholder_text(Some(tr("check.oldCode.field")));
    old_box.append(&old_entry);
    let words_link = flat_button(tr("check.oldCode.useWords"));
    words_link.add_css_class("small");
    words_link.set_halign(gtk::Align::Start);
    old_box.append(&words_link);
    content.append(&old_box);

    // ---- Result, only at the end.
    let result_box = vbox(sp(XS));
    let result_title = label("", &["strong"]);
    let result_body = label("", &[]);
    result_box.append(&result_title);
    result_box.append(&result_body);
    let result = gtk::Revealer::new();
    result.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    result.set_child(Some(&result_box));
    result.set_margin_top(sp(SM));
    content.append(&result);

    let row = widgets::action_row();
    let later = widgets::button(tr("check.later"));
    let again = widgets::button(tr("check.again"));
    again.set_visible(false);
    let check = primary_button(tr("check.verify"));
    check.set_sensitive(false);
    let done = primary_button(tr("common.done"));
    done.set_visible(false);
    row.append(&later);
    row.append(&again);
    row.append(&check);
    row.append(&done);
    content.append(&row);

    let cw = Rc::new_cyclic(|me| CheckWords {
        ctx: ctx.clone(),
        me: me.clone(),
        window: window.clone(),
        contact: contact.clone(),
        mode: Cell::new(Mode::Words),
        intro,
        words_box,
        chips: RefCell::new(Vec::new()),
        flow,
        progress,
        entry,
        hint,
        status,
        old_link,
        old_box,
        old_entry,
        words_link,
        result,
        result_title,
        result_body,
        check,
        again,
        later,
        done,
        guard: Cell::new(false),
        hint_word: RefCell::new(None),
        text_gen: Cell::new(0),
        checking: Cell::new(false),
        mismatches: Cell::new(0),
        on_verified,
        _grid: grid,
    });
    cw.wire();
    widgets::keep_alive(&window, cw.clone());
    window.present();
    cw.entry.grab_focus();
}

impl CheckWords {
    fn wire(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.entry.connect_changed(move |_| {
            if let Some(s) = me.upgrade() {
                if !s.guard.get() {
                    s.process(false, false);
                }
            }
        });
        let me = Rc::downgrade(self);
        self.entry.connect_activate(move |_| {
            if let Some(s) = me.upgrade() {
                s.process(true, true);
            }
        });
        // Tab picks the completion; Backspace in an empty field removes the
        // last word. Capture phase: before the text field handles the key.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, _| {
            let Some(s) = me.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if key == gtk::gdk::Key::Tab && s.current_hint().is_some() {
                s.process(true, false);
                return glib::Propagation::Stop;
            }
            if key == gtk::gdk::Key::BackSpace && s.entry.text().is_empty() {
                s.remove_last_chip();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.entry.add_controller(keys);

        let me = Rc::downgrade(self);
        self.old_entry.connect_changed(move |_| {
            if let Some(s) = me.upgrade() {
                s.update_check_sensitive();
            }
        });
        let me = Rc::downgrade(self);
        self.old_entry.connect_activate(move |_| {
            if let Some(s) = me.upgrade() {
                s.run_check();
            }
        });
        let me = Rc::downgrade(self);
        self.old_link.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.set_mode(Mode::OldCode);
            }
        });
        let me = Rc::downgrade(self);
        self.words_link.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.set_mode(Mode::Words);
            }
        });

        let me = Rc::downgrade(self);
        self.check.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.run_check();
            }
        });
        let me = Rc::downgrade(self);
        self.again.connect_clicked(move |_| {
            if let Some(s) = me.upgrade() {
                s.reset();
            }
        });
        let w = self.window.downgrade();
        let close = move |_: &gtk::Button| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        };
        self.later.connect_clicked(close.clone());
        self.done.connect_clicked(close);
    }

    fn name(&self) -> &str {
        &self.contact.name
    }

    // ------------------------------------------------------------ modes

    /// Words or old code. Switching clears the result of the other mode.
    fn set_mode(&self, mode: Mode) {
        if self.checking.get() || self.mode.replace(mode) == mode {
            return;
        }
        self.words_box.set_visible(mode == Mode::Words);
        self.old_box.set_visible(mode == Mode::OldCode);
        let intro = words::check_intro_key(mode == Mode::OldCode);
        self.intro.set_text(&trf(intro, &[("name", self.name())]));
        self.clear_result();
        match mode {
            Mode::Words => self.entry.grab_focus(),
            Mode::OldCode => self.old_entry.grab_focus(),
        };
    }

    fn stopped(&self) -> bool {
        self.mismatches.get() >= MAX_MISMATCHES
    }

    fn update_check_sensitive(&self) {
        let ready = match self.mode.get() {
            Mode::Words => self.chips.borrow().len() == WORD_COUNT,
            Mode::OldCode => !self.old_entry.text().trim().is_empty(),
        };
        self.check
            .set_sensitive(ready && !self.stopped() && !self.checking.get());
    }

    // ------------------------------------------------------------ words

    /// What the field says about one finished token, from its shape and
    /// the engine's completion. Never about the friend's words.
    fn token_result(&self, token: &str, looked_up: Lookup) -> Result<String, String> {
        match words::token_shape(token) {
            TokenShape::NotLatin => Err(tr("check.englishLetters").to_string()),
            TokenShape::TooShort => {
                let n = (self.chips.borrow().len() + 1).to_string();
                Err(trf("check.tooShort", &[("n", &n)]))
            }
            TokenShape::Lookup => match looked_up {
                Some(Ok(Some(word))) => Ok(word),
                Some(Ok(None)) | None => Err(trf("check.notARavenWord", &[("name", self.name())])),
                Some(Err(e)) => Err(e.message),
            },
        }
    }

    fn set_status(&self, text: Option<&str>) {
        match text {
            Some(t) => {
                self.status.set_text(t);
                self.status.set_visible(true);
            }
            None => self.status.set_visible(false),
        }
    }

    fn set_entry(&self, text: &str) {
        self.guard.set(true);
        self.entry.set_text(text);
        self.entry.set_position(-1);
        self.guard.set(false);
    }

    fn next_gen(&self) -> u64 {
        let g = self.text_gen.get() + 1;
        self.text_gen.set(g);
        g
    }

    /// The text changed (`accept` false), or Tab / Enter takes the word
    /// being typed (`accept` true; Enter also checks when all ten words are
    /// there). One engine job looks up every finished token and the word
    /// being typed; its answer is used only if the text is still the same.
    fn process(&self, accept: bool, check_after: bool) {
        let text = self.entry.text().to_string();
        let (finished, partial) = words::split_entry(&text);
        let generation = self.next_gen();
        if finished.is_empty() {
            // Nothing for the engine, or the completion is already here.
            if partial.is_empty() {
                self.set_status(None);
                self.set_hint(None);
                if accept && check_after {
                    self.run_check();
                }
                return;
            }
            if accept {
                if let Some(word) = self.current_hint() {
                    return self.take_partial(Ok(word), check_after);
                }
            }
            if words::token_shape(&partial) != TokenShape::Lookup {
                self.set_hint(None);
                if accept {
                    self.take_partial(self.token_result(&partial, None), check_after);
                }
                return;
            }
        }
        let me = self.me.clone();
        let (tokens, part) = (finished.clone(), partial.clone());
        self.ctx.run(
            move |e| {
                let look = |t: &String| {
                    (words::token_shape(t) == TokenShape::Lookup).then(|| e.verify_word_hint(t))
                };
                let lookups: Vec<Lookup> = tokens.iter().map(look).collect();
                (lookups, look(&part))
            },
            move |_, (lookups, partial_lookup)| {
                let Some(s) = me.upgrade() else { return };
                if s.text_gen.get() != generation {
                    return; // the text changed again: a newer answer comes
                }
                if !s.apply_finished(&finished, &partial, lookups) {
                    return; // a bad token stays in the field with a note
                }
                if accept {
                    let r = s.token_result(&partial, partial_lookup);
                    s.take_partial(r, check_after);
                } else {
                    let hint = partial_lookup.and_then(Result::ok).flatten();
                    s.set_hint(hint.map(|w| (partial, w)));
                }
            },
        );
    }

    /// Turn finished tokens into chips; keep the first bad one (and what
    /// follows) in the field with a note. False when a note is shown.
    fn apply_finished(&self, finished: &[String], partial: &str, lookups: Vec<Lookup>) -> bool {
        if finished.is_empty() {
            if partial.is_empty() {
                self.set_status(None);
            }
            return true;
        }
        let mut leftover: Vec<String> = Vec::new();
        let mut note = None;
        for (i, (tok, looked_up)) in finished.iter().zip(lookups).enumerate() {
            if self.chips.borrow().len() >= WORD_COUNT {
                break;
            }
            match self.token_result(tok, looked_up) {
                Ok(word) => self.add_chip(&word),
                Err(msg) => {
                    note = Some(msg);
                    leftover = finished[i..].to_vec();
                    break;
                }
            }
        }
        if !partial.is_empty() {
            leftover.push(partial.to_string());
        }
        let mut rest = leftover.join(" ");
        if note.is_some() && partial.is_empty() {
            rest.push(' ');
        }
        self.set_status(note.as_deref());
        self.set_entry(&rest);
        if note.is_some() {
            self.set_hint(None);
            return false;
        }
        true
    }

    fn set_hint(&self, hint: Option<(String, String)>) {
        match &hint {
            Some((_, w)) => {
                self.hint.set_text(&format!("\u{2192} {w}"));
                self.hint.set_visible(true);
            }
            None => self.hint.set_visible(false),
        }
        self.hint_word.replace(hint);
    }

    /// The completion, if it belongs to the word in the field now.
    fn current_hint(&self) -> Option<String> {
        let text = self.entry.text().to_string();
        let (_, partial) = words::split_entry(&text);
        self.hint_word
            .borrow()
            .as_ref()
            .filter(|(p, _)| *p == partial && !partial.is_empty())
            .map(|(_, w)| w.clone())
    }

    /// Take the word being typed as a chip, or say why not.
    fn take_partial(&self, chosen: Result<String, String>, check_after: bool) {
        match chosen {
            Ok(word) => {
                self.add_chip(&word);
                self.set_entry("");
                self.set_hint(None);
                self.set_status(None);
                if check_after {
                    self.run_check();
                }
            }
            Err(msg) => self.set_status(Some(&msg)),
        }
    }

    fn add_chip(&self, word: &str) {
        if self.chips.borrow().len() >= WORD_COUNT {
            return;
        }
        let n = self.chips.borrow().len() + 1;
        let chip = hbox(0);
        chip.add_css_class("chip");
        let num = gtk::Label::new(Some(&format!("{n} ")));
        num.add_css_class("dim");
        num.add_css_class("small");
        chip.append(&num);
        chip.append(&gtk::Label::new(Some(word)));
        // Neutral: it only says this is one of Raven's words.
        chip.set_tooltip_text(Some(tr("check.ravenWord")));
        let child = gtk::FlowBoxChild::new();
        child.set_child(Some(&chip));
        child.set_can_focus(false);
        self.flow.insert(&child, -1);
        self.chips.borrow_mut().push((word.to_string(), child));
        self.update_progress();
    }

    fn remove_last_chip(&self) {
        if self.result.reveals_child() || self.checking.get() {
            return;
        }
        let last = self.chips.borrow_mut().pop();
        if let Some((_, child)) = last {
            self.flow.remove(&child);
        }
        self.update_progress();
    }

    fn update_progress(&self) {
        let n = self.chips.borrow().len();
        self.progress
            .set_text(&trf("check.progress", &[("n", &n.to_string())]));
        self.update_check_sensitive();
    }

    // ------------------------------------------------------------ check

    fn run_check(&self) {
        if self.checking.get() || self.stopped() || self.result.reveals_child() {
            return;
        }
        let address = self.contact.address.clone();
        let me = self.me.clone();
        match self.mode.get() {
            Mode::Words => {
                if self.chips.borrow().len() != WORD_COUNT {
                    return;
                }
                self.begin_check();
                self.entry.set_sensitive(false);
                self.set_hint(None);
                self.set_status(Some(tr("check.checking")));
                self.status.remove_css_class("warning");
                self.status.add_css_class("dim");
                let words: Vec<String> =
                    self.chips.borrow().iter().map(|(w, _)| w.clone()).collect();
                self.ctx.call(
                    move |e| e.verify_check_words(&address, &words),
                    move |_, outcome| {
                        if let Some(s) = me.upgrade() {
                            s.finish_words(outcome);
                        }
                    },
                );
            }
            Mode::OldCode => {
                let code = self.old_entry.text().trim().to_string();
                if code.is_empty() {
                    return;
                }
                self.begin_check();
                self.old_entry.set_sensitive(false);
                self.ctx.call(
                    move |e| e.verify_check_old_code(&address, &code),
                    move |_, outcome| {
                        if let Some(s) = me.upgrade() {
                            s.finish_old_code(outcome);
                        }
                    },
                );
            }
        }
    }

    fn begin_check(&self) {
        self.checking.set(true);
        self.check.set_sensitive(false);
        self.old_link.set_sensitive(false);
        self.words_link.set_sensitive(false);
    }

    fn end_check(&self) {
        self.checking.set(false);
        self.old_link.set_sensitive(true);
        self.words_link.set_sensitive(true);
        self.status.remove_css_class("dim");
        self.status.add_css_class("warning");
        self.set_status(None);
        for cls in ["success", "warning", "danger"] {
            self.result_title.remove_css_class(cls);
        }
    }

    fn finish_words(&self, outcome: Result<CheckOutcome, ApiError>) {
        self.end_check();
        let name = self.name().to_string();
        let n = name.as_str();
        match outcome {
            Ok(CheckOutcome::Match) => self.matched(),
            Ok(CheckOutcome::Typo) => {
                self.result_title.set_text(tr("check.typo"));
                self.result_title.add_css_class("warning");
                self.result_body
                    .set_text(&trf("check.typo.action", &[("name", n)]));
                self.again.set_visible(true);
            }
            Ok(CheckOutcome::Mismatch) => {
                self.mismatched(&trf("check.mismatch", &[("name", n)]));
            }
            Ok(CheckOutcome::Unknown) => self.failed(tr("error.internal")),
            Err(e) => self.failed(&e.message),
        }
        self.reveal_result();
    }

    fn finish_old_code(&self, outcome: Result<OldCodeOutcome, ApiError>) {
        self.end_check();
        match outcome {
            Ok(OldCodeOutcome::Match) => self.matched(),
            Ok(OldCodeOutcome::Mismatch) => {
                self.mismatched(&trf("check.oldCode.mismatch", &[("name", self.name())]));
            }
            Ok(OldCodeOutcome::Unknown) => self.failed(tr("error.internal")),
            Err(e) => self.failed(&e.message),
        }
        self.reveal_result();
    }

    /// The same for words and the old code: the friend is now verified.
    fn matched(&self) {
        let n = self.name().to_string();
        self.result_title
            .set_text(&format!("\u{2713} {}", trf("check.match", &[("name", &n)])));
        self.result_title.add_css_class("success");
        self.result_body
            .set_text(&trf("check.match.save", &[("name", &n)]));
        self.check.set_visible(false);
        self.later.set_visible(false);
        self.again.set_visible(false);
        self.old_link.set_visible(false);
        self.words_link.set_visible(false);
        self.done.set_visible(true);
        (self.on_verified)();
    }

    fn mismatched(&self, title: &str) {
        let m = self.mismatches.get() + 1;
        self.mismatches.set(m);
        self.result_title.set_text(title);
        self.result_title.add_css_class("danger");
        if self.stopped() {
            self.result_body
                .set_text(&trf("check.stop", &[("name", self.name())]));
            self.check.set_visible(false);
            self.again.set_visible(false);
            self.old_link.set_visible(false);
            self.words_link.set_visible(false);
        } else {
            self.result_body.set_text("");
            self.again.set_visible(true);
        }
    }

    fn failed(&self, message: &str) {
        self.result_title.set_text(message);
        self.result_title.add_css_class("warning");
        self.result_body.set_text("");
        self.again.set_visible(true);
    }

    fn reveal_result(&self) {
        self.result
            .set_transition_duration(self.ctx.style.duration(tokens().durations.word_reveal));
        self.result.set_reveal_child(true);
    }

    fn clear_result(&self) {
        self.result.set_transition_duration(0);
        self.result.set_reveal_child(false);
        self.again.set_visible(false);
        if !self.stopped() {
            self.check.set_visible(true);
        }
        self.entry.set_sensitive(true);
        self.old_entry.set_sensitive(true);
        self.update_check_sensitive();
    }

    /// "Type again": clear what was typed in this mode and the result.
    fn reset(&self) {
        match self.mode.get() {
            Mode::Words => {
                let chips: Vec<_> = self.chips.borrow_mut().drain(..).collect();
                for (_, child) in chips {
                    self.flow.remove(&child);
                }
                self.set_entry("");
                self.set_status(None);
                self.set_hint(None);
                self.next_gen();
            }
            Mode::OldCode => self.old_entry.set_text(""),
        }
        self.clear_result();
        self.update_progress();
        match self.mode.get() {
            Mode::Words => self.entry.grab_focus(),
            Mode::OldCode => self.old_entry.grab_focus(),
        };
    }
}
