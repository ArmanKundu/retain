# Retain

A study tracker for VCE, built for macOS. Times your sessions honestly, shows you a year of work
at a glance, and keeps a streak you earn by studying rather than by opening the app.

Local SQLite, no account, no server, no telemetry. Everything stays on your Mac.

> **Checkpoint 1 of 3.** What's here: onboarding, subjects, settings, the timer with its menu bar
> item, the contribution grid, streaks and weekly goals. Quick capture, flashcards, the error log,
> exams, notifications, Compass import and the AI features come in later checkpoints.

<!-- Screenshot goes here: docs/screenshot.png
     Left deliberately empty. A screenshot of this app on a fresh install is an
     empty grid and a zero streak, which sells nothing and shows less. Take one
     after a fortnight of real use — a grid with actual colour in it is the whole
     pitch — and drop it in as:
       ![Retain](docs/screenshot.png) -->


---

## Installing

Retain is distributed unsigned, because an Apple Developer ID costs $149/year and this is a tool
for about four people. macOS will refuse to open it the first time. That is expected, and the way
around it takes about fifteen seconds.

1. Download `Retain_x.y.z_x64.dmg` from [Releases](../../releases), open it, and drag **Retain** to
   your Applications folder.

2. **Try to open it.** Double-click Retain in Applications. macOS will show a dialog saying it
   can't verify the developer, with only a **Done** button. Click Done.

   This step is not optional and not a mistake — the button you need in step 3 only appears *after*
   macOS has blocked a launch attempt, and Apple documents it as lasting about an hour from then.

3. Open **System Settings → Privacy & Security**, scroll down to the **Security** section. You'll
   see a line saying *"Retain" was blocked to protect your Mac*. Click **Open Anyway**.

4. Enter your login password, then click **Open** on the confirmation dialog.

That's once, ever — after that Retain opens normally by double-clicking.

<details>
<summary>Why there's no right-click → Open step</summary>

Older guides tell you to Control-click the app and choose Open. **That no longer works.** macOS 15
(Sequoia) removed the Control-click bypass for apps that aren't signed and notarised; the only
supported route is the Privacy & Security panel above. If you're on macOS 14 or earlier the
Control-click shortcut still works, but the steps above work everywhere.

</details>

<details>
<summary>If "Open Anyway" doesn't appear</summary>

Some download tools flag files more aggressively. You can clear the quarantine attribute directly:

```bash
xattr -dr com.apple.quarantine /Applications/Retain.app
```

This is a fallback, not the normal path — try the steps above first. Only run it on software you
actually trust, since it's the check being skipped.

</details>

### Apple Silicon

Retain is built for Intel (`x86_64`) only, and runs on Apple Silicon under Rosetta 2. If Rosetta
isn't installed, macOS offers to install it the first time you open the app. A universal binary is
a deliberate non-goal — the build machine is an Intel Mac with a Homebrew Rust toolchain and no
`rustup`, so a second target can't be added.

---

## What it does

### Timer

Pick a subject, choose stopwatch or Pomodoro (25/5, 50/10, or your own), and go. The elapsed time
lives in your menu bar, so you never have to bring the window up to check it.

The timer **pauses itself after two minutes with no keyboard or mouse activity**, and picks back up
the moment you touch anything. It also backdates that pause to when you actually stopped, not when
it noticed — so walking away doesn't quietly earn you two free minutes each time.

There are two numbers per session: elapsed time, and **active time** — elapsed minus every pause,
idle stretch and break. Active time is the one that counts toward everything. Pause counts are
shown rather than hidden, because a session with eleven pauses is worth knowing about.

On stop you're offered a one-line note. It's optional and dismissible, and you get asked every time.

### Contribution grid and streak

A year of study as a grid. Each cell is tinted with the colour of whatever subject you spent the
most time on that day, so you can see at a glance that you've done nothing but Biology for a
fortnight. Hovering breaks the day down by subject.

