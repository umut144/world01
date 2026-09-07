# World 01 — Rivers: authoring, presentation and simulation

Last updated: 2026-09-07

## Purpose and authority

This document records how a river reaches World 01 and in which order the
capability is built. It is a roadmap, not a specification: the contracts it
depends on live in [`SANDBOX_TECHNICAL.md`](SANDBOX_TECHNICAL.md) and in
SceneMaker's own export contract, and shared design lives in
[`WORLD_DESIGN.md`](WORLD_DESIGN.md). `TASKS.md` carries only the phase that is
current; the phases beyond it are described here so that nobody has to
reconstruct the intent from a tracker row.

## Where a river comes from

A river is authored in SceneMaker and arrives twice, for two different readers:

- **The spine**, in `scene.water_bodies`: an ordered set of authored points with
  bezier handles, and per point an elevation, a width, a channel depth and a
  clearance above. This is what the author drew.
- **The raster**, in `water_raster`: the spine resolved onto the finer water
  grid, one cell each carrying `bed_meters`, `surface_meters` and
  `cut_top_meters`. This is what the author inspected in the Section View.

The division of labour follows from that:

- **The raster is the truth for the simulation.** Where water is, what lies
  under it, and how much air a bridge has above it, is read from the raster and
  enters the same column rule as an authored Path cut.
- **A ribbon along the spine is the truth for the eye.** It carries the flow
  direction, a station along the course and a width across it, which is the
  coordinate system every moving decoration needs.
- **World 01 derives neither.** Both arrive from SceneMaker.

That last line was decided on 2026-09-07 and the reasoning matters, because the
opposite was considered first. If branches appeared procedurally during play,
SceneMaker could not be the source of their geometry, and World 01 would have
had to own the flattening of every river — otherwise a branch drawn by one
flattener would meet a main channel drawn by another, at the junction, where
the eye is. Authoring the branches instead keeps one flattener, and it also
keeps the raster authored: a branch World 01 invented would have to be
rasterised by World 01, in the simulation, deterministically on server and
client. Authored branches remove that entire problem rather than solving it.

## Templates serve two purposes

A Template fills an Anchor while the game runs, and it is also a building block
for maps generated **before** the game runs. A river that lives in a Template
therefore has to survive both paths. Today the map reader refuses any Template
that carries water at all, which is the deliberate starting point, not an
oversight: composition would have to merge water bodies and rasters the way it
already merges Terrain and Props.

## The width budget

A branch takes water from its parent, and the parent continues narrower by
exactly what the branch took:

    width_after = width_before − width_branch

A branch may take at most half of the width available at that point:

    width_branch ≤ width_before / 2

The two rules together reproduce the authored intent exactly. From an 8 m
river, at most 4 m may branch off. From that 4 m branch, 1 m branches may be
taken three times — the caps are 2 m, then 1.5 m, then 1 m, and a fourth would
need a cap of 0.5 m and is refused, leaving a 1 m channel. A cap of "anything
that leaves something behind" would allow 7 m out of 8, so the half is what
makes the first number a rule rather than a coincidence.

Open: whether the cap is exactly one half, and whether it is evaluated at the
branch point alone or across a window of stations when two branches sit close
together.

## Tides and drying, which are not built

Water in this world rises and falls, and a river can dry out; a bed that falls
dry is meant to open the cave mouths in its flanks. None of that is built, and
none of it is designed here. What is settled is that the column may not make it
impossible:

- A water cell offers **two** walking surfaces, the bed and the water standing
  over it, the same way an excavated column offers the tunnel floor and the
  ground above it. The bed is never dropped just because water covers it today.
- A surface under standing water is marked **flooded**, and a flooded surface is
  offered to nobody. That is what keeps a Character on the bank without any rule
  about water having to be written into movement.
- The water level a surface is measured against is the authored one. A world
  with tides would move that value; the same ground would stop being flooded,
  and the graph would be rebuilt the way it already is when a map changes.
- The depth at a place is the difference between the two surfaces of its column,
  so wading - crossing where the water is shallow - would not need new data
  either, only a rule in a Character's profile.

Until then water is simply not walkable, at the bank and everywhere else.

## A bed is entered, never fallen into

A river cannot be crossed, wet or dry. A dry bed can be stood in, but only by a
way somebody authored into it — a ramp running down the channel, a cave in its
flank. Falling in was considered on 2026-09-07 and withdrawn the same hour: it
would have meant an asymmetric step rule, where a drop beyond a Character's step
height is allowed downwards and refused upwards, and that rule would have made
every ledge in the world enterable, not just this one.

Two things follow, and both are the reason the decision cost nothing:

- **Nothing has to be built for it.** The step rule is symmetric, so a bed a
  metre below its bank is refused in both directions already.
