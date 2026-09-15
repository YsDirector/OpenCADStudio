// HostSession — plugin-facing API implemented inside `app` (private field access).

use std::any::Any;

use acadrust::tables::AppId;
use acadrust::xdata::ExtendedDataRecord;
use ocs_plugin_api::host::{CadDocument, EntityType, Handle, HostApi};
use ocs_plugin_api::shm::{DocumentSnapshotStore, DocumentViewData};

#[cfg(not(target_arch = "wasm32"))]
use crate::plugin::v4_support;
use super::OpenCADStudio;

/// Session adapter: one active document tab, command line, undo.
pub(crate) struct HostSession<'a> {
    app: &'a mut OpenCADStudio,
    tab: usize,
    doc_store: Option<DocumentSnapshotStore<DocumentViewData>>,
}

impl<'a> HostSession<'a> {
    pub(crate) fn new(app: &'a mut OpenCADStudio, tab: usize) -> Self {
        Self {
            app,
            tab,
            doc_store: None,
        }
    }

    pub fn tab_index(&self) -> usize {
        self.tab
    }

    pub fn tab_id(&self) -> u64 {
        self.app.tabs[self.tab].id
    }

    pub fn document_path(&self, tab_id: u64) -> Option<std::path::PathBuf> {
        self.app
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.current_path.clone())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn document_view_v4(&mut self, tab_id: u64) -> Option<ocs_plugin_api::shm::DocumentViewInfo> {
        if tab_id != self.tab_id() {
            return None;
        }
        v4_support::open_document_view_v4(tab_id, self.document())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn close_document_view_v4(&mut self, tab_id: u64) {
        if tab_id == self.tab_id() {
            v4_support::close_document_view_v4(tab_id);
        }
    }

    pub fn document(&self) -> &CadDocument {
        &self.app.tabs[self.tab].scene.document
    }

    pub fn document_mut(&mut self) -> &mut CadDocument {
        &mut self.app.tabs[self.tab].scene.document
    }

    pub fn document_view(&mut self) -> Option<ocs_plugin_api::shm::DocumentViewInfo> {
        if self.doc_store.is_none() {
            let mut store = DocumentSnapshotStore::<DocumentViewData>::new(self.tab as u64, 8 * 1024 * 1024).ok()?;
            store.publish(&self.document().into()).ok()?;
            self.doc_store = Some(store);
        }
        let store = self.doc_store.as_ref()?;
        Some(ocs_plugin_api::shm::DocumentViewInfo {
            path: store.path().to_string_lossy().to_string(),
            version: store.version(),
        })
    }

    fn publish_document_view(&mut self) {
        let doc = &self.app.tabs[self.tab].scene.document;
        // Only publish views that have been opened by a consumer. V3 is lazily
        // created by document_view(); V4 is tracked per-tab by the V4 manager.
        if let Some(store) = self.doc_store.as_mut() {
            if let Err(e) = store.publish(&doc.into()) {
                eprintln!(
                    "[host] failed to publish document view for tab {}: {e}",
                    self.tab
                );
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        v4_support::publish_document_view_v4(self.tab_id(), doc);
    }

    pub fn add_entity(&mut self, entity: EntityType) -> Handle {
        let handle = self.app.tabs[self.tab].scene.add_entity(entity);
        self.publish_document_view();
        handle
    }

    pub fn add_entities(&mut self, entities: Vec<EntityType>) -> Vec<Handle> {
        let handles = self.app.tabs[self.tab].scene.add_entities(entities);
        self.publish_document_view();
        handles
    }

    /// API v5 (OCSMechanical): load `req.path` (a frame DWG), merge its
    /// tables, and define it as a block under `req.block_name` when absent.
    /// Returns the block's attribute definitions (ATTDEFs) in draw order so
    /// the plugin can build an INSERT with concrete attribute values.
    pub fn import_frame_block_impl(
        &mut self,
        req: ocs_plugin_api::host::ImportFrameBlockRequest,
    ) -> Result<Vec<acadrust::entities::AttributeDefinition>, String> {
        let i = self.tab;
        let block_name = req.block_name.trim().to_string();
        if block_name.is_empty() || block_name.starts_with('*') {
            return Err("OCSMFRAME: 非法的块名".to_string());
        }

        self.push_undo("OCSMFRAME");

        // 1. Define the block from the frame DWG when it is not present yet.
        if self.app.tabs[i].scene.document.block_records.get(&block_name).is_none() {
            let frame_doc = crate::io::load_file(std::path::Path::new(&req.path))?;
            self.import_frame_doc_as_block(&frame_doc, &block_name)?;
            self.app.tabs[i].scene.populate_meshes_from_document();
        }

        // 2. Return the block's attribute definitions in draw order.
        let doc = &self.app.tabs[i].scene.document;
        let mut attdefs = Vec::new();
        if let Some(br) = doc.block_records.get(&block_name) {
            for &h in &br.entity_handles {
                if let Some(acadrust::EntityType::AttributeDefinition(ad)) = doc.get_entity(h) {
                    attdefs.push(ad.clone());
                }
            }
        }
        Ok(attdefs)
    }

    /// Merge `frame_doc`'s layers / linetypes / text styles into the active
    /// document (name-collision-safe, no xref-style prefixing), then define a
    /// block named `block_name` from its model-space entities (ATTDEFs kept).
    /// Fails when the frame has nested non-layout blocks (unsupported in v1).
    pub fn import_frame_doc_as_block(
        &mut self,
        frame_doc: &acadrust::CadDocument,
        block_name: &str,
    ) -> Result<(), String> {
        use acadrust::EntityType;
        let i = self.tab;

        // ── Tables: layers / linetypes / text styles (by name, keep existing).
        {
            let doc = &mut self.app.tabs[i].scene.document;
            for layer in frame_doc.layers.iter() {
                if doc.layers.iter().any(|l| l.name.eq_ignore_ascii_case(&layer.name)) {
                    continue;
                }
                let mut cloned = layer.clone();
                cloned.handle = doc.allocate_handle();
                doc.layers.add_or_replace(cloned);
            }
            for lt in frame_doc.line_types.iter() {
                let sentinel = lt.name.eq_ignore_ascii_case("ByLayer")
                    || lt.name.eq_ignore_ascii_case("ByBlock")
                    || lt.name.eq_ignore_ascii_case("Continuous");
                if sentinel
                    || doc
                        .line_types
                        .iter()
                        .any(|l| l.name.eq_ignore_ascii_case(&lt.name))
                {
                    continue;
                }
                let mut cloned = lt.clone();
                cloned.handle = doc.allocate_handle();
                doc.line_types.add_or_replace(cloned);
            }
            for style in frame_doc.text_styles.iter() {
                if doc
                    .text_styles
                    .iter()
                    .any(|s| s.name.eq_ignore_ascii_case(&style.name))
                {
                    continue;
                }
                let mut cloned = style.clone();
                cloned.handle = doc.allocate_handle();
                doc.text_styles.add_or_replace(cloned);
            }
        }

        // ── Nested block records: reject in v1 rather than leave dangling
        // references inside the frame block.
        let nested: Vec<String> = frame_doc
            .block_records
            .iter()
            .filter(|br| {
                !br.is_model_space()
                    && !br.is_paper_space()
                    && !br.is_layout()
                    && !br.flags.is_xref
                    && !br.flags.is_xref_overlay
            })
            .map(|br| br.name.clone())
            .collect();
        if !nested.is_empty() {
            return Err(format!(
                "OCSMFRAME: 图框包含嵌套块（{}），暂不支持",
                nested.join(", ")
            ));
        }

        // ── Model-space entities in block draw order, corrupt ones dropped.
        let ms_handle = frame_doc.header.model_space_block_handle;
        let mut entities: Vec<EntityType> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(br) = frame_doc
            .block_records
            .iter()
            .find(|br| br.handle == ms_handle)
        {
            for &h in &br.entity_handles {
                if let Some(e) = frame_doc.get_entity(h) {
                    if seen.insert(h) {
                        entities.push(e.clone());
                    }
                }
            }
        }
        for e in frame_doc.entities() {
            let h = e.common().handle;
            if seen.insert(h) {
                entities.push(e.clone());
            }
        }
        entities.retain(|e| {
            !matches!(
                e,
                EntityType::Block(_)
                    | EntityType::BlockEnd(_)
                    | EntityType::AttributeEntity(_)
            ) && !crate::io::is_entity_corrupt(e)
        });

        if entities.is_empty() {
            return Err("OCSMFRAME: 图框 DWG 没有可导入的实体".to_string());
        }

        // ── Define the block; base = the frame's model-space insertion base.
        let base_vec = frame_doc.header.model_space_insertion_base;
        let base = glam::DVec3::new(base_vec.x, base_vec.y, base_vec.z);
        self.app.tabs[i]
            .scene
            .define_block_from_owned_entities(entities, block_name, base)
            .map_err(|e| format!("OCSMFRAME: 定义块失败: {e}"))?;
        Ok(())
    }

    pub fn bump_geometry(&mut self) {
        self.app.tabs[self.tab].scene.bump_geometry();
    }

    /// Replace the entity carrying `entity`'s handle in place, refreshing the
    /// scene's derived caches. Returns `false` when no entity has that handle.
    pub fn update_entity(&mut self, entity: EntityType) -> bool {
        let ok = self.app.tabs[self.tab].scene.update_entity(entity);
        if ok {
            self.publish_document_view();
        }
        ok
    }

    /// Delete the entity with `handle`, keeping the scene's render caches in
    /// sync. Returns `false` when the entity is absent or on a locked layer
    /// (which `erase_entities` refuses to remove).
    pub fn remove_entity(&mut self, handle: Handle) -> bool {
        if self.document().get_entity(handle).is_none() {
            return false;
        }
        self.app.tabs[self.tab].scene.erase_entities(&[handle]);
        let removed = self.document().get_entity(handle).is_none();
        if removed {
            self.publish_document_view();
        }
        removed
    }

    // ── XDATA convenience ──────────────────────────────────────────────────
    // Plugins persist domain data as XDATA on plain entities so it round-trips
    // through DWG/DXF. These wrap the `acadrust::xdata` API keyed by entity
    // handle and keep the APPID table in sync.

    /// Read the XDATA record for `app_name` attached to entity `handle`, if any.
    pub fn read_record(&self, handle: Handle, app_name: &str) -> Option<&ExtendedDataRecord> {
        self.document()
            .get_entity(handle)?
            .common()
            .extended_data
            .get_record(app_name)
    }

    /// Attach `record` to entity `handle`, replacing any existing record for the
    /// same application. Registers the application in the APPID table when
    /// missing so the file stays valid for other CAD apps. Returns `false` when
    /// the entity does not exist.
    pub fn write_record(&mut self, handle: Handle, record: ExtendedDataRecord) -> bool {
        if self.app.tabs[self.tab].scene.is_layer_locked(handle) {
            return false;
        }
        let app = record.application_name.clone();
        self.ensure_app_id(&app);
        let app_handle = self.document().app_ids.get(&app).map(|a| a.handle.value());
        let Some(entity) = self.document_mut().get_entity_mut(handle) else {
            return false;
        };
        let xd = &mut entity.common_mut().extended_data;
        // Drop any existing record for this app, then append the new one.
        let kept: Vec<_> = xd
            .records()
            .iter()
            .filter(|r| r.application_name != app)
            .cloned()
            .collect();
        xd.clear();
        for r in kept {
            xd.add_record(r);
        }
        xd.add_record(record);
        // Drop stale verbatim EED for this app so the fresh record — not the
        // pre-edit bytes captured on a prior read — wins on the next save.
        // Otherwise a plugin's edit made after a save/reopen (which registered
        // the app in `raw_dwg_eed`) would not persist.
        if let Some(ah) = app_handle {
            xd.raw_dwg_eed.retain(|(a, _)| *a != ah);
        }
        self.publish_document_view();
        true
    }

    /// Remove the XDATA record for `app_name` from entity `handle`. Returns
    /// `true` when a record was actually removed.
    pub fn remove_record(&mut self, handle: Handle, app_name: &str) -> bool {
        if self.app.tabs[self.tab].scene.is_layer_locked(handle) {
            return false;
        }
        let app_handle = self.document().app_ids.get(app_name).map(|a| a.handle.value());
        let Some(entity) = self.document_mut().get_entity_mut(handle) else {
            return false;
        };
        let xd = &mut entity.common_mut().extended_data;
        let kept: Vec<_> = xd
            .records()
            .iter()
            .filter(|r| r.application_name != app_name)
            .cloned()
            .collect();
        let removed_record = kept.len() != xd.records().len();
        // Also drop verbatim EED for this app so the removal persists across a
        // save (a record read back from DWG lives in `raw_dwg_eed`).
        let removed_raw = app_handle
            .map(|ah| xd.raw_dwg_eed.iter().any(|(a, _)| *a == ah))
            .unwrap_or(false);
        if !removed_record && !removed_raw {
            return false;
        }
        xd.clear();
        for r in kept {
            xd.add_record(r);
        }
        if let Some(ah) = app_handle {
            xd.raw_dwg_eed.retain(|(a, _)| *a != ah);
        }
        self.publish_document_view();
        true
    }

    /// Register `name` in the APPID table if it is not already present, so XDATA
    /// written under it survives a DWG/DXF round-trip. The entry is given a real
    /// handle — a null-handle APPID is written as handle 0, which the DWG EED
    /// reference then can't resolve, so the XDATA would be dropped on reopen.
    fn ensure_app_id(&mut self, name: &str) {
        let doc = self.document_mut();
        if !doc.app_ids.contains(name) {
            let mut app = AppId::new(name);
            app.handle = doc.allocate_handle();
            let _ = doc.app_ids.add(app);
        }
    }

    pub fn push_undo(&mut self, label: &str) {
        self.app.push_undo_snapshot(self.tab, label);
    }

    pub fn set_dirty(&mut self) {
        self.app.tabs[self.tab].dirty = true;
    }

    pub fn push_info(&mut self, msg: &str) {
        self.app.command_line.push_info(msg);
    }

    pub fn push_output(&mut self, msg: &str) {
        self.app.command_line.push_output(msg);
    }

    pub fn push_error(&mut self, msg: &str) {
        self.app.command_line.push_error(msg);
    }
}

/// The stable contract a plugin's `dispatch` sees. Each method forwards to the
/// inherent `HostSession` method of the same name (inherent methods take
/// resolution priority, so this is plain delegation, not recursion). The
/// per-tab plugin-state accessors expose the raw `Any` box; the typed
/// `ocs_plugin_api::host::plugin_state*` helpers wrap them.
impl HostApi for HostSession<'_> {
    fn tab_index(&self) -> usize {
        self.tab_index()
    }
    fn document(&self) -> &CadDocument {
        self.document()
    }
    fn document_mut(&mut self) -> &mut CadDocument {
        self.document_mut()
    }
    fn add_entity(&mut self, entity: EntityType) -> Handle {
        self.add_entity(entity)
    }
    fn add_entities(&mut self, entities: Vec<EntityType>) -> Vec<Handle> {
        self.add_entities(entities)
    }
    fn update_entity(&mut self, entity: EntityType) -> bool {
        self.update_entity(entity)
    }
    fn remove_entity(&mut self, handle: Handle) -> bool {
        self.remove_entity(handle)
    }
    fn bump_geometry(&mut self) {
        self.bump_geometry()
    }
    fn read_record(&self, handle: Handle, app_name: &str) -> Option<&ExtendedDataRecord> {
        self.read_record(handle, app_name)
    }
    fn write_record(&mut self, handle: Handle, record: ExtendedDataRecord) -> bool {
        self.write_record(handle, record)
    }
    fn remove_record(&mut self, handle: Handle, app_name: &str) -> bool {
        self.remove_record(handle, app_name)
    }
    fn push_undo(&mut self, label: &str) {
        self.push_undo(label)
    }
    fn begin_undo(&mut self, label: &str) {
        let tab = self.tab;
        self.app.begin_deferred_undo(tab, label);
    }
    fn commit_undo(&mut self) {
        let tab = self.tab;
        self.app.commit_deferred_undo(tab);
    }
    fn set_dirty(&mut self) {
        self.set_dirty()
    }
    fn push_info(&mut self, msg: &str) {
        self.push_info(msg)
    }
    fn push_output(&mut self, msg: &str) {
        self.push_output(msg)
    }
    fn push_error(&mut self, msg: &str) {
        self.push_error(msg)
    }
    fn start_interactive(&mut self, command: Box<dyn ocs_plugin_api::host::InteractiveCommand>) {
        self.app.tabs[self.tab].active_cmd =
            Some(Box::new(PluginInteractiveAdapter {
                inner: command,
                pending_snap: None,
            }));
    }
    fn plugin_state_any(&self, plugin_id: &str) -> Option<&(dyn Any + Send + Sync)> {
        self.app.tabs[self.tab]
            .plugin_state
            .get(plugin_id)
            .map(|b| b.as_ref())
    }
    fn plugin_state_any_mut(&mut self, plugin_id: &str) -> Option<&mut (dyn Any + Send + Sync)> {
        self.app.tabs[self.tab]
            .plugin_state
            .get_mut(plugin_id)
            .map(|b| b.as_mut())
    }
    fn ensure_plugin_state_any(
        &mut self,
        plugin_id: &'static str,
        init: &mut dyn FnMut() -> Box<dyn Any + Send + Sync>,
    ) -> &mut (dyn Any + Send + Sync) {
        self.app.tabs[self.tab]
            .plugin_state
            .entry(plugin_id)
            .or_insert_with(|| init())
            .as_mut()
    }
    fn document_reader(&self) -> Box<dyn ocs_plugin_api::host::DocumentReader + '_> {
        Box::new(ocs_plugin_api::host::CadDocumentReader(self.document()))
    }
    fn document_view(&mut self) -> Option<ocs_plugin_api::shm::DocumentViewInfo> {
        self.document_view()
    }
    fn tab_id(&self) -> u64 {
        self.tab_id()
    }
    fn document_path(&self, tab_id: u64) -> Option<std::path::PathBuf> {
        self.document_path(tab_id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn document_view_v4(&mut self, tab_id: u64) -> Option<ocs_plugin_api::shm::DocumentViewInfo> {
        self.document_view_v4(tab_id)
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn close_document_view_v4(&mut self, tab_id: u64) {
        self.close_document_view_v4(tab_id)
    }

    // ── API v5 (OCSMechanical) ──────────────────────────────────────────────
    fn selected_handles(&self) -> Vec<Handle> {
        self.app.tabs[self.tab]
            .scene
            .selected_entities()
            .into_iter()
            .map(|(h, _)| h)
            .collect()
    }

    fn set_current_layer(&mut self, name: &str) -> bool {
        let i = self.tab;
        if !self.app.tabs[i].scene.document.layers.contains(name) {
            return false;
        }
        let handle = self.app.tabs[i]
            .scene
            .document
            .layers
            .get(name)
            .map(|l| l.handle)
            .unwrap_or(Handle::NULL);
        // Mirror the host's LAYMCUR (#93): header + per-tab default + layers
        // panel + ribbon dropdown must stay in sync or the no-selection
        // refresh re-reads the stale header layer.
        self.app.tabs[i].scene.document.header.current_layer_name = name.to_string();
        self.app.tabs[i].scene.document.header.current_layer_handle = handle;
        self.app.tabs[i].active_layer = name.to_string();
        self.app.tabs[i].layers.current_layer = name.to_string();
        self.app.tabs[i].dirty = true;
        self.app.ribbon.active_layer = name.to_string();
        self.app.refresh_layer_panel();
        true
    }

    fn ensure_layers(&mut self, defs: Vec<ocs_plugin_api::host::LayerDef>) -> usize {
        let i = self.tab;
        let mut created = 0usize;
        for def in defs {
            let doc = &mut self.app.tabs[i].scene.document;
            if doc.layers.iter().any(|l| l.name.eq_ignore_ascii_case(&def.name)) {
                continue;
            }
            let mut layer = acadrust::tables::Layer::new(&def.name);
            layer.handle = doc.allocate_handle();
            layer.color = def.color;
            layer.line_type = def.linetype;
            layer.line_weight = def.lineweight;
            layer.is_plottable = def.plottable;
            layer.flags.off = def.off;
            doc.layers.add_or_replace(layer);
            created += 1;
        }
        if created > 0 {
            self.app.refresh_layer_panel();
            self.app.sync_ribbon_layers();
        }
        created
    }

    fn ensure_linetypes(&mut self, defs: Vec<ocs_plugin_api::host::LinetypeDef>) -> usize {
        use acadrust::tables::{LineType, LineTypeElement};
        let i = self.tab;
        let mut created = 0usize;
        for def in defs {
            let doc = &mut self.app.tabs[i].scene.document;
            if doc.line_types.iter().any(|l| l.name.eq_ignore_ascii_case(&def.name)) {
                continue;
            }
            let mut lt = LineType::new(&def.name);
            lt.handle = doc.allocate_handle();
            lt.description = def.description;
            // .lin convention: positive = dash, negative = space, 0 = dot.
            lt.elements = def
                .elements
                .into_iter()
                .map(|v| LineTypeElement {
                    length: v,
                    complex: None,
                })
                .collect();
            lt.pattern_length = lt.elements.iter().map(|e| e.length.abs()).sum();
            doc.line_types.add_or_replace(lt);
            created += 1;
        }
        if created > 0 {
            self.app.refresh_layer_panel();
        }
        created
    }

    fn ensure_text_styles(&mut self, defs: Vec<ocs_plugin_api::host::TextStyleDef>) -> usize {
        use acadrust::tables::TextStyle;
        let i = self.tab;
        let mut created = 0usize;
        for def in defs {
            let doc = &mut self.app.tabs[i].scene.document;
            if doc
                .text_styles
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(&def.name))
            {
                continue;
            }
            let mut style = TextStyle::new(&def.name);
            style.handle = doc.allocate_handle();
            style.font_file = def.font_file;
            style.big_font_file = def.big_font_file;
            style.true_type_font = def.true_type_font;
            style.height = def.height;
            style.width_factor = def.width_factor;
            style.annotative = def.annotative;
            style.is_shape_file = def.is_shape_file;
            style.is_vertical = def.is_vertical;
            doc.text_styles.add_or_replace(style);
            created += 1;
        }
        created
    }

    fn ensure_dim_styles(&mut self, defs: Vec<ocs_plugin_api::host::DimStyleDef>) -> usize {
        use acadrust::tables::DimStyle;
        let i = self.tab;
        let mut created = 0usize;
        for def in defs {
            let doc = &mut self.app.tabs[i].scene.document;
            if doc
                .dim_styles
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(&def.name))
            {
                // Even when the style already exists, honour `make_current`.
                if def.make_current
                    && !doc
                        .header
                        .current_dimstyle_name
                        .eq_ignore_ascii_case(&def.name)
                {
                    doc.header.current_dimstyle_name = def.name.clone();
                    self.app.tabs[i].dirty = true;
                }
                continue;
            }
            // Template = the document's active dimension style (fields a def
            // does not specify inherit its values — arrow blocks, tolerances,
            // alternate units, …). Falls back to a fresh Standard-style entry.
            let template_name = doc.header.current_dimstyle_name.trim();
            let template = doc
                .dim_styles
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(template_name))
                .cloned()
                .unwrap_or_else(|| DimStyle::new("Standard"));
            let mut style = template;
            style.name = def.name.clone();
            style.handle = doc.allocate_handle();
            // ── Lines & arrows ──
            style.dimtxt = def.dimtxt;
            style.dimasz = def.dimasz;
            style.dimcen = def.dimcen;
            style.dimexe = def.dimexe;
            style.dimexo = def.dimexo;
            style.dimgap = def.dimgap;
            style.dimdli = def.dimdli;
            style.dimdle = def.dimdle;
            style.dimclrd = def.dimclrd;
            style.dimclre = def.dimclre;
            style.dimclrt = def.dimclrt;
            style.dimlwd = def.dimlwd;
            style.dimlwe = def.dimlwe;
            style.dimtad = def.dimtad;
            style.dimjust = def.dimjust;
            style.dimtih = def.dimtih;
            style.dimtoh = def.dimtoh;
            style.dimtix = def.dimtix;
            style.dimsoxd = def.dimsoxd;
            style.dimtofl = def.dimtofl;
            // ── Scale / units ──
            style.dimscale = def.dimscale;
            style.annotative = def.annotative;
            style.dimdec = def.dimdec;
            style.dimlunit = def.dimlunit;
            style.dimzin = def.dimzin;
            style.dimaunit = def.dimaunit;
            style.dimadec = def.dimadec;
            // ── Tolerances ──
            style.dimtol = def.dimtol;
            style.dimtp = def.dimtp;
            style.dimtm = def.dimtm;
            style.dimtdec = def.dimtdec;
            // ── Text ──
            style.dimpost = def.dimpost.clone();
            if !def.dimtxsty.trim().is_empty() {
                style.dimtxsty = def.dimtxsty.clone();
                style.dimtxsty_handle = doc
                    .text_styles
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(&def.dimtxsty))
                    .map(|s| s.handle)
                    .unwrap_or_else(|| {
                        // Keep the name; the writer re-resolves or falls back.
                        acadrust::Handle::NULL
                    });
            }
            doc.dim_styles.add_or_replace(style);
            if def.make_current {
                doc.header.current_dimstyle_name = def.name.clone();
            }
            created += 1;
        }
        if created > 0 {
            self.app.tabs[i].dirty = true;
        }
        created
    }

