# Putting the database in iCloud Drive — analysis before implementation

**Status:** analysis complete. **Decision: the live-database-in-iCloud toggle is NOT shipped.**
A safe subset ships in Checkpoint 1 instead. See "What ships" at the bottom.

This document exists because the brief asked for "an optional setting to place the DB file in
iCloud Drive for free cross-Mac sync", and the follow-up constraint required that the failure
modes be documented *before* any code was written. Having worked through them, the honest
answer is that the naive version of this feature corrupts data, so it should not be built as
described.

---

## Why the obvious implementation is unsafe

### 1. A SQLite database in WAL mode is three files, and iCloud syncs files independently

In WAL mode SQLite maintains:

| File | Role |
| --- | --- |
| `retain.db` | the main database |
| `retain.db-wal` | the write-ahead log — **contains committed transactions not yet in the main file** |
| `retain.db-shm` | shared memory index coordinating readers/writers **on one machine** |

iCloud Drive has no concept of "sync these three files as one atomic unit". It uploads each
file when it notices it changed. So the other Mac can easily receive a `retain.db` from 14:03
alongside a `retain.db-wal` from 14:01, or a main file with no WAL at all. Committed
transactions living only in the WAL simply vanish, and a WAL whose header doesn't match the
database it lands next to is rejected or, worse, misapplied.

There is no ordering guarantee to lean on here. This alone is disqualifying.

### 2. `-shm` must never leave the machine that made it

The shared-memory file is a coordination structure for processes on a single host. A copy of
it from another Mac is not merely useless, it is actively misleading to WAL index recovery.
Any design that syncs the directory wholesale ships this file by accident.

### 3. SQLite's locking does not work across machines

SQLite's concurrency safety is built on POSIX advisory locks (`fcntl`). Those locks are local.
Two Macs with the app open against the same iCloud file have **no mutual exclusion whatsoever** —
each believes it holds the write lock. This is the same reason SQLite's own documentation warns
against putting databases on NFS and SMB shares, and iCloud Drive is weaker than either, because
it isn't even a filesystem protocol — it's an eventually-consistent file replicator.

Two machines writing concurrently is not an edge case for this app. The whole point of the
feature is using it on more than one Mac.

### 4. iCloud can evict file contents out from under an open database

With "Optimise Mac Storage" enabled — on by default when disk space is low — iCloud Drive
evicts file *contents* and leaves a dataless placeholder, materialising the bytes on demand
when something reads them. SQLite reads at arbitrary offsets and holds a file descriptor open
across a session. Hitting an evicted region mid-transaction produces a stall or an I/O error
at a point where SQLite expects neither. An I/O error during a commit is one of the few ways to
genuinely corrupt a SQLite file.

### 5. Conflicts are resolved at file granularity, and databases cannot be merged

If both Macs write while offline, iCloud produces a conflicted copy. There is no merge for two
divergent SQLite files — you pick one and discard the other. In practice the discarded one is
renamed to something the user never opens, so the loss is silent. "Free cross-Mac sync" that
silently drops a week of error-log entries is worse than no sync.

### 6. A mid-write upload is a corrupt snapshot

SQLite guarantees atomicity at the transaction level via the WAL, not at the file level. iCloud
may begin uploading `retain.db` at any instant, including halfway through a checkpoint. The
bytes that arrive on the other Mac are a torn snapshot that was never a valid database state.

---

## What would actually be safe, if this gets built later

Not a live database on the network volume — a **snapshot handoff**:

1. Keep the live database local, always.
2. Produce a snapshot with SQLite's `VACUUM INTO`, which writes a **single, transactionally
   consistent file with no WAL sidecar**. This is the key primitive: it is a valid database at
   a single point in time, and it is one file, so file-granular sync is no longer a lie.
3. Write that snapshot to iCloud Drive along with a small manifest (device name, schema version,
   snapshot timestamp, row counts).
4. On the other Mac, *never* open the snapshot directly. Detect that a newer one exists, show
   what it contains versus local state, and require an explicit user action to import it.
5. Treat simultaneous use as the conflict it is: if both devices have written since the last
   common snapshot, say so plainly and make the user choose, rather than picking silently.

That is a real feature with real design work in it — reconciliation UI, device identity,
divergence detection. It is not a checkbox, and pretending it is a checkbox is how people lose
data. It belongs in its own checkpoint with its own scope discussion, not smuggled into
Checkpoint 1.

---

## What ships in Checkpoint 1 instead

The parts that make the local database genuinely safe and recoverable, which the snapshot design
above depends on anyway:

- **Database stays at** `~/Library/Application Support/com.armankundu.retain/retain.db`. Local disk only.
- **WAL mode** with `synchronous = FULL`. Write volume here is trivial (a few rows per study
  session), so the durability cost is irrelevant and the crash-safety is worth having.
- **`foreign_keys = ON`**, enforced per connection.
- **Integrity check on startup** (`PRAGMA quick_check`). If it fails, the app refuses to write
  and offers the most recent snapshot rather than compounding the damage.
- **Automatic snapshots via `VACUUM INTO`** on launch, keeping the last 7, in
  `…/com.armankundu.retain/snapshots/`. This is the same primitive the iCloud design would use,
  so building it now is not throwaway work.
- **Complete JSON export/import.** The exporter enumerates tables from `sqlite_master` and dumps
  every row rather than naming tables by hand, so it stays complete automatically as Checkpoints
  2 and 3 add tables. Export carries the schema version; import validates it before touching
  anything.

The escape hatch the brief actually cares about — "I'm never trapped in my own app" — is fully
delivered by the JSON export. The iCloud toggle was a convenience on top of that, and it is the
convenience, not the escape hatch, that turns out to be unsafe.
