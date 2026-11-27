// Море. Вершины сетки вокруг камеры сдвигают волны Герстнера (та же формула на процессоре —
// `water.rs`, по ней качаются поплавок и пловец). Во фрагменте:
// * нормаль: крупные волны (из вершин) + мелкие волны и рябь, которые вдали гаснут;
// * преломление: дно и сваи видны сквозь воду со сдвигом по нормали (копия кадра после
//   непрозрачного прохода — «пропускание» Bevy), свет в толще гаснет по Бугеру — Ламберту:
//   красный уходит первым, вода у берега бирюзовая, в глубине тёмно-зелёная;
// * отражение: город, стены и корабли — трассировкой по экрану (глубина из предпрохода),
//   где луч ушёл за кадр — небо с облаками той же формулой, что купол;
// * солнечная и лунная дорожки (GGX с шероховатостью, растущей вдаль);
// * пена у берега (по глубине) и на гребнях крутых волн, рассеяние в гребнях на просвет;
// * снизу, из-под воды: окно Снелла (небо и берег сквозь поверхность) и полное внутреннее
//   отражение за ним.
// Глубина и укрытость (бухта, мелководье) — из карты `water_map` (её строит water.rs).

import bevy_pbr::render::{
    mesh_functions,
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::{globals, view},
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    view_transformations::{depth_ndc_to_view_z, position_world_to_view, position_world_to_clip},
};
import medieval_rpg::shaders::sky_common::{SkyParams, sky_radiance, vnoise, fbm3};
@if(DEPTH_PREPASS)
import bevy_pbr::prepass::utils::prepass_depth;
@if(DEPTH_PREPASS)
import bevy_pbr::render::mesh_view_bindings::depth_prepass_texture;
@if(VIEW_TRANSMISSION_TEXTURE)
import bevy_pbr::render::mesh_view_bindings::{view_transmission_texture, view_transmission_sampler};

struct Water {
    sky: SkyParams,
    // Цвет рассеяния в толще (линейный), w — множитель поглощения.
    deep: vec4<f32>,
    // Цвет мелководья (без пропускания), w — сила пены.
    shallow: vec4<f32>,
    // x — сила солнечного блика, y — масштаб мелких волн, z — 1, если есть глубина из
    // предпрохода, w — 1 на простом качестве воды.
    params: vec4<f32>,
    // x — высота волн (ветер), y — уровень моря, z — 1: трассировка отражений по экрану,
    // w — запас.
    waves: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> water: Water;
// r — глубина/16 м, g — сила волн (мелководье, бухта), b — близость берега.
@group(constants::MATERIAL_BIND_GROUP) @binding(101)
var water_map: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102)
var water_map_sampler: sampler;

const MAP_HALF: f32 = 900.0;
const N_WAVES: u32 = 6u;
// Волны Герстнера: xy — направление, z — длина волны, м, w — высота (амплитуда), м.
// Те же числа — `WAVES` в water.rs.
const WAVES = array<vec4<f32>, 6>(
    vec4<f32>(0.28, 0.96, 31.0, 0.17),
    vec4<f32>(-0.34, 0.94, 17.5, 0.10),
    vec4<f32>(0.71, 0.70, 10.2, 0.06),
    vec4<f32>(-0.80, 0.60, 6.1, 0.035),
    vec4<f32>(0.10, -0.99, 3.7, 0.02),
    vec4<f32>(0.93, 0.37, 2.3, 0.012),
);
// Крутизна гребней (0 — синусоида, 1 — острые гребни).
const STEEP: f32 = 0.62;

fn map_at(xz: vec2<f32>) -> vec4<f32> {
    let uv = (xz + vec2<f32>(MAP_HALF)) / (2.0 * MAP_HALF);
    return textureSampleLevel(water_map, water_map_sampler, uv, 0.0);
}

struct Gerstner {
    offset: vec3<f32>,
    normal: vec3<f32>,
    // Сжатие поверхности на гребне (больше — острее гребень, там пена).
    crest: f32,
}

