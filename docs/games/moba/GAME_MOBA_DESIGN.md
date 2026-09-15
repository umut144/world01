# World 01 — MOBA: game design

Last updated: 2026-09-13

## Purpose and authority

This document is the persistent source of truth for the MOBA's game-design
intent, scope boundaries, confirmed decisions, and open questions. It is not
the source for shared World 01 mechanics: those live in
[`WORLD_DESIGN.md`](../../WORLD_DESIGN.md), and every deviation from them is
written down here as an explicit override rather than assumed.

The build order and the authoring gates live beside this document in
[`MOBA_ROADMAP.md`](MOBA_ROADMAP.md). This document says what the game is;
the roadmap says in which order it arrives and what has to be authored before
each part of it can exist.

The game is developed on the branch `game/moba`, taken from `main` on
2026-09-13. `main` remains the shared sandbox base as
[`SANDBOX_VISION.md`](../../SANDBOX_VISION.md) describes it. What returns to
`main` and under which conditions is [its own section](#what-returns-to-main)
below.

## The match

Two teams of five fight on one map divided by a river. The target is one
playable session of:

- one human player and four bots on one team,
- five bots on the other,
- a server-authoritative simulation the human connects to as an ordinary
  client.

A team wins by destroying the opposing team's **Totem of Life**. There is no
second win condition, no timer, and no draw in the first iteration.

Ten Actors is a number the current transport does not carry as ten clients:
`MAX_CLIENTS` is `5`. It does not have to. Bots are server-driven Actors with
no `PlayerOwner`, so a nine-bot match needs exactly one connected client. The
client limit becomes a question only if the MOBA ever wants ten humans, and it
is a constant, not a design problem.

## Teams

The MOBA is the first World 01 game with sides. Nothing in the repository had
a team concept before it.

- Allegiance is carried by a protocol-neutral `TeamId` component in
  `world_data`, replicated like `ActorId`. It carries identity and nothing
  else.
- `TeamId` decides which Totems an Actor must protect, which Ankhs it respawns
  at, which creeps march for it, and what a bot's evaluation function treats as
  an enemy.
- `TeamId` grants **no damage immunity**. World 01 has no friendly-fire
  category, and this game does not introduce one. A Mage beam still stops at a
  teammate's body and still damages them; a HammerStrike still catches the
  Hammerer's own team inside its arc.

That last point is deliberate and it is unusual for the genre. It follows from
World Design rather than from a MOBA decision, and it is kept because the
geometry-based damage rule is what makes the Mage beam and the Hammer arc
readable in the first place: an attack that passes through some bodies and not
others cannot be read from the picture. If playtesting shows it dominating the
match rather than flavouring it, the override belongs here and nowhere else.
See [Open questions](#open-questions).

The two teams are **West** and **East**, named for the bank of the river they
hold. The names are positional rather than fictional; a World 01 name for each
side is a later decision and changes nothing mechanical.

## The map

The map is authored by the developer in SceneMaker and PolyTools. Nothing about
it is generated at runtime. What the runtime needs from it is listed here so
the authoring and the code can be built against the same description.

- A **river** runs across the map and divides it into a West half and an East
  half. It is a SceneMaker water body and it is impassable: its channel is
  authored deeper than a Character's `max_wade_depth_meters`, so the only way
  across is a bridge.
- **Five bridges** cross it. Five against three lanes is intentional: two of
  them are off-lane and exist to make a flank a real choice rather than a
  detour with no reward.
- **Three lanes** run West to East, each joining one of a team's Totems across
  one of the three wider bridges to a Totem on the far side. They are authored
  as **cobblestone Terrain** through the grass, so a lane is a thing the map
  declares and not merely the gap tree cover happens to leave. That makes a lane
  readable to the eye and to a bot from the same fact.
- **Four Ankhs**, two on each bank, one pair per team. They are the team's
  respawn points and they use the shared World-01 Ankh.
- **Six Totems**, three per team, described below.

The river divides the map, the bridges are where the map narrows, and the
Totems sit behind each team's own bank. Everything else about the layout is an
authoring decision and is not fixed here.

`map01`, authored 2026-09-14, is the first such map: `135 x 100` Terrain cells
at `1 m` each, one water body, five bridges, 4 860 cobblestone cells carrying
the three lanes through 8 640 of grass, four Ankhs, six Totems and fifty Trees.
Two numbers follow from its size and are worth knowing before tuning anything.
A Character at the World-01 Hammerer's `0.6 m/s` needs over three minutes to
cross `135 m`, which is most of why the MOBA overrides the movement
normalization at all. And fifty Trees over `13 500` cells is sparse cover, so
what separates the lanes today is mostly the open grass between them rather
than anything blocking sight or movement. That is deliberate for a first
iteration: the Trees are there to give navigation and bots something to walk
around while the map is tested. Denser cover, hills, ramps, and tunnels into a
cave beneath are later authoring, and none of them changes anything written
here.

### How the runtime learns what stands where

Two facts have to reach the simulation about a placed Prop: **what kind of
thing it is**, and **whose it is**.

The kind comes from the PolyTools Asset key. A Prop placed from `totem_of_life`
is a Totem of Life because that is the Asset that was placed; nothing infers it
from a name convention or a position. Three Totem Assets exist -
`totem_of_life`, `totem_of_mana`, `totem_of_time` - and a map places each twice,
once per side. The Assets carry no team.

Whose it is comes from the game's design data. One file per map, beside the
other per-thing design data and in the same shape: `schema_version` and a list,
as JSON.

```
crates/design/games/moba/maps/map01.json
```

It lists only the Props that belong to a side, by SceneMaker **`instance_id`**,
with a team number. The numbers are `0` and `1` and mean nothing on their own:
the game maps them onto West and East. A design file should not have to know
what a side is called.

The file says nothing else. Not the Asset, because the map already says it; not
the position, because the map already says that too. A coordinate copied into
design data is a second source of truth that goes stale the first time a Prop
moves, and a stale coordinate is exactly the kind of forgotten data the
World-01 no-fallbacks principle exists to keep out of behaviour. Which Prop a
line refers to is looked up in SceneMaker, where the author is already standing
when they decide a side.

### What this asks of an instance_id

The whole arrangement rests on one property, and it is worth stating because it
is a promise SceneMaker has to make rather than something world01 can check:

**An `instance_id` is never reused.** Once a number has named a Prop, no later
Prop in that Scene may carry it again, even after the first is deleted.

SceneMaker guarantees this from **export 20**, allocating from a per-Asset
counter stored on the Scene document that is never decremented, and showing a
selected Prop's `instance_id` in the inspector so it can be read and pasted
straight into the file below. The repository's own exports are still at 19 and
the importer still requires 19; raising it is one change that waits for the
re-export.

Uniqueness among living Props is not enough. A reference that lives in another
repository outlives the Prop it names, and the failure that matters is not a
dangling reference - that one is loud, and loading refuses it - but a reference
that silently resolves to a *different* Prop than the one the author meant. A
Totem of Life that quietly became the other team's is not a crash; it is a game
that plays wrong.

Non-reuse needs no tombstones and no long identifiers. It needs an allocation
counter that is stored with the Scene and never counts down - not a scan for the
lowest free number, and never an index into an array. The width of the number is
the least important part of it: a four-digit counter that never repeats is safer
than a ten-digit one that does.

When a reference does dangle, the error names every `instance_id` the map
actually contains, so the author can see what became of it without opening the
editor. That is the diagnostic a deletion record would have provided, at no
standing cost.

Loading fails in both directions: an entry naming an instance the map does not
contain, and a Totem or Ankh in the map that no entry names. A forgotten Totem
would otherwise be a neutral objective no one could win by; a mistyped one would
be an objective that silently is not there.

Two alternatives were rejected. Deriving the side from which half of the map a
Prop stands on was rejected on 2026-09-13: a Totem placed on the wrong bank
would silently become the enemy's rather than failing to load, and the river is
a curve rather than an axis. Authoring the team in SceneMaker was rejected on
2026-09-15 by the developer's preference, and it is the better call for a
reason worth recording: a side is a rule of one game, and SceneMaker authors
World 01 rather than the MOBA. A Totem marked for a team in the editor would
mean the editor knew about teams.

The cost is that moving a Totem across the river means editing one line. That
is the right cost, because changing which side an objective belongs to is a
design decision and should take a deliberate edit.

## The Totems

A Totem is a stationary Building-Prop with health. Each team has three, and
each of the three does something different while it stands.

They are Props rather than Characters. They do not move, they have no
traversal profile, no stamina, no gaze and no abilities. What they share with a
Character is health and hurt geometry: a Totem is destroyed by being hit, using
the same geometry-based damage rule everything else in World 01 uses.

| Totem | While it stands | When it falls |
|---|---|---|
| Totem of Life | Nothing. It is the objective. | Its team loses the match. |
| Totem of Mana | Its team regenerates mana in proportion to the Totem's current HP. | The team regenerates no mana. |
| Totem of Time | Its team's dead members return faster, in proportion to the Totem's current HP. | The team's dead members wait the full respawn time. |

Two consequences follow from proportionality and both are the point of the
design:

- A Totem of Mana at half health halves its team's mana income. It does not
  have to fall to matter, so attacking it is worth doing even when you cannot
  finish it, and defending it is worth doing at any health.
- The same is true of the Totem of Time. A team that has lost neither support
  Totem but has let both be worn down fights a slower, poorer fight long before
  anything is destroyed.

### Mana regeneration

    team_mana_per_second = mana_totem_current_hp × mana_per_totem_hp_per_second

Linear in current HP, so a destroyed Totem contributes exactly zero without
needing a separate rule for the destroyed case. `mana_per_totem_hp_per_second`
is MOBA design data.

Mana is a **team** resource, not a per-Character one: one pool, shared by the
five members, filled by the Totem and spent by whoever casts. That is the
reading of "gives the team mana regeneration" and it is the more interesting
one, because it makes a wasted ability cost a teammate something. Whether it
survives playtesting is an open question; a per-Character pool fed at the same
rate is the obvious alternative and changes no other part of the design.

### Respawn time

    respawn_seconds = max_respawn_seconds
                    − (time_totem_current_hp / time_totem_max_hp)
                      × (max_respawn_seconds − min_respawn_seconds)

At full health the team waits `min_respawn_seconds`; at zero,
`max_respawn_seconds`. Both are MOBA design data.

The delay is read **at the moment the respawn resolves**, not at the moment of
death, so destroying a Totem of Time lengthens the wait of everyone already
lying dead. That is the more dramatic rule and it needs no extra state.

### Totem health and hurt geometry

A Totem's MaxHP is MOBA design data rather than derived from its authored fill
area. The World-01 area-based derivation exists to keep Characters comparable
to one another; a building is not on that scale, and deriving a Totem's health
from how large it happens to be drawn would tie a balance value to an art
decision.

Its hurt geometry follows the shared rule unchanged: a Totem declares an
authored `hurt` Region, and a Totem that authors none cannot be hit. It also
declares a `collision` Region, because a Totem is something you walk around.

## Mana

Mana does not exist in World 01. The MOBA introduces it, and it stays
MOBA-owned until another game wants it.

- Mana is a team pool with a maximum, regenerated by the Totem of Mana.
- Abilities cost mana. The two existing abilities, `HammerStrike` and
  `MageEyeBeams`, receive MOBA costs; they cost nothing in the sandbox and
  nothing in The Labyrinth.
- Stamina is unchanged and stays per-Character. RUN and DASH keep costing
  stamina exactly as World Design defines them. Mana and stamina are different
  resources for different things: stamina is what your body spends on moving,
  mana is what your team spends on acting.
- An ability whose mana cost cannot be paid does not start. It is not queued
  and it does not partially charge.

## Life, death and respawn in this game

World 01's life model stays in force and the MOBA overrides two things.

What is kept, unchanged:

- Reaching zero HP transitions the Character to `DEAD`.
- A teammate can revive a dead Character by holding `A` over its body for
  `8` seconds, returning it to `ALIVE` at `80%` MaxHP. A revived Character
  never uses the respawn timer at all. This is kept because it is a World 01
  mechanic worth having in a team game, and because it gives a dead teammate's
  body a location that is worth fighting over.
- The dead Character can reject the revival with its own `A` input and enter
  `DEATH_CONFIRMING`, as in World 01.
- **The Ankh is the nearest one, not the team's own.** World 01 respawns at the
  Ankh nearest the death position and the MOBA keeps that unchanged, decided on
  2026-09-15. It means dying deep on the enemy bank returns you there, which
  makes how far you push and where you choose to fall a risk you are taking
  rather than a detail. An Ankh therefore belongs to no team and appears in no
  design file.

What the MOBA overrides:

- **The respawn is timed.** Completing `DEATH_CONFIRMING` no longer respawns
  immediately. It commits the Character to the queue, and the Ankh spawn
  happens after the Totem-of-Time-derived delay.
- **Respawn health** is MOBA design data rather than World 01's `40%`.

## Creeps

Each team sends waves of creeps down the three lanes. They march toward the
enemy, they meet near the bridges, and they fight without anyone telling them
to.

- A creep is an Actor with a `TeamId`, health, one attack, and no
  `PlayerOwner`. It is not a Character in the roster sense: it has no character
  selection, no abilities, no stamina and no mana.
- Creeps are driven by the same behaviour trees as bots, with a much smaller
  tree. A creep is not a separate AI system; it is a bot with three nodes.
- A wave spawns on a cadence from a point behind each team's own Totems and
  walks its lane.

### What a lane is, technically

A lane is an ordered list of waypoints in the MOBA's design data, one list per
lane per team, ending at the enemy Totem of Life. A creep paths from its
current position to its next waypoint with the shared ground navigation graph
and advances when it arrives.

The alternative - letting creeps simply path to the enemy Totem of Life and
calling whatever route they take a lane - was rejected because five bridges and
three lanes do not agree: every creep on the map would take the cheapest
bridge, and the other four would stand empty. Waypoints are what make three
lanes three lanes.

Waypoints are coordinates, so they are cheap to tune and they need nothing new
from SceneMaker. Their values wait on the authored map.

## The roster

The MOBA exposes five of World 01's characters:

**Mage, Hammerer, Rogue, ArcherF, Monk.**

Each has **two attack abilities**. Two of the ten exist today:

| Character | Ability 1 | Ability 2 |
|---|---|---|
| Hammerer | `HammerStrike` (exists) | to be decided |
| Mage | `MageEyeBeams` (exists) | to be decided |
| Rogue | to be decided | to be decided |
| ArcherF | to be decided | to be decided |
| Monk | to be decided | to be decided |

The eight undecided abilities are a gate, not a plan: the developer decides
what they are, and the roadmap says when. What can be said now is the shape
they will be built from.

### Two shapes, not eight implementations

`HammerStrike` and `MageEyeBeams` are each a bespoke system with its own
replicated state component. Building eight more the same way would be eight
more of everything, and a Rogue's dagger is not a new idea - it is a short
melee arc, the way the Hammer is a long one.

Almost every plausible ability in this roster is one of two shapes:

- **A melee swing.** Authored `attack` Regions on a weapon Asset, evaluated
  against target hurt geometry at an authoritative impact tick, with
  per-Component effect overrides. `HammerStrike` already is this.
- **A travelling projectile.** One or more projectiles emitted from authored
  origins along a locked direction, stopping at the first collision, carrying
  damage and effects. `MageEyeBeams` already is this.

So the eight new abilities are built as design data over two reusable
implementations extracted from the two that exist, rather than as eight new
systems. Charge curves, damage, reach, cooldowns, mana costs and effects are
values; the behaviour is shared. An ability that genuinely fits neither shape -
a blink, a shield, a heal - is a new implementation and is allowed to be one,
but it has to earn that by not fitting.

This extraction is sandbox-generic. It returns to `main`.

### Two attack inputs

`PlayerInput` carries one `AttackIntent` today. It gains a second,
`attack_secondary`, bound to a second button. That is the whole change: two
abilities, two buttons, no ability-slot indirection.

A MOBA that later wants four abilities per Character will want slots, and this
decision is made in the knowledge that it may be revisited. Two fields for two
abilities is what the current iteration needs, and the project rule is to build
that rather than the system the third and fourth ability would want.

Nothing about an ability crosses the network, and it is worth writing down why,
because it is what makes eight new abilities cheap. What goes up is the bool.
Which ability it starts is decided by what the Actor *is*: the join handler
attaches `HammerAttackState` or `MageAttackState` from the character's design
profile, and `advance_hammer_attacks` and `advance_mage_attacks` query on those
components, so the archetype decides which system sees the press. No character
name is compared and no ability identity is sent. What comes back is the
ability's own state component, replicated and predicted, from which the client
derives every visible thing.

That routing is also why a second input field is not quite the whole change.
Both systems read the same `AttackIntent` today, which is unambiguous only
because no Actor carries two ability states at once. The moment one does, a
single press would start both. So an `Ability` also has to say which input it
listens to, and `CharacterAbilityCatalog` attaches that binding along with the
state - still design data, still one arm in `Ability::from_name_key`, but a
second thing the catalog knows.

The binding arrives with the abilities that need it, in Phase 5, and not with
the input field in Phase 1. A binding with one legal value is an abstraction
with no consumer.

## Overrides of World 01 baselines

The MOBA's numbers differ from the MMORPG's. Characters move faster, hit
harder, and have more health, because a match is minutes long and a world is
not.

These overrides live in `design/games/moba.toml`, loaded over
`design/world01.toml`. The overlay holds **only** the values the MOBA changes;
everything it does not name keeps its World 01 value and keeps tracking it when
World Design changes.

The file's expected contents, by area:

| Area | What the MOBA overrides |
|---|---|
| Movement | One multiplier over each Character's World-01 derived normal speed, `2.1` by default for every Character and overridable per Character. The mass curve, its exponent and the relative ordering of Characters stay World 01's. The MOBA is the fastest game in World 01 and the sandbox, which becomes the MMORPG, is the slowest; one factor is what that sentence costs. |
| Health | The MaxHP normalization reference. The area-based derivation is unchanged, so re-authoring a Character's body still changes its health the same way. |
| Damage | Per-ability damage values. |
| Life | Respawn health percentage, minimum and maximum respawn seconds. |
| Mana | Team pool maximum, mana per Totem HP per second, per-ability costs. |
| Totems | MaxHP per Totem kind, and the placement rank of each Totem Asset. Ranks are added by the overlay rather than overridden, so the overlay is a merge over `world01.toml` and not only a field-by-field replacement. |
| Creeps | Wave cadence, creep health, creep damage, lane waypoints. |
| Teams | Which authored Prop IDs belong to West and to East. |

Multiplying the derived speed rather than restating it per Character is
deliberate. The World-01 mass curve decides that the Rogue is the fastest and
the Hammerer the slowest; the MOBA decides how fast the whole game is. A factor
keeps both facts true, keeps each in one place, and lets a single Character
deviate later without anyone having to retype the other four.

At `2.1` the Hammerer walks `1.26 m/s`, runs `1.89 m/s` and dashes at
`2.52 m/s`. Crossing `map01`'s `135 m` takes about `107` seconds at a walk and
`71` running, against more than three minutes at the World-01 baseline.

## Bots

The bots are the reason this game exists as a slice rather than a sketch, and
they are specified in enough detail to build the first iteration and no more.
Their behaviour is expected to be wrong at first and to be fixed by playing
against it.

### Architecture

A `crates/ai` crate, generic and free of MOBA semantics, holding a behaviour
tree with utility-scored selection. This is task `SBX-25` in the tracker and it
is built here.

- The tree is **data-driven**: nodes, conditions and actions are resolved by
  name from design files. Adding a behaviour is a data change plus, where the
  behaviour is genuinely new, one named condition or action in a match arm -
  the same shape `Ability::from_name_key` already uses.
- Node kinds are deliberately few: `Sequence`, `Selector`, `Condition`,
  `Action`, and `UtilitySelector`, which scores each child and runs the highest
  scorer. Scores come from named scoring functions over the world the bot can
  see. That is the whole language; a bot that needs more than this is a bot
  whose behaviour was not thought through yet.
- A tree runs in a new `SimulationSet::Decision` phase **before** the gameplay
  step, and it **writes only intent components** - the same `MovementIntent`,
  `GazeIntent`, `AttackIntent`, `AttackSecondaryIntent`, `RunIntent`,
  `DashIntent` and `DeathConfirmIntent` a human's input writes. A bot has no
  path to health, position, or any authoritative outcome. It plays the game
  through the same opening a player does, which is what keeps it honest and
  what makes a bot's exploit a player's exploit.
- Each bot carries a small blackboard: current target, current path and the
  index along it, and a few timers.

### Where a bot runs, and what it may know

Bots are **server-only**. They are replicated as server-authoritative and
interpolated rather than predicted, so `SimulationAuthority::Predicted` skips
them entirely. This is task `SBX-26`.

Two things follow, and both are the reason:

- Neither the behaviour tree nor the pathfinding has to be deterministic on the
  client, because the client never runs them. That removes an entire class of
  divergence from a system that will be iterated on constantly.
- A bot's knowledge never reaches a player's machine. The ground navigation
  graph is already derived only on the server for this reason; the bot's
  decisions join it there.

A bot's evaluation function reads the same shared surfaces a player's
controller reads - the status action mask, incoming-damage modifiers, the
navigation graph, the ability catalog - rather than its own copies. This is
what task `SBX-32` asks for and the bot work is what unblocks it. Two rules
that decide the same thing are two rules that will disagree.

### Decision cadence

A bot does not decide sixty times a second. Its tree runs every `N` fixed
ticks, staggered across bots so ten of them never think in the same tick, and
its intents are held between decisions. `N` is design data and starts around
ten decisions per second.

This is cheaper, but that is not the reason. A bot that re-decides every tick
twitches, reverses on noise, and is unreadable to the player it is fighting.
Holding an intent for a tenth of a second is most of what makes a bot look like
it meant something.

### First iteration behaviour

Deliberately small, and expected to be replaced:

1. If dead, confirm death.
2. If health is below a threshold, walk to the nearest own Ankh.
3. If an enemy is within ability reach, face it and attack.
4. Otherwise, advance along the assigned lane toward the enemy Totem of Life,
   attacking the nearest enemy Totem when one is in reach.

Target selection is a `UtilitySelector` over visible enemies, scored on
distance, current health, and whether the target is already being attacked by a
teammate. Everything beyond that is the next iteration's problem.

### Later behaviours, not built

Recorded so they are not re-invented, and deliberately out of first-iteration
scope: focused fire on a called target, retreating as a group rather than
individually, ability combinations across Characters, holding a bridge,
rotating to a lane under pressure, baiting with the DASH invulnerability
window, denying a revival by finishing the body, splitting to force the human
to choose, and using the five bridges as a flank rather than as five copies of
the same crossing.

## Presentation

First iteration only, and following the World-01 preference for diegetic
communication over overlays:

- A Totem shows its health through its own geometry and material rather than a
  bar. What that looks like is an authoring decision; a placeholder that reads
  at a glance is enough until it exists.
- Team identity is carried by the Totem Assets themselves, which are authored
  per team. Whether Characters also need a team marking is an open question and
  waits until ten Actors are on the map and the answer is visible.
- The end of a match is a stated outcome, not a screen. A screen is scope for
  later.

## What returns to `main`

`main` is the shared base. The MOBA is a branch. A capability written here goes
back only when its MOBA semantics are gone and its contract stands on its own,
which is [`SANDBOX_VISION.md`](../../SANDBOX_VISION.md)'s rule, not a new one.

Expected to return:

- `TeamId` in `world_data` - identity only, no rules attached.
- The second attack intent in `PlayerInput` and its client binding.
- The `ai` crate: behaviour tree, utility selection, the `Decision` phase, and
  the rule that a tree writes only intents. Nothing in it names a Totem, a lane
  or a team.
- The melee-swing and projectile ability shapes extracted from `HammerStrike`
  and `MageEyeBeams`.
- Whatever the bots need from navigation that is missing today, such as
  resolving the graph node nearest a world position.
- Any shader, animation or presentation capability built here that does not
  describe a MOBA object.

Expected to stay:

- Totems, lanes, creeps, the win condition, mana, team assignment, the MOBA
  spawn rules, and `design/games/moba.toml`.

The working rule that makes this cheap is about commits rather than about
architecture: **a commit is either sandbox-generic or MOBA-owned, never both.**
A generic commit touches only `world_data`, `simulation`, `content`, `network`,
`configs`, `design`'s shared files, or the new `ai` crate, and it compiles and
passes its tests without anything MOBA-specific. Such a commit cherry-picks
onto `main` as it stands. A commit that mixes the two has to be taken apart by
hand later, by someone who has forgotten which half was which.

## Open questions

- **Friendly fire.** World 01 has none as a category and this game inherits
  that. Whether a MOBA can carry it is a playtesting question. If it cannot,
  the override is written here, and it is a rule about damage sources rather
  than about teams.
- **One mana pool or five.** A shared team pool is the current reading and the
  more interesting one. A per-Character pool fed at the same rate is the
  fallback and costs nothing to switch to.
- **Whether the two teams' Totems need to look different.** They are the same
  three Assets on both banks today, so across a river the picture does not say
  whose a Totem is. Deciding to commit is exactly the moment that question gets
  asked. Team colour applied at spawn, a marker Prop, or three more authored
  Assets are the candidates; which one waits until ten Actors are on the map and
  the problem is visible rather than predicted.
- **Whether Characters need visible team marking**, and whether that is colour,
  a symbol, or something the Ankh does to whoever spawns from it.
- **What the second ability of each of the five Characters is.** A gate, owned
  by the developer.
- **Whether two attack inputs survive.** If the MOBA grows past two abilities
  per Character, `PlayerInput` gains slots and this decision is revisited
  deliberately rather than by accretion.
- **Whether respawning at the nearest Ankh stays fun.** It is now a deliberate
  rule rather than an open question, and the thing to watch in play is whether
  returning inside the enemy bank reads as a risk the player took or as a
  punishment the map handed out.
- **Whether creeps should be attackable by their own team.** Follows from the
  friendly-fire question and probably does not deserve a separate answer.
