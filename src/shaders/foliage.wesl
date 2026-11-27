// Листва, трава и кусты: качание на ветру в вершинном шейдере.
// Один и тот же сдвиг считается и в основном проходе, и в предпроходе (глубина, тени,
// векторы движения для TAA) — иначе листва мерцала бы и отрывалась от своих теней.
//
// Второй UV-канал меша: x — гибкость (0 у корня/ствола, 1 на концах ветвей), y — фаза.

import bevy_pbr::render::{
    mesh_functions,
    view_transformations::position_world_to_clip,
};
import bevy_render::globals::Globals;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::mesh_view_bindings::globals;

@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::{Vertex, VertexOutput, decompress_vertex};
@else
import bevy_pbr::render::forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex};
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
};
@if(!PREPASS_PIPELINE && VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;

struct Foliage {
    // x — насколько нормали листвы скруглены по кроне (0 — как в модели), y — сила ветра,
    // z — порывистость, w — высота середины кроны (в единицах модели).
    // Время берём из `globals` вида: материал меняется только при смене погоды, а не каждый
    // кадр (изменённый материал заставляет Bevy заново готовить все ~12 тыс. мешей листвы).
    wind: vec4<f32>,
    // xy — направление ветра (мир, XZ), z — жёсткость (1 — дерево, 0.3 — трава),
    // w — добавка рассеянного света в кроне (0 — как у всего остального).
    dir: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> foliage: Foliage;

// Время кадра. В основном проходе `globals` берём из привязок вида, а в предпроходе (глубина,
// тени, векторы движения) он лежит в другой привязке — объявляем его сами.
@if(PREPASS_PIPELINE)
@group(0) @binding(1) var<uniform> prepass_globals: Globals;

/// (время, длительность кадра), с.
fn frame_time() -> vec2<f32> {
@if(PREPASS_PIPELINE) {
    return vec2(prepass_globals.time, prepass_globals.delta_time);
} @else {
    return vec2(globals.time, globals.delta_time);
}
}

/// Сдвиг вершины ветром (в мировых координатах).
fn wind_offset(world: vec3<f32>, base: vec3<f32>, flex: f32, phase: f32, t: f32) -> vec3<f32> {
    let dir = normalize(vec3(foliage.dir.x, 0.0, foliage.dir.y) + vec3(1e-4, 0.0, 0.0));
    let strength = foliage.wind.y;
    // Медленное качание всего дерева (по положению дерева) + порывы, бегущие по местности.
    let gust = 0.55 + 0.45 * sin(t * 0.35 + dot(base.xz, vec2(0.021, 0.013)) * 6.0) * sin(t * 0.17 + base.x * 0.01);
    let sway = sin(t * (1.1 + 0.3 * foliage.dir.z) + phase * 6.2831 + dot(base.xz, vec2(0.13, 0.07)));
    let bend = flex * flex * strength * gust;
    // Мелкая дрожь листвы.
    let flutter = sin(t * 7.3 + phase * 40.0 + dot(world, vec3(2.1, 1.7, 2.9))) * flex * 0.35 * foliage.wind.z;
    var off = dir * (bend * (0.7 + 0.5 * sway)) + vec3(0.0, -bend * 0.15, 0.0);
    off += vec3(flutter, flutter * 0.6, -flutter) * strength * 0.08 * foliage.dir.z;
    return off;
}

/// Нормаль листа, скруглённая по кроне: каждая карточка листвы смотрит куда попало, и с
/// нормалями модели крона пестрит светлыми и тёмными лоскутами. С нормалями «от середины
/// кроны» дерево освещено как один объём: солнечный бок светлый, теневой тёмный.
fn crown_normal(n: vec3<f32>, world: vec3<f32>, world_from_local: mat4x4<f32>) -> vec3<f32> {
    let k = foliage.wind.x;
    if k <= 0.0 {
        return n;
    }
    let center = world_from_local[3].xyz + world_from_local[1].xyz * foliage.wind.w;
    // Низ кроны смотрит вбок, а не в землю: иначе снизу куст чёрный.
    let rn = normalize(world - center);
    let up = normalize(world_from_local[1].xyz);
    let round = normalize(rn + up * (0.3 + 0.6 * max(-dot(rn, up), 0.0)));
    return normalize(mix(n, round, k));
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let base = world_from_local[3].xyz;
    var flex = 0.0;
    var phase = 0.0;
@if(VERTEX_UVS_B) {
    flex = vertex.uv_b.x;
    phase = vertex.uv_b.y;
}
    // Масштаб дерева: большие деревья гнутся сильнее в метрах.
    let scale = length(world_from_local[1].xyz);
    let off = wind_offset(world.xyz, base, flex, phase, frame_time().x) * scale;
    out.world_position = vec4(world.xyz + off, 1.0);
    out.position = position_world_to_clip(out.world_position.xyz);

@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;

@if(PREPASS_PIPELINE) {
@if(UNCLIPPED_DEPTH_ORTHO_EMULATION) {
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
}
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
    out.world_normal = crown_normal(mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index), world.xyz, world_from_local);
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
@if(MOTION_VECTOR_PREPASS) {
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_in.instance_index);
    let prev_world = mesh_functions::mesh_position_local_to_world(prev_model, vec4<f32>(vertex.position, 1.0));
    let prev_off = wind_offset(prev_world.xyz, prev_model[3].xyz, flex, phase, frame_time().x - frame_time().y) * scale;
    out.previous_world_position = vec4(prev_world.xyz + prev_off, 1.0);
}
} @else {
@if(VERTEX_NORMALS)
    out.world_normal = crown_normal(mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index), world.xyz, world_from_local);
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
}

@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(vertex_in.instance_index, world_from_local[3]);
    return out;
}

// Цвет листвы — обычный расчёт света Bevy, но без затенения по экрану (SSAO) в полную силу:
// в густой кроне карточки листвы заслоняют друг друга, и SSAO съедал там весь рассеянный
// свет — тенистый низ кустов и можжевельников выходил чёрной кляксой. Листва же пропускает
// и рассеивает свет, в тени кроны она тёмно-зелёная, а не чёрная.
@if(!PREPASS_PIPELINE)
@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
    // Крона ещё и светится насквозь: свет неба проходит через хвою и листья, поэтому теневой
    // бок дерева получает больше рассеянного света, чем глухая стена.
    pbr_input.diffuse_occlusion = max(pbr_input.diffuse_occlusion, vec3<f32>(0.8)) * (1.0 + foliage.dir.w);
    pbr_input.specular_occlusion = max(pbr_input.specular_occlusion, 0.8);
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
