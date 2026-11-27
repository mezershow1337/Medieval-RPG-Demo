// Процедурные поверхности замка: тёсаная кладка, плиты пола, брусчатка, дерево,
// сланцевая черепица, ткань, металл, вода, угли, трава и листва.
// Узоры строятся по мировым координатам (трипланарно), поэтому любой меш
// без развёртки сразу выглядит как каменная кладка нужного масштаба.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::{globals, view},
};
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::mesh_view_bindings::lights;

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
    // Фотоматериал (ambientCG, CC0): x — слой сверху (−1 — нет), y — 1/м, z — слой сбоку, w — 1/м.
    photo: vec4<f32>,
    // rgb — средний цвет верхнего слоя, w — сила перекраски.
    photo_mean: vec4<f32>,
    // rgb — средний цвет бокового слоя, w — сила нормалей фото.
    photo_mean_b: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> surface: Surface;
@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var photo_albedo: texture_2d_array<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var photo_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103)
var photo_nrm: texture_2d_array<f32>;

// ---------- фотоматериалы ----------

struct Photo {
    albedo: vec3<f32>, // линейный цвет
    n: vec3<f32>,      // мировая нормаль с рельефом
    rough: f32,
}

// ---------- объёмный рельеф (параллакс по карте высот) ----------
// Высота слоя лежит в альфа-канале цвета (1 — выступ, 0 — дно шва). Луч взгляда идёт внутрь
// поверхности, пока не упрётся в рельеф: выступы закрывают то, что за ними, а к солнцу
// от них ложатся мелкие тени — черепица, кладка и булыжник читаются объёмными.

// Взгляд (от точки к камере), направление на солнце, сила эффекта (гаснет с расстоянием)
// и вес текущей проекции — задаются во фрагментном шейдере перед выборками.
var<private> pom_view: vec3<f32>;
var<private> pom_sun: vec3<f32>;
var<private> pom_k: f32;
var<private> pom_gate: f32;
// Близость к камере для мелкой фактуры: 1 — вплотную, 0 — дальше 14 м.
var<private> pom_near: f32;

// Глубина рельефа слоя, м. Номера — из surface_layers.rs.
fn layer_depth(layer: i32) -> f32 {
    switch layer {
        case 3 { return 0.1; }          // черепица: калиптеры над тегулами
        case 1 { return 0.06; }           // бутовая кладка
        case 10 { return 0.045; }         // булыжник
        case 0 { return 0.03; }           // тёсаные блоки
        case 13 { return 0.022; }         // кирпич
        case 11 { return 0.02; }          // плиты
        case 19 { return 0.035; }         // солома
        case 9 { return 0.07; }           // скала
        case 18 { return 0.04; }          // галька
        case 6, 8 { return 0.03; }        // земля
        case 7 { return 0.02; }          // трава
        case 4 { return 0.018; }          // доски
        case 5 { return 0.012; }          // брус
        case 2 { return 0.008; }          // штукатурка
        default { return 0.0; }
    }
}

fn photo_height(uv: vec2<f32>, layer: i32, lod: f32) -> f32 {
    return textureSampleLevel(photo_albedo, photo_sampler, uv, layer, lod).a;
}

// Одна проекция: цвет, шероховатость и нормаль, развёрнутая в мир по осям t (u) и b (v вверх по картинке).
// `scale` — сколько единиц uv в одном метре.
fn photo_plane(layer: i32, uv0: vec2<f32>, t: vec3<f32>, b: vec3<f32>, n: vec3<f32>, relief: f32, scale: f32) -> Photo {
    var o: Photo;
    let dx = dpdx(uv0);
    let dy = dpdy(uv0);
    var uv = uv0;
    var shade = 1.0;
    let depth = layer_depth(layer) * pom_k * pom_gate;
    if depth > 0.004 {
        let size = f32(textureDimensions(photo_albedo).x);
        let lod = max(0.5 * log2(max(dot(dx, dx), dot(dy, dy)) * size * size), 0.0);
        let vz = max(dot(pom_view, n), 0.0);
        // Под скользящим углом сдвиг ограничен, иначе рисунок «плывёт».
        let slide = vec2<f32>(dot(pom_view, t), -dot(pom_view, b)) / max(vz, 0.3);
        let span = -slide * depth * scale;
        let steps = i32(mix(16.0, 7.0, vz));
        let dl = 1.0 / f32(steps);
        var d = 0.0;
        var prev_gap = 0.0;
        var gap = 0.0;
        var hit = 1.0;
        for (var i = 0; i <= steps; i++) {
            gap = (1.0 - photo_height(uv0 + span * d, layer, lod)) - d;
            if gap <= 0.0 {
                // Точное место — между двумя последними шагами.
                let k = prev_gap / max(prev_gap - gap, 1e-4);
                hit = d - dl * (1.0 - k);
                break;
            }
            prev_gap = gap;
            d += dl;
        }
        hit = clamp(hit, 0.0, 1.0);
        uv = uv0 + span * hit;
        // Тень от выступов: от найденной точки идём к солнцу и смотрим, что выше луча.
        let sz = dot(pom_sun, n);
        if sz > 0.02 && pom_k > 0.35 && hit > 0.01 {
            let to_sun = vec2<f32>(dot(pom_sun, t), -dot(pom_sun, b)) / max(sz, 0.12) * depth * scale;
            let h0 = 1.0 - hit;
            var occ = 0.0;
            for (var i = 1; i <= 6; i++) {
                let f = f32(i) / 6.0;
                // Луч поднимается от h0 до верха рельефа.
                let ray_h = h0 + (1.0 - h0) * f;
                let hh = photo_height(uv + to_sun * (1.0 - h0) * f, layer, lod);
                occ = max(occ, (hh - ray_h) * (1.3 - f));
            }
            // На мягком (трава, земля) тени от «выступов» слабые: там рельеф — комки и стебли, не камень.
            // У скалы щели широкие: в полную силу тень от них ложилась чёрными кляксами.
            let soft = select(select(1.0, 0.45, layer == 9), 0.3, layer == 7 || layer == 6 || layer == 8);
            shade = 1.0 - clamp(occ * 8.0, 0.0, 1.0) * 0.55 * soft * smoothstep(0.35, 0.6, pom_k);
        }
    }
    let tex = textureSampleGrad(photo_albedo, photo_sampler, uv, layer, dx, dy);
    // Швы и щели темнее выступов (туда не попадает рассеянный свет): рисунок кладки
    // читается и на ярком солнце, а не только в тени.
    let cavity = mix(1.0, smoothstep(-0.25, 0.55, tex.a), clamp(layer_depth(layer) * 22.0, 0.0, 0.75) * select(1.0, 0.4, layer == 9));
    o.albedo = tex.rgb * shade * cavity;
    if layer == 19 {
        // Камыш: убираем «лоскуты» светлых и тёмных пучков (издали они читались подушками),
        // оставляем стебли и ряды снопов.
        let low = textureSampleLevel(photo_albedo, photo_sampler, uv, layer, 5.0).rgb;
        let mean = textureSampleLevel(photo_albedo, photo_sampler, uv, layer, 10.0).rgb;
        let flat_c = tex.rgb / max(low, vec3<f32>(0.03)) * mean;
        o.albedo = mix(tex.rgb, flat_c, 0.7) * mix(1.0, shade * cavity, 0.6);
    }
    let m = textureSampleGrad(photo_nrm, photo_sampler, uv, layer, dx, dy);
    var xy = (m.xy * 2.0 - 1.0) * relief;
    // Мелкая фактура вблизи: тот же материал в 4,7 раза мельче (у кладки и плит — зерно скалы)
    // добавляет поры, зёрна и волокна, которых в текстуре 1024 при подходе вплотную не хватает.
    let metal = layer == 14 || layer == 15 || layer == 16 || layer == 17;
    if pom_near > 0.02 && !metal && layer != 19 {
        let dl = select(layer, 9, layer_has_grid(layer) || layer == 3);
        let duv = uv * 4.7 + vec2<f32>(0.37, 0.11);
        let fine = textureSampleGrad(photo_albedo, photo_sampler, duv, dl, dx * 4.7, dy * 4.7).rgb;
        let coarse = textureSampleLevel(photo_albedo, photo_sampler, duv, dl, 5.0).rgb;
        let lw = vec3<f32>(0.3, 0.5, 0.2);
        let ratio = clamp(dot(fine, lw) / max(dot(coarse, lw), 0.02), 0.55, 1.6);
        o.albedo *= mix(1.0, ratio, 0.6 * pom_near);
        let fn2 = textureSampleGrad(photo_nrm, photo_sampler, duv, dl, dx * 4.7, dy * 4.7).xy * 2.0 - 1.0;
        xy += fn2 * relief * 0.7 * pom_near;
    }
    let z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    o.n = t * xy.x + b * xy.y + n * z;
    o.rough = m.z;
    return o;
}

