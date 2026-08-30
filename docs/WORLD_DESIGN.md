# World 01 — shared world and cross-game design context

> This document is the canonical design source for concepts shared by multiple
> games in World 01. It is not a technical specification and it is not a
> game-specific rules document. Sandbox-wide technical contracts live in
> [`SANDBOX_VISION.md`](SANDBOX_VISION.md) and
> [`SANDBOX_TECHNICAL.md`](SANDBOX_TECHNICAL.md). Game-specific decisions live
> in the corresponding document below [`games/`](games/).

Last updated: 2026-08-30

## Purpose and authority

This is compact continuity context for the solo developer and AI agents after
a chat or context switch. It records the current design intent for the shared
World 01 setting and for recurring characters, weapons, abilities, and design
principles. It is not a polished pitch or a frozen production specification.

Interpretation rules:

- Distinguish **confirmed direction**, **World-01 baseline values**, and
  **open questions**.
- A game may tune a World-01 value or add an explicit exception, but it must
  document that override in its own game-design document.
- A game must not silently redefine a shared character, weapon, or ability.
- Do not turn examples or brainstorming into requirements.
- Do not implement anything merely because it is described here; an explicit
  implementation request is still required.
- Update this document when the developer makes a durable cross-game design
  decision.

## Normierte Größenentscheidungen

Diese Tabelle enthält die wichtigsten bestätigten Referenzgrößen. Charaktere,
Assets und UI-Elemente werden relativ zu diesen Referenzen abgeleitet. Die
ausführlichen Regeln und Ausnahmen bleiben in den jeweiligen Fachabschnitten
dieses Dokuments maßgeblich.

| Bereich | Referenz | Normwert | Modell | Ableitung / Anwendung |
|---|---|---:|---|---|
| Weltmaßstab | PolyTools- und Weltgeometrie | `1 m = 192 px` | `x_m = x_px / 192` | Einheitliche Umrechnung zwischen authored Assets und Spielwelt |
| Kamera | Maximierter Referenz-Viewport | `2880 × 1800 px` |  | Andere Fensterformate werden über aspektgerechtes Framing und Letterboxing angepasst |
| Flächendichte | Sechs Dichteklassen | `0.00 / 0.25 / 0.50 / 1.00 / 2.00 / 4.00` | `d(k) ∈ {0.00, 0.25, 0.50, 1.00, 2.00, 4.00}` | Relative 2D-Flächendichte der Klassen Weightless / Gas bis Very Heavy |
| Masse | Materialtragende Components | Flächengewichtete Summe | `M(X) = Σ_i A_i × d(k_i)` | Transformierte Fill-Fläche mal Dichtefaktor; ausgeschlossene Components tragen keine Masse bei |
| Bewegungsmasse | Character-Körper | Waffen zunächst ausgeschlossen | `M_move(C) = M_body(C)` | Waffenmasse wird separat abgeleitet; vorbereitetes Gesamtmodell: `M_total(C) = M_body(C) + Σ_w M_weapon(w)` |
| Normalgeschwindigkeit | Hammerer | `0.6 m/s` | `v(C) = 0.6 m/s × (M_body(Hammerer) / M_body(C))^0.25` | Vierter-Wurzel-Massenkurve; RUN und DASH bauen auf der effektiven Normalgeschwindigkeit auf |
| MaxHP | Hammerer | `140 HP` | `HP(C) = 140 × A_HP(C) / A_HP(Hammerer)` | Referenz für die flächenbasierte HP-Normierung |
| MaxHP-Fläche | Alle Charaktere | `body + optional feet` | `A_HP(C) = A_body(C) + A_feet(C)` | Fläche aus exportierten triangulierten Fill-Meshes; automatisch datengetrieben |
| HP-Balkenlänge | Mage | Faktor `1.0` | `L_factor(C) = MaxHP(C) / MaxHP(Mage)` | Temporäre UI-Verifikation; kein vorgesehenes finales HP-Display |
| Pupillenfläche | Jeweilige Augenregion | `26.0%` | `A_pupil(C) = 0.26 × A_eye(C)` | Pupillenradius wird unabhängig aus der jeweiligen Augenfläche abgeleitet |
| Pupillenkollision | Hammerer | `35.0%` seines Pupillenradius | `r_collision(C) = min(0.35 × r_pupil(Hammerer), r_pupil(C))` | Gemeinsame absolute Kollisionsreferenz, begrenzt auf den jeweiligen Pupillenradius |