// Сдвиг точки поверхности волнами. `fade_dist` — расстояние до камеры: волны, которые сетка
// там уже не передаёт (короче 5–9 шагов сетки), плавно гасим.
fn gerstner(p: vec2<f32>, t: f32, amp: f32, fade_dist: f32) -> Gerstner {
    var g: Gerstner;
    g.offset = vec3<f32>(0.0);
    var nx = 0.0;
    var nz = 0.0;
    var ny = 0.0;
    var jac = 0.0;
    for (var i = 0u; i < N_WAVES; i++) {
        let w = WAVES[i];
        let d = normalize(w.xy);
        let k = 6.2831853 / w.z;
        let c = sqrt(9.81 / k);
        let fade = 1.0 - smoothstep(w.z * 5.0, w.z * 9.0, fade_dist);
        let a = w.w * amp * fade;
        let q = STEEP / (k * max(w.w, 1e-4) * f32(N_WAVES));
        let ph = k * dot(d, p) - c * k * t + f32(i) * 1.7;
        let s = sin(ph);
        let co = cos(ph);
        g.offset += vec3<f32>(d.x * q * a * co, a * s, d.y * q * a * co);
        let wa = k * a;
        nx += d.x * wa * co;
        nz += d.y * wa * co;
        ny += q * wa * s;
        jac += q * wa * s;
    }
    g.normal = normalize(vec3<f32>(-nx, 1.0 - ny, -nz));
    g.crest = jac;
    return g;
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;
    let m = map_at(world.xz);
    let dist = length(world.xz - view.world_position.xz);
    let g = gerstner(world.xz, globals.time, water.waves.x * m.g, dist);
    let wp = world + g.offset;
    out.world_position = vec4<f32>(wp, 1.0);
    out.position = position_world_to_clip(wp);
    out.world_normal = g.normal;
@if(VERTEX_UVS_A)
    out.uv = vec2<f32>(g.crest, m.g);
@if(VERTEX_UVS_B)
    out.uv_b = world.xz;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
    return out;
}

// Наклон поверхности от одной мелкой волны.
fn wave_grad(p: vec2<f32>, dir: vec2<f32>, len: f32, steep: f32, t: f32) -> vec2<f32> {
    let k = 6.2831853 / len;
    let c = sqrt(9.81 / k);
    let ph = k * dot(dir, p) - c * k * t;
    return dir * (steep * cos(ph));
}

fn ripples(p: vec2<f32>, t: f32) -> vec2<f32> {
    let e = 0.15;
    let a = p * 1.6 + vec2<f32>(t * 0.35, t * 0.21);
    let b = p * 3.7 - vec2<f32>(t * 0.27, -t * 0.4);
    let h0 = vnoise(a) + vnoise(b) * 0.5;
    let hx = vnoise(a + vec2<f32>(e, 0.0)) + vnoise(b + vec2<f32>(e * 2.3, 0.0)) * 0.5;
    let hz = vnoise(a + vec2<f32>(0.0, e)) + vnoise(b + vec2<f32>(0.0, e * 2.3)) * 0.5;
    return vec2<f32>(hx - h0, hz - h0) / e;
}

// Поглощение света в воде, 1/м (красный — сильнее всего): вода Понта зеленовато-синяя.
fn sigma() -> vec3<f32> {
    return vec3<f32>(0.55, 0.2, 0.21) * water.deep.w;
}

