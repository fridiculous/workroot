use serde::Serialize;

use crate::error::{AppError, AppResult};

use super::RadarView;

#[derive(Debug, Serialize)]
struct StatusJson<'a> {
    schema_version: u32,
    #[serde(flatten)]
    view: &'a RadarView,
}

pub(super) fn render_radar_json(view: &RadarView) -> AppResult<String> {
    serde_json::to_string_pretty(&StatusJson {
        schema_version: 1,
        view,
    })
    .map(|json| format!("{json}\n"))
    .map_err(|source| AppError::SerializeJson {
        kind: "status JSON",
        source: Box::new(source),
    })
}
