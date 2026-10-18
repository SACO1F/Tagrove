#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if std::env::args()
        .nth(1)
        .is_some_and(|s| s.starts_with("chrome-extension://"))
    {
        shiqian_lib::capture_bridge::native_main();
        return;
    }
    shiqian_lib::run();
}
