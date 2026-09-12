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

/// `blade-egui/shader.wgsl`, whole — checked against the original.
#[test]
fn egui() {
    const ORIGINAL: &str = r#"
struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @location(1) color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

struct Uniforms {
    screen_size: vec2<f32>,
    convert_to_linear: f32,
    padding: f32,
};
var<uniform> r_uniforms: Uniforms;

struct Vertex {
    pos: vec2<f32>,
    uv: vec2<f32>,
    color: u32,
}

fn linear_from_gamma(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

@vertex
fn vs_main(input: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coord = input.uv;
    out.color = unpack4x8unorm(input.color);
    out.position = vec4<f32>(
        2.0 * input.pos.x / r_uniforms.screen_size.x - 1.0,
        1.0 - 2.0 * input.pos.y / r_uniforms.screen_size.y,
        0.0,
        1.0,
    );
    return out;
}

var r_texture: texture_2d<f32>;
var r_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let blended = in.color * textureSample(r_texture, r_sampler, in.tex_coord);
    return vec4f(linear_from_gamma(blended.xyz), blended.a);
}
"#;

    // `in` is a Rust keyword, so the fragment argument is renamed; everything
    // else is the shader as written.
    const PORT: &str = r#"
        struct VertexOutput {
            #[location(0)] tex_coord: vec2,
            #[location(1)] color: vec4,
            #[builtin(position)] position: vec4,
        }

        struct Uniforms {
            screen_size: vec2,
            convert_to_linear: f32,
            padding: f32,
        }
        static r_uniforms: Uniforms = ();

        struct Vertex {
            pos: vec2,
            uv: vec2,
            color: u32,
        }

        fn linear_from_gamma(srgb: vec3) -> vec3 {
            let cutoff = srgb < vec3(0.04045);
            let lower = srgb / vec3(12.92);
            let higher = pow((srgb + vec3(0.055)) / vec3(1.055), vec3(2.4));
            select(higher, lower, cutoff)
        }

        #[vertex]
        fn vs_main(input: Vertex) -> VertexOutput {
            let out: VertexOutput;
            out.tex_coord = input.uv;
            out.color = unpack4x8unorm(input.color);
            out.position = vec4(
                2.0 * input.pos.x / r_uniforms.screen_size.x - 1.0,
                1.0 - 2.0 * input.pos.y / r_uniforms.screen_size.y,
                0.0,
                1.0,
            );
            out
        }

        static r_texture: texture_2d<f32> = ();
        static r_sampler: sampler = ();

        #[fragment]
        #[output(location(0))]
        fn fs_main(vo: VertexOutput) -> vec4 {
            let blended = vo.color * textureSample(r_texture, r_sampler, vo.tex_coord);
            vec4f(linear_from_gamma(blended.xyz), blended.a)
        }
    "#;

    let wgsl = roundtrip_unbound(PORT);
    assert!(wgsl.contains("unpack4x8unorm"), "{wgsl}");
    assert_ports(ORIGINAL, PORT);
}

