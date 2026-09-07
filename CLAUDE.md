# Claude project instructions

## Important first notice: limited internet data allowance

The developer is currently working with a limited internet data allowance.
Use external network access only when it is strictly necessary for the current
task and local repository files, installed dependencies, caches, and existing
documentation are insufficient. Keep the number of requests, transferred data,
and downloads to the absolute minimum. Ask for permission before any external
access that is not clearly essential.

## Project instructions

Before planning, discussing, or changing this project, read `AGENTS.md` in
full and follow all instructions and document-reading requirements defined
there. Treat `AGENTS.md` as the canonical source for project workflow, scope,
architecture, validation, and Git rules.

## Validation from this session: always use the check watcher

This session's shell runs in the mounted-folder sandbox and has no Cargo, so it
cannot execute `./scripts/check.sh` itself. It must never interrupt the working
loop with "please run `./scripts/check.sh --tests`". From now on, every routine
validation goes through the watcher:

    ./scripts/check-agent-run.sh              # wrapper default path
    ./scripts/check-agent-run.sh --tests      # additionally the library tests
    ./scripts/check-agent-run.sh --poll       # keep waiting on a long run
    ./scripts/check-agent-run.sh --full       # print the complete output

The developer keeps the watcher running in a terminal tab (`checkw start`). It
executes `./scripts/check.sh` on the host with the normal toolchain and warm
`target/`, and reports back through `.agent-check/`: `result` (header with `id`,
`exit`, `args`, `duration`, `lines`, then the output) and `last.log` (the raw
output, also useful for `grep`).

The developer's own manual runs (`check` / `checkt`) land there too. They always
write `manual-result` (same header, with `id=manual-<epoch>`) and `manual.log`.
They are mirrored into `result` and `last.log` only when no request of this
session is outstanding — that is, when `pending-id` matches the `id` in
`result`; while a request is pending, both stay untouched, so a manual run is
never mistaken for the requested one. Before requesting a run, `manual-result`
is worth a look: the developer may just have run the same thing by hand. Do not
rely on the file existing, and note that its header does not say which state of
the tree it ran against — when in doubt, request a fresh run.

Runner exit codes:

- `0` — the wrapper succeeded.
- `1` — the wrapper failed; its own exit code is in the `exit=` header line.
- `3` — no watcher is running. This is the only case in which the developer is
  asked: state plainly that the watcher is off, ask them to start it with
  `checkw start` (foreground, so status line and sound stay in the tab), and
  wait for confirmation instead of falling back to asking them to run the
  wrapper by hand.
- `4` — the run is still going; call again with `--poll` rather than reporting
  a result.

The usual validation rules of `AGENTS.md` are unchanged by this: the watcher is
only the transport. Silent success stays silent, failures are diagnosed from the
existing output first, and the default path stays the routine one.

## Git from the sandbox: no optional locks

The sandbox cannot delete files, so a plain `git status` in this session leaves
a stale `.git/index.lock` behind that blocks the developer's next Git command.
Always use `git --no-optional-locks ...` for read-only Git commands from here
(`status`, `diff --stat`, `grep`); it refreshes nothing and leaves no lock.

## Tooling wishes go to the developer

The watcher and its runner are a developer convenience, not project scope. If
this session finds that the loop would run better with an extension or another
convenience — a shell function or alias, an auto-start for the watcher, a new
field in the result header, a second command it would rather call than
improvise — it does not build that itself in passing and does not quietly work
around it. It says so in the conversation, concretely enough to act on (what,
why, and the exact contract it expects), and appends the idea to the wishlist
below. The developer takes care of the shell and dotfiles side.

### Wishlist

_Nothing open. The two earlier entries — manual runs published to
`.agent-check/`, and a `checkw` helper to control the watcher — were built on
the shell side on 2026-09-07._

## Deleting files from the sandbox

This session cannot delete files. When a change requires removing a file, move
it into `_to_delete/` at the repository root instead. Do this silently: do not
ask first, and do not report it afterwards. The developer empties that folder
when the day's work is done.

Commit the change as usual, but never stage `_to_delete/` itself. It stays
untracked on purpose, so that the folder appears in the developer's `git
status` at the end of the day while nothing of it reaches the history.

Never read, search, or reference anything below `_to_delete/`. Its contents are
discarded work; treat the folder as if it were empty.