## World 01 identity

- Shared world identity: **World 01**.
- The repository and Rust package namespace use the technical name `world01`.
- Theme title: **“Secrets, Room's & Travels'”**. The apostrophes are
  deliberately incorrect/unusual for marketing and must not be “corrected”
  automatically.
- World/theme: **High Fantasy**.
- The setting is part of a larger world intended to support a later MMORPG.
- The world currently contains 23 classes. Individual games expose subsets of
  those classes and may use different modes, rules, or presentations.

## World-to-game design model

World 01 owns the identity and canonical behavior of concepts that recur
across games. A game owns selection, context, balance tuning, encounter rules,
and explicit exceptions.

| Layer | Owns | Example |
|---|---|---|
| World 01 | Canonical cross-game concept and baseline behavior | The Hammerer transforms the Hammer during an attack |
| Game design | Availability, context, tuning, and exceptions | A game changes HammerStrike damage or disables Hammerer |
| Sandbox technical | Reusable implementation contracts | Authoritative simulation, replication, content loading |

The default reading order for a shared mechanic is World Design first, then the
active game's design document for overrides. A game document must link back to
this document instead of copying a shared mechanic as if it were game-owned.

## Shared design principles

- Character and weapon concepts should remain recognizable when they appear in
  different games.
- Strong silhouettes, readable transformations, and a small number of
  high-signal geometric identifiers are preferred over surface detail.
- World danger and character state should be communicated through geometry,
  animation, shader/material state, VFX, and sound wherever practical.
- Mechanics should be built iteratively, beginning with the highest
  fun-leverage principles and tunable parameters.
- Simulation/gameplay and presentation/art are separate iteration layers.
- Shared foundations should not prevent plausible future team play, but future
  systems should not be implemented speculatively.

## World scale and authored assets

- The World-01 authored world scale is `1 m = 192 px`.
- PolyTools remains the source for authored character and weapon geometry.
- Semantic attachment frames, weapon guides, and Components carry design
  meaning across games; their use in a specific game may be tuned, but their
  identity must remain stable unless a deliberate World-01 decision changes it.
- The complete authored Hammer Asset and its Weapon frames share the exported
  scale; consuming games do not add a hard-coded weapon-size multiplier.
- The Ankh uses each authored Component's projection depth and retains its
  authored contours in the projected depth form. Its presentation is tilted
  by `+30°` around the Y axis followed by `+30°` around the X axis.
- For the Ankh's projected-depth presentation, only authored PolyTools path
  points whose handle mode is `Corner` produce a contour edge through the
  Component's depth. Smooth points never produce a depth contour edge.
  PolyTools remains the source of this visual intent; runtime rendering
  consumes the exported local point positions rather than inferring corners
  from triangulation.
- The Ankh's Fill, side surfaces, front contour, and authored depth edges use
  geometric depth occlusion within their authored Component layer. Portions of
  a depth edge behind the front face or an extruded side surface are hidden;
  the result is a solid projected object rather than a wireframe.
- World 01 may use multiple distinct 3D presentation models. The Ankh's
  authored-Corner contour model is not an automatic default for the Hammer or
  for other projected-depth Assets; each such presentation is decided
  separately.

## Shared character presentation

### Visual language

- Art direction: **2D polygon style**.
- Characters are intentionally archetypal and minimalist.
- Designs should be constructible from a small number of simple closed
  polygons.
- Character readability comes from a strong primary silhouette and a few
  high-signal geometric identifiers, not surface detail.
- Related characters may share visual language while retaining distinct
  silhouettes, face openings, eye treatment, class symbols, and weapon/form
  motifs.
- Preserve the charming, readable abstraction; do not “improve” it into
  anatomically complex or detail-heavy fantasy art by default.

### Eyes and gaze

- Every currently catalogued character except Barde has eyes with one shared,
  round black pupil shape.
- Each pupil covers exactly **26.0%** of its authored eye polygon's area; its
  radius is derived independently from that eye's geometry.
- Pupil movement uses the Hammerer's **35.0%** pupil-radius collision as the
  World-01 normalization reference. Every other eye uses the same absolute
  collision radius, capped at its own pupil radius.
- Every closed-region edge remains a collision boundary, but only edges with a
  visible outline clip the pupil. Hidden outline edges may therefore retain a
  round pupil overlap. The contour remains visually in front, and eyes have no
  separate visible fill.
