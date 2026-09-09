use nagasaki::{parse_str, to_wgsl, validate};

fn roundtrip(src: &str) -> String {
    let module = parse_str(src).expect("parse");
    let info = validate(&module).expect("validate");
    to_wgsl(&module, &info).expect("wgsl")
}

#[test]
fn assign_local() {
    let wgsl = roundtrip("fn f(a: f32, b: f32) -> f32 { let x = a; x = b; x }");
    assert!(wgsl.contains('='), "{wgsl}");
}

#[test]
fn assign_uses_new_value() {
    validate_only("fn f(a: f32, b: f32) -> f32 { let x = a; x = x + b; x }");
}

#[test]
fn compound_add_assign() {
    let wgsl = roundtrip("fn f(a: f32, b: f32) -> f32 { let x = a; x += b; x }");
    assert!(wgsl.contains('+') || wgsl.contains("add"), "{wgsl}");
}

#[test]
fn assign_as_expr_tail() {
    validate_only("fn f(a: f32, b: f32) -> f32 { let x = a; x = b }");
}

#[test]
fn assign_inside_if() {
    validate_only(
        r#"
        fn f(c: bool, a: f32, b: f32) -> f32 {
            let x = a;
            if c {
                x = b;
            }
            x
        }
        "#,
    );
}

#[test]
fn rejects_assign_to_argument() {
    let err = parse_str("fn f(a: f32, b: f32) -> f32 { a = b; a }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("argument") || msg.contains("assign"), "{msg}");
}

#[test]
fn rejects_assign_to_literal() {
    let err = parse_str("fn f(a: f32) -> f32 { 1 = a; a }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("assign") || msg.contains("target"), "{msg}");
}

fn validate_only(src: &str) {
    let module = parse_str(src).expect(src);
    validate(&module).expect(src);
}
