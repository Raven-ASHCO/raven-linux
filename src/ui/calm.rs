//! The Calm building blocks the screens share, in plain GTK 4 (no
//! libadwaita): icons and the dotted mark drawn with cairo in the text
//! colour, a container that keeps content at a readable width, frame-clock
//! tweens on the tokens' curves, the tick mark that fills when a signed
//! receipt comes, the group ring, the segmented control with its sliding
//! thumb, the gliding selection list, and grouped cards of rows.
//!
//! Every motion here obeys `Style::motion_allowed()` (the desktop's
//! Animations switch and Raven's Reduced motion): without it the end state
//! is drawn at once.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use raven_desktop::groupview::{self, Arc};
use raven_desktop::icons::{self, Icon, Op};
use raven_desktop::mark;
use raven_desktop::present::TickIcon;
use raven_desktop::theme::{tokens, Rgba};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::style::Style;
use super::widgets::{after, hbox, label, line, vbox};

// ---------------------------------------------------------------- drawing

/// The widget's CSS text colour (inherits from its parents: a chip's path
/// colour, a tick's tone).
fn css_color(w: &impl IsA<gtk::Widget>) -> (f64, f64, f64, f64) {
    let c = w.as_ref().style_context().color();
    (
        c.red() as f64,
        c.green() as f64,
        c.blue() as f64,
        c.alpha() as f64,
    )
}

fn trace(cr: &gtk::cairo::Context, ops: &[Op]) {
    for op in ops {
        match *op {
            Op::Move(x, y) => cr.move_to(x, y),
            Op::Line(x, y) => cr.line_to(x, y),
            Op::Curve(a, b, c, d, e, f) => cr.curve_to(a, b, c, d, e, f),
            Op::Close => cr.close_path(),
        }
    }
}

/// Draw an icon into a `size`-point square in the current source colour.
pub fn draw_icon(cr: &gtk::cairo::Context, icon: Icon, size: f64) {
    let def = icons::icon(icon);
    let _ = cr.save();
    let s = size / def.size;
    cr.scale(s, s);
    cr.set_line_width(def.stroke);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    for shape in &def.shapes {
        cr.new_path();
        trace(cr, &shape.ops);
        let _ = if shape.fill { cr.fill() } else { cr.stroke() };
    }
    let _ = cr.restore();
}

/// A drawing with no accessible text of its own (the text beside it says
/// the same): role `presentation`, set at construction as GTK 4.0 needs.
pub fn drawing() -> gtk::DrawingArea {
    glib::Object::builder::<gtk::DrawingArea>()
        .property("accessible-role", gtk::AccessibleRole::Presentation)
        .build()
}

/// An icon in the widget's text colour, `px` points square.
pub fn icon(icon: Icon, px: i32) -> gtk::DrawingArea {
    IconView::new(icon, px).area
}

/// An icon that can change (a chip's path icon).
pub struct IconView {
    pub area: gtk::DrawingArea,
    current: Rc<Cell<Icon>>,
}

impl IconView {
    pub fn new(icon: Icon, px: i32) -> Self {
        let area = drawing();
        area.set_content_width(px);
        area.set_content_height(px);
        area.set_valign(gtk::Align::Center);
        area.set_halign(gtk::Align::Center);
        area.set_can_target(false);
        area.add_css_class("icon");
        let current = Rc::new(Cell::new(icon));
        let c = current.clone();
        area.set_draw_func(move |a, cr, w, h| {
            let (r, g, b, al) = css_color(a);
            cr.set_source_rgba(r, g, b, al);
            let size = w.min(h) as f64;
            cr.translate((w as f64 - size) / 2.0, (h as f64 - size) / 2.0);
            draw_icon(cr, c.get(), size);
        });
        Self { area, current }
    }

    pub fn set(&self, icon: Icon) {
        if self.current.replace(icon) != icon {
            self.area.queue_draw();
        }
    }
}

/// The dotted Raven mark (`mark.rs`) in the text colour, `width` points
/// wide; `reveal` (0..1) shows that share of its dots (setup's identity
/// forge), 1 shows all.
pub struct MarkView {
    pub area: gtk::DrawingArea,
    reveal: Rc<Cell<f64>>,
}

