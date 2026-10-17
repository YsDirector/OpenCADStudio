# 02 Sheet Format and Frame

> English translation of `handbook/02-图幅与图框.md` (the Chinese original is the source of truth).

> Creating a sheet format, swapping the frame, printing at scale.

## 1. Two-Step Flow

```
Step 1: OCSMFRAMEINIT (without arguments) → opens the "图框选择窗口" (frame selection window)
Step 2: OCSMFRAMEINSERT  → after choosing a scale, click the insertion point in the drawing to drop the frame
```

## 1. Two Ways to Use It

### A. Human side: two steps (selection window)

```
Step 1: OCSMFRAMEINIT (without arguments) → opens the "图框选择窗口" (frame selection window)
Step 2: OCSMFRAMEINSERT  → after choosing a scale, click the insertion point in the drawing to drop the frame
```

### B. AI/automation side: one line, direct insert (**the selection window does not open**)

```
OCSMFRAMEINIT <name> [scale] [at x,y] [rot deg]      (OCSMFRAMEINSERT uses the same syntax)
```

| Form | Effect |
| --- | --- |
| `OCSMFRAMEINIT a3_landscape_att 1:2 at 0,0` | **Fully automatic**: inserts an `a3_landscape_att` at 1:2 directly, base point (0,0) |
| `OCSMFRAMEINIT a3_landscape_att 1:2` | Frame + scale fixed → enters **cursor-follow** placement (one click to settle) |
| `OCSMFRAMEINIT a3_landscape_att` | Frame + 1:1 + cursor follow |
| `OCSMFRAMEINIT 1:2 at 0,0` | **The name may be omitted** (when `frame/` contains only one frame) |
| `OCSMFRAMEINIT a3_landscape_att 1:2 at 0,0 rot 90` | also rotates by 90° |

* **Name**: the dwg file name in `frame/` (the `.dwg` may be omitted, matching is case-insensitive, and a fragment such as `landscape` also works).
  An exact name wins; a fragment that matches several → an error listing the available frames.
* **Scale**: `1:2`, `1：2` (full-width colon), `1/2`, or just `2` (= reduced 2×); for enlargement write `2:1` or `0.5`.
  Same rule as the selection window: **two positive integers with one of them 1**.
* **Coordinates**: `at x,y` (`at` may be omitted, write `x,y` directly; `x,y,z` is also accepted).
* Errors always carry the usage hint (emitted in Chinese):

  ```
  OCSMFRAMEINIT：比例必须是两个正整数且其一为 1（例 1:2 / 2:1 / 1:1）。
  OCSMFRAMEINIT：没有叫「a2」的图框。现有：a3_landscape_att / a4_portrait。
  OCSMFRAMEINIT：认不出的参数「乱写」。用法：OCSMFRAMEINIT <名称> [比例] [at x,y] [rot 度]（不带参数 = 打开选择窗口）
  ```

  As emitted, these read: "OCSMFRAMEINIT: scale must be two positive integers, one of which is 1 (e.g. 1:2 / 2:1 / 1:1).",
  "OCSMFRAMEINIT: no frame named 「a2」. Available: a3_landscape / a4_portrait.",
  "OCSMFRAMEINIT: unrecognized argument 「乱写」. Usage: OCSMFRAMEINIT <名称> [比例] [at x,y] [rot 度] (no arguments = open the selection window)".
* In automation it is recommended to **`cancel` first, then send the command** (to avoid a leftover interactive state):

```jsonc
{ "op": "run", "request_id": "f1", "document_id": 1, "revision": 3,
  "cmd": "OCSMFRAMEINIT a3_landscape_att 1:2 at 0,0" }
```

### Step 1 `OCSMFRAMEINIT` (human side): pick the frame

- It lists top-level `*.dwg` files in the **plugin directory**'s `frame/` (sorted by file name; subdirectories are not scanned); just click one in the window.
  - Actual path: `~/.config/OpenCADStudio/plugins/opencad.ocsm/frame/`
  - Intermediate/backup frames belong in a separate subdirectory such as `frame/_intermediate/`, not beside selectable templates; `OCSMFRAMEINIT` scans only top-level DWGs.
  - Currently available: `a3_landscape_att.dwg` (A3 landscape, with a **185×40 title block** + a vertical additional column on the left); the actual directory content is authoritative.
- **The frame DWGs are user-supplied data and are not part of the plugin repository** (there is no `frame/`
  directory under `crates/ocs_ocsm/`):
  - On the maintainer machine the sample frames live in `~/桌面/OCSM/frame/`; they are cleaned
    (`tools/frame_clean.py --install`: strips the old CAD environment's leftovers and repairs layer
    colours) before landing in the plugin directory's `frame/`;
  - The frame's `OCSM_GB` text style is defined **the same way as the plugin's `text_style_defs()`**
    (user decision 2026-09-28: the plugin is authoritative): group 3 = the TrueType name
    (`Zhuque Fangsong`, not the `Unicode` SHX placeholder), width factor 0.7, `height = 3.5`
    (the template used to say 0.0 = no fixed height) and `annotative = 1`. `tools/frame_clean.py`
    produces the frame with those values and self-checks the result; `annotative` travels in the
    `AcadAnnotative` XDATA, which LibreDWG drops when it writes the DWG, so the plugin's
    `ensure_text_styles()` lines the host's style up on insertion;
  - On a new machine: drop the frame DWGs into that `frame/`, or pass `OCSM_FRAME_SRC=<dir with frames>`
    to `tools/deploy_plugin.sh` (an already-installed file with the same name is never overwritten);
  - The deploy script's self-check **only warns (and still exits 0) when `frame/` is empty** — frames
    are user-supplied data, a first-time user should not be blocked; an empty directory merely makes
    `OCSMFRAMEINIT` report “frame folder not found”. To turn it into a hard gate (release/CI), set
    `OCSM_REQUIRE_FRAME=1` explicitly (only then does it exit non-zero).
