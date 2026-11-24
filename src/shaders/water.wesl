// Море: зыбь из нескольких систем волн и ряби, отражение неба по Френелю (то же небо,
// что на куполе, с облаками), солнечная дорожка (блик GGX, который вдали расплывается
// в широкую полосу, как на настоящей воде), толща воды с рассеянием, прозрачность и пена
// у берега по глубине из предварительного прохода, свет факелов и туман — от PBR Bevy.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    mesh_view_bindings::{globals, view},
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    view_transformations::{depth_ndc_to_view_z, position_world_to_view},
};
import medieval_rpg::shaders::sky_common::{SkyParams, sky_radiance, vnoise, fbm3};
@if(DEPTH_PREPASS)
import bevy_pbr::prepass::utils::prepass_depth;

struct Water {
    sky: SkyParams,
    // Цвет толщи на глубине (линейный), w — плотность (1/м).
    deep: vec4<f32>,
    // Цвет мелководья, w — сила пены.
    shallow: vec4<f32>,
    // x — сила солнечного блика, y — масштаб волн, z — 1, если есть глубина из предпрохода.
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> water: Water;

// Наклон поверхности от одной системы волн: направление, длина волны, крутизна, скорость.
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

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let wp = in.world_position.xyz;
    let t = globals.time;
    let to_cam = view.world_position - wp;
    let dist = length(to_cam);
    let v = to_cam / dist;
    let sc = water.params.y;
    let p = wp.xz * sc;

    // Волны: вдали мелкие гасим (иначе мерцают) — их заменяет шероховатость блика.
    // Простая вода (настройка «Вода»): без мелкой ряби — только крупные волны.
    let fade_small = (1.0 - smoothstep(25.0, 160.0, dist)) * (1.0 - water.params.w);
    let fade_mid = 1.0 - smoothstep(120.0, 700.0, dist);
    var g = vec2<f32>(0.0);
    g += wave_grad(p, normalize(vec2<f32>(0.8, 0.6)), 23.0, 0.10, t) * fade_mid;
    g += wave_grad(p, normalize(vec2<f32>(0.3, 0.95)), 11.0, 0.09, t) * fade_mid;
    g += wave_grad(p, normalize(vec2<f32>(0.95, -0.3)), 6.3, 0.08, t) * mix(0.3, 1.0, fade_small);
    if water.params.w < 0.5 {
        g += wave_grad(p, normalize(vec2<f32>(-0.4, 0.9)), 3.1, 0.07, t) * fade_small;
        g += wave_grad(p, normalize(vec2<f32>(0.6, -0.8)), 1.7, 0.06, t) * fade_small;
        g += ripples(p, t) * 0.045 * fade_small;
    }
    let N = normalize(vec3<f32>(-g.x, 1.0, -g.y));

    // Толща воды под этой точкой: по глубине сцены из предварительного прохода.
    var thick = 60.0;
    @if(DEPTH_PREPASS) {
        if water.params.z > 0.5 {
            let scene_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
            let water_z = position_world_to_view(wp).z;
            // Вдоль луча → по вертикали.
            thick = max(water_z - scene_z, 0.0) * max(v.y, 0.05);
        }
    }

    // Пена у берега и у бортов: рваная полоса, бегущая к берегу.
    let foam_noise = fbm3(wp.xz * 0.9 + vec2<f32>(t * 0.12, -t * 0.07));
    let surf = 0.5 + 0.5 * sin(thick * 9.0 - t * 1.6 + foam_noise * 5.0);
    var foam = (1.0 - smoothstep(0.0, 0.55, thick + (foam_noise - 0.5) * 0.35)) * mix(0.55, 1.0, surf);
    foam = clamp(foam * water.shallow.w, 0.0, 1.0);

    // Френель (Шлик, n = 1.33).
    let nv = max(dot(N, v), 0.0);
    let fres = 0.02 + 0.98 * pow(1.0 - nv, 5.0);

    // Отражение неба (луч не ниже горизонта — там море видит само себя).
    var r = reflect(-v, N);
    r.y = max(r.y, 0.004);
    var s = water.sky;
    s.zenith.w = t;
    let refl = sky_radiance(r, s, t, false);

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
    // Цвет блика: днём белый, на закате золотой; ниже горизонта солнца нет — ночью
    // дорожку на воде даёт луна; в ненастье блика почти не видно.
    var sun_col = mix(vec3<f32>(1.0, 0.62, 0.28), vec3<f32>(1.0, 0.95, 0.85), clamp(s.sun.w, 0.0, 1.0));
    sun_col *= smoothstep(-0.02, 0.03, sun.y) * (1.0 - 0.85 * clamp(s.extra.y, 0.0, 1.0));
    let spec = sun_col * ggx * f_sun * nl / max(4.0 * nv * nl + 0.05, 0.05) * water.params.x;
    // Лунная дорожка.
    let moon = normalize(s.moon.xyz);
    let hm = normalize(moon + v);
    let nhm = max(dot(N, hm), 0.0);
    let dm = nhm * nhm * (a2 - 1.0) + 1.0;
    let moon_k = s.moon.w * clamp(s.extra.x, 0.0, 1.0) * smoothstep(0.0, 0.08, moon.y) * (1.0 - 0.9 * clamp(s.extra.y, 0.0, 1.0));
    let spec_moon = vec3<f32>(0.55, 0.6, 0.7) * a2 / (3.14159265 * dm * dm) * f_sun * max(dot(N, moon), 0.0) / max(4.0 * nv * max(dot(N, moon), 0.0) + 0.05, 0.05) * water.params.x * 0.08 * moon_k;

    // Толща: на мелководье видно дно (альфа меньше), глубже — тёмно-зелёная вода Понта.
    let absorb = 1.0 - exp(-thick * water.deep.w);
    let body = mix(water.shallow.rgb, water.deep.rgb, absorb);
    // Рассеяние на гребнях волн, просвеченных низким солнцем.
    let sss = pow(max(dot(-v, sun), 0.0), 3.0) * max(g.x * sun.x + g.y * sun.z, 0.0) * 2.0;

    pbr_input.material.base_color = vec4<f32>(mix(body, vec3<f32>(0.85, 0.87, 0.86), foam), 1.0);
    pbr_input.material.perceptual_roughness = mix(0.25, 0.9, foam);
    pbr_input.material.reflectance = vec3<f32>(0.0);
    pbr_input.material.metallic = 0.0;
    pbr_input.N = normalize(mix(N, vec3<f32>(0.0, 1.0, 0.0), foam));
    pbr_input.material.emissive = vec4<f32>(0.0);

    var out: FragmentOutput;
    var col = apply_pbr_lighting(pbr_input);
    // Отражение и блик — в единицах неба (без экспозиции), как купол.
    let mirror = (refl * fres + spec + spec_moon + water.deep.rgb * sss * sun_col) * (1.0 - foam);
    col = vec4<f32>(col.rgb + mirror, 1.0);
    // Прозрачность: у берега и при взгляде сверху вниз видно дно; под углом — зеркало.
    let alpha = clamp(max(max(absorb, fres), foam) * 0.92 + 0.08, 0.0, 1.0);
    col = vec4<f32>(col.rgb, alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, col);
    return out;
}
