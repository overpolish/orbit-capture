use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorDetails {
  id: u32,
  name: String,
  position: Position,
  size: Size,
  scale_factor: f32,
  is_primary: bool,
  is_builtin: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Position {
  x: i32,
  y: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Size {
  width: u32,
  height: u32,
}

#[tauri::command]
pub fn list_monitors() -> Result<Vec<MonitorDetails>, String> {
  let monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;

  monitors
    .into_iter()
    .map(|monitor| {
      Ok(MonitorDetails {
        id: monitor.id().map_err(|error| error.to_string())?,
        name: monitor
          .friendly_name()
          .or_else(|_| monitor.name())
          .map_err(|error| error.to_string())?,
        position: Position {
          x: monitor.x().map_err(|error| error.to_string())?,
          y: monitor.y().map_err(|error| error.to_string())?,
        },
        size: Size {
          width: monitor.width().map_err(|error| error.to_string())?,
          height: monitor.height().map_err(|error| error.to_string())?,
        },
        scale_factor: monitor.scale_factor().map_err(|error| error.to_string())?,
        is_primary: monitor.is_primary().map_err(|error| error.to_string())?,
        is_builtin: monitor.is_builtin().map_err(|error| error.to_string())?,
      })
    })
    .collect()
}