- **A ramp is a Path.** World 01 has read authored Paths with their grade since
  the Path work, and stepping onto one from Terrain and off it at the far end is
  the rule that already carries a bridge deck. A ramp into a riverbed is that
  rule pointing downwards; it is authoring, not code.

Beds are therefore authored deeper than a Character can step - a metre - so that
the channel is a place one arrives in on purpose.

## Phases

### Phase 1 — one river, no branches — **done, 2026-09-07**

A river is unwalkable, drawn as a band, and carries the marks that show its
current.

What it took, and what each part decided:

- **The raster is what the simulation reads.** Standing water enters the column
  the way an excavating Path does: the channel is taken out of the Terrain down
  to the bed, and the water stands over what is left. A water cell therefore
  offers two surfaces - the bed, and the water above it. Ground under standing
  water is marked flooded and is offered to nobody, which is the whole rule
  that keeps a Character on the bank. In `overworld01` this turned 269 of
  10 000 Terrain cells into water, and the two authored bridges stopped being
  decorative.
- **The band is what the eye reads.** SceneMaker bakes it from the same
  flattener a Path uses. Unlike a bridge deck it carries no authored interval,
  so it is not one of the surfaces a Character walks along - where water may be
  stood on was already decided by the cells.
- **The centerline is what the current is placed along.** `MapWaterBody::flow_at`
  answers, for one station of the course, where the water is, which way it runs
  and how wide it is. That frame - along the course and across it - is what
  anything the water carries uses, and the first thing it carries is the marks.
- **A mark is a contour, not a surface.** `river01` is authored as a line with
  its own thickness, and it is drawn flat the way an eyelash and an eyebrow have
  always been drawn. It is scaled evenly, larger mid-channel and smaller towards
  the bank, so the authored line keeps its weight: one authored shape, scaled to
  what the place asks for, the way a plank is.

Everything a mark shows is presentation: the choice of variant and the size are
functions of station, lane and time, identical on every client, and nothing
about them is sent or stored.

Open from this phase: how many variants the `river` Palette should hold. One is
legal and works; variety is an authoring decision, not a blocked dependency.

### Phase 2 — one authored branch, switched on in play

A branch is authored in SceneMaker, marked as activatable, and shown there as
what it is. In play a trigger turns it on: from that moment its raster counts
in the column rule and its ribbon is drawn.

The unit of activation is a water body. The state behind it is a set of
booleans — small enough to replicate and to persist, unlike geometry.

- **SceneMaker**: how a branch is expressed (an own water body with a reference
  to its parent, or points inside one body), and how "always on" is told apart
  from "activatable".
- **World 01**: activation state, and the rule that an inactive body is neither
  drawn nor rasterised.

### Phase 3 — branches of branches, and the width budget

Authoring a branch shows, at the branch point, how much narrower the parent
becomes, with a number the author sets. The budget above is enforced where it
is authored, so an impossible river cannot be exported.

- **SceneMaker**: the authoring input and the constraint.
- **World 01**: read the resulting widths; refuse an export whose branch widths
  do not add up, because a rule that is only enforced upstream is a rule that
  breaks silently downstream.

### Phase 4 — markers divide at a junction

The markers flowing in a channel split at a junction in proportion to the
widths, so that a river visibly separates rather than two independent streams
of decoration overlapping.

Consequence to settle when it is reached: a marker's lane is currently a
function of the channel it is in. At a junction a lane has to be continuous
across two channels of different width, which is a mapping question, not a
rendering one.

### Phase 5 — rivers in Templates

Water becomes composable: a Template may carry a river with its branches, and
the map generator builds maps from finished river Templates instead of from a
river generator.

- **World 01**: lift the rule that a Template carries no water; merge water
  bodies and rasters in composition, including the ID namespacing that Props
  already use.

## Open questions

| To | Question |
|---|---|
| SceneMaker | How a branch is expressed, and how an activatable one is marked. Held as `WATER-01`. |
| PolyTools | How many variants the `river` Palette should hold. Not blocking: a Palette of one draws a river already. |
| World 01 | Is the width cap exactly one half, and is it evaluated per branch point or across nearby stations? |
| World 01 | What happens to an Actor standing in a dry bed when the water arrives. It is the first case where a state change takes the ground out from under someone: nothing pushes an Actor out of ground that stopped existing, the way separation pushes one out of a collider. Rare, because a bed is only entered by an authored way — washed to the bank, or drowned, are the two candidates. Undecided on purpose; it is a design question, and it waits for the first authored ramp. |

Answered and closed:

- *A water bake per water body* — delivered as export 16, in the shape asked
  for, with the height per sample because a river is not level.
- *The curve rule* — no longer needed. It was asked while World 01 briefly
  considered flattening the spine itself; with branches authored and the band
  delivered, nothing here recomputes the curve.
- *Is `river01` an outline or a filled surface* — an outline, authored with its
  own thickness, and right for a mark of current.
