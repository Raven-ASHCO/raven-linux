//! Application start, the shared context, engine calls (on the engine
//! thread), event polling, actions and quitting.

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use raven_desktop::api::{
    poll, ApiError, EventKind, LockState, NetworkStatus, NodeStatus, ProfileStateKind, RavenEngine,
    Settings, WhenQuit,
};
use raven_desktop::netview::{self, NetworkSummary};
use raven_desktop::secview;
use raven_desktop::strings::tr;
use raven_desktop::tray::{self, CloseAction, TrayAction, TrayInput};
use raven_desktop::worker::EngineWorker;
use raven_desktop::{timefmt, APP_ID};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::main_window::MainWindow;
use super::style::Style;
use super::tray::Tray;
use super::widgets::{self, details_expander, heading, label, primary_button, sp, vbox, MD};
use super::{add_friend, lock_screen, quit, welcome};

type AppAction = Box<dyn Fn(&Rc<Ctx>)>;

/// Shared state of the running app.
pub struct Ctx {
    pub app: gtk::Application,
    /// The engine runs on its own threads: use [`Ctx::call`] / [`Ctx::run`]
    /// (in order) and `poll_once` (its own lane), never a blocking call on
    /// the GTK main thread.
    worker: EngineWorker,
    pub style: Rc<Style>,
    pub settings: RefCell<Settings>,
    pub node: RefCell<NodeStatus>,
    pub main: RefCell<Option<Rc<MainWindow>>>,
    /// The welcome, locked or error window while there is no main window.
    pub first_window: RefCell<Option<gtk::Window>>,
    last_seq: Cell<u64>,
    /// False until the first poll: its backlog is applied without motion.
    primed: Cell<bool>,
    started_timers: Cell<bool>,
    ticks: Cell<u64>,
    /// An `events.poll` is on its way: don't queue another one.
    polling: Cell<bool>,
    last_poll: Cell<Option<Instant>>,
    /// The main window is being opened (its engine calls are on their way).
    opening: Cell<bool>,
    /// The StatusNotifierItem (`None` without a session bus).
    tray: RefCell<Option<Rc<Tray>>>,
    /// `privacy.state` `available`: "Lock now" in the tray works.
    pub privacy_available: Cell<bool>,
    /// Engine time of the last `status_ok` (the tray's heartbeat line).
    last_ok: Cell<u64>,
    /// Network changes wait for the background program to restart (the
    /// person chose Later): the Network page keeps a reminder.
    pub restart_pending: Cell<bool>,
    /// What the tray and the sidebar say about the network.
    network: RefCell<Option<NetworkSummary>>,
    /// Lock while running (Lock v2), as `lock.state` last said.
    lock: RefCell<LockState>,
}

/// Every this many seconds the clock reads `lock.state` and
/// `network.status` again (a `raven lock` or a change made in a terminal
/// shows up in the app, the tray and the sidebar).
const STATUS_EVERY_SECS: u64 = 10;

/// Built on the engine thread. Raven's engine (raven-client, linked in with
/// the `engine` feature) on the CLI's profile. Debug builds run on the
/// in-memory mock only when asked for (`RAVEN_DEMO=1` or
/// `RAVEN_MOCK_PROFILE`); release builds do not contain the mock.
fn make_engine() -> Box<dyn RavenEngine> {
    #[cfg(debug_assertions)]
    if raven_desktop::mock::demo_requested(|key| std::env::var(key).ok()) {
        return Box::new(raven_desktop::mock::MockRaven::from_env());
    }
    Box::new(raven_desktop::client::ClientEngine::new())
}

fn update_local_offset() {
    if let Ok(now) = glib::DateTime::now_local() {
        timefmt::set_local_offset_secs(now.utc_offset().as_seconds() as i32);
    }
}

pub fn run() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    let slot: Rc<RefCell<Option<Rc<Ctx>>>> = Rc::new(RefCell::new(None));
    let activate_slot = slot.clone();
    app.connect_activate(move |app| {
        let existing = activate_slot.borrow().clone();
        if let Some(ctx) = existing {
            ctx.present();
            return;
        }
        let ctx = Ctx::new(app);
        activate_slot.replace(Some(ctx.clone()));
        ctx.start();
    });
    let code = app.run();
    // Let calls already sent (for example a settings change) finish.
    let ctx = slot.take();
    if let Some(ctx) = ctx {
        ctx.worker.shutdown();
    }
    code
}

