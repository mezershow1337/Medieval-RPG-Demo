// Процедурные поверхности замка: тёсаная кладка, плиты пола, брусчатка, дерево,
// сланцевая черепица, ткань, металл, вода, угли, трава и листва.
// Узоры строятся по мировым координатам (трипланарно), поэтому любой меш
// без развёртки сразу выглядит как каменная кладка нужного масштаба.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::globals,
};

@if(PREPASS_PIPELINE)
import bevy_pbr::{
    prepass::io::{VertexOutput, FragmentOutput},
    deferred::functions::deferred_output,
};
@else
import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
};

struct Surface {
    color_b: vec4<f32>,
    params: vec4<f32>,
    params2: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> surface: Surface;

// ---------- шум ----------

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
    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        v += a * vnoise(q);
        q = q * 2.03 + vec2<f32>(1.7, 9.2);
        a *= 0.5;
    }
    return v;
}

// Вороной: x — расстояние до ближайшей точки, y — до второй, z — id клетки.
fn voronoi(p: vec2<f32>) -> vec3<f32> {
    let i = floor(p);
    let f = fract(p);
    var d1 = 8.0;
    var d2 = 8.0;
    var id = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let g = vec2<f32>(f32(x), f32(y));
            let o = hash22(i + g) * 0.8 + 0.1;
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
    return vec3<f32>(sqrt(d1), sqrt(d2), id);
}

// ---------- узоры ----------
// Каждый узор возвращает: x — высота рельефа (0..1), y — тон (0..1, смешение цветов),
// z — «шов» (1 в швах), w — случайное число элемента.

fn ashlar(p: vec2<f32>, size: vec2<f32>) -> vec4<f32> {
    var q = p / size;
    let row = floor(q.y);
    q.x += fract(row * 0.5) + hash21(vec2<f32>(row, 3.1)) * 0.3;
    let cell = floor(q);
    let f = fract(q);
    let e = min(min(f.x, 1.0 - f.x) * size.x, min(f.y, 1.0 - f.y) * size.y);
    let id = hash21(cell);
    let mortar = 0.022;
    let joint = 1.0 - smoothstep(mortar, mortar + 0.025, e);
    // Сколы и неровности камня.
    let chip = fbm(p * 7.0 + id * 13.0);
    let h = (1.0 - joint) * (0.75 + 0.25 * smoothstep(0.0, 0.08, e)) * (0.85 + 0.15 * chip);
    let tone = id * 0.7 + chip * 0.3;
    return vec4<f32>(h, tone, joint, id);
}

fn flagstone(p: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(1.2, 1.2);
    let cell = floor(p / size);
    let f = fract(p / size);
    let e = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y)) * size.x;
    let joint = 1.0 - smoothstep(0.015, 0.035, e);
    let id = hash21(cell);
    let checker = fract((cell.x + cell.y) * 0.5) * 2.0; // шахматный порядок
    let n = fbm(p * 3.0 + id * 7.0);
    let h = (1.0 - joint) * (0.9 + 0.1 * n);
    return vec4<f32>(h, checker * 0.75 + n * 0.25, joint, id);
}

fn cobble(p: vec2<f32>) -> vec4<f32> {
    let v = voronoi(p * 3.2);
    let edge = v.y - v.x;
    let joint = 1.0 - smoothstep(0.04, 0.16, edge);
    let h = smoothstep(0.0, 0.35, edge) * (1.0 - v.x * 0.6);
    return vec4<f32>(h, v.z * 0.7 + fbm(p * 6.0) * 0.3, joint, v.z);
}

fn planks(p: vec2<f32>) -> vec4<f32> {
    // Доски вдоль u.
    let w = 0.24;
    let row = floor(p.y / w);
    let off = hash21(vec2<f32>(row, 1.0)) * 7.0;
    let len = 2.4;
    let seg = floor((p.x + off) / len);
    let fu = fract((p.x + off) / len) * len;
    let fv = fract(p.y / w) * w;
    let e = min(min(fu, len - fu), min(fv, w - fv));
    let joint = 1.0 - smoothstep(0.006, 0.018, e);
    let id = hash21(vec2<f32>(row, seg));
    let grain = fbm(vec2<f32>(p.x * 1.5 + id * 10.0, p.y * 30.0));
    let rings = sin((p.y * 60.0 + grain * 8.0)) * 0.5 + 0.5;
    let h = (1.0 - joint) * (0.85 + 0.15 * grain);
    return vec4<f32>(h, mix(grain, rings, 0.35), joint, id);
}

