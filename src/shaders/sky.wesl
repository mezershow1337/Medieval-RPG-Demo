// Закатное небо: градиент, солнце с ореолом, подсвеченные облака, первые звёзды
// и силуэт далёких гор на горизонте.

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::{globals, view},
};

struct Sky {
    sun: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: Sky;

const PI: f32 = 3.14159265;

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
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        v += a * vnoise(q);
        q = q * 2.02 + vec2<f32>(5.3, 1.7);
        a *= 0.5;
    }
    return v;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz - view.world_position);
    let sun = normalize(sky.sun.xyz);
    let t = globals.time;
    let e = dir.y;
    let sd = max(dot(dir, sun), 0.0);
    // Насколько направление «со стороны солнца» по азимуту.
    let az_sun = max(dot(normalize(vec2<f32>(dir.x, dir.z) + 1e-5), normalize(sun.xz)), 0.0);

    // Градиент неба.
    let zenith = vec3<f32>(0.09, 0.11, 0.27);
    let mid = vec3<f32>(0.36, 0.29, 0.46);
    let horizon_warm = vec3<f32>(1.25, 0.55, 0.28);
    let horizon_cool = vec3<f32>(0.62, 0.48, 0.55);
    let horizon = mix(horizon_cool, horizon_warm, pow(az_sun, 3.0));
    let h = clamp(e, 0.0, 1.0);
    var col = mix(horizon, mid, smoothstep(0.0, 0.25, h));
    col = mix(col, zenith, smoothstep(0.2, 0.9, h));

    // Ореол и диск солнца.
    col += vec3<f32>(1.4, 0.65, 0.3) * pow(sd, 6.0) * 0.8;
    col += vec3<f32>(2.0, 1.1, 0.55) * pow(sd, 64.0);
    col += vec3<f32>(30.0, 20.0, 12.0) * smoothstep(0.9993, 0.9997, sd);

    // Звёзды в зените.
    if e > 0.25 {
        let sp = dir.xz / (e + 0.3) * 220.0;
        let cell = floor(sp);
        let star = hash21(cell);
        let fp = fract(sp) - 0.5;
        let tw = 0.6 + 0.4 * sin(t * 2.0 + star * 50.0);
        col += vec3<f32>(0.9, 0.9, 1.0) * step(0.985, star) * smoothstep(0.12, 0.0, length(fp)) * smoothstep(0.25, 0.7, e) * tw * 1.5;
    }

    // Облака на плоскости.
    if e > 0.0 {
        let cp = dir.xz / (e + 0.12) * 1.3 + vec2<f32>(t * 0.004, t * 0.002);
        let n = fbm(cp);
        let cover = smoothstep(0.48, 0.78, n) * smoothstep(0.0, 0.12, e);
        let lit = pow(az_sun, 2.0) * (0.4 + 0.6 * (1.0 - smoothstep(0.0, 0.5, e)));
        let thick = smoothstep(0.55, 0.95, n);
        let cloud_col = mix(vec3<f32>(0.20, 0.15, 0.24), vec3<f32>(1.5, 0.62, 0.32), lit * (1.0 - thick * 0.6));
        // Яркая кромка у облаков рядом с солнцем.
        let rim = pow(sd, 12.0) * (1.0 - thick) * 3.0;
        col = mix(col, cloud_col + vec3<f32>(1.6, 0.9, 0.5) * rim, cover);
    }

    // Силуэт далёких гор.
    let az = atan2(dir.z, dir.x);
    let ridge = 0.025 + 0.06 * fbm(vec2<f32>(az * 3.0, 1.0)) + 0.03 * fbm(vec2<f32>(az * 11.0, 5.0));
    let near_ridge = 0.01 + 0.035 * fbm(vec2<f32>(az * 6.0 + 9.0, 2.0));
    let haze = mix(vec3<f32>(0.30, 0.24, 0.34), vec3<f32>(0.75, 0.42, 0.30), pow(az_sun, 4.0));
    col = mix(col, haze, (1.0 - smoothstep(ridge - 0.003, ridge + 0.003, e)) * 0.85);
    col = mix(col, haze * 0.55, 1.0 - smoothstep(near_ridge - 0.003, near_ridge + 0.003, e));
    if e < 0.0 {
        // Ниже горизонта — цвет тумана, чтобы земля плавно уходила в дымку.
        col = vec3<f32>(0.133, 0.119, 0.181);
    }
    return vec4<f32>(col, 1.0);
}
