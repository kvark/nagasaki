use nagasaki::{parse_str, to_wgsl, validate};

#[test]
fn add_lowers_and_validates() {
    let src = r#"
        fn add(a: f32, b: f32) -> f32 {
            a + b
        }
    "#;
    let module = parse_str(src).unwrap();
    assert_eq!(module.functions.len(), 1);
    let info = validate(&module).unwrap();
    let wgsl = to_wgsl(&module, &info).unwrap();
    assert!(wgsl.contains("fn add(a: f32, b: f32) -> f32"), "{wgsl}");
    assert!(wgsl.contains("a + b") || wgsl.contains("(a + b)"), "{wgsl}");
}

#[test]
fn explicit_return() {
    let module = parse_str("fn add(a: f32, b: f32) -> f32 { return a + b; }").unwrap();
    validate(&module).unwrap();
}

#[test]
fn arithmetic_and_compare() {
    let src = r#"
        fn mad(a: f32, b: f32, c: f32) -> f32 { a * b + c }
        fn lt(a: i32, b: i32) -> bool { a < b }
        fn bits(x: u32, y: u32) -> u32 { x & y | y }
    "#;
    let module = parse_str(src).unwrap();
    assert_eq!(module.functions.len(), 3);
    validate(&module).unwrap();
}

#[test]
fn unary_and_bool() {
    let src = r#"
        fn neg(x: f32) -> f32 { -x }
        fn not_b(x: bool) -> bool { !x }
    "#;
    let module = parse_str(src).unwrap();
    validate(&module).unwrap();
}

#[test]
fn rejects_let() {
    let err = parse_str("fn f(a: f32) -> f32 { let x = a; x }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("let"), "{msg}");
}
