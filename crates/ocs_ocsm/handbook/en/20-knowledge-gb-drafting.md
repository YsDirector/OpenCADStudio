# 20 Knowledge: GB Drafting Conventions (Skeleton, Grows as Needed)

> English translation of `handbook/20-知识-国标制图画法.md` (the Chinese original is the source of truth).

> This page grows the way "whatever kind of drawing we hit, we add that section". Confirmed rules are written down as hard facts; unconfirmed ones are marked **to be verified**.

## 1. Lines and Linetypes (GB/T 4457.4)

| Line | Linetype | Width | Use | OCSM layer |
| --- | --- | --- | --- | --- |
| Thick continuous line | Continuous | thick (0.5 or 0.7) | visible outlines, thread crests, cutting symbols | `1轮廓实线层` |
| Thin continuous line | Continuous | thin (≈ 1/2 of thick) | dimension/extension lines, hatching, thread roots, transition lines | `2细线层` / `7标注层` |
| Thin centre line (dash-dot) | CENTER2 | thin | axes, symmetry lines | `3中心线层` |
| Thin dashed line | DASHED2 | thin | invisible outlines | `4虚线层` |
| Thin phantom line (double-dash) | DIVIDE2 | thin | imaginary projections, extreme positions of moving parts | `9双点划线层` |
| Wavy line | Continuous (hand-drawn) | thin | break boundaries, local sections | `2细线层` |

- Thick:thin = 2:1; the same class of line must have the same width throughout one drawing.
- The two ends of a centre line must **overshoot the contour by 2–5 mm**; draw a cross at the centre of a circle (for ≤Ø12 a thin continuous-line cross is enough).

## 2. Sheet Formats and Scales (GB/T 14689)

- Basic formats: A0 841×1189, A1 594×841, A2 420×594, A3 **297×420**, A4 210×297.
- Margins: 25 for the binding edge, 5 for the others (for A4 portrait the 25 binding margin is on the left).
- Title block: the title block of this plugin's frame `a3_landscape_att.dwg` is **185×40** (placed bottom right) + an additional vertical column on the left.
- Scale series: 1:1, 1:2, 1:5, 1:10, 1:2×10ⁿ…; enlargement 2:1, 5:1….
  **How OCSM does it**: the geometry is always 1:1; the frame carries the scale, while `OCSM_GB_xN` is created by `OCSMDIMGULIDE` / `OCSMDIM2GB` on demand at the actual annotation anchor (see `02-sheet-and-frame.md`).

## 3. Thread Drawing (GB/T 4459.1)

- External thread (front view): **the crest is a thick continuous line** (major diameter), **the root is a thin continuous line** (minor diameter ≈0.85d);
  the end chamfer is 45° (`0.075d`), and the thin root line is drawn up to the chamfer and stops there; the chamfer at the bar end is closed off with a thin line.
- Internal thread (section view): thin line at the crest, thick line at the root (the opposite of an external thread).
- The screwed-together part is drawn as an **external thread**; for a blind hole use a dashed line for the drilled depth and a thin continuous line for the thread run-out.
- OCSM has checked this against the templates one by one (`hex_bolt_b_full` / `nut_c41` / `nut_61721` etc.) —
  **do not draw these by hand**, use `OCSMPART`/`OCSMJOINT`.

## 4. Sections and Cut Views (GB/T 4458.6 / 17452)

- Cutting-plane symbol: a thick short stroke (length 5–10) + an arrow giving the projection direction + letters (uppercase, e.g. `A—A`).
- Hatching: 45° to the main contour, same direction and spacing within one part; adjacent parts must differ in spacing/direction (this plugin uses two ANSI31 patches).
- **Material distinction (GB/T 4457.5)**: metal uses **ANSI31** (45°, one direction); rubber/non-metal uses **ANSI37** (45°+135°, cross-hatch in both directions).
  Example: a rotary shaft lip seal, FB type, rubber-covered frame with an auxiliary lip — metal frame as two ANSI31 patches, lip rubber as two ANSI37 patches (measured from the template).
