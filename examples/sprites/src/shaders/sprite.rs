//! Instanced sprite drawing.

struct Locals {
    position: vec2,
    velocity: vec2,
    color: u32,
}

static locals: Locals = ();
static sprite_texture: texture_2d<f32> = ();
static sprite_sampler: sampler = ();

struct Vertex {
    pos: vec2,
}

struct VertexOutput {
    #[builtin(position)] position: vec4,
    #[location(0)] tex_coords: vec2,
    #[location(1)] color: vec4,
}

#[vertex]
fn vs_main(vertex: Vertex) -> VertexOutput {
    let tc = vertex.pos;
    let offset = tc * globals.sprite_size;
    VertexOutput {
        position: globals.mvp_transform * vec4(locals.position + offset, 0.0, 1.0),
        tex_coords: tc,
        color: unpack_color(locals.color),
    }
}

#[fragment]
#[output(location(0))]
fn fs_main(vertex: VertexOutput) -> vec4 {
    vertex.color * textureSampleLevel(sprite_texture, sprite_sampler, vertex.tex_coords, 0.0)
}