A day is earned by **one focused session** (20 minutes of active time by default, adjustable) **or
by clearing every review due that day**. Opening the app earns nothing.

Two safety valves: **freezes**, which cover a missed day automatically and regenerate one per seven
earned days up to two, and **rest days**, weekdays you nominate that never break a run.

The full rule, and why the default is 20 minutes rather than 25, is in
[docs/streak-rule.md](docs/streak-rule.md).

### Weekly goals

Set an hours-per-week target per subject and get an Apple-Watch-style ring for each.

### School calendar

Retain can subscribe to an **ICS calendar address** — the published feed URL Compass gives you
under its calendar settings — and show what's on today and tomorrow on the Today screen.

That address is the entire integration. Retain never signs in to Compass, never scrapes a page,
and never touches an unofficial API. There is no code path that could: it fetches one URL over
HTTPS and parses the calendar file it gets back.

Recurring events are expanded in **their own timezone**, which is what keeps a Wednesday 9am class
at 9am on both sides of daylight saving rather than drifting an hour for half the year. Instances
that were individually moved or cancelled replace the original slot instead of appearing twice, and
syncing replaces the stored events rather than merging, so a cancelled class actually disappears.

If a sync fails, the previously fetched events stay put and the error is shown in Settings. An
unreachable calendar is a normal state, not a broken app.

### Biology 3/4

A topic tree, an exam simulation, a terminology deck and a command-word reference.

**Retain ships no VCAA content.** There are no dot points, Area of Study titles or key-knowledge
text in the app. The study design is VCAA's document, it changes between accreditation periods, and
content invented to look plausible would be worse than none — you'd revise against it and never
find out. Instead you paste your own copy of the outline and Retain builds the tree from it,
indentation setting the nesting.

The exam simulation runs 15 minutes reading then 2 hours 30 writing, with the phase change marked.
It's stored as a start instant rather than a countdown, so quitting mid-exam and reopening resumes
where you actually are. Pausing is allowed but banked separately, so a broken-up attempt isn't
logged as a clean sitting.

Biology 3/4 also gets extra error-log categories — terminology, process/mechanism, experimental
design, data interpretation, command word, genetics, cell biology, immunity and so on — on top of
the generic Science ones. These appear for Biology at 3/4 level and nowhere else.

### Your material, and the library

Retain can hold your own study material — the VCAA study design, past papers,
class notes — under **Library → Materials**. It stores the extracted text, splits
it into passages and indexes them with SQLite's full-text search.

When you then ask for notes or a practice question, the relevant passages are
retrieved and put in front of the model as authoritative context, and the
excerpts used are shown underneath the answer. That turns "write me notes on
protein synthesis" from a recollection of VCE Biology into something written
from your actual documents.

It's keyword search, not semantic search. There are no embeddings and no vector
database, because both would mean either a network round trip per query or a
model file several times the size of the app. Ask about "protein synthesis" and
it finds pages containing those words; ask about "how cells make things" and it
may not. **Check coverage** on the same screen tells you what would be retrieved
before you spend a request finding out.

PDFs have to be pasted rather than dropped — the webview has no PDF parser, and
accepting the file only to store nothing would be worse than saying so.

Everything the AI writes is kept automatically under **Library → Saved**: notes,
practice questions, weekly reviews. Each item records what was asked, which
model wrote it and when, and can be exported as Markdown or printed. There is no
save button, because a save button you have to remember is a feature that mostly
doesn't happen.

### The assistant

A conversation view that answers from **your own material**, with a toggle for
how far it may stray.

**Strict** is the default and is the point of the feature: it answers from what
you've uploaded, and when your notes don't cover something it says so rather than
filling the gap. A model asked about VCE Biology will answer confidently either
way — the difference is whether you can check it. Switch to **Material + general
knowledge** and it uses both, labelling which is which.

Every answer shows the passages it used, so a bad retrieval is visible rather
than silently shaping what you revise from. It can also see what's due, what's
coming up and this week's hours, so questions about your actual schedule work.