fn slate(uv: vec2<f32>) -> vec4<f32> {
    // Ряды скруглённых чешуек; v идёт вниз по скату.
    let tile = vec2<f32>(0.32, 0.22);
    let row = floor(uv.y / tile.y);
    let u = uv.x / tile.x + fract(row * 0.5);
    let cell = vec2<f32>(floor(u), row);
    let fu = fract(u) - 0.5;
    let fv = fract(uv.y / tile.y);
    // Нижний край чешуйки скруглён.
    let edge = fv + (fu * fu) * 0.9;
    let joint = smoothstep(0.86, 0.98, edge) + (1.0 - smoothstep(0.0, 0.05, 0.5 - abs(fu))) * 0.6;
    let id = hash21(cell);
    let h = clamp(fv * 0.8 + 0.2 - joint * 0.6, 0.0, 1.0);
    return vec4<f32>(h, id, clamp(joint, 0.0, 1.0), id);
}

// Узор в 2D-проекции.
fn pattern2d(kind: i32, p: vec2<f32>, top: bool) -> vec4<f32> {
    let s = surface.params.y;
    switch kind {
        case 0: {
            if top {
                return ashlar(p * s, vec2<f32>(0.9, 0.6));
            }
            return ashlar(p * s, vec2<f32>(0.95, 0.42));
        }
        case 1: {
            if top {
                return flagstone(p * s);
            }
            return ashlar(p * s, vec2<f32>(0.9, 0.42));
        }
        case 2: {
            return planks(p * s);
        }
        case 3: {
            if top {
                return cobble(p * s);
            }
            return ashlar(p * s, vec2<f32>(0.5, 0.25));
        }
        case 4: {
            let n = fbm(p * 0.35);
            let blades = fbm(p * 12.0);
            return vec4<f32>(blades, smoothstep(0.35, 0.75, n), 0.0, n);
        }
        case 10: {
            let v = voronoi(p * 5.0);
            return vec4<f32>(1.0 - v.x, v.z * 0.6 + fbm(p * 4.0) * 0.4, smoothstep(0.5, 0.9, v.x), v.z);
        }
        default: {
            let n = fbm(p * 2.0);
            return vec4<f32>(n, n, 0.0, n);
        }
    }
}

struct Sample {
    tone: f32,
    joint: f32,
    id: f32,
    grad: vec3<f32>,
}

// Трипланарная выборка: узор по доминирующим осям нормали + градиент высоты для рельефа.
fn triplanar(kind: i32, wp: vec3<f32>, n: vec3<f32>) -> Sample {
    var w = pow(abs(n), vec3<f32>(6.0));
    w = w / (w.x + w.y + w.z);
    var out: Sample;
    out.tone = 0.0;
    out.joint = 0.0;
    out.id = 0.0;
    out.grad = vec3<f32>(0.0);
    let e = 0.012;
    if w.x > 0.02 {
        let p = wp.zy;
        let c = pattern2d(kind, p, false);
        let hu = pattern2d(kind, p + vec2<f32>(e, 0.0), false).x;
        let hv = pattern2d(kind, p + vec2<f32>(0.0, e), false).x;
        out.tone += c.y * w.x;
        out.joint += c.z * w.x;
        out.id += c.w * w.x;
        out.grad += vec3<f32>(0.0, hv - c.x, hu - c.x) / e * w.x;
    }
    if w.y > 0.02 {
        let p = wp.xz;
        let c = pattern2d(kind, p, true);
        let hu = pattern2d(kind, p + vec2<f32>(e, 0.0), true).x;
        let hv = pattern2d(kind, p + vec2<f32>(0.0, e), true).x;
        out.tone += c.y * w.y;
        out.joint += c.z * w.y;
        out.id += c.w * w.y;
        out.grad += vec3<f32>(hu - c.x, 0.0, hv - c.x) / e * w.y;
    }
    if w.z > 0.02 {
        let p = wp.xy;
        let c = pattern2d(kind, p, false);
        let hu = pattern2d(kind, p + vec2<f32>(e, 0.0), false).x;
        let hv = pattern2d(kind, p + vec2<f32>(0.0, e), false).x;
        out.tone += c.y * w.z;
        out.joint += c.z * w.z;
        out.id += c.w * w.z;
        out.grad += vec3<f32>(hu - c.x, hv - c.x, 0.0) / e * w.z;
    }
    return out;
}

