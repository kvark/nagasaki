//! Post-process pass: exposure, tone map, sRGB encode.

const LUMA: vec3 = vec3(0.2126, 0.7152, 0.0722);

struct PostParams {
    exposure: f32,
    needs_srgb: u32,
}

static post_params: PostParams = ();
static hdr: texture_2d<f32> = ();
static ldr: texture_storage_2d<Rgba8Unorm, Write> = ();

fn encode_srgb(linear: vec3) -> vec3 {
    let low = 12.92 * linear;
    let high = 1.055 * pow(max(linear, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    select(high, low, linear <= vec3(0.0031308))
}

#[compute]
#[workgroup_size(8, 8)]
fn tonemap(#[builtin(global_invocation_id)] gid: vec3<u32>) {
    let size = textureDimensions(hdr);
    if gid.x >= size.x || gid.y >= size.y {
        return;
    }
    let coord = gid.xy as vec2<i32>;
    let raw = textureLoad(hdr, coord, 0);
    let mapped = raw.xyz * post_params.exposure / (dot(raw.xyz, LUMA) + 1.0);
    let encoded = select(mapped, encode_srgb(mapped), post_params.needs_srgb != 0u32);
    textureStore(ldr, coord, vec4(encoded, raw.w));
}