It doesn't *act*. It won't create a card or move an assessment — a model that
silently writes to your deck is one whose mistakes you inherit. It points you at
the screen instead.

You can attach files to a single question; those are scoped to that message and
don't join the searchable library.

### Subject folders

Retain creates `~/Documents/Retain/<Subject>/` for each of your subjects. Drop a
term's PDFs into the right one, press **Sync** on the Library screen, and they're
read and indexed — the subject comes from the folder, so there's no per-file
tagging. Files already read are skipped, so syncing again after adding two PDFs
costs two files of work.

PDFs are read directly now. A **scanned** PDF has no text layer — its pages are
images — and that's reported as its own outcome rather than stored as an empty
document. Word and Pages files are refused with the instruction to export as PDF
first, rather than half-parsed.

### Updates

Retain checks [GitHub Releases](https://github.com/ArmanKundu/retain/releases) about once a day, in
the background, and tells you if a newer version exists. There's also a **Check now** button in
Settings.

To publish a new version, from a clean checkout:

```bash
./scripts/release.sh 0.3.0
```

That bumps the version in the three files that carry it, runs the tests, tags and pushes. GitHub
Actions builds the DMG on a macOS runner and attaches it to the release. Anyone running Retain sees
the update within a day and downloads the DMG themselves — no Rust toolchain needed to receive one.

It downloads nothing and installs nothing. That's deliberate: the app is ad-hoc signed, so a
program that silently replaced its own bundle would be indistinguishable from something malicious
doing the same. Updating means downloading the new DMG yourself.

Nothing about you or this Mac is sent — it's an unauthenticated GET of a public JSON endpoint, the
same request visiting the releases page makes. "Couldn't check" is shown as its own state, never
disguised as "up to date".

---

## Your data

One SQLite file at `~/Library/Application Support/com.armankundu.retain/retain.db`.

- **Snapshots.** A consistent copy is written on every launch, keeping the last seven, in
  `snapshots/` beside the database. Written with `VACUUM INTO`, so each one is a single valid
  database file rather than a copy that might have been taken mid-write.
- **Export.** Settings → Data → Export everything writes every table to JSON in your Downloads
  folder. The exporter reads the table list from the database itself, so it stays complete as the
  app grows rather than silently falling behind.
- **Restore.** Import replaces everything, after taking a snapshot first.

API keys are **not** in the database. They're in your macOS Keychain, and Retain has no code path
that reads one back out to the interface — it can only check whether one exists. That also means
exports never contain credentials.

> **A note on iCloud.** The brief for this app asked for an option to keep the database in iCloud
> Drive for free cross-Mac sync. It isn't offered, because it corrupts data: a SQLite database in
> WAL mode is three files that iCloud syncs independently, there's no cross-machine file locking,
> and iCloud can evict file contents out from under an open database. The full analysis, and the
> snapshot-based design that would actually be safe, is in
> [docs/icloud-sqlite-analysis.md](docs/icloud-sqlite-analysis.md). Use the JSON export to move
> between Macs.

---

## Deliberate non-goals

- **No Compass scraping.** Retain will read your Compass calendar over its ICS subscription URL
  (Checkpoint 3) and nothing else. No scraper, no unofficial API wrapper, no login flow, and it
  will never ask for your school password. Those tools need your credentials, break whenever
  Compass changes an endpoint, and sit in terms-of-service grey territory — a prior open-source
  Compass frontend was taken down at the Department of Education's request.
- **No app or website blocking.** Screen Time and Family Controls are entitlement-gated, unreliable,
  and trivially bypassed.
- **No social features, leaderboards, study rooms, pets, avatars or XP.** You're competing with your
  own past self.
- **No backend or sync service. No mobile app. No universal binary.**

---

## Building from source

Requires Node 22+, Rust 1.85+ and the Xcode Command Line Tools.

```bash
npm install
npm run tauri:dev
```

To produce the DMG:

```bash
npm run tauri:build
```

The output lands in `src-tauri/target/x86_64-apple-darwin/release/bundle/dmg/`.

### Layout

```
src/                     React + TypeScript frontend
  components/            Shared UI primitives, grid, rings
  screens/               Onboarding, Today, Timer, Progress, Settings
  lib/                   Typed Tauri bindings, formatting, VCE subject catalogue
src-tauri/src/           Rust backend
  db.rs                  Connection, PRAGMAs, migrations, snapshots
  timer.rs               Session state machine and active-time accounting
  idle.rs                macOS idle detection via CoreGraphics
  streak.rs              Streak rule, contribution grid, weekly goals
  secrets.rs             Keychain-backed API key storage
  export.rs              Complete JSON export/import
  tray.rs                Menu bar item
docs/                    Design notes for decisions that needed one
```

The Rust is commented far more heavily than usual, on purpose.

### What each dependency is for

| Dependency | Why |
| --- | --- |
| `tauri` (with `tray-icon`) | App shell and the macOS menu bar item. The tray is core Tauri, not a plugin. |
| `tauri-plugin-notification` | Bridges `UNUserNotificationCenter`, including the onboarding permission request. |
| `tauri-plugin-opener` | Opens links in the default browser. |
| `rusqlite` (`bundled`) | SQLite, compiled into the binary. Chosen over `tauri-plugin-sql` so scheduling, streak and analytics logic stays in Rust instead of being pushed into TypeScript as SQL strings. |
| `keyring` | The macOS Keychain, via Security.framework. Chosen over `tauri-plugin-stronghold`, which is an encrypted vault needing its own password on every launch — not the Keychain. |
| CoreGraphics (linked directly) | `CGEventSourceSecondsSinceLastEventType` for idle detection. Needs no entitlement and no Accessibility permission, unlike an event tap. |
| `fsrs` | The reference FSRS-6 implementation. Chosen over `rs-fsrs`, whose only release predates FSRS-6. |
| `reqwest` (`rustls-tls`) | HTTPS for the three network features. Pure Rust TLS, so the build never links system OpenSSL. |
| `chrono-tz` | The IANA timezone database, for reading a calendar's `TZID` and getting daylight saving right in a southern-hemisphere zone. |
| `iana-time-zone` | Asks macOS which timezone this Mac is in, so floating and all-day calendar events land on the right day. |

---

## Known limitations

- **Notifications only fire while Retain is running.** Closing the window is fine — it hides to the
  menu bar and keeps working. After ⌘Q, nothing fires until you reopen it. There's no background
  daemon, by choice.
- **The topic tree starts empty.** You paste your own study-design outline; see Biology 3/4 above
  for why nothing is shipped pre-filled.
- **Re-importing an outline replaces the tree.** Cards and error entries survive but lose their
  topic link. The screen says so before you commit, and shows a preview of exactly what will be
  created.
- **The calendar looks about a year ahead.** Recurring events are expanded to a 400-day horizon, so
  a timetable that repeats indefinitely doesn't fill the database.
- **Sub-daily recurrence isn't expanded.** `FREQ=HOURLY` and finer are kept as a single event; they
  don't appear in a school timetable and expanding them would produce tens of thousands of rows.
- **AI model names go stale, and one already did.** Google retired `gemini-2.0-flash` and removed it
  from the API entirely, which broke every AI feature with an unhelpful "change it in Settings".
  The Gemini default is now the maintained alias `gemini-flash-latest`, Settings shows the model
  actually in use, **Find available models** asks your key what it can see, and **Test this model**
  performs a real generation. Being listed does not guarantee a model runs — measured, not assumed —
  so Test is the only reliable check.
- **Keychain re-prompts after updates.** Because the app is ad-hoc signed, its signature changes
  with each build, and Keychain permission is tied to that signature. Once per release, not per
  launch.
