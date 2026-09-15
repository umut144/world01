# World 01 — MOBA: build order and authoring gates

Last updated: 2026-09-13

## Purpose and authority

This document records in which order the MOBA is built and what has to be
authored before each part of it can exist. It is a roadmap, not a
specification: what the game *is* lives in
[`GAME_MOBA_DESIGN.md`](GAME_MOBA_DESIGN.md), the shared mechanics it builds on
live in [`WORLD_DESIGN.md`](../../WORLD_DESIGN.md), and the technical contracts
live in [`SANDBOX_TECHNICAL.md`](../../SANDBOX_TECHNICAL.md).

[`TASKS.md`](../../../TASKS.md) carries only the phase that is current. The
phases beyond it are described here so that nobody has to reconstruct the
intent from a tracker row.

## What a gate is

A **gate** is work only the developer can do: authoring in PolyTools or
SceneMaker, a design decision, or a playtest. The code phase behind a gate does
not start until the gate is met, because starting it would mean inventing the
data the gate delivers, and invented content is the thing the World-01
no-fallbacks principle exists to prevent.

Each gate below states exactly what is needed and how the runtime will know it
arrived. A gate that turns out to be wrong is a good outcome discovered early;
a gate that is worked around silently is a phase built against a guess.

## Phases at a glance

| Phase | What arrives | Gates | Returns to `main` |
|---|---|---|---|
| 0 | The branch and these documents | — | documents only, on merge |
| 1 | Teams and a second attack button | — | all of it |
| 2 | The map, the Totems, and a match that ends | A, B, C — **met** | nothing |
| 3 | Mana, timed respawn, and what the support Totems do | — | nothing |
| 4 | Bots, first iteration — the first playable 5v5 | D | the `ai` crate |
| 5 | Ten abilities across five Characters | E, F | the two ability shapes |
| 6 | Creeps in three lanes | G, H | nothing |
| 7 | Bot behaviour, second iteration | playtesting | improvements to `ai` |

---

## Phase 0 — the branch and the documents — **done, 2026-09-13**

`game/moba` taken from `main` at `8fa55f7`. This document and
[`GAME_MOBA_DESIGN.md`](GAME_MOBA_DESIGN.md) written, and both named in
[`AGENTS.md`](../../../AGENTS.md) with their reading rules.

Four decisions were settled before anything was written, and they are recorded
in the design document with their reasoning: allegiance is a shared `TeamId` in
`world_data`; genre tuning is an overlay file over `world01.toml`; two
abilities means two attack inputs rather than ability slots; the branch and
documents are named `moba`.

## Phase 1 — teams and the second attack

Nothing about the MOBA yet. Two small sandbox-generic changes that everything
after them needs, built first because they are the only parts of the game that
belong to `main` from the first line.

What arrives:

- `TeamId` in `world_data`: a protocol-neutral component carrying identity and
  nothing else, registered with `replicate_once()` beside `ActorId`. No
  simulation system reads it in this phase. It is data looking for a consumer,
  and the consumer is Phase 2.
- `attack_secondary` in `PlayerInput`, beside the existing `attack`: the intent
  component, the protocol registration, the tick-input application, and a
  keyboard and controller binding in the client. It drives nothing yet, because
  no Character has a second ability yet.

What it deliberately leaves for Phase 5 is the binding that says *which* input
an ability listens to. Both existing abilities read `AttackIntent`, which is
unambiguous only while no Actor carries two ability states at once; the moment
one does, a single press would start both. That is a real addition to
`CharacterAbilityCatalog`, and it belongs with the abilities that need it
rather than here, where it would have exactly one legal value.

What it does not do: assign anyone to a team, or make the new button mean
anything. Both are deliberate. This phase's value is that the wire format and
the input shape change once, early, while there is nothing built on top of them
to break.

Validation: `./scripts/check-agent-run.sh --tests`. Protocol and input changes
touch `network` and `world_data`, so the test path is the right one.

Returns to `main`: all of it, as one or two commits that name no MOBA concept.

## Phase 2 — the map, the Totems, and a match that ends

The first phase with a game in it. At its end you can walk up to an enemy Totem
of Life, hit it until it falls, and the match is over.

### Gate A — the Totem Assets (PolyTools) — **met, 2026-09-14**

Three Assets, not six: `totem_of_life`, `totem_of_mana`, `totem_of_time`. A map
places each twice, once per side, so six Totems stand on the map and the Assets
themselves carry no team. This gate originally asked for six Assets so that team
colour would be an art decision; that was overtaken by the authoring and the
question moved to the design document's open list, where it waits for a match to
show whether the two banks read apart.

Each carries what it fails to load without, and all three do:

- an authored **`collision` Region**, because a Totem is walked around;
- a **mass classification** for every material-bearing Component.