fn perturb(n: vec3<f32>, grad: vec3<f32>, strength: f32) -> vec3<f32> {
    let g = grad - n * dot(grad, n);
    return normalize(n - g * strength * 0.02);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let kind = i32(surface.params.x + 0.5);
    let variation = surface.params.z;
    let bump = surface.params.w;
    let moss_amount = surface.params2.x;
    let grime_amount = surface.params2.y;
    let glow = surface.params2.z;
    let wp = in.world_position.xyz;
    let n = normalize(pbr_input.world_normal);
    let base_a = pbr_input.material.base_color.rgb;
    let base_b = surface.color_b.rgb;

    var color = base_a;
    var rough = pbr_input.material.perceptual_roughness;
    var N = pbr_input.N;
    var emissive = vec3<f32>(0.0);

    if kind == 5 || kind == 6 || kind == 7 || kind == 8 || kind == 9 {
        // Узоры, завязанные на UV или особые.
        var uv = vec2<f32>(0.0);
        @if(VERTEX_UVS_A) {
            uv = in.uv;
        }
        switch kind {
            case 5: {
                // Знамя: ткань, золотая кайма, геральдический крест.
                let weave = sin(uv.x * 900.0) * sin(uv.y * 900.0) * 0.05;
                let folds = sin(uv.x * 18.0 + fbm(uv * 3.0) * 3.0) * 0.5 + 0.5;
                let border = step(min(min(uv.x, 1.0 - uv.x), uv.y), 0.07);
                let c = uv - vec2<f32>(0.5, 0.42);
                let emblem = step(abs(c.x), 0.07) * step(abs(c.y), 0.25) + step(abs(c.y), 0.06) * step(abs(c.x), 0.22);
                let gold = clamp(border + emblem, 0.0, 1.0);
                // Ласточкин хвост внизу.
                let tail = step(0.92 - abs(uv.x - 0.5) * 0.25, uv.y);
                if tail > 0.5 && abs(uv.x - 0.5) < 0.18 {
                    discard;
                }
                color = mix(base_a * (0.7 + 0.3 * folds), base_b, gold) * (1.0 + weave);
                rough = mix(0.9, 0.45, gold);
                pbr_input.material.metallic = gold * 0.8;
                N = perturb(N, vec3<f32>(cos(uv.x * 18.0) * 2.0, 0.0, 0.0), 0.5);
            }
            case 6: {
                let s = slate(uv);
                color = mix(base_a, base_a * 1.35, s.y * variation * 3.0);
                color = mix(color, base_b, s.z * 0.8);
                // Лишайник на черепице.
                let lichen = smoothstep(0.62, 0.8, fbm(uv * 1.3 + s.w));
                color = mix(color, vec3<f32>(0.32, 0.33, 0.18), lichen * moss_amount);
                rough = mix(0.45, 0.85, s.z + lichen);
                let e = 0.01;
                let hu = slate(uv + vec2<f32>(e, 0.0)).x - s.x;
                let hv = slate(uv + vec2<f32>(0.0, e)).x - s.x;
                // Направления u и v на скате приблизительно: u — горизонтально, v — вниз по скату.
                let t = normalize(cross(n, vec3<f32>(0.0, 1.0, 0.0)) + vec3<f32>(1e-4, 0.0, 0.0));
                let b = cross(t, n);
                N = perturb(N, (t * hu + b * hv) / e, bump);
            }
            case 7: {
                // Металл: кованые пятна и потёртости.
                let nn = fbm(wp.xz * 6.0 + wp.y * 4.0);
                let rust = smoothstep(0.55, 0.8, fbm(wp.xy * 3.0 + wp.zz * 2.0));
                color = mix(base_a * (0.8 + 0.4 * nn), base_b, rust * variation * 2.0);
                rough = clamp(rough + (nn - 0.5) * 0.3 + rust * 0.4, 0.1, 1.0);
                pbr_input.material.metallic = pbr_input.material.metallic * (1.0 - rust * 0.7);
            }
            case 8: {
                // Вода: тёмная, с бегущей рябью.
                let t = globals.time;
                let p = wp.xz;
                let g = vec3<f32>(
                    sin(p.x * 7.0 + t * 1.7) + sin(p.x * 3.1 - p.y * 4.3 + t * 1.1),
                    0.0,
                    cos(p.y * 6.3 - t * 1.3) + sin(p.y * 2.7 + p.x * 5.1 + t * 0.9),
                );
                N = normalize(N + g * 0.035);
                color = mix(base_a, base_b, fbm(p * 2.0 + t * 0.1));
                rough = 0.04;
            }
            default: {
                // Угли: тёмная корка и пульсирующий жар в трещинах.
                let t = globals.time;
                let v = voronoi(wp.xz * 9.0);
                let crack = 1.0 - smoothstep(0.02, 0.12, v.y - v.x);
                let pulse = 0.6 + 0.4 * sin(t * 2.3 + v.z * 20.0) * fbm(wp.xz * 4.0 + t * 0.4);
                color = base_a * (0.6 + 0.4 * v.z);
                emissive = base_b * glow * (crack * 0.8 + 0.25) * pulse;
            }
        }
    } else {
        let s = triplanar(kind, wp, n);
        var tone = s.tone;
        color = base_a * (1.0 + (tone - 0.5) * variation * 2.0);
        // Лёгкий тёплый/холодный оттенок у разных камней.
        color *= mix(vec3<f32>(1.04, 1.0, 0.95), vec3<f32>(0.95, 0.98, 1.05), fract(s.id * 7.31));
        if kind == 1 {
            color = mix(base_a * (0.9 + 0.2 * s.id), base_b * 1.6, step(0.62, tone));
        }
        if kind == 4 {
            color = mix(base_a, base_b, tone);
            color *= 0.8 + 0.4 * s.id;
        }
        if kind == 10 {
            color = mix(base_a, base_b, tone) * (0.7 + 0.5 * (1.0 - s.joint));
        }
        color = mix(color, base_b, s.joint * 0.9);
        rough = clamp(rough + s.joint * 0.15 + (tone - 0.5) * 0.15, 0.05, 1.0);
        N = perturb(N, s.grad, bump);

        // Мох в швах и на верхних гранях, грязь и сырость у земли.
        let up = clamp(n.y, 0.0, 1.0);
        let large = fbm(wp.xz * 0.15 + wp.y * 0.05);
        let moss = smoothstep(0.55, 0.75, large + up * 0.35 + s.joint * 0.25 - clamp(wp.y - 1.5, 0.0, 6.0) * 0.06) * moss_amount;
        color = mix(color, vec3<f32>(0.14, 0.19, 0.07) * (0.8 + 0.4 * fbm(wp.xz * 3.0)), moss);
        let ground = 1.0 - smoothstep(0.0, 1.6, wp.y + fbm(wp.xz * 0.8 + wp.zx) * 0.6);
        let streaks = smoothstep(0.4, 0.8, fbm(vec2<f32>(wp.x + wp.z, wp.y * 0.08) * 1.7)) * (1.0 - up);
        color *= 1.0 - (ground * 0.45 + streaks * 0.25) * grime_amount;
        rough = mix(rough, 0.35, ground * grime_amount * 0.5);
    }

    pbr_input.material.base_color = vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
    pbr_input.material.perceptual_roughness = rough;
    pbr_input.N = N;
    pbr_input.material.emissive = vec4<f32>(emissive, 1.0);

    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );

    var out: FragmentOutput;
@if(PREPASS_PIPELINE) {
    out = deferred_output(in, pbr_input);
} @else {
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
}
    return out;
}
