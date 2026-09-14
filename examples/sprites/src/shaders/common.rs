//! Shared by every shader module here, in place of a WGSL `#include`.

use synaga_shader::*;

pub struct Globals {
    pub mvp_transform: mat4,
    pub sprite_size: vec2,
}

pub static globals: Uniform<Globals> = binding();

pub fn unpack_color(raw: u32) -> vec4 {
    let bytes = (vec4u::splat(raw) >> vec4u(0, 8, 16, 24)) & vec4u::splat(0xFF);
    vec4::from(bytes) / 255.0
}