- Gaze is independent from movement, body facing, and weapon orientation.
- Directional input selects one of eight gaze directions immediately. Releasing
  directional input retains the visible gaze direction. The default direction
  is right; there is no neutral runtime gaze.
- Each character keeps its own eye geometry, eye positions, and eye pivots.
  Pupil movement is constrained by that eye's geometry rather than the overall
  character pivot. Rotated or mirrored eye assets retain the same visible look
  direction.
- Whether a particular game uses `IJKL`, a controller stick, or another input
  mapping is game-owned. The gameplay meaning of retained gaze is World-01
  design.

### Movement-dependent body pose

- Each PolyTools character asset declares its authored initial pose as Left,
  Right, Neutral, Top, or Down.
- Authored Left/Right poses follow the character's authoritative body facing.
  Moving horizontally in the authored direction keeps the original geometry;
  moving in the opposite direction mirrors the complete character presentation.
- Neutral characters do not flip. While moving, their complete authored head
  subtree shifts slightly in the current movement direction, including
  diagonals, and smoothly returns to its authored neutral position when they
  stop.
- Top and Down are retained as authored metadata for later use and have no
  shared runtime behavior yet.
- Pure vertical movement and stopping retain the last horizontal body pose. At
  spawn, the character uses its authored pose.
- Body pose and eye gaze are independent. Mirroring the body must not mirror
  the gaze in screen space.
- Body facing and gaze are retained and replicated independently when a game
  uses networked authoritative state.

## Shared mass and movement-speed foundation

### Density classes and mass derivation

- World 01 uses six relative two-dimensional density classes. They represent
  gameplay areal density rather than physical kilograms per cubic meter,
  because authored character Components provide area without a canonical
  material thickness.
- Density class numbers are stable identifiers, not density values to use
  directly in calculations.

| Class | Name | Areal-density factor | Baseline materials |
|---:|---|---:|---|
| `0` | Weightless / Gas | `0.00` | Smoke, steam, fog, gas, magical particles |
| `1` | Very Light | `0.25` | Hair, feathers, fur, leaves, thin fabric |
| `2` | Light | `0.50` | Hats, clothing, leather, light plants |
| `3` | Medium | `1.00` | Wood, body/tissue, rubber, bone |
| `4` | Heavy | `2.00` | Glass, stone, ceramic |
| `5` | Very Heavy | `4.00` | Steel, iron, massive metal parts |

### Confirmed current catalog assignments

The following assignment is the confirmed World-01 baseline for the current
Character catalog. `Excluded` is intentionally separate from density class
`0`: excluded Components are not material for this model, while class `0`
remains available for material that has no relevant gameplay mass.

| Character | Very Light (`1`) | Light (`2`) | Medium (`3`) | Excluded |
|---|---|---|---|---|
| ArcherF | `head_tip01`, `head_tip02`, `head_tip03` | — | `body`, `feet`, `head`, `forehead` | `eye_left`, `eye_right` |
| Barde | — | — | `feet`, `body`, `head` | `belly`, `eye_left`, `eye_right` |
| Chantres | — | `belly`, `hat` | `body`, `head` | `eye_left`, `eye_right`, `hat_line` |
| Glavier | — | `belly`, `head_tip` | `body`, `head` | `eye_left`, `eye_right` |
| Hammerer | — | `cloak`, `hat` | `body`, `feet`, `head` | `eye_left`, `eye_right` |
| Mage | — | `hat`, `hat_tip` | `body`, `head` | `arm_line`, `eye_left`, `eye_right` |
| Monk | `eyebrow_left`, `eyebrow_right` | — | `feet`, `body`, `head`, `forehead` | `eye_left`, `eye_right`, `forehead_dot01`–`forehead_dot06` |
| Rogue | — | `hat`, `hat_tip` | `body`, `head` | `arm_line`, `eye_left`, `eye_right`, `eyebrow_left`, `eyebrow_right` |
| Sorcerer | — | `hat_back`, `hat` | `body`, `head` | `arm_line`, `eye_left`, `eye_right` |
| Warrior | — | — | `body`, `head`, `forehead`, `body_side_left`, `body_side_right`, `thorn_side_left`, `thorn_side_right`, `thorn_left`, `thorn_right` | `eye_left`, `eye_right` |
| Wizard | — | `hat` | `body`, `head` | `arm_line`, `eye_left`, `eye_right` |

