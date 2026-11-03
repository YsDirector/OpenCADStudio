# 25 Knowledge: Cast Gearbox Housing Design (Quick Reference)

> English translation of `handbook/25-知识-铸造箱体设计.md` (the Chinese original is the source of truth).

> For the "the housing is cast" route: how to set the outline, how bosses grow out of the wall, how to draw
> the cover mounting face, in which order to cut features, and how to verify. Everything below comes from the
> actual rework log of a two-stage cylindrical gear reducer (V4 working drafts 17–44).

## 1. Welded Language vs Cast Language

| Feature | Welded housing | Cast housing |
| --- | --- | --- |
| Outline | rectangular plates + sections, straight lines | follows the gear / bearing outer circles: **concentric arcs + external common tangents**, one continuous curve |
| Stiffening | welded ribs, angles | solid side wall, **lightening windows milled out**; the remaining metal between windows *is* the rib |
| Bosses | welded / bolted on | **grow out of the wall**, continuous with wall and seat; never an independent cylinder or block glued on |
| Base | steel frame | **horizontal base**, bottom face must be flat (constant Z) |

Rule of thumb: rectangular cavity + square flange = welded language. To draw a cast housing, the outline must
first be replaced by an arc envelope.

## 2. Overall Outline (two-stage gear reducer)

1. **Upper half (Z>0)**: take the **convex hull** of the addendum circles (G1/G2/G3/G4) and the bearing boss
   circles (common external tangents between neighbours); offset the cavity inward by the wall thickness and
   the outline outward by the same. This project: wall 10, addendum +6 clearance, boss `R = D/2 + 16`.
2. **Lower half (Z<0)**: do **not** continue the arc envelope — the bottom would be curved and unmountable.
   Take the maximum width at that height and **extend it vertically** down to the outer bottom (here Z=−150),
   oil sump floor at Z=−140, bottom face horizontal.
3. **Split flange**: outline expanded in X–Z, but **in Y only out to the cover mounting face**;
   otherwise the flange covers the cover seat (here 55 did, 30 did not).
4. **Ribs**: one per bearing seat, centred on the axis, upper end embossed into the boss circle, lower end
   reaching the base top face; width 24, protrusion 8 (nominal).

## 3. Cover Mounting Face (Seat Pad) — the Most Reworked Feature

### 3.1 Shape: a curved-edge trapezoid whose two legs are the adjacent boss circles

```
        +-------- seat face (horizontal, Z=H) --------+
       /                                            \
  leg = arc of seat I/II outer circle      leg = arc of seat II/III outer circle
     /                                                \
 ---+------ split plane Z=0 (gap between the two seats) ------+---
```

- The two legs **are** the outer-circle arcs of the adjacent bearing seats (cover flanges), `R = D/2 + 16`;
- The pad's side surface coincides with the boss cylinder, so after fusing they are the *same* cylindrical
  face: a "block tower", a free-standing vertical wall, a 1 mm sliver or a sharp valley **cannot** appear;
- The seat width follows from the arcs, it is not an arbitrary constant;
- The bottom edge lies on the split plane Z=0; its width is the gap between the two seats at that height.

### 3.2 Why not "vertical wall + big root fillet"

A vertical wall stands proud of the boss crown and always reads as a tower; a root fillet applied afterwards
to a fused body fails outright (`Shape.makeFillet` → `BRep_API: command not done`). Either draw the fillet
into the profile, or drop the vertical wall — the curved-edge trapezoid does the latter.

### 3.3 Seat height H (wrench space)

Criterion (JB/ZQ 4005—1997, M12: `A=42` open-end, `A2=30` box/socket, taken as the *radial* distance from the
bolt axis to the obstacle):

```
wrench lower face = seat face + washer thickness (2.5 here)
require:  d − √(rb² − (H+2.5)²) ≥ A
```

- `d` = distance between bolt axis and the adjacent seat axis, `rb` = adjacent seat outer radius;
- **H must stay below both adjacent seat radii**, otherwise the arcs never reach the seat crowns and the
  curved legs break;
