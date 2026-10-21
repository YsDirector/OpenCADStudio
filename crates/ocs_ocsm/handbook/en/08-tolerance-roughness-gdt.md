# 08 Tolerance, Fits, Roughness and GD&T Symbols

> English translation of `handbook/08-公差粗糙度与GDT.md` (the Chinese original is the source of truth).

> Dimensional tolerances, fit designations, surface roughness, geometric tolerances / datums / engineering symbols — all of them are done in the **annotation settings window** (opened with `OCSMDIMGULIDE`).

## 1. Dimensional Tolerance (`TOLERANCE`)

Two modes (switched in the settings window):

| Mode | How it is written | Use |
| --- | --- | --- |
| **Code mode** | `⌀30H7`, `⌀30k6` | fit dimensions that mate with something |
| **Limit-deviation mode** | `⌀30 +0.021/0` (upper/lower deviations) | one-off dimensions, given per the process |

- **Fit table**: the plugin has ISO 286's 4 tables built in (hole/shaft × common tolerance bands); the deviations are looked up automatically when you pick a code;
  you do not have to leaf through a handbook for the limit deviations of common fits.
- Diameter/radius tolerance: drawn as **stacked MTEXT inside an anonymous block** (upper deviation above, lower deviation below), per the GB convention.
- When only a "text override" is given, the text is emitted as you wrote it (the hand-made part is preserved).

## 2. Geometric Tolerance (`GD&T`) and Datum (`DATUM`)

- **Feature-control frame**: the leader = 1 PLINE locating the frame's position; the content = symbol (straightness/roundness/parallelism/position…) + tolerance value +
  datum letter, picked in the window (there is a **GDT symbol font**, no typing characters to fake them).
- **Datum symbol**: two GB drawing conventions (1996 / 2008); pick per the project's requirement.
- Symbol-type output lands on `8符号标注层` (anonymous block + INSERT).

### General design tasks: semantic pre-checks for GD&T input (not only redraw work)

This section is a design-input pre-check summary based on **GB/T 1182-2018** (current Chinese edition, modified adoption of ISO 1101:2017); it is not a replacement for the standard text. Before geometry or annotations are generated, GD&T input must close this chain:

```text
measured-feature type → tolerance characteristic → tolerance-zone form → datum system / degrees of freedom → TED/basic dimensions and inspection direction
```

If the chain is incomplete or the roles conflict, stop generation and report `INPUT_CONTRADICTION`, identifying the missing or incompatible input. The fact that the OCSM dialog has a datum cell does not make an arbitrary datum letter a valid design.

#### 1. Feature-characteristic / datum semantic quick reference

| Characteristic | Datum cell | Semantic pre-check | Typical rejection |
| --- | --- | --- | --- |
| Straightness, flatness, circularity, cylindricity | Normally none | These are form controls; identify whether the measured element is a surface line, an axis/derived median line, a plane, a circular section or a cylindrical surface | A pure form requirement is made dependent on A/B/C; measured-element type does not match the zone. Axis straightness may still use an applicable `⌀` or `Ⓜ/Ⓛ` modifier without gaining a datum. |
| Line profile, surface profile | Optional | Without a datum, mainly controls the profile's own form; with a datum, it may also control orientation and/or location within the datum system's constrained degrees of freedom; a theoretical/nominal profile is required | A datum is supplied without an interpretable nominal profile, datum system or nominal geometry; “with datum” is treated as automatically locking every direction and location. |
| Parallelism, perpendicularity | Required | These are orientation controls. The datum may be a datum plane, axis/centre line, centre plane or an appropriate combination, depending on the measured element, zone and intended direction | The datum cannot supply the required direction; for example, an axis is required but only an isolated end face is supplied. |
| Angularity | Required | The datum and theoretical exact angle define the direction; use a TED/basic angle, not an ordinary toleranced angle | Datum or theoretical angle is missing, or a 0°/90° relation is called angularity without the required angle semantics. |
| Position | Optional | When datums are used, check datum order, the actually required degrees of freedom, and the theoretical location defined by TED/basic dimensions. Without datums, do not silently interpret it as absolute placement in the part's global coordinate system | No theoretical location/basic dimensions; datums do not support the functional or inspection requirement but A/B/C are invented; the datum system is mechanically required to lock all six degrees of freedom. |
| Coaxiality | Required | The datum must provide a unique datum axis or equivalent centre line | A single plane/end face is used instead of a datum axis; the source of the axis is unspecified. |
| Symmetry | Required | The datum must establish the specified symmetry relation with the measured derived median plane, centre line or axis; choose datum plane, centre plane or datum axis/line according to the measured element and zone | An arbitrary plane/axis is silently treated as the required centre relation; a side face is substituted where a median relation is required. |
| Circular run-out, total run-out | Required | A cylinder, cone, common/compound datum or other explicit system must establish a unique rotation axis. Circular run-out is checked at each cross-section; total run-out covers the whole measured surface | A single annular end face/plane is used as the unique axis datum; the source of the datum axis is not defined. |