- **Hatch direction convention for four patches**: in the section view of a symmetric shaft-type part the patches upper-outer/upper-inner/lower-outer/lower-inner = 0°/90°/270°/180° (this is how the GB/T 288 spherical roller bearing template does it);
  a nut / round nut section uses two patches = 0°/270°.
- **Implementation pitfalls (already absorbed by this repo)**: ① the TSV from `ref_dump` **does not fill in the layer column of HATCH** (it looks like an empty layer), so to see a hatch's layer/pattern/scale
  you must read the DXF `HATCH` with `ezdxf` (`dxf.layer` / `pattern_name` / `pattern_angle` / `pattern_scale` / `paths[].edges`);
  ② `pattern_scale` differs between templates (0.25/1.0/1.5 all show up), the line spacing = **3.175 mm × scale**, and the pattern definition's `offset`
  must be multiplied by scale as well (handled by `partgen_kit::hatch_ansi31_edges` / `hatch_ansi37_edges`);
  ③ a hatch boundary **may contain arc edges** (raceway arcs, lip arcs) and must not be approximated by polylines (`HatchEdge::Arc` supports them).
- Common section views: full section, half section, local section, aligned (revolved) section, offset (stepped) section (`SECTION` leader lines support polylines = aligned/offset sections).
- Hatching of metal parts is a thin continuous line with 2–4 mm spacing (adjust to the sheet format).

### 4.1 Local (Broken-out) Sections by Complementary Trimming (OCSM / FreeCAD)

> Confirmed by the user on 2026-10-25 using the four-stage example `~/桌面/OCSM/局部剖示例.dxf`.
> Applies to OCSM shaft-generator output and to 2D views generated/exported from a FreeCAD 3D model.

**Principle: a local section is not a layer of hatch placed over a normal view. Trim the normal and full-section views into complementary parts, then combine them.**

#### Prerequisites

- Both views must represent **the same part/model and dimensions, viewing direction, projection convention, scale and units**. Preserve the originals and trim working copies.
- For OCSM, use the same complete shaft expression to generate `VIEW 常规` and `VIEW 剖视`; do not redraw the entire shaft outline by hand.
- For FreeCAD 3D-to-2D work, prepare the corresponding normal projection and a **genuine section derived from the model**. Its cutting plane/depth must reveal the intended local internal features; do not combine an unrelated orientation or arbitrary cutting depth.

#### Four stages, left to right

| Stage | Normal view | Full-section view / combined result |
| --- | --- | --- |
| 1. Prepare both views | Retain the complete unsectioned appearance | Prepare the matching full section, including actual cut contours and correct hatching |
| 2. Define the same local region R | Draw the break-boundary curves required by the reference | **Copy the same curves using corresponding datums**; do not independently sketch two sets that merely look alike |
| 3. Trim complementary parts | **Remove the lines inside R** that the section replaces; keep the unsectioned part outside R | **Keep only the cut contours, internal-feature lines and hatch inside R**; remove the content outside R |
| 4. Align and combine | Align both parts precisely using the same axis, end face or datum point | Form one local-section view; retain one copy of each break boundary and remove coincident lines/duplicate centerlines caused by joining views of the same part |

In short: **result = normal view outside R + full-section view inside R + one copy of the break boundary.**
R may contain one or several areas bounded by free curves; it is not restricted to an axial rectangular window. The upper/lower patches in this example do not require all parts to use symmetric local sections.

#### Trimming and acceptance checks

- **Process contours, internal-feature lines and HATCH boundaries together**, not just hatch strokes. Compute the break-line/contour intersections and share their exact endpoints; retain true circles/arcs rather than approximating them with polylines.
- Hatch boundaries must enclose the **trimmed material regions**. Do not fill holes or slots. Preserve the original section's prescribed unhatched features, such as gear teeth.
- Leave no hatch or section-only internal lines in unsectioned areas, and no superseded normal-view lines in the cut region. The combined result must have no missing lines, gaps, duplicate outlines caused by joining views of the same part or duplicate break boundaries. This does not require deleting intentional coincident outlines retained in different assembly-part blocks for independent reuse; check assembly visibility and downstream export as well (chapter `16` §6a).
- Keep hatch direction and spacing consistent within one part. Preserve layers and ByLayer properties: break boundaries on `2细线层`, HATCH on `5剖面线层`.
- Verify **datum coordinates/intersections, detail screenshots and hatch-loop closure**; alignment by eye is insufficient. Apply the drawing audit and save/reload checks required by the practical tracing rules in chapter 18.
- **Do not** substitute a full section for a local section in the reference, blanket the normal view with HATCH, or use white masking to pretend that complementary trimming was performed.