| Weapon | Medium (`3`) | Very Heavy (`5`) |
|---|---|---|
| Hammer | `shaft_center` | `head_mid`, `head_left`, `head_right`, `shaft_bevel_top`, `shaft_bevel_bottom` |

- A material-bearing Component contributes its transformed triangulated fill
  area in square meters multiplied by its density factor. A Character's mass
  is the sum of those Component contributions.
- Contours and contour-only Components do not contribute mass. This includes
  authored line Components such as `arm_line`, eye geometry, and decorative
  contour marks such as `forehead_dot`. Technical guides, attachment frames,
  and presentation-only geometry likewise do not contribute mass.
- Material-bearing and excluded Components must be classified explicitly. A
  Component is not included merely because it has geometry or a recognizable
  name; overlapping presentation geometry must not accidentally count the same
  physical material twice.
- Weapons have their own derived mass. Both Character-body mass and total
  equipped mass remain available as distinct values so games can choose which
  model they use.
- The initial normal-movement model uses Character-body mass only and excludes
  weapon mass. Weapon mass must therefore be derived without affecting initial
  movement speed, and the boundary must allow a later game to include equipped
  weapon mass without replacing the derivation model.

### Mass-derived normal movement speed

- The Hammerer is the World-01 normalization reference. His effective normal
  movement speed is `0.6 m/s`.
- Initial Character speed uses the fourth-root mass curve
  `speed = 0.6 m/s * (Hammerer body mass / Character body mass)^0.25`.
- RUN and DASH continue to derive from the resulting effective normal movement
  speed through their existing multipliers.
- The formula has no arbitrary minimum or maximum speed clamp. The Rogue is
  intentionally the clear fastest current Character and may run more than
  twice as fast as the Hammerer; the Hammerer is the slowest current Character.

## Shared movement abilities and status foundations

These are the World-01 baseline abilities for playable characters. Games may
tune values, controls, or availability, but a different mechanical behavior
must be documented as an explicit game variant.

### RUN

- Every playable character has RUN by default.
- RUN is a toggle intent. It may remain active while the character is
  stationary and consumes no stamina until actual movement begins.
- While moving with RUN active, stamina drains at `8` absolute points per
  second.
- RUN uses the effective normal movement speed multiplied by `1.5`.
- RUN disables automatically when stamina is depleted.

### DASH

- Every playable character has DASH by default.
- DASH activates on button press and does not repeat merely because the input
  remains held.
- DASH requires a non-zero current physical velocity vector. Its direction is
  locked at activation and its speed is twice the current speed magnitude.
- DASH lasts exactly `1` second. RUN may remain active after DASH, and its
  continuous stamina drain continues while DASH carries the character.
- DASH costs `17%` of maximum stamina immediately. The cost may exceed current
  stamina; stamina is reduced to zero and depletion consequences are applied.
- If DASH depletion triggers KNOCKDOWNED, that status takes precedence in the
  same simulation tick, disables RUN, and interrupts DASH.
- DASH provides an invulnerability window of `0.337` seconds centered on the
  middle of its one-second duration: `[0.3315s, 0.6685s)`.

### Stamina and statuses

- Stamina is a numeric resource. `100` is the standard baseline, but a
  character may have more than `100` maximum stamina.
- Costs may be expressed as absolute values or percentages of maximum stamina.
- Stamina regenerates continuously at `2.5%` of maximum stamina per second,
  including during normal movement, RUN, KNOCKDOWNED, or STUNNED.
- Stamina depletion at or below zero triggers KNOCKDOWNED, applies damage equal
  to `5%` of maximum HP, and disables RUN.
- KNOCKDOWNED lasts `2` seconds and blocks all input. It is mechanically
  STUNNED plus the stamina-depletion damage penalty.
- KNOCKBACKED is a separate status effect. KNOCKDOWNED, STUNNED, SILENCED,
  DISARMED, and ROOTED remain distinct effects.
- STUNNED blocks all input, SILENCED blocks shoulder-button abilities,
  DISARMED blocks action buttons, and ROOTED blocks movement from the left
  analog stick. Exact input mappings remain game-owned.

### Shared incapacitation presentation

- `STUNNED` hides the character's contour, scales the character to `90%`, and
  tilts it `14°` counterclockwise.
- `KNOCKDOWNED` reuses the `STUNNED` presentation and input-blocking logic. It
  remains distinct because it also carries the stamina-depletion damage and
  its own duration.

