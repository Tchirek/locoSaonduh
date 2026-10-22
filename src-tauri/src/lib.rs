pub mod adaptive;
pub mod audio;
pub mod protocol;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running locoSaonduh");
}