    fn add_block_record(
        &mut self,
        name: &str,
        entities: Vec<acadrust::EntityType>,
    ) -> Result<acadrust::Handle, String> {
        use acadrust::entities::{Block, BlockEnd};
        use acadrust::EntityType as E;
        let i = self.tab;
        let doc = &mut self.app.tabs[i].scene.document;
        if name.trim().is_empty() {
            return Err("add_block_record: empty block name".into());
        }
        if doc.block_records.get(name).is_some() {
            return Err(format!("块 {name} 已存在"));
        }
        let mut br = acadrust::tables::BlockRecord::new(name);
        br.handle = doc.allocate_handle();
        let origin = acadrust::types::Vector3::new(0.0, 0.0, 0.0);
        // BLOCK/ENDBLK 标记实体需显式 handle + owner（add_entity 不为它们
        // 分配；缺失会导致保存的 DXF 含 handle=0 的非法实体）。
        // 顺序至关重要：先注册 BlockRecord 再 add 成员——否则成员的 owner
        // 路由找不到块，fallback 落入 modelspace（ENTITIES 段双写）。
        let mut block = Block::new(name, origin);
        block.common.handle = doc.allocate_handle();
        block.common.owner_handle = br.handle;
        br.block_entity_handle = block.common.handle;
        doc.add_entity(E::Block(block)).map_err(|e| e.to_string())?;
        let br_handle = br.handle;
        doc.block_records.add(br).map_err(|e| e.to_string())?;

        let mut member_handles = Vec::new();
        for mut e in entities {
            e.common_mut().owner_handle = br_handle;
            member_handles.push(doc.add_entity(e).map_err(|e| e.to_string())?);
        }
        let mut end = BlockEnd::new();
        end.common.handle = doc.allocate_handle();
        end.common.owner_handle = br_handle;
        let end_handle = end.common.handle;
        doc.add_entity(E::BlockEnd(end)).map_err(|e| e.to_string())?;

        if let Some(br) = doc.block_records.get_mut(name) {
            br.entity_handles = member_handles;
            // ENDBLK 只由 block_end_handle 引用，不进 entity_handles
            //（否则写出器会写两次 ENDBLK，handle 重复）。
            br.block_end_handle = end_handle;
        }
        self.app.tabs[i].dirty = true;
        Ok(br_handle)
    }

