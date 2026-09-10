//! Reading and editing SoundCloud sets (playlists and albums).
//!
//! **Lost-update caveat:** SoundCloud edits a set by replacing its whole
//! track list, not by patching one entry. `edit_tracks` therefore reads the
//! current list, changes it, and writes the whole thing back. If two edits
//! race, both read the same starting list and the second write wins,
//! silently discarding the first. SoundCloud's own client has the same
//! behaviour and the API offers no conditional write (no ETag, no version
//! field), so this is accepted rather than solved.

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use serde::Serialize;

use crate::soundcloud::http::Http;
use crate::soundcloud::wire::{self, Entry, Playlist as RawPlaylist};

/// How many ids one `GET /tracks?ids=…` request carries before the URL gets
/// chunked into more than one request.
const BATCH_SIZE: usize = 50;

async fn fetch_raw(http: &Http, id: &str) -> Result<RawPlaylist> {
    let raw: RawPlaylist = http
        .get_json(&format!("/playlists/{id}"), &[])
        .await
        .context("cannot fetch the soundcloud set")?;
    http.remember_permalink(raw.id, raw.permalink_url.clone());
    Ok(raw)
}

/// Resolves a set's full track list, in the set's own order.
///
/// `GET /playlists/{id}` inlines only the first five tracks; the rest arrive
/// as id-only stubs. This fills the stubs in with one batched
/// `GET /tracks?ids=…` call (chunked if there are more ids than fit in one
/// request) and then restores the set's own ordering, because the batch
/// endpoint answers in its own order, not the order the ids were sent in.
async fn resolve_tracks(http: &Http, id: &str, raw: &RawPlaylist) -> Result<Vec<crate::Track>> {
    let order = wire::entry_ids(&raw.tracks);

    let mut by_id: HashMap<u64, crate::Track> = HashMap::with_capacity(order.len());
    let mut missing = Vec::new();
    for entry in &raw.tracks {
        match entry {
            Entry::Full(track) => {
                http.remember_permalink(track.id, track.permalink_url.clone());
                by_id.insert(track.id, wire::track((**track).clone()));
            }
            Entry::Stub(stub) => missing.push(stub.id),
        }
    }

    for chunk in missing.chunks(BATCH_SIZE) {
        let ids = chunk
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let fetched: Vec<wire::Track> = http
            .get_json("/tracks", &[("ids", ids.as_str())])
            .await
            .context("cannot resolve the soundcloud set's remaining tracks")?;
        http.remember_permalinks(
            fetched
                .iter()
                .map(|track| (track.id, track.permalink_url.clone())),
        );
        for raw_track in fetched {
            by_id.insert(raw_track.id, wire::track(raw_track));
        }
    }

    let resolved = in_playlist_order(&order, by_id);
    if resolved.len() != order.len() {
        log::warn!(
            "soundcloud: playlist {id} dropped {} unresolved track ids of {}",
            order.len() - resolved.len(),
            order.len()
        );
    }
    Ok(resolved)
}

/// Puts resolved tracks back into the order the playlist gave.
///
/// The batch endpoint answers in its own order, so the playlist's own id
/// sequence is the only authority on how an album plays. An id the batch
/// call did not return (a deleted or region-blocked track) is dropped
/// silently rather than represented as a placeholder.
fn in_playlist_order(order: &[u64], mut by_id: HashMap<u64, crate::Track>) -> Vec<crate::Track> {
    order.iter().filter_map(|id| by_id.remove(id)).collect()
}

pub async fn detail(http: &Http, id: &str) -> Result<crate::PlaylistDetail> {
    let raw = fetch_raw(http, id).await?;
    let tracks = resolve_tracks(http, id, &raw).await?;
    Ok(crate::PlaylistDetail {
        playlist: wire::playlist(raw),
        tracks,
    })
}

pub async fn tracks(http: &Http, id: &str) -> Result<Vec<crate::Track>> {
    let raw = fetch_raw(http, id).await?;
    resolve_tracks(http, id, &raw).await
}

pub async fn album(http: &Http, id: &str) -> Result<crate::AlbumDetail> {
    let raw = fetch_raw(http, id).await?;
    let tracks = resolve_tracks(http, id, &raw).await?;
    Ok(crate::AlbumDetail {
        album: wire::album(raw),
        tracks,
    })
}

/// Resolves an album's tracks, filling in the album name and id.
///
/// A bare SoundCloud track genuinely does not know which set it belongs to,
/// so `wire::track` always leaves `album` and `album_id` empty; this is the
/// one place that has just read the set and can fill both in.
pub async fn album_tracks(http: &Http, id: &str) -> Result<Vec<crate::Track>> {
    let raw = fetch_raw(http, id).await?;
    let mut tracks = resolve_tracks(http, id, &raw).await?;
    let album_name = raw.title.clone();
    let album_id = raw.id.to_string();
    for track in &mut tracks {
        track.album = album_name.clone();
        track.album_id = Some(album_id.clone());
    }
    Ok(tracks)
}

