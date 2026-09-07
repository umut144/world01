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
serves one queue, so several sessions can share it, and it runs
`./scripts/check.sh` on the host with the normal toolchain and warm `target/`.
The state lives in `.agent-check/`:

    requests/<id>      queued request, oldest served first
    pending/<session>  the id this session waits for
    results/<id>       header with `id`, `exit`, `args`, `duration`, `lines`,
                       then the output
    logs/<id>.log      raw output of that run, also useful for `grep`

Every result also carries the `protocol` of the watcher that wrote it. If the
runner reports WATCHER-VERALTET, the watcher process is older than its script:
the run itself is still valid, but say so and ask the developer for `checkw stop
&& checkw start` before relying on anything a newer script added to the header.
RUNNER-VERALTET is the other direction — this repo's copy of the scripts is
behind and has to be pulled over again.

`result` and `last.log` at the top level hold whichever run finished last, no
matter who asked for it. They are the developer's view and what `checkw status`
reads; this session does not read them, because the run they describe is often
not its own.

Sessions are told apart by `CHECK_AGENT_SESSION`, which `--poll` needs to find
its own request again. Export one stable value for the whole session; without it
the identity is derived from the physical path of the worktree, which is stable
per sandbox but cannot separate two sessions working from the same path.

The developer's own manual runs (`check` / `checkt`) write `manual-result` (same
header, with `id=manual-<epoch>`) and `manual.log`. Before requesting a run,
`manual-result` is worth a look: the developer may just have run the same thing
by hand. It is absent until the first manual run in a given clone, and its
header does not say which state of the tree it ran against — when in doubt,
request a fresh run. `manual.log` is captured through a command substitution,
which strips trailing blank lines, so for one and the same run it can differ
from the watcher's log by exactly those; compare runs through the header
fields, never by comparing logs byte for byte.

Runner exit codes:

- `0` — the wrapper succeeded.
- `1` — the wrapper failed; its own exit code is in the `exit=` header line.
- `2` — the call itself is wrong: `--poll` without a request of this session,
  or an argument outside `CHECK_AGENT_ARGS`. Fix the call; do not repeat it
  unchanged.
- `3` — no watcher is running. This always needs the developer: state plainly
  that the watcher is off, ask them to type `checkw start` in a terminal tab
  (foreground, so status line and sound stay there), and wait for their
  confirmation instead of falling back to asking them to run the wrapper by
  hand.
- `4` — this session's run is still queued or running; call again with `--poll`
  rather than reporting a result.
- `5` — the request is gone and no result was written: the watcher was restarted
  while it was queued, or it sat unread for over an hour. Request again; never
  keep polling.

Beyond exit code 3, stop and ask the developer whenever the obstacle is not
yours to remove: the same request fails or is dropped three times in a row, a
run keeps polling far past its usual duration, or the failure names something
about the machine rather than the code — a missing toolchain, a full disk, a
binary that is gone. Say what you tried, what you saw, and what you need.
Changing code in response to a broken environment is worse than waiting.

The usual validation rules of `AGENTS.md` are unchanged by this: the watcher is
only the transport. Silent success stays silent, failures are diagnosed from the
existing output first, and the default path stays the routine one.

## Git from the sandbox: no optional locks

The sandbox cannot delete files, so a plain `git status` in this session leaves
a stale `.git/index.lock` behind that blocks the developer's next Git command.
Always use `git --no-optional-locks ...` for read-only Git commands from here
(`status`, `diff --stat`, `grep`); it refreshes nothing and leaves no lock.

Never move or delete anything below `.git/` by hand; the `_to_delete/` rule
below does not reach in there. A lock that a running Git still holds belongs to
it, and taking it away destroys its commit. If a Git command fails on a lock,
run `./scripts/git-unlock.sh`; if it refuses, say so and ask.

Run `GIT_UNLOCK_AGE=5 ./scripts/git-unlock.sh` after each of your own commits,
not only after a Git command has already failed on a lock: every commit leaves
locks and temp objects behind that this session cannot delete, and they block
whoever commits next. The script's own default is 15 seconds; 5 is enough at
this call site because `lsof` is the real check and the age is only its
fallback, and a loosened margin belongs where it can be read rather than in the
default. If the script refuses because the leftovers are still too young, wait
a moment and run it again; if it still refuses, say so and ask.

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

_Nothing open. Earlier entries — manual runs published to `.agent-check/`, a
`checkw` helper to control the watcher, and the request queue that lets several
sessions share one watcher — were all built on 2026-09-07._

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