    fn show_frame_picker(&mut self, frames: Vec<ocs_plugin_api::host::FrameItem>) -> bool {
        if frames.is_empty() {
            return false;
        }
        self.app.ocsm_frame_picker = Some(crate::app::OcsmFramePicker {
            frames,
            selected: 0,
            // 比例缺省 1:1（用户可改，两个正整数且其一必须为 1）。
            scale_v1: "1".to_string(),
            scale_v2: "1".to_string(),
            error: None,
        });
        self.app.active_modal = Some(crate::app::ModalKind::OcsmFramePicker);
        true
    }

    fn take_pending_frame_selection(&mut self) -> Option<ocs_plugin_api::host::FrameSelection> {
        self.app.ocsm_pending_frame_selection.take()
    }

    fn import_frame_block(
        &mut self,
        req: ocs_plugin_api::host::ImportFrameBlockRequest,
    ) -> Result<Vec<acadrust::entities::AttributeDefinition>, String> {
        self.import_frame_block_impl(req)
    }
}

/// Bridges a plugin's [`InteractiveCommand`](ocs_plugin_api::host::InteractiveCommand)
/// to the host's internal `CadCommand`, so a plugin tool drives the host's
/// point-collection flow (viewport clicks or `--serve` coordinates) just like a
/// built-in tool.
struct PluginInteractiveAdapter {
    inner: Box<dyn ocs_plugin_api::host::InteractiveCommand>,
    /// OSNAP result computed at the pending entity-pick click (injected by the
    /// view layer when [`CadCommand::entity_pick_applies_osnap`] is true).
    pending_snap: Option<glam::DVec3>,
}

