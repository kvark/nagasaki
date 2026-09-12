//! Real Blade shaders, rewritten in the nagasaki dialect.
//!
//! Each of these is a port of a file in <https://github.com/kvark/blade>, kept
//! line-for-line with the WGSL where the dialect allows it. They are the
//! measure of whether the dialect can actually carry Blade's shaders, rather
//! than a collection of features that each work on their own.
//!
//! Blade leaves `@group`/`@binding` and vertex attribute locations out of its
//! shaders and fills them in at pipeline creation, so these validate the way
//! Blade validates: `ValidationFlags::all() ^ BINDINGS`.

mod common;

use common::*;

/// Check a port against the WGSL it came from: Naga parses the original, and
/// the two modules must describe the same interface.
fn assert_ports(original: &str, port: &str) {
    let original = naga::front::wgsl::parse_str(original).expect("naga parses the original WGSL");
    let ported = nagasaki::parse_str(port).expect("nagasaki parses the port");
    assert_eq!(interface(&original), interface(&ported));
}

/// `blade-render/code/color.inc.wgsl`
#[test]
fn color_inc() {
    let wgsl = roundtrip_unbound(COLOR_INC);
    assert!(wgsl.contains("fn encode_srgb"), "{wgsl}");
    assert!(wgsl.contains("fn encode_surface_color"), "{wgsl}");
    assert_ports(
        r#"
fn encode_srgb(linear: vec3<f32>) -> vec3<f32> {
    let low = 12.92 * linear;
    let high = 1.055 * pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(high, low, linear <= vec3<f32>(0.0031308));
}

fn encode_surface_color(color: vec3<f32>, needs_encoding: bool) -> vec3<f32> {
    return select(color, encode_srgb(color), needs_encoding);
}
"#,
        COLOR_INC,
    );
}

