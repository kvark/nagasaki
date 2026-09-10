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
fn compute_global_id() {
    let wgsl = roundtrip(
        r#"
        #[compute]
        #[workgroup_size(8, 8, 1)]
        fn cs_main(#[builtin(global_invocation_id)] id: vec3<u32>) {
            let x = id.x;
        }
        "#,
    );
    assert!(wgsl.contains("@compute") || wgsl.contains("compute"), "{wgsl}");
    assert!(wgsl.contains("workgroup_size") || wgsl.contains("8"), "{wgsl}");
}

#[test]
fn vertex_position() {
    let wgsl = roundtrip(
        r#"
        #[vertex]
        #[output(builtin(position))]
        fn vs_main(#[location(0)] pos: vec3) -> vec4 {
            vec4(pos.x, pos.y, pos.z, 1.0)
        }
        "#,
    );
    assert!(wgsl.contains("@vertex") || wgsl.contains("vertex"), "{wgsl}");
    assert!(wgsl.contains("position"), "{wgsl}");
}

#[test]
fn fragment_color() {
    validate_only(
        r#"
        #[fragment]
        #[output(location(0))]
        fn fs_main(#[location(0)] color: vec4) -> vec4 {
            color
        }
        "#,
    );
}

#[test]
fn vertex_with_index() {
    validate_only(
        r#"
        #[vertex]
        #[output(builtin(position))]
        fn vs_main(#[builtin(vertex_index)] vid: u32) -> vec4 {
            vec4(0.0, 0.0, 0.0, 1.0)
        }
        "#,
    );
}

#[test]
fn regular_fn_still_in_functions() {
    let module = parse_str("fn add(a: f32, b: f32) -> f32 { a + b }").unwrap();
    assert_eq!(module.functions.len(), 1);
    assert!(module.entry_points.is_empty());
}

#[test]
fn entry_not_in_functions() {
    let module = parse_str(
        r#"
        #[compute]
        #[workgroup_size(1)]
        fn cs_main(#[builtin(local_invocation_index)] i: u32) {}
        "#,
    )
    .unwrap();
    assert_eq!(module.functions.len(), 0);
    assert_eq!(module.entry_points.len(), 1);
}

#[test]
fn rejects_compute_without_workgroup() {
    let err = parse_str(
        r#"
        #[compute]
        fn cs_main(#[builtin(local_invocation_index)] i: u32) {}
        "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("workgroup"), "{msg}");
}

#[test]
fn rejects_workgroup_on_vertex() {
    let err = parse_str(
        r#"
        #[vertex]
        #[workgroup_size(8)]
        #[output(builtin(position))]
        fn vs_main() -> vec4 { vec4(0.0, 0.0, 0.0, 1.0) }
        "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("workgroup") || msg.contains("compute"), "{msg}");
}

#[test]
fn rejects_missing_arg_binding() {
    let err = parse_str(
        r#"
        #[compute]
        #[workgroup_size(1)]
        fn cs_main(id: vec3<u32>) {}
        "#,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("location") || msg.contains("builtin") || msg.contains("binding"), "{msg}");
}
