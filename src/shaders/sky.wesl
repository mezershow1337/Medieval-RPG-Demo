// Купол неба. Само небо (градиент, солнце, облака, горы) — в sky_common.wesl:
// той же функцией считаются отражения в море и карта окружения.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view},
};
import medieval_rpg::shaders::sky_common::{SkyParams, sky_radiance_cover, hash21};

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: SkyParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    var s = sky;
    s.zenith.w = globals.time;
    var cloud = 0.0;
    var col = sky_radiance_cover(dir, s, globals.time, true, &cloud);
    // Звёзды: первые — в вечернем зените, ночью — всё небо (за облаками не видно).
    let e = dir.y;
    let n = clamp(s.extra.x, 0.0, 1.0);
    let star_vis = max(n, select(0.0, 0.35, s.sun.w < 0.5 && s.sun.y < 0.12)) * (1.0 - clamp(s.extra.y, 0.0, 1.0));
    if star_vis > 0.0 && e > 0.04 {
        let sp = dir.xz / (e + 0.3) * 260.0;
        let star = hash21(floor(sp));
        let fp = fract(sp) - 0.5;
        let tw = 0.6 + 0.4 * sin(globals.time * 2.0 + star * 50.0);
        let mag = step(0.988, star) * (0.3 + 3.0 * pow((star - 0.988) / 0.012, 3.0));
        // Млечный путь: полоса мелких звёзд и бледное свечение.
        let band = exp(-pow((dir.x * 0.8 - dir.z * 0.6 + 0.15 * dir.y), 2.0) * 9.0);
        let dust = step(0.95 - 0.04 * band, hash21(floor(sp * 1.7) + 17.0)) * band * 0.25;
        // Звезда — пятнышко около пикселя (меньше — теряется при сглаживании кадра).
        var stars = vec3<f32>(0.9, 0.92, 1.0) * (mag * 1.6 + dust) * smoothstep(0.34, 0.04, length(fp)) * tw;
        stars += vec3<f32>(0.010, 0.011, 0.016) * band * n;
        // За облаками звёзд не видно.
        col += stars * smoothstep(0.04, 0.5, e) * star_vis * 0.5 * (1.0 - cloud);
    }
    // Лёгкий шум против полос в плавном градиенте.
    col += (hash21(in.position.xy) - 0.5) / 255.0;
    return vec4<f32>(col, 1.0);
}