impl MarkView {
    pub fn new(width: i32) -> Self {
        let m = mark::mark();
        let area = drawing();
        let height = (width as f64 * m.height / m.width).round() as i32;
        area.set_content_width(width);
        area.set_content_height(height.max(1));
        area.set_valign(gtk::Align::Center);
        area.set_halign(gtk::Align::Center);
        area.set_can_target(false);
        area.add_css_class("mark");
        let reveal = Rc::new(Cell::new(1.0_f64));
        let r = reveal.clone();
        let order = Rc::new(mark::reveal_order(m.dots.len()));
        area.set_draw_func(move |a, cr, w, h| {
            let (cr_r, cg, cb, ca) = css_color(a);
            cr.set_source_rgba(cr_r, cg, cb, ca);
            let s = (w as f64 / m.width).min(h as f64 / m.height);
            cr.translate(
                (w as f64 - m.width * s) / 2.0,
                (h as f64 - m.height * s) / 2.0,
            );
            // Small marks keep their dots visible.
            let radius = (m.radius * s).max(0.55);
            let shown = (r.get().clamp(0.0, 1.0) * m.dots.len() as f64).round() as usize;
            for &i in order.iter().take(shown) {
                let (x, y) = m.dots[i];
                cr.new_sub_path();
                cr.arc(x * s, y * s, radius, 0.0, std::f64::consts::TAU);
            }
            let _ = cr.fill();
        });
        Self { area, reveal }
    }

    pub fn set_reveal(&self, share: f64) {
        self.reveal.set(share);
        self.area.queue_draw();
    }
}

