//! Request/response envelopes exchanged between the host and a plugin process.
//!
//! A single bidirectional socket is used. Each side sends either a request
//! (expecting a response) or a response (to a previous request). This lets the
//! host handle plugin RPCs inline while it waits for the result of a host→plugin
//! request such as `Dispatch`, avoiding the need for two sockets or threads.

use serde::{Deserialize, Serialize};

use crate::host::{CommandSource, CommandStep};
use crate::manifest::ApiVersion;
use crate::ribbon::owned::{OwnedPluginManifest, OwnedRibbonGroup};

pub use acadrust::xdata::ExtendedDataRecord;
pub use acadrust::{CadDocument, EntityType, Handle};

/// Events the host forwards to an active plugin `InteractiveCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractiveEvent {
    Point([f64; 3]),
    Enter,
    ObjectPick {
        handle: Handle,
        pt: [f64; 3],
        /// True when `pt` is an OSNAP result rather than the raw click point.
        /// The host only fills this for plugins that opt into object-pick
        /// snapping (see `InteractiveCommand::on_object_pick_snapped`); older
        /// plugin binaries never see it because the host sends the plain
        /// variant to them.
        snapped: bool,
    },
    /// Typed command-line text (keyword letter, value, or override).
    Text(String),
    /// Mouse moved to `pt` — the plugin returns a preview entity to render
    /// transiently (see `InteractiveCommand::on_mouse_move`).
    MouseMove([f64; 3]),
}

/// Initial handshake sent by the plugin runner immediately after connecting.
///
/// The runner proves it was spawned by this host by presenting a pre-shared
/// token delivered through the `OCS_PLUGIN_TOKEN` environment variable. A
/// mismatch causes the host to close the connection.
///
/// `TokenV4` is appended as the last variant so the existing `Token(String)`
/// variant keeps its bincode discriminant index (0), preserving V2/V3 wire
/// compatibility.
#[derive(Debug, Serialize, Deserialize)]
pub enum RunnerHandshake {
    Token(String),
    TokenV4 { token: String, protocol_version: u32 },
}

/// Environment variable through which the host passes the pre-shared
/// authentication token to the plugin runner child process.
pub const PLUGIN_TOKEN_ENV: &str = "OCS_PLUGIN_TOKEN";

/// Requests the host sends to the plugin runner.
#[derive(Debug, Serialize, Deserialize)]
pub enum HostRequest {
    GetManifest,
    GetRibbon,
    Dispatch {
        cmd: String,
    },
    InteractiveEvent {
        command_id: u64,
        event: InteractiveEvent,
    },
    GetPrompt {
        command_id: u64,
    },
    NeedsEntityPick {
        command_id: u64,
    },
    Shutdown,
    ExecuteCode {
        command_id: u64,
        source: CommandSource,
        code: String,
        tab_index: usize,
    },
    /// API v5: whether the interactive command wants typed text input.
    WantsTextInput {
        command_id: u64,
    },
    /// API v5: whether the interactive command wants mouse-move previews.
    WantsMouseMove {
        command_id: u64,
    },
    /// API v5: whether the interactive command wants object-snap at
    /// entity-pick clicks (POWERDIM toggles pick-point vs segment-select mode).
    EntityPickOsnap {
        command_id: u64,
    },
}

/// Responses the plugin runner sends back for `HostRequest`.
#[derive(Debug, Serialize, Deserialize)]
pub enum HostResponse {
    Bool(bool),
    CommandStep(Box<CommandStep>),
    Text(String),
    Ribbon(Vec<OwnedRibbonGroup>),
    Manifest(OwnedPluginManifest),
    Error(String),
    CodeExecutionResult(crate::host::ExecutionResult),
    /// API v5: the plugin's preview entity for the current cursor position.
    Preview(Option<EntityType>),
}