## Shared health and damage foundations

- Current and maximum HP are authoritative gameplay state when a game uses the
  shared health foundation.
- A character's maximum HP is derived from the summed area of its current
  triangulated PolyTools fill meshes for the semantic `body` component and,
  when present, the semantic `feet` component.
- The Hammerer is the World-01 normalization reference at `140 HP`; other
  baseline values use the same HP-per-square-meter ratio.
- Resizing or reauthoring a character's `body`/`feet` geometry changes derived
  MaxHP after the next asset sync/export. A second duplicated HP table is not
  introduced in `design.toml`.
- Contours, outlines, eyes, clothing, weapons, and auxiliary overlays such as
  `body_side_*` are excluded from the area calculation.
- When a temporary HP bar is used for development, Mage uses a bar-length
  factor of `1.0`; other bars are normalized as `MaxHP / MaxHP(Mage)`.
- A temporary HP bar is a verification aid, not the intended final HP
  presentation.
- Games may tune resulting HP or use a different health model, but the
  override must be explicit.

### Life, death, revival, and respawn

- Health is clamped at zero and never becomes negative.
- Reaching zero HP transitions an otherwise living character to `DEAD`. A dead
  character cannot receive further normal attack damage and is incapable of
  every gameplay action except the life-state use of the controller `A` action.
- `DEATH_CONFIRMING` begins when the dead character holds `A`. It accumulates
  over `4` seconds while held and decays at `1` second per second while
  released; release returns the state to `DEAD`. During the accumulated
  confirmation time, the body rotates counterclockwise with a deterministic,
  linear angular-velocity ramp from `144°/s` to `1440°/s`. The reverse decay
  uses the same time function.
- A static dead body hides its contour, is scaled to `90%`, and tilts `14°`
  clockwise. `DEATH_CONFIRMING` uses that base pose plus the confirmation
  rotation. `REVIVING` reuses the static dead pose.
- An `ALIVE` character can begin `REVIVING` by holding `A` while its authored
  body/head geometry overlaps a dead body. The server accepts exactly one
  reviver; simultaneous candidates resolve deterministically by lowest
  `PlayerId`. The hold takes `8` seconds and is cancelled by releasing `A`,
  losing overlap, or an action-button-blocking status effect such as
  `STUNNED`. Incoming damage does not cancel revival.
- The dead character can reject a revival at any time: its own `A` input has
  higher priority, cancels `REVIVING` immediately, and enters
  `DEATH_CONFIRMING`.
- Completing `REVIVING` returns the target to `ALIVE` with `80%` maximum HP.
  Completing `DEATH_CONFIRMING` respawns the character at an Ankh with `40%`
  maximum HP. The spawn is selected deterministically within a `4 m` radius
  around the current world-center Ankh anchor.

## The Hammerer and the transforming Hammer

The Hammerer is a recurring World-01 character concept. The Hammer is a
transforming weapon: at rest, it appears as a compact, flat card-like form; in
combat, it unfolds into a powerful volumetric 3D hammer, strikes with full
force, and returns to its compact form after the attack.

### Character and weapon identity

- The Hammerer carries the Hammer weapon authored in PolyTools.
- The Hammerer's `weapon_socket_primary` and the Hammer's `grip_primary` are
  authored attachment frames. They carry orientation as well as position so
  the Hammer rotates around its grip rather than its visual center.
- `head_mid`, `head_left`, and `head_right` are the authored polygonal attack
  Components; no separate AttackRegion is authored.
- The Hammer defines the named ability `HammerStrike`. Its attack Components
  remain separate during hit evaluation so each Component can contribute its
  own gameplay effect.
- `grip_primary` is the carried contact aligned to
  `weapon_socket_primary`. The attack regrips the same hand to the separately
  authored weapon-local `grip_secondary`.
- `attack_point_primary` is placed at the Hammer head's authored center and
  gives the swing and attack Components an explicit alignment reference.
- The released strike's full-length impact radius is derived from
  `grip_secondary` to `attack_point_primary`. No duplicate impact-distance
  balance value is introduced as the World-01 source of truth.
- PolyTools `z_index` values express only ordering among parts of the same
  authored Asset. They are not absolute game-world Z coordinates.

### Aim and carried presentation

