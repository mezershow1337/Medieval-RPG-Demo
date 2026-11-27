// Карта объёмных кучевых облаков на всё небо (круг «рыбий глаз», см. dome_dir в sky_common).
//
// Для каждой точки карты луч идёт сквозь слой облаков (1,2–3,2 км над землёй) и на каждом шаге
// читает плотность из картинки шума (cloud_noise.rs), а от каждой точки в облаке — ещё
// несколько шагов к солнцу: сколько облака между ней и светом. Отсюда сами собой получаются
// белые освещённые бока, серые днища, тени одного клуба на другом и светящаяся кромка против
// солнца. Карта рисуется полосками, по нескольку строк за кадр (cloud_map.rs), поэтому шагов
// можно не жалеть; купол неба (sky.wesl) её только читает.

import medieval_rpg::shaders::sky_common::{SkyParams, horizon_color, dome_dir};

// origin: xyz — откуда снимается карта (камера в начале круга);
// wind: xy — на сколько метров ветер унёс облака, z — номер круга, w — сторона карты в точках;
// quality: x — шагов луча, y — шагов к солнцу.
struct CloudView {
    origin: vec4<f32>,
    wind: vec4<f32>,
    quality: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0)
var<uniform> sky: SkyParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1)
var cloud_noise: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2)
var cloud_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3)
var<uniform> cloud: CloudView;

// Слой кучевых облаков, м над уровнем моря.
const CLOUD_Y0: f32 = 1150.0;
const CLOUD_Y1: f32 = 3200.0;
// Сторона плитки шума формы облаков, м.
const CLOUD_TILE: f32 = 8200.0;

fn remap01(x: f32, a: f32, b: f32) -> f32 {
    return clamp((x - a) / (b - a), 0.0, 1.0);
}

// Объёмный шум из плоской картинки: по высоте — слои, каждый читает картинку со своим
// сдвигом, между слоями плавный переход.
fn noise_slices(uv: vec2<f32>, z: f32, lod: f32) -> vec4<f32> {
    let zi = floor(z);
    let f = z - zi;
    let k = f * f * (3.0 - 2.0 * f);
    let a = textureSampleLevel(cloud_noise, cloud_sampler, uv + vec2<f32>(0.37, 0.61) * zi, lod);
    let b = textureSampleLevel(cloud_noise, cloud_sampler, uv + vec2<f32>(0.37, 0.61) * (zi + 1.0), lod);
    return mix(a, b, k);
}

// Плотность облака в точке (0 — пусто). `cover` — облачность 0..1, `lod` — уровень шума
// (дальние облака читают крупные мип-уровни: не рябят и дешевле).
fn cloud_density(p: vec3<f32>, wind: vec2<f32>, cover: f32, lod: f32, fine: bool) -> f32 {
    let h = (p.y - CLOUD_Y0) / (CLOUD_Y1 - CLOUD_Y0);
    if h <= 0.0 || h >= 1.0 {
        return 0.0;
    }
    let uv = (p.xz + wind) / CLOUD_TILE;
    // Карта погоды: крупные гряды и просветы между ними; где гряда — облака выше.
    let weather = textureSampleLevel(cloud_noise, cloud_sampler, uv * 0.23 + vec2<f32>(0.31, 0.57), 0.0).a;
    var top = mix(0.25, 1.0, smoothstep(0.3, 0.8, weather));
    // Макушки разной высоты: башни и провалы между ними, а не ровная «колбаса».
    let bump = textureSampleLevel(cloud_noise, cloud_sampler, uv * 3.1 + vec2<f32>(0.13, 0.71), lod).g;
    // Купол над каждой ячейкой шума: макушки круглые, а не конусами.
    top *= 1.0 - 0.55 * (1.0 - sqrt(clamp(1.0 - pow((1.0 - bump) / 0.6, 2.0), 0.0, 1.0)));
    // Днище плоское и резкое, макушка круглая.
    let shape = textureSampleLevel(cloud_noise, cloud_sampler, uv, lod).r;
    let local = clamp(cover * mix(0.55, 1.3, weather), 0.0, 1.0);
    // К краям облако ниже (в разрезе — купол): иначе по краю висит тонкая отвесная «занавеска».
    top *= mix(0.3, 1.0, sqrt(remap01(shape, 1.0 - local, 1.0 - local * 0.45)));
    let profile = smoothstep(0.0, 0.07, h) * (1.0 - smoothstep(top * 0.45, top, h));
    var d = remap01(shape * profile, 1.0 - local, 1.0 - local * 0.5);
    if d <= 0.0 {
        return 0.0;
    }
    if fine {
        // Края выедает ячеистый шум: внизу клочьями, вверху клубами. Шум объёмный (слои по
        // высоте): иначе бока облаков выходят «расчёсанными» сверху вниз.
        let det = noise_slices(uv * 7.0, p.y / 170.0, lod + 1.0).g;
        let wisp = noise_slices(uv * 23.0, p.y / 55.0, lod + 2.0).b;
        // Клубы покрупнее — тоже объёмные: сбоку облако не похоже на ряд «органных труб».
        let mid = noise_slices(uv * 3.3, p.y / 420.0, lod + 0.5).g;
        let ero = mix(1.0 - det, det, clamp(h * 3.0, 0.0, 1.0)) * 0.52 + (1.0 - mid) * 0.24 + (1.0 - wisp) * 0.24;
        d = remap01(d, 0.06 + ero * 0.76, 1.0);
    }
    return d;
}

// Доля света в сторону зрителя (Хеньи — Гринстейн).
fn hg(c: f32, g: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (12.566 * pow(1.0 + g2 - 2.0 * g * c, 1.5));
}