impl crate::command::CadCommand for PluginInteractiveAdapter {
    fn name(&self) -> &'static str {
        "PLUGIN"
    }
    // Every call into the plugin runs under a panic guard (#145): a buggy plugin
    // that panics mid-command leaves the host running — the command just ends.
    fn prompt(&self) -> String {
        crate::plugin::guard("InteractiveCommand::prompt", || self.inner.prompt())
            .unwrap_or_default()
    }
    fn on_point(&mut self, pt: glam::DVec3) -> crate::command::CmdResult {
        crate::plugin::guard("InteractiveCommand::on_point", || {
            self.inner.on_point([pt.x as f64, pt.y as f64, pt.z as f64])
        })
        .map(plugin_step_to_result)
        .unwrap_or(crate::command::CmdResult::Cancel)
    }
    fn on_enter(&mut self) -> crate::command::CmdResult {
        crate::plugin::guard("InteractiveCommand::on_enter", || self.inner.on_enter())
            .map(plugin_step_to_result)
            .unwrap_or(crate::command::CmdResult::Cancel)
    }
    fn wants_text_input(&self) -> bool {
        crate::plugin::guard("InteractiveCommand::wants_text_input", || {
            self.inner.wants_text_input()
        })
        .unwrap_or(false)
    }
    fn on_text_input(&mut self, text: &str) -> Option<crate::command::CmdResult> {
        use ocs_plugin_api::host::CommandStep;
        crate::plugin::guard("InteractiveCommand::on_text_input", || {
            self.inner.on_text_input(text)
        })
        .and_then(|step| match step {
            // A plugin that does not claim the token lets the host fall
            // through to its normal interpretation (handle pick, …).
            CommandStep::Ignored => None,
            other => Some(plugin_step_to_result(other)),
        })
    }
    fn needs_entity_pick(&self) -> bool {
        crate::plugin::guard("InteractiveCommand::needs_object_pick", || {
            self.inner.needs_object_pick()
        })
        .unwrap_or(false)
    }
    fn entity_pick_applies_osnap(&self) -> bool {
        crate::plugin::guard("InteractiveCommand::entity_pick_applies_osnap", || {
            self.inner.entity_pick_applies_osnap()
        })
        .unwrap_or(true)
    }
    fn hides_crosshair(&self) -> bool {
        !crate::plugin::guard("InteractiveCommand::entity_pick_applies_osnap", || {
            self.inner.entity_pick_applies_osnap()
        })
        .unwrap_or(true)
    }
    // 插件命令：空白点击也转发（POWERDIM 两种模式都需要任意点放置）。
    fn entity_pick_blank_forwards(&self) -> bool {
        true
    }
    fn inject_pick_snap(&mut self, snap: Option<glam::DVec3>) {
        self.pending_snap = snap;
    }
    fn on_entity_pick(&mut self, handle: Handle, pt: glam::DVec3) -> crate::command::CmdResult {
        let snap = self.pending_snap.take();
        let snapped = snap.is_some();
        let pt = snap.unwrap_or(pt);
        crate::plugin::guard("InteractiveCommand::on_object_pick_snapped", || {
            self.inner
                .on_object_pick_snapped(handle, [pt.x as f64, pt.y as f64, pt.z as f64], snapped)
        })
        .map(plugin_step_to_result)
        .unwrap_or(crate::command::CmdResult::Cancel)
    }
    fn plugin_preview_entity(&mut self, pt: glam::DVec3) -> Option<acadrust::EntityType> {
        if !crate::plugin::guard("InteractiveCommand::wants_mouse_move", || {
            self.inner.wants_mouse_move()
        })
        .unwrap_or(false)
        {
            return None;
        }
        crate::plugin::guard("InteractiveCommand::on_mouse_move", || {
            self.inner
                .on_mouse_move([pt.x as f64, pt.y as f64, pt.z as f64])
        })
        .unwrap_or(None)
    }
}

/// Bridges an out-of-process plugin's interactive command to the host's
/// `CadCommand`. Events are sent over IPC and the returned `CommandStep` is
/// translated into a `CmdResult`. Prompt and object-pick mode are cached and
/// refreshed after each event.
pub(crate) struct PluginProcessInteractiveAdapter {
    pub process: std::sync::Arc<ocs_plugin_api::process::PluginProcess>,
    pub command_id: u64,
    prompt: Option<String>,
    needs_entity_pick: Option<bool>,
    wants_text_input: Option<bool>,
    wants_mouse_move: Option<bool>,
    /// Cached object-snap-at-entity-pick flag (POWERDIM pick-point vs
    /// segment-select mode). Refreshed after every command event.
    entity_pick_osnap: Option<bool>,
    /// OSNAP result computed at the pending entity-pick click (injected by the
    /// view layer when [`CadCommand::entity_pick_applies_osnap`] is true).
    pending_snap: Option<glam::DVec3>,
}

impl PluginProcessInteractiveAdapter {
    pub(crate) fn new(
        process: std::sync::Arc<ocs_plugin_api::process::PluginProcess>,
        command_id: u64,
    ) -> Self {
        let prompt = process.get_prompt(command_id).ok();
        let needs_entity_pick = process.needs_entity_pick(command_id).ok();
        let wants_text_input = process.get_wants_text_input(command_id).ok();
        let wants_mouse_move = process.get_wants_mouse_move(command_id).ok();
        let entity_pick_osnap = process.get_entity_pick_osnap(command_id).ok();
        Self {
            process,
            command_id,
            prompt,
            needs_entity_pick,
            wants_text_input,
            wants_mouse_move,
            entity_pick_osnap,
            pending_snap: None,
        }
    }

    fn refresh(&mut self) {
        self.prompt = self.process.get_prompt(self.command_id).ok();
        self.needs_entity_pick = self.process.needs_entity_pick(self.command_id).ok();
        self.wants_text_input = self.process.get_wants_text_input(self.command_id).ok();
        self.wants_mouse_move = self.process.get_wants_mouse_move(self.command_id).ok();
        self.entity_pick_osnap = self.process.get_entity_pick_osnap(self.command_id).ok();
    }
}

