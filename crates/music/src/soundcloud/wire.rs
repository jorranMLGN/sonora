use std::time::Duration;

use serde::Deserialize;

use crate::models::{
    Album as AlbumModel, ArtistRef, Playlist as PlaylistModel, ReleaseType, Track as Model,
};

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    pub id: u64,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub streamable: bool,
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub playback_count: Option<u64>,
    #[serde(default)]
    pub likes_count: Option<u64>,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub genre: Option<String>,
    pub user: User,
}

// filename substitution, not a request
#[allow(dead_code)]
pub fn artwork(url: Option<&str>, size: &str) -> Option<String> {
    let url = url?;
    Some(match url.rsplit_once("-large.") {
        Some((head, ext)) => format!("{head}-{size}.{ext}"),
        None => url.to_string(),
    })
}

#[allow(dead_code)]
pub fn track(raw: Track) -> Model {
    let artist = raw.user.username.clone();
    Model {
        id: Some(raw.id.to_string()),
        name: raw.title,
        playable: raw.streamable && raw.policy != "BLOCK" && raw.policy != "SNIP",
        artists: artist.clone(),
        artist_refs: vec![ArtistRef {
            name: artist,
            id: Some(raw.user.id.to_string()),
        }],
        album: String::new(),
        album_id: None,
        cover: artwork(raw.artwork_url.as_deref(), "t500x500"),
        duration: Duration::from_millis(raw.duration),
        added_at: None,
        added_by: None,
        playcount: raw.playback_count,
        popularity: 0,
        explicit: false,
        track_number: 0,
        disc_number: 1,
        tags: raw.genre.into_iter().collect(),
        languages: Vec::new(),
        credits: Vec::new(),
    }
}

/// One entry in a playlist's embedded track list.
///
/// SoundCloud inlines only the first five tracks in full; the rest arrive as
/// id-and-policy stubs that a later task resolves in one batch request. Any
/// entry lacking `user` falls through to `Stub` regardless of what other
/// fields it carries, so those fields are silently discarded.
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Entry {
    Full(Box<Track>),
    Stub(Stub),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Stub {
    pub id: u64,
}

/// The id of every entry, full or stub, in the order the playlist gives them.
#[allow(dead_code)]
pub fn entry_ids(entries: &[Entry]) -> Vec<u64> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Full(track) => track.id,
            Entry::Stub(stub) => stub.id,
        })
        .collect()
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
pub struct Playlist {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub set_type: String,
    #[serde(default)]
    pub sharing: String,
    #[serde(default)]
    pub track_count: u32,
    #[serde(default)]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub permalink_url: Option<String>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub tracks: Vec<Entry>,
    pub user: User,
}

