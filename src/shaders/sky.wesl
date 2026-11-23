// Купол неба. Само небо (градиент, солнце, облака, горы) — в sky_common.wesl:
// той же функцией считаются отражения в море и карта окружения.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view},
};
import medieval_rpg::shaders::sky_common::{SkyParams, sky_radiance, hash21};

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: SkyParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    var s = sky;
    s.zenith.w = globals.time;
    var col = sky_radiance(dir, s, globals.time, true);
    // Первые звёзды в вечернем зените.
    let e = dir.y;
    if s.sun.w < 0.5 && s.sun.y < 0.12 && e > 0.35 {
        let sp = dir.xz / (e + 0.3) * 260.0;
        let star = hash21(floor(sp));
        let fp = fract(sp) - 0.5;
        let tw = 0.6 + 0.4 * sin(globals.time * 2.0 + star * 50.0);
        col += vec3<f32>(0.9, 0.9, 1.0) * step(0.992, star) * smoothstep(0.12, 0.0, length(fp)) * smoothstep(0.45, 0.9, e) * tw * 0.5;
    }
    // Лёгкий шум против полос в плавном градиенте.
    col += (hash21(in.position.xy) - 0.5) / 255.0;
    return vec4<f32>(col, 1.0);
}
