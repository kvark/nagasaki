//! Shared by every shader module here, in place of a WGSL `#include`.

struct Globals {
    mvp_transform: mat4,
    sprite_size: vec2,
}

static globals: Globals = ();

fn unpack_color(raw: u32) -> vec4 {
    ((vec4u(raw) >> vec4u(0, 8, 16, 24)) & vec4u(0xFF)) as vec4<f32> / 255.0
}
