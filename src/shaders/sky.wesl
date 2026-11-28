// Купол неба. Само небо (градиент, солнце, перистые облака, горы) — в sky_common.wesl:
// той же функцией считаются отражения в море и карта окружения.
//
// Днём кучевые облака объёмные. Их рисует отдельный проход (cloud_map.wesl) в карту всего
// неба — понемногу, полоска за кадр; купол эту карту только читает. Поэтому облака почти
// ничего не стоят в кадре и не рябят. Пока рисуется новая карта, облака в прежней плывут:
// купол сдвигает взгляд на тот путь, что ветер прошёл с момента её снимка. На закате и ночью —
// прежние облака из sky_common.wesl (закаты не трогаем); между ними плавный переход по
// высоте солнца.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view},
};
import medieval_rpg::shaders::sky_common::{SkyParams, sky_radiance_parts, mountain_map_uv, hash21, dome_uv};

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: SkyParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1)
var cloud_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2)
var cloud_map_sampler: sampler;

// origin: xyz — откуда снята карта облаков, w — 1, если карта готова (0 — облака плоские);
// drift: xy — на сколько метров ветер унёс облака с момента снимка, z — 1, если готова
// панорама гор.
struct CloudDome {
    origin: vec4<f32>,
    drift: vec4<f32>,
}
@group(constants::MATERIAL_BIND_GROUP) @binding(3)
var<uniform> cloud_dome: CloudDome;
// Панорама дальних гор (mountain_map.wesl): rgb — свет (×0,5, умноженный на покрытие), a — покрытие.
@group(constants::MATERIAL_BIND_GROUP) @binding(4)
var mountain_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(5)
var mountain_map_sampler: sampler;

// Высота, на которой сдвиг облаков ветром пересчитывается в сдвиг взгляда (низ облаков —
// то, что видно с земли).
const CLOUD_REF_Y: f32 = 1500.0;

// Объёмные кучевые из карты: rgb — свет облаков (уже умноженный на непрозрачность), a —
// непрозрачность.
fn volumetric_clouds(dir: vec3<f32>) -> vec4<f32> {
    if dir.y < 0.004 {
        return vec4<f32>(0.0);
    }
    let cam = view.world_position;
    let t = (CLOUD_REF_Y - cam.y) / max(dir.y, 0.03);
    let p = cam + dir * t + vec3<f32>(cloud_dome.drift.x, 0.0, cloud_dome.drift.y);
    let uv = dome_uv(normalize(p - cloud_dome.origin.xyz));
    // Пять отсчётов крестом: мелкое зерно карты (случайный сдвиг шагов луча) сглаживается.
    let px = 0.7 / vec2<f32>(textureDimensions(cloud_map));
    var v = textureSampleLevel(cloud_map, cloud_map_sampler, uv, 0.0) * 0.36;
    v += textureSampleLevel(cloud_map, cloud_map_sampler, uv + px, 0.0) * 0.16;
    v += textureSampleLevel(cloud_map, cloud_map_sampler, uv - px, 0.0) * 0.16;
    v += textureSampleLevel(cloud_map, cloud_map_sampler, uv + vec2<f32>(px.x, -px.y), 0.0) * 0.16;
    v += textureSampleLevel(cloud_map, cloud_map_sampler, uv + vec2<f32>(-px.x, px.y), 0.0) * 0.16;
    // В карте свет записан с множителем 0,4 (яркие кромки ярче единицы).
    return vec4<f32>(v.rgb * 2.5 * 1.2, v.a);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    var s = sky;
    s.zenith.w = globals.time;
    let day = clamp(s.sun.w, 0.0, 1.0) * (1.0 - clamp(s.extra.x, 0.0, 1.0));
    // Днём кучевые рисует объёмный слой, а не плоский из общей формулы.
    let vol_k = day * cloud_dome.origin.w;
    var cloud = 0.0;
    var col = sky_radiance_parts(dir, s, globals.time, true, &cloud, 1.0 - vol_k, false);
    if vol_k > 0.001 {
        let v = volumetric_clouds(dir);
        let tr = 1.0 - v.a * vol_k;
        col = col * tr + v.rgb * vol_k;
        cloud = max(cloud, 1.0 - tr);
    }
    // Горы — поверх облаков: у горизонта облака дальше гор.
    if dir.y >= -0.06 && cloud_dome.drift.z > 0.5 {
        let muv = mountain_map_uv(dir);
        if muv.z > 0.5 {
            let m = textureSampleLevel(mountain_map, mountain_map_sampler, muv.xy, 0.0);
            col = col * (1.0 - m.a) + m.rgb * 2.0;
            cloud = max(cloud, m.a);
        }
    }
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
