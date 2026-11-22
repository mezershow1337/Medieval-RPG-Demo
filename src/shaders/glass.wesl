// Витражи: свинцовые переплёты, медальоны и роза. Светятся сами (HDR + блум).

import bevy_pbr::render::{
    forward_io::VertexOutput,
    mesh_view_bindings::globals,
};

struct Glass {
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> glass: Glass;

const PI: f32 = 3.14159265;

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(123.34, 456.21));
    q += dot(q, q + 45.32);
    return fract(q.x * q.y);
}

fn hash22(p: vec2<f32>) -> vec2<f32> {
    let n = hash21(p);
    return vec2<f32>(n, hash21(p + n * 17.17));
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash21(i), hash21(i + vec2<f32>(1.0, 0.0)), u.x), mix(hash21(i + vec2<f32>(0.0, 1.0)), hash21(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

// x — расстояние до границы клетки, y — id клетки.
fn cells(p: vec2<f32>) -> vec2<f32> {
    let i = floor(p);
    let f = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    var id = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let g = vec2<f32>(f32(x), f32(y));
            let o = hash22(i + g) * 0.7 + 0.15;
            let r = g + o - f;
            let d = dot(r, r);
            if d < d1 {
                d2 = d1;
                d1 = d;
                id = hash21(i + g);
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    return vec2<f32>(sqrt(d2) - sqrt(d1), id);
}

fn palette(i: f32) -> vec3<f32> {
    let k = i32(floor(fract(i) * 6.0));
    switch k {
        case 0: { return vec3<f32>(0.04, 0.12, 0.65); }  // синий
        case 1: { return vec3<f32>(0.70, 0.04, 0.06); }  // рубиновый
        case 2: { return vec3<f32>(0.85, 0.55, 0.08); }  // золотой
        case 3: { return vec3<f32>(0.04, 0.42, 0.18); }  // изумрудный
        case 4: { return vec3<f32>(0.35, 0.06, 0.50); }  // пурпурный
        default: { return vec3<f32>(0.08, 0.30, 0.70); } // лазурный
    }
}

// Возвращает (цвет, свинец 0..1).
fn lancet(uv: vec2<f32>) -> vec4<f32> {
    let p = uv * vec2<f32>(1.0, 3.2);
    var lead = 0.0;
    var col = vec3<f32>(0.0);
    // Кайма.
    let edge = min(min(uv.x, 1.0 - uv.x), 1.0 - uv.y);
    let in_border = edge < 0.12;
    if in_border {
        let k = floor(uv.y * 26.0);
        col = select(vec3<f32>(0.70, 0.04, 0.06), vec3<f32>(0.04, 0.12, 0.65), fract(k * 0.5) > 0.25);
        lead = max(lead, 1.0 - smoothstep(0.0, 0.02, abs(fract(uv.y * 26.0) - 0.5) - 0.46));
    } else {
        // Медальоны по высоте окна.
        let m = vec2<f32>(0.5, (floor(p.y * 1.1) + 0.5) / 1.1);
        let d = length((p - m) * vec2<f32>(1.0, 1.0));
        let r = 0.33;
        if d < r {
            let c = cells(p * 9.0);
            col = palette(c.y * 0.5 + floor(p.y * 1.1) * 0.37 + 0.1);
            // Сюжет медальона: светлая фигура в центре.
            let fig = smoothstep(0.13, 0.11, length((p - m) * vec2<f32>(1.6, 0.8)));
            col = mix(col, vec3<f32>(0.85, 0.75, 0.55), fig);
            lead = max(lead, 1.0 - smoothstep(0.03, 0.08, c.x));
        } else {
            let c = cells(p * 6.0);
            col = mix(vec3<f32>(0.05, 0.14, 0.55), vec3<f32>(0.10, 0.25, 0.60), c.y);
            if c.y > 0.82 {
                col = palette(c.y * 3.7);
            }
            lead = max(lead, 1.0 - smoothstep(0.03, 0.08, c.x));
        }
        lead = max(lead, 1.0 - smoothstep(0.0, 0.025, abs(d - r)));
        lead = max(lead, 1.0 - smoothstep(0.0, 0.02, abs(edge - 0.12)));
    }
    // Горизонтальные прутья-стяжки.
    lead = max(lead, 1.0 - smoothstep(0.0, 0.012, abs(fract(uv.y * 5.0) - 0.5) - 0.488));
    return vec4<f32>(col, lead);
}

fn rose(uv: vec2<f32>) -> vec4<f32> {
    let q = (uv - 0.5) * 2.0;
    let r = length(q);
    var a = atan2(q.y, q.x) / (2.0 * PI) + 0.5;
    var col = vec3<f32>(0.0);
    var lead = 0.0;
    let petals = 12.0;
    let sector = floor(a * petals);
    let fa = fract(a * petals) - 0.5;
    if r < 0.2 {
        // Центр: золотая звезда на синем.
        let star = smoothstep(0.02, 0.0, r - 0.12 - 0.05 * cos(atan2(q.y, q.x) * 8.0));
        col = mix(vec3<f32>(0.04, 0.12, 0.65), vec3<f32>(0.9, 0.62, 0.1), star);
        lead = 1.0 - smoothstep(0.0, 0.02, abs(r - 0.2));
    } else if r < 0.78 {
        // Лепестки — стрельчатые арки, расходящиеся от центра.
        let t = (r - 0.2) / 0.58;
        let petal = abs(fa) * 2.0 < (1.0 - t * t) * 0.92;
        let c = cells(vec2<f32>(a * 40.0, r * 12.0));
        if petal {
            col = palette(sector * 0.21 + t * 0.4 + c.y * 0.3);
        } else {
            col = palette(c.y * 0.5 + 0.55) * 0.8;
        }
        lead = 1.0 - smoothstep(0.0, 0.04, abs(abs(fa) * 2.0 - (1.0 - t * t) * 0.92));
        lead = max(lead, 1.0 - smoothstep(0.03, 0.08, c.x));
        lead = max(lead, 1.0 - smoothstep(0.0, 0.02, abs(r - 0.78)));
    } else {
        // Внешнее кольцо малых кругов.
        let ca = vec2<f32>(fract(a * 24.0) - 0.5, (r - 0.89) * 5.0);
        let d = length(ca);
        col = select(vec3<f32>(0.70, 0.04, 0.06), vec3<f32>(0.85, 0.55, 0.08), fract(floor(a * 24.0) * 0.5) > 0.25);
        col = select(vec3<f32>(0.04, 0.12, 0.65), col, d < 0.38);
        lead = 1.0 - smoothstep(0.0, 0.06, abs(d - 0.38));
        lead = max(lead, 1.0 - smoothstep(0.0, 0.03, abs(fract(a * 24.0) - 0.5) - 0.47));
    }
    return vec4<f32>(col, lead);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = vec2<f32>(0.5);
    @if(VERTEX_UVS_A) {
        uv = in.uv;
    }
    var g: vec4<f32>;
    if glass.params.x > 0.5 {
        g = rose(uv);
    } else {
        g = lancet(uv);
    }
    // Неровное стекло и медленное мерцание света за ним.
    let t = globals.time;
    let wp = in.world_position.xyz;
    let grain = 0.75 + 0.5 * vnoise(uv * 60.0 + wp.xz);
    let shimmer = 0.85 + 0.15 * sin(t * 0.7 + wp.x * 0.3 + wp.z * 0.2) * vnoise(uv * 4.0 + t * 0.1);
    let glow = glass.params.y * grain * shimmer;
    let color = g.rgb * glow * (1.0 - g.a) + vec3<f32>(0.006) * g.a;
    return vec4<f32>(color, 1.0);
}