// Слои с правильным рисунком (кладка, плиты, брусчатка, доски, кирпич, ткань): повёрнутая
// копия поверх них даёт косые «призрачные» швы и камни другого размера, поэтому для них
// вторую выборку не подмешиваем. Номера — из surface_layers.rs.
fn layer_has_grid(layer: i32) -> bool {
    return layer == 0 || layer == 1 || layer == 4 || layer == 5 || layer == 10 || layer == 11 || layer == 13 || layer == 14;
}

// Сверху — две выборки: обычная и повёрнутая на ~37° крупнее, смешанные по мягкому шуму,
// чтобы на больших площадях (трава, земля, галька) не было видно повтора тайла.
fn photo_top(layer: i32, inv: f32, wp: vec3<f32>, n: vec3<f32>, relief: f32, sy: f32) -> Photo {
    let gate = pom_gate;
    let grid = layer_has_grid(layer);
    let k = select(smoothstep(0.35, 0.65, fbm(wp.xz * 0.11)), 0.0, grid);
    pom_gate = gate * (1.0 - k);
    let a = photo_plane(layer, vec2<f32>(wp.x, -wp.z * sy) * inv, vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, sy), n, relief, inv);
    pom_gate = gate * k;
    let r = mat2x2<f32>(0.8, 0.6, -0.6, 0.8);
    let q = r * vec2<f32>(wp.x, -wp.z * sy) * inv * 0.61 + vec2<f32>(0.37, 0.71);
    let tb = r * vec2<f32>(1.0, 0.0);
    let bb = r * vec2<f32>(0.0, 1.0);
    let b = photo_plane(layer, q, vec3<f32>(tb.x, 0.0, -tb.y * sy), vec3<f32>(bb.x, 0.0, -bb.y * sy), n, relief, inv * 0.61);
    pom_gate = gate;
    // Обе выборки читаются всегда (номер слоя зависит от нормали — ветвиться по нему нельзя),
    // у слоёв с рисунком просто берём только первую.
    var o: Photo;
    o.albedo = mix(a.albedo, b.albedo, k);
    o.n = mix(a.n, b.n, k);
    o.rough = mix(a.rough, b.rough, k);
    return o;
}

// Трипланарная выборка фото: сверху слой `top`, на стенах и снизу — `side`.
// Все проекции читаются всегда (без ветвлений по нормали): выборка текстуры с мипами
// разрешена только в однородном потоке управления.
fn photo_triplanar(top: i32, side: i32, inv_top: f32, inv_side: f32, wp: vec3<f32>, n: vec3<f32>, relief: f32) -> Photo {
    var w = pow(abs(n), vec3<f32>(4.0));
    w = w / (w.x + w.y + w.z);
    let sx = select(-1.0, 1.0, n.x >= 0.0);
    let sy = select(-1.0, 1.0, n.y >= 0.0);
    let sz = select(-1.0, 1.0, n.z >= 0.0);
    // Картинка «вверх» — мировой +Y на стенах; v в текстуре растёт вниз, отсюда минус.
    // Рельеф считаем только для проекций, которые здесь действительно видны.
    pom_gate = smoothstep(0.1, 0.4, w.x);
    let px = photo_plane(side, vec2<f32>(-wp.z * sx, -wp.y) * inv_side, vec3<f32>(0.0, 0.0, -sx), vec3<f32>(0.0, 1.0, 0.0), n, relief, inv_side);
    pom_gate = smoothstep(0.1, 0.4, w.z);
    let pz = photo_plane(side, vec2<f32>(wp.x * sz, -wp.y) * inv_side, vec3<f32>(sz, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), n, relief, inv_side);
    pom_gate = smoothstep(0.1, 0.4, w.y);
    let layer = select(side, top, n.y > 0.0);
    let inv = select(inv_side, inv_top, n.y > 0.0);
    let py = photo_top(layer, inv, wp, n, relief, sy);
    pom_gate = 1.0;
    var o: Photo;
    o.albedo = px.albedo * w.x + py.albedo * w.y + pz.albedo * w.z;
    o.n = normalize(px.n * w.x + py.n * w.y + pz.n * w.z);
    o.rough = px.rough * w.x + py.rough * w.y + pz.rough * w.z;
    return o;
}

// Перекраска фото в цвет материала: сохраняем рисунок, меняем средний тон.
fn retint(albedo: vec3<f32>, mean: vec3<f32>, target_c: vec3<f32>, amount: f32) -> vec3<f32> {
    let recolored = albedo * target_c / max(mean, vec3<f32>(0.01));
    return mix(albedo, recolored, amount);
}

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

// Бутовая кладка: неровные камни разного размера в толстых швах раствора.
fn rubble(p: vec2<f32>) -> vec4<f32> {
    let q = p * vec2<f32>(2.4, 3.4) + vec2<f32>(fbm(p * 1.3), fbm(p * 1.3 + 4.0)) * 0.5;
    let v = voronoi(q);
    let edge = v.y - v.x;
    let joint = 1.0 - smoothstep(0.06, 0.2, edge);
    let n = fbm(p * 9.0 + v.z * 11.0);
    let h = smoothstep(0.02, 0.4, edge) * (0.8 + 0.2 * n);
    return vec4<f32>(h, v.z * 0.65 + n * 0.35, joint, v.z);
}

