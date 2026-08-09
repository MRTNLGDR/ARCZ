//! Ponte de comunicação tipada entre a UI (Tauri 2 / React 19) e o Viewport wgpu nativo.
//!
//! Implementa os contratos do ADR-0002 e UI_ENGINE_CONTRACT.md para troca bidirecional de comandos
//! (UI -> Rust) e eventos (Rust -> UI).

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

/// Modo de operação do gizmo no viewport.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModoGizmo {
    #[default]
    Camera,
    Mover,
    Girar,
    Escalar,
}

/// Pose de câmera no espaço geoespacial ENU/WGS84.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraPose {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub altitude_m: f64,
    pub heading_deg: f32,
    pub pitch_deg: f32,
    pub distance_m: f32,
}

impl Default for CameraPose {
    fn default() -> Self {
        Self {
            lat_deg: -27.143256,
            lon_deg: -48.508924,
            altitude_m: 128.0,
            heading_deg: 125.25,
            pitch_deg: -28.0,
            distance_m: 800.0,
        }
    }
}

/// Limites físicos do viewport na janela.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ViewportBounds {
    pub x: u32,
    pub y: u32,
    pub largura: u32,
    pub altura: u32,
    pub dpi_scale: u32,
}

/// Ação enviada da UI para o Renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", content = "payload")]
pub enum UiToRendererAction {
    SetCamera(CameraPose),
    SelectNode {
        node_id: Option<u64>,
    },
    TransformNode {
        node_id: u64,
        delta_x: f32,
        delta_y: f32,
        delta_z: f32,
    },
    SetGizmoMode(ModoGizmo),
    ResizeViewport(ViewportBounds),
    ResetView,
    SetSnapping {
        ativo: bool,
        passo_grid_m: f32,
    },
}

/// Estado gerenciado pela ponte do renderer.
#[derive(Debug, Default)]
pub struct RendererBridgeState {
    pub camera: CameraPose,
    pub gizmo_modo: Mutex<ModoGizmo>,
    pub selecionado_id: Mutex<Option<u64>>,
    pub bounds: Mutex<Option<ViewportBounds>>,
    pub snapping_ativo: Mutex<bool>,
}

impl RendererBridgeState {
    pub fn new() -> Self {
        Self {
            camera: CameraPose::default(),
            gizmo_modo: Mutex::new(ModoGizmo::Camera),
            selecionado_id: Mutex::new(None),
            bounds: Mutex::new(None),
            snapping_ativo: Mutex::new(true),
        }
    }
}

/// Relatório de status da ponte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStatusReport {
    pub ok: bool,
    pub backend: String,
    pub camera: CameraPose,
    pub selecionado_id: Option<u64>,
    pub modo_gizmo: ModoGizmo,
}

/// Comando Tauri: Processa ação enviada da UI para o Renderer.
#[tauri::command]
pub fn ui_to_renderer(
    app: AppHandle,
    estado: State<'_, Arc<RendererBridgeState>>,
    acao: UiToRendererAction,
) -> Result<BridgeStatusReport, String> {
    match &acao {
        UiToRendererAction::SetCamera(pose) => {
            log::info!(
                "ui_to_renderer: SetCamera lat={:.5}, lon={:.5}",
                pose.lat_deg,
                pose.lon_deg
            );
            app.emit("viewport:camera_updated", pose)
                .map_err(|e| e.to_string())?;
        }

        UiToRendererAction::SelectNode { node_id } => {
            log::info!("ui_to_renderer: SelectNode {:?}", node_id);
            if let Ok(mut sel) = estado.selecionado_id.lock() {
                *sel = *node_id;
            }
            app.emit("viewport:node_selected", node_id)
                .map_err(|e| e.to_string())?;
        }
        UiToRendererAction::SetGizmoMode(modo) => {
            log::info!("ui_to_renderer: SetGizmoMode {:?}", modo);
            if let Ok(mut g) = estado.gizmo_modo.lock() {
                *g = *modo;
            }
        }
        UiToRendererAction::ResizeViewport(bounds) => {
            log::info!(
                "ui_to_renderer: ResizeViewport {}x{}",
                bounds.largura,
                bounds.altura
            );
            if let Ok(mut b) = estado.bounds.lock() {
                *b = Some(*bounds);
            }
        }
        UiToRendererAction::ResetView => {
            log::info!("ui_to_renderer: ResetView");
            let default_pose = CameraPose::default();
            app.emit("viewport:camera_updated", &default_pose)
                .map_err(|e| e.to_string())?;
        }
        UiToRendererAction::TransformNode {
            node_id,
            delta_x,
            delta_y,
            delta_z,
        } => {
            log::info!(
                "ui_to_renderer: TransformNode #{} d=({}, {}, {})",
                node_id,
                delta_x,
                delta_y,
                delta_z
            );
        }
        UiToRendererAction::SetSnapping {
            ativo,
            passo_grid_m,
        } => {
            log::info!(
                "ui_to_renderer: SetSnapping ativo={} passo={}m",
                ativo,
                passo_grid_m
            );
            if let Ok(mut s) = estado.snapping_ativo.lock() {
                *s = *ativo;
            }
        }
    }

    let modo = estado
        .gizmo_modo
        .lock()
        .map(|g| *g)
        .unwrap_or(ModoGizmo::Camera);
    let sel_id = estado.selecionado_id.lock().map(|s| *s).unwrap_or(None);

    Ok(BridgeStatusReport {
        ok: true,
        backend: "wgpu 27".to_string(),
        camera: estado.camera.clone(),
        selecionado_id: sel_id,
        modo_gizmo: modo,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_pose_default() {
        let pose = CameraPose::default();
        assert_eq!(pose.altitude_m, 128.0);
        assert_eq!(pose.heading_deg, 125.25);
    }

    #[test]
    fn test_bridge_state_initialization() {
        let state = RendererBridgeState::new();
        assert_eq!(*state.gizmo_modo.lock().unwrap(), ModoGizmo::Camera);
        assert_eq!(*state.selecionado_id.lock().unwrap(), None);
    }
}
