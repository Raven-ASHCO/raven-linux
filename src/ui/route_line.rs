//! The route line under the chat header (Calm): You ── path ── friend. The
//! track is solid while the friend is in touch and dashed while not; in its
//! middle, the way the last message travelled in its path colour. A packet
//! travels out on `send_started`, the signed receipt travels back as a green
//! check on `receipt`, and an incoming message travels in on `incoming`.
//!
//! It is still unless an engine event arrives: each event queues one run;
//! a frame-clock tick callback draws it and removes itself when the queue is
//! empty, so an idle window costs nothing. Under reduced motion the run is
//! skipped and its completion (for example the "Delivered" tick text) happens
//! at once.

use gtk::prelude::*;
use raven_desktop::motion::Packet;
use raven_desktop::present::PathStyle;
use raven_desktop::theme::tokens;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use super::calm::{self, IconView};
use super::style::Style;
use super::widgets::{hbox, line};

type Done = Box<dyn FnOnce()>;

struct Run {
    packet: Packet,
    start_us: Option<i64>,
    duration_us: i64,
    done: Option<Done>,
}

pub struct RouteLine {
    /// The whole line: "You", the track, the friend's name.
    pub root: gtk::Box,
    pub area: gtk::DrawingArea,
    you: gtk::Label,
    friend: gtk::Label,
    label_box: gtk::Box,
    label_icon: IconView,
    label: gtk::Label,
    label_class: Cell<Option<&'static str>>,
    dashed: Cell<bool>,
    style: Rc<Style>,
    queue: RefCell<VecDeque<(Packet, Option<Done>)>>,
    current: RefCell<Option<Run>>,
    progress: Cell<f64>,
    ticking: Cell<bool>,
    tick_id: RefCell<Option<gtk::TickCallbackId>>,
}

