# World 01 — Rivers: authoring, presentation and simulation

Last updated: 2026-09-09

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

- A water cell offers its **bed** as a walking surface, the same way an
  excavated column offers the tunnel floor and the ground above it, and reports
  how deep the water standing over it is. The bed is never dropped just because
  water covers it today.
- The water itself is **no surface**. Until 2026-09-08 it was one, so that the
  ground beneath it could be marked flooded and offered to nobody; wading
  replaced that rule, and a level on which nobody may stand is a level nobody
  needs.
- The level a depth is measured against is the authored one. A world with tides
  would move that value; the same ground would report a smaller depth, and the
  graph would be rebuilt the way it already is when a map changes.
- Wading therefore needed no new data. The depth was already in the column; only
  the threshold had to be written, and it belongs to the Character.

## A bed is entered, never fallen into

A river deeper than a Character wades cannot be crossed, wet or dry. A dry bed
can be stood in, but only by a way somebody authored into it — a ramp running down the channel, a cave in its
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

Beds that are meant to hold somebody are therefore authored deeper than a
Character can step - a metre - so that the channel is a place one arrives in on
purpose.

## How deep is too deep

Settled 2026-09-08, at the end of the exchange with SceneMaker held as
`WATER-01`.

The metre was briefly a rule of its own: every bed authored deeper than any
Character can step, so that no river could be crossed anywhere. SceneMaker
withdrew it and asked for the opposite case to stay open - a shallow river
should be something one walks through - and handed the threshold to World 01,
because how deep one may wade is a property of a Character and not of a channel.

What stands:

- The column says **how deep** the water over each surface is. It no longer says
  who may be there.
- A Character carries `max_wade_depth_meters` beside `max_step_height_meters`.
  Water deeper than that is no place for it, exactly as a step higher than the
  other is no step. Both are refused by the same kind of rule, in the same
  place, which is why neither needs a special case in movement.
- Every Character wades **0.40 m** today. One number for all eleven until a
  Character is meant to differ; the field sits per Character so that it can.
- Wading is **slower than walking**, at the same half a steep Path costs, so
  crossing water is a decision rather than a shortcut.
- A move that begins or ends in water is priced as wading, by the deeper of its
  two ends. Leaving the water costs what entering it costs, so there is no free
  step back onto the bank.

Nothing in `overworld01` changes yet: its rivers are a metre deep and a metre
below their banks, and two independent rules refuse them - the depth and the
step. What changed is that a shallow river is now something SceneMaker can
author, and it will behave when it arrives.

SceneMaker takes both numbers as workspace values so that a ford is visible
while it is drawn - they know the Terrain head, `bed_meters` and
`surface_meters`, so both differences are theirs already. Two conditions were
attached and both hold: the values describe a reference Character rather than
the law, because the fields are per Character; and the editor shows, never
validates. A river nobody is meant to cross is the normal case, so a check that
forced a ford would forbid good maps. That is the opposite of the width budget,
where a rule has to be enforced where it is authored.

## Phases

### Phase 1 — one river, no branches — **done, 2026-09-07**

A river is unwalkable, drawn as a band, and carries the marks that show its
current.

What it took, and what each part decided:

- **The raster is what the simulation reads.** Standing water enters the column
  the way an excavating Path does: the channel is taken out of the Terrain down
  to the bed, and the water stands over what is left. A water cell therefore
  offers two surfaces - the bed, and the water above it. Ground under standing
  water was marked flooded and offered to nobody, which was the whole rule that
  kept a Character on the bank; on 2026-09-08 that was replaced by a depth and a
  threshold, without the world behaving differently. In `overworld01` this
  turned 269 of 10 000 Terrain cells into water, and the two authored bridges
  stopped being decorative.
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
  from "activatable". Settled in export 19: a body of its own, carrying
  `switch: "<name>"` or nothing, against a scene-level `switches` list of names
  with an `initially_on`. A switch is a name and a bit; any body may carry one,
  a main river included.
- **World 01**: switch state, and the rule that a body whose switch is off
  carries nothing at all - no fill, no cut, the ground standing as if it had
  never been authored.

The named states this replaced (`active_in`, `inactive: dry_bed | absent`) were
withdrawn on 2026-09-09 because nobody could author against them: the word `dry`
named a state in one field and an aftermath in the next, and in every map ever
authored the assignment meant "there when on". A model with named states was
built and used exclusively as on and off.

The dry bed did not go with them, it changed sides. It is not an authoring
statement but a runtime one - drought, season, a closed weir - and the export
already carries it, because a cell names `bed_meters`, `surface_meters` and
`cut_top_meters` separately. Drawing the cut and leaving out the fill *is* the
dry bed. So two independent runtime ideas replace one authored field: whether a
body **exists**, which is SceneMaker's switch, and whether it **carries water**,
which is ours. Existence is authored; weather is not.

**A body is active only if the body feeding it is active.** Added by SceneMaker
in export 18, and the reason it is their rule rather than ours: a branch is fed
by the river it leaves, so switching that river off takes everything hanging
under it, to any depth. Without the rule, "branch on, parent off" would have
been a case World 01 had to invent an answer for. Three parts follow it:

