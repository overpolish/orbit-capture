#[cfg(desktop)]
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .setup(|app| {
      #[cfg(target_os = "macos")]
      app.set_activation_policy(tauri::ActivationPolicy::Accessory);

      #[cfg(desktop)]
      tray::initialize(app)?;

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