impl RouteLine {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let area = calm::drawing();
        area.set_content_height(tokens().route_line_height as i32);
        area.set_hexpand(true);
        area.add_css_class("route-line");
        area.set_can_target(false);
        let label_icon = IconView::new(raven_desktop::icons::Icon::Route, 13);
        let label = line("", &[]);
        let label_box = hbox(5);
        label_box.add_css_class("route-label");
        label_box.append(&label_icon.area);
        label_box.append(&label);
        label_box.set_halign(gtk::Align::Center);
        label_box.set_valign(gtk::Align::Center);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&area));
        overlay.add_overlay(&label_box);
        overlay.set_hexpand(true);
        let you = line("", &[]);
        let friend = line("", &[]);
        let root = hbox(10);
        root.add_css_class("route");
        root.append(&you);
        root.append(&overlay);
        root.append(&friend);
        let line = Rc::new(Self {
            root,
            area,
            you,
            friend,
            label_box,
            label_icon,
            label,
            label_class: Cell::new(None),
            dashed: Cell::new(false),
            style: style.clone(),
            queue: RefCell::new(VecDeque::new()),
            current: RefCell::new(None),
            progress: Cell::new(0.0),
            ticking: Cell::new(false),
            tick_id: RefCell::new(None),
        });
        let weak = Rc::downgrade(&line);
        line.area.set_draw_func(move |_, cr, w, h| {
            if let Some(line) = weak.upgrade() {
                line.draw(cr, w as f64, h as f64);
            }
        });
        // Leaving the screen finishes everything at once (texts update).
        let weak = Rc::downgrade(&line);
        line.area.connect_unmap(move |_| {
            if let Some(line) = weak.upgrade() {
                line.stop_ticking();
                line.flush();
            }
        });
        style.watch(&line, |line| line.area.queue_draw());
        line
    }

    /// What the line shows: its two ends, the label in the middle (empty:
    /// none) in a path's colour, and whether the track is dashed (the
    /// friend isn't in touch).
    pub fn set(&self, you: &str, friend: &str, text: &str, style: PathStyle, dashed: bool) {
        self.you.set_text(you);
        self.friend.set_text(friend);
        self.label.set_text(text);
        self.label_box.set_visible(!text.is_empty());
        if let Some(old) = self.label_class.replace(style.class) {
            self.label_box.remove_css_class(&format!("p-{old}"));
        }
        if let Some(new) = style.class {
            self.label_box.add_css_class(&format!("p-{new}"));
        }
        self.label_icon.set(style.icon);
        if self.dashed.replace(dashed) != dashed {
            self.area.queue_draw();
        }
    }

    /// Queue one run; `done` runs when it arrives (or at once under reduced
    /// motion, or when the line is not on screen).
    pub fn play(self: &Rc<Self>, packet: Packet, done: Option<Done>) {
        if !self.style.motion_allowed() || !self.area.is_mapped() {
            self.flush();
            if let Some(f) = done {
                f();
            }
            return;
        }
        self.queue.borrow_mut().push_back((packet, done));
        if !self.ticking.get() {
            self.ticking.set(true);
            let weak = Rc::downgrade(self);
            let id = self.area.add_tick_callback(move |_, clock| {
                let Some(line) = weak.upgrade() else {
                    return gtk::glib::ControlFlow::Break;
                };
                line.tick(clock.frame_time())
            });
            self.tick_id.replace(Some(id));
        }
    }

    fn stop_ticking(&self) {
        if let Some(id) = self.tick_id.take() {
            id.remove();
        }
        self.ticking.set(false);
    }

    /// Finish everything now (motion turned off, chat switched).
    pub fn flush(&self) {
        let mut pending: Vec<Done> = Vec::new();
        if let Some(run) = self.current.borrow_mut().take() {
            pending.extend(run.done);
        }
        pending.extend(self.queue.borrow_mut().drain(..).filter_map(|(_, d)| d));
        self.area.queue_draw();
        for f in pending {
            f();
        }
    }

    fn tick(&self, now_us: i64) -> gtk::glib::ControlFlow {
        if self.current.borrow().is_none() {
            let next = self.queue.borrow_mut().pop_front();
            match next {
                Some((packet, done)) => {
                    let ms = packet.duration_ms(&tokens().durations) as i64;
                    self.current.replace(Some(Run {
                        packet,
                        start_us: None,
                        duration_us: ms * 1000,
                        done,
                    }));
                }
                None => {
                    self.ticking.set(false);
                    self.tick_id.take();
                    self.area.queue_draw();
                    return gtk::glib::ControlFlow::Break;
                }
            }
        }
        let finished = {
            let mut cur = self.current.borrow_mut();
            let run = cur.as_mut().expect("a run is active");
            let start = *run.start_us.get_or_insert(now_us);
            let t = if self.style.motion_allowed() {
                (now_us - start) as f64 / run.duration_us.max(1) as f64
            } else {
                1.0
            };
            self.progress.set(t.min(1.0));
            t >= 1.0
        };
        self.area.queue_draw();
        if finished {
            let done = self.current.borrow_mut().take().and_then(|r| r.done);
            if let Some(f) = done {
                f();
            }
        }
        gtk::glib::ControlFlow::Continue
    }

    fn draw(&self, cr: &gtk::cairo::Context, w: f64, h: f64) {
        // The track: 2 points, solid while in touch, dashed while not.
        let mid = (h / 2.0).round();
        let (r, g, b, a) = self.style.color("borderStrong").to_f64();
        cr.set_source_rgba(r, g, b, a);
        cr.set_line_width(2.0);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        if self.dashed.get() {
            cr.set_dash(&[6.0, 5.0], 0.0);
        }
        cr.move_to(1.0, mid);
        cr.line_to(w - 1.0, mid);
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);

        let cur = self.current.borrow();
        let Some(run) = cur.as_ref() else { return };
        if run.start_us.is_none() {
            return;
        }
        let t = self.progress.get();
        let receipt = run.packet == Packet::ReceiptBack;
        let radius = if receipt { 7.0 } else { 5.0 };
        let x = radius + 1.0 + run.packet.position(t) * (w - 2.0 * radius - 2.0);
        let alpha = Packet::opacity(t);
        let tau = std::f64::consts::TAU;
        let fill = if receipt { "successFill" } else { "accentFill" };
        let (r, g, b, a) = self.style.color(fill).to_f64();
        // A soft halo, then the packet.
        cr.set_source_rgba(r, g, b, a * alpha * 0.2);
        cr.arc(x, mid, radius + 4.0, 0.0, tau);
        let _ = cr.fill();
        cr.set_source_rgba(r, g, b, a * alpha);
        cr.arc(x, mid, radius, 0.0, tau);
        let _ = cr.fill();
        if receipt {
            // The signed receipt: a white check in the green dot.
            cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
            cr.set_line_width(1.8);
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            cr.move_to(x - 3.0, mid + 0.2);
            cr.line_to(x - 0.8, mid + 2.4);
            cr.line_to(x + 3.2, mid - 2.0);
            let _ = cr.stroke();
        }
    }
}