This is a general drafting workflow, **not a claim that a one-command arbitrary-curve local-section feature exists**. Use only trimming commands or automation interfaces verified for the current host.
The worked example and source-file notes are in [chapter 18 (Chinese)](../18-抄图实战口径.md), §9.

## 5. Dimension Annotation Elements (GB/T 4458.4)

- Extension lines are led out from the contour, leaving a gap of 2–3 to the contour; the spacing between dimension lines is ≥ 5 (small dimensions may be moved outside).
- Text: height 5 (OCSM_GB style), **readable from the top**; vertical dimension text reads from the left.
- Diameter `⌀`, radius `R`, sphere `S`, thickness `t`, 45° chamfer `C2`; dimension chamfers as uniformly as possible throughout one drawing.
- When the text is moved outside the dimension line: the dimension line is extended along the axis (already implemented, `DIMTMOVE`).

## 6. Tolerances and Fits (GB/T 1800 series)

- Prefer a **code** (`⌀30H7`); give limit deviations only when necessary (`⌀30 +0.021/0`).
- Hole basis `H` is the common choice; holes mating with standard parts prefer `H7`.
- For the actual tables: see `22-knowledge-tolerances-fits.md` (the plugin has the ISO 286 tables built in).

## 7. Surface Roughness and Geometric Tolerance (GB/T 131 / 1182)

- Roughness: basic symbol + value (`Ra`); for material removal draw the small triangle; write it on the contour line/dimension line with the triangle tip pointing at the surface.
- Roughness rotation follows “tip on the surface, phase follows the local contour, choose the side by clearance”; do not use 0° universally. A horizontal sectioned surface usually chooses 0° or 180°; an end-view bore normally puts the symbol body in the empty bore. Sloped lines and arcs follow their local tangent/surface-facing relationship. Rotate the whole INSERT/block and keep the text upright; repeated callouts for one surface may have different orientations but must keep the same value and meaning. Parallel opposite end faces use opposite rotations according to their outward normals (vertical left/right faces may use +90°/-90°); C2's circle-in-V means no material removal, and blank A/A′ fields do not justify inventing a value.
- Feature-control frame: as many cells as needed (symbol / tolerance value / datum); the leader line comes out of the frame's edge.
- Datum symbol: two GB drawing conventions (1996 / 2008); keep one style per project.

## 8. Assembly Drawings and BOM (GB/T 4458.2 / 10609.1)

- Assembly drawing order: draw the main body first → then fit the standard parts (`OCSMJOINT`) → dimension the fits/key sizes → item balloons → BOM.
- Item balloons: the leader comes out of the part, the balloons sit outside the sheet border or in a neat row, and the item numbers correspond one-to-one with the BOM.
- BOM: fill it from the bottom up (item 1 at the very bottom), with the header at the very top of the whole table (see `10-bom.md`).
- In a threaded-joint section view: the hatching of the clamped parts points the opposite way; bolts/nuts are drawn unsectioned (this plugin already generates them this way).

## To Be Added (write it when we hit it)

- [ ] Standardised drawing of keys/pins/springs/gears (gears: pitch circle thin dash-dot, tip thick, root thin; how to draw the meshing zone)
- [ ] Simplified drawing of centre holes/chamfers/relief grooves
- [ ] The complete weld-symbol annotation system (GB/T 324)
- [ ] Development drawings / sheet metal and bend annotation
- [ ] Springs, rolling bearings (simplified/standardised drawing) and seals
