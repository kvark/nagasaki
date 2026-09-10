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
fn while_counts() {
    let wgsl = roundtrip(
        r#"
        fn f(n: i32) -> i32 {
            let i = 0;
            while i < n {
                i += 1;
            }
            i
        }
        "#,
    );
    assert!(wgsl.contains("loop") || wgsl.contains("while"), "{wgsl}");
}

#[test]
fn loop_with_break() {
    validate_only(
        r#"
        fn f(n: i32) -> i32 {
            let x = 0;
            loop {
                if x >= n {
                    break;
                }
                x += 1;
            }
            x
        }
        "#,
    );
}

#[test]
fn while_continue() {
    validate_only(
        r#"
        fn f(n: i32) -> i32 {
            let i = 0;
            let s = 0;
            while i < n {
                i += 1;
                if i < 0 {
                    continue;
                }
                s += i;
            }
            s
        }
        "#,
    );
}

#[test]
fn return_inside_loop() {
    validate_only(
        r#"
        fn f(n: i32) -> i32 {
            let i = 0;
            loop {
                if i >= n {
                    return i;
                }
                i += 1;
            }
        }
        "#,
    );
}

#[test]
fn rejects_labeled_loop() {
    let err = parse_str("fn f(a: i32) -> i32 { 'x: loop { break; } a }").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("label") || msg.contains("unsupported"),
        "{msg}"
    );
}

#[test]
fn rejects_break_value() {
    let err = parse_str("fn f(a: i32) -> i32 { loop { break a; } }").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("break") || msg.contains("value"), "{msg}");
}