“Required” means that the standard semantics of the characteristic needs a datum reference; it does not mean that only one physical surface type is possible. “Optional” does not waive the need for a theoretical profile or theoretical location.

#### 2. Datum-role compatibility matrix

| Required datum role | Common valid sources | Not sufficient by itself |
| --- | --- | --- |
| Rotation axis / centre line | External cylinder, internal bore cylinder, conical rotational feature, an explicit centre element, a common datum or a compound system that derives an axis | A single plane or end face; it supplies direction but not a unique axis location |
| Datum direction | The normal of a datum plane, the direction of a datum axis/centre line, or an appropriate combination | A point without a direction definition |
| Datum plane / centre plane | A physical plane, an explicitly derived median/centre plane, or a functional datum system | A single side face silently substituted when a centre relation is required |
| Locating datum system | Ordered A\|B\|C (or a common datum) plus TED/basic dimensions; constrain only the degrees of freedom required by function and inspection | Filling empty frame cells automatically, or treating one A as a universal coordinate system |

Therefore, in this drawing, if the upstream input declares A to be the right annular end face while both frames are `circular run-out | 0.025 | A`, reject the input before modelling: the end face cannot establish a unique rotation axis. The Φ42 external cylinder, an internal bore or another axis-producing feature may be a candidate, but the run-out characteristic alone cannot select the design intent among multiple valid candidates.

#### 3. Modifiers also require semantic validation

- `⌀` specifies the applicable circular/cylindrical tolerance zone; it is not a diameter decoration that may be added to any value.
- `Ⓜ/Ⓛ` (maximum/minimum material) generally requires the measured or datum feature to be a standard-defined feature of size; putting it after the tolerance value and putting it after a datum letter are different semantics.
- `Ⓟ` (projected tolerance zone) is only for standard-permitted features with an assembly direction and projection-length requirement; the projection length must be given, and it cannot repair a missing datum or theoretical location.
- `Ⓢ` (independency) and other modifiers cannot be combined arbitrarily. Check the measured-element type, characteristic, zone form and adopted standard version; do not silently import ASME default rules into a GB/T 1182 / ISO GPS workflow.

#### 4. Closed loop for general design tasks

1. Normalize input into `target_feature`, measured-element type, `datum_role`, allowed/forbidden datum roles, TED/basic dimensions, inspection direction and any required `guide_direction`.
2. Check compatibility among characteristic, measured element, datum system and modifiers, including the degrees of freedom actually constrained by the datum system.
3. On conflict, stop and return `INPUT_CONTRADICTION`; request functional, assembly or inspection information instead of selecting an axis or inventing a datum by convention.
4. Only after the check passes, generate geometry, leaders and the feature-control frame; then read back `OCSM_EDIT` `p1/p2`, the datum attachment point and FCF parameters.
5. Run the same semantic matrix against the generated result before structural audit, visual inspection and `save_verified`. A screenshot cannot replace a datum-DOF or tolerance-zone check.

This rule applies to from-scratch design, modification and redraw work. The source-image attachment semantics in `18-抄图实战口径.md` are additional constraints, not a replacement for this general rule.

## 3. Surface Roughness (`OCSMRGH`)

```
Command: OCSMRGH
Flow: interactively pick the symbol insertion point → open the "粗糙度配置" (roughness settings) window → pick the form + fill in the Ra value → "确定" (OK)
Output: anonymous block (with ATTDEF text) + INSERT (onto 8符号标注层)
```

- Forms: basic symbol / material removal / no material removal / with annotation (e.g. `Ra 3.2`, `Ra 1.6`, `Rz`…). Machined surfaces normally use **material removal** (`shape:"remove"`); only follow another form when the source drawing explicitly shows it.
- Roughness is **not a leader-annotation type** (`GuideType` has no `Rgh`, and the `OCSMDIMGULIDE` window has no roughness tab); always use the standalone `OCSMRGH` command (pick the insertion point + settings window).
- **Position, rotation and clearance are acceptance rules**: the V-tip must snap to the target contour or a legal extension, and the whole symbol must rotate with that local geometry; never use a global 0° default or rotate text alone. For a horizontal sectioned surface, 0° and 180° share the same baseline but put the symbol body on opposite sides; choose the side that leaves the body/value in clear space. For sloped lines, arcs and an end view of a bore, follow the local tangent/surface-facing relationship; for an internal bore, the body normally occupies the empty bore side.
- **Rotate the whole INSERT/block** (symbol, value and ATTDEF layout). When the block is flipped through the 90°–270° range, adjust local ATTDEF alignment/text rotation so the final text remains upright/readable.
- **Repeated callouts across views**: if the same machined surface is marked in a section and an end view, the values and engineering meaning are the same; each INSERT may use a different 0°/180° or local angle to fit its view. Different orientations do not imply different roughness requirements.
- **Opposite-end-face normal rule**: when two parallel opposite end faces are marked, choose opposite rotations according to their opposite outward normals; for example, vertical left/right faces may use `+90°`/`-90°`. The V-tip stays on each face and the body points into material-free/clear space; do not force both faces to use one angle.
- **C2 form**: a circle inside the V means “obtained without material removal”; when `A/A′` are blank, do not invent a numeric roughness value.