const COLOR_INC: &str = r#"
        fn encode_srgb(linear: vec3) -> vec3 {
            let low = 12.92 * linear;
            let high = 1.055 * pow(max(linear, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
            select(high, low, linear <= vec3(0.0031308))
        }

        fn encode_surface_color(color: vec3, needs_encoding: bool) -> vec3 {
            select(color, encode_srgb(color), needs_encoding)
        }
        "#;

/// `blade-render/code/quaternion.inc.wgsl`
#[test]
fn quaternion_inc() {
    let wgsl = roundtrip_unbound(QUATERNION_INC);
    assert!(wgsl.contains("fn make_quat"), "{wgsl}");
    assert!(wgsl.contains("fn shortest_arc_quat"), "{wgsl}");
    assert_ports(
        r#"
fn qrot(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    return v + 2.0*cross(q.xyz, cross(q.xyz,v) + q.w*v);
}
fn qinv(q: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(-q.xyz,q.w);
}
fn make_quat(m: mat3x3<f32>) -> vec4<f32> {
    var q: vec4<f32>;
    if (m[2].z < 0.0) {
        q = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    } else {
        q = vec4<f32>(0.0, 1.0, 0.0, 0.0);
    }
    return normalize(q);
}
fn shortest_arc_quat(a: vec3<f32>, b: vec3<f32>) -> vec4<f32> {
    return normalize(vec4<f32>(cross(a, b), 1.0 + dot(a, b)));
}
"#,
        QUATERNION_INC,
    );
}

const QUATERNION_INC: &str = r#"
        fn qrot(q: vec4, v: vec3) -> vec3 {
            v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v)
        }

        fn qinv(q: vec4) -> vec4 {
            vec4(-q.xyz, q.w)
        }

        // Based on "quaternionRotationMatrix" in:
        // https://github.com/microsoft/DirectXMath
        fn make_quat(m: mat3) -> vec4 {
            let q: vec4;
            if m[2].z < 0.0 {
                // x^2 + y^2 >= z^2 + w^2
                let dif10 = m[1].y - m[0].x;
                let omm22 = 1.0 - m[2].z;
                if dif10 < 0.0 {
                    // x^2 >= y^2
                    q = vec4(omm22 - dif10, m[0].y + m[1].x, m[0].z + m[2].x, m[1].z - m[2].y);
                } else {
                    // y^2 >= x^2
                    q = vec4(m[0].y + m[1].x, omm22 + dif10, m[1].z + m[2].y, m[2].x - m[0].z);
                }
            } else {
                // z^2 + w^2 >= x^2 + y^2
                let sum10 = m[1].y + m[0].x;
                let opm22 = 1.0 + m[2].z;
                if sum10 < 0.0 {
                    // z^2 >= w^2
                    q = vec4(m[0].z + m[2].x, m[1].z + m[2].y, opm22 - sum10, m[0].y - m[1].x);
                } else {
                    // w^2 >= z^2
                    q = vec4(m[1].z - m[2].y, m[2].x - m[0].z, m[0].y - m[1].x, opm22 + sum10);
                }
            }
            normalize(q)
        }

        // Find a quaternion that turns vector 'a' into vector 'b' in a shortest arc.
        fn shortest_arc_quat(a: vec3, b: vec3) -> vec4 {
            if dot(a, b) < -0.99999 {
                // Choose the axis of rotation that doesn't align with the vectors
                select(
                    vec4(1.0, 0.0, 0.0, 0.0),
                    vec4(0.0, 1.0, 0.0, 0.0),
                    abs(a.x) > abs(a.y),
                )
            } else {
                normalize(vec4(cross(a, b), 1.0 + dot(a, b)))
            }
        }
        "#;

/// `examples/bunnymark/shader.wgsl`, whole.
#[test]
fn bunnymark() {
    let wgsl = roundtrip_unbound(
        r#"
        struct Globals {
            mvp_transform: mat4,
            sprite_size: vec2,
        }
        static globals: Globals = ();

        struct Locals {
            position: vec2,
            velocity: vec2,
            color: u32,
        }
        static locals: Locals = ();

        struct Vertex {
            pos: vec2,
        }

        static sprite_texture: texture_2d<f32> = ();
        static sprite_sampler: sampler = ();

        struct VertexOutput {
            #[builtin(position)] position: vec4,
            #[location(0)] tex_coords: vec2,
            #[location(1)] color: vec4,
        }

        fn unpack_color(raw: u32) -> vec4 {
            ((vec4u(raw) >> vec4u(0, 8, 16, 24)) & vec4u(0xFF)) as vec4<f32> / 255.0
        }

        #[vertex]
        fn vs_main(vertex: Vertex) -> VertexOutput {
            let tc = vertex.pos;
            let offset = tc * globals.sprite_size;
            let pos = globals.mvp_transform * vec4(locals.position + offset, 0.0, 1.0);
            let color = unpack_color(locals.color);
            VertexOutput { position: pos, tex_coords: tc, color }
        }

        #[fragment]
        #[output(location(0))]
        fn fs_main(vertex: VertexOutput) -> vec4 {
            vertex.color
                * textureSampleLevel(sprite_texture, sprite_sampler, vertex.tex_coords, 0.0)
        }
        "#,
    );
    assert!(wgsl.contains("@vertex"), "{wgsl}");
    assert!(wgsl.contains("@fragment"), "{wgsl}");
    assert!(
        wgsl.contains("var sprite_texture: texture_2d<f32>"),
        "{wgsl}"
    );
    // Blade fills the vertex attribute locations in by field name, so the
    // input struct arrives with none.
    assert!(wgsl.contains("struct Vertex"), "{wgsl}");
    assert!(wgsl.contains("textureSampleLevel("), "{wgsl}");
}

/// `blade-render/code/debug-blit.wgsl`
#[test]
fn debug_blit() {
    let wgsl = roundtrip_unbound(
        r#"
        struct BlitParams {
            target_offset: vec2,
            target_size: vec2,
            mip_level: f32,
            unused: u32,
        }
        static params: BlitParams = ();
        static input: texture_2d<f32> = ();
        static samp: sampler = ();

        struct VertexOutput {
            #[builtin(position)] clip_pos: vec4,
            #[location(0)] uv: vec2,
        }

        #[vertex]
        fn blit_vs(#[builtin(vertex_index)] vi: u32) -> VertexOutput {
            let uv = vec2((vi << 1) & 2, vi & 2) as vec2<f32>;
            let clip = params.target_offset + uv * params.target_size;
            VertexOutput { clip_pos: vec4(clip, 0.0, 1.0), uv }
        }

        #[fragment]
        #[output(location(0))]
        fn blit_fs(vo: VertexOutput) -> vec4 {
            textureSampleLevel(input, samp, vo.uv, params.mip_level)
        }
        "#,
    );
    assert!(wgsl.contains("fn blit_vs"), "{wgsl}");
    assert!(wgsl.contains("fn blit_fs"), "{wgsl}");
}

/// A compute pass in the shape of `blade-particle/src/particle.wgsl`: a
/// runtime-sized storage buffer of structs, updated in place.
#[test]
fn particle_update() {
    let wgsl = roundtrip_unbound(
        r#"
        const MAX_LIFETIME: f32 = 10.0;

        struct Particle {
            position: vec3,
            life: f32,
            velocity: vec3,
            color: vec4,
        }

        struct UpdateParams {
            time_delta: f32,
            gravity: vec3,
        }

        static update_params: UpdateParams = ();
        #[storage(read_write)] static particles: [Particle] = ();
        #[storage(read_write)] static free_list: [u32] = ();

        fn is_alive(p: Particle) -> bool {
            p.life > 0.0
        }

        #[compute]
        #[workgroup_size(64)]
        fn update(#[builtin(global_invocation_id)] gid: vec3<u32>) {
            let index = gid.x;
            let dt = update_params.time_delta;

            if !is_alive(particles[index]) {
                free_list[index] = index;
                return;
            }

            particles[index].life -= dt;
            particles[index].velocity += update_params.gravity * dt;
            particles[index].position += particles[index].velocity * dt;

            let fade = clamp(particles[index].life / MAX_LIFETIME, 0.0, 1.0);
            particles[index].color.w = fade;
        }
        "#,
    );
    assert!(wgsl.contains("array<Particle>"), "{wgsl}");
    assert!(wgsl.contains("].life = "), "{wgsl}");
    assert!(wgsl.contains("].color.w = "), "{wgsl}");
    assert!(wgsl.contains("const MAX_LIFETIME"), "{wgsl}");
}

/// A post-process pass in the shape of `blade-render/code/post-proc.wgsl`:
/// storage-texture read, tone map, storage-texture write.
#[test]
fn post_process() {
    let wgsl = roundtrip_unbound(
        r#"
        const LUMA: vec3 = vec3(0.2126, 0.7152, 0.0722);

        struct PostProcParams {
            exposure: f32,
            needs_srgb: u32,
        }
        static params: PostProcParams = ();
        static input: texture_2d<f32> = ();
        static output: texture_storage_2d<Rgba8Unorm, Write> = ();

        fn encode_srgb(linear: vec3) -> vec3 {
            let low = 12.92 * linear;
            let high = 1.055 * pow(max(linear, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
            select(high, low, linear <= vec3(0.0031308))
        }

        fn tone_map(color: vec3) -> vec3 {
            color / (dot(color, LUMA) + 1.0)
        }

        #[compute]
        #[workgroup_size(8, 8)]
        fn post_proc(#[builtin(global_invocation_id)] gid: vec3<u32>) {
            let size = textureDimensions(input);
            if gid.x >= size.x || gid.y >= size.y {
                return;
            }
            let coord = gid.xy as vec2<i32>;
            let raw = textureLoad(input, coord, 0);
            let mapped = tone_map(raw.xyz * params.exposure);
            let encoded = select(mapped, encode_srgb(mapped), params.needs_srgb != 0u32);
            textureStore(output, coord, vec4(encoded, raw.w));
        }
        "#,
    );
    assert!(wgsl.contains("textureStore(output,"), "{wgsl}");
    assert!(wgsl.contains("textureDimensions(input)"), "{wgsl}");
}

/// The bunnymark port checked against the shader it came from.
///
/// Blade's original WGSL, parsed by Naga's own frontend, must describe the same
/// interface as the Rust above: same globals in the same address spaces, same
/// entry points, same struct layouts and bindings. A port that merely compiles
/// proves nothing on its own.
#[test]
fn bunnymark_matches_the_original_wgsl() {
    // examples/bunnymark/shader.wgsl, verbatim.
    const ORIGINAL: &str = r#"
struct Globals {
    mvp_transform: mat4x4<f32>,
    sprite_size: vec2<f32>,
};
var<uniform> globals: Globals;

struct Locals {
    position: vec2<f32>,
    velocity: vec2<f32>,
    color: u32,
};
var<uniform> locals: Locals;

struct Vertex {
    pos: vec2<f32>,
};

var sprite_texture: texture_2d<f32>;
var sprite_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f32>,
}

fn unpack_color(raw: u32) -> vec4<f32> {
    return vec4<f32>((vec4<u32>(raw) >> vec4<u32>(0u, 8u, 16u, 24u)) & vec4<u32>(0xFFu)) / 255.0;
}

@vertex
fn vs_main(vertex: Vertex) -> VertexOutput {
    let tc = vertex.pos;
    let offset = tc * globals.sprite_size;
    let pos = globals.mvp_transform * vec4<f32>(locals.position + offset, 0.0, 1.0);
    let color = unpack_color(locals.color);
    return VertexOutput(pos, tc, color);
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vertex.color * textureSampleLevel(sprite_texture, sprite_sampler, vertex.tex_coords, 0.0);
}
"#;

    const PORT: &str = r#"
        struct Globals {
            mvp_transform: mat4,
            sprite_size: vec2,
        }
        static globals: Globals = ();

        struct Locals {
            position: vec2,
            velocity: vec2,
            color: u32,
        }
        static locals: Locals = ();

        struct Vertex {
            pos: vec2,
        }

        static sprite_texture: texture_2d<f32> = ();
        static sprite_sampler: sampler = ();

        struct VertexOutput {
            #[builtin(position)] position: vec4,
            #[location(0)] tex_coords: vec2,
            #[location(1)] color: vec4,
        }

        fn unpack_color(raw: u32) -> vec4 {
            ((vec4u(raw) >> vec4u(0, 8, 16, 24)) & vec4u(0xFF)) as vec4<f32> / 255.0
        }

        #[vertex]
        fn vs_main(vertex: Vertex) -> VertexOutput {
            let tc = vertex.pos;
            let offset = tc * globals.sprite_size;
            let pos = globals.mvp_transform * vec4(locals.position + offset, 0.0, 1.0);
            let color = unpack_color(locals.color);
            VertexOutput { position: pos, tex_coords: tc, color }
        }

        #[fragment]
        #[output(location(0))]
        fn fs_main(vertex: VertexOutput) -> vec4 {
            vertex.color
                * textureSampleLevel(sprite_texture, sprite_sampler, vertex.tex_coords, 0.0)
        }
    "#;

    assert_ports(ORIGINAL, PORT);
}
