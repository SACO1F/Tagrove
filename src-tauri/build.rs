fn main() {
    // Native resource compilation must follow logo asset changes as well as config.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