## 4. AI-Side Recipe

```jsonc
// roughness: pick the insertion point first (enters interactive mode) → the window parameters are submitted by a human or over HTTP
{ "op": "run", "request_id": "r1", "document_id": 1, "revision": 40, "cmd": "OCSMRGH" }
POST /api/rough_apply   { "x": 30, "y": 12, "shape": "remove", "value": "3.2", … }   // creates an anonymous block + INSERT
// tolerance/geometric tolerance: leader line + the OCSMDIMGULIDE settings window (or POST /api/apply_refresh with parameters)
{ "op": "run", "request_id": "t1", "document_id": 1, "revision": 41, "cmd": "LINE 0,0 30,0" }  // dimension leader line
{ "op": "run", "request_id": "t2", "document_id": 1, "revision": 41, "cmd": "OCSMDIMGULIDE" }           // pick the tolerance in the window
```

## 5. Pitfalls

1. **A tolerance attaches to a dimension**: `TOLERANCE` adds deviations to a **dimension annotation**, it is not a standalone symbol → there must be a dimension first (the host-native `QDIM` or a leader dimension from `OCSMDIMGULIDE`).
2. **Pick the right basis system for a fit code**: `H7/k6` is hole basis (common), `K7/h6` is shaft basis; pick the wrong one and the deviations come out pointing the other way.
3. **Do not type GDT symbols by hand**: the font carries dedicated symbols; only picking them in the settings window guarantees the right shape and avoids rare/unsupported characters.
4. **A roughness value omits "Ra"**: the default is `Ra`; for `Rz` you must change the symbol type in the window.
5. **Position, rotation and clearance are hard acceptance rules**: the roughness tip must snap to the target contour or legal extension, and the symbol must remain at least 3 mm from non-target contours, dimension lines/text, GDT frames, centerlines and neighboring symbols. A screenshot alone is not a clearance calculation.

### Machine-checkable visual-clearance gate

After every addition, deletion or move of a roughness symbol, dimension, text, GD&T frame or centerline, run a semantic clearance audit. The DXF structural audit returned by `save_verified` is not a substitute. If OCS has no dedicated clearance command, run a deterministic script over `query`/record-exported entity data and emit the clearance table; a script error, unknown value or unexpanded block is FAIL, never a visual pass.

1. Read each roughness `INSERT` block name, insertion point, rotation, scale and attributes. Transform all visible block members, `ATTDEF/ATTRIB` text and leaders into world coordinates. Expand the resulting world envelope by 3 mm and query nearby contours, dimensions, dimension text, centerlines, GD&T frames and symbols. Do not use only the INSERT point, V-tip or screen pixels.
2. Decompose dimensions into dimension line, extension lines, arrows and dimension text. Compute the XY model-space minimum distance `min_distance_mm` for every relevant obstacle and retain the source handle/type.
3. PASS only when every non-target obstacle has `min_distance_mm >= 3.000`. Tip contact with the specified target contour or legal extension is the default permitted `<3 mm` contact. Any other shared endpoint must state its engineering meaning. Emit at least: `rough_handle | obstacle_handle/type | min_distance_mm | contact_kind | PASS/FAIL`. “About 3 mm”, an unexpanded block or an unknown distance is FAIL.
4. If the roughness tip, rotation and attributes are correct, move the conflicting dimension offset first. For host-native dimensions, move definition-point grip 2 while preserving the first/second points and face attachment; do not MOVE the whole DIMENSION or its extension origins. For `OCSMDIM2GB`, edit the `OCSM_EDIT` guide points and call `apply_refresh`. Change the roughness INSERT only when its attachment/orientation is wrong.
5. Re-query, recompute the table and capture a screenshot after every repair. Only after all non-target rows PASS may `save_verified` be used. Delivery requires both semantic-clearance and structural audits.

6. **The roughness window is also "pinned to a drawing"** (its URL carries `tab=`): the insertion only lands in the drawing that opened it; once that drawing is closed,
   the old window errors out immediately. See `06-leader-annotation.md` §5.7 for the rule.

## 6. Verification Checklist

- [ ] Tolerance text is stacked per GB (upper deviation on top, lower deviation below, decimal points aligned).
- [ ] Symbol layer = `8符号标注层`; the block is an anonymous block `*X{n}`.
- [ ] The tip of the roughness symbol's triangle points at the material surface.
- [ ] Every roughness INSERT is expanded into world coordinates, including block geometry, ATTDEF/ATTRIB text and leaders.
- [ ] A `rough_handle | obstacle_handle/type | min_distance_mm | contact_kind | PASS/FAIL` table is emitted; every non-target row has `min_distance_mm >= 3.000`.
- [ ] Target-tip contacts and other shared-endpoint exceptions state their engineering meaning; no "about 3 mm", screen-pixel or unexpanded-block conclusion is used.
- [ ] Both the semantic-clearance audit and the DXF structural audit pass before `save_verified`.
