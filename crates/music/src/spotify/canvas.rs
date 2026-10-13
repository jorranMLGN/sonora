//! A track's Canvas through extended metadata, the same batched route `TRACK_V4` takes.

use anyhow::{Context as _, Result};
use librespot_core::Session;
use librespot_protocol::canvaz::Type;
use librespot_protocol::canvaz::entity_canvaz_response::Canvaz;
use librespot_protocol::extension_kind::ExtensionKind;
use protobuf::Message as _;

use super::collection;

/// The video url for `track_id`, or `None` when the track has no canvas or only a still one.
/// An empty extension is how Spotify says "none".
pub(crate) async fn canvas(session: &Session, track_id: &str) -> Result<Option<String>> {
    let uri = format!("spotify:track:{track_id}");
    let entities = collection::extended(session, std::slice::from_ref(&uri), ExtensionKind::CANVAZ)
        .await
        .context("cannot read the canvas")?;
    let Some(entity) = entities.into_iter().find(|entity| entity.entity_uri == uri) else {
        return Ok(None);
    };
    if entity.extension_data.value.is_empty() {
        return Ok(None);
    }
    let canvas = Canvaz::parse_from_bytes(&entity.extension_data.value)
        .context("cannot parse the canvas")?;
    let moving = matches!(
        canvas.type_.enum_value(),
        Ok(Type::VIDEO | Type::VIDEO_LOOPING | Type::VIDEO_LOOPING_RANDOM)
    );
    Ok((moving && !canvas.url.is_empty()).then_some(canvas.url))
}
