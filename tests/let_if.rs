use nagasaki::{parse_str, to_wgsl, validate};

fn roundtrip(src: &str) -> String {
    let module = parse_str(src).expect("parse");
    let info = validate(&module).expect("validate");
    to_wgsl(&module, &info).expect("wgsl")
}

#[test]
fn let_inferred() {
    let wgsl = roundtrip("fn add(a: f32, b: f32) -> f32 { let x = a + b; x }");
    assert!(wgsl.contains("fn add"), "{wgsl}");
    assert!(wgsl.contains("var") || wgsl.contains("let"), "{wgsl}");
}

#[test]
fn let_annotated() {
    let wgsl = roundtrip("fn id(a: f32) -> f32 { let x: f32 = a; x }");
    assert!(wgsl.contains("fn id"), "{wgsl}");
    validate_only("fn id(a: f32) -> f32 { let x: f32 = a; x }");
}

#[test]
fn let_shadows() {
    validate_only("fn f(a: f32, b: f32) -> f32 { let x = a; let x = x + b; x }");
}

#[test]
fn let_shadows_arg() {
    validate_only("fn f(a: f32) -> f32 { let a = a + a; a }");
}

#[test]
fn if_expr_tail() {
    let wgsl = roundtrip("fn pick(c: bool, a: f32, b: f32) -> f32 { if c { a } else { b }");
    assert!(wgsl.contains("if"), "{wgsl}");
}

#[test]
fn if_expr_in_let() {
    validate_only("fn pick(c: bool, a: f32, b: f32) -> f32 { let x = if c { a } else { b }; x }");
}

#[test]
fn if_stmt_then_return() {
    validate_only(
        r#"
        fn pick(c: bool, a: f32, b: f32) -> f32 {
            if c {
                return a;
            }
            b
        }
    "#,
    );
}

#[test]
fn else_if_expr() {
    validate_only(
        r#"
        fn pick(c: bool, d: bool, a: f32, b: f32, e: f32) -> f32 {
            if c { a } else if d { b } else { e }
        }
    "#,
    );
}

#[test]
fn nested_block_tail() {
    validate_only("fn f(a: f32) -> f32 { { let x = a; x } }");
}

#[test]
fn rejects_bare_let() {
    let err = parse_str("fn f(a: f32) -> f32 { let x; x }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("let") || msg.contains("initializer"), "{msg}");
}

#[test]
fn rejects_if_expr_without_else() {
    let err = parse_str("fn f(c: bool, a: f32) -> f32 { if c { a } }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("else"), "{msg}");
}

fn validate_only(src: &str) {
    let module = parse_str(src).expect(src);
    validate(&module).expect(src);
}
