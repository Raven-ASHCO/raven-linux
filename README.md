# Raven for Linux

The native Linux app of [Raven](https://raven-messager.com), a private messenger with no central
server. It is written in Rust with [gtk4-rs](https://gtk-rs.org/) (`gtk4 = "=0.11.5"`, GTK 4.0 APIs
only, no libadwaita), follows the shared design in `core/desktop/design/` (the **Calm** design:
tokens version 2, texts, honest motion), and talks to Raven's engine only through the JSON contract
in `core/desktop/api/API.md`.

The engine (`raven-client`), the background program `raven-node`, the protocol and the shared design
live in the [`raven-core`](https://github.com/Raven-ASHCO/raven-core) repository, included here as a
git submodule at `core/`. Get the code with:

```sh
git clone --recurse-submodules https://github.com/Raven-ASHCO/raven-linux.git
cd raven-linux
# or, in an existing clone:
git submodule update --init
```

## Status

**In development.** There are no packages and nothing to download yet. The app shows what the engine
reports (`network.status`, `groups.list`) and offers what it allows. Since raven-core version 0.1.0 a
release engine reaches friends on the same local network directly and friends on other networks too:
over Tor with the default settings, and over Internet direct, peer-to-peer and router port mapping once
you turn listening on; friends can keep mail for each other, relay for each other and tell each other
their routes, and groups are on. Bluetooth nearby is macOS only: Linux release builds have no Bluetooth
backend. Every other way to reach friends is a lab feature that stays off. None of this is
independently reviewed yet (see `core/README.md`, "Status").

This is a standalone Cargo project with its own `Cargo.lock`; it is **not** part of the `core/node/`
workspace. Dependencies: `gtk4` (=0.11.5), `serde` (=1.0.229), `serde_json` (=1.0.151) and Raven's
engine, `raven-client` (`core/node/crates/raven-client`, by path, behind the cargo feature `engine`,
on by default). The node workspace's patch of `libsqlite3-sys` (its audited SQLCipher fork) is
repeated in `Cargo.toml`, because patches apply only from the root project. `Cargo.lock` keeps every
crate it shares with `core/node/Cargo.lock` at that version: `python3 tools/check-lock-sync.py`
checks it (CI runs it), and lists the few GTK-only crates on another semver line and the one allowed
exception (`pkg-config` 0.3.34, which GTK's `system-deps` 9 requires; the node workspace has 0.3.33).
When `core/node/Cargo.lock` changes, follow it with `cargo update -p <crate> --precise <its version>`.

## Build and run (Linux)

You need Rust 1.92 or newer ([rustup](https://rustup.rs/); CI and the node workspace, whose engine this
app links, use 1.98), a C compiler (the engine builds SQLite) and the GTK 4 development files
(GTK 4.0 or newer; Ubuntu 22.04 ships 4.6, Debian 12 ships 4.8):

| Distribution | Command |
|---|---|
| Debian 12+, Ubuntu 22.04+ | `sudo apt install build-essential pkg-config libgtk-4-dev` |
| Fedora | `sudo dnf install gcc pkgconf-pkg-config gtk4-devel` |
| Arch, Manjaro | `sudo pacman -S --needed base-devel pkgconf gtk4` |
| openSUSE | `sudo zypper install gcc pkg-config gtk4-devel` |

```sh
RAVEN_DATA_DIR=/tmp/raven-desktop-profile cargo run   # Raven's engine on a separate profile (see below)
RAVEN_DEMO=1 cargo run                      # debug build on the demo engine (MockRaven), first-run screens
RAVEN_MOCK_PROFILE=ready cargo run          # demo: skip Welcome/Setup, open the main window with demo friends
RAVEN_MOCK_PROFILE=locked cargo run         # demo: start on the Locked window (Unlock says "not in this release yet")
RAVEN_MOCK_PROFILE=ready RAVEN_MOCK_NETWORK=lab cargo run   # demo: the network screens as a lab build (below)
cargo test                                  # unit tests and the engine tests (no display needed)
cargo test --no-default-features --features engine   # the same without GTK (the engine path only)
cargo test --lib --no-default-features      # the GTK-free core only: needs neither GTK nor the engine
```

Every build runs on **Raven's engine** (`raven-client`, linked in): the CLI's own code on the CLI's profile
(`RAVEN_DATA_DIR`, else `~/.raven`), so the app, `raven` and `ash` share one identity and one friends list.
A `cargo run` without `RAVEN_DATA_DIR` therefore opens your real profile, exactly as the CLI does. The
engine starts the background program `raven-node` when the app needs it: `RAVEN_NODE_BIN` when set, else
`raven-node` next to the app's executable, else the CLI's lookup. From the source tree:

```sh
(cd core/node && cargo build --release -p raven-node)
RAVEN_NODE_BIN=$PWD/core/node/target/release/raven-node RAVEN_DATA_DIR=/tmp/raven-desktop-profile cargo run
```

`MockRaven` (demo friends and events, below) is compiled only into debug builds, and runs only when asked for:
`RAVEN_DEMO=1` (anything but empty or `0`) or `RAVEN_MOCK_PROFILE=ready|locked`. Release builds
(`cargo build --release`) do not contain it. A build without the engine (`--no-default-features
--features gui`) starts and shows "Part of Raven is missing from this installation".

Without GTK installed (for example on macOS) the GTK code can still be **type-checked**: the GTK `-sys`
crates skip their pkg-config probe when `DOCS_RS` is set, and `cargo check` does not link:

```sh
DOCS_RS=1 cargo check --all-targets && DOCS_RS=1 cargo clippy --all-targets -- -D warnings
```

The engine builds on macOS too (it is the CLI's code), so the engine tests run there:
`cargo test --no-default-features --features engine`.

### Install by hand (until packages exist)

```sh
cargo build --release
(cd core/node && cargo build --release -p raven-node)
ID=io.github.raven_ashco.Raven
sudo install -Dm755 target/release/raven-desktop /usr/local/bin/raven-desktop
sudo install -Dm755 core/node/target/release/raven-node /usr/local/bin/raven-node   # next to the app
sudo install -Dm644 data/$ID.desktop /usr/local/share/applications/$ID.desktop
sudo install -Dm644 data/$ID.metainfo.xml /usr/local/share/metainfo/$ID.metainfo.xml
for s in 64 128 192; do
  sudo install -Dm644 data/icons/hicolor/${s}x${s}/apps/$ID.png /usr/local/share/icons/hicolor/${s}x${s}/apps/$ID.png
done
```

## Layout

| Path | What it does |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Standalone project `raven-desktop-linux`: lib `raven_desktop` (GTK-free) and bin `raven-desktop` (feature `gui`); feature `engine` links `raven-client` (both on by default); the `libsqlite3-sys` patch of the node workspace |
| `src/lib.rs` | The GTK-free core and the app id |
| `src/api.rs` | API.md in Rust: `RavenEngine` trait (`call(op, args) -> Result<Value, ApiError>` plus typed wrappers), all types, the envelope, the refusal of secret-looking fields, display sanitising of every decoded text, the polling intervals |
| `src/worker.rs` | `EngineWorker`: the engine's two threads (calls in order on `raven-engine`, `events.poll` on its own lane `raven-events`); jobs over channels, answers as small futures the GTK main loop awaits |
| `src/tray.rs` | The tray menu (StatusNotifierItem + dbusmenu), GTK-free: lines, actions, dbusmenu properties and revisions, the icon pixmap, the close rule |
| `src/mock.rs`, `src/mock/groups.rs` | `MockRaven` (debug builds only, on `RAVEN_DEMO=1` / `RAVEN_MOCK_PROFILE`): the in-memory engine with the demo friends, the demo group and the demo event stream; `demo_requested` |
| `src/client.rs` | `ClientEngine`: `raven_client::call()` (the process's engine); without the `engine` feature every call answers `NOT_INITIALIZED` |
| `tests/engine.rs` | The real engine through the app's types and engine threads, on a throwaway profile (`target/tmp/engine-lab/`) with the file-backed secret stores: `app.info`, `profile.state`, `identity.create`, `identity.me`, settings round trip, privacy, the network ops and the network screens' view models, groups not offered without the lab switches, errors, `events.poll` beside a call |
| `tools/check-lock-sync.py` | Checks that `Cargo.lock` uses `core/node/Cargo.lock`'s versions (the engine's whole tree identical; Python 3.11+, standard library) |
| `src/strings.rs` | Loads `core/desktop/design/strings.en.json` (embedded; the only source of texts); `tr`/`trf`/`tr_list` |
| `src/theme.rs` | Parses `core/desktop/design/tokens.json` version 2 (embedded: palette, shadows, type ramp, radii, layout, durations, curves) and generates the Calm GTK stylesheet for light and dark, with or without motion; the avatar rule, the path and section colours |
| `src/icons.rs`, `src/mark.rs` | The Calm icons as drawing operations (a small SVG path reader: lines, curves, arcs) and the Raven mark as dots from `core/desktop/design/assets/raven-logo-braille.txt` (pure, tested) |
| `src/groupview.rs` | What the group screens say: the sidebar line, the header, "Delivered to 3 of 5" and its note, the ring's arcs, each member's receipt line (pure, tested) |
| `src/settingsview.rs`, `src/appprefs.rs` | The seven Settings sections and which way the content slides; Theme and Show technical details, kept in the app's own `$XDG_CONFIG_HOME/io.github.raven_ashco.Raven/app.conf` (never in Raven's profile) |
| `src/sanitize.rs` | Removes control, bidi and zero-width characters from engine text |
| `src/present.rs` | What screens say: status lines, tick texts and icons, header line, network texts, the message path chip and its colour by the tokens' `pathRule` (pure, tested) |
| `src/netview.rs` | The Network page, sidebar and tray from `network.status`: one row per way (state chip, plain line, switch where `network.set` allows it), this computer, router port opening, relays, web proxy, Bluetooth nearby (BlueZ `Privacy = device`), who carries mail, help-friends flags, the lab banner; Check network rows (pure, tested) |
| `src/netflow.rs` | Changing a network setting on the engine thread: consent sheets (`agreed: true` only after Agree), the refusal of a held way, the restart prompt and the restart (`node.stop` + `node.start`, a login service left running) (tested against the mock and canned answers) |
| `src/routesview.rs` | A friend's routes panel and its form (`contacts.routes` / `contacts.set_routes`) (pure, tested) |
| `src/secview.rs` | Security notices, Lock while running (Lock v2, the locked screen) and Updates (read-only) (pure, tested) |
| `src/timefmt.rs` | `{when}`, `{time}`, `{left}`, `{secs}` and the chat's day lines without a date library (the GUI passes the local UTC offset from GLib) |
| `src/words.rs` | Splitting typed security words (position numbers, Persian/Arabic-Indic digits, separators); normalising a typed old code; your old code in groups of four for display |
| `src/motion.rs` | The honest-motion policy, the tokens' cubic-bezier curves (springs that overshoot) and tweens, and the route-line packet maths |
| `src/main.rs`, `src/ui/` | The GTK front end (below) |
| `src/ui/app.rs` | Startup, the shared context and its engine calls (`Ctx::call` / `Ctx::run`), `events.poll` (500 ms with a window open, 5 s without), the 1-second clock, actions, quitting |
| `src/ui/style.rs` | CSS provider, light/dark following, Theme (System/Light/Dark), motion (the desktop's Animations switch and Raven's Reduced motion) |
| `src/ui/calm.rs` | The Calm building blocks: icons and the mark drawn in the text colour, `Clamp` (content widths), frame-clock tweens, the tick mark, path chips, the group ring, the segmented control, the gliding list, Settings cards and rows |
| `src/ui/a11y.rs` | Accessible names and descriptions (GTK 4.0's `gtk_accessible_update_property_value`, declared here because gtk4-rs exposes it only with `v4_10`) |
| `src/ui/welcome.rs` | Welcome and Setup on a card with its step strip (name, receive, the identity being made, your security words); whom Raven reaches follows `network.status` ("the same network" only while LAN direct is the one way on) |
| `src/ui/main_window.rs`, `sidebar.rs`, `chat_view.rs` | The window: the sidebar (mark and heartbeat, You, Search, Friends, Groups, Add a friend, New group, Network, Security words, Security, Settings), the chat (header, route line, day lines, bubbles, ticks, chips, group rings, the away card, the toast pill, the composer), and the handling of every engine event |
| `src/ui/group_sheets.rs` | New group and Members (add, remove, leave) |
| `src/ui/settings.rs` | Settings: General, Appearance, Privacy (Erase sheet), Lock (Lock sheet), Network, Updates, About |
| `src/ui/words_page.rs` | Security words (your words, old code, fingerprint, your card) |
| `src/ui/network_page.rs` | The Network page, its consent sheet and restart prompt, and the Check network dialog |
| `src/ui/security_page.rs`, `src/ui/lock_screen.rs` | The Security page (notices) and the locked window |
| `src/ui/routes.rs` | A friend's Routes panel (from the chat header) |
| `src/ui/route_line.rs` | The route line: You ── path ── friend, with its packets (`DrawingArea`) |
| `src/ui/check_words.rs`, `add_friend.rs`, `message_details.rs`, `quit.rs` | Dialogs (sheets); Add a friend shows the engine's `card.parse` `note` under the pasted card's summary when it left unsigned routes out of the card |
| `src/ui/tray.rs` | The tray icon: `org.kde.StatusNotifierItem` and `com.canonical.dbusmenu` exported with gio's D-Bus API on the app's own session-bus connection |
| `src/ui/widgets.rs` | Labels, Calm buttons, sheets, avatar sizes with presence dot and retry ring, the read-aloud word cards, the step strip |
| `data/` | `.desktop` file, AppStream metainfo, hicolor icons (64, 128, 192 px from `core/desktop/design/assets/raven-logo-*.png`) |
| `.github/workflows/linux.yml` | CI: ubuntu-latest, `libgtk-4-dev`, the lock check, fmt, `cargo build/test --locked` with GTK and the engine, the engine without GTK, the core alone, clippy (debug, no features, release), a release build (checked to link `raven-client` and not contain the mock), a release `raven-node`, both kept as the artifact `raven-desktop-linux-x86_64` (7 days) |

## How the design is implemented

- **Engine calls off the main thread.** `raven_call` may block (`node.start` can take a few seconds, API.md),
  so the GTK main thread never calls the engine. `EngineWorker` (`src/worker.rs`, standard library only)
  shares the engine between two threads. Every call except `events.poll` runs on `raven-engine`, one
  after another in the order sent; `events.poll` has its own lane, `raven-events`, so a poll never waits
  behind a slow call such as `node.start` (API.md: "`events.poll` never blocks"; `raven_call` is
  thread-safe). Each answer is a small future awaited with `glib::spawn_future_local` (glib's wakers may
  be woken from any thread). A job may make several calls that belong together (for example create the
  identity, save the receive choice, start the background program). Every answer checks that its window
  still exists, and answers for text that changed meanwhile (word completion, a pasted invite) are dropped.
  When the app quits, queued jobs on both lanes finish before the threads stop. Tests cover the order of
  calls, a poll answering while a `node.start` is held at a gate, panics on either lane and shutdown.
- **Polling.** `events.poll` every 500 ms while a window is open and every 5 s when none is (`api::poll`,
  API.md); a poll is never queued while the previous one is still on its way. The 1-second clock only
  updates texts. When the engine answers `reset: true` (it dropped events from its bounded buffer), the
  batch is applied without motion and the main window reads the friends, the open chat and the node status
  again (API.md, "Events").

- **Theme (Calm).** `theme.rs` turns every colour token into `@define-color raven_<name>` (light or dark),
  and the style classes use only those colours plus the spacing, radius, layout, shadow, type, duration and
  curve tokens (GTK 4 has no CSS variables or `color-mix()`, so tints are `alpha(@colour, n)` and each path
  colour has its own rules). Font sizes are in `rem`, relative to the system font (Calm's body 14 = 1rem),
  so the user's font size and scaling are kept. Settings › Appearance › Theme can force light or dark
  (kept in the app's own config file). Plain GTK 4 (no libadwaita)
  does not follow the desktop's dark preference by itself before GTK 4.20, so `ui/style.rs` reads the XDG
  desktop portal setting `org.freedesktop.appearance color-scheme` (GNOME, KDE and others), follows its
  `SettingChanged` signal, and falls back to `gtk-theme-name` ending in `-dark` or
  `gtk-application-prefer-dark-theme` from `settings.ini`. It then loads the matching stylesheet and sets
  `gtk-application-prefer-dark-theme` so GTK's own widgets match.
- **Honest motion (Calm).** Animations start only from engine events or the person's own action:
  `status_ok` beats the heartbeat dot (and, on the Network page, spreads the radar rings and beats the
  "in use" dots); `send_started`, `receipt` and `incoming` queue runs on the route line (a dot out, the
  green check back, a dot in; a frame-clock tick callback on the token curve `packet` that removes itself
  when the queue is empty); `messages.send` answering lifts your bubble out of the composer (520 ms,
  `lift`); a receipt fills the tick's check with one pop and one ripple and slides the path chip in;
  `incoming` raises the bubble with a soft ring for 1.5 s; `presence` blinks the dot and in-touch dots
  breathe; `retry` spreads a ring around the avatar; `group_changed` fades each member's arc of your
  group message's ring to its new colour. The person's own actions: the Settings selection glides
  (`glide`, with its overshoot) and the section slides in from the direction of travel, segmented
  thumbs slide, switches change colour, sheets rise in, the Lock sheet's shackle closes before Raven
  locks, a wrong passphrase shakes the locked card, Play a send in Appearance. "Delivered" appears when
  the receipt has travelled back, never before a `receipt` event. The only decorative motion is the
  mark's reveal on the welcome screen (once). Reduced motion (GNOME's Animations switch, i.e.
  `gtk-enable-animations`, or Raven's own setting) stops all of it, also the read-aloud stepping and the
  spinners, while texts still update: the stylesheet is loaded without any animation or transition,
  Raven's own switch also sets this process's `gtk-enable-animations` to false (GTK's switches, stacks and
  revealers stop too; it follows the desktop again when the switch goes off), the app's own tweens draw
  their end state at once, and a `reduced-motion` class on every window is a backstop. The first
  `events.poll` backlog is applied without motion (the window already loaded the current state).
- **Groups** (`core/desktop/api/API.md`, Groups; on in release builds since raven-core 0.1.0). The sidebar shows
  GROUPS only when `groups.list` says `available` (a build that holds groups says no; an engine without the
  ops answers an error: the same). A group
  chat shows each member's name in their avatar colour with a small avatar, "Passed on by …" when another
  member brought it, and under your message a ring with one arc per member (delivered, waiting, sent,
  failed), "Delivered to 3 of 5" and its note ("1 waiting · 1 not confirmed yet"), and Details with every
  member's state; a member counts as delivered only after its signed receipt. New group (a name and up
  to 15 of your friends: you become its one admin), Members (the admin adds friends and removes members;
  anyone else can leave; the admin can't), "You left / were removed" with the history kept. Events:
  `group_message` adds the bubble (or counts it unread), `group_changed` reads the group again.
- **Security words.** Check words shows your words with a read-aloud highlight and word 10 marked "check".
  The friend's words are typed into neutral chips (a chip only means "one of Raven's words"); the field
  completes a word after 3 letters via `verify.word_hint` (Tab picks it). There is no per-word right or
  wrong; the engine's `verify.check_words` gives the only result: match, typo or mismatch. After three
  mismatches Check is no longer offered. The friend's words are never shown anywhere (`card.parse` and
  `verify.*` never return them).
- **Old code.** Under the typed words, the small link "Older Raven? Use the old code instead" switches the
  dialog to your old code (to read aloud, in groups of four as in the design: `If4x 36FU omFi`; the
  welcome screen shows it the same way) and one plain field for the friend's (no chips, no completion,
  no per-character marks). The intro then talks about the code (`check.oldCode.intro`: capital and small
  letters are different, spaces and dashes don't matter). `verify.check_old_code` answers match (the friend is verified exactly as with
  the words) or mismatch; "Use the 10 words instead" switches back. Mismatches count towards the same
  limit of three.
- **Privacy actions.** Settings › Privacy reads `privacy.state`. While `available` is false (this
  release), "Encrypt everything" and "Erase Raven from this computer…" are disabled with "Preview, not in
  this release" under each (as in the Windows app). When available, the switch shows `encrypted` and turns
  encryption on (`privacy.encrypt`), and Erase opens a dialog that enables **Erase** only after "erase" is
  typed, calls `privacy.erase` and returns to Welcome. "Lock now" is in the tray menu, disabled while
  `privacy.state` says `available: false` and Lock while running (Lock v2) can't lock.
- **Sanitising.** Every text decoded from the engine passes through `sanitize.rs` in the typed wrappers
  (names, messages, paths, error texts, details), and labels are always plain text, never Pango markup.
  A response containing a field named like `seed`, `private_key`, `plaintext`, `recovery` or `mnemonic` is
  refused.
- **Tray.** `src/ui/tray.rs` exports a StatusNotifierItem (`org.kde.StatusNotifierItem` at
  `/StatusNotifierItem`, bus name `org.kde.StatusNotifierItem-<pid>-1`) with a `com.canonical.dbusmenu`
  menu at `/MenuBar`, using gio's D-Bus API that gtk4-rs re-exports (no extra crate), and registers it with
  `org.kde.StatusNotifierWatcher` (KDE Plasma, Xfce, Cinnamon, MATE, LXQt, Budgie, waybar; GNOME with the
  AppIndicator extension). The menu is the macOS and Windows one (`src/tray.rs`, GTK-free and tested):
  the status ("Raven is listening on Wi-Fi" / "Raven isn't receiving", then "♥ background program ok" or
  the "isn't responding" sentence; on Linux also "Ways on: …" and, in developer builds with lab
  switches, "Lab switches on"), the friends ("Ali · In touch 5 min ago", "2 new from Rena"; a click
  opens that chat), Open Raven, Add a friend…, Network…, Lock now (disabled until Lock can be used),
  "Quit window (keeps receiving)" and "Quit and stop the background program". A click on the icon opens Raven. The
  menu is recomputed on the 1-second clock, after each poll and when the panel is about to show it;
  `LayoutUpdated` and `NewToolTip` are sent only when a text changed. The icon is `IconName` =
  the app id (with `IconThemePath` pointing at `data/icons` in debug builds) plus an ARGB pixmap of the
  same picture for panels that cannot find it. With the icon on screen, closing the window hides Raven to
  it (polling every 5 s, like the Windows tray); if the watcher goes away, the hidden window comes back.
- **Network screens** (NETWORK_PARITY.md, "What the screens need next"; Linux: the network page in
  the GTK sidebar and the tray's status line). At the bottom of the sidebar, **Network** ("3 ways on"
  beside it) and **Security**; the window menu's "Network and status", the heartbeat beside the mark
  and the tray's Network… open the Network page. It is built from `network.status`
  (`src/netview.rs`): one row per way with its state chip (On, Off, "Preview, not in this release",
  "Lab only", "Not on this computer"), one plain line (what the way does, then the engine's reason),
  "In use now" when the background program says so, the CLI's row, the gate and (developer builds)
  the lab switches under Details, and a switch where `network.set` can change it (a held way's
  switch only moves while it is still on, to turn it off). Below: this computer (port, listening
  addresses, internet and p2p ports, a LAN meeting's end, and "Open on this network for a while":
  LAN direct on for 30 minutes to 24 hours with `lan_for`), router port opening (what Raven asks, the
  router's answer, NAT-PMP/PCP), relays in use (who lent them), the web proxy (auto, off or
  `http://host:port`; a user name or password is refused before it is sent), Bluetooth nearby (on
  Linux: BlueZ with `Privacy = device` in `/etc/bluetooth/main.conf`, else Raven keeps Bluetooth
  off; a public adapter address is called out), who carries mail for you and for whom you carry
  (with "Carry mail for friends" and "Hide this computer's IP address"), the bridge/store/relay
  flags with the engine's honest effects, and the protections. Developer builds whose engine sees
  `RAVEN_LAB_*` switches show "Lab switches on: …" at the top; release builds never do (the app's
  `debug_assertions` and the engine's `build` must both be dev). A change runs `netflow::set` on the
  engine thread and the page is rebuilt from the answer (no motion; focus returns to the switch):
  `carry_for_friends`, `friends_carry_mine` and `handoff` turned on show a consent sheet first (the
  engine's three, API.md's consent rule: what it does, what it costs, what others can and can't
  see, and the engine's own words without the terminal's `[y/N]`; "Not now" is the default button)
  and `agreed: true` is sent only from "I agree, turn it on", a `CONSENT_NEEDED` from the engine
  leads to the same sheet; relay for friends and the bridge/store/relay flags have no sheet, their
  honest effect stands beside the switch; `NOT_AVAILABLE_YET` shows the engine's
  words and nothing changes; `restart_needed` asks "Restart the background program?" (saying so when
  this app did not start the running program), Restart now runs `node.stop` then `node.start`, and a
  login service's program is left running with the `systemctl --user restart raven-node` hint; Later
  keeps a reminder with Restart now on the page. "Check network…" opens `network.check` (summary,
  next step, network lines) with "Ask my router" (read-only, up to about 4 s) behind its own button.
  The chat toolbar's **Routes** opens a friend's routes (`contacts.routes`): saved routes with chips
  and Remove, what the friend told this computer, who carries the friend's mail, hide my IP from
  this friend, and Change routes (Internet route, p2p route and relays, up to two verified mail
  friends). Each message shows a **path chip** from `path_kind` (LAN, Internet, p2p, Relay, Hole
  punched, 443, Nearby, Friend, Handoff, Letter), with the engine's full path words as its tooltip;
  a receipt's `path_kind` updates it. The **Security** page shows the notices (resolving stays in
  the terminal); Lock (Lock now…, when the background program serves Lock v2) and Updates (read-only;
  installing stays in the terminal) are sections of Settings. When `lock.state` says locked (at start, after Lock now, or a
  `raven lock` seen by the 10-second check) the main window gives way to the locked screen, whose
  passphrase goes once to `lock.unlock` and is cleared; it comes back when unlocked here or
  elsewhere. The tray menu adds "Ways on: …", "Lab switches on" (developer builds with switches)
  and Network…; its Lock now uses Lock v2 when it can.
- **Background without a tray.** When no StatusNotifierWatcher is on the bus (or no panel shows items),
  nothing changes from before: the window's menu has the tray's items (Add a friend…, Network and
  status, Settings, "Quit window (keeps receiving)", "Quit and stop the background program"); closing the
  window follows Settings › When I quit; "Ask me" shows the Quit dialog that explains what keeps running;
  "Keep Raven running" quits the window and sends a desktop notification with the same explanation. The
  background program (`raven-node`) is a separate process, so it keeps receiving after the window closes.

## What is mocked

Debug builds started with `RAVEN_DEMO=1` or `RAVEN_MOCK_PROFILE` run on `MockRaven` (`src/mock.rs`), which
goes through the same JSON envelope as the real engine:

- Your identity is the public test vector "alice" of the security-words design (words
  `turtle frog bread wolf paper laptop funnel mango honey wagon`, old code `If4x-36FU-omFi`).
- Friends: **Ali** (verified, in touch), **Mina** (verified, one message waiting), **Rena** (not verified).
  To see a **match** in Check words with Rena, type `anchor basket candle eagle garden helmet island jacket
  kettle milk` (demo values; the app never displays a friend's words). Change one word to see **typo**;
  type your own words to see **mismatch**. With the old code, Rena's is `Tq7m-29Xa-hKe4` (the same in the
  macOS and Windows mocks): it is case-sensitive; spaces, dashes and Persian or Arabic-Indic digits are
  accepted. Other friends have no old code in the mock, so it never matches for them.
- The word list is Raven's draft 256-word list
  (`core/desktop/design/raven-security-words-v1.txt`, embedded in debug builds
  only) with the real GF(2^8) check word.
- Demo events after the main window opens: a heartbeat every 2 s; at 1.2 s a message from Ali; at 2.6 s a
  message from Rena; at 6 s a retry of Mina's waiting message; at 10.5 s Mina is back and the waiting
  message is delivered. Sending to Ali gives `send_started`, then "Sent, not confirmed yet" (0.8 s), then a
  signed receipt (1.0 s). Sending to a friend who is not in touch (Rena before his message arrives, or a
  friend you just added) waits for 24 hours: click the tick text for Message details ("Try now", "Stop
  trying"). Mina's waiting message shows the same until she comes back.
- Add a friend accepts an invite such as
  `raven:rvn1qsamdemo8x7k2v4m:0123456789abcdef0123?lan=192.168.1.50:7420` (any `raven:<rvn1 address>:<hex
  key>`; without `?lan=` the friend has no address and messages are "Not sent: meet first").
- Errors use the engine texts of `strings.en.json` (`error.*`), with the API's codes; an unknown operation
  or bad arguments answer `BAD_REQUEST`.
- Privacy: `privacy.state` answers `available: false`; `privacy.lock`, `privacy.unlock`,
  `privacy.encrypt` and `privacy.erase` answer `NOT_AVAILABLE_YET` (erase without `confirm: "erase"` is a
  `BAD_REQUEST`). `RAVEN_MOCK_PROFILE=locked` starts locked, to review the Locked window.
- Groups: one demo group, **Weekend trip** (admin Ali; you, Mina, Rena and two members who are not your
  friends, Sara and Noor), with yesterday's and today's messages and your message delivered to 3 of 5.
  Demo events: at 4 s a message from Noor (passed on by Ali), at 7.5 s Noor's receipt, at 10.8 s Mina's
  (she comes back). Sending to the group confirms friends in touch in about a second and members who
  aren't your friends through Ali; Mina's copy waits. New group, add, remove and leave follow API.md
  (16 members at most, you included; the admin can't leave).
- Network: by default the answers API.md's Mocks section describes (the release state before version
  0.1.0): LAN direct on, every other way Preview, a held way refused with `NOT_AVAILABLE_YET`, Lock v2, security notices and updates not in
  this version. `RAVEN_MOCK_NETWORK=lab` answers as a developer build with lab switches set, to
  review every screen state: the ways the switches open are Off until switched on, then Lab only;
  `carry_for_friends`, `friends_carry_mine` and `handoff` answer `CONSENT_NEEDED` without
  `agreed: true`; Internet direct, p2p, port mapping and the proxy apply at a restart
  (`restart_needed`; in use only after `node.start`, then relays and a mapped port appear); the web
  proxy stays held; Bluetooth nearby shows a public adapter address (Raven keeps it off: the BlueZ
  explanation); Ali told this computer about Mina as a mail friend; Lock now locks, and Unlock takes
  the demo passphrase `demo passphrase` (no passphrase: `PASSPHRASE_NEEDED`, another: `PASSPHRASE_WRONG`,
  three wrong: `UNLOCK_WAIT` for 30 s); one security notice about Rena; an updates view.
  `RAVEN_MOCK_NETWORK=lab-no-bluetooth` is the same on a computer without Bluetooth LE (nearby "Not on
  this computer"). `node.stop` says what happened (`stopped` / `not_running`).

## What is verified and what is not

The **Calm redesign** (2026-10-28) was written on macOS without GTK. Verified: `cargo test --lib
--no-default-features` (146 tests: tokens version 2 and the Calm stylesheet for both schemes, the reduced
stylesheet without any animation or transition, curves and tweens, every icon inside its box and the
SVG path reader (arcs, smooth curves, relative commands), the mark's dots, day lines, tick icons and tones,
path colours by `pathRule`, the group texts and ring, the groups API types and events, the mock's groups
(create, add, remove, leave, the 16-member cap, receipts filling one at a time, unread), the app's own
preferences file, the Settings sections); `cargo test --no-default-features --features engine` (also the
real engine answering `groups.list` with `available: false` and `groups.send` with `NOT_AVAILABLE_YET`
without the lab switches); `DOCS_RS=1 cargo clippy --all-targets -- -D warnings` (debug and `--release`),
`cargo clippy --all-targets --no-default-features -- -D warnings`, `cargo fmt --check`,
`RAVEN_APP_LINUX=. python3 core/desktop/design/check_strings.py`, `tools/check-lock-sync.py`. **Not verified** (type-checked only, never
linked or run): everything visual and every motion: that GTK parses the stylesheet without warnings
(numeric `font-weight`, `rem`, `filter`, `transform` in keyframes, `outline-offset`, gradients with point
stops in the header bar), that CSS `transform` animations (lift, pop, ripple, radar, shake, view and
section entrances) run and look like the prototype, the `Clamp` container's sizes, the glider's and the
thumb's positions (`compute_bounds` after the first frame), the Fixed-under-Overlay layering, the cairo
icons, mark, ticks, ring and route line, the switch's knob size from the tokens, focus order and arrow
keys in the sidebar and Settings, the accessible names set through `gtk_accessible_update_property_value`
and the heading roles, the theme file being written, the in-process `gtk-enable-animations` override and
its reset, the welcome forge, and the group screens against a real lab engine (`RAVEN_LAB_TEST_A=1
RAVEN_LAB_HORIZON=1 RAVEN_LAB_GROUPS=1`).

Before the redesign:

Verified here (on macOS, without GTK), for the network screens: `cargo test --lib
--no-default-features` (108 tests: every way state and its switch, LAN on for a meeting, the lab
banner only in developer builds with switches, this computer, port mapping, relays, the proxy
values (no credentials),
nearby with the BlueZ note and the public-address warning, mail both ways, help flags, the tray and
sidebar line, Check network rows, the consent flow (sheet first, `CONSENT_NEEDED`, agree), the
refusal of a held way (`NOT_AVAILABLE_YET`), the restart prompt and every `node.stop` outcome
(`stopped`, `managed` with the systemctl hint, `advice`, `still_running`, `failed`, a failed start),
friend routes and their form, security notices, Lock v2 and the unlock errors, updates, every
path chip, the tray's network lines and Network…, all against fixtures, canned engine answers and
the mock in release and lab mode); `cargo test --no-default-features --features engine` (also the
engine test that builds these view models from the real engine's `network.status`, `network.check`,
`security.status`, `lock.state` and `updates.status` on a throwaway profile); `DOCS_RS=1 cargo check
--all-targets` and clippy `-D warnings` (debug, release, without features, `gui` without the engine).
The network screens' GTK code is **type-checked only, not run**: not the layout, the chips' CSS, the
dialogs, the switches and focus after a change, the mnemonic labels as accessible names, the locked
screen, the routes panel, the path chips, the sidebar entries or the tray's new lines on a panel.

Verified earlier (on macOS, without GTK): `cargo test --no-default-features --features engine` (72 unit tests
and the 8 engine tests of `tests/engine.rs`, on Raven's real engine with a throwaway profile and the
file-backed secret stores: `app.info`, `profile.state` new then ready, `identity.create` (twice: the identity
is kept), `identity.me`, the settings round trip into `desktop_settings.json` (owner-only), a refused setting,
`privacy.*`, `BAD_REQUEST` and `NOT_A_CONTACT`, `events.poll` on the events lane beside `node.status` on the
calls lane); `python3 tools/check-lock-sync.py`; `cargo test --lib --no-default-features` (73 tests: API envelope and
events, secret-field refusal, the new results and the polling intervals, MockRaven including the demo event
stream, sending, match/typo/mismatch, the old code (match, mismatch, case, Persian and Arabic-Indic
digits, groups of four, its own intro), the privacy operations (`NOT_AVAILABLE_YET`), `BAD_REQUEST` and
the engine error texts, word hints and adding friends, the engine threads (calls in order, a poll that
does not wait behind a held `node.start`, errors and panics on both lanes, shutdown, the mock behind
them), the tray menu (lines and order, actions, Lock, dbusmenu properties and revisions, pixmaps, the
close rule), sanitiser, strings (every key the code uses exists in `strings.en.json`), CSS generation from tokens
for both schemes, time texts, word and old-code input, presence and tick texts, motion policy);
`DOCS_RS=1 cargo check --all-targets` and `cargo clippy -D warnings` for the GTK code with the engine, also
`--release` and without features (it type-checks against gtk4 0.11.5 with no GTK version features, so it uses
GTK 4.0 APIs only); `cargo fmt --check`.

Not verified (no GTK on the machine that wrote it): linking, running, and anything visual. In particular:
that the stylesheet parses without GTK warnings and looks like the mockup; the CSS keyframe animations
(`transform: scale()` on the ♥, the retry ring and the logo); the route-line drawing; the portal colour-scheme
reading on real desktops; the quit notification being delivered while the app quits (and GNOME shows it
only for an installed .desktop file); keyboard handling in Check words (Tab, Backspace); the old-code
switch and the Privacy page with the Preview labels; answers from the engine threads reaching the GTK
main loop (glib futures woken from another thread); focus and sizes of dialogs; behaviour on Wayland and
X11; the tray on a real panel (registration with the watcher, the host showing the icon and menu,
clicks, the icon lookup, hiding to the tray and the window coming back when the watcher goes away; the
D-Bus glue only type-checks here); the engine linked into the GTK binary, and the app running on it
(Setup creating a real identity, starting `raven-node` with `receive` on, sending, the Secret Service or
passphrase vault instead of the test stores); the `reset` reload. The CI workflow builds and tests on
Ubuntu (with the engine) but does not start the app.

## Packaging plan

Install layout for every format: `bin/raven-desktop` and `bin/raven-node` (the engine finds the background
program next to the app's executable; CI builds both), `share/applications/<id>.desktop`,
`share/metainfo/<id>.metainfo.xml`, `share/icons/hicolor/{64x64,128x128,192x192}/apps/<id>.png`, with
`<id>` = `io.github.raven_ashco.Raven`.

1. **deb** (Debian, Ubuntu): `dpkg-deb` from a staged tree (or `cargo-deb` if approved), `Depends:
   libgtk-4-1 (>= 4.6)`, plus `raven-node` as a dependency or bundled (how the background program is
   started on login, for example a systemd user unit, is decided with the engine work).
2. **AppImage**: `linuxdeploy` with its GTK plugin bundling GTK 4, built on the oldest supported base
   (Ubuntu 22.04); the background program shipped alongside and started by the app.
3. **Flatpak** (Flathub): `org.gnome.Platform` runtime with the `rust-stable` SDK extension, crates vendored
   with `flatpak-cargo-generator`, `finish-args`: `--share=network`, `--socket=wayland`,
   `--socket=fallback-x11`, `--device=dri`. Open question: the background program must outlive the app,
   which in a sandbox needs the Background portal (autostart) or running `raven-node` inside the sandbox;
   to decide with the engine work. Flathub also needs screenshots in the metainfo.

Signing: deb repositories signed with the project key; AppImage with an embedded signature
(`--sign`); Flatpak through Flathub.

## Open issues

- **App id**: decided (`core/desktop/README.md`, "App identity"): `io.github.raven_ashco.Raven` on Linux,
  `io.github.raven-ashco.Raven` on macOS and Windows. Changing it means renaming the files in `data/` and
  `APP_ID`.
- **Engine**: linked (`src/client.rs`, feature `engine`). A build without it shows
  `error.engineMissing` ("Part of Raven is missing from this installation. Install Raven again, then
  open it.").
- **Retry of an ended message**: the engine sends it again as a new message with a new id (`replaces`,
  API.md); the chat shows it as a new bubble below the old one instead of in its place.
- **Lock file**: `pkg-config` is 0.3.34 here and 0.3.33 in `core/node/Cargo.lock` (GTK needs the newer one);
  moving `core/node/Cargo.lock` to 0.3.34 would make the engine's tree identical without an exception.
- **Privacy pack**: "Encrypt everything" and "Erase Raven from this computer…" wait until the engine
  offers them (shown as Preview; the code for the available case is in place but has only run against
  "not available"). The engine already encrypts where the build does and refuses an unfinished erase;
  the Locked screen's Unlock clears a `raven lock` through the engine (`privacy.unlock`).
- **Tray**: the icon does not "breathe" while listening (the macOS menu-bar mark and the Windows tray icon
  do); on Linux that would mean sending `NewIcon` with alternating pixmaps every second.
- **Blind-entry rule 5** (Enter ignored for 300 ms after a paste) is not implemented yet.
- The read-aloud highlight steps automatically while the words are on screen (as in the mockup); a
  start/stop control may be friendlier.
- The in-touch "breathing" dot is an endless CSS animation, so a window with a friend in touch redraws
  that dot continuously (small, but not "still").
- **Accessible names**: the app uses GTK 4.0 APIs only and gtk4-rs offers `update_property`
  from GTK 4.10, so `ui/a11y.rs` declares GTK 4.0's `gtk_accessible_update_property_value` itself
  for icon-and-text buttons, chips, rows and groups of choices; switches and fields are also named
  by their label (mnemonic widget). Unverified on a screen reader.
- **Calm adapted to GTK/GNOME**: sheets are modal dialog windows attached to the main window
  (GNOME's own way) instead of in-window sheets over a blurred scrim, and the locked window shows
  only its card (nothing of Raven behind it); GtkSwitch moves its knob with GTK's own animation
  (the tokens' `knob` spring applies to its colour only); the bubble lifts a fixed 34 points from
  below instead of from the composer's exact position (CSS keyframes); the old section leaves at
  once instead of sliding out (only the new one slides in); "Start Raven when I log in" and "Lock
  when idle" of the prototype are not shown (the engine has no such setting yet).
- **Brand assets**: the icons in `data/icons/` are the Raven logo, reserved brand material
  (`core/ASSET_LICENSE.md`, `core/TRADEMARK.md`); forks must replace them.

## License

Copyright (C) 2026 ASHCO. GNU Affero General Public License v3.0 (`LICENSE`; notice and pending
additional terms in `NOTICE`). The Raven name, logo and app icons are covered
separately by `core/TRADEMARK.md` and `core/ASSET_LICENSE.md`. Security issues: please write to
security@raven-messager.com (`SECURITY.md`), not a public issue.