/// Requests the plugin runner sends to the host.
#[derive(Debug, Serialize, Deserialize)]
pub enum PluginRequest {
    PushInfo(String),
    PushOutput(String),
    PushError(String),
    AddEntity(EntityType),
    /// Replace the existing entity carrying this entity's handle in place.
    UpdateEntity(EntityType),
    /// Delete the entity with `handle`.
    RemoveEntity {
        handle: Handle,
    },
    BumpGeometry,
    ReadRecord {
        handle: Handle,
        app_name: String,
    },
    WriteRecord {
        handle: Handle,
        record: ExtendedDataRecord,
    },
    RemoveRecord {
        handle: Handle,
        app_name: String,
    },
    PushUndo {
        label: String,
    },
    SetDirty,
    StartInteractive {
        command_id: u64,
    },
    DocumentSnapshot,
    /// Ask the host to create/refresh a shared-memory document view and return
    /// the file path + current version.
    OpenDocumentView,
    /// Add multiple entities in a single request.
    AddEntities(Vec<EntityType>),
    /// V4: ask the host to create/refresh a tab-keyed shared-memory document
    /// view and return the file path + current version.
    OpenDocumentViewV4 { tab_id: u64 },
    /// V4: ask the host to close the tab-keyed shared-memory document view.
    CloseDocumentViewV4 { tab_id: u64 },
    /// V4: ask the host for the stable tab identifier of the active tab.
    GetTabId,
    /// API v5: handles of the currently selected entities.
    SelectedHandles,
    /// API v5: set the current layer by name.
    SetCurrentLayer(String),
    /// API v5: create missing layers.
    EnsureLayers(Vec<crate::host::LayerDef>),
    /// API v5: create missing linetypes.
    EnsureLinetypes(Vec<crate::host::LinetypeDef>),
    /// API v5: create missing text styles.
    EnsureTextStyles(Vec<crate::host::TextStyleDef>),
    /// API v5: open the host frame picker modal with the given frames.
    ShowFramePicker(Vec<crate::host::FrameItem>),
    /// API v5: take the pending frame selection, if any.
    TakePendingFrameSelection,
    /// API v5: load a frame DWG and define it as a block.
    ImportFrameBlock(crate::host::ImportFrameBlockRequest),
    /// API v5: create missing dimension styles.
    EnsureDimStyles(Vec<crate::host::DimStyleDef>),
    /// API v5: create a block definition whose members are `entities` (world
    /// coordinates, insertion at origin). Used for baked dimension graphics
    /// (anonymous `*D` blocks). Fails when the block already exists.
    AddBlockRecord {
        name: String,
        entities: Vec<EntityType>,
    },
}

/// Responses the host sends back for `PluginRequest`.
#[derive(Debug, Serialize, Deserialize)]
pub enum PluginResponse {
    Ok,
    Bool(bool),
    Handle(Handle),
    Record(Option<ExtendedDataRecord>),
    Document(Box<CadDocument>),
    Error(String),
    /// Path to the memory-mapped file and the current snapshot version.
    DocumentView {
        path: String,
        version: u64,
    },
    Handles(Vec<Handle>),
    /// V4: path to the tab-keyed memory-mapped file and current version.
    DocumentViewV4 {
        path: String,
        version: u64,
    },
    /// V4: stable tab identifier of the active tab.
    TabId(u64),
    /// API v5: count returned by `ensure_layers` / `ensure_linetypes` /
    /// `ensure_text_styles` (number of entries created).
    Count(usize),
    /// API v5: pending frame selection from the picker modal.
    FrameSelection(Option<crate::host::FrameSelection>),
    /// API v5: result of a frame block import (ATTDEFs in draw order).
    ImportFrameBlock(Result<Vec<acadrust::entities::AttributeDefinition>, String>),
}

/// Messages sent from the host to the plugin runner.
#[derive(Debug, Serialize, Deserialize)]
pub enum HostToPlugin {
    Request(HostRequest),
    Response(Box<PluginResponse>),
}

/// Messages sent from the plugin runner to the host.
#[derive(Debug, Serialize, Deserialize)]
pub enum PluginToHost {
    Request(Box<PluginRequest>),
    Response(HostResponse),
}

