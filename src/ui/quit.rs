//! "Quit Raven?": explains what keeps running. Shown when "When I quit" is
//! "Ask me". Linux has no tray icon yet (StatusNotifierItem support is a
//! later step), so this dialog and the quit notification are how the app
//! says that the background program keeps receiving.

use gtk::prelude::*;
use raven_desktop::strings::tr;
use std::rc::Rc;

use super::app::Ctx;
use super::widgets::{self, dialog, heading, label, primary_button};

pub fn show(ctx: &Rc<Ctx>) {
    let parent = ctx.main_window();
    let (window, content) = dialog(parent.as_ref(), tr("quit.title"), &ctx.style, 440);
    content.append(&heading(tr("quit.title")));
    content.append(&label(tr("quit.running"), &[]));
    let row = widgets::action_row();
    let stay = widgets::button(tr("quit.stay"));
    let stop = widgets::button(tr("quit.quitAndStop"));
    let quit = primary_button(tr("quit.quit"));
    let w = window.downgrade();
    stay.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    let c = Rc::downgrade(ctx);
    stop.connect_clicked(move |_| {
        if let Some(ctx) = c.upgrade() {
            ctx.quit_and_stop();
        }
    });
    let c = Rc::downgrade(ctx);
    quit.connect_clicked(move |_| {
        if let Some(ctx) = c.upgrade() {
            // The dialog already said what keeps running: no notification.
            ctx.app.quit();
        }
    });
    row.append(&stay);
    row.append(&stop);
    row.append(&quit);
    content.append(&row);
    window.present();
    quit.grab_focus();
}
