struct Globais {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    luz: vec4<f32>,
    camera: vec4<f32>,
    vista: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globais: Globais;

struct SkyOut {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyOut {
    var triangle = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = triangle[vertex_index];
    var out: SkyOut;
    out.position = vec4<f32>(p, 1.0, 1.0);
    out.ndc = p;
    return out;
}

fn ray_direction(ndc: vec2<f32>) -> vec3<f32> {
    let near_h = globais.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let far_h = globais.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let near_p = near_h.xyz / max(abs(near_h.w), 0.00001);
    let far_p = far_h.xyz / max(abs(far_h.w), 0.00001);
    return normalize(far_p - near_p);
}

@fragment
fn fs_sky(input: SkyOut) -> @location(0) vec4<f32> {
    let ray = ray_direction(input.ndc);
    let up = clamp(ray.z * 0.5 + 0.5, 0.0, 1.0);
    let horizon = vec3<f32>(0.69, 0.79, 0.92);
    let zenith = vec3<f32>(0.12, 0.31, 0.58);
    let ground_haze = vec3<f32>(0.83, 0.84, 0.82);
    var sky = mix(horizon, zenith, pow(up, 0.72));
    if (ray.z < 0.0) {
        sky = mix(horizon, ground_haze, clamp(-ray.z * 1.8, 0.0, 1.0));
    }

    let sun_dir = normalize(globais.luz.xyz);
    let sun_dot = max(dot(ray, sun_dir), 0.0);
    let solar_disk = pow(sun_dot, 1800.0);
    let solar_glow = pow(sun_dot, 18.0);
    sky += vec3<f32>(1.0, 0.72, 0.34) * solar_glow * 0.20;
    sky += vec3<f32>(1.0, 0.94, 0.78) * solar_disk * 6.0;

    return vec4<f32>(sky, 1.0);
}