impl Ctx {
    fn new(app: &gtk::Application) -> Rc<Self> {
        update_local_offset();
        Rc::new(Self {
            app: app.clone(),
            worker: EngineWorker::start(make_engine),
            style: Style::install(),
            settings: RefCell::new(Settings::default()),
            node: RefCell::new(NodeStatus::default()),
            main: RefCell::new(None),
            first_window: RefCell::new(None),
            last_seq: Cell::new(0),
            primed: Cell::new(false),
            started_timers: Cell::new(false),
            ticks: Cell::new(0),
            polling: Cell::new(false),
            last_poll: Cell::new(None),
            opening: Cell::new(false),
            tray: RefCell::new(None),
            privacy_available: Cell::new(false),
            last_ok: Cell::new(0),
            restart_pending: Cell::new(false),
            network: RefCell::new(None),
            lock: RefCell::new(LockState::default()),
        })
    }

    /// Run `job` on the engine's calls lane; `done` gets its result on the
    /// GTK main thread (skipped if the app is gone by then). Calls run one
    /// after another in the order they were made (`events.poll` has its own
    /// lane, see `poll_once`).
    pub fn call<T, J, D>(self: &Rc<Self>, job: J, done: D)
    where
        T: Send + 'static,
        J: FnOnce(&dyn RavenEngine) -> Result<T, ApiError> + Send + 'static,
        D: FnOnce(&Rc<Ctx>, Result<T, ApiError>) + 'static,
    {
        let reply = self.worker.call(job);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = reply.await;
            if let Some(ctx) = weak.upgrade() {
                done(&ctx, result);
            }
        });
    }

    /// Like [`Ctx::call`] for a job that makes several engine calls and
    /// returns them together. `done` is skipped if the engine thread stopped.
    pub fn run<T, J, D>(self: &Rc<Self>, job: J, done: D)
    where
        T: Send + 'static,
        J: FnOnce(&dyn RavenEngine) -> T + Send + 'static,
        D: FnOnce(&Rc<Ctx>, T) + 'static,
    {
        let reply = self.worker.run(job);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            if let (Some(result), Some(ctx)) = (reply.await, weak.upgrade()) {
                done(&ctx, result);
            }
        });
    }

    fn start(self: &Rc<Self>) {
        gtk::Window::set_default_icon_name(APP_ID);
        #[cfg(debug_assertions)]
        if let Some(display) = gtk::gdk::Display::default() {
            // Running from the source tree: find the icon without installing.
            gtk::IconTheme::for_display(&display)
                .add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons"));
        }
        self.add_actions();
        // The tray icon where the desktop shows one (StatusNotifierItem).
        self.tray.replace(Tray::start(self));
        // The app has no window until the engine answered: hold it.
        let hold = self.app.hold();
        self.run(
            |e| (e.settings_get(), e.profile_state(), e.privacy_state()),
            move |ctx, (settings, profile, privacy)| {
                drop(hold);
                if let Ok(s) = settings {
                    ctx.settings_changed(s);
                }
                ctx.privacy_available
                    .set(privacy.is_ok_and(|p| p.available));
                match profile {
                    Ok(p) => match p.state {
                        ProfileStateKind::New => welcome::show(ctx),
                        ProfileStateKind::Ready => ctx.open_main(),
                        ProfileStateKind::Locked => ctx.show_locked(),
                        ProfileStateKind::Unreadable | ProfileStateKind::Unknown => {
                            let reason = p.reason.unwrap_or_default();
                            let message = if reason.is_empty() {
                                tr("error.internal").to_string()
                            } else {
                                reason.clone()
                            };
                            ctx.show_error(&ApiError::new(
                                "PROFILE_UNREADABLE",
                                message,
                                format!("profile.state: {:?} {reason}", p.state),
                            ));
                        }
                    },
                    Err(e) => ctx.show_error(&e),
                }
            },
        );
    }

    fn present(&self) {
        if let Some(m) = self.main.borrow().as_ref() {
            m.window.present();
        } else if let Some(w) = self.first_window.borrow().as_ref() {
            w.present();
        }
    }

    /// The main window (after setup, or straight away for a ready profile).
    pub fn open_main(self: &Rc<Self>) {
        if self.main.borrow().is_some() || self.opening.replace(true) {
            return;
        }
        // At start there may be no window until the answer is here.
        let hold = self.app.hold();
        self.run(
            |e| {
                let settings = e.settings_get();
                let node = match e.node_status() {
                    // Sending needs the background program too, so start it
                    // even when receiving is off (it then doesn't listen).
                    Ok(n) if !n.running => Some(e.node_start().unwrap_or(n)),
                    Ok(n) => Some(n),
                    Err(_) => None,
                };
                (settings, node, e.lock_state().ok())
            },
            move |ctx, (settings, node, lock)| {
                let _hold = hold;
                ctx.opening.set(false);
                if let Ok(s) = settings {
                    ctx.settings_changed(s);
                }
                if let Some(n) = node {
                    ctx.last_ok.set(ctx.last_ok.get().max(n.last_ok_ms));
                    ctx.node.replace(n);
                }
                let lock = lock.unwrap_or_default();
                ctx.lock.replace(lock.clone());
                if secview::lock_view(&lock).locked {
                    // Locked while running (a `raven lock`, or before the
                    // app quit): the locked screen first.
                    let w = lock_screen::show(ctx, &lock);
                    ctx.set_first_window(&w);
                    return;
                }
                let main = MainWindow::new(ctx);
                ctx.main.replace(Some(main.clone()));
                if let Some(w) = ctx.first_window.take() {
                    w.destroy();
                }
                main.window.present();
                ctx.start_timers();
                ctx.refresh_status();
                ctx.refresh_tray();
            },
        );
    }

    /// Read `lock.state` and `network.status` again (on the clock, and
    /// after the main window opened). On the locked screen only `lock.state`
    /// matters: unlocked elsewhere (`raven unlock`), the main window returns.
    fn refresh_status(self: &Rc<Self>) {
        let main_open = self.main.borrow().is_some();
        let locked = secview::lock_view(&self.lock.borrow()).locked;
        if !main_open && !locked {
            return;
        }
        self.run(
            |e| (e.lock_state(), e.network_status()),
            move |ctx, (lock, network)| {
                if let Ok(n) = network {
                    ctx.network_changed(&n);
                }
                let Ok(l) = lock else { return };
                let still_locked = secview::lock_view(&l).locked;
                if !main_open && locked && !still_locked && ctx.main.borrow().is_none() {
                    ctx.unlocked(l);
                } else {
                    ctx.lock_changed(l);
                }
            },
        );
    }

    /// A new `network.status`: the tray's lines and the sidebar's.
    pub fn network_changed(&self, status: &NetworkStatus) {
        let summary = netview::summary(status, cfg!(debug_assertions));
        let main = self.main.borrow().clone();
        if let Some(m) = main {
            m.set_network_summary(Some(summary.on));
        }
        self.network.replace(Some(summary));
        self.refresh_tray();
    }

    /// A new `lock.state`. Locked while the main window is open (Lock now,
    /// or `raven lock` in a terminal): the locked screen replaces it.
    pub fn lock_changed(self: &Rc<Self>, lock: LockState) {
        let locked = secview::lock_view(&lock).locked;
        self.lock.replace(lock.clone());
        self.refresh_tray();
        if !locked || self.main.borrow().is_none() {
            return;
        }
        let main = self.main.take();
        self.primed.set(false);
        let w = lock_screen::show(self, &lock);
        self.set_first_window(&w);
        if let Some(m) = main {
            m.window.destroy();
        }
    }

    /// Unlocked on the locked screen: back to the main window.
    pub fn unlocked(self: &Rc<Self>, lock: LockState) {
        self.lock.replace(lock);
        self.open_main();
    }

    fn start_timers(self: &Rc<Self>) {
        if self.started_timers.replace(true) {
            return;
        }
        // Ticks every 500 ms; polls every 500 ms while a window is open and
        // every 5 s when no window is (API.md).
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(poll::WINDOW_OPEN_MS), move || {
            let Some(ctx) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let since = ctx.last_poll.get().map(|t| t.elapsed().as_millis() as u64);
            if poll::due(ctx.window_open(), since) {
                ctx.poll_once();
            }
            glib::ControlFlow::Continue
        });
        // The 1-second clock: relative times and the heartbeat age.
        let weak = Rc::downgrade(self);
        glib::timeout_add_seconds_local(1, move || {
            let Some(ctx) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let n = ctx.ticks.get() + 1;
            ctx.ticks.set(n);
            if n.is_multiple_of(60) {
                update_local_offset();
            }
            let main = ctx.main.borrow().clone();
            if let Some(m) = main {
                m.clock_tick(n);
            }
            if n.is_multiple_of(STATUS_EVERY_SECS) {
                ctx.refresh_status();
            }
            // Cheap: the menu is recomputed, D-Bus hears only of changes.
            ctx.refresh_tray();
            glib::ControlFlow::Continue
        });
        self.poll_once();
    }

    /// True while any of the app's windows is on screen.
    fn window_open(&self) -> bool {
        self.app.windows().iter().any(|w| w.is_visible())
    }

    fn poll_once(self: &Rc<Self>) {
        if self.polling.replace(true) {
            return; // the previous poll hasn't answered yet
        }
        self.last_poll.set(Some(Instant::now()));
        // Its own lane: a poll never waits behind a slow call (node.start).
        let reply = self.worker.poll_events(self.last_seq.get());
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = reply.await;
            if let Some(ctx) = weak.upgrade() {
                ctx.polling.set(false);
                ctx.apply_poll(result);
                ctx.refresh_tray();
            }
        });
    }

    fn apply_poll(&self, result: Result<raven_desktop::api::Poll, ApiError>) {
        let main = self.main.borrow().clone();
        match result {
            Ok(p) => {
                self.last_seq.set(p.seq);
                if main.is_none() || !self.primed.replace(true) || p.reset {
                    // The window loaded the current state (friends, history,
                    // node status) when it opened, so older events are not
                    // replayed as motion; only the node state is kept. With
                    // no main window (Welcome after an erase) the same. After
                    // a `reset` (events were dropped) the window loads it
                    // again.
                    let mut last_ok = None;
                    for ev in &p.events {
                        match &ev.kind {
                            EventKind::Node { status } => {
                                self.node.replace(status.clone());
                            }
                            EventKind::StatusOk => last_ok = Some(ev.at_ms),
                            _ => {}
                        }
                    }
                    if let Some(at) = last_ok {
                        self.last_ok.set(at.max(1));
                    }
                    if let Some(m) = &main {
                        m.primed(last_ok);
                        if p.reset {
                            m.reload();
                        }
                    }
                    return;
                }
                for ev in &p.events {
                    if let EventKind::StatusOk = ev.kind {
                        self.last_ok.set(ev.at_ms.max(1));
                    }
                    let node_changed = if let EventKind::Node { status } = &ev.kind {
                        self.node.replace(status.clone());
                        true
                    } else {
                        false
                    };
                    let _ = node_changed;
                    if let Some(m) = &main {
                        m.handle_event(ev);
                    }
                }
            }
            Err(e) => {
                self.last_ok.set(0);
                if let Some(m) = &main {
                    m.engine_unreachable(&e);
                }
            }
        }
    }

    /// Re-read settings into the context (after the Preferences window
    /// changed something).
    pub fn settings_changed(&self, s: Settings) {
        self.style.set_app_reduced_motion(s.reduced_motion);
        self.settings.replace(s);
    }

    /// Engine time of the last successful status check (0: none yet).
    pub fn last_status_ok(&self) -> u64 {
        self.last_ok.get()
    }

    /// A short message in the main window's pill (`tone`: "ok", "wait",
    /// "bad" or "").
    pub fn toast(&self, text: &str, tone: &str) {
        let main = self.main.borrow().clone();
        if let Some(m) = main {
            m.toast(text, tone);
        }
    }

    pub fn main_window(&self) -> Option<gtk::Window> {
        self.main
            .borrow()
            .as_ref()
            .map(|m| m.window.clone().upcast())
    }

    fn add_actions(self: &Rc<Self>) {
        let add = |name: &str, f: AppAction| {
            let a = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            a.connect_activate(move |_, _| {
                if let Some(ctx) = weak.upgrade() {
                    f(&ctx);
                }
            });
            self.app.add_action(&a);
        };
        add(
            "add-friend",
            Box::new(|ctx| {
                if ctx.main.borrow().is_some() {
                    add_friend::show(ctx);
                }
            }),
        );
        add(
            "network",
            Box::new(|ctx| {
                let main = ctx.main.borrow().clone();
                match main {
                    Some(m) => {
                        m.window.present();
                        m.show_network();
                    }
                    None => ctx.present(),
                }
            }),
        );
        add(
            "preferences",
            Box::new(|ctx| {
                let main = ctx.main.borrow().clone();
                match main {
                    Some(m) => {
                        m.window.present();
                        m.show_settings(None);
                    }
                    None => ctx.present(),
                }
            }),
        );
        add("quit-window", Box::new(|ctx| ctx.quit_keep_running()));
        add("quit-all", Box::new(|ctx| ctx.quit_and_stop()));
        add("quit", Box::new(|ctx| ctx.request_quit()));
        self.app.set_accels_for_action("app.quit", &["<Control>q"]);
        self.app
            .set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.app
            .set_accels_for_action("app.add-friend", &["<Control>n"]);
    }

    // ------------------------------------------------------------ tray

    /// What the tray menu shows now.
    pub fn tray_input(&self) -> TrayInput {
        let main = self.main.borrow().clone();
        let node = self.node.borrow().clone();
        TrayInput {
            last_ok_ms: self.last_ok.get(),
            now_ms: timefmt::now_ms(),
            ready: main.is_some(),
            friends: main.map(|m| m.tray_friends()).unwrap_or_default(),
            can_lock: self.privacy_available.get()
                || secview::lock_view(&self.lock.borrow()).can_lock,
            network: self.network.borrow().clone(),
            node,
        }
    }

    fn refresh_tray(&self) {
        let tray = self.tray.borrow().clone();
        if let Some(t) = tray {
            t.refresh();
        }
    }

    /// True while a panel shows Raven's tray icon.
    pub fn tray_shown(&self) -> bool {
        self.tray.borrow().as_ref().is_some_and(|t| t.shown())
    }

    /// The tray icon appeared or went away. Never leave Raven running with
    /// a hidden window and no icon: bring the window back.
    pub fn tray_shown_changed(&self, shown: bool) {
        if shown {
            return;
        }
        let main = self.main.borrow().clone();
        if let Some(m) = main {
            if !m.window.is_visible() {
                m.window.present();
            }
        }
    }

    /// A click in the tray menu (or on the icon: `Open`).
    pub fn tray_action(self: &Rc<Self>, action: TrayAction) {
        let main = self.main.borrow().clone();
        match action {
            TrayAction::Open => self.present(),
            TrayAction::OpenFriend(address) => {
                self.present();
                if let Some(m) = main {
                    m.select_row_for(&address);
                }
            }
            TrayAction::AddFriend => {
                if main.is_some() {
                    self.present();
                    add_friend::show(self);
                }
            }
            TrayAction::Network => {
                self.present();
                if let Some(m) = main {
                    m.show_network();
                }
            }
            TrayAction::Lock => self.lock_now(),
            TrayAction::QuitWindow => self.quit_keep_running(),
            TrayAction::QuitAll => self.quit_and_stop(),
        }
    }

    /// The main window's close button: hide to the tray when its icon is on
    /// screen (Raven keeps running and polls every 5 s), otherwise follow
    /// "When I quit".
    pub fn close_main(self: &Rc<Self>) {
        match tray::close_action(self.tray_shown()) {
            CloseAction::HideToTray => {
                let main = self.main.borrow().clone();
                if let Some(m) = main {
                    m.window.set_visible(false);
                }
            }
            CloseAction::FollowWhenQuit => self.request_quit(),
        }
    }

    /// "Lock now" (tray, Security page). Lock while running (Lock v2) when
    /// the background program serves it; else the privacy pack's lock when
    /// `privacy.state` says it is there; otherwise the item is disabled.
    pub fn lock_now(self: &Rc<Self>) {
        if self.main.borrow().is_none() {
            return;
        }
        if secview::lock_view(&self.lock.borrow()).can_lock {
            self.call(
                |e| e.lock_now(),
                |ctx, result| match result {
                    Ok(l) => ctx.lock_changed(l),
                    Err(e) => {
                        ctx.present();
                        let main = ctx.main.borrow().clone();
                        if let Some(m) = main {
                            m.show_error(&secview::unlock_error(&e));
                        }
                    }
                },
            );
            return;
        }
        if !self.privacy_available.get() {
            return;
        }
        self.call(
            |e| e.privacy_lock(),
            |ctx, result| match result {
                Ok(p) if p.state == ProfileStateKind::Locked => {
                    let main = ctx.main.take();
                    ctx.primed.set(false);
                    ctx.show_locked();
                    if let Some(m) = main {
                        m.window.destroy();
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    ctx.present();
                    let main = ctx.main.borrow().clone();
                    if let Some(m) = main {
                        m.show_error(&e.message);
                    }
                }
            },
        );
    }

    /// Ctrl+Q, the window menu's Quit, or closing the window without a tray
    /// icon: follow "When I quit".
    pub fn request_quit(self: &Rc<Self>) {
        if self.main.borrow().is_none() {
            self.app.quit();
            return;
        }
        let when = self.settings.borrow().when_quit;
        match when {
            WhenQuit::Keep => self.quit_keep_running(),
            WhenQuit::StopIfStarted => {
                if self.node.borrow().started_by_app {
                    self.quit_and_stop();
                } else {
                    self.quit_keep_running();
                }
            }
            WhenQuit::Ask | WhenQuit::Unknown => quit::show(self),
        }
    }

    /// Quit the window app; the background program keeps receiving. The
    /// tray icon (if any) goes away with the app, so a desktop notification
    /// says that Raven keeps receiving.
    pub fn quit_keep_running(&self) {
        if self.node.borrow().running {
            let n = gio::Notification::new(tr("app.name"));
            n.set_body(Some(tr("quit.running")));
            self.app.send_notification(Some("keeps-running"), &n);
        }
        self.app.quit();
    }

    /// Quit and stop the background program (after the engine answered).
    pub fn quit_and_stop(self: &Rc<Self>) {
        self.call(|e| e.node_stop(), |ctx, _| ctx.app.quit());
    }

    /// After `privacy.erase`: close everything and start again at Welcome.
    pub fn erased(self: &Rc<Self>) {
        let main = self.main.take();
        self.last_seq.set(0);
        self.primed.set(false);
        welcome::show(self);
        if let Some(m) = main {
            m.window.destroy();
        }
    }

    fn set_first_window(&self, w: &gtk::ApplicationWindow) {
        if let Some(old) = self.first_window.replace(Some(w.clone().upcast())) {
            old.destroy();
        }
        w.present();
    }

    /// The locked window of the privacy pack's lock (`profile.state` says
    /// locked; until the privacy pack `privacy.unlock` answers
    /// NOT_AVAILABLE_YET).
    fn show_locked(self: &Rc<Self>) {
        let (w, card, error) = lock_screen::locked_window(self, tr("lock.body"));
        card.append(&error);
        let unlock = primary_button(tr("lock.unlock"));
        unlock.set_hexpand(true);
        let weak = Rc::downgrade(self);
        let (err, cd) = (error.downgrade(), card.downgrade());
        unlock.connect_clicked(move |button| {
            let Some(ctx) = weak.upgrade() else { return };
            button.set_sensitive(false);
            let (button, error, cd) = (button.downgrade(), err.clone(), cd.clone());
            ctx.call(
                |e| e.privacy_unlock(),
                move |ctx, result| {
                    if let Some(b) = button.upgrade() {
                        b.set_sensitive(true);
                    }
                    match result {
                        Ok(p) if p.state == ProfileStateKind::Ready => ctx.open_main(),
                        Ok(_) => {}
                        Err(e) => {
                            if let Some(l) = error.upgrade() {
                                l.set_text(&e.message);
                            }
                            if let Some(c) = cd.upgrade() {
                                lock_screen::shake(ctx, &c);
                            }
                        }
                    }
                },
            );
        });
        card.append(&unlock);
        self.set_first_window(&w);
    }

    /// The engine could not start: what happened, one next step, details.
    pub fn show_error(self: &Rc<Self>, e: &ApiError) {
        let w = gtk::ApplicationWindow::new(&self.app);
        w.set_title(Some(tr("app.name")));
        w.set_default_size(460, 260);
        self.style.decorate_window(&w);
        let content = vbox(sp(MD));
        content.add_css_class("raven-page");
        content.append(&heading(tr("app.name")));
        content.append(&label(&e.message, &[]));
        content.append(&details_expander(&format!("{}\n{}", e.code, e.detail)));
        let row = widgets::action_row();
        let close = widgets::button(tr("common.close"));
        let app = self.app.clone();
        close.connect_clicked(move |_| app.quit());
        row.append(&close);
        content.append(&row);
        w.set_child(Some(&content));
        self.set_first_window(&w);
    }
}
