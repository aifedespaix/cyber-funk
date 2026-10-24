// Passage de profondeur des façades (occlusion GPU de la caméra du monde).
//
// Même déplacement que le vertex shader de `facade.wesl` (les tours « poussent » du sol entre 300 et 440 m) : sans lui, la
// profondeur écrite ne serait pas celle du dessin, et les tours encore à demi enfoncées s'occulteraient à tort.

import bevy_pbr::render::mesh_view_bindings::view;
import bevy_pbr::render::mesh_functions::get_world_from_local;
import bevy_pbr::render::view_transformations::position_world_to_clip;
import bevy_pbr::prepass::io::{Vertex, VertexOutput, decompress_vertex};

@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let m = get_world_from_local(vertex_in.instance_index);
    var wp = m * vec4<f32>(vertex.position, 1.0);
    wp.y *= 1.0 - smoothstep(300.0, 440.0, distance(wp.xz, view.world_position.xz));
    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_NORMALS)
    out.world_normal = normalize((m * vec4<f32>(vertex.normal, 0.0)).xyz);
@if(MOTION_VECTOR_PREPASS)
    out.previous_world_position = wp;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
    return out;
}
