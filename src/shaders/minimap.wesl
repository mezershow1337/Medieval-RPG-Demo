// Круглая мини-карта: кусок листа карты писца в бронзовом ободе.
// uv узла 0..1 — весь круг вместе с ободом; карта видна внутри радиуса RIM (в долях радиуса).

import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct MiniMap {
    // xy — доля листа в левом верхнем углу узла, z — сколько листа на всю ширину узла,
    // w — 1, когда лист уже нарисован.
    window: vec4<f32>,
}

@group(1) @binding(0) var<uniform> mini: MiniMap;
@group(1) @binding(1) var sheet_tex: texture_2d<f32>;
@group(1) @binding(2) var sheet_smp: sampler;

const RIM: f32 = 0.88;
// Цвета линейные (вывод шейдера — в линейном пространстве): пергамент, бронза, тёмная кромка.
const PARCHMENT: vec3<f32> = vec3<f32>(0.60, 0.46, 0.24);
const BRONZE: vec3<f32> = vec3<f32>(0.27, 0.13, 0.03);
const DARK: vec3<f32> = vec3<f32>(0.015, 0.007, 0.003);

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let p = in.uv * 2.0 - 1.0;
    let r = length(p);
    // Ширина пикселя в долях радиуса — для мягкого края без лесенки.
    let aa = 2.0 / max(in.size.x, 1.0);

    let uv = mini.window.xy + in.uv * mini.window.z;
    var map = textureSample(sheet_tex, sheet_smp, uv).rgb;
    let outside_sheet = any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0));
    if (mini.window.w < 0.5 || outside_sheet) {
        map = PARCHMENT;
    }
    // К краю карта слегка темнеет, как старый лист под стеклом.
    map = map * mix(1.0, 0.8, smoothstep(0.55, RIM, r));

    // Обод: литая бронза с фаской, свет сверху слева.
    let t = clamp((r - RIM) / (1.0 - RIM), 0.0, 1.0);
    let dir = p / max(r, 1e-4);
    let light = dot(dir, vec2<f32>(-0.55, -0.83));
    let bevel = cos(t * 3.14159);
    var rim = BRONZE * (0.85 + 0.35 * sin(t * 3.14159)) * (1.0 + 0.45 * light * bevel);
    rim = rim + vec3<f32>(0.35, 0.22, 0.08) * pow(max(light * bevel, 0.0), 6.0);

    var col = mix(map, rim, smoothstep(RIM - aa, RIM + aa, r));
    // Тонкие тёмные кромки: по краю карты и по внешнему краю обода.
    col = mix(col, DARK, (1.0 - smoothstep(0.0, aa * 1.5, abs(r - RIM))) * 0.85);
    col = mix(col, DARK, smoothstep(1.0 - aa * 3.0, 1.0 - aa, r) * 0.8);

    let alpha = 1.0 - smoothstep(1.0 - aa, 1.0, r);
    return vec4<f32>(col, alpha);
}
