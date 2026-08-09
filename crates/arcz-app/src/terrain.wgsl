struct Globais {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    luz: vec4<f32>,
    camera: vec4<f32>,
    vista: vec4<f32>,
};

struct Material {
    base_color: vec4<f32>,
    flags: vec4<f32>,
};

struct Transform {
    modelo: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> globais: Globais;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var material_texture: texture_2d<f32>;
@group(1) @binding(2) var material_sampler: sampler;
@group(2) @binding(0) var<uniform> transform: Transform;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    let world = transform.modelo * vec4<f32>(input.position, 1.0);
    let normal_matrix = mat3x3<f32>(
        transform.modelo[0].xyz,
        transform.modelo[1].xyz,
        transform.modelo[2].xyz,
    );
    var out: VertexOut;
    out.clip_position = globais.view_proj * world;
    out.world_position = world.xyz;
    out.world_normal = normalize(normal_matrix * input.normal);
    out.uv = input.uv;
    return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    // Plano de corte da vista arquitetonica: tudo acima da cota fica fora.
    if (globais.vista.y > 0.5 && input.world_position.z > globais.vista.x) {
        discard;
    }

    var base = material.base_color;
    if (material.flags.x > 0.5) {
        base *= textureSample(material_texture, material_sampler, input.uv);
    }
    if (base.a < material.flags.y) {
        discard;
    }

    let n = normalize(input.world_normal);
    let l = normalize(globais.luz.xyz);
    // Imported CAD/glTF frequently contains inconsistent winding/normals. Keep
    // architectural surfaces readable from either side, matching the no-cull
    // model pipeline used by the Rust renderer.
    let diffuse = abs(dot(n, l));
    let ambient = clamp(globais.luz.w, 0.04, 1.0);
    let hemi = 0.10 + 0.12 * abs(n.z);
    var rgb = base.rgb * (ambient + hemi + diffuse * 0.82);

    // Vista 1 = planta humanizada; vista 2 = sketch tecnico.
    if (globais.vista.z > 1.5) {
        let edge = 1.0 - abs(dot(n, normalize(globais.camera.xyz - input.world_position)));
        let ink = smoothstep(0.55, 0.92, edge);
        let paper = vec3<f32>(0.94, 0.94, 0.91);
        rgb = mix(base.rgb * 0.72 + paper * 0.28, vec3<f32>(0.08), ink * 0.72);
    } else if (globais.vista.z > 0.5) {
        rgb = mix(rgb, base.rgb, 0.35);
    }

    return vec4<f32>(rgb, base.a);
}