**Correction, 2026-09-15**: this gate originally also asked for an authored
`hurt` Region, on the assumption a Totem could reuse the Character hurt
pipeline. It cannot: `CharacterHurtGeometryCatalog::from_content` only ever
iterates `content.characters()`, so a `hurt` Region on a Prop is invisible to
every runtime system that reads it, not merely unused. A Totem needs its own
role instead - a `destructible` Region, read the way `collision` already is,
by `WorldCollisionGeometryCatalog::from_content_and_map`'s translation-only
pipeline rather than the Character pipeline's facing-and-rotation one. The
three Totem Assets still need this Region re-authored; until then a Totem
cannot be hit, the same graceful degradation `collision`'s absence already has.

A Totem's health is **not** derived from its authored area. It is MOBA design
data, so a Totem needs no entry in `crates/design/hp.json` and its size in
PolyTools is free.

Each also needs a **placement rank**, because a missing rank is a load error for
any placed Asset. A Totem outranks everything currently in the table - grass,
cobblestone, tree, Ankh - since no Template may ever paint over an objective.
The entries belong in the MOBA overlay rather than in `world01.toml`, which
means the overlay has to be able to *add* placement ranks and not only override
scalar values. **Still owed**: the three ranks are not written yet.

### Gate B — the MOBA map Instance (SceneMaker) — **met, 2026-09-14**

Authored as `map01`: `135 x 100` Terrain cells at `1 m`, one water body, five
bridges, three cobblestone lanes through grass, four Ankhs, six Totems and fifty
Trees. The lanes turned out to be authored Terrain rather than gaps in tree
cover, which is the better answer and is now what the design document says.

One Instance scene containing:

- the **river**, as a water body, with its channel authored deeper than
  `max_wade_depth_meters` (currently `0.4 m` for every Character) so that it
  cannot be waded;
- **five bridges** across it;
- **three lane corridors** left open through tree cover, running West to East;
- **Tree Props** over most of the remaining area, which is what makes the lanes
  lanes;
- **four Ankhs**, two per bank;
- **six Totems**, three per bank, using the Assets from Gate A.

The map does not need to be final, balanced or pretty. It needs to be
structurally complete, because every rule in this phase reads it.

Note for authoring: the existing export contract rejects a Template carrying
water. The MOBA map is an Instance, not a Template, so this does not apply to
it — but it does mean the river cannot arrive inside a Template Anchor. That is
a constraint on the map, not a task for this phase.

### Gate C — sync and the team lists — **met, 2026-09-15**

Two gates inside the gate had to open first, and both are `main`'s rather than
the MOBA's. The content boundary still demanded PolyTools Manifest schema 21
while the sync had already written 22, so the repository was red before any MOBA
work. And the map sync read one hardcoded SceneMaker Game, while the workspace
now holds `sandbox` and `moba`; since the sync replaces the whole destination
directory, the MOBA Scenes could only have arrived by dropping the sandbox maps
the embedded catalog and the respawn tests stand on.

`crates/design/games/moba/maps/map01.json` now carries the team lists, keyed by
`instance_id` - one file per map rather than the single `moba.toml` overlay
this gate originally named, since ownership is per-map data with nothing in
`world01.toml` to override.



- Run `./scripts/sync_polytools_characters.sh` and
  `./scripts/sync_scenemaker_world.sh`, which validate and embed the new
  content.
- Hand over the **authored Prop IDs** of the six Totems and four Ankhs from the
  export, so the team lists in `design/games/moba.toml` can be written.

A Prop ID the list forgets, or a listed ID the map does not contain, is a load
error. This is the gate where the map stops being a picture and becomes data
the simulation is allowed to depend on.

### What is built behind those gates

- Done: `crates/design/games/moba/maps/map01.json` for Totem/Ankh ownership by
  team, and `crates/design/games/moba/totems.json` for each Totem kind's
  MaxHP - both plain additive design files, not an overlay over
  `world01.toml`, because neither has anything in the sandbox file to
  override. The unified overlay-merge machinery this bullet originally
  implied stays unbuilt until a MOBA value actually needs to change one of
  `world01.toml`'s own numbers.
- Done: `TotemLayout` derived from the composed world in the same fixed-tick
  world transaction that already builds `AnkhLayout`, so a Totem survives a
  recomposition the way an Ankh does, and fails loudly on an unowned Totem, a
  dangling ownership entry, or a kind with no health entry.
- Done: team assignment on join, and team spawn at the joining team's own
  Totem of Life, replacing the five fixed spawn positions the server used
  before. This is first join only; respawn after death is still Phase 3's
  concern and still spawns at the team's Ankh, unchanged - the two are
  different moments and were deliberately decided separately.
