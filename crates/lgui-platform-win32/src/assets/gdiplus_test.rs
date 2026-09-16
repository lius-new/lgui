use super::GdiPlusRuntime;

#[test]
fn startup_produces_a_live_gdiplus_token() {
    let runtime = GdiPlusRuntime::start().expect("GDI+ should start on Windows");
    assert_ne!(runtime.token, 0);
}