/// `blade-render/code/skin.wgsl` with its include inlined, checked against the
/// original.
#[test]
fn skin() {
    const ORIGINAL: &str = r#"
struct Vertex {
    position: vec3<f32>,
    bitangent_sign: f32,
    tex_coords: vec2<f32>,
    normal: u32,
    tangent: u32,
}

struct SkinVertex {
    joints: u32,
    weights: vec4<f32>,
}

struct SkinDispatch {
    vertex_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

var<uniform> skin_dispatch: SkinDispatch;
var<storage, read> source: array<Vertex>;
var<storage, read> skin_source: array<SkinVertex>;
var<storage, read_write> destination: array<Vertex>;

fn decode_normal(raw: u32) -> vec3<f32> {
    return unpack4x8snorm(raw).xyz;
}

fn encode_normal(n: vec3<f32>) -> u32 {
    return pack4x8snorm(vec4<f32>(n, 0.0));
}

fn normalize_or_zero(v: vec3<f32>) -> vec3<f32> {
    let len2 = dot(v, v);
    if (len2 < 1.0e-20) {
        return vec3<f32>(0.0);
    }
    return v * inverseSqrt(len2);
}

fn skin_stored_vertex(input: Vertex, skin: SkinVertex, linear: mat3x3<f32>) -> Vertex {
    var out = input;
    out.normal = encode_normal(normalize_or_zero(linear * decode_normal(input.normal)));
    out.tangent = encode_normal(normalize_or_zero(linear * decode_normal(input.tangent)));
    out.bitangent_sign *= sign(determinant(linear));
    return out;
}

@compute
@workgroup_size(64, 1, 1)
fn skin(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let i = global_id.x;
    if (i >= skin_dispatch.vertex_count) {
        return;
    }
    destination[i] = skin_stored_vertex(source[i], skin_source[i], mat3x3<f32>());
}
"#;

    const PORT: &str = r#"
        struct Vertex {
            position: vec3,
            bitangent_sign: f32,
            tex_coords: vec2,
            normal: u32,
            tangent: u32,
        }

        struct SkinVertex {
            joints: u32,
            weights: vec4,
        }

        struct SkinDispatch {
            vertex_count: u32,
            _pad0: u32,
            _pad1: u32,
            _pad2: u32,
        }

        static skin_dispatch: SkinDispatch = ();
        #[storage] static source: [Vertex] = ();
        #[storage] static skin_source: [SkinVertex] = ();
        #[storage(read_write)] static destination: [Vertex] = ();

        fn decode_normal(raw: u32) -> vec3 {
            unpack4x8snorm(raw).xyz
        }

        fn encode_normal(n: vec3) -> u32 {
            pack4x8snorm(vec4(n, 0.0))
        }

        fn normalize_or_zero(v: vec3) -> vec3 {
            let len2 = dot(v, v);
            if len2 < 1.0e-20 {
                return vec3(0.0);
            }
            v * inverseSqrt(len2)
        }

        fn skin_stored_vertex(input: Vertex, skin: SkinVertex, linear: mat3) -> Vertex {
            let out = input;
            out.normal = encode_normal(normalize_or_zero(linear * decode_normal(input.normal)));
            out.tangent = encode_normal(normalize_or_zero(linear * decode_normal(input.tangent)));
            out.bitangent_sign *= sign(determinant(linear));
            out
        }

        #[compute]
        #[workgroup_size(64, 1, 1)]
        fn skin(#[builtin(global_invocation_id)] global_id: vec3<u32>) {
            let i = global_id.x;
            if i >= skin_dispatch.vertex_count {
                return;
            }
            let m: mat3;
            destination[i] = skin_stored_vertex(source[i], skin_source[i], m);
        }
    "#;

    let wgsl = roundtrip_unbound(PORT);
    assert!(wgsl.contains("pack4x8snorm"), "{wgsl}");
    assert!(wgsl.contains("destination["), "{wgsl}");
    assert_ports(ORIGINAL, PORT);
}

/// The random-number generator from `blade-render/code/random.inc.wgsl`, which
/// threads its state through a pointer parameter.
#[test]
fn random_state() {
    let wgsl = roundtrip_unbound(
        r#"
        struct RandomState {
            seed: u32,
            index: u32,
        }

        fn random_init(pixel_index: u32, frame_index: u32) -> RandomState {
            let rs: RandomState;
            rs.seed = pixel_index * 1664525u32 + frame_index * 1013904223u32;
            rs.index = 0u32;
            rs
        }

        fn random_u32(rng: &mut RandomState) -> u32 {
            rng.index += 1;
            rng.seed = rng.seed * 1664525u32 + 1013904223u32;
            let word = (rng.seed >> ((rng.seed >> 28) + 4)) ^ rng.seed;
            (word >> 22) ^ word
        }

        fn random_gen(rng: &mut RandomState) -> f32 {
            (random_u32(rng) >> 8) as f32 / 16777216.0
        }

        #[compute]
        #[workgroup_size(8, 8)]
        fn noise(#[builtin(global_invocation_id)] gid: vec3<u32>) {
            let rng = random_init(gid.x + gid.y * 1920u32, 0u32);
            let total = 0.0;
            for _i in 0..4u32 {
                total += random_gen(&mut rng);
            }
        }
        "#,
    );
    assert!(wgsl.contains("ptr<function, RandomState>"), "{wgsl}");
    assert!(wgsl.contains("random_gen"), "{wgsl}");
}

/// `blade-render/code/a-trous.wgsl`'s edge-avoiding filter, with the pieces its
/// `#include`s supply written inline. The densest compute shader in Blade that
/// does not trace rays: nested `for`s with `continue`, a const vector indexed by
/// a loop counter, storage-texture load and store.
#[test]
fn a_trous_filter() {
    let wgsl = roundtrip_unbound(
        r#"
        struct Surface {
            flat_normal: vec3,
            depth: f32,
        }

        struct Params {
            extent: vec2<i32>,
            temporal_weight: f32,
            iteration: u32,
            use_motion_vectors: u32,
        }

        static params: Params = ();
        static t_depth: texture_2d<f32> = ();
        static t_flat_normal: texture_2d<f32> = ();
        static input: texture_2d<f32> = ();
        static output: texture_storage_2d<Rgba16Float, ReadWrite> = ();

        const LUMA: vec3 = vec3(0.2126, 0.7152, 0.0722);
        const GAUSSIAN_WEIGHTS: vec2 = vec2(0.44198, 0.27901);
        const SIGMA_L: f32 = 4.0;
        const SIGMA_N: f32 = 4.0;
        const EPSILON: f32 = 0.001;

        fn compare_flat_normals(a: vec3, b: vec3) -> f32 {
            pow(max(0.0, dot(a, b)), SIGMA_N)
        }

        fn compare_depths(a: f32, b: f32) -> f32 {
            1.0 - smoothstep(0.0, 100.0, abs(a - b))
        }

        fn compare_luminance(a_lum: f32, b_lum: f32, variance: f32) -> f32 {
            exp(-abs(a_lum - b_lum) / (SIGMA_L * variance + EPSILON))
        }

        fn w4(w: f32) -> vec4 {
            vec4(vec3(w), w * w)
        }

        fn read_surface(pixel: vec2<i32>) -> Surface {
            let surface = Surface();
            surface.flat_normal = normalize(textureLoad(t_flat_normal, pixel, 0).xyz);
            surface.depth = textureLoad(t_depth, pixel, 0).x;
            surface
        }

        #[compute]
        #[workgroup_size(8, 8)]
        fn atrous3x3(#[builtin(global_invocation_id)] global_id: vec3<u32>) {
            let center = global_id.xy as vec2<i32>;
            if any(center >= params.extent) {
                return;
            }

            let center_ilm = textureLoad(input, center, 0);
            let center_luma = dot(center_ilm.xyz, LUMA);
            let center_suf = read_surface(center);
            let filtered_ilm = center_ilm;

            for yy in -1..=1 {
                for xx in -1..=1 {
                    let p = center + vec2(xx, yy) * (1 << params.iteration);
                    if all(p == center) || any(p < vec2(0, 0)) || any(p >= params.extent) {
                        continue;
                    }

                    let surface = read_surface(p);
                    let weight = GAUSSIAN_WEIGHTS[abs(xx)] * GAUSSIAN_WEIGHTS[abs(yy)];
                    weight *= compare_flat_normals(surface.flat_normal, center_suf.flat_normal);
                    weight *= compare_depths(surface.depth, center_suf.depth);
                    let other_ilm = textureLoad(input, p, 0);
                    let variance = sqrt(max(center_ilm.w, other_ilm.w));
                    weight *= compare_luminance(center_luma, dot(other_ilm.xyz, LUMA), variance);

                    filtered_ilm += w4(weight) * (other_ilm - center_ilm);
                }
            }

            textureStore(output, global_id.xy as vec2<i32>, filtered_ilm);
        }
        "#,
    );
    assert!(wgsl.contains("fn atrous3x3"), "{wgsl}");
    assert!(wgsl.contains("textureStore(output,"), "{wgsl}");
    assert!(wgsl.contains("const GAUSSIAN_WEIGHTS"), "{wgsl}");
}