- Here: X=62 (seat II rb61, d71.75) → H≥50.75, take **51**; X=201 (seat III rb71, d78.25) → H≥58.55, take **59**;
- Box/socket only: ~45/50; socket only (OD 22–24): ~23. **Fix the tool first, then compute H.**

## 4. Order: Mounting Face First, Holes After

This was the last and most typical root cause of rework in this project.

**Correct order**

1. `Sketch` (curved-edge trapezoid) + `Pad` — build the **whole cover mounting face** first;
2. `Pocket` the split-bolt holes (Ø13.5 here);
3. `Pocket` the bearing bore set (seat bore ØD, shoulder, through hole);
4. `Pocket` the cover screw holes (Ø6.8×14, drilled in from the mounting face);
5. Remove "legacy" features of the old design (e.g. an old Ø30 spot-face recess — a flat seat no longer needs it).

**Counter-example (this really happened)**: the flat mounting face plus Ø30 spot-face recess existed first, and
the face was later raised. The new pad is only 13.5 mm wide over Z=10…22 (the two boss circles nearly touch), so
it cannot cover the wings of the Ø30 recess and the section shows two internal voids
(X 190…193 / 209…211, Z 10…21) — literally "two holes" in the drawing.

**Corollary**: whenever the mounting-face shape changes, every hole / recess / step made against the old face
must be redone or filled, otherwise the narrower the new face, the more the old features poke through.

## 5. Bosses, Draft, Fillets (Casting Practice)

- Give pad / boss side faces **1°–2° draft** (with a curved-edge trapezoid the cylindrical faces already follow
  the moulding direction);
- Root fillets and window edge breaks (R2–R3) are **drawn into the profile/sketch**, never added afterwards;
- Prefer uniform wall thickness; large solid blocks (like the seat pad) need a separate hot-spot /
  solidification review — "it can be drawn" is not a casting approval;
- Lightening windows: better fewer and with thick edges than any strip thinner than ~8 mm — a milling cutter
  cannot enter it and the casting would scab.

## 6. Verification (Geometry Level, Automatable)

| Check | How | Pass criterion |
| --- | --- | --- |
| Single solid | `len(shape.Solids)==1 and shape.isValid()` | both halves |
| Void scan | `isInside` on a 1 mm grid over the pad mid-plane, excluding the intended holes (bolt / bearing / cover) | 0 unexpected voids |
| Seat flatness | point below the washer ring solid, point above void | all true |
| No new interference | `common()` with covers / bearings / shafts, compare before vs after | no new positive volume |
| Everything else untouched | per-part bounding box / volume / face-count signature | identical |
| Fasteners | zero interference with the housing, washer support ratio = 1.0 | all true |

> An empty `common()` result is "undetermined", **not** zero interference; re-judge it or record it as open.

## 7. Modelling Expression: Keep the Process in the File

Use **PartDesign** (`Body ▸ Sketch ▸ Pad`) instead of assigning a Shape to a `Part::Feature`:

- one `Body` per seat pad, named by column/position (`座台_上_201_前`), the sketch can be re-dimensioned and
  recomputed;
- express solid walls plus windows with `Pad` / `Pocket` too, so window positions follow sketch parameters;
- at delivery, a *visible modelling process* is worth more than one fused final shape: others can change and
  review it.

## 8. Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| "Block tower" on the side wall | vertical wall + fillet applied afterwards | use the curved-edge trapezoid (legs = seat outer arcs) |
| 1 mm sliver between pad and boss | pad extends 1 mm beyond the wall face / boss circle | align strictly with the wall face and the boss circle, do not "leave a little extra" |
| Sharp valley (width ~1 mm) | 45° chamfer intersecting the boss circle directly | add a tangent arc, or switch to the curved-edge trapezoid |
| Two holes in the section view | old recess / old hole not redone with the new face | follow the order in §4, fill the legacy feature |
| Cover will not go in | flange extends too far in Y and covers the mounting face | stop the flange at the seat face in Y |
| 4 mm strip at a window edge | window sized only by "avoidance" | keep 8–12 mm margins so a cutter can enter |