- The feeder is what a body's junction at its own source names, and the export
  says which end that is: `scene.water_bodies[].junctions[].end == "source"`.
  The raster carries the same junction with both stations, and every junction
  appears in both halves, so the two are cross-checked and a disagreement
  refuses the export - the same discipline `activation` gets, for the same
  reason. Two feeders mean water as soon as **either** flows, which is what
  water does.

  `overworld01` witnesses it since 2026-09-09: the feeding chain runs four deep,
  `river_0001` to `river_0002` to `river_0004` to `river_0005`, and
  `upper_valley` sits on `river_0002` and opens on. Switch it off and three
  bodies go, two of which carry no switch of their own. Of the 1 101 distinct
  cells those three hold, 9 stay occupied because `river_0001` runs through
  them and 1 092 fall empty - with `branch_between_bridges` off, which is how
  the map opens. With that second switch on instead, `river_0003` holds 11 more
  of them and the split is 20 and 1 081.

  The rule was briefly "the entry whose `own_station_meters` is 0", and that was
  withdrawn on 2026-09-08 because it infers a direction from a position. A body
  split at its branch point carries a zero for its own source; a further branch
  leaving it at its very start adds a second zero pointing the other way, and
  the reader would follow the wrong one into a ring that does not exist. The
  authored `end` never has two answers. `overworld01` is one branch short of the
  case. SceneMaker deliberately did **not** raise the export number for the
  correction: the bytes always meant this, only the instructions for reading
  them were wrong, and a number that rises for a corrected sentence teaches that
  a rise can also mean nothing.
- A ring of bodies feeding each other is refused at export. World 01 refuses one
  too rather than trusting that: a reader that walks forever is a worse failure
  than one that says no, and a rule enforced only upstream breaks silently
  downstream - the same reasoning the width budget already carries.
- `inactive` stays the property of the body that carries it. A branch that falls
  dry because its parent was switched off leaves its bed or no trace by its own
  authored answer, not its parent's.

Accepted when one and the same cell answers three ways:

| Switch | Water | The column offers |
|---|---|---|
| off | — | whole ground, no channel at all |
| on | yes | the bed, under water deeper than any Character wades |
| on | no | the bed, walkable |

The middle and the last are the pair the old wording asked for. The first is new
and only exists because a switched-off body now leaves no cut behind, so there
is no bed to stand in - which is why the old sentence, written against `dry` and
`flowing`, had to be replaced rather than reworded.

The other half of this phase is that a Terrain cell holds the **stacked** fills
of every body over it instead of only the last one written. In `overworld01`
(export 19) 108 of 1 675 distinct water cells carry more than one body - 102 of
them twice and 6 of them three times, which is what makes the 1 789 cell entries
the five bodies contribute. The fixture is the mouth of
`river_0003`: 27 cells where it meets water that is flowing - 16 on
`river_0001`, 17 on `river_0002`, 6 on both - in one block of x 88-94, y 53-59.
Switch `branch_between_bridges` off and those 27 cells must stay water, because
another river runs through them.

**Run it with `upper_valley` on**, which is how the map opens. Giving
`river_0002` a switch on 2026-09-09 moved no cell at all, but it took the word
"always" off 11 of those 27: with `upper_valley` off, only the 16 cells backed
by `river_0001` still have a second body, and the test quietly shrinks to those
without failing.

Worth knowing why this could not have been caught earlier: every body in that
map is authored to the same depth, so no overlapping cell has two different
values and the last writer happens to write what the first one would have. The
fault is invisible in the geometry today and becomes visible only through
activation, which is to say exactly here.

`stack01` is the fixture for the geometry half, authored by SceneMaker on
2026-09-09 for this purpose: a flat plain at 3 m, three rivers, nothing else.
`river_0001` fills 0.0-1.0 and cuts to 6.0; `river_0002` fills 0.5-1.5 across
it, spans overlapping; `river_0003` fills 2.0-2.5 across it, spans disjoint.
The two crossings are far apart, so the coordinate alone says which case is
under the reader:

| Where | Cells | The column must resolve to |
|---|---|---|
| x 12-19, y 16-23 | 64 | one surface at 0.0 m under **1.5 m** of water: the two spans overlap and merge to `[0.0, 1.5]`, and the two cuts merge to `[0.0, 6.5]` |
| x 34-45, y 16-23 | 96 | one surface at 0.0 m under **1.0 m** of water. The upper fill stands over no ground at all - it is drawn, and it carries nobody |

Its own numbers, for the same reason: 784 distinct water cells, of which 160
carry two bodies - the 64 and the 96 above - out of 944 cell entries.

The second case is the aqueduct, and it is the sharper test. Keeping only the
last body written puts the walking surface at 2.0 m, on ground that the deep
river's cut has taken away, and the river underneath disappears from the column
entirely. Keeping only the first is accidentally right about where to stand and
wrong about nothing visible, which is exactly why one authored example beats
reasoning about the format.

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