/// Covers for a playlist, deduplicated in track order.
///
/// Reuses `crate::distinct_covers` rather than reimplementing dedup here.
pub async fn playlist_covers(http: &Http, id: &str, wanted: usize) -> Result<Vec<String>> {
    let tracks = tracks(http, id).await?;
    Ok(crate::distinct_covers(&tracks, wanted))
}

#[derive(Serialize)]
struct CreateBody<'a> {
    playlist: CreatePlaylist<'a>,
}

#[derive(Serialize)]
struct CreatePlaylist<'a> {
    title: &'a str,
    sharing: &'a str,
    tracks: &'a [TrackId],
}

#[derive(Serialize)]
struct TrackId {
    id: u64,
}

#[derive(Serialize)]
struct EditBody<'a> {
    playlist: EditPlaylist<'a>,
}

#[derive(Serialize, Default)]
struct EditPlaylist<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sharing: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tracks: Option<Vec<TrackId>>,
}

#[derive(serde::Deserialize)]
struct CreatedPlaylist {
    id: u64,
}

/// Creates a new, empty, private set.
///
/// UNVERIFIED: no write endpoint on this API has been exercised against the
/// live service. This follows `POST /playlists` with
/// `{"playlist": {"title": name, "sharing": "private", "tracks": []}}` as
/// the steps describe.
pub async fn create(http: &Http, name: &str) -> Result<String> {
    require_authenticated(http)?;
    let body = CreateBody {
        playlist: CreatePlaylist {
            title: name,
            sharing: "private",
            tracks: &[],
        },
    };
    let created: CreatedPlaylist = http
        .post_json("/playlists", &body)
        .await
        .context("cannot create the soundcloud set")?;
    Ok(created.id.to_string())
}

/// Renames a set.
///
/// UNVERIFIED: this follows `PUT /playlists/{id}` with
/// `{"playlist": {"title": name}}` as the steps describe.
pub async fn rename(http: &Http, id: &str, name: &str) -> Result<()> {
    require_authenticated(http)?;
    let body = EditBody {
        playlist: EditPlaylist {
            title: Some(name),
            ..Default::default()
        },
    };
    http.put_json::<_, serde_json::Value>(&format!("/playlists/{id}"), &body)
        .await
        .context("cannot rename the soundcloud set")?;
    Ok(())
}

/// Deletes a set.
///
/// UNVERIFIED: this follows `DELETE /playlists/{id}` as the steps describe.
pub async fn delete(http: &Http, id: &str) -> Result<()> {
    require_authenticated(http)?;
    http.delete(&format!("/playlists/{id}"))
        .await
        .context("cannot delete the soundcloud set")
}

/// Changes a set's public/private visibility.
///
/// UNVERIFIED: this follows `PUT /playlists/{id}` with
/// `{"playlist": {"sharing": "public" | "private"}}` as the steps describe.
pub async fn set_public(http: &Http, id: &str, public: bool) -> Result<()> {
    require_authenticated(http)?;
    let sharing = if public { "public" } else { "private" };
    let body = EditBody {
        playlist: EditPlaylist {
            sharing: Some(sharing),
            ..Default::default()
        },
    };
    http.put_json::<_, serde_json::Value>(&format!("/playlists/{id}"), &body)
        .await
        .context("cannot change the soundcloud set's visibility")?;
    Ok(())
}

/// Replaces a set's whole track list.
///
/// UNVERIFIED: this follows `PUT /playlists/{id}` with
/// `{"playlist": {"tracks": [{"id": …}, …]}}` as the steps describe.
pub async fn set_tracks(http: &Http, id: &str, ids: &[u64]) -> Result<()> {
    require_authenticated(http)?;
    let body = EditBody {
        playlist: EditPlaylist {
            tracks: Some(ids.iter().map(|id| TrackId { id: *id }).collect()),
            ..Default::default()
        },
    };
    http.put_json::<_, serde_json::Value>(&format!("/playlists/{id}"), &body)
        .await
        .context("cannot update the soundcloud set's tracks")?;
    Ok(())
}

/// The current track ids of a set, in its own order — the read half of a
/// read-modify-write edit.
async fn track_ids(http: &Http, id: &str) -> Result<Vec<u64>> {
    let raw = fetch_raw(http, id).await?;
    Ok(wire::entry_ids(&raw.tracks))
}

/// Reads a set's track list, applies `change`, and writes the whole list
/// back. See the module doc comment for the lost-update caveat this
/// read-modify-write shape accepts.
async fn edit_tracks<F>(http: &Http, id: &str, change: F) -> Result<()>
where
    F: FnOnce(&mut Vec<u64>),
{
    let mut ids = track_ids(http, id).await?;
    change(&mut ids);
    set_tracks(http, id, &ids).await
}

