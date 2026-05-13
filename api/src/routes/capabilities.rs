use axum::response::Json;

use skycode_contracts::sky_capability::SkyCapabilityInfo;

pub async fn handler() -> Json<SkyCapabilityInfo> {
    Json(SkyCapabilityInfo::default())
}