impl crate::command::CadCommand for PluginProcessInteractiveAdapter {
    fn name(&self) -> &'static str {
        "PLUGIN"
    }
    fn prompt(&self) -> String {
        self.prompt.clone().unwrap_or_default()
    }
    fn on_point(&mut self, pt: glam::DVec3) -> crate::command::CmdResult {
        use ocs_plugin_api::ipc::protocol::InteractiveEvent;
        let result = self
            .process
            .interactive_event(
                self.command_id,
                InteractiveEvent::Point([pt.x, pt.y, pt.z]),
            )
            .map(plugin_step_to_result)
            .unwrap_or(crate::command::CmdResult::Cancel);
        self.refresh();
        result
    }
    fn on_enter(&mut self) -> crate::command::CmdResult {
        use ocs_plugin_api::ipc::protocol::InteractiveEvent;
        let result = self
            .process
            .interactive_event(self.command_id, InteractiveEvent::Enter)
            .map(plugin_step_to_result)
            .unwrap_or(crate::command::CmdResult::Cancel);
        self.refresh();
        result
    }
    fn wants_text_input(&self) -> bool {
        self.wants_text_input.unwrap_or(false)
    }
    fn on_text_input(&mut self, text: &str) -> Option<crate::command::CmdResult> {
        use ocs_plugin_api::ipc::protocol::InteractiveEvent;
        let result = self
            .process
            .interactive_event(
                self.command_id,
                InteractiveEvent::Text(text.to_string()),
            )
            .ok()
            .and_then(|step| match step {
                // A plugin that does not claim the token lets the host fall
                // through to its normal interpretation (handle pick, …).
                ocs_plugin_api::host::CommandStep::Ignored => None,
                other => Some(plugin_step_to_result(other)),
            });
        self.refresh();
        result
    }
    fn needs_entity_pick(&self) -> bool {
        self.needs_entity_pick.unwrap_or(false)
    }
    fn entity_pick_applies_osnap(&self) -> bool {
        self.entity_pick_osnap.unwrap_or(true)
    }
    /// Segment-select mode (object snap off) shows a pure pick-box cursor.
    fn hides_crosshair(&self) -> bool {
        self.entity_pick_osnap == Some(false)
    }
    // 插件命令：空白点击也转发（POWERDIM 两种模式都需要任意点放置）。
    fn entity_pick_blank_forwards(&self) -> bool {
        true
    }
    fn inject_pick_snap(&mut self, snap: Option<glam::DVec3>) {
        self.pending_snap = snap;
    }
    fn on_entity_pick(&mut self, handle: Handle, pt: glam::DVec3) -> crate::command::CmdResult {
        use ocs_plugin_api::ipc::protocol::InteractiveEvent;
        let snap = self.pending_snap.take();
        let snapped = snap.is_some();
        let pt = snap.unwrap_or(pt);
        let result = self
            .process
            .interactive_event(
                self.command_id,
                InteractiveEvent::ObjectPick {
                    handle,
                    pt: [pt.x, pt.y, pt.z],
                    snapped,
                },
            )
            .map(plugin_step_to_result)
            .unwrap_or(crate::command::CmdResult::Cancel);
        self.refresh();
        result
    }
    fn plugin_preview_entity(&mut self, pt: glam::DVec3) -> Option<acadrust::EntityType> {
        // Skip the IPC round-trip when the plugin's current step shows no
        // rubber band (the flag is refreshed after every command event).
        if !self.wants_mouse_move.unwrap_or(false) {
            return None;
        }
        self.process
            .interactive_hover(self.command_id, [pt.x, pt.y, pt.z])
            .unwrap_or(None)
    }
}

