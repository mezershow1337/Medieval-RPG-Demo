// Подводная жизнь: водоросли колышутся в толще, рыбы изгибаются на плаву.
// Сдвиг считается одинаково в основном проходе и в предпроходе (глубина, векторы движения
// для TAA), иначе водоросли мерцали бы.
//
// Водоросли: второй UV-канал — x: гибкость (0 у корня, 1 на кончике), y: фаза.
// Рыбы: x — доля длины от носа (0) к хвосту (1), y — фаза.

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
import bevy_pbr::render::forward_io::{Vertex, VertexOutput, decompress_vertex};

struct SeaLife {
    // x — 0: водоросли, 1: рыба; y — сила (качание или взмахи хвоста); z — частота; w — запас.
    mode: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> life: SeaLife;

@if(PREPASS_PIPELINE)
@group(0) @binding(1) var<uniform> prepass_globals: Globals;

fn frame_time() -> vec2<f32> {
@if(PREPASS_PIPELINE) {
    return vec2(prepass_globals.time, prepass_globals.delta_time);
} @else {
    return vec2(globals.time, globals.delta_time);
}
}

// Сдвиг в локальных координатах меша.
fn sway(local: vec3<f32>, base: vec3<f32>, flex: f32, phase: f32, t: f32) -> vec3<f32> {
    if life.mode.x < 0.5 {
        // Водоросль: медленная зыбь у дна, кончик описывает петлю.
        let w = t * life.mode.z + phase * 6.2831 + dot(base.xz, vec2(0.21, 0.13));
        let bend = flex * flex * life.mode.y;
        return vec3(sin(w) * bend, -bend * 0.25 * (1.0 + sin(w * 0.5)), cos(w * 0.8 + 1.3) * bend * 0.6);
    }
    // Рыба: волна вдоль тела, у хвоста сильнее (локальный -Z — вперёд).
    let w = t * life.mode.z + phase * 6.2831 - flex * 4.5;
    let amp = life.mode.y * (0.15 + flex * flex);
    return vec3(sin(w) * amp, 0.0, 0.0);
}

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);
    let base = world_from_local[3].xyz;
    var flex = 0.0;
    var phase = 0.0;
@if(VERTEX_UVS_B) {
    flex = vertex.uv_b.x;
    phase = vertex.uv_b.y;
}
    let t = frame_time().x;
    let local = vertex.position + sway(vertex.position, base, flex, phase, t);
    let world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(local, 1.0));
    out.world_position = world;
    out.position = position_world_to_clip(world.xyz);

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
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
@if(MOTION_VECTOR_PREPASS) {
    let prev_model = mesh_functions::get_previous_world_from_local(vertex_in.instance_index);
    let prev_local = vertex.position + sway(vertex.position, prev_model[3].xyz, flex, phase, t - frame_time().y);
    out.previous_world_position = mesh_functions::mesh_position_local_to_world(prev_model, vec4<f32>(prev_local, 1.0));
}
} @else {
@if(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
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
