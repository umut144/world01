# World 01 — The `testbed` Game

Last updated: 2026-09-18

## What it is

`testbed` is a SceneMaker Game whose Scenes exist only so the sandbox core can
be tested against authored data. It is not a game anyone plays, it has no
design files, no rules crate and no binary. It is a fixture that happens to be
authored in SceneMaker rather than written by hand, because the things the core
has to get right - navigation over real terrain, separation out of a real
collider, a river switched off taking the bodies hanging under it - are not
things a synthetic four-by-four map can show.

Its exports sync like any other Game's, into `assets/maps/testbed/`, and they
follow whatever export contract version is current. That is the whole reason it
is a Game and not a frozen copy checked into this repository: a frozen copy
would have to be migrated by hand on every contract bump, and the one thing
guaranteed about the export contract is that it changes.

## Who owns what

**World 01 states the need.** A test in a shared crate that wants a world with
some property - a second river, a steeper route, a Prop with a hole in its
footprint - is a request recorded in this document. The core does not author
Scenes and does not edit them.

**SceneMaker fulfils it.** New Scenes are added under the `testbed` Game and
exported. Nothing else about the Game is decided here.

This split is why the core may depend on `testbed` without depending on a game.
`sandbox` is a real game with real design intent; if the core tested against
`overworld01`, then moving a tree while designing the sandbox would break
`world01-simulation`, which is backwards. Nobody designs anything in `testbed`.

## The standing promise

Core tests address authored objects by their `instance_id`. Those IDs are the
contract; a Scene may gain anything, but what is listed here may not be removed
or renamed without the tests that read it being changed in the same commit.

`core01` currently promises:

| Name | What a test needs it for |
|---|---|
| `template_anchor_001`, `template_anchor_002` | Template projection, Placement Rank resolution, occupancy generations |
| `tree_0001`, `tree_0003`, `tree_0004` | A real collider to be separated out of, and real Props for a Template to displace |
| `ankh_0001`, `ankh_0002` | Respawn and revival candidate selection |
| `river_0001`, `river_0002`, `river_0003` | Water bodies, one hanging under another, and a switchable river |
| `route_0001`, `route_0004` | Route sampling, authored segment grades, excavation cut cells |

Beyond the names, the Scene has to keep carrying at least: terrain with
elevation changes, two switches, four water bodies, four route surfaces with
both additive and subtractive segments, two bridges, and two Template Anchors.
Those counts are what the current tests happen to exercise, not a ceiling.

`core01` began as a copy of the sandbox's `overworld01`, which is why it
carries exactly that inventory - the tests were written against that world
before `testbed` existed.

## Requesting a new Scene

Add a row to the table below, describing the property a test needs rather than
the geometry that would provide it. SceneMaker decides how to author it.

| Scene | Needed for | Status |
|---|---|---|
| `core01` | Everything above | Exists |

Keep new Scenes small and single-purpose. `core01` is large because it was
inherited; a Scene that exists to test one thing should carry one thing, so
that a failure names its own cause.

## Rules

A Scene under `testbed` is never "improved". It is extended when a test needs
something it cannot express, and otherwise left alone. A change that makes an
existing test fail is a change to the wrong Scene - author a new one instead.

`testbed` maps are embedded into every binary today, like every other Game's.
That is wasteful for a shipped server and will stop being true once each game
gets its own binary embedding only its own maps; it is not worth solving before
then.
