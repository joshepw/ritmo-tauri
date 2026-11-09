fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out.join("google.client.json");
    println!("cargo:rerun-if-changed=google.client.json");
    let source = std::path::Path::new("google.client.json");
    if source.is_file() {
        std::fs::copy(source, &dest).expect("no se pudo incluir el cliente de Google");
    } else {
        std::fs::write(&dest, "{}\n").expect("no se pudo preparar el cliente de Google");
    }
    tauri_build::build();
}