- Not yet: Totem entities with health and destructible geometry, taking
  damage through the existing damage resolution. No new damage rule: a Totem
  will be hit by the same `HammerStrike` and `MageEyeBeams` evaluation
  everything else is - blocked on the `destructible` Region from Gate A's
  correction above being authored and synced.
- Not yet: the win condition, a destroyed Totem of Life ending the match,
  stated to the game console.

Validation: `--tests`, with focused tests for the overlay resolution, the Totem
layout derivation, and the win condition.

Returns to `main`: nothing. Every line of this phase names a MOBA concept.

## Phase 3 — mana, timed respawn, and what the support Totems do

The two support Totems stop being scenery.

- A team mana pool with a maximum, regenerated in proportion to the Totem of
  Mana's current HP.
- Mana costs on `HammerStrike` and `MageEyeBeams`, in the overlay file only. An
  ability whose cost cannot be paid does not start, is not queued, and does not
  partially charge.
- A respawn delay derived from the Totem of Time's current HP, read at the
  moment the respawn resolves rather than at the moment of death.
- Respawn at the dead Character's own team Ankhs, at the MOBA's respawn health.
- Revival by a teammate unchanged, and skipping the timer entirely.

At the end of this phase the loop is complete for a single player: you fight,
you die, you wait, you come back, and how long you waited depended on something
you could have defended.

Validation: `--tests`. The respawn and life systems are covered by existing
tests that this phase changes the behaviour of.

Returns to `main`: nothing.

## Phase 4 — bots, first iteration

The phase the whole slice exists for. At its end: one human and four bots
against five bots, on the authored map, with a match that ends.

This is tracker task `SBX-25`, and `SBX-26` beside it.

What arrives:

- **`crates/ai`**: a behaviour tree with utility-scored selection, data-driven,
  free of any MOBA concept. Node kinds are `Sequence`, `Selector`, `Condition`,
  `Action` and `UtilitySelector`, and nothing else until something cannot be
  expressed with them.
- A **`SimulationSet::Decision`** phase before the gameplay step, in which a
  tree runs and **writes only intent components**. A bot has no path to health,
  position, or any authoritative outcome; it plays through the same opening a
  player does.
- **Server-only bots**, replicated as authoritative and interpolated rather
  than predicted, so `SimulationAuthority::Predicted` skips them. Neither the
  tree nor the pathfinding has to be deterministic on the client, and a bot's
  knowledge never reaches a player's machine.
- A **decision cadence** of roughly ten per second, staggered across bots, with
  intents held in between.
- The **first behaviour**: confirm death when dead; retreat to an own Ankh below
  a health threshold; attack an enemy in reach; otherwise advance along the
  assigned lane toward the enemy Totem of Life. Target selection is a utility
  score over distance, target health, and whether a teammate is already on it.
- Nine bots spawned and assigned to teams, with the human filling the tenth
  place.

Two things will be missing from the sandbox and are built here because the bots
are the first consumer, and both are generic:

- resolving the ground navigation node nearest a world position, which the
  graph does not expose today;
- the shared status action mask and incoming-damage modifiers as queries on
  `ActorCondition`, which is tracker task `SBX-32` — a bot and a player's
  controller must decide what is available from one rule, or they will drift.

The bots fight with `HammerStrike` and `MageEyeBeams`, because those are the
abilities that exist. A first bot match of Hammerers and Mages is not a
compromise; it is the vertical slice working before it is widened. Phase 5
widens it.

### Gate D — play it

Play a full match and say what is wrong. This is the hinge of the whole
roadmap: the design document deliberately specifies a small, probably-wrong
first behaviour, and the second iteration is written from what this gate
returns, not from what seems sensible in advance.

Validation: `--tests`, with deterministic tests for tree evaluation and utility
scoring over fixed inputs.

Returns to `main`: the `ai` crate, the `Decision` phase, the navigation query
and the `ActorCondition` queries — as commits that name no MOBA concept. The
bot spawning, the team assignment and the MOBA's own tree data stay here.

## Phase 5 — ten abilities

### Gate E — decide the eight

For Rogue, ArcherF and Monk, two abilities each; for Hammerer and Mage, one
more each. For every one: what it does, which weapon it uses, and roughly what
it costs and how far it reaches. Exact numbers are tuning and can follow.

The design document's [two shapes](GAME_MOBA_DESIGN.md#two-shapes-not-eight-implementations)
are the vocabulary to decide in: a melee swing or a travelling projectile.
An ability that is neither is allowed and becomes its own implementation, but
it should be chosen knowing that it costs more than the others.

The weapon Assets already synchronized are `arrow`, `bomb`, `bow`, `dagger`,
`fist`, `hammer`, `needle`, `stone`, `throwing_knife` and `throwing_star`. What
they can currently do is only what has been authored on them.

### Gate F — author what the decisions need