- When the directory does not exist / is empty it reports: `OCSMFRAMEINIT: 找不到图框文件夹 …。请在插件目录的 frame/ 中放入 DWG。`
  (OCSMFRAMEINIT: frame folder not found …; put DWG files into `frame/` in the plugin directory.)
- This step only **registers the choice**, nothing is put into the drawing yet → run `OCSMFRAMEINSERT` next.

### Step 2 `OCSMFRAMEINSERT`: insert at scale

- Parameters: **the scale is two positive integers and one of them must be 1** (`1:1`, `1:2`, `2:1`…). An illegal scale reports
  `OCSMFRAMEINSERT: 非法的比例（必须为两个正整数且其一为 1）。` (illegal scale (must be two positive integers with one of them 1)).
- Running it without `OCSMFRAMEINIT` first reports: `OCSMFRAMEINSERT: 没有待处理的图框选择。请先运行 OCSMFRAMEINIT。` (no pending frame selection; run OCSMFRAMEINIT first).
- The plugin will:
  1. compute the scale factor `scale = scale_v2 / scale_v1`;
  2. **does not pre-create a scaled dimension style**; when an annotation is generated, `OCSMDIMGULIDE` / `OCSMDIM2GB` creates the matching `OCSM_GB_xN` on demand at the annotation's actual frame anchor, so dimensions use the correct frame scale;
  3. import the frame block (including the title-block ATTDEFs);
  4. enter **cursor-follow** interaction: moving the mouse carries the frame preview → **one click to settle**.
- After settling, the title-block attributes (drawing name / drawing number / material / scale / weight etc.) can be edited directly by double-clicking the ATTDEF, or with the host's attribute editor.

## 2. The Relation Between Scale and Dimensions (key concept)

| Concept | Where | Notes |
| --- | --- | --- |
| **Drawing scale** | frame-block scaling | the geometry is still drawn 1:1; do **not** shrink the geometry into the frame |
| **Dimension text height** | `OCSMDIMGULIDE` / `OCSMDIM2GB` creates `OCSM_GB_xN` on demand at the actual annotation anchor | the annotation uses the frame scale at that anchor; the printed text height remains 5 |
| the "scale" cell of the title block | frame ATTDEF | filled in / changed by hand, e.g. `1:2` |

> When printing, print at the frame's actual size (A3 = 420×297); because the geometry is 1:1, enlarging the frame N× means enlarging "the sheet" N×,
> so the dimension style is enlarged N× as well.

## 3. AI-Side Recipe

```jsonc
// 1) open the frame selection window (a human picks; an AI-driven run needs human help at this step)
{ "op": "run", "request_id": "f1", "document_id": 1, "revision": 3, "cmd": "OCSMFRAMEINIT" }
// —— the human picks a frame in the window, fills in the scale, clicks "确定" (OK) ——
// 2) insert (enters cursor follow; an AI cannot click the mouse → a human must click, or a host run carrying point coordinates)
{ "op": "run", "request_id": "f2", "document_id": 1, "revision": 4, "cmd": "OCSMFRAMEINSERT" }
// 3) verify: the frame block + title-block ATTDEFs
{ "op": "read", "op": "query", "parameters": { "type": "Insert" } }
```

> If an AI-driven run cannot click: treat the frame as a "human step" and hand it to the user (or later import the frame block in headless mode and scale it to the target size yourself).

## 4. Pitfalls

1. **Forgetting `OCSMFRAMEINSERT` after `OCSMFRAMEINIT`**: only a registration, no entity.
2. **Scale written the wrong way round**: `1:2` = the geometry is enlarged 2× to fit the frame (printed at reduced scale); `2:1` = the geometry is halved. Choose per the GB intent.
3. **Layers/styles carried by the frame itself**: an imported frame block brings its own layers (such as "图框线"), which is **normal** —
   OCSM's 10 layers govern part geometry, so the frame does not have to be moved onto `1轮廓实线层`.
4. **The BOM position depends on the frame geometry**: the BOM's right boundary/width/bottom edge are measured from the frame (inner frame right edge, title-block width, title-block top edge),
   so after swapping the frame the BOM position must be re-aligned (see `10-bom.md`).

## 5. Verification Checklist

- [ ] `frame/` contains the sheet-format dwg you want (the file name = the label in the window).
- [ ] After inserting, `query Insert` shows the frame block; the title-block ATTDEFs are editable.
- [ ] Inserting the frame itself does not create `OCSM_GB_xN`; after `OCSMDIMGULIDE` or `OCSMDIM2GB` generates an annotation inside a scaled frame, the matching style appears and is used.
