// Пламя факела: язык огня с бегущим вверх шумом, раскалённое ядро и тёмно-красные края.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::globals,
};

struct Flame {
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> flame: Flame;

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.55 + vnoise(p * 2.1 + 3.7) * 0.3 + vnoise(p * 4.3 + 7.1) * 0.15;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = vec2<f32>(0.5);
    @if(VERTEX_UVS_A) {
        uv = in.uv;
    }
    let t = globals.time;
    let seed = in.world_position.x * 1.3 + in.world_position.z * 2.1;
    let h = 1.0 - uv.y; // 0 внизу, 1 вверху
    let x = uv.x - 0.5;
    // Шум, бегущий вверх, искажает язык пламени.
    let n = fbm(vec2<f32>(uv.x * 3.0 + seed, h * 2.5 - t * 3.2 + seed));
    let sway = (vnoise(vec2<f32>(t * 1.7 + seed, 0.0)) - 0.5) * 0.12 * h;
    let xx = x - sway - (n - 0.5) * 0.18 * h;
    // Профиль: капля, широкая снизу и острая сверху.
    let width = 0.30 * pow(max(h, 0.0), 0.35) * (1.0 - h) * 1.6;
    let body = 1.0 - smoothstep(width * 0.55, width, abs(xx));
    let fade = smoothstep(0.0, 0.08, h) * (1.0 - smoothstep(0.55 + n * 0.3, 0.95, h));
    let mask = clamp(body * fade, 0.0, 1.0);
    if mask < 0.003 {
        discard;
    }
    let core = (1.0 - smoothstep(0.0, width * 0.55, abs(xx))) * (1.0 - smoothstep(0.1, 0.55, h));
    var col = mix(vec3<f32>(1.0, 0.18, 0.02), vec3<f32>(1.0, 0.55, 0.12), mask);
    col = mix(col, vec3<f32>(1.0, 0.88, 0.55), core);
    let intensity = 7.0 * (0.85 + 0.3 * n);
    return vec4<f32>(col * mask * intensity, mask);
}
