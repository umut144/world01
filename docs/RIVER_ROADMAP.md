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

## Phases

### Phase 1 — one river, no branches

A river is unwalkable, drawn as a ribbon, and carries flow markers.

- **World 01**: read `water_bodies` and `water_raster`; enter the raster into
  the column rule so the river cannot be walked and the bridge above keeps its
  clearance; draw the ribbon; place `river01` markers by station and lateral
  offset, oriented on the tangent, drifting with time, scaled continuously —
  thicker mid-channel, thinner towards the bank.
- **SceneMaker**: a water bake per water body, in the shape the Path and Bridge
  bakes already use — vertices with elevation, triangles, boundary edges and
  centerline samples with station and width.
- **PolyTools**: whether `river01` is meant to be an outline or to carry a fill
  mesh; more Palette variants when variety is wanted.

Everything a marker shows is presentation: the choice of variant and the size
are functions of station, lane and time, identical on every client, and nothing
about them is sent or stored.

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
| SceneMaker | A water bake per water body, in the shape of the Path and Bridge bakes. |
| SceneMaker | How a branch is expressed, and how an activatable one is marked. |
| SceneMaker | The curve rule: what `mode` means, how the handles are read, and the flattening tolerance — needed even with a delivered bake, so that a World 01 reader can check what it receives. |
| PolyTools | Is `river01` an outline or a filled surface? More variants for the Palette. |
| World 01 | Is the width cap exactly one half, and is it evaluated per branch point or across nearby stations? |