/// Convenience helper for manifest serialization.
impl From<&'static crate::manifest::PluginManifest> for OwnedPluginManifest {
    fn from(m: &'static crate::manifest::PluginManifest) -> Self {
        Self {
            id: m.id.to_string(),
            name: m.name.to_string(),
            version: m.version.to_string(),
            description: m.description.to_string(),
            api_version: m.api_version.major,
            ribbon_order: m.ribbon_order,
            xdata_apps: m.xdata_apps.iter().map(|s| s.to_string()).collect(),
            command_prefixes: m.command_prefixes.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl OwnedPluginManifest {
    pub fn api_version(&self) -> ApiVersion {
        ApiVersion {
            major: self.api_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lt() -> crate::host::LinetypeDef {
        crate::host::LinetypeDef {
            name: "X".into(),
            description: "d".into(),
            elements: vec![1.0, -2.0],
        }
    }

    fn layer() -> crate::host::LayerDef {
        crate::host::LayerDef {
            name: "A".into(),
            color: acadrust::types::Color::from_index(1),
            linetype: "Continuous".into(),
            lineweight: acadrust::types::LineWeight::from_value(25),
            plottable: true,
            off: false,
        }
    }

    fn style() -> crate::host::TextStyleDef {
        crate::host::TextStyleDef {
            name: "S".into(),
            font_file: "f".into(),
            big_font_file: String::new(),
            true_type_font: "t".into(),
            height: 3.5,
            width_factor: 0.7,
            annotative: true,
            is_shape_file: false,
            is_vertical: false,
        }
    }

    fn frame_item() -> crate::host::FrameItem {
        crate::host::FrameItem {
            path: "/a.dwg".into(),
            label: "a".into(),
        }
    }

    fn dim_style() -> crate::host::DimStyleDef {
        crate::host::DimStyleDef {
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
        }
    }

    #[test]
    fn v5_plugin_request_variants_round_trip_stably() {
        // bincode 序列化往返必须稳定（插件与宿主可能不同版本编译）。
        let reqs = vec![
            PluginRequest::SelectedHandles,
            PluginRequest::SetCurrentLayer("1轮廓实线层".into()),
            PluginRequest::EnsureLayers(vec![layer()]),
            PluginRequest::EnsureLinetypes(vec![lt()]),
            PluginRequest::EnsureTextStyles(vec![style()]),
            PluginRequest::ShowFramePicker(vec![frame_item()]),
            PluginRequest::TakePendingFrameSelection,
            PluginRequest::ImportFrameBlock(crate::host::ImportFrameBlockRequest {
                path: "/a.dwg".into(),
                block_name: "a".into(),
            }),
            PluginRequest::EnsureDimStyles(vec![dim_style()]),
        ];
        for req in reqs {
            let bytes = bincode::serialize(&req).unwrap();
            let back: PluginRequest = bincode::deserialize(&bytes).unwrap();
            assert_eq!(bincode::serialize(&back).unwrap(), bytes);
        }
    }

    #[test]
    fn v5_interactive_events_round_trip_with_snap_text_and_mouse() {
        let events = vec![
            InteractiveEvent::ObjectPick {
                handle: Handle::new(7),
                pt: [1.0, 2.0, 0.0],
                snapped: true,
            },
            InteractiveEvent::Text("A".into()),
            InteractiveEvent::MouseMove([3.0, 4.0, 0.0]),
            InteractiveEvent::Point([5.0, 6.0, 0.0]),
            InteractiveEvent::Enter,
        ];
        for ev in events {
            let bytes = bincode::serialize(&ev).unwrap();
            let back: InteractiveEvent = bincode::deserialize(&bytes).unwrap();
            assert_eq!(bincode::serialize(&back).unwrap(), bytes);
        }
    }

    #[test]
    fn v5_host_requests_wants_queries_round_trip() {
        let reqs = vec![
            HostRequest::WantsTextInput { command_id: 1 },
            HostRequest::WantsMouseMove { command_id: 2 },
            HostRequest::EntityPickOsnap { command_id: 3 },
        ];
        for req in reqs {
            let bytes = bincode::serialize(&req).unwrap();
            let back: HostRequest = bincode::deserialize(&bytes).unwrap();
            assert_eq!(bincode::serialize(&back).unwrap(), bytes);
        }
    }

    #[test]
    fn v5_preview_response_round_trips_entity() {
        let preview = HostResponse::Preview(Some(EntityType::Line(acadrust::entities::Line::new())));
        let bytes = bincode::serialize(&preview).unwrap();
        let back: HostResponse = bincode::deserialize(&bytes).unwrap();
        assert_eq!(bincode::serialize(&back).unwrap(), bytes);
        assert!(matches!(
            back,
            HostResponse::Preview(Some(EntityType::Line(_)))
        ));
    }

    #[test]
    fn v5_plugin_response_variants_round_trip_stably() {
        let resps = vec![
            PluginResponse::Count(3),
            PluginResponse::FrameSelection(Some(crate::host::FrameSelection {
                path: "p".into(),
                scale_v1: 1,
                scale_v2: 2,
            })),
            PluginResponse::FrameSelection(None),
            PluginResponse::ImportFrameBlock(Ok(vec![])),
            PluginResponse::ImportFrameBlock(Err("boom".into())),
        ];
        for resp in resps {
            let bytes = bincode::serialize(&resp).unwrap();
            let back: PluginResponse = bincode::deserialize(&bytes).unwrap();
            assert_eq!(bincode::serialize(&back).unwrap(), bytes);
        }
    }
}