// Блики на дне (каустика): свет, собранный в сетку волнами наверху. 0..~1.
fn caustic(p: vec2<f32>, t: f32) -> f32 {
    let q = (p * 0.55) % vec2<f32>(6.2831853) - vec2<f32>(250.0);
    var i = q;
    var c = 1.0;
    let inten = 0.005;
    for (var n = 0; n < 4; n++) {
        let tt = t * 0.55 * (1.0 - 3.5 / f32(n + 1));
        i = q + vec2<f32>(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
        c += 1.0 / length(vec2<f32>(q.x / (sin(i.x + tt) / inten), q.y / (cos(i.y + tt) / inten)));
    }
    c /= 4.0;
    c = 1.17 - pow(c, 1.4);
    return clamp(pow(abs(c), 8.0), 0.0, 2.0);
}

fn screen_uv(frag: vec2<f32>) -> vec2<f32> {
    return (frag - view.viewport.xy) / view.viewport.zw;
}

// Глубина сцены (вид, z < 0 перед камерой) в точке экрана.
fn scene_view_z(uv: vec2<f32>) -> f32 {
    var z = -1.0e6;
    @if(DEPTH_PREPASS) {
        let px = vec2<i32>(clamp(uv, vec2<f32>(0.0), vec2<f32>(0.9999)) * view.viewport.zw + view.viewport.xy);
        let d = textureLoad(depth_prepass_texture, px, 0);
        z = depth_ndc_to_view_z(d);
    }
    return z;
}

fn scene_color(uv: vec2<f32>) -> vec3<f32> {
    var c = vec3<f32>(0.0);
    @if(VIEW_TRANSMISSION_TEXTURE) {
        c = textureSampleLevel(view_transmission_texture, view_transmission_sampler, clamp(uv, vec2<f32>(0.001), vec2<f32>(0.999)), 0.0).rgb;
    }
    return c;
}

// Отражение по экрану: шагаем по отражённому лучу, пока он не уйдёт за то, что нарисовано
// в кадре. Возвращает цвет и уверенность (0 — луч ушёл за кадр или в небо).
fn trace_reflection(wp: vec3<f32>, r: vec3<f32>, jitter: f32) -> vec4<f32> {
    var hit = vec4<f32>(0.0);
    @if(DEPTH_PREPASS && VIEW_TRANSMISSION_TEXTURE) {
        var s = 0.6 + jitter * 0.6;
        var prev = 0.0;
        for (var i = 0; i < 20; i++) {
            let p = wp + r * s;
            let clip = position_world_to_clip(p);
            if clip.w <= 0.0 {
                break;
            }
            let ndc = clip.xy / clip.w;
            let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) {
                break;
            }
            let ray_z = position_world_to_view(p).z;
            let sz = scene_view_z(uv);
            let behind = sz - ray_z;
            // Луч зашёл за поверхность, но не слишком глубоко (иначе это то, что перед ним).
            if behind > 0.0 && behind < max(s - prev, 0.6) * 1.6 + 0.3 {
                // Уточняем точку пополам.
                var a = prev;
                var b = s;
                for (var j = 0; j < 4; j++) {
                    let m = (a + b) * 0.5;
                    let pm = wp + r * m;
                    let cm = position_world_to_clip(pm);
                    let um = vec2<f32>(cm.x / cm.w * 0.5 + 0.5, 0.5 - cm.y / cm.w * 0.5);
                    if scene_view_z(um) - position_world_to_view(pm).z > 0.0 {
                        b = m;
                    } else {
                        a = m;
                    }
                }
                let pf = wp + r * b;
                let cf = position_world_to_clip(pf);
                let uf = vec2<f32>(cf.x / cf.w * 0.5 + 0.5, 0.5 - cf.y / cf.w * 0.5);
                let edge = smoothstep(0.0, 0.08, min(min(uf.x, 1.0 - uf.x), min(uf.y, 1.0 - uf.y)));
                // Чем дальше ушёл луч, тем меньше доверия (и меньше разрыв с небом).
                let far = 1.0 - smoothstep(120.0, 260.0, b);
                hit = vec4<f32>(scene_color(uf), edge * far);
                break;
            }
            prev = s;
            s = s * 1.32 + 0.35;
        }
    }
    return hit;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let wp = in.world_position.xyz;
    let t = globals.time;
    let to_cam = view.world_position - wp;
    let dist = length(to_cam);
    let v = to_cam / dist;
    let m = map_at(wp.xz);
    let depth_map = m.r * 16.0;
    let calm = m.g;
    let p = wp.xz * water.params.y;

    // Камера под этой точкой поверхности: смотрим на воду снизу.
    let below = v.y < 0.0;

    // Нормаль: крупные волны — из вершин, мелкие и рябь — здесь. Вдали мелочь гаснет
    // (иначе мерцает), её заменяет шероховатость блика. Простая вода — без ряби.
    let fade_small = (1.0 - smoothstep(25.0, 160.0, dist)) * (1.0 - water.params.w);
    var g = vec2<f32>(0.0);
    let small = mix(0.45, 1.0, calm);
    g += wave_grad(p, normalize(vec2<f32>(0.95, -0.3)), 1.45, 0.06, t) * fade_small * small;
    if water.params.w < 0.5 {
        g += wave_grad(p, normalize(vec2<f32>(-0.4, 0.9)), 0.9, 0.05, t) * fade_small * small;
        g += wave_grad(p, normalize(vec2<f32>(0.6, -0.8)), 0.55, 0.04, t) * fade_small * small;
        g += ripples(p, t) * 0.04 * fade_small;
    }
    let big = normalize(in.world_normal);
    var N = normalize(vec3<f32>(big.x - g.x, big.y, big.z - g.y));

    // Пена: у берега (рваная полоса, бегущая к берегу) и на крутых гребнях.
    var crest = 0.0;
    @if(VERTEX_UVS_A) {
        crest = in.uv.x;
    }
    let foam_noise = fbm3(wp.xz * 0.9 + vec2<f32>(t * 0.12, -t * 0.07));
    let foam_fine = vnoise(wp.xz * 6.0 + vec2<f32>(t * 0.3, 0.0));

    if below {
        // ---- Снизу, из-под воды ----
        let n_down = -N;
        // Луч из воды в воздух: окно Снелла (≈ 97°), за ним — полное внутреннее отражение.
        let tr = refract(-v, n_down, 1.333);
        var col: vec3<f32>;
        // Цвет толщи снизу — как дымка под водой (swim.rs), иначе на стыке видна граница.
        let scatter = vec3<f32>(0.05, 0.22, 0.24) * (0.25 + 0.75 * smoothstep(-0.05, 0.4, water.sky.sun.y)) * (1.0 - 0.6 * clamp(water.sky.extra.x, 0.0, 1.0));
        if dot(tr, tr) < 1e-4 {
            col = scatter;
        } else {
            let cos_t = max(dot(-v, n_down), 0.0);
            let fres = 0.02 + 0.98 * pow(1.0 - cos_t, 5.0);
            var s = water.sky;
            s.zenith.w = t;
            var above = sky_radiance(normalize(tr), s, t, true);
            @if(VIEW_TRANSMISSION_TEXTURE) {
                let uv = screen_uv(in.position.xy) + (tr.xz - (-v).xz) * 0.05;
                above = scene_color(uv);
            }
            col = mix(above, scatter, clamp(fres * 1.6, 0.0, 1.0));
        }
        pbr_input.material.base_color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
        pbr_input.material.reflectance = vec3<f32>(0.0);
        pbr_input.material.emissive = vec4<f32>(0.0);
        var out: FragmentOutput;
        var lit = apply_pbr_lighting(pbr_input);
        lit = vec4<f32>(lit.rgb + col, 1.0);
        out.color = main_pass_post_lighting_processing(pbr_input, lit);
        return out;
    }

    // ---- Сверху ----
    // Толща воды по лучу (м) до дна или сваи под этой точкой.
    var ray_len = depth_map / max(v.y, 0.08);
    var have_depth = false;
    let uv0 = screen_uv(in.position.xy);
    let water_z = position_world_to_view(wp).z;
    @if(DEPTH_PREPASS) {
        if water.params.z > 0.5 {
            ray_len = max(water_z - scene_view_z(uv0), 0.0);
            have_depth = true;
        }
    }
    let thick = ray_len * max(v.y, 0.05);

    // Прибой: полосы пены бегут к берегу и рвутся на кружево.
    let shore_band = 1.0 - smoothstep(0.0, 0.45, thick + (foam_noise - 0.5) * 0.3);
    let surf = 0.5 + 0.5 * sin(thick * 10.0 - t * 1.5 + foam_noise * 5.0);
    let lace = smoothstep(0.25, 0.6, foam_fine * 0.6 + foam_noise * 0.6);
    var foam = shore_band * mix(0.35, 1.0, surf) * mix(0.45, 1.0, lace);
    // Гребни: где волна сжата сильнее всего, рваными пятнами.
    let crest_foam = smoothstep(0.32, 0.62, crest + (foam_noise - 0.5) * 0.35) * smoothstep(0.35, 0.75, foam_fine + foam_noise * 0.4);
    foam = max(foam, crest_foam * 0.85 * (1.0 - smoothstep(150.0, 400.0, dist)));
    foam = clamp(foam * water.shallow.w, 0.0, 1.0);
    N = normalize(mix(N, vec3<f32>(0.0, 1.0, 0.0), foam * 0.6));

    // Френель (Шлик, n = 1.33).
    let nv = max(dot(N, v), 0.0);
    var fres = 0.02 + 0.98 * pow(1.0 - nv, 5.0);
    // Вдали рябь не рисуется, но она есть: склоны мелких волн повёрнуты к зрителю, поэтому
    // настоящее море у горизонта не зеркало — оно темнее неба и отражает небо повыше.
    // Полосы порывов (вытянуты по ветру) дают тёмные и светлые дорожки на воде.
    let far_k = smoothstep(30.0, 350.0, dist) * (1.0 - water.params.w);
    let gust = smoothstep(0.3, 0.7, fbm3(vec2<f32>(wp.x * 0.004 + t * 0.004, wp.z * 0.022 - t * 0.002)));
    fres = mix(fres, min(fres, mix(0.18, 0.4, gust)), far_k);

    // Отражение неба (луч не ниже горизонта — там море видит само себя).
    var r = reflect(-v, N);
    r.y = max(r.y, 0.004) + mix(0.24, 0.08, gust) * far_k;
    r = normalize(r);
    var s = water.sky;
    s.zenith.w = t;
    var refl = sky_radiance(r, s, t, false);
    // Город, стены, корабли — трассировкой по кадру.
    if water.waves.z > 0.5 && water.params.w < 0.5 {
        let jitter = fract(sin(dot(in.position.xy, vec2<f32>(12.9898, 78.233))) * 43758.5453);
        let h = trace_reflection(wp, normalize(r), jitter);
        refl = mix(refl, h.rgb, h.a);
    }

    // Солнечная дорожка: GGX-блик, шероховатость растёт с расстоянием.
    let sun = normalize(s.sun.xyz);
    let hv = normalize(sun + v);
    let nh = max(dot(N, hv), 0.0);
    let rough = mix(0.035, 0.17, smoothstep(15.0, 500.0, dist));
    let a2 = rough * rough * rough * rough;
    let dd = nh * nh * (a2 - 1.0) + 1.0;
    let ggx = a2 / (3.14159265 * dd * dd);
    let nl = max(dot(N, sun), 0.0);
    let f_sun = 0.02 + 0.98 * pow(1.0 - max(dot(hv, v), 0.0), 5.0);
    var sun_col = mix(vec3<f32>(1.0, 0.62, 0.28), vec3<f32>(1.0, 0.95, 0.85), clamp(s.sun.w, 0.0, 1.0));
    sun_col *= smoothstep(-0.02, 0.03, sun.y) * (1.0 - 0.85 * clamp(s.extra.y, 0.0, 1.0));
    let spec = sun_col * ggx * f_sun * nl / max(4.0 * nv * nl + 0.05, 0.05) * water.params.x;
    let moon = normalize(s.moon.xyz);
    let hm = normalize(moon + v);
    let nhm = max(dot(N, hm), 0.0);
    let dm = nhm * nhm * (a2 - 1.0) + 1.0;
    let moon_k = s.moon.w * clamp(s.extra.x, 0.0, 1.0) * smoothstep(0.0, 0.08, moon.y) * (1.0 - 0.9 * clamp(s.extra.y, 0.0, 1.0));
    let spec_moon = vec3<f32>(0.55, 0.6, 0.7) * a2 / (3.14159265 * dm * dm) * f_sun * max(dot(N, moon), 0.0) / max(4.0 * nv * max(dot(N, moon), 0.0) + 0.05, 0.05) * water.params.x * 0.08 * moon_k;

    // Освещённость толщи и пены (рассеянный свет, солнце с тенями от берега и кораблей,
    // факелы): один расчёт для белой поверхности, потом домножаем на цвет.
    pbr_input.material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    pbr_input.material.perceptual_roughness = 0.9;
    pbr_input.material.reflectance = vec3<f32>(0.0);
    pbr_input.material.metallic = 0.0;
    pbr_input.material.emissive = vec4<f32>(0.0);
    pbr_input.N = normalize(mix(vec3<f32>(0.0, 1.0, 0.0), N, 0.35));
    let light = apply_pbr_lighting(pbr_input).rgb;
    // Толщу в тени подсвечивает рассеянный свет соседней освещённой воды, поэтому тень корабля
    // или набережной в воде мягче и светлее, чем на суше: берём свет и с тенями, и без них.
    var open_input = pbr_input;
    open_input.flags = pbr_input.flags & ~(1u << 29u);
    let light_open = apply_pbr_lighting(open_input).rgb;
    let light_body = mix(light, light_open, 0.75);

    // Рассеяние на гребнях волн, просвеченных низким солнцем.
    let sss = pow(max(dot(-v, sun), 0.0), 3.0) * clamp(crest * 1.5 + 0.15, 0.0, 1.0) * smoothstep(0.0, 0.2, sun.y) * 1.6;
    let inscatter = water.deep.rgb * light_body + water.deep.rgb * sss * sun_col * 0.6;

    var col: vec3<f32>;
    var alpha = 1.0;
    var refracted = false;
    @if(VIEW_TRANSMISSION_TEXTURE) {
        refracted = true;
        // Сдвиг по нормали тем сильнее, чем толще вода (у кромки — почти без сдвига).
        let k = clamp(ray_len * 0.35, 0.0, 1.0) * 0.045 / max(1.0, dist * 0.06);
        var uv = uv0 + vec2<f32>(N.x - big.x, -(N.z - big.z)) * k * 3.0 + vec2<f32>(N.x, -N.z) * k;
        var len = ray_len;
        if have_depth {
            // Под сдвигом оказалось то, что над водой (свая, лодка) — берём без сдвига.
            let sz = scene_view_z(uv);
            if sz > water_z {
                uv = uv0;
            } else {
                len = max(water_z - sz, 0.0);
            }
        }
        var bottom = scene_color(uv);
        // Дно сквозь толщу размыто тем сильнее, чем толще вода: резкие края теней и швы
        // на дне не проступают квадратами. Соседей над водой (сваи, борт) не подмешиваем.
        if have_depth {
            let r = clamp(len * 0.012, 0.0, 0.03) / max(1.0, dist * 0.05);
            if r > 0.0005 {
                var sum = bottom;
                var n = 1.0;
                let offs = array<vec2<f32>, 6>(vec2(1.0, 0.0), vec2(-0.5, 0.87), vec2(-0.5, -0.87), vec2(0.5, 0.87), vec2(-1.0, 0.0), vec2(0.5, -0.87));
                for (var i = 0; i < 6; i++) {
                    let o = offs[i] * r * select(1.0, 0.5, i >= 3);
                    let q = uv + vec2<f32>(o.x, o.y * view.viewport.z / max(view.viewport.w, 1.0));
                    if scene_view_z(q) < water_z {
                        sum += scene_color(q);
                        n += 1.0;
                    }
                }
                bottom = sum / n;
            }
        }
        // Каустика на дне: точка дна по лучу, блики гаснут с глубиной и в тени туч.
        if have_depth && water.params.w < 0.5 {
            let fwd = -normalize(view.world_from_view[2].xyz);
            let along = len / max(dot(-v, fwd), 0.15);
            let bp = wp - v * along;
            let d = max(water.waves.y - bp.y, 0.0);
            let sun_k = smoothstep(0.0, 0.25, water.sky.sun.y) * (1.0 - 0.8 * clamp(water.sky.extra.y, 0.0, 1.0));
            let ck = caustic(bp.xz + N.xz * 0.6, t) * exp(-d * 0.22) * smoothstep(0.05, 0.6, d) * sun_k * (1.0 - smoothstep(40.0, 120.0, dist));
            bottom *= 1.0 + ck * 1.6;
        }
        // Свет идёт до дна сверху и потом от дна к глазу: гаснет на обоих отрезках.
        let tr = exp(-sigma() * (len + len * max(v.y, 0.05)));
        let through = bottom * tr + inscatter * (vec3<f32>(1.0) - tr);
        col = mix(through, refl, fres) + spec + spec_moon;
    }
    if !refracted {
        // Простая вода: полупрозрачный слой поверх дна (как раньше).
        let absorb = 1.0 - exp(-thick * 0.55);
        let body = mix(water.shallow.rgb, water.deep.rgb, absorb) * light;
        col = body + refl * fres + spec + spec_moon;
        alpha = clamp(max(absorb, fres) * 0.92 + 0.08, 0.0, 1.0);
    }
    // Пена поверх всего: белая, освещённая так же, как вода.
    let foam_col = vec3<f32>(0.86, 0.88, 0.87) * light;
    col = mix(col, foam_col, foam);
    alpha = max(alpha, foam);

    var out: FragmentOutput;
    out.color = main_pass_post_lighting_processing(pbr_input, vec4<f32>(col, alpha));
    return out;
}