Any weapon Asset an ability needs and that does not yet carry its authored
`attack` Regions and attachment frames — a grip aligned to the Character's
`weapon_socket_primary`, and whatever the swing or the emission needs. Plus the
sync run that embeds them.

### What is built behind those gates

- The melee-swing and projectile shapes extracted from `HammerStrike` and
  `MageEyeBeams`, so the behaviour is shared and the difference between two
  abilities is design data.
- The eight abilities as data over those shapes, registered through the
  existing `Ability::from_name_key` arm, which is already the one place a
  design name becomes executable behaviour.
- The input binding on `Ability`: which of the two attack intents an ability
  listens to, attached by `CharacterAbilityCatalog` beside the state it already
  attaches. This is what stops one press from starting both of a Character's
  abilities, and it is the reason Phase 1 could add the input field without it.
- Bots taught that they have two abilities and a crude rule for choosing —
  reach and cost. Choosing well is Phase 7.

Validation: `--tests`. The extraction changes behaviour the Hammer and Mage
tests already cover, which is exactly the safety net an extraction wants.

Returns to `main`: the two ability shapes. The eight abilities are World 01
design data rather than MOBA-owned — a Rogue's dagger belongs to the Rogue in
every game — so their definitions go to `main` too, while their MOBA damage and
mana values stay in the overlay.

## Phase 6 — creeps

### Gate G — a creep body

One PolyTools Asset for a creep, with a `hurt` Region, a `collision` Region and
mass classification, like anything else that stands in the world.

This gate accepts a placeholder. An existing authored Character used as a creep
body is enough to build and tune the entire phase, and the real Asset can
replace it later without touching a line of the lane logic. Taking the
placeholder is the recommended path, because lane pushing is a rhythm that
needs tuning and tuning wants to start early.

### Gate H — the lane waypoints

Three lanes, one ordered waypoint list per lane per team, ending at the enemy
Totem of Life. Coordinates read off the authored map. This gate can be met any
time after Gate B.

`map01` makes this easier than expected: the lanes are authored as cobblestone
Terrain, so the waypoints can be read off the cobblestone rather than guessed
from where trees are not. Whether they are still written down by hand or derived
from the three cobblestone runs is an implementation question for Phase 6 -
derived would be pleasant, but three runs have to be told apart from one
another, and that is the part authoring answers for free.

Five bridges and three lanes do not agree, which is why the waypoints exist: 
without them every creep on the map takes the cheapest crossing and the other
four bridges stand empty.

### What is built behind those gates

- Creep Actors: a `TeamId`, health, one attack, no `PlayerOwner`, no character
  selection, no stamina and no mana.
- Wave spawning on a cadence from behind each team's Totems.
- A three-node behaviour tree per creep, running in the same `Decision` phase
  as the bots. A creep is not a second AI system.
- Lane following: path to the next waypoint through the shared navigation
  graph, advance on arrival, fight what is in the way.

Validation: `--tests`.

Returns to `main`: nothing, unless wave spawning turns out to want a generic
shape, which it should not be built to have in advance.

## Phase 7 — bot behaviour, second iteration

Written from Gate D and from every match played since. The design document
lists the behaviours already thought of and deliberately not built: focused fire
on a called target, group retreats, ability combinations across Characters,
holding a bridge, rotating to a lane under pressure, baiting with the DASH
invulnerability window, denying a revival, and using the two off-lane bridges as
a flank.

None of them is committed to. The list exists so that a behaviour that turns
out to matter is recognized rather than rediscovered, and so that a behaviour
that sounds clever but never came up in a match can be left alone.

Returns to `main`: whatever of it is a capability of the tree rather than a
MOBA tactic. A new node kind returns; "hold the bridge" does not.

---

## Later, and not scoped

Recorded so they are not mistaken for oversights: items and a shop, gold, fog
of war and vision, wards, a jungle and neutral camps, towers that are not
Totems, more than two abilities per Character, ranked matchmaking, a match-end
screen, ten human players, and a second map.

## Open roadmap questions

- **Whether Phase 5 should precede Phase 4.** It is ordered the other way on
  purpose: a bot match fought by Hammerers and Mages proves the `ai` crate
  before eight new abilities are laid on top of it. If the first bot match is
  dull enough to make judging behaviour impossible, swapping the two phases
  costs nothing.
- **Whether the Totem Assets stay two sets of three.** They exist as six
  Assets because team colour should be an art decision. If the two sets turn
  out to differ only by colour, an authored team marking on the Prop is the
  better answer and it is a SceneMaker gate.
- **When Prop team ownership moves out of design data.** Decided by how often
  the map moves, not by preference.
- **Whether the client needs its presentation split before Phase 2.** Tracker
  task `SBX-27` is already planned on `main`; the MOBA adds Totems, team
  colours and a match outcome to a file that is already the largest in the
  client.