- Weapon aim is independent from gaze.
- While a directional aim input is held, weapon aim approaches the selected
  direction at a baseline of `60°/s` (`1°` per 60 Hz tick), without acceleration
  or braking. At a remaining difference of at most `1°`, it clamps to the
  exact target.
- Releasing aim input immediately stops weapon rotation while gaze retains its
  last direction.
- Weapon-aim speed is individually configurable. At an exact `180°` difference
  the last non-zero turn direction is retained; the initial fallback is
  clockwise.
- Outside an attack, the carried Hammer points behind the Hammerer, opposite
  the current weapon-aim direction. This opposite placement is Hammerer-
  specific, not a general weapon rule.
- During Charging, the Hammer follows changing weapon aim, remains held
  opposite it, and appears smaller. Continued aim input may keep rotating the
  weapon.
- Carried and charging presentation is behind the Hammerer. At the released
  swing's overhead apex, the complete Hammer switches in front and remains
  there through impact. It switches behind again after returning to the
  carried pose.

### Charge and attack behavior

- Pressing and quickly releasing the attack input produces the basic strike;
  holding it charges the same strike and releasing executes it.
- Charging caps at `5.0 s` and follows three independent progress curves:
  - From `0.0–2.0 s`, the held contact moves linearly from `grip_primary` to
    `grip_secondary`. Releasing freezes the intermediate grip; reach remains
    capped at `grip_secondary` from `2.0 s` onward.
  - Visible Hammer scale moves linearly from `1.0` to `0.8` during
    `0.0–2.0 s`, then from `0.8` to `0.5` during `2.0–5.0 s`.
  - Visible inward pull begins at `2.0 s` and grows linearly to at most `5%`
    of the secondary-grip-to-attack-point distance at the charge cap.
- The baseline uncharged attack deals `20 HP` (`100%`). Every completed
  `0.5 s` adds `10%`, reaching `200%` (`40 HP`) at `5.0 s`.
- Charge scale and inward pull are presentation-only anticipation. They do not
  change the locked attack grip, authoritative impact length, attack geometry,
  or Component-specific effects.
- The Hammerer retains normal movement speed during Charging, Swing, Embedded
  Impact, and Recovery unless a game explicitly overrides that rule.
- On release, attack direction, charge duration, and current interpolated
  attack grip are frozen.

### Impact, effects, and movement

- `HammerStrike` evaluates each configured attack Component separately against
  target Hurt Components at authoritative impact.
- Multiple targets may be hit by one impact, but overlapping attack Components
  deal damage only once to the same Character.
- Friendly fire is enabled by the World-01 baseline, including possible
  self-hit when authored Components overlap.
- A DASH-invulnerable target receives `0` damage and no additional effect.
- `head_mid`, `head_left`, and `head_right` deal the same calculated
  `HammerStrike` damage. A hit through `head_left` or `head_right` additionally
  applies STUNNED for `4` seconds; multiple side hits apply it only once per
  Character.
- In the baseline character damage model, `body` and `head` together form one
  shared HurtRegion. Multiple Components of one Character still produce one
  hit, while separate Characters can each be hit.
- During Embedded Impact, the Hammerer may move freely inside authored reach.
  At maximum reach, only movement farther away from the planted head is
  blocked; tangential and inward movement remain available.
- The desired reach endpoint is slightly above `shaft_bevel_bottom`.
  PolyTools provides the semantic Weapon Guide `reach_limit_primary`.
- The current authored baseline defines approximately `2.1674 m` maximum
  planted-head reach, `1.3674 m` carried grip-to-head distance, and `1.7674 m`
  full impact radius from the secondary attack grip.

### Transform, depth, and timing

- The procedural swing crosses over the Hammerer's body through depth rather
  than circling around it in the screen plane.
- From a left-carried `180°` pose toward a rightward `0°` impact, it is
  projected like an exact `180°` rotation around the screen-space Y axis. For
  arbitrary attack angles, the depth-rotation axis follows the screen-space
  tangent perpendicular to the locked attack direction.
- PolyTools authors a Component-local projection depth in centimeters. The
  visible Hammer uses it as a closed side silhouette during Swing and Embedded
  Impact, so the midpoint shows authored thickness rather than disappearing.
- Idle, Charging, and Recovery retain the normal flat polygon presentation.
  The authored depth contribution follows `|sin(θ)|`; the original 2D geometry
  follows `|cos(θ)|`.