// Плинфа: тонкий кирпич в толстом слое раствора (византийская кладка).
fn bricks(p: vec2<f32>) -> vec4<f32> {
    let course = 0.085;
    let row = floor(p.y / course);
    let len = 0.37;
    let u = p.x / len + fract(row * 0.5) * 1.0;
    let cell = vec2<f32>(floor(u), row);
    let fu = fract(u) * len;
    let fv = fract(p.y / course) * course;
    let e = min(min(fu, len - fu) * 0.6, min(fv, course - fv - 0.032));
    let joint = 1.0 - smoothstep(0.0, 0.006, e);
    let id = hash21(cell);
    let n = fbm(p * 14.0 + id * 9.0);
    return vec4<f32>((1.0 - joint) * (0.85 + 0.15 * n), id * 0.7 + n * 0.3, joint, id);
}

// Смешанная кладка (opus mixtum): ряды тёсаного камня, через каждые 1,6 м — пояс из пяти рядов плинфы.
// Кирпич помечен id > 1.
fn opus_mixtum(p: vec2<f32>) -> vec4<f32> {
    let band = fract(p.y / 1.6);
    if band < 0.27 {
        let b = bricks(p);
        return vec4<f32>(b.x, b.y, b.z, 2.0 + b.w);
    }
    return ashlar(p, vec2<f32>(0.5, 0.29));
}

fn marble(p: vec2<f32>, top: bool) -> vec4<f32> {
    let warp = fbm(p * 1.7) * 5.0 + fbm(p * 6.0) * 0.8;
    let vein = pow(1.0 - abs(sin(p.x * 2.3 + p.y * 1.1 + warp)), 18.0);
    var joint = 0.0;
    var id = 0.0;
    if top {
        let f = flagstone(p * 0.8);
        joint = f.z;
        id = f.w;
    }
    return vec4<f32>(1.0 - joint * 0.6, clamp(vein + fbm(p * 3.0) * 0.15, 0.0, 1.0), joint, id);
}

// Мозаичный пол: тессеры по 2,5 см, круги-«плетёнка» в квадратах 1,2 м и кайма.
fn mosaic(p: vec2<f32>) -> vec4<f32> {
    let t = 0.025;
    let cell = floor(p / t);
    let f = fract(p / t);
    let e = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y));
    let joint = 1.0 - smoothstep(0.04, 0.14, e);
    let c = (cell + 0.5) * t;
    let q = fract(c / 1.2) - 0.5;
    let r = length(q);
    let ring = step(abs(r - 0.36), 0.035) + step(abs(r - 0.22), 0.03);
    let cross = step(abs(q.x), 0.03) * step(r, 0.18) + step(abs(q.y), 0.03) * step(r, 0.18);
    let frame = step(0.47, max(abs(q.x), abs(q.y)));
    // tone: 0 — белый фон, 0.5 — красный узор, 1 — тёмная кайма.
    var tone = 0.0;
    if ring + cross > 0.5 {
        tone = 0.5;
    }
    if frame > 0.5 {
        tone = 1.0;
    }
    let id = hash21(cell);
    return vec4<f32>(1.0 - joint, tone, joint, id);
}

// Утоптанная земля с мелким щебнем.
fn earth(p: vec2<f32>) -> vec4<f32> {
    let v = voronoi(p * 9.0);
    let pebble = 1.0 - smoothstep(0.18, 0.32, v.x);
    let big = fbm(p * 0.6);
    let n = fbm(p * 4.0);
    let peb = pebble * step(0.55, v.z);
    return vec4<f32>(peb * 0.8 + n * 0.2, big * 0.6 + n * 0.4, peb * 0.4, v.z + peb);
}

fn tile(uv: vec2<f32>) -> vec4<f32> {
    let w = 0.44;
    let course = 0.58;
    let col = floor(uv.x / w);
    let row = floor(uv.y / course);
    let fu = fract(uv.x / w) * w;
    let fv = fract(uv.y / course);
    // Калиптер — полуцилиндр шириной 0.16 м над стыком тегул.
    let d = min(fu, w - fu);
    let cover = 1.0 - smoothstep(0.07, 0.085, d);
    let cyl = sqrt(max(0.0, 1.0 - pow(d / 0.08, 2.0)));
    // Тегула: плоская, нижний край чуть ниже следующего ряда (нахлёст).
    let flat_h = 0.25 + fv * 0.25;
    let step_edge = smoothstep(0.93, 1.0, fv);
    let h = mix(flat_h, 0.7 + cyl * 0.3, cover);
    let id = hash21(vec2<f32>(col, row));
    let joint = clamp(step_edge * (1.0 - cover) + (1.0 - smoothstep(0.0, 0.012, abs(d - 0.08))) * 0.5, 0.0, 1.0);
    return vec4<f32>(h, id, joint, id);
}

// Солома и камыш по развёртке (u — вдоль карниза, v — вниз по скату): стебли вдоль ската,
// уложенные рядами внахлёст по ~0.3 м. Под нижним краем каждого ряда — тень, сам край
// неровный (пучки разной длины). `fine` — доля мелких стеблей: вдали её гасим, иначе рябит.
// x — высота, y — тон стебля, z — тень под краем ряда, w — плавные пятна (гуще/реже, темнее).
fn thatch(uv: vec2<f32>, fine: f32) -> vec4<f32> {
    let course = 0.3;
    let ragged = (vnoise(vec2<f32>(uv.x * 5.0, floor(uv.y / course) * 3.1)) - 0.5) * 0.12
        + (vnoise(vec2<f32>(uv.x * 23.0, uv.y * 2.0)) - 0.5) * 0.05;
    let q = uv.y + ragged;
    let row = floor(q / course);
    let fv = fract(q / course);
    let su = uv.x + hash21(vec2<f32>(row, 2.7)) * 3.0;
    let thin = vnoise(vec2<f32>(su * 110.0, q * 4.0));
    let mid = vnoise(vec2<f32>(su * 26.0, q * 1.6));
    // Пятна — плавные и вдоль, и поперёк ската: без них кровля ровная, как ткань,
    // а пятна «по ряду и пучку» превращают солому в дранку.
    let blotch = vnoise(vec2<f32>(uv.x * 1.7, uv.y * 1.1)) * 0.6 + vnoise(vec2<f32>(uv.x * 5.3, uv.y * 3.7)) * 0.4;
    let strands = mix(0.5, thin, fine) * 0.5 + mid * 0.5;
    let shade = 1.0 - smoothstep(0.0, 0.22, fv);
    // Ряд толще книзу, а у самого края скруглён (без ступеньки в пиксель — она бы рябила).
    let h = clamp(0.25 + fv * (1.0 - smoothstep(0.88, 1.0, fv)) * 0.45 + strands * 0.3, 0.0, 1.0);
    return vec4<f32>(h, strands, shade, blotch);
}

// Касательные по развёртке (u, v): t — вдоль u, b — против v (вверх по картинке).
// Без развёртки (вырожденные производные) — горизонталь по грани и «вверх» по ней.
struct UvFrame {
    t: vec3<f32>,
    b: vec3<f32>,
}