pub(crate) fn append(ids: &mut Vec<u64>, id: u64) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

pub(crate) fn remove(ids: &mut Vec<u64>, id: u64) {
    ids.retain(|kept| *kept != id);
}

/// Adds a track to a set, unless it is already there.
///
/// UNVERIFIED: built on the unverified `set_tracks` read-modify-write.
pub async fn add_track(http: &Http, playlist_id: &str, track_id: u64) -> Result<()> {
    require_authenticated(http)?;
    edit_tracks(http, playlist_id, |ids| append(ids, track_id)).await
}

/// Removes every occurrence of a track from a set.
///
/// UNVERIFIED: built on the unverified `set_tracks` read-modify-write.
pub async fn remove_track(http: &Http, playlist_id: &str, track_id: u64) -> Result<()> {
    require_authenticated(http)?;
    edit_tracks(http, playlist_id, |ids| remove(ids, track_id)).await
}

fn require_authenticated(http: &Http) -> Result<()> {
    if !http.authenticated() {
        anyhow::bail!("signing in is required to change a set");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_without_duplicating() {
        let mut ids = vec![1_u64, 2, 3];
        append(&mut ids, 4);
        assert_eq!(ids, vec![1, 2, 3, 4]);
        append(&mut ids, 2);
        assert_eq!(
            ids,
            vec![1, 2, 3, 4],
            "an existing track must not be added twice"
        );
    }

    #[test]
    fn removes_every_occurrence() {
        let mut ids = vec![1_u64, 2, 3, 2];
        remove(&mut ids, 2);
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn removing_an_absent_track_changes_nothing() {
        let mut ids = vec![1_u64, 2];
        remove(&mut ids, 9);
        assert_eq!(ids, vec![1, 2]);
    }

    fn track_with_id(id: u64) -> crate::Track {
        let mut track = crate::soundcloud::wire::track(
            serde_json::from_str(include_str!("fixtures/track.json")).unwrap(),
        );
        track.id = Some(id.to_string());
        track
    }

    #[test]
    fn looks_up_every_id_by_order_never_by_map_iteration() {
        // `by_id` is a HashMap, so its iteration order is arbitrary
        // regardless of how it was populated — the batch response order
        // cannot leak into the result even if this map happened to be
        // built in playlist order. Only `order` can determine the output,
        // which is what makes a wrong order unrepresentable.
        let order = vec![10_u64, 20, 30, 40];
        let by_id: HashMap<u64, crate::Track> = [30, 10, 40, 20]
            .into_iter()
            .map(|id| (id, track_with_id(id)))
            .collect();

        let result = in_playlist_order(&order, by_id);

        let ids: Vec<u64> = result
            .iter()
            .map(|t| t.id.as_deref().unwrap().parse().unwrap())
            .collect();
        assert_eq!(ids, order);
    }

    #[test]
    fn drops_an_id_the_batch_call_did_not_return() {
        let order = vec![1_u64, 2, 3];
        let mut resolved = HashMap::new();
        resolved.insert(1, track_with_id(1));
        resolved.insert(3, track_with_id(3));
        // 2 is missing: deleted or region-blocked.

        let result = in_playlist_order(&order, resolved);

        let ids: Vec<u64> = result
            .iter()
            .map(|t| t.id.as_deref().unwrap().parse().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![1, 3],
            "a missing id must be dropped, not placeholdered"
        );
    }

    #[test]
    fn resolves_a_playlist_with_more_than_five_entries() {
        let raw: RawPlaylist =
            serde_json::from_str(include_str!("fixtures/playlist_album.json")).unwrap();
        let order = wire::entry_ids(&raw.tracks);
        assert_eq!(order.len(), 17);

        let mut by_id = HashMap::new();
        for entry in &raw.tracks {
            match entry {
                Entry::Full(track) => {
                    by_id.insert(track.id, wire::track((**track).clone()));
                }
                Entry::Stub(_) => {}
            }
        }
        // Simulate the batch response resolving every stub, in an order
        // that does not match the playlist.
        let stub_ids: Vec<u64> = raw
            .tracks
            .iter()
            .filter_map(|e| match e {
                Entry::Stub(stub) => Some(stub.id),
                Entry::Full(_) => None,
            })
            .collect();
        for &id in stub_ids.iter().rev() {
            by_id.insert(id, track_with_id(id));
        }

        let result = in_playlist_order(&order, by_id);
        assert_eq!(
            result.len(),
            17,
            "all five full tracks and all stubs must resolve"
        );
        let result_ids: Vec<u64> = result
            .iter()
            .map(|t| t.id.as_deref().unwrap().parse().unwrap())
            .collect();
        assert_eq!(
            result_ids, order,
            "the resolved tracks must come back in the playlist's own sequence, \
             not the order the stubs happened to be inserted into the map"
        );
    }
}