fn plugin_step_to_result(step: ocs_plugin_api::host::CommandStep) -> crate::command::CmdResult {
    use crate::command::CmdResult;
    use ocs_plugin_api::host::CommandStep;
    match step {
        CommandStep::NeedPoint => CmdResult::NeedPoint,
        CommandStep::Commit(e) => CmdResult::CommitEntity(e),
        CommandStep::CommitAndEnd(e) => CmdResult::CommitAndExit(e),
        CommandStep::Done | CommandStep::Cancel | CommandStep::Ignored => CmdResult::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::OpenCADStudio;
    use acadrust::entities::Point;
    use acadrust::xdata::XDataValue;
    use ocs_plugin_api::host::DocumentReader;

    // ── API v5 (OCSMechanical) ──────────────────────────────────────────────

    fn layer_def() -> ocs_plugin_api::host::LayerDef {
        ocs_plugin_api::host::LayerDef {
            name: "1轮廓实线层".into(),
            color: acadrust::types::Color::from_index(7),
            linetype: "Continuous".into(),
            lineweight: acadrust::types::LineWeight::from_value(35),
            plottable: true,
            off: false,
        }
    }

    #[test]
    fn v5_ensure_layers_is_idempotent_and_assigns_real_handles() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        assert_eq!(host.ensure_layers(vec![layer_def()]), 1);
        let l = host.document().layers.get("1轮廓实线层").expect("layer created");
        assert!(!l.handle.is_null(), "layer must carry a real handle");
        assert_eq!(l.color, acadrust::types::Color::from_index(7));
        assert_eq!(l.line_type, "Continuous");
        assert_eq!(l.line_weight, acadrust::types::LineWeight::from_value(35));

        // 幂等：再跑一次不重复创建；大小写不敏感也跳过。
        assert_eq!(host.ensure_layers(vec![layer_def()]), 0);
        assert_eq!(
            host.ensure_layers(vec![ocs_plugin_api::host::LayerDef {
                name: "1轮廓实线层".to_ascii_lowercase(),
                ..layer_def()
            }]),
            0
        );
    }

    #[test]
    fn v6_begin_undo_keeps_the_snapshot_open_across_messages() {
        // 插件 HTTP/GUI 流程：一次用户动作 = 多条宿主请求。宿主在每条 message 末尾
        // 提交（并丢弃空）pending 快照，所以必须靠显式事务把快照握住。
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        {
            let mut host = HostSession::new(&mut app, 0);
            host.begin_undo("表面粗糙度");
        }
        app.finish_all_pending_history();
        assert!(
            app.tabs[0].history.undo_stack.is_empty(),
            "事务未提交前不应产生撤销条目"
        );
        // 第二条请求：真正改文档（建一个图层代表插件的写操作）。
        {
            let mut host = HostSession::new(&mut app, 0);
            assert_eq!(host.ensure_layers(vec![layer_def()]), 1);
        }
        app.finish_all_pending_history();
        assert!(
            app.tabs[0].history.undo_stack.is_empty(),
            "事务保持打开，跨 message 不提交"
        );
        // 第三条请求：插件提交事务（快照成条目 + revision 自增）。
        let revision_before = app.tabs[0].edit_revision;
        {
            let mut host = HostSession::new(&mut app, 0);
            host.commit_undo();
        }
        assert_eq!(app.tabs[0].history.undo_stack.len(), 1, "提交后才有条目");
        assert_eq!(
            app.tabs[0].edit_revision,
            revision_before + 1,
            "提交即 revision 自增（MCP compare-and-set 才看得到）"
        );
        // 撤销：图层表回到创建前。
        app.undo_active_tab();
        assert!(
            app.tabs[0].scene.document.layers.get("1轮廓实线层").is_none(),
            "插件事务的改动可被撤销"
        );
    }

    #[test]
    fn v6_add_block_record_creates_block_with_members() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        use acadrust::entities::{Line, Solid};
        use acadrust::types::Vector3;
        use acadrust::EntityType as E;
        let mk_line = |a: (f64, f64), b: (f64, f64)| -> E {
            E::Line(Line {
                common: Default::default(),
                start: Vector3::new(a.0, a.1, 0.0),
                end: Vector3::new(b.0, b.1, 0.0),
                thickness: 0.0,
                normal: Vector3::new(0.0, 0.0, 1.0),
            })
        };
        let members = vec![
            mk_line((0.0, 0.0), (35.0, 35.0)),
            E::Solid(Solid::new(
                Vector3::new(35.0, 35.0, 0.0),
                Vector3::new(32.0, 35.0, 0.0),
                Vector3::new(35.0, 32.0, 0.0),
                Vector3::new(35.0, 32.0, 0.0),
            )),
        ];
        let h = host.add_block_record("*D1", members).expect("建块");
        assert!(!h.is_null());

        let doc = host.document();
        let br = doc.block_records.get("*D1").expect("块定义存在");
        // 成员(2)；BLOCK/ENDBLK 由专门字段引用，不进 entity_handles。
        assert_eq!(br.entity_handles.len(), 2);
        assert!(!br.block_entity_handle.is_null(), "BLOCK handle 必须设置");
        assert!(!br.block_end_handle.is_null(), "ENDBLK handle 必须设置");
        assert!(!br.block_end_handle.is_null());
        // 成员实体确实在文档中，且 owner 指向块（不落 modelspace）。
        let m0 = doc.get_entity(br.entity_handles[0]).expect("成员0");
        assert_eq!(m0.common().owner_handle, br.handle, "成员 owner 应为块记录");
        assert!(doc.get_entity(br.entity_handles[1]).is_some());

        // 查重：同名再建报错。
        assert!(host.add_block_record("*D1", vec![]).is_err());
    }

    #[test]
    fn v5_ensure_layers_applies_off_flag() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        let off_def = ocs_plugin_api::host::LayerDef {
            name: "10引导线层".into(),
            color: acadrust::types::Color::from_index(5),
            linetype: "Continuous".into(),
            lineweight: acadrust::types::LineWeight::from_value(0),
            plottable: true,
            off: true,
        };
        assert_eq!(host.ensure_layers(vec![off_def]), 1);
        let l = host.document().layers.get("10引导线层").expect("layer created");
        assert!(l.flags.off, "off=true 的层必须创建为关闭状态");
        assert_eq!(l.line_weight, acadrust::types::LineWeight::from_value(0));

        // 普通层（off=false）保持开启。
        assert_eq!(host.ensure_layers(vec![layer_def()]), 1);
        let l2 = host.document().layers.get("1轮廓实线层").expect("layer created");
        assert!(!l2.flags.off, "off=false 的层应保持开启");
    }

    #[test]
    fn v5_set_current_layer_syncs_all_mirrors() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        {
            let mut host = HostSession::new(&mut app, 0);
            host.ensure_layers(vec![layer_def()]);
        }
        let i = 0;
        {
            let mut host = HostSession::new(&mut app, 0);
            assert!(host.set_current_layer("1轮廓实线层"));
        }
        assert_eq!(app.tabs[i].active_layer, "1轮廓实线层");
        assert_eq!(app.tabs[i].layers.current_layer, "1轮廓实线层");
        assert_eq!(app.ribbon.active_layer, "1轮廓实线层");
        assert_eq!(
            app.tabs[i].scene.document.header.current_layer_name,
            "1轮廓实线层"
        );
        let handle = app.tabs[i]
            .scene
            .document
            .layers
            .get("1轮廓实线层")
            .unwrap()
            .handle;
        assert_eq!(
            app.tabs[i].scene.document.header.current_layer_handle,
            handle
        );
        assert!(app.tabs[i].dirty);

        // 不存在的图层返回 false 且不动状态。
        {
            let mut host = HostSession::new(&mut app, 0);
            assert!(!host.set_current_layer("NO_SUCH_LAYER"));
        }
        assert_eq!(app.tabs[i].active_layer, "1轮廓实线层");
    }

    #[test]
    fn v5_selected_handles_mirrors_selection() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let (h1, h2) = {
            let mut host = HostSession::new(&mut app, 0);
            let h1 = host.add_entity(EntityType::Point(Point::new()));
            let h2 = host.add_entity(EntityType::Point(Point::new()));
            assert!(host.selected_handles().is_empty());
            (h1, h2)
        };
        app.tabs[0].scene.selected.insert(h1);
        {
            let mut host = HostSession::new(&mut app, 0);
            assert_eq!(host.selected_handles(), vec![h1]);
        }
        app.tabs[0].scene.selected.insert(h2);
        {
            let mut host = HostSession::new(&mut app, 0);
            assert_eq!(host.selected_handles().len(), 2);
        }
    }

    #[test]
    fn v5_ensure_linetypes_and_text_styles_idempotent() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        let lts = vec![ocs_plugin_api::host::LinetypeDef {
            name: "OCSM_TEST_LT".into(),
            description: "test".into(),
            elements: vec![6.35, -3.175, 0.0, -3.175, 0.0, -3.175],
        }];
        assert_eq!(host.ensure_linetypes(lts.clone()), 1);
        let lt = host.document().line_types.get("OCSM_TEST_LT").unwrap();
        assert_eq!(lt.elements.len(), 6);
        assert!(!lt.handle.is_null());
        assert_eq!(host.ensure_linetypes(lts), 0);

        let styles = vec![ocs_plugin_api::host::TextStyleDef {
            name: "OCSM_TEST_STYLE".into(),
            font_file: "Unicode".into(),
            big_font_file: String::new(),
            true_type_font: "Zhuque Fangsong".into(),
            height: 3.5,
            width_factor: 0.7,
            annotative: true,
            is_shape_file: false,
            is_vertical: false,
        }];
        assert_eq!(host.ensure_text_styles(styles.clone()), 1);
        let s = host.document().text_styles.get("OCSM_TEST_STYLE").unwrap();
        assert_eq!(s.height, 3.5);
        assert_eq!(s.width_factor, 0.7);
        assert!(s.annotative);
        assert!(!s.handle.is_null());
        assert_eq!(host.ensure_text_styles(styles), 0);
    }

    #[test]
    fn v5_ensure_dim_styles_idempotent_and_sets_current() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        // 先建 OCSM_GB 文字样式，让 dimtxsty 能解析出真实句柄。
        {
            let mut host = HostSession::new(&mut app, 0);
            host.ensure_text_styles(vec![ocs_plugin_api::host::TextStyleDef {
                name: "OCSM_GB".into(),
                font_file: "Unicode".into(),
                big_font_file: String::new(),
                true_type_font: "Zhuque Fangsong".into(),
                height: 3.5,
                width_factor: 0.7,
                annotative: true,
                is_shape_file: false,
                is_vertical: false,
            }]);
        }
        let def = ocs_plugin_api::host::DimStyleDef {
            name: "OCSM_GB".into(),
            make_current: true,
            dimtxt: 2.5,
            dimasz: 2.5,
            dimcen: 2.5,
            dimexe: 2.0,
            dimexo: 0.0625,
            dimgap: 1.0,
            dimdli: 10.0,
            dimdle: 0.0,
            dimscale: 1.0,
            dimtad: 1,
            dimjust: 0,
            dimdec: 4,
            dimlunit: 2,
            dimzin: 8,
            dimaunit: 0,
            dimadec: 0,
            dimtih: false,
            dimtoh: true,
            dimtix: false,
            dimsoxd: false,
            dimtofl: true,
            dimtol: false,
            dimtp: 0.0,
            dimtm: 0.0,
            dimtdec: 2,
            dimclrd: 130,
            dimclre: 130,
            dimclrt: 3,
            dimlwd: -1,
            dimlwe: -1,
            dimtxsty: "OCSM_GB".into(),
            dimpost: String::new(),
            annotative: false,
        };
        let i = 0;
        {
            let mut host = HostSession::new(&mut app, i);
            assert_eq!(host.ensure_dim_styles(vec![def.clone()]), 1);
        }
        let doc = &app.tabs[i].scene.document;
        let style = doc.dim_styles.get("OCSM_GB").expect("style created");
        assert!(!style.handle.is_null(), "style must carry a real handle");
        assert_eq!(style.dimtxt, 2.5);
        assert_eq!(style.dimasz, 2.5);
        assert_eq!(style.dimexe, 2.0);
        assert_eq!(style.dimtad, 1);
        assert_eq!(style.dimtofl, true);
        assert_eq!(style.dimclrd, 130);
        assert_eq!(style.dimclrt, 3);
        assert_eq!(style.dimtxsty, "OCSM_GB");
        let th = doc
            .text_styles
            .get("OCSM_GB")
            .expect("text style exists")
            .handle;
        assert_eq!(style.dimtxsty_handle, th, "text-style handle resolved");
        assert_eq!(doc.header.current_dimstyle_name, "OCSM_GB");
        // 幂等 + 大小写不敏感。
        {
            let mut host = HostSession::new(&mut app, i);
            assert_eq!(host.ensure_dim_styles(vec![def.clone()]), 0);
            assert_eq!(
                host.ensure_dim_styles(vec![ocs_plugin_api::host::DimStyleDef {
                    name: "ocsm_gb".into(),
                    ..def.clone()
                }]),
                0
            );
        }
    }

    #[test]
    fn v5_inprocess_adapter_forwards_text_and_snap() {
        use ocs_plugin_api::host::InteractiveCommand as Ic;
        use std::sync::{Arc, Mutex};
        struct Spy {
            text: String,
            snapped: Option<bool>,
        }
        impl Ic for Spy {
            fn prompt(&self) -> String {
                "spy".into()
            }
            fn on_point(&mut self, _pt: [f64; 3]) -> ocs_plugin_api::host::CommandStep {
                ocs_plugin_api::host::CommandStep::NeedPoint
            }
            fn wants_text_input(&self) -> bool {
                true
            }
            fn on_text_input(&mut self, text: &str) -> ocs_plugin_api::host::CommandStep {
                self.text = text.to_string();
                ocs_plugin_api::host::CommandStep::NeedPoint
            }
            fn needs_object_pick(&self) -> bool {
                true
            }
            fn on_object_pick_snapped(
                &mut self,
                _handle: acadrust::Handle,
                pt: [f64; 3],
                snapped: bool,
            ) -> ocs_plugin_api::host::CommandStep {
                self.snapped = Some(snapped);
                let _ = pt;
                ocs_plugin_api::host::CommandStep::NeedPoint
            }
        }
        let spy = Arc::new(Mutex::new(Spy {
            text: String::new(),
            snapped: None,
        }));
        let inner = spy.clone();
        let mut adapter = PluginInteractiveAdapter {
            inner: Box::new(SpyBridge { spy: inner }),
            pending_snap: None,
        };
        use crate::command::CadCommand;
        assert!(adapter.wants_text_input());
        adapter.on_text_input("A");
        assert_eq!(spy.lock().unwrap().text, "A");
        // 注入 OSNAP 命中 → on_entity_pick 转发 snapped=true 且 pt 为命中点。
        adapter.inject_pick_snap(Some(glam::DVec3::new(5.0, 6.0, 0.0)));
        let _ = adapter.on_entity_pick(acadrust::Handle::new(7), glam::DVec3::new(1.0, 1.0, 0.0));
        assert_eq!(spy.lock().unwrap().snapped, Some(true));
        adapter.inject_pick_snap(None);
        let _ = adapter.on_entity_pick(acadrust::Handle::new(8), glam::DVec3::new(2.0, 2.0, 0.0));
        assert_eq!(spy.lock().unwrap().snapped, Some(false));

        /// Forward every InteractiveCommand call to the shared `Spy` so the
        /// test can observe what the adapter forwarded.
        struct SpyBridge {
            spy: Arc<Mutex<Spy>>,
        }
        impl Ic for SpyBridge {
            fn prompt(&self) -> String {
                "spy".into()
            }
            fn on_point(&mut self, pt: [f64; 3]) -> ocs_plugin_api::host::CommandStep {
                self.spy.lock().unwrap().on_point(pt)
            }
            fn wants_text_input(&self) -> bool {
                self.spy.lock().unwrap().wants_text_input()
            }
            fn on_text_input(&mut self, text: &str) -> ocs_plugin_api::host::CommandStep {
                self.spy.lock().unwrap().on_text_input(text)
            }
            fn needs_object_pick(&self) -> bool {
                self.spy.lock().unwrap().needs_object_pick()
            }
            fn on_object_pick_snapped(
                &mut self,
                handle: acadrust::Handle,
                pt: [f64; 3],
                snapped: bool,
            ) -> ocs_plugin_api::host::CommandStep {
                self.spy.lock().unwrap().on_object_pick_snapped(handle, pt, snapped)
            }
        }
    }

    /// 构造一个带线段 + `比例` ATTDEF 的图框文档（内存）。
    fn synthetic_frame_doc() -> acadrust::CadDocument {
        use acadrust::entities::{AttributeDefinition, Line};
        use acadrust::types::Vector3;
        let mut frame = acadrust::CadDocument::default();
        let mut line = Line::new();
        line.start = Vector3::ZERO;
        line.end = Vector3::new(420.0, 0.0, 0.0);
        line.common.handle = frame.allocate_handle();
        let _ = frame.add_entity(EntityType::Line(line));
        let mut ad = AttributeDefinition::new("比例".to_string(), "Scale".to_string(), " ".to_string());
        ad.common.handle = frame.allocate_handle();
        let _ = frame.add_entity(EntityType::AttributeDefinition(ad));
        frame
    }

    #[test]
    fn v5_import_frame_doc_as_block_defines_block_with_attdefs() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let frame = synthetic_frame_doc();
        {
            let mut host = HostSession::new(&mut app, 0);
            host.import_frame_doc_as_block(&frame, "A3横式图框")
                .expect("import should succeed");
        }
        let doc = &app.tabs[0].scene.document;
        let br = doc.block_records.get("A3横式图框").expect("block defined");
        assert!(!br.entity_handles.is_empty());
        // ATTDEF 保留在块内（按原顺序：线在前，ATTDEF 在后）。
        let attdefs: Vec<_> = br
            .entity_handles
            .iter()
            .filter_map(|h| match doc.get_entity(*h) {
                Some(EntityType::AttributeDefinition(ad)) => Some(ad.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(attdefs.len(), 1);
        assert_eq!(attdefs[0].tag, "比例");
    }

    #[test]
    fn v5_import_frame_block_round_trips_through_dwg_file() {
        use acadrust::io::dwg::DwgWriter;
        use ocs_plugin_api::host::ImportFrameBlockRequest;

        let frame = synthetic_frame_doc();
        let dir = std::env::temp_dir().join("ocs_ocsm_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("synthetic_frame.dwg");
        DwgWriter::write_to_file(&path, &frame).expect("write frame dwg");

        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let attdefs = {
            let mut host = HostSession::new(&mut app, 0);
            host.import_frame_block_impl(ImportFrameBlockRequest {
                path: path.to_string_lossy().into_owned(),
                block_name: "A3横式图框".into(),
            })
            .expect("import_frame_block_impl")
        };
        assert_eq!(attdefs.len(), 1);
        assert_eq!(attdefs[0].tag, "比例");
        // 幂等：重复导入不重复定义块。
        {
            let mut host = HostSession::new(&mut app, 0);
            let again = host
                .import_frame_block_impl(ImportFrameBlockRequest {
                    path: path.to_string_lossy().into_owned(),
                    block_name: "A3横式图框".into(),
                })
                .expect("reimport");
            assert_eq!(again.len(), 1);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v5_frame_picker_state_and_pending_selection() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let frames = vec![ocs_plugin_api::host::FrameItem {
            path: "/tmp/a.dwg".into(),
            label: "a".into(),
        }];
        {
            let mut host = HostSession::new(&mut app, 0);
            assert!(host.show_frame_picker(frames));
            assert!(host.take_pending_frame_selection().is_none());
        }
        assert!(matches!(
            app.active_modal,
            Some(crate::app::ModalKind::OcsmFramePicker)
        ));
        // 比例缺省 1:1；图框列表已加载。
        let picker = app.ocsm_frame_picker.as_ref().expect("picker state");
        assert_eq!(picker.frames.len(), 1);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.scale_v1, "1");
        assert_eq!(picker.scale_v2, "1");
        app.ocsm_pending_frame_selection = Some(ocs_plugin_api::host::FrameSelection {
            path: "/tmp/a.dwg".into(),
            scale_v1: 1,
            scale_v2: 2,
        });
        let sel = {
            let mut host = HostSession::new(&mut app, 0);
            host.take_pending_frame_selection().expect("pending selection")
        };
        assert_eq!(sel.scale_v1, 1);
        assert_eq!(sel.scale_v2, 2);
        {
            let mut host = HostSession::new(&mut app, 0);
            assert!(host.take_pending_frame_selection().is_none());
        }
    }

    #[test]
    fn xdata_record_round_trips_and_registers_appid() {
        let mut app = OpenCADStudio::new_for_test();
        let mut host = HostSession::new(&mut app, 0);
        let h = host.add_entity(EntityType::Point(Point::new()));

        let mut rec = ExtendedDataRecord::new("DEMO_SURVEY");
        rec.add_value(XDataValue::String("PNT-1".to_string()));
        rec.add_value(XDataValue::Integer32(42));
        assert!(host.write_record(h, rec));

        let got = host.read_record(h, "DEMO_SURVEY").expect("record missing");
        assert_eq!(got.values.len(), 2);
        // APPID registered so the XDATA survives a DWG/DXF round-trip.
        assert!(host.document().app_ids.contains("DEMO_SURVEY"));

        // A second write replaces rather than duplicates the record.
        let mut rec2 = ExtendedDataRecord::new("DEMO_SURVEY");
        rec2.add_value(XDataValue::String("PNT-2".to_string()));
        assert!(host.write_record(h, rec2));
        let got = host.read_record(h, "DEMO_SURVEY").unwrap();
        assert_eq!(got.values.len(), 1);

        // Removal reports whether anything was dropped.
        assert!(host.remove_record(h, "DEMO_SURVEY"));
        assert!(host.read_record(h, "DEMO_SURVEY").is_none());
        assert!(!host.remove_record(h, "DEMO_SURVEY"));
    }

    #[test]
    fn plugin_state_round_trips_through_hostapi_trait() {
        use ocs_plugin_api::host::{self, HostApi};
        let mut app = OpenCADStudio::new_for_test();
        let mut session = HostSession::new(&mut app, 0);
        let host: &mut dyn HostApi = &mut session;

        // Absent before first use.
        assert!(host::plugin_state::<u32>(&*host, "opencad.demo").is_none());
        // Insert via ensure, then mutate.
        *host::ensure_plugin_state(host, "opencad.demo", || 7u32) += 1;
        assert_eq!(
            *host::plugin_state::<u32>(&*host, "opencad.demo").unwrap(),
            8
        );
        *host::plugin_state_mut::<u32>(host, "opencad.demo").unwrap() = 100;
        assert_eq!(
            *host::plugin_state::<u32>(&*host, "opencad.demo").unwrap(),
            100
        );
    }

    #[test]
    fn add_entities_batch_assigns_handles_and_publishes_once() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        let pts: Vec<EntityType> = (0..5)
            .map(|i| {
                EntityType::Point(Point::at(acadrust::types::Vector3::new(
                    i as f64, i as f64, 0.0,
                )))
            })
            .collect();
        let handles = host.add_entities(pts);

        assert_eq!(handles.len(), 5);
        assert!(handles.iter().all(|h| !h.is_null()));
        // Each handle is unique.
        let mut set = std::collections::HashSet::new();
        for h in &handles {
            assert!(set.insert(h.value()));
        }
        // All entities are in the document.
        for h in &handles {
            assert!(host.document().get_entity(*h).is_some());
        }
    }

    #[test]
    fn update_entity_replaces_in_place_preserving_handle() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        let h = host.add_entity(EntityType::Point(Point::at(acadrust::types::Vector3::new(
            1.0, 1.0, 0.0,
        ))));
        let epoch_before = host.app.tabs[0].scene.geometry_epoch;

        // Edit a snapshot copy (as a plugin would) and commit it.
        let mut edited = host.document().get_entity(h).unwrap().clone();
        edited.common_mut().layer = "PLUGIN_EDIT".to_string();
        assert!(host.update_entity(edited));

        // Same handle, edit applied, geometry re-tessellated.
        let got = host.document().get_entity(h).expect("entity kept its handle");
        assert_eq!(got.common().layer, "PLUGIN_EDIT");
        assert_ne!(
            host.app.tabs[0].scene.geometry_epoch, epoch_before,
            "update should bump geometry"
        );

        // Updating an unknown handle fails and changes nothing.
        let mut ghost = Point::new();
        ghost.common.handle = Handle::new(999_999);
        assert!(!host.update_entity(EntityType::Point(ghost)));
    }

    #[test]
    fn add_and_update_auto_register_novel_layers_with_real_handles() {
        // A plugin adds/edits an entity naming a layer no LAYER command ever
        // created. The layer must gain a real table entry (non-null handle) so
        // it survives a DWG save instead of collapsing to layer 0 (#252, #67).
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);

        assert!(!host.document().layers.contains("PLUGIN-LAYER"));
        let mut pt = Point::at(acadrust::types::Vector3::new(3.0, 3.0, 0.0));
        pt.common.layer = "PLUGIN-LAYER".to_string();
        host.add_entity(EntityType::Point(pt));

        let layer = host
            .document()
            .layers
            .get("PLUGIN-LAYER")
            .expect("novel layer auto-registered on add_entity");
        assert!(
            !layer.handle.is_null(),
            "auto-registered layer must carry a real handle (#67)"
        );

        // Adding again on the same (now-existing) layer must not duplicate or
        // error — the always-present default "0" is likewise never re-created.
        let count_before = host.document().layers.len();
        host.add_entity(EntityType::Point(Point::new())); // default layer "0"
        let mut pt2 = Point::new();
        pt2.common.layer = "PLUGIN-LAYER".to_string();
        host.add_entity(EntityType::Point(pt2));
        assert_eq!(host.document().layers.len(), count_before);

        // Retargeting an entity to a novel layer via update_entity registers it.
        let h = host.add_entity(EntityType::Point(Point::new()));
        let mut edited = host.document().get_entity(h).unwrap().clone();
        edited.common_mut().layer = "EDIT-LAYER".to_string();
        assert!(host.update_entity(edited));
        assert!(host.document().layers.contains("EDIT-LAYER"));
    }

    #[test]
    fn remove_entity_deletes_and_clears_caches() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        let h = host.add_entity(EntityType::Point(Point::at(acadrust::types::Vector3::new(
            2.0, 2.0, 0.0,
        ))));
        assert!(host.document().get_entity(h).is_some());

        assert!(host.remove_entity(h));
        assert!(host.document().get_entity(h).is_none());
        assert!(!host.app.tabs[0].scene.hatches.contains_key(&h));
        assert!(!host.app.tabs[0].scene.meshes.contains_key(&h));

        // Removing an already-gone handle reports false.
        assert!(!host.remove_entity(h));
    }

    /// A plugin command: second point commits a Point and ends.
    struct PlacePoint {
        got_first: bool,
    }
    impl ocs_plugin_api::host::InteractiveCommand for PlacePoint {
        fn prompt(&self) -> String {
            crate::t!("Pick a point").into_owned()
        }
        fn on_point(&mut self, pt: [f64; 3]) -> ocs_plugin_api::host::CommandStep {
            use ocs_plugin_api::host::CommandStep;
            if self.got_first {
                let p = acadrust::entities::Point::at(acadrust::types::Vector3::new(
                    pt[0], pt[1], pt[2],
                ));
                CommandStep::CommitAndEnd(acadrust::EntityType::Point(p))
            } else {
                self.got_first = true;
                CommandStep::NeedPoint
            }
        }
    }

    #[test]
    fn plugin_interactive_command_drives_host_flow() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        {
            let mut host = HostSession::new(&mut app, 0);
            host.start_interactive(Box::new(PlacePoint { got_first: false }));
        }
        assert!(app.tabs[0].active_cmd.is_some());
        for pt in [glam::DVec3::new(0.0, 0.0, 0.0), glam::DVec3::new(5.0, 5.0, 0.0)] {
            let r = app.tabs[0].active_cmd.as_mut().unwrap().on_point(pt);
            let _ = app.apply_cmd_result(r);
        }
        assert_eq!(app.tabs[0].scene.document.entities().count(), 1);
        assert!(app.tabs[0].active_cmd.is_none(), "command should have ended");
    }

    /// A plugin command that picks an existing object, then marks it.
    struct PickThenMark;
    impl ocs_plugin_api::host::InteractiveCommand for PickThenMark {
        fn prompt(&self) -> String {
            crate::t!("Pick an object").into_owned()
        }
        fn on_point(&mut self, _pt: [f64; 3]) -> ocs_plugin_api::host::CommandStep {
            ocs_plugin_api::host::CommandStep::Cancel
        }
        fn needs_object_pick(&self) -> bool {
            true
        }
        fn on_object_pick(
            &mut self,
            _handle: acadrust::Handle,
            pt: [f64; 3],
        ) -> ocs_plugin_api::host::CommandStep {
            let p =
                acadrust::entities::Point::at(acadrust::types::Vector3::new(pt[0], pt[1], pt[2]));
            ocs_plugin_api::host::CommandStep::CommitAndEnd(acadrust::EntityType::Point(p))
        }
    }

    #[test]
    fn plugin_object_pick_routes_to_command() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let target = {
            let mut host = HostSession::new(&mut app, 0);
            let h = host.add_entity(acadrust::EntityType::Point(acadrust::entities::Point::at(
                acadrust::types::Vector3::new(3.0, 4.0, 0.0),
            )));
            host.start_interactive(Box::new(PickThenMark));
            h
        };
        // The command requested an entity pick, not a free point.
        assert!(app.tabs[0].active_cmd.as_ref().unwrap().needs_entity_pick());
        let r = app.tabs[0]
            .active_cmd
            .as_mut()
            .unwrap()
            .on_entity_pick(target, glam::DVec3::new(3.0, 4.0, 0.0));
        let _ = app.apply_cmd_result(r);
        // Original point + the mark the command committed.
        assert_eq!(app.tabs[0].scene.document.entities().count(), 2);
    }

    #[test]
    fn host_document_reader_sees_entities() {
        use ocs_plugin_api::host::ReaderEntityKind;
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        host.add_entity(acadrust::EntityType::Point(acadrust::entities::Point::at(
            acadrust::types::Vector3::new(7.0, 8.0, 0.0),
        )));
        let reader = host.document_reader();
        assert_eq!(reader.entity_count(), 1);
        let mut kinds = Vec::new();
        reader.for_each_entity(&mut |e| kinds.push(e.kind));
        assert_eq!(kinds, vec![ReaderEntityKind::Point]);
    }

    #[test]
    fn host_document_view_publish_and_read_shared() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        let info = host.document_view().unwrap();
        let reader =
            ocs_plugin_api::shm::SharedDocumentReader::<ocs_plugin_api::shm::DocumentViewData>::open(std::path::Path::new(&info.path))
                .unwrap();
        assert_eq!(reader.entity_count(), 0);

        host.add_entity(acadrust::EntityType::Point(acadrust::entities::Point::at(
            acadrust::types::Vector3::new(1.0, 2.0, 0.0),
        )));

        assert_eq!(reader.entity_count(), 1);
    }

    /// Read an entity handle from the live document, write XDATA for that
    /// handle, read it back, and remove it.
    #[test]
    fn document_reader_to_xdata_roundtrip() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        let h = host.add_entity(EntityType::Point(Point::at(acadrust::types::Vector3::new(
            7.0, 8.0, 0.0,
        ))));

        {
            let reader = host.document_reader();
            assert_eq!(reader.entity_count(), 1);
            let mut handles = Vec::new();
            reader.for_each_entity(&mut |e| handles.push(e.handle));
            assert_eq!(handles, vec![h]);
        }

        let mut rec = ExtendedDataRecord::new("ROUNDTRIP");
        rec.add_value(XDataValue::String("from-reader".to_string()));
        assert!(host.write_record(h, rec));

        let got = host.read_record(h, "ROUNDTRIP").expect("record missing");
        assert_eq!(got.values.len(), 1);
        assert!(matches!(got.values[0], XDataValue::String(ref s) if s == "from-reader"));

        assert!(host.remove_record(h, "ROUNDTRIP"));
        assert!(host.read_record(h, "ROUNDTRIP").is_none());
    }

    /// Publish a shared document view, read the entity handle from shared
    /// memory, then write and read-back XDATA through the normal HostApi RPCs.
    #[test]
    fn shared_document_view_read_then_write_xdata_roundtrip() {
        let mut app = OpenCADStudio::new_for_test();
        app.tabs[0].is_start = false;
        let mut host = HostSession::new(&mut app, 0);
        let info = host.document_view().unwrap();
        let reader =
            ocs_plugin_api::shm::SharedDocumentReader::<ocs_plugin_api::shm::DocumentViewData>::open(std::path::Path::new(&info.path))
                .unwrap();

        let h = host.add_entity(EntityType::Point(Point::at(acadrust::types::Vector3::new(
            1.0, 2.0, 0.0,
        ))));
        assert_eq!(reader.entity_count(), 1);

        let mut handles = Vec::new();
        reader.for_each_entity(&mut |e| handles.push(e.handle));
        assert_eq!(handles, vec![h]);

        let mut rec = ExtendedDataRecord::new("SHM_ROUNDTRIP");
        rec.add_value(XDataValue::Integer32(123));
        assert!(host.write_record(h, rec));

        let got = host
            .read_record(h, "SHM_ROUNDTRIP")
            .expect("record missing");
        assert_eq!(got.values.len(), 1);
        assert!(matches!(got.values[0], XDataValue::Integer32(123)));
    }
}
