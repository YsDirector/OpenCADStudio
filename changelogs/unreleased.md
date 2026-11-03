# OpenCADStudio - Unreleased (2026-11-01 to Present)

> **Summary:** Added chained action sequencing to dynamic block parameters and corrected block attribute propagation and visibility-state behavior.

### 🌟 Notable Release Notes

* **Chained Parameter Actions:** A parameter can now chain its actions in a named order, authored from the Actions tab and named by its evaluation graph node. *(Contributed by [@KarimJerbi](https://github.com/KarimJerbi))*
* **Block Attribute Propagation:** Inserted references receive the attributes their definitions declare, bare references fill from their definition, and copies keep their own identity. *(Contributed by [@KarimJerbi](https://github.com/KarimJerbi))*
* **Visibility State Handling:** A state hides only the objects it names, objects outside the state set are left alone, and the first state keeps every object visible. *(Contributed by [@KarimJerbi](https://github.com/KarimJerbi))*

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
* **Chained Actions:** Chain a parameter's actions in the named order from the Actions tab, with the chained action named when it is authored and resolved through its evaluation graph node.
* **Parameter Value Evaluation:** A written parameter value moves the reference it was set on.

### 🐛 Bug Fixes

* **Attribute Insertion:** Give inserted references the attributes their definitions declare, and fill a bare reference from its definition.
* **Attribute Copies:** Keep a reference's own identity when it is copied.
* **Attributed Marker:** Make the attributed marker follow the definition body.
* **Visibility States:** Keep every object visible in the first state, and leave objects outside the state set alone.
* **Explodable Flag:** Make the explodable flag decide against the explode command.
