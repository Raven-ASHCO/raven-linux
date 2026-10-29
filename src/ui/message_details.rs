//! Message details: status, why, what you can do; Try now / Stop trying /
//! Send again, and the technical text under Details. All texts come from the
//! engine (`messages.details`), sanitised.

use gtk::prelude::*;
use raven_desktop::api::{ApiError, Message, MessageDetails, MessageState, RavenEngine};
use raven_desktop::strings::{tr, trf};
use raven_desktop::timefmt;
use std::rc::Rc;

use super::app::Ctx;
use super::widgets::{self, details_expander, dialog, heading, label, primary_button};

/// What a button asks the engine (run on the engine thread).
#[derive(Clone)]
enum Action {
    Stop(String),
    Retry(String),
    SendAgain { address: String, text: String },
}

impl Action {
    fn run(self, e: &dyn RavenEngine) -> Result<Message, ApiError> {
        match self {
            Action::Stop(id) => e.messages_stop(&id),
            Action::Retry(id) => e.messages_retry(&id),
            Action::SendAgain { address, text } => e.messages_send(&address, &text),
        }
    }
}

pub fn show(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    id: &str,
    friend: &str,
    on_change: Box<dyn Fn(Message)>,
) {
    let job_id = id.to_string();
    let (parent, friend) = (parent.downgrade(), friend.to_string());
    ctx.call(
        move |e| e.messages_details(&job_id),
        move |ctx, result| {
            let Some(parent) = parent.upgrade() else {
                return;
            };
            match result {
                Ok(d) => show_details(ctx, &parent, d, &friend, on_change),
                Err(e) => {
                    let (w, content) =
                        dialog(Some(&parent), tr("message.details"), &ctx.style, 420);
                    content.append(&label(&e.message, &["danger"]));
                    content.append(&details_expander(&e.detail));
                    w.present();
                }
            }
        },
    );
}

fn show_details(
    ctx: &Rc<Ctx>,
    parent: &gtk::Window,
    d: MessageDetails,
    friend: &str,
    on_change: Box<dyn Fn(Message)>,
) {
    let when = timefmt::relative_past(timefmt::now_ms(), d.message.at_ms);
    let title = trf("message.title", &[("name", friend), ("when", &when)]);
    let (window, content) = dialog(Some(parent), &title, &ctx.style, 460);
    content.append(&heading(&title));
    let quoted = label(&d.message.text, &["dim"]);
    quoted.set_lines(3);
    quoted.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&quoted);
    for text in [&d.status_text, &d.why_text, &d.action_text] {
        if !text.is_empty() {
            content.append(&label(text, &[]));
        }
    }
    content.append(&details_expander(&d.detail));

    let on_change = Rc::new(on_change);
    let row = widgets::action_row();
    let close = widgets::button(tr("common.close"));
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    row.append(&close);

    // One action per call: the engine's answer replaces the message.
    let action = |text: &str, primary: bool, action: Action| {
        let b = if primary {
            primary_button(text)
        } else {
            widgets::button(text)
        };
        let ctx = Rc::downgrade(ctx);
        let w = window.downgrade();
        let on_change = on_change.clone();
        let error = content.downgrade();
        b.connect_clicked(move |b| {
            let Some(ctx) = ctx.upgrade() else { return };
            b.set_sensitive(false);
            let action = action.clone();
            let (b, w, error, on_change) =
                (b.downgrade(), w.clone(), error.clone(), on_change.clone());
            ctx.call(
                move |e| action.run(e),
                move |_, result| match result {
                    Ok(m) => {
                        on_change(m);
                        if let Some(w) = w.upgrade() {
                            w.close();
                        }
                    }
                    Err(e) => {
                        if let Some(b) = b.upgrade() {
                            b.set_sensitive(true);
                        }
                        if let Some(content) = error.upgrade() {
                            content.append(&label(&e.message, &["danger"]));
                        }
                    }
                },
            );
        });
        b
    };
    let m = d.message.clone();
    match m.state {
        MessageState::Waiting => {
            row.append(&action(
                tr("message.stopTrying"),
                false,
                Action::Stop(m.id.clone()),
            ));
            row.append(&action(
                tr("message.tryNow"),
                true,
                Action::Retry(m.id.clone()),
            ));
        }
        MessageState::TimeRanOut | MessageState::Failed | MessageState::NotSentMeetFirst => {
            row.append(&action(
                tr("message.sendAgain"),
                true,
                Action::SendAgain {
                    address: m.address.clone(),
                    text: m.text.clone(),
                },
            ));
        }
        _ => {}
    }
    content.append(&row);
    window.present();
}
