# OpenCADStudio - Unreleased (2026-10-25 to Present)

> **Summary:** Added dimension association controls and converter-based file interchange with explicit review of content a conversion cannot preserve.

### 🌟 Notable Release Notes

* **Dimension and Constraint Controls:** Added `DIMOVERRIDE`, `DIMDISASSOCIATE`, `DIMREASSOCIATE`, and `DCDISPLAY`, plus `QPMODE` for Quick Properties. Reassociation anchors dimensions to geometry at their definition points and reports partial results. *(Contributed by [@HakanSeven12](https://github.com/HakanSeven12))*
* **Additional File Formats:** Added open and save workflows through OpenCADDocument converters. Before a save that would lose converter-reported drawing content, the app lists the losses and lets you return or continue. *(Contributed by [@HakanSeven12](https://github.com/HakanSeven12))*
* **Recovery Diagnostics:** Recovery reports now identify the acadrust reader revision used to read the drawing. *(Contributed by [@HakanSeven12](https://github.com/HakanSeven12))*

### 🚀 New Features

* **OCSM Shaft Fillets and Independent Keyway Lengths:** Added independent left/right convex fillets (`FI2@L` / `FI2@R`) and concave shoulder-root fillets (`FI2@L:CONCAVE`), tangent to cylindrical or tapered outlines. CH/FI are mutually exclusive only at the same end; a root fillet may share a shoulder with the neighbouring outer chamfer. Optional keyway `SL` sets the actual slot length and draws the open slot without assembly-key lead-in geometry. DSL/JSON, preview, native section boundaries and export share the same validation.
* **Hatch Region Trimming:** `TRIM` now accepts interior hatch picks and removes only the filled region delimited by cutting edges, leaving outlines and cutting geometry intact. Retains hatch pattern properties, supports repeated picks and Undo/Redo, and preserves holes; the clipped hatch is detached from its old full-boundary association. See [usage and scope](../docs/trim-hatch.md).
* **Native Block Properties Table Grips:** Author and edit table values through native grips, with must-match row behavior.
* **Reference-Side Row Selection:** Select a Block Properties Table row from a dynamic block reference and see its authoring state in the Block Editor.
* **Authoring Selection and Preview:** The Block Editor now presents table popup and selection state, edits and previews parameters, and manages parameter and base-point graph items more consistently.
* **Plugin API v11–v13:** Plugins can read and drive entity grips, place their own grip points and react to dragging them, create constraints, and run commands quietly. Plugins now learn which entities changed instead of rescanning the whole drawing, read and write records in batches, and start up more reliably. *(Contributed by [@schoeller](https://github.com/schoeller))*
* **Dimension Overrides and Associations:** Set or clear per-dimension variable overrides; remove a dimension's association or reassociate it to geometry. `DCDISPLAY` toggles constraint indicators on selected objects, and `QPMODE` reads or sets Quick Properties visibility.
* **Converter Interchange and Loss Review:** Open and save supported external formats through registered converters. Review converter-reported losses before choosing to proceed with a save.
* **Drawing Properties Dialog:** `DWGPROPS` opens a dialog with a read-only General tab, an editable Summary tab and a table of custom properties (name/value). OK stores the changes as a single undo step, Cancel leaves the drawing untouched, and imported values are kept exactly as read. Saving to DWG or DXF R2000 and older, which cannot store the drawing properties, warns first when the drawing has any. The `FIELD` dialog lists the custom properties under Document and inserts them as `%<\AcVar CustomDP.Name>%`, which shows the current value and follows later edits.
* **Layer Walk:** `LAYWALK` opens the non-modal LayerWalk pallet. Click through layers and only their objects stay on screen; the hide is a temporary view override that leaves the layer table, the dirty flag and undo untouched until you commit it with *Keep Result* or by turning *Restore on exit* off. Wildcard filtering with saved patterns, hold (Always Show), purge, layer-state capture, zoom to layer extents and select objects on layer are all in the pallet. See [docs/layer-walk.md](../docs/layer-walk.md).

### 🐛 Bug Fixes

* **Partial Dimension Reassociation Reporting:** Identify dimensions that could only be partly re-anchored.
* **Dimension and Preview Precision at Large Coordinates:** Dimensions are now built in a local frame and folded back into their double-single vertex buffers, so their lines, arrows and extension lines stay on the points they measure on UTM-scale drawings. Measurement and drafting previews likewise keep full precision instead of downcasting absolute coordinates to f32, which had left the `DIST` rubber band offset from the picked points.
* **Recovery Reader Revision:** Read the embedded acadrust source hash so recovery reports show the reader revision following the shared document-model migration.
