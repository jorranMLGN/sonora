use std::time::Duration;

use serde::Deserialize;

use crate::models::{ArtistRef, Track as Model};

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

#[cfg(test)]
mod tests {
    use super::{Track as Raw, artwork, track};

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
}
