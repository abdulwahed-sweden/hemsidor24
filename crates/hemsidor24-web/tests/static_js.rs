#[test]
fn order_form_script_does_not_intercept_native_submit() {
    let js = include_str!("../static/js/main.js");

    assert!(
        !js.contains("preventDefault"),
        "production order-form JavaScript must not block the native POST to /bestall"
    );
    assert!(
        !js.contains("addEventListener('submit'"),
        "production order-form JavaScript must not replace the server submission path"
    );
    assert!(
        js.contains("[data-pick]"),
        "package preselection JavaScript should remain intact"
    );
}
