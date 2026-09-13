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
- **Three lanes** run West to East. They are not authored objects. They are
  the corridors left open by tree cover, and they exist because most of the map
  is covered in Tree Props. A lane is where you can walk, not a thing the map
  declares.
- **Four Ankhs**, two on each bank, one pair per team. They are the team's
  respawn points and they use the shared World-01 Ankh.
- **Six Totems**, three per team, described below.

The river divides the map, the bridges are where the map narrows, and the
Totems sit behind each team's own bank. Everything else about the layout is an
authoring decision and is not fixed here.

### How the runtime learns what stands where

Two facts have to reach the simulation about a placed Prop: **what kind of
thing it is**, and **whose it is**.

The kind comes from the PolyTools Asset key. A Prop with the asset key
`totem_life` is a Totem of Life because that is the Asset that was placed;
nothing infers it from a name convention or a position.

Whose it is comes from the game's design data, which names the authored Prop
IDs belonging to each team. The map export gives every placed Prop a stable ID,
and `design/games/moba.toml` lists them per team for both Totems and Ankhs. A
Prop the list forgets, or a listed Prop the map does not contain, is a load
error. This follows the World-01 no-fallbacks principle at the place it
actually matters: a Totem whose side was guessed would look like a working game
and be the wrong one.

The alternative was deriving the side from which half of the river a Prop
stands on. It was rejected on 2026-09-13 for exactly that reason - a Totem
placed on the wrong bank would silently become the enemy's rather than failing
to load - and because the river is a curve, not an axis, so "which side" is a
question with no cheap honest answer.

The cost of the chosen rule is real and is accepted for now: moving a Totem in
SceneMaker means editing one line of design data. If map iteration makes that
friction rather than discipline, the better end state is an authored team
marking on the Prop itself in SceneMaker, after which the design-data list
disappears. That is a SceneMaker gate and it is listed in the roadmap as a
later phase, not a blocker.

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

What the MOBA overrides:

- **The respawn is timed.** Completing `DEATH_CONFIRMING` no longer respawns
  immediately. It commits the Character to the queue, and the Ankh spawn
  happens after the Totem-of-Time-derived delay.
- **The Ankh is the team's.** Respawn selects among the two Ankhs of the dead
  Character's own team, not the nearest Ankh on the map. The deterministic
  candidate search around the chosen Ankh is the shared one and is unchanged.
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
| Movement | The normalization speed the mass curve derives from. The curve itself, the exponent, and the relative ordering of Characters are World 01's and are not touched. |
| Health | The MaxHP normalization reference. The area-based derivation is unchanged, so re-authoring a Character's body still changes its health the same way. |
| Damage | Per-ability damage values. |
| Life | Respawn health percentage, minimum and maximum respawn seconds. |
| Mana | Team pool maximum, mana per Totem HP per second, per-ability costs. |
| Totems | MaxHP per Totem kind, and the placement rank of each Totem Asset. Ranks are added by the overlay rather than overridden, so the overlay is a merge over `world01.toml` and not only a field-by-field replacement. |
| Creeps | Wave cadence, creep health, creep damage, lane waypoints. |
| Teams | Which authored Prop IDs belong to West and to East. |

Scaling the *reference* rather than each Character's value is deliberate. The
World-01 mass curve decides that the Rogue is the fastest and the Hammerer the
slowest; the MOBA decides how fast the whole game is. Overriding one number
keeps both facts true and keeps them in one place each.

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
- **Whether Prop team ownership stays in design data** or becomes an authored
  marking in SceneMaker. Decided by how often the map moves.
- **Whether Characters need visible team marking**, and whether that is colour,
  a symbol, or something the Ankh does to whoever spawns from it.
- **What the second ability of each of the five Characters is.** A gate, owned
  by the developer.
- **Whether two attack inputs survive.** If the MOBA grows past two abilities
  per Character, `PlayerInput` gains slots and this decision is revisited
  deliberately rather than by accretion.
- **Whether creeps should be attackable by their own team.** Follows from the
  friendly-fire question and probably does not deserve a separate answer.