// ---------------------------------------------------------------- clamp

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Clamp {
        pub max: Cell<i32>,
        pub child: RefCell<Option<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Clamp {
        const NAME: &'static str = "RavenClamp";
        type Type = super::Clamp;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Clamp {
        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Clamp {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.child.borrow().clone() else {
                return (0, 0, -1, -1);
            };
            let max = self.max.get();
            if orientation == gtk::Orientation::Horizontal {
                let (min, nat, _, _) = child.measure(orientation, for_size);
                (min, nat.min(max).max(min), -1, -1)
            } else {
                let width = if for_size < 0 {
                    for_size
                } else {
                    let (min, _, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
                    for_size.min(max).max(min.min(for_size))
                };
                child.measure(orientation, width)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let Some(child) = self.child.borrow().clone() else {
                return;
            };
            let (min, _, _, _) = child.measure(gtk::Orientation::Horizontal, -1);
            let w = width.min(self.max.get()).max(min.min(width));
            let x = (width - w) / 2;
            child.size_allocate(&gtk::Allocation::new(x, 0, w, height), baseline);
        }
    }
}

glib::wrapper! {
    /// Keeps its child at most `max` points wide, centred (the tokens'
    /// content widths: 760 for chats, 900 for pages, 700 for Settings).
    pub struct Clamp(ObjectSubclass<imp::Clamp>)
        @extends gtk::Widget,
        @implements gtk::Buildable, gtk::ConstraintTarget;
}

impl Clamp {
    pub fn new(max: u32, child: &impl IsA<gtk::Widget>) -> Self {
        let c: Self = glib::Object::new();
        c.imp().max.set(max as i32);
        child.set_parent(&c);
        c.imp().child.replace(Some(child.clone().upcast()));
        c.set_hexpand(true);
        c
    }
}

// ---------------------------------------------------------------- motion

/// A frame-clock animation of one value from 0 to 1 on a token curve. A new
/// run replaces the one before; without motion (or off screen) the end
/// value is applied at once.
#[derive(Default, Clone)]
pub struct Tween(Rc<RefCell<Option<gtk::TickCallbackId>>>);

impl Tween {
    pub fn run(
        &self,
        widget: &impl IsA<gtk::Widget>,
        style: &Style,
        ms: u32,
        curve: &str,
        frame: impl Fn(f64) + 'static,
    ) {
        self.stop();
        let w = widget.as_ref();
        if !style.motion_allowed() || !w.is_mapped() || ms == 0 {
            frame(1.0);
            return;
        }
        let curve = style.curve(curve);
        let start = Cell::new(None::<i64>);
        let slot = self.0.clone();
        let id = w.add_tick_callback(move |_, clock| {
            let now = clock.frame_time();
            let t0 = match start.get() {
                Some(t) => t,
                None => {
                    start.set(Some(now));
                    now
                }
            };
            let t = ((now - t0) as f64 / 1000.0 / ms as f64).min(1.0);
            frame(curve.ease(t));
            if t >= 1.0 {
                slot.borrow_mut().take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        self.0.replace(Some(id));
    }

    pub fn stop(&self) {
        if let Some(id) = self.0.borrow_mut().take() {
            id.remove();
        }
    }
}

/// Play a CSS animation once: the class is added on the next main-loop
/// turn (so a repeat restarts it) and removed after `ms`. Nothing happens
/// under reduced motion.
pub fn play_class(widget: &impl IsA<gtk::Widget>, style: &Style, class: &'static str, ms: u32) {
    if !style.motion_allowed() {
        return;
    }
    let w = widget.as_ref();
    w.remove_css_class(class);
    let weak = w.downgrade();
    glib::idle_add_local_once(move || {
        if let Some(w) = weak.upgrade() {
            w.add_css_class(class);
            let w2 = w.downgrade();
            after(ms + 40, move || {
                if let Some(w) = w2.upgrade() {
                    w.remove_css_class(class);
                }
            });
        }
    });
}

// ---------------------------------------------------------------- ticks

/// The tick's icon, 14 points: a spinner while sending, a ring with a check
/// once sent, the filled check that pops with one ripple when the signed
/// receipt comes, a dashed clock while waiting.
pub struct TickMark {
    pub root: gtk::Overlay,
    area: gtk::DrawingArea,
    ripple: gtk::Box,
    state: Rc<Cell<TickIcon>>,
    style: Rc<Style>,
}

impl TickMark {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let area = drawing();
        area.set_content_width(14);
        area.set_content_height(14);
        area.add_css_class("st-ic");
        area.set_can_target(false);
        let ripple = hbox(0);
        ripple.add_css_class("ripple");
        ripple.set_halign(gtk::Align::Center);
        ripple.set_valign(gtk::Align::Center);
        ripple.set_can_target(false);
        let root = gtk::Overlay::new();
        root.set_child(Some(&area));
        root.add_overlay(&ripple);
        root.set_valign(gtk::Align::Center);
        let state = Rc::new(Cell::new(TickIcon::None));
        let st = state.clone();
        let sty = style.clone();
        area.set_draw_func(move |a, cr, w, h| {
            let (r, g, b, al) = css_color(a);
            cr.set_source_rgba(r, g, b, al);
            let s = w.min(h) as f64 / 14.0;
            cr.scale(s, s);
            cr.set_line_cap(gtk::cairo::LineCap::Round);
            cr.set_line_join(gtk::cairo::LineJoin::Round);
            let tau = std::f64::consts::TAU;
            match st.get() {
                TickIcon::None => {}
                TickIcon::Spinner => {
                    cr.set_line_width(1.6);
                    cr.set_source_rgba(r, g, b, al * 0.25);
                    cr.arc(7.0, 7.0, 5.5, 0.0, tau);
                    let _ = cr.stroke();
                    cr.set_source_rgba(r, g, b, al);
                    cr.arc(7.0, 7.0, 5.5, -tau / 4.0, 0.0);
                    let _ = cr.stroke();
                }
                TickIcon::Sent => {
                    cr.set_line_width(1.4);
                    cr.arc(7.0, 7.0, 5.6, 0.0, tau);
                    let _ = cr.stroke();
                    cr.set_line_width(1.5);
                    cr.move_to(4.4, 7.2);
                    cr.line_to(6.2, 9.0);
                    cr.line_to(9.7, 5.3);
                    let _ = cr.stroke();
                }
                TickIcon::Delivered => {
                    cr.arc(7.0, 7.0, 6.4, 0.0, tau);
                    let _ = cr.fill();
                    let (sr, sg, sb, _) = sty.color("surface").to_f64();
                    cr.set_source_rgb(sr, sg, sb);
                    cr.set_line_width(1.7);
                    cr.move_to(4.3, 7.2);
                    cr.line_to(6.2, 9.1);
                    cr.line_to(9.8, 5.3);
                    let _ = cr.stroke();
                }
                TickIcon::Waiting => {
                    cr.set_line_width(1.4);
                    cr.set_dash(&[2.4, 2.2], 0.0);
                    cr.arc(7.0, 7.0, 5.6, 0.0, tau);
                    let _ = cr.stroke();
                    cr.set_dash(&[], 0.0);
                    cr.move_to(7.0, 4.3);
                    cr.line_to(7.0, 7.0);
                    cr.line_to(8.8, 8.2);
                    let _ = cr.stroke();
                }
                TickIcon::Problem => {
                    cr.set_line_width(1.4);
                    cr.arc(7.0, 7.0, 5.6, 0.0, tau);
                    let _ = cr.stroke();
                    cr.set_line_width(1.6);
                    cr.move_to(7.0, 4.2);
                    cr.line_to(7.0, 7.6);
                    let _ = cr.stroke();
                    cr.arc(7.0, 9.9, 0.9, 0.0, tau);
                    let _ = cr.fill();
                }
            }
        });
        let mark = Rc::new(Self {
            root,
            area,
            ripple,
            state,
            style: style.clone(),
        });
        style.watch(&mark, |m| {
            m.refresh_spin();
            m.area.queue_draw();
        });
        mark
    }

    /// Show a state. `arriving` (a receipt that just came): the delivered
    /// check pops with one ripple.
    pub fn set(&self, icon: TickIcon, arriving: bool) {
        let before = self.state.replace(icon);
        self.root.set_visible(icon != TickIcon::None);
        self.refresh_spin();
        self.area.queue_draw();
        if arriving && icon == TickIcon::Delivered && before != TickIcon::Delivered {
            let m = &tokens().calm;
            play_class(&self.area, &self.style, "pop", m.check_pop);
            play_class(&self.ripple, &self.style, "go", m.tick_ripple);
        }
    }

    fn refresh_spin(&self) {
        if self.state.get() == TickIcon::Spinner && self.style.motion_allowed() {
            self.area.add_css_class("spin");
        } else {
            self.area.remove_css_class("spin");
        }
    }
}

/// A path chip: the path's icon and its short label on a tint of its
/// colour (tokens' `pathRule`).
pub struct Chip {
    pub root: gtk::Box,
    icon: IconView,
    text: gtk::Label,
    class: Cell<Option<&'static str>>,
}

impl Chip {
    pub fn new(pill: bool) -> Self {
        let root = hbox(4);
        root.add_css_class(if pill { "pill" } else { "chip" });
        root.set_valign(gtk::Align::Center);
        let icon = IconView::new(Icon::Route, if pill { 14 } else { 12 });
        let text = line("", &[]);
        root.append(&icon.area);
        root.append(&text);
        root.set_visible(false);
        Self {
            root,
            icon,
            text,
            class: Cell::new(None),
        }
    }

    /// Show `label` with the style of a path kind; the tooltip (also the
    /// accessible description) has the engine's full words.
    pub fn set(&self, label: &str, style: raven_desktop::present::PathStyle, words: &str) {
        if let Some(old) = self.class.replace(style.class) {
            self.root.remove_css_class(&format!("p-{old}"));
        }
        if let Some(new) = style.class {
            self.root.add_css_class(&format!("p-{new}"));
        }
        self.icon.set(style.icon);
        self.text.set_text(label);
        self.root.set_tooltip_text(Some(words));
        super::a11y::set_label(&self.root, words);
        self.root.set_visible(!label.is_empty());
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
    }
}

// ---------------------------------------------------------------- group ring

/// The ring under your group message: one arc per member, coloured as
/// each member's copy is delivered, waits, has left or failed; an arc that
/// changes fades to its new colour (groupArc).
pub struct GroupRing {
    pub area: gtk::DrawingArea,
    arcs: RefCell<Vec<Arc>>,
    from: RefCell<Vec<Rgba>>,
    progress: Rc<Cell<f64>>,
    tween: Tween,
    style: Rc<Style>,
}

impl GroupRing {
    pub fn new(style: &Rc<Style>) -> Rc<Self> {
        let px = tokens().layout.group_ring as i32;
        let area = drawing();
        area.set_content_width(px);
        area.set_content_height(px);
        area.add_css_class("gring");
        area.set_valign(gtk::Align::Center);
        area.set_can_target(false);
        let ring = Rc::new(Self {
            area,
            arcs: RefCell::new(Vec::new()),
            from: RefCell::new(Vec::new()),
            progress: Rc::new(Cell::new(1.0)),
            tween: Tween::default(),
            style: style.clone(),
        });
        let weak = Rc::downgrade(&ring);
        ring.area.set_draw_func(move |_, cr, w, h| {
            let Some(ring) = weak.upgrade() else { return };
            let arcs = ring.arcs.borrow();
            let from = ring.from.borrow();
            let t = ring.progress.get();
            let size = w.min(h) as f64;
            let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
            let radius = size / 2.0 - 2.0;
            cr.set_line_width(3.2 * size / 18.0);
            for (i, (start, sweep)) in groupview::arc_geometry(arcs.len()).into_iter().enumerate() {
                let to = ring.style.color(arcs[i].color());
                let c = from.get(i).map_or(to, |f| f.mix(to, t));
                let (r, g, b, a) = c.to_f64();
                cr.set_source_rgba(r, g, b, a);
                cr.new_sub_path();
                cr.arc(cx, cy, radius, start, start + sweep);
                let _ = cr.stroke();
            }
        });
        style.watch(&ring, |r| r.area.queue_draw());
        ring
    }

    /// New member states; arcs that changed fade to their new colour.
    pub fn set(&self, arcs: Vec<Arc>) {
        let old: Vec<Rgba> = self
            .arcs
            .borrow()
            .iter()
            .map(|a| self.style.color(a.color()))
            .collect();
        let changed =
            old.len() == arcs.len() && self.arcs.borrow().iter().zip(&arcs).any(|(a, b)| a != b);
        self.arcs.replace(arcs);
        if changed {
            self.from.replace(old);
            self.progress.set(0.0);
            let (p, area) = (self.progress.clone(), self.area.downgrade());
            self.tween.run(
                &self.area,
                &self.style,
                tokens().calm.group_arc,
                "arrive",
                move |t| {
                    p.set(t);
                    if let Some(a) = area.upgrade() {
                        a.queue_draw();
                    }
                },
            );
        } else {
            self.from.replace(Vec::new());
            self.progress.set(1.0);
            self.area.queue_draw();
        }
    }
}

// ---------------------------------------------------------------- segmented

/// A segmented control (Theme, Keep chats for, …): toggle buttons in a
/// group over a thumb that slides and resizes to the chosen one (curve
/// `thumb`). Arrow keys and Tab reach every choice; the group's label is
/// its accessible name.
pub struct Segmented {
    pub root: gtk::Overlay,
    buttons: Vec<gtk::ToggleButton>,
    row: gtk::Box,
    fixed: gtk::Fixed,
    thumb: gtk::Box,
    tween: Tween,
    style: Rc<Style>,
    /// The thumb's x and width now.
    at: Cell<(f64, f64)>,
    on_change: RefCell<Option<OnChoice>>,
}

/// What a segmented control calls with the chosen index.
type OnChoice = Box<dyn Fn(usize)>;

impl Segmented {
    pub fn new(style: &Rc<Style>, options: &[&str], selected: usize, name: &str) -> Rc<Self> {
        let row = hbox(0);
        row.set_homogeneous(false);
        let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
        for (i, text) in options.iter().enumerate() {
            let b = gtk::ToggleButton::with_label(text);
            b.set_use_underline(false);
            if let Some(first) = buttons.first() {
                b.set_group(Some(first));
            }
            b.set_active(i == selected);
            row.append(&b);
            buttons.push(b);
        }
        let thumb = hbox(0);
        thumb.add_css_class("seg-thumb");
        thumb.set_can_target(false);
        let fixed = gtk::Fixed::new();
        fixed.put(&thumb, 0.0, 0.0);
        fixed.set_can_target(false);
        let root = gtk::Overlay::new();
        root.add_css_class("seg");
        root.set_child(Some(&fixed));
        root.add_overlay(&row);
        root.set_measure_overlay(&row, true);
        root.set_valign(gtk::Align::Center);
        root.set_halign(gtk::Align::End);
        super::a11y::set_label(&root, name);
        let seg = Rc::new(Self {
            root,
            buttons,
            row,
            fixed,
            thumb,
            tween: Tween::default(),
            style: style.clone(),
            at: Cell::new((0.0, 0.0)),
            on_change: RefCell::new(None),
        });
        for (i, b) in seg.buttons.iter().enumerate() {
            let weak = Rc::downgrade(&seg);
            b.connect_toggled(move |b| {
                let Some(s) = weak.upgrade() else { return };
                if !b.is_active() {
                    return;
                }
                s.place(true);
                let callback = s.on_change.borrow();
                if let Some(f) = callback.as_ref() {
                    f(i);
                }
            });
        }
        // Place the thumb once the buttons have their size.
        let weak = Rc::downgrade(&seg);
        // Tick callbacks run before layout: try again on the next frames
        // until the buttons have their size.
        seg.root.connect_map(move |root| {
            let weak = weak.clone();
            let frames = Cell::new(0u32);
            root.add_tick_callback(move |_, _| {
                frames.set(frames.get() + 1);
                let placed = weak.upgrade().is_none_or(|s| s.place(false));
                if placed || frames.get() > 30 {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        });
        seg
    }

    pub fn connect_changed(&self, f: impl Fn(usize) + 'static) {
        self.on_change.replace(Some(Box::new(f)));
    }

    /// Put the thumb under the chosen button; false while the buttons have
    /// no size yet.
    fn place(&self, animate: bool) -> bool {
        let Some(b) = self.buttons.iter().find(|b| b.is_active()) else {
            return true;
        };
        let Some(bounds) = b.compute_bounds(&self.row) else {
            return false;
        };
        let (x, w) = (bounds.x() as f64, bounds.width() as f64);
        let h = self.row.height();
        if w <= 0.0 || h <= 0 {
            return false;
        }
        let (fx, fw) = self.at.get();
        let set = {
            let (fixed, thumb) = (self.fixed.downgrade(), self.thumb.downgrade());
            move |x: f64, w: f64| {
                if let (Some(f), Some(t)) = (fixed.upgrade(), thumb.upgrade()) {
                    t.set_size_request(w.round().max(1.0) as i32, h.max(1));
                    f.move_(&t, x, 0.0);
                }
            }
        };
        if !animate || fw == 0.0 {
            self.tween.stop();
            self.at.set((x, w));
            set(x, w);
            return true;
        }
        self.at.set((x, w));
        self.tween.run(
            &self.root,
            &self.style,
            tokens().calm.segment_thumb,
            "thumb",
            move |t| set(fx + (x - fx) * t, fw + (w - fw) * t),
        );
        true
    }
}

// ---------------------------------------------------------------- glide list

/// The Settings section list: one button per section over a selection
/// that glides to the chosen one (curve `glide`, with its small overshoot).
pub struct GlideList {
    pub root: gtk::Overlay,
    column: gtk::Box,
    fixed: gtk::Fixed,
    glider: gtk::Box,
    pub buttons: Vec<gtk::Button>,
    current: Cell<Option<usize>>,
    tween: Tween,
    style: Rc<Style>,
    y: Cell<f64>,
}

impl GlideList {
    /// `items`: (label, icon, CSS suffix of its colour square).
    pub fn new(style: &Rc<Style>, items: &[(&str, Icon, &str)]) -> Rc<Self> {
        let column = vbox(2);
        let mut buttons = Vec::new();
        for (text, ic, colour) in items {
            let square = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            square.add_css_class("sec-ic");
            square.add_css_class(&format!("sec-{colour}"));
            square.set_valign(gtk::Align::Center);
            let i = icon(*ic, 15);
            i.set_hexpand(true);
            square.append(&i);
            let row = hbox(10);
            row.append(&square);
            row.append(&line(text, &[]));
            let b = gtk::Button::new();
            b.add_css_class("setnav-item");
            b.set_child(Some(&row));
            super::a11y::set_label(&b, text);
            column.append(&b);
            buttons.push(b);
        }
        let glider = hbox(0);
        glider.add_css_class("glider");
        glider.set_can_target(false);
        let fixed = gtk::Fixed::new();
        fixed.put(&glider, 0.0, 0.0);
        fixed.set_can_target(false);
        let root = gtk::Overlay::new();
        root.set_child(Some(&fixed));
        root.add_overlay(&column);
        root.set_measure_overlay(&column, true);
        glider.set_visible(false);
        let list = Rc::new(Self {
            root,
            column,
            fixed,
            glider,
            buttons,
            current: Cell::new(None),
            tween: Tween::default(),
            style: style.clone(),
            y: Cell::new(0.0),
        });
        let weak = Rc::downgrade(&list);
        // Tick callbacks run before layout: try again on the next frames
        // until the buttons have their size.
        list.root.connect_map(move |root| {
            let weak = weak.clone();
            let frames = Cell::new(0u32);
            root.add_tick_callback(move |_, _| {
                frames.set(frames.get() + 1);
                let placed = weak.upgrade().is_none_or(|l| l.place(false));
                if placed || frames.get() > 30 {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        });
        list
    }

    /// Mark `i` as chosen; the selection glides there when `animate`.
    pub fn select(&self, i: usize, animate: bool) {
        if let Some(old) = self.current.replace(Some(i)) {
            if let Some(b) = self.buttons.get(old) {
                b.remove_css_class("current");
            }
        }
        if let Some(b) = self.buttons.get(i) {
            b.add_css_class("current");
        }
        self.place(animate);
    }

    /// Put the glider under the chosen button; false while the buttons
    /// have no size yet.
    fn place(&self, animate: bool) -> bool {
        let Some(i) = self.current.get() else {
            return true;
        };
        let Some(b) = self.buttons.get(i) else {
            return true;
        };
        let Some(bounds) = b.compute_bounds(&self.column) else {
            return false;
        };
        let (y, w, h) = (
            bounds.y() as f64,
            bounds.width().round() as i32,
            bounds.height().round() as i32,
        );
        if w <= 0 || h <= 0 {
            return false;
        }
        self.glider.set_size_request(w, h);
        self.glider.set_visible(true);
        let from = self.y.get();
        self.y.set(y);
        let (fixed, glider) = (self.fixed.downgrade(), self.glider.downgrade());
        let set = move |y: f64| {
            if let (Some(f), Some(g)) = (fixed.upgrade(), glider.upgrade()) {
                f.move_(&g, 0.0, y);
            }
        };
        if !animate {
            self.tween.stop();
            set(y);
            return true;
        }
        self.tween.run(
            &self.root,
            &self.style,
            tokens().calm.settings_glide,
            "glide",
            move |t| set(from + (y - from) * t),
        );
        true
    }
}

// ---------------------------------------------------------------- cards

/// A Settings group: a card of rows.
pub fn group_card() -> gtk::Box {
    let b = vbox(0);
    b.add_css_class("grp");
    b
}

/// A group label above a card ("WAYS TO REACH FRIENDS").
pub fn group_label(text: &str) -> gtk::Label {
    let l = label(&text.to_uppercase(), &["glbl"]);
    l.set_wrap(false);
    l
}

/// One row of a card: title, description under it, control on the right.
/// The title is the control's accessible name; the description its
/// accessible description.
pub fn item(
    card: &gtk::Box,
    title: &str,
    desc: Option<&str>,
    control: Option<&gtk::Widget>,
) -> gtk::Box {
    let row = hbox(18);
    row.add_css_class("item");
    if card.first_child().is_none() {
        row.add_css_class("first");
    }
    let texts = vbox(3);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    let t = label(title, &["item-title"]);
    texts.append(&t);
    if let Some(d) = desc.filter(|d| !d.is_empty()) {
        texts.append(&label(d, &["item-desc"]));
    }
    row.append(&texts);
    if let Some(c) = control {
        c.set_valign(gtk::Align::Center);
        if c.is::<gtk::Switch>() || c.is::<gtk::Entry>() || c.is::<gtk::Button>() {
            t.set_mnemonic_widget(Some(c));
        }
        if let Some(d) = desc {
            super::a11y::set_description(c, d);
        }
        row.append(c);
    }
    card.append(&row);
    row
}

/// A status chip ("On", "Set up"): `tone` is "", "n" (neutral) or "w"
/// (warning).
pub fn stat(text: &str, tone: &str) -> gtk::Box {
    let b = hbox(5);
    b.add_css_class("stat");
    if !tone.is_empty() {
        b.add_css_class(tone);
    }
    b.set_valign(gtk::Align::Center);
    let dot = hbox(0);
    dot.add_css_class("ldot");
    dot.set_valign(gtk::Align::Center);
    b.append(&dot);
    b.append(&line(text, &[]));
    b
}

/// A label a screen reader announces as a heading.
pub fn heading_label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = glib::Object::builder::<gtk::Label>()
        .property("accessible-role", gtk::AccessibleRole::Heading)
        .build();
    l.set_text(text);
    l.set_use_markup(false);
    l.set_wrap(true);
    l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// A section's title with its lead sentence.
pub fn section_head(page: &gtk::Box, title: &str, lead: Option<&str>) {
    let t = heading_label(title, &["sec-title"]);
    page.append(&t);
    if let Some(l) = lead {
        let l = label(l, &["lead"]);
        l.set_margin_bottom(20);
        l.set_margin_top(4);
        page.append(&l);
    }
}
