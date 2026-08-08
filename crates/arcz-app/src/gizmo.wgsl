struct Globais {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    luz: vec4<f32>,
    camera: vec4<f32>,
    vista: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globais: Globais;

struct GizmoIn {
    @location(0) position: vec3<f32>,
    @location(1) cor: vec4<f32>,
};

struct GizmoOut {
    @builtin(position) position: vec4<f32>,
    @location(0) cor: vec4<f32>,
};

@vertex
fn vs_gizmo(input: GizmoIn) -> GizmoOut {
    var out: GizmoOut;
    out.position = globais.view_proj * vec4<f32>(input.position, 1.0);
    out.cor = input.cor;
    return out;
}

@fragment
fn fs_gizmo(input: GizmoOut) -> @location(0) vec4<f32> {
    return input.cor;
}
