use nagasaki::{parse_str, to_wgsl, validate};

fn roundtrip(src: &str) -> String {
    let module = parse_str(src).expect("parse");
    let info = validate(&module).expect("validate");
    to_wgsl(&module, &info).expect("wgsl")
}

fn validate_only(src: &str) {
    let module = parse_str(src).expect(src);
    validate(&module).expect(src);
}

#[test]
fn user_fn_call() {
    let wgsl = roundtrip(
        r#"
        fn add(a: f32, b: f32) -> f32 { a + b }
        fn mad(a: f32, b: f32, c: f32) -> f32 { add(a, b) + c }
        "#,
    );
    assert!(wgsl.contains("fn add"), "{wgsl}");
    assert!(wgsl.contains("fn mad"), "{wgsl}");
}

#[test]
fn call_in_entry() {
    validate_only(
        r#"
        fn scale(v: vec3, s: f32) -> vec3 { v * s }
        #[vertex]
        #[output(builtin(position))]
        fn vs_main(#[location(0)] pos: vec3) -> vec4 {
            let p = scale(pos, 2.0);
            vec4(p.x, p.y, p.z, 1.0)
        }
        "#,
    );
}

#[test]
fn dot_normalize() {
    validate_only(
        r#"
        fn f(a: vec3, b: vec3) -> f32 {
            dot(normalize(a), b)
        }
        "#,
    );
}

#[test]
fn cross_length() {
    validate_only("fn f(a: vec3, b: vec3) -> f32 { length(cross(a, b)) }");
}

#[test]
fn clamp_mix() {
    validate_only("fn f(x: f32, a: f32, b: f32) -> f32 { mix(a, b, clamp(x, 0.0, 1.0)) }");
}

#[test]
fn rejects_unknown_fn() {
    let err = parse_str("fn f(a: f32) -> f32 { foo(a) }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("foo") || msg.contains("unknown") || msg.contains("constructor"), "{msg}");
}

#[test]
fn rejects_forward_ref() {
    let err = parse_str(
        r#"
        fn a(x: f32) -> f32 { b(x) }
        fn b(x: f32) -> f32 { x }
        "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("b") || msg.contains("unknown"), "{msg}");
}