// Случайное число точки карты (белый шум: соседние точки не похожи друг на друга).
fn rnd(p: vec2<f32>) -> f32 {
    var q = fract(p * vec2<f32>(0.1031, 0.1030));
    q += dot(q, q.yx + 33.33);
    return fract((q.x + q.y) * q.y);
}

// rgb — свет облаков (уже умноженный на их непрозрачность), a — непрозрачность.
fn volumetric_clouds(dir: vec3<f32>, s: SkyParams, frag: vec2<f32>) -> vec4<f32> {
    let steps = i32(cloud.quality.x + 0.5);
    if steps < 1 || dir.y < 0.012 {
        return vec4<f32>(0.0);
    }
    let cam = cloud.origin.xyz;
    let t0 = (CLOUD_Y0 - cam.y) / dir.y;
    // Вблизи горизонта луч шёл бы сквозь слой десятки километров — берём первые 14 км.
    let len = min((CLOUD_Y1 - CLOUD_Y0) / dir.y, 14000.0);
    let sun = normalize(s.sun.xyz);
    let oc = clamp(s.extra.y, 0.0, 1.0);
    let cover = clamp(s.horizon.w * 0.68 + 0.46 * oc, 0.0, 1.0);
    let wind = cloud.wind.xy;
    // Сдвиг шагов свой у каждой точки и у каждого круга: вместо полос — мелкое зерно.
    let jitter = rnd(frag + 17.0 * (cloud.wind.z % 64.0));
    let cs = dot(dir, sun);
    // Свет вперёд (яркая кромка против солнца) и назад (общий белый свет облака).
    let phase = mix(hg(cs, 0.62), hg(cs, -0.25), 0.45) * 12.566;
    let light_steps = i32(cloud.quality.y + 0.5);
    // Плотность → ослабление света на метр.
    let sigma = 0.03;
    let sun_c = mix(vec3<f32>(1.6, 1.05, 0.6), vec3<f32>(1.88, 1.81, 1.7), clamp(s.sun.w, 0.0, 1.0)) * (1.0 - 0.8 * oc);
    let amb_top = mix(vec3<f32>(0.47, 0.54, 0.68), vec3<f32>(0.6, 0.62, 0.66), oc);
    let amb_bot = mix(vec3<f32>(0.26, 0.3, 0.39), vec3<f32>(0.3, 0.31, 0.34), oc);
    var trans = 1.0;
    var col = vec3<f32>(0.0);
    var depth = 0.0;
    var wsum = 0.0;
    let n = f32(steps);
    for (var i = 0; i < steps; i++) {
        // Ближе к зрителю шаги мельче: ближние облака крупнее на экране.
        let ta = len * pow(f32(i) / n, 1.4);
        let tb = len * pow(f32(i + 1) / n, 1.4);
        let dt = tb - ta;
        let tt = t0 + ta + dt * jitter;
        let p = cam + dir * tt;
        let lod = clamp(log2(max(tt, 1.0) / 5000.0), 0.0, 4.0);
        let d = cloud_density(p, wind, cover, lod, true);
        if d > 0.002 {
            // Сколько облака между точкой и солнцем.
            // Путь к солнцу — отрезками, каждый длиннее прежнего; плотность берём около середины
            // отрезка, со случайным сдвигом (в заданных точках тень ложилась бы ступенями).
            var od = 0.0;
            var ls = 30.0;
            var along = 0.0;
            for (var j = 0; j < light_steps; j++) {
                let u = 0.5 + (fract(jitter + 0.618 * f32(i) + 0.382 * f32(j)) - 0.5) * 0.5;
                od += cloud_density(p + sun * (along + ls * u), wind, cover, lod + 1.0, j < 3) * ls;
                along += ls;
                ls *= 1.9;
            }
            let h = (p.y - CLOUD_Y0) / (CLOUD_Y1 - CLOUD_Y0);
            // Закон Бера и добавка многократного рассеяния: в глубине облако серое, а не чёрное.
            let beer = max(exp(-od * sigma * 0.7), 0.62 * exp(-od * sigma * 0.14));
            // У самой кромки, обращённой к солнцу, облако чуть темнее («сахарная пудра»).
            let powder = 1.0 - 0.3 * exp(-d * 6.0 - od * sigma * 2.0);
            let lit = sun_c * beer * powder * mix(0.62, 1.0, clamp(phase, 0.0, 1.6) / 1.6) + sun_c * max(phase - 1.6, 0.0) * 0.22 * beer;
            let amb = mix(amb_bot, amb_top, clamp(h * 1.4, 0.0, 1.0)) * (0.45 + 0.55 * exp(-d * 2.0));
            let a = 1.0 - exp(-d * sigma * dt);
            col += trans * a * (lit + amb);
            depth += trans * a * tt;
            wsum += trans * a;
            trans *= 1.0 - a;
            if trans < 0.02 {
                break;
            }
        }
    }
    // Дальние облака тонут в дымке горизонта.
    let far = depth / max(wsum, 1e-4);
    let haze = 1.0 - exp(-far / 26000.0);
    let hor = horizon_color(dir, s);
    col = mix(col, hor * (1.0 - trans), haze * 0.9);
    // У самого горизонта слой сходит на нет (там дымка).
    let edge = smoothstep(0.012, 0.05, dir.y);
    return vec4<f32>(col * edge, (1.0 - trans) * edge);
}

@fragment
fn fragment(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let d = dome_dir(frag.xy / cloud.wind.w);
    if d.w < 0.5 {
        return vec4<f32>(0.0);
    }
    let v = volumetric_clouds(d.xyz, sky, frag.xy);
    // Свет пишем с множителем 0,4: в восьми битах помещаются кромки ярче единицы.
    return vec4<f32>(clamp(v.rgb * 0.4, vec3<f32>(0.0), vec3<f32>(1.0)), v.a);
}
