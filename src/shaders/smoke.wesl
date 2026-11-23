// Дым из очагов и горнов: столб, повёрнутый к камере, с поднимающимся шумом.
// Внизу узкий и плотный (с отсветом огня), выше расплывается и сносится ветром.
// Подсветка: со стороны заката дым светится тёплым (рассеяние вперёд), в тени — сизый.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view},
};
import medieval_rpg::shaders::sky_common::{vnoise, horizon_color, SkyParams};

struct Smoke {
    sky: SkyParams,
    // rgb — собственный цвет дыма (светлый древесный / тёмный угольный), w — плотность.
    tint: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> smoke: Smoke;

fn fbm3(p: vec2<f32>) -> f32 {
    return vnoise(p) * 0.55 + vnoise(p * 2.07 + 3.7) * 0.28 + vnoise(p * 4.3 + 7.1) * 0.17;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    // uv.x = номер столба × 4 + поперёк (0..1); uv.y = 0 у основания, 1 наверху.
    let seed = floor(in.uv.x / 4.0);
    let u = in.uv.x - seed * 4.0;
    let h = in.uv.y;
    let t = globals.time + seed * 13.7;

    // Ширина растёт с высотой, ось сносит ветром и слегка колышет.
    let sway = (vnoise(vec2<f32>(h * 2.0 - t * 0.15, seed)) - 0.5) * 0.25 * h;
    let center = 0.5 + h * h * 0.22 + sway;
    let width = mix(0.05, 0.42, pow(h, 0.7));
    let x = (u - center) / width;
    let profile = exp(-x * x * 2.2);

    // Клубы поднимаются: шум в координатах, бегущих вверх.
    let p = vec2<f32>(x * 0.9 + seed * 3.1, h * 5.0 - t * 0.45);
    let n = fbm3(p + vec2<f32>(fbm3(p * 0.7 + t * 0.05) * 1.2, 0.0));
    var dens = profile * smoothstep(0.25, 0.75, n + profile * 0.25);
    // Тает к верху, мягко начинается у основания.
    dens *= smoothstep(0.0, 0.06, h) * (1.0 - smoothstep(0.45, 1.0, h));
    dens *= smoke.tint.w;

    let sun = normalize(smoke.sky.sun.xyz);
    let v = normalize(in.world_position.xyz - view.world_position);
    // Рассеяние вперёд: против солнца дым светится.
    let fwd = pow(max(dot(v, sun), 0.0), 6.0);
    let hor = horizon_color(v, smoke.sky);
    let lit = mix(hor * 0.85, vec3<f32>(1.25, 0.8, 0.5), 0.35 + 0.35 * n) + vec3<f32>(1.6, 0.9, 0.4) * fwd * 0.6;
    var col = smoke.tint.rgb * lit;
    // Отсвет огня у основания.
    col += vec3<f32>(1.4, 0.45, 0.1) * (1.0 - smoothstep(0.0, 0.12, h)) * profile * 0.8;
    // Вдали дым тонет в дымке.
    let dist = length(in.world_position.xyz - view.world_position);
    col = mix(col, hor, smoothstep(150.0, 700.0, dist) * 0.7);
    return vec4<f32>(col, clamp(dens, 0.0, 0.85));
}