fn uv_frame(n: vec3<f32>, dp_x: vec3<f32>, dp_y: vec3<f32>, duv_x: vec2<f32>, duv_y: vec2<f32>) -> UvFrame {
    var f: UvFrame;
    f.t = normalize(cross(n, vec3<f32>(0.0, 1.0, 0.0)) + vec3<f32>(1e-4, 0.0, 0.0));
    f.b = cross(f.t, n);
    let det = duv_x.x * duv_y.y - duv_x.y * duv_y.x;
    if abs(det) > 1e-12 {
        let tu = (dp_x * duv_y.y - dp_y * duv_x.y) / det;
        let tv = (dp_y * duv_x.x - dp_x * duv_y.x) / det;
        let t2 = tu - n * dot(n, tu);
        let b2 = tv - n * dot(n, tv);
        if dot(t2, t2) > 1e-10 && dot(b2, b2) > 1e-10 {
            f.t = normalize(t2);
            f.b = -normalize(b2);
        }
    }
    return f;
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
        case 11: {
            return rubble(p * s);
        }
        case 13: {
            let n = fbm(p * 1.1 * s);
            let fine = fbm(p * 9.0 * s);
            return vec4<f32>(fine * 0.4, n * 0.7 + fine * 0.3, 0.0, n);
        }
        case 14: {
            return bricks(p * s);
        }
        case 15: {
            if top {
                return ashlar(p * s, vec2<f32>(0.6, 0.45));
            }
            return opus_mixtum(p * s);
        }
        case 16: {
            return marble(p * s, top);
        }
        case 17: {
            if top {
                return mosaic(p);
            }
            return vec4<f32>(fbm(p * 3.0), fbm(p * 1.5), 0.0, 0.0);
        }
        case 18: {
            return earth(p * s);
        }
        case 19: {
            if top {
                let n = fbm(p * 0.05);
                let blades = fbm(p * 9.0);
                let e = earth(p * 0.7);
                return vec4<f32>(blades * 0.6 + e.x * 0.4, n, 0.0, fbm(p * 0.6));
            }
            // Скальные выходы известняка: слоистость.
            let layers = fract(p.y * 0.9 + fbm(p * 0.3) * 1.5);
            let r = rubble(p * 0.35);
            return vec4<f32>(r.x * 0.7 + layers * 0.3, r.y, r.z * 0.5 + step(0.92, layers) * 0.5, 3.0);
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
    var water = false;
    // Объёмный рельеф: в полную силу вблизи, к 45 м сходит на нет (дальше его не разглядеть).
    let to_cam = view.world_position.xyz - wp;
    pom_view = normalize(to_cam);
    pom_k = 1.0 - smoothstep(18.0, 45.0, length(to_cam));
    pom_gate = 1.0;
    pom_near = 1.0 - smoothstep(3.5, 14.0, length(to_cam));
    pom_sun = vec3<f32>(0.0, -1.0, 0.0);
@if(!PREPASS_PIPELINE) {
    if lights.n_directional_lights > 0u {
        pom_sun = lights.directional_lights[0].direction_to_light;
    }
}

    if kind == 5 || kind == 6 || kind == 7 || kind == 8 || kind == 9 || kind == 12 || kind == 20 || kind == 21 {
        // Узоры, завязанные на UV или особые.
        var uv = vec2<f32>(0.0);
        @if(VERTEX_UVS_A) {
            uv = in.uv;
        }
        // Производные развёртки — для касательных по UV (черепица: зеркальные скаты, коньки).
        let dp_x = dpdx(wp);
        let dp_y = dpdy(wp);
        let duv_x = dpdx(uv);
        let duv_y = dpdy(uv);
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
            case 12: {
                if surface.photo.x >= 0.0 {
                    // Фото черепицы по развёртке ската: u — вдоль карниза, v — вниз по скату.
                    // Касательные — по самой развёртке (t вдоль u, b к коньку, против v): на двух
                    // скатах одной крыши u идёт в одну сторону, а нормаль — в разные, и по одному
                    // лишь `cross(n, Y)` свет на калиптерах падал бы на половине крыш не с той стороны.
                    let fr = uv_frame(n, dp_x, dp_y, duv_x, duv_y);
                    let tt = fr.t;
                    let bb = fr.b;
                    // Каждая черепица своя (шаг колонн и рядов в текстуре — 0.5 м): тегулы и
                    // калиптеры из разных обжигов, часть заменена новыми, часть старая и тёмная.
                    // Ряды кладут руками — каждый чуть сдвинут поперёк ската.
                    let row = floor(uv.y / 0.5);
                    let uvr = uv + vec2<f32>((hash21(vec2<f32>(row, 5.3)) - 0.5) * 0.035, 0.0);
                    let dk = abs(fract(uvr.x / 0.5 + 0.5) - 0.5) * 0.5;
                    let on_k = 1.0 - smoothstep(0.085, 0.105, dk);
                    let teg = floor(vec2<f32>(uvr.x / 0.5, (uvr.y - 0.03) / 0.5));
                    let kal = floor(vec2<f32>(uvr.x / 0.5 + 0.5, uvr.y / 0.5));
                    let cell = select(teg, kal + vec2<f32>(311.0, 97.0), on_k > 0.5);
                    let id = hash21(cell);
                    let kiln = hash21(cell * 1.37 + vec2<f32>(3.1, 7.7));
                    let ph = photo_plane(i32(surface.photo.x + 0.5), uvr * surface.photo.y, tt, bb, n, surface.photo_mean_b.w * 0.85, surface.photo.y);
                    color = retint(ph.albedo, surface.photo_mean.rgb, base_a, surface.photo_mean.w);
                    color *= (0.88 + id * 0.22) * vec3<f32>(1.0 + (kiln - 0.5) * 0.1, 1.0, 1.0 - (kiln - 0.5) * 0.14);
                    // Новые (краснее и светлее) и старые (бурые, серые) черепицы вперемешку.
                    color *= mix(vec3<f32>(1.0), vec3<f32>(1.16, 0.95, 0.86), step(0.94, kiln));
                    color *= mix(vec3<f32>(1.0), vec3<f32>(0.72, 0.7, 0.69), step(kiln, 0.06));
                    color *= 1.0 + (fbm(wp.xz * 0.2) - 0.5) * variation * 1.5;
                    // Пятна перекладки: участки в несколько метров светлее или темнее соседних.
                    color *= 1.0 + (fbm(wp.xz * 0.09 + vec2<f32>(wp.y * 0.1, 4.2)) - 0.5) * 0.35;
                    // Издалека: у одних домов черепица новее и краснее, у других — старая,
                    // бурая и серая от копоти очагов и лишайника; пятнами, а не ровной заливкой.
                    let age = smoothstep(0.38, 0.62, fbm(wp.xz * 0.035 + vec2<f32>(3.7, 9.1)));
                    color *= mix(vec3<f32>(1.1, 1.03, 0.96), vec3<f32>(0.7, 0.66, 0.64), age);
                    let soot = smoothstep(0.52, 0.68, fbm(wp.xz * 0.32 + vec2<f32>(wp.y * 0.2, 1.3)));
                    color = mix(color, vec3<f32>(0.075, 0.068, 0.058), soot * (0.2 + 0.35 * age));
                    let lichen = smoothstep(0.62, 0.78, fbm(uv * 0.7 + wp.xz * 0.05)) * (0.5 + age);
                    color = mix(color, vec3<f32>(0.36, 0.36, 0.22), lichen * moss_amount * 0.7);
                    rough = clamp(ph.rough + lichen * 0.2, 0.3, 1.0);
                    N = normalize(ph.n);
                    break;
                }
                // Керамическая черепица: плоские тегулы с бортиками и полукруглые калиптеры
                // над стыками. u — вдоль карниза, v — вниз по скату.
                let t = tile(uv);
                color = mix(base_a, base_a * vec3<f32>(1.25, 1.1, 1.0), t.y * variation * 2.5);
                color = mix(color, base_b, t.z * 0.85);
                let lichen = smoothstep(0.66, 0.82, fbm(uv * 0.9 + t.w * 3.0));
                color = mix(color, vec3<f32>(0.36, 0.36, 0.22), lichen * moss_amount);
                rough = mix(0.55, 0.9, t.z + lichen);
                let e = 0.01;
                let hu = tile(uv + vec2<f32>(e, 0.0)).x - t.x;
                let hv = tile(uv + vec2<f32>(0.0, e)).x - t.x;
                let tt = normalize(cross(n, vec3<f32>(0.0, 1.0, 0.0)) + vec3<f32>(1e-4, 0.0, 0.0));
                let bb = cross(tt, n);
                N = perturb(N, (tt * hu + bb * hv) / e, bump);
            }
            case 21: {
                // Круглый лес: u — вдоль бревна (м), v — по окружности (м). Волокна фото идут
                // вдоль u, поэтому у стоек, верхняков и жердей они всегда вдоль ствола, а не
                // по мировым осям, как у досок. Торцы помечены u > 500: годичные кольца.
                if uv.x > 500.0 {
                    // Торец: u = 1000 + доля радиуса (0 — сердцевина, 1 — край), v — радиус бревна (м).
                    let rn = uv.x - 1000.0;
                    let r = rn * uv.y;
                    let wob = (fbm(vec2<f32>(wp.x + wp.z, wp.y) * 9.0) - 0.5) * 0.7;
                    let ring = fract(r * 30.0 + wob);
                    let late = smoothstep(0.55, 0.9, ring);
                    // Трещины усушки и тёмная заболонь с остатками коры у края.
                    let crack = smoothstep(0.9, 0.97, fbm(vec2<f32>(wp.x - wp.z, wp.y) * 5.0));
                    let rim = smoothstep(0.82, 0.97, rn);
                    color = mix(base_b, base_b * vec3<f32>(0.7, 0.6, 0.48), late * 0.55);
                    color = mix(color, base_a * 0.5, crack * 0.7);
                    color = mix(color, base_a * 0.65, rim);
                    // Старый срез сереет и темнеет пятнами.
                    color *= 0.82 + 0.2 * fbm(vec2<f32>(wp.x + wp.z, wp.y) * 2.5);
                    rough = 0.92;
                    break;
                }
                if surface.photo.x >= 0.0 {
                    let fr = uv_frame(n, dp_x, dp_y, duv_x, duv_y);
                    let ph = photo_plane(i32(surface.photo.x + 0.5), uv * surface.photo.y, fr.t, fr.b, n, surface.photo_mean_b.w, surface.photo.y);
                    color = retint(ph.albedo, surface.photo_mean.rgb, base_a, surface.photo_mean.w);
                    // Каждое бревно своего тона, вдоль ствола — потёки и серые обветренные полосы.
                    let streak = fbm(vec2<f32>(uv.x * 0.35, uv.y * 2.2) + wp.xz * 0.07);
                    color *= 0.8 + 0.4 * streak;
                    let grey = smoothstep(0.45, 0.8, fbm(vec2<f32>(uv.x * 0.8, uv.y * 0.6) + wp.zx * 0.11));
                    color = mix(color, vec3<f32>(0.2, 0.19, 0.17), grey * 0.35 * variation * 2.0);
                    rough = clamp(ph.rough * 0.6 + 0.4, 0.5, 1.0);
                    N = normalize(ph.n);
                    break;
                }
                // Без фото — волокна вдоль u.
                let fr = uv_frame(n, dp_x, dp_y, duv_x, duv_y);
                let g = fbm(vec2<f32>(uv.x * 0.8, uv.y * 30.0));
                color = base_a * (0.75 + 0.5 * g);
                rough = 0.85;
                let e = 0.01;
                let hv = fbm(vec2<f32>(uv.x * 0.8, (uv.y + e) * 30.0)) - g;
                N = perturb(N, -fr.b * hv / e, bump);
            }
            case 20: {
                let fr = uv_frame(n, dp_x, dp_y, duv_x, duv_y);
                if surface.photo.x >= 0.0 {
                    // Своя солома из Blender (tools/textures/blender_thatch.py): ряды по 0.3 м,
                    // пучки стеблей с рваными комлями, верх картинки — конёк. Развёртка та же,
                    // что у черепицы: u — вдоль карниза, v — вниз по скату.
                    let ph = photo_plane(i32(surface.photo.x + 0.5), uv * surface.photo.y, fr.t, fr.b, n, surface.photo_mean_b.w, surface.photo.y);
                    color = retint(ph.albedo, surface.photo_mean.rgb, base_a, surface.photo_mean.w);
                    // Тайл 1.5 м повторяется вдоль карниза — полосы по скату своей ширины и
                    // оттенка (не кратные тайлу) прячут повтор.
                    let streak = fbm(vec2<f32>(uv.x * 2.3, uv.y * 0.45) + wp.xz * 0.05);
                    color *= 0.84 + 0.32 * streak;
                    // Старая солома сереет и темнеет пятнами, свежие заплаты светлее и желтее.
                    let age = smoothstep(0.35, 0.75, fbm(wp.xz * 0.18 + vec2<f32>(wp.y * 0.1, 2.3)));
                    color = mix(color * vec3<f32>(1.08, 1.04, 0.9), color * vec3<f32>(0.72, 0.71, 0.72), age);
                    let moss = smoothstep(0.62, 0.8, fbm(uv * 0.9 + wp.xz * 0.04)) * moss_amount * (0.4 + age);
                    color = mix(color, vec3<f32>(0.17, 0.19, 0.08), moss * 0.7);
                    rough = clamp(ph.rough + moss * 0.1, 0.6, 1.0);
                    N = normalize(ph.n);
                    break;
                }
                // Без фото — процедурная солома: стебли вниз по скату, ряды внахлёст.
                // Мелкие стебли гасим, когда на пиксель их приходится больше пары, — иначе рябь.
                let fw = max(length(duv_x), length(duv_y));
                let fine = 1.0 - smoothstep(0.004, 0.012, fw);
                let th = thatch(uv, fine);
                color = mix(base_b, base_a, 0.2 + 0.8 * th.y) * (0.78 + 0.44 * th.w);
                // Старая солома сереет пятнами, свежие заплаты светлее и желтее.
                let age = smoothstep(0.35, 0.75, fbm(wp.xz * 0.18 + vec2<f32>(wp.y * 0.1, 2.3)));
                color = mix(color * vec3<f32>(1.08, 1.04, 0.9), color * vec3<f32>(0.7, 0.7, 0.72), age);
                color *= 1.0 - th.z * 0.38;
                let moss = smoothstep(0.62, 0.8, fbm(uv * 0.9 + wp.xz * 0.04)) * moss_amount * (0.4 + age);
                color = mix(color, vec3<f32>(0.17, 0.19, 0.08), moss * 0.7);
                rough = 0.96;
                let e = 0.004;
                let hu = thatch(uv + vec2<f32>(e, 0.0), fine).x - th.x;
                let hv = thatch(uv + vec2<f32>(0.0, e), fine).x - th.x;
                // v растёт вниз, `fr.b` смотрит вверх — отсюда минус у второй производной.
                N = perturb(N, (fr.t * hu - fr.b * hv) / e, bump * (0.35 + 0.65 * fine));
            }
            case 7: {
                if surface.photo.x >= 0.0 {
                    let ph = photo_triplanar(i32(surface.photo.x + 0.5), i32(surface.photo.z + 0.5), surface.photo.y, surface.photo.w, wp, n, surface.photo_mean_b.w);
                    let rust = smoothstep(0.6, 0.85, fbm(wp.xy * 3.0 + wp.zz * 2.0));
                    color = mix(retint(ph.albedo, surface.photo_mean.rgb, base_a, surface.photo_mean.w), base_b, rust * variation * 1.5);
                    rough = clamp(ph.rough * 0.8 + rust * 0.4, 0.15, 1.0);
                    pbr_input.material.metallic = pbr_input.material.metallic * (1.0 - rust * 0.7);
                    N = ph.n;
                    break;
                }
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
                // Крупная зыбь + мелкая рябь.
                let g2 = vec3<f32>(
                    sin(p.x * 0.9 + p.y * 0.4 + t * 0.6) + 0.5 * sin(p.x * 1.7 - p.y * 1.1 + t * 0.8),
                    0.0,
                    cos(p.y * 0.8 - p.x * 0.3 + t * 0.5) + 0.5 * cos(p.y * 2.1 + p.x * 0.7 - t * 0.7),
                );
                N = normalize(N + g * 0.03 + g2 * 0.045);
                color = mix(base_a, base_b, fbm(p * 2.0 + t * 0.1));
                rough = 0.04;
                // Отражение неба по Френелю и солнечная дорожка (небо — не карта окружения,
                // поэтому считаем его здесь тем же градиентом, что и купол неба).
                let v = normalize(view.world_position - wp);
                let r = reflect(-v, N);
                let sun = normalize(surface.photo_mean.xyz);
                let fres = 0.02 + 0.98 * pow(1.0 - max(dot(N, v), 0.0), 5.0);
                let h = clamp(r.y, 0.0, 1.0);
                let toward = pow(max(dot(normalize(r.xz + vec2<f32>(1e-5)), normalize(sun.xz + vec2<f32>(1e-5))), 0.0), 3.0);
                var skyc: vec3<f32>;
                if surface.photo_mean_b.x > 0.5 {
                    skyc = mix(vec3<f32>(0.62, 0.74, 0.9), vec3<f32>(0.2, 0.4, 0.8), smoothstep(0.0, 0.5, h));
                } else {
                    let hor = mix(vec3<f32>(0.62, 0.48, 0.55), vec3<f32>(1.25, 0.55, 0.28), toward);
                    skyc = mix(hor, vec3<f32>(0.12, 0.13, 0.3), smoothstep(0.0, 0.6, h));
                }
                let glint = pow(max(dot(r, sun), 0.0), 900.0) * 40.0 + pow(max(dot(r, sun), 0.0), 60.0) * 1.2;
                emissive = (skyc * fres + vec3<f32>(1.0, 0.78, 0.5) * glint) * 0.9;
                water = true;
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
    } else if surface.photo.x >= 0.0 && kind != 17 {
        // Фотоматериал: настоящая текстура камня, дерева, земли с картами нормалей и шероховатости.
        let top = i32(surface.photo.x + 0.5);
        let side = i32(surface.photo.z + 0.5);
        let relief = max(surface.photo_mean_b.w, 0.0);
        let tint = surface.photo_mean.w;
        var ph: Photo;
        var mean = surface.photo_mean.rgb;
        if kind == 19 {
            // Рельеф: трава и сухая почва на пологих местах, скала на крутых, галька у воды.
            let slope = 1.0 - clamp(n.y, 0.0, 1.0);
            // Номера слоёв почвы и гальки — L_SOIL и L_GRAVEL из surface_layers.rs.
            let grass = photo_top(top, surface.photo.y, wp, n, relief, 1.0);
            let soil = photo_top(8, 1.0 / 3.5, wp + vec3<f32>(17.0, 0.0, 5.0), n, relief, 1.0);
            let rock = photo_triplanar(side, side, surface.photo.w, surface.photo.w, wp * 0.5, n, relief);
            let gravel = photo_top(18, 0.5, wp, n, relief, 1.0);
            let dry = smoothstep(0.5, 0.78, fbm(wp.xz * 0.045) + fbm(wp.xz * 0.4) * 0.25);
            // Вблизи это земля под пучками травы (тёмная, пучки растут поверх — vegetation.rs);
            // дальше, чем растёт трава, — сам травяной ковёр, светлее и зеленее: иначе вокруг
            // игрока было бы видно кольцо, где трава кончается.
            let carpet = smoothstep(35.0, 75.0, length(view.world_position.xyz - wp));
            let g_col = retint(grass.albedo, mean, base_a, tint) * mix(vec3<f32>(1.0), vec3<f32>(1.05, 1.02, 0.9), smoothstep(0.4, 0.85, fbm(wp.xz * 0.02))) * mix(vec3<f32>(0.19, 0.23, 0.51), vec3<f32>(0.215, 0.235, 0.7), carpet);
            // Сухая почва под травой — тёмная, с прозеленью: светлая соломенная земля спорила
            // с зелёной травой и пересвечивала кадр.
            let soil_c = mix(soil.albedo * mix(vec3<f32>(0.4, 0.45, 0.36), vec3<f32>(0.5, 0.54, 0.46), carpet), g_col, 0.5);
            ph.albedo = mix(g_col, soil_c, dry);
            ph.n = normalize(mix(grass.n, soil.n, dry));
            ph.rough = mix(grass.rough, soil.rough, dry);
            // На крутом трава не держится: сначала голая земля с осыпью, круче — скала.
            let bare = smoothstep(0.24, 0.33, slope + (fbm(wp.xz * 0.3) - 0.5) * 0.1 + (fbm(wp.xz * 0.9) - 0.5) * 0.05);
            // Осыпь: голая земля вперемешку с щебнем, пятнами.
            let scree = smoothstep(0.42, 0.62, fbm(wp.xz * 0.55 + vec2<f32>(7.3, 1.9))) * 0.75;
            let bare_c = mix(mix(vec3<f32>(dot(soil.albedo, vec3<f32>(0.3, 0.6, 0.1))), soil.albedo, 0.45) * vec3<f32>(0.33, 0.32, 0.3), gravel.albedo * vec3<f32>(0.5, 0.5, 0.49), scree);
            // На крутом проекция сверху растягивает рисунок земли полосами вдоль склона. Там
            // рисунок берём со скалы (она снята с трёх сторон и не тянется): выходит плотный
            // каменистый грунт, как на осыпающемся берегу.
            let tilt_k = smoothstep(0.2, 0.42, slope);
            let stony = clamp(rock.albedo / max(surface.photo_mean_b.rgb, vec3<f32>(0.02)), vec3<f32>(0.45), vec3<f32>(1.9)) * vec3<f32>(0.17, 0.155, 0.132) * mix(0.85, 1.12, fbm(wp.xz * 0.37 + vec2<f32>(wp.y, 2.7)));
            // Щебень на каменистом грунте: мелкие камни разного тона с тёмными щелями между ними
            // (вблизи; дальше 45 м они мельче пиксела и только рябят).
            let peb_far = 1.0 - smoothstep(18.0, 45.0, length(view.world_position.xyz - wp));
            let peb = voronoi(vec2<f32>(wp.x + wp.y * 0.7, wp.z + wp.y * 0.6) * 3.3);
            let peb_gap = 1.0 - smoothstep(0.02, 0.14, peb.y - peb.x);
            let pebbles = mix(1.0, mix(0.78, 1.22, peb.z) * (1.0 - 0.45 * peb_gap), peb_far * smoothstep(0.3, 0.6, fbm(wp.xz * 0.8 + vec2<f32>(wp.y, 5.5))));
            let bare_cc = mix(bare_c, stony * pebbles, tilt_k);
            ph.albedo = mix(ph.albedo, bare_cc, bare);
            ph.n = normalize(mix(ph.n, mix(mix(soil.n, gravel.n, scree), normalize(mix(n, rock.n, 0.6)), tilt_k), bare));
            ph.rough = mix(ph.rough, mix(soil.rough, gravel.rough, scree), bare);
            let rk = smoothstep(0.31, 0.5, slope + (fbm(wp.xz * 0.08) - 0.5) * 0.3 + (fbm(wp.xz * 0.6) - 0.5) * 0.12);
            // Скала обрывов: серый обветренный известняк пластами (светлый на солнце выгорал в белое
            // пятно), сверху по уступам — тёмные потёки.
            let strata = fbm(vec2<f32>((wp.x + wp.z) * 0.05, wp.y * 0.9));
            let drip = smoothstep(0.45, 0.8, fbm(vec2<f32>((wp.x - wp.z) * 0.7, wp.y * 0.12)));
            // Чёрные щели скана на солнце выглядят кляксами — контраст картинки убавлен.
            let rock_flat = 0.5 * (surface.photo_mean_b.rgb + base_b);
            // Самые чёрные щели скана подняты к среднему тону: на солнце они лежали кляксами.
            let rock_a = mix(rock.albedo, max(rock.albedo, surface.photo_mean_b.rgb * 0.55), 0.75);
            let rock_c = mix(rock_flat, retint(rock_a, surface.photo_mean_b.rgb, base_b, 0.5), mix(0.6, 0.9, pom_k)) * vec3<f32>(0.57, 0.57, 0.55) * mix(0.72, 1.12, strata) * (1.0 - 0.3 * drip);
            // Скала ломается на блоки по пластам и трещинам: у каждого блока свой наклон грани
            // и тон, между блоками — тёмные щели. Без этого обрыв читался гладкой «подушкой».
            // Вблизи рельеф скалы в полную силу (резкие грани и трещинки), вдали мягче — там он рябит.
            var crag_n = normalize(mix(n, rock.n, mix(0.4, 0.85, pom_k)));
            var crag = 1.0;
            if rk > 0.01 {
                // Пласты лежат почти горизонтально и длинные; трещины идут не сплошной сеткой, а
                // местами (сплошная сетка читалась каменной кладкой).
                let warp = fbm(wp.xz * 0.21 + vec2<f32>(wp.y * 0.3, 4.1)) - 0.5;
                let ly = wp.y * 0.5 + fbm(wp.xz * 0.06) * 2.2 + warp * 0.9;
                let wx = smoothstep(0.3, 0.7, n.x * n.x / max(n.x * n.x + n.z * n.z, 1e-4));
                let va = voronoi(vec2<f32>(wp.z * 0.17 + ly * 0.2 + warp, ly));
                let vb = voronoi(vec2<f32>(wp.x * 0.17 - ly * 0.2 + warp, ly + 3.7));
                let far = 1.0 - smoothstep(250.0, 500.0, length(view.world_position.xyz - wp));
                let open = smoothstep(0.38, 0.62, fbm(vec2<f32>(wp.x + wp.z, wp.y * 2.0) * 0.23 + vec2<f32>(9.2, 3.3)));
                let gap = mix(1.0 - smoothstep(0.02, 0.13, vb.y - vb.x), 1.0 - smoothstep(0.02, 0.13, va.y - va.x), wx) * far * mix(0.25, 1.0, open);
                let id = mix(vb.z, va.z, step(0.5, wx));
                crag = mix(0.82, 1.12, id) * (1.0 - 0.5 * gap);
                let tilt = vec3<f32>(fract(id * 17.3) - 0.5, (fract(id * 5.1) - 0.5) * 0.5, fract(id * 29.7) - 0.5);
                crag_n = normalize(crag_n + tilt * 0.7 * far * (1.0 - gap));
            }
            // Живой камень: серо-оливковый лишайник пятнами и тёмный мох на гранях, глядящих вверх.
            let lichen = smoothstep(0.52, 0.74, fbm(vec2<f32>(wp.x + wp.y * 0.6, wp.z - wp.y * 0.5) * 0.9)) * smoothstep(0.3, 0.62, fbm(wp.xz * 3.1 + vec2<f32>(wp.y * 2.0, 1.3)));
            let moss = smoothstep(0.62, 0.8, crag_n.y) * smoothstep(0.38, 0.6, fbm(wp.xz * 1.3 + vec2<f32>(3.1, wp.y)));
            var rock_cc = rock_c * crag;
            rock_cc = mix(rock_cc, (rock_cc * 0.55 + vec3<f32>(0.035, 0.04, 0.022)) * vec3<f32>(0.9, 1.0, 0.72), lichen * 0.6);
            rock_cc = mix(rock_cc, vec3<f32>(0.035, 0.05, 0.018) * (0.6 + rock_a.g / max(surface.photo_mean_b.g, 0.02) * 0.5), moss * 0.7);
            ph.albedo = mix(ph.albedo, rock_cc, rk);
            ph.n = normalize(mix(ph.n, crag_n, rk));
            ph.rough = mix(ph.rough, rock.rough, rk);
            let shore = 1.0 - smoothstep(-1.0, 0.4, wp.y + (fbm(wp.xz * 0.3) - 0.5) * 0.8);
            ph.albedo = mix(ph.albedo, gravel.albedo, shore);
            ph.n = normalize(mix(ph.n, gravel.n, shore));
            ph.rough = mix(ph.rough, gravel.rough * 0.7, shore);
            color = ph.albedo;
        } else {
            if kind == 15 {
                // Смешанная кладка (opus mixtum): камень, а через каждые 1,6 м — пояс плинфы.
                ph = photo_triplanar(top, top, surface.photo.y, surface.photo.y, wp, n, relief);
                let br = photo_triplanar(side, side, surface.photo.w, surface.photo.w, wp, n, relief);
                // Край пояса сглажен на ширину пикселя: жёсткий step вдали «дёргался» лесенкой.
                let yb = wp.y / 1.6;
                let edge = abs(fract(yb - 0.135 + 0.5) - 0.5);
                let aa = max(fwidth(yb), 0.001);
                let band = (1.0 - smoothstep(0.135 - aa, 0.135 + aa, edge)) * (1.0 - smoothstep(0.65, 0.75, n.y));
                let stone_c = retint(ph.albedo, mean, base_a, tint);
                let brick_c = retint(br.albedo, surface.photo_mean_b.rgb, vec3<f32>(0.42, 0.17, 0.09), 0.6);
                color = mix(stone_c, brick_c, band);
                ph.n = normalize(mix(ph.n, br.n, band));
                ph.rough = mix(ph.rough, br.rough, band);
            } else {
                ph = photo_triplanar(top, side, surface.photo.y, surface.photo.w, wp, n, relief);
                // Средний цвет того слоя, что виден: сверху — верхнего, на стенах — бокового.
                let m = mix(surface.photo_mean_b.rgb, mean, smoothstep(0.3, 0.9, n.y));
                color = retint(ph.albedo, m, base_a, tint);
            }
        }
        // Крупные пятна выцветания и потемнения — ломают повтор тайла.
        let macro_v = fbm(wp.xz * 0.07 + wp.y * 0.05) - 0.5;
        color *= 1.0 + macro_v * variation * 1.2;
        rough = clamp(mix(rough, ph.rough, 0.85), 0.05, 1.0);
        N = ph.n;
        if kind == 19 {
            // Земля и трава матовые: против солнца блик на мелких нормалях давал белёсую рябь
            // с чёрными точками.
            rough = max(rough, 0.93);
            pbr_input.material.reflectance = vec3<f32>(0.12);
            N = normalize(mix(n, N, 0.6));
        }

        // Мох у земли и на верхних гранях, грязь и подтёки снизу.
        let up = clamp(n.y, 0.0, 1.0);
        let large = fbm(wp.xz * 0.15 + wp.y * 0.05);
        let moss = smoothstep(0.62, 0.82, large + up * 0.3 - clamp(wp.y - 1.0, 0.0, 6.0) * 0.08) * moss_amount;
        color = mix(color, vec3<f32>(0.10, 0.14, 0.05) * (0.8 + 0.4 * fbm(wp.xz * 3.0)), moss);
        let ground = 1.0 - smoothstep(0.0, 1.4, wp.y + fbm(wp.xz * 0.8 + wp.zx) * 0.6);
        let streaks = smoothstep(0.45, 0.85, fbm(vec2<f32>(wp.x + wp.z, wp.y * 0.08) * 1.7)) * (1.0 - up);
        color *= 1.0 - (ground * 0.35 + streaks * 0.2) * grime_amount;
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
        if kind == 15 && s.id > 1.5 {
            // Пояс плинфы.
            color = vec3<f32>(0.42, 0.17, 0.09) * (0.8 + 0.5 * fract(s.id * 3.7));
        }
        if kind == 16 {
            color = mix(base_a * (0.95 + 0.1 * fbm(wp.xz * 2.0)), base_b, s.tone);
        }
        if kind == 17 {
            if s.tone > 0.75 {
                color = vec3<f32>(0.06, 0.05, 0.05);
            } else if s.tone > 0.25 {
                color = base_b * (0.85 + 0.3 * fract(s.id * 5.1));
            } else {
                color = base_a * (0.9 + 0.15 * fract(s.id * 3.3));
            }
        }
        if kind == 18 {
            color = mix(base_a, base_a * 0.75, s.tone) * (1.0 + (fract(s.id) - 0.5) * 0.15);
            color = mix(color, base_b, clamp(s.id - 1.0, 0.0, 1.0) * 0.8);
        }
        if kind == 19 {
            // Трава на пологом, камень на крутом.
            let slope = 1.0 - clamp(n.y, 0.0, 1.0);
            let dry = vec3<f32>(0.45, 0.40, 0.22);
            let green = base_a;
            let soil = vec3<f32>(0.33, 0.27, 0.19);
            var c = mix(green, dry, smoothstep(0.3, 0.7, s.tone));
            c = mix(c, soil, smoothstep(0.55, 0.85, fract(s.id * 1.0)) * 0.6);
            c *= 0.85 + 0.3 * fbm(wp.xz * 0.9);
            let rock = base_b * (0.8 + 0.4 * fbm(wp.xz * 0.5 + wp.y));
            color = mix(c, rock, smoothstep(0.35, 0.6, slope + (fbm(wp.xz * 0.08) - 0.5) * 0.3));
            // Галька у воды.
            color = mix(color, vec3<f32>(0.55, 0.52, 0.46), (1.0 - smoothstep(-1.0, 0.3, wp.y)) * 0.7);
        }
        if kind == 17 {
            color *= 1.0 - s.joint * 0.45;
        } else {
            color = mix(color, base_b, s.joint * 0.9);
        }
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

    // Дождь: камень и земля темнеют и блестят, на ровном — лужи (params2.w — мокрость города,
    // color_b.w — насколько этот материал вообще мокнет; под крышей полы не мокнут).
    let wet = clamp(surface.params2.w, 0.0, 1.0) * surface.color_b.w;
    if wet > 0.002 && !water {
        let up = clamp(n.y, 0.0, 1.0);
        let pn = fbm(wp.xz * 0.23 + 1.7) * 0.7 + fbm(wp.xz * 1.3) * 0.3;
        let flat_ground = smoothstep(0.93, 0.99, up);
        let puddle = smoothstep(0.6, 0.67, pn + (wet - 1.0) * 0.35) * flat_ground * smoothstep(0.35, 0.8, wet);
        // Потёки по стенам: мокрее у швов и книзу.
        let runs = smoothstep(0.35, 0.75, fbm(vec2<f32>(wp.x + wp.z, wp.y * 0.15) * 2.3)) * (1.0 - up);
        color *= 1.0 - wet * (0.22 + 0.18 * up + 0.12 * runs);
        color = mix(color, color * 0.6 + vec3<f32>(0.004, 0.005, 0.007), puddle);
        // Мокрый камень блестит, но не зеркалит целиком: против солнца склон и стена
        // превращались в белый лист. Земля под травой (рельеф) почти не блестит, лужи на ней редки.
        let ground19 = select(0.0, 1.0, kind == 19);
        rough = mix(rough, rough * mix(mix(0.8, 0.5, up), 0.9, ground19), wet);
        rough = mix(rough, 0.04, puddle * (1.0 - 0.8 * ground19));
        // В луже рельеф камня тонет: поверхность ровная, как зеркало.
        N = normalize(mix(N, n, puddle * 0.92 + wet * up * 0.25));
    }

    pbr_input.material.base_color = vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
    pbr_input.material.perceptual_roughness = rough;
    pbr_input.N = N;
    // Отражение неба у воды — в тех же единицах, что и купол неба (без экспозиции камеры).
    pbr_input.material.emissive = vec4<f32>(emissive, select(1.0, 0.0, water));

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