/// Maps a SoundCloud `set_type` onto the shared release model.
///
/// A set with no recognised type is a plain playlist, not an album.
#[allow(dead_code)]
pub fn release_type(set_type: &str) -> Option<ReleaseType> {
    match set_type {
        "album" => Some(ReleaseType::Album),
        "ep" => Some(ReleaseType::Ep),
        "single" => Some(ReleaseType::Single),
        "compilation" => Some(ReleaseType::Compilation),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn is_album(raw: &Playlist) -> bool {
    release_type(&raw.set_type).is_some()
}

#[allow(dead_code)]
pub fn playlist(raw: Playlist) -> PlaylistModel {
    PlaylistModel {
        id: raw.id.to_string(),
        name: raw.title,
        owner: raw.user.username,
        owner_id: raw.user.id.to_string(),
        owned: false,
        collaborative: false,
        blend: false,
        public: raw.sharing == "public",
        cover: artwork(raw.artwork_url.as_deref(), "t500x500"),
        track_count: raw.track_count,
        modified_at: None,
    }
}

/// Converts a set the caller has already confirmed is an album.
///
/// The caller must check `is_album(&raw)` first; a plain playlist's
/// `set_type` does not map onto a `ReleaseType` and this treats that as a
/// caller bug, not a value to guess at.
#[allow(dead_code)]
pub fn album(raw: Playlist) -> AlbumModel {
    debug_assert!(
        is_album(&raw),
        "album() called on set_type {:?}, which is not an album; check is_album() first",
        raw.set_type
    );
    let artist = raw.user.username.clone();
    let release_type = release_type(&raw.set_type).unwrap_or(ReleaseType::Album);
    AlbumModel {
        id: raw.id.to_string(),
        name: raw.title,
        artists: artist.clone(),
        artist_refs: vec![ArtistRef {
            name: artist,
            id: Some(raw.user.id.to_string()),
        }],
        cover: artwork(raw.artwork_url.as_deref(), "t500x500"),
        cover_large: artwork(raw.artwork_url.as_deref(), "t500x500"),
        release_type,
        year: 0,
        track_count: raw.track_count,
        release_date: raw.release_date.unwrap_or_default(),
        label: String::new(),
        copyrights: Vec::new(),
        added_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Entry, Playlist, Track as Raw, album, artwork, entry_ids, is_album, playlist, release_type,
        track,
    };

    fn fixture() -> Raw {
        let json = include_str!("fixtures/track.json");
        serde_json::from_str(json).expect("the captured track fixture must parse")
    }

    #[test]
    fn parses_the_captured_track() {
        let raw = fixture();
        assert!(raw.id > 0);
        assert!(!raw.title.is_empty());
    }

    #[test]
    fn converts_the_captured_track() {
        let converted = track(fixture());
        assert!(converted.id.is_some());
        assert!(!converted.name.is_empty());
        assert!(!converted.artists.is_empty());
        assert!(converted.duration.as_millis() > 0);
        assert_eq!(converted.disc_number, 1);
        assert!(!converted.explicit);
        assert!(converted.album.is_empty());
        assert!(converted.album_id.is_none());
    }

    #[test]
    fn refuses_a_blocked_track() {
        let mut raw = fixture();
        raw.policy = "BLOCK".to_string();
        assert!(!track(raw).playable);
    }

    #[test]
    fn refuses_a_snipped_track() {
        let mut raw = fixture();
        raw.policy = "SNIP".to_string();
        assert!(!track(raw).playable);
    }

    #[test]
    fn refuses_an_unstreamable_track() {
        let mut raw = fixture();
        raw.streamable = false;
        assert!(!track(raw).playable);
    }

    #[test]
    fn upgrades_the_artwork_size() {
        let url = artwork(
            Some("https://i1.sndcdn.com/artworks-abc-large.jpg"),
            "t500x500",
        );
        assert_eq!(
            url.as_deref(),
            Some("https://i1.sndcdn.com/artworks-abc-t500x500.jpg")
        );
    }

    #[test]
    fn leaves_an_unsized_artwork_alone() {
        let url = artwork(Some("https://i1.sndcdn.com/artworks-abc.jpg"), "t500x500");
        assert_eq!(
            url.as_deref(),
            Some("https://i1.sndcdn.com/artworks-abc.jpg")
        );
    }

    #[test]
    fn has_no_artwork_without_a_url() {
        assert!(artwork(None, "t500x500").is_none());
    }

    #[test]
    fn an_unknown_policy_stays_playable() {
        let mut raw = fixture();
        raw.policy = "SOMETHING_ELSE".to_string();
        assert!(
            track(raw).playable,
            "a denylist must let an unrecognised policy through; an allowlist would not"
        );
    }

    #[test]
    fn maps_the_known_set_types() {
        use crate::models::ReleaseType;
        assert_eq!(release_type("album"), Some(ReleaseType::Album));
        assert_eq!(release_type("ep"), Some(ReleaseType::Ep));
        assert_eq!(release_type("single"), Some(ReleaseType::Single));
        assert_eq!(release_type("compilation"), Some(ReleaseType::Compilation));
    }

    #[test]
    fn treats_an_unknown_set_type_as_a_playlist() {
        assert_eq!(release_type(""), None);
        assert_eq!(release_type("playlist"), None);
    }

    #[test]
    fn recognises_the_captured_album() {
        let raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_album.json")).unwrap();
        assert!(is_album(&raw));
        let converted = album(raw);
        assert!(!converted.name.is_empty());
        assert!(!converted.artists.is_empty());
    }

    #[test]
    fn recognises_the_captured_set_as_a_playlist() {
        let raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
        assert!(!is_album(&raw));
        let converted = playlist(raw);
        assert!(!converted.name.is_empty());
        assert!(!converted.owner.is_empty());
    }

    #[test]
    fn reads_visibility_from_sharing() {
        let mut raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
        raw.sharing = "private".to_string();
        assert!(!playlist(raw.clone()).public);
        raw.sharing = "public".to_string();
        assert!(playlist(raw).public);
    }

    #[test]
    fn parses_a_playlist_of_mixed_full_and_stub_entries() {
        let raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_album.json")).unwrap();
        assert_eq!(raw.tracks.len(), 17, "the album fixture has 17 entries");
        let full = raw
            .tracks
            .iter()
            .filter(|e| matches!(e, Entry::Full(_)))
            .count();
        let stub = raw
            .tracks
            .iter()
            .filter(|e| matches!(e, Entry::Stub(_)))
            .count();
        assert_eq!((full, stub), (5, 12), "only the first five arrive in full");
    }

    #[test]
    #[should_panic(expected = "which is not an album")]
    fn album_panics_on_a_plain_playlist_in_debug_builds() {
        let raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
        album(raw);
    }

    #[test]
    fn lists_entry_ids_in_playlist_order() {
        let raw: Playlist =
            serde_json::from_str(include_str!("fixtures/playlist_set.json")).unwrap();
        let ids = entry_ids(&raw.tracks);
        assert_eq!(ids.len(), raw.tracks.len());
        assert!(ids.iter().all(|id| *id > 0));
    }
}
