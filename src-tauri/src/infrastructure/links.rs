use crate::domain::AppError;

pub fn launch(url: &str, app_name: Option<&str>) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    if let Some(app) = app_name {
        let status = std::process::Command::new("open")
            .args(["-a", app, url])
            .status()
            .map_err(|err| AppError::message(format!("No se pudo abrir {app}: {err}")))?;
        if status.success() {
            return Ok(());
        }
        return Err(AppError::message(format!("No encontré {app}.")));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app_name;
    open::that(url).map_err(|err| AppError::message(format!("No se pudo abrir el enlace: {err}")))
}