- During the final `0.15 s` of Embedded, the depth form rolls around the
  planted grip-to-head axis into normal flat-polygon parity while keeping the
  head anchored. Recovery replaces it only after both projections match.
- Release-to-impact duration is `1.15 s`. Authoritative impact occurs only on
  the fixed-tick `Swing -> Embedded` transition; visible animation can never
  produce an earlier hit.
- The Hammer head remains embedded at its world-space impact point for `2.0 s`
  with deterministic presentation-only shake of at most `±1.5 cm`. The shake
  is zero at the held grip, increases toward the head, and eases to zero during
  the final Embedded roll. Simulation state and the authoritative impact point
  never shake.
- Recovery lasts exactly `1.0 s` and returns directly to the current
  behind-the-Hammerer shoulder pose, regripping from the attack contact back to
  `grip_primary` on `weapon_socket_primary`.
- Recovery captures its visible world-space start pose once, keeps its chosen
  half-turn direction continuous across the `180°` seam, and follows the live
  shoulder target without rewriting that start when movement or render
  correction changes it.
- The Hammer uses scale `1.25` at the overhead apex and `1.0` at impact.

The visible Hammer transform is never authoritative collision state. A game
uses the shared semantic attack geometry and authoritative impact rules; its
presentation may use the focused procedural 3D transform and Hammer-specific
shader described above.

## Shared UI and progression direction

### Diegetic and minimal UI

- Conventional camera-lens HUD text, numbers, floating bars, and overlays
  should appear only when absolutely necessary or genuinely useful.
- HP should not default to a health bar. Cracks, injuries, or visible
  degradation are preferred when they communicate state clearly.
- Explicit UI remains appropriate for account, currency, inventory, or meta
  screens when it materially improves usability.

### Persistent and seasonal progression

- Currency: **Magic Coins**, stored persistently on player accounts.
- Magic Coins may improve future starting conditions and/or purchase character
  upgrades during a season.
- Coins found in a match become persistent immediately and remain after early
  death.
- Seasons are envisioned as roughly three months.
- Gameplay-affecting seasonal progression may reset completely each season and
  be rebuilt alongside new content.
- A future **Season History Book** should retain emotional ownership through
  history, statistics, achievements/trophies, victories, and other memories
  after power resets.
- The Season History Book and its extra art are not first-iteration scope.

## Reference art

Current character reference drawings:

- `reference_drawings/5 chars/wizard01.JPG`
- `reference_drawings/5 chars/wizard02.JPG`
- `reference_drawings/5 chars/mage01.JPG`
- `reference_drawings/5 chars/mage02.JPG`
- `reference_drawings/5 chars/sorcerer01.JPG`
- `reference_drawings/5 chars/sorcerer02.JPG`
- `reference_drawings/5 chars/rogue01.JPG`
- `reference_drawings/5 chars/rogue02.JPG`
- `reference_drawings/5 chars/glavier01.JPG`
- `reference_drawings/5 chars/glavier02.JPG`

Related UI/collision concept reference:

- `reference_drawings/design_concepts/hit_region_is_bottom_trapez_and_hp_example_for_UIless_visualization.JPG`

The lower trapezoid/body region in that drawing remains a concept, not a
frozen universal collision specification.

## Iterative design philosophy

The developer follows an iterative philosophy described metaphorically as
“SLAM transferred to game design”: while building, discover detailed design
questions and close them progressively.

- First the coarse structure, then the fine detail; always think iteratively.
- Optimize scope around mechanics with the greatest leverage, where leverage
  means **fun**.
- Simulation/gameplay and presentation/art are separate iteration layers.
- Early implementation should prove only the most important, high-fun-leverage
  World-01 principles.
- Art, shaders, VFX, and sound initially receive only a first sufficient
  iteration.
- Expand and refine both layers season by season.
- Build tunable systems before trying to discover final balance values.
- Do not spend current scope on future problems merely because they can already
  be imagined.
- Creative fallbacks are valid engineering and game-design tools; correctness
  can include detecting a bad state and converting it into a coherent event.

## Open World-01 design questions

- Which World-01 characters and abilities are mandatory in every game, and
  which are shared defaults that a game may omit?
- Which World-01 baseline values are intended to be tunable by every game,
  versus protected identity-defining invariants?
- Should the current Hammerer reach, damage, and timing values remain the
  canonical World-01 defaults, or should some become game-owned tuning data?
- Which additional characters, weapons, status effects, and progression rules
  belong in World Design as they are introduced?
