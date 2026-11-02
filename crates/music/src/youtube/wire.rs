use std::time::Duration;

use crate::models::{
    Album, AlbumDetail, Artist, ArtistProfile, ArtistRef, Playlist, PlaylistDetail, ReleaseType,
    Track, UserProfile,
};

pub fn track(source: ytmusic::Track, index: u32) -> Track {
    let artists = source
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let artist_refs = source
        .artists
        .into_iter()
        .map(|artist| ArtistRef {
            name: artist.name,
            id: artist.id,
        })
        .collect();
    Track {
        id: source.video_id,
        name: source.title,
        playable: source.available,
        artists,
        artist_refs,
        album: source
            .album
            .as_ref()
            .map(|album| album.name.clone())
            .unwrap_or_default(),
        album_id: source.album.and_then(|album| album.id),
        cover: cover(&source.thumbnails),
        duration: source.duration.unwrap_or(Duration::ZERO),
        added_at: None,
        added_by: None,
        playcount: None,
        popularity: 0,
        explicit: source.explicit,
        track_number: index + 1,
        disc_number: 1,
        tags: Vec::new(),
        languages: Vec::new(),
        release_date: String::new(),
        credits: Vec::new(),
    }
}

pub fn album(source: ytmusic::Album) -> Album {
    let artists = source
        .artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Album {
        id: source.browse_id,
        name: source.title,
        artists,
        artist_refs: source
            .artists
            .into_iter()
            .map(|artist| ArtistRef {
                name: artist.name,
                id: artist.id,
            })
            .collect(),
        cover: cover(&source.thumbnails),
        cover_large: cover_large(&source.thumbnails),
        release_type: release_type(source.kind),
        year: source.year.unwrap_or(0),
        track_count: source.track_count.unwrap_or(0),
        release_date: source.year.map(|year| year.to_string()).unwrap_or_default(),
        label: String::new(),
        copyrights: Vec::new(),
        added_at: None,
    }
}

/// An album page. A listed song names no date of its own, so it takes the album's year.
pub fn album_detail(source: ytmusic::AlbumDetail) -> AlbumDetail {
    let album = album(source.album);
    let tracks = source
        .tracks
        .into_iter()
        .enumerate()
        .map(|(index, item)| Track {
            release_date: album.release_date.clone(),
            ..track(item, index as u32)
        })
        .collect();
    AlbumDetail {
        cover_max: album.cover_large.clone(),
        album,
        tracks,
    }
}

pub fn playlist(source: ytmusic::Playlist, owned: bool, public: bool) -> Playlist {
    Playlist {
        id: source.id,
        name: source.title,
        owner: source.author.unwrap_or_default(),
        owner_id: String::new(),
        owned: owned || source.owned,
        collaborative: false,
        blend: false,
        public: source.public.unwrap_or(public),
        cover: cover(&source.thumbnails),
        track_count: source.track_count.unwrap_or(0),
        modified_at: None,
    }
}

pub fn playlist_detail(source: ytmusic::PlaylistDetail) -> PlaylistDetail {
    let owned = source.playlist.owned;
    let public = source.public;
    let tracks = source
        .tracks
        .into_iter()
        .enumerate()
        .map(|(index, item)| track(item, index as u32))
        .collect();
    PlaylistDetail {
        playlist: playlist(source.playlist, owned, public),
        tracks,
        continuation: source.continuation,
    }
}

pub fn playlist_page(source: ytmusic::PlaylistPage) -> (Vec<Track>, Option<String>) {
    let tracks = source
        .tracks
        .into_iter()
        .enumerate()
        .map(|(index, item)| track(item, index as u32))
        .collect();
    (tracks, source.continuation)
}

pub fn artist(source: ytmusic::Artist) -> Artist {
    let top_tracks = source
        .top_tracks
        .into_iter()
        .enumerate()
        .map(|(index, item)| track(item, index as u32))
        .collect();
    let albums = source
        .albums
        .into_iter()
        .chain(source.singles)
        .map(album)
        .collect();
    Artist {
        name: source.name,
        cover_large: cover_large(&source.thumbnails),
        biography: source.description,
        monthly_listeners: None,
        top_tracks,
        albums,
    }
}

pub fn artist_profile(source: &ytmusic::Artist) -> ArtistProfile {
    ArtistProfile {
        name: source.name.clone(),
        cover_large: cover_large(&source.thumbnails),
        biography: source.description.clone(),
    }
}

pub fn profile(source: ytmusic::Profile) -> UserProfile {
    UserProfile {
        avatar: source
            .thumbnails
            .last()
            .map(|thumbnail| thumbnail.url.clone()),
        id: source.email.unwrap_or_else(|| source.name.clone()),
        display_name: source.name,
    }
}

fn release_type(kind: ytmusic::AlbumKind) -> ReleaseType {
    match kind {
        ytmusic::AlbumKind::Album => ReleaseType::Album,
        ytmusic::AlbumKind::Single => ReleaseType::Single,
        ytmusic::AlbumKind::Ep => ReleaseType::Ep,
        ytmusic::AlbumKind::Compilation => ReleaseType::Compilation,
    }
}

pub fn cover(thumbnails: &[ytmusic::Thumbnail]) -> Option<String> {
    thumbnails.last().map(|thumb| thumb.url.clone())
}

pub fn cover_large(thumbnails: &[ytmusic::Thumbnail]) -> Option<String> {
    thumbnails.last().map(|thumb| thumb.url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(year: Option<i32>) -> ytmusic::AlbumDetail {
        ytmusic::AlbumDetail {
            album: ytmusic::Album {
                browse_id: "MPRE1".to_owned(),
                playlist_id: None,
                title: "Album".to_owned(),
                artists: Vec::new(),
                kind: ytmusic::AlbumKind::Album,
                year,
                track_count: Some(1),
                thumbnails: Vec::new(),
            },
            description: None,
            duration_text: None,
            tracks: vec![ytmusic::Track::default()],
        }
    }

    #[test]
    fn an_albums_tracks_carry_its_year() {
        assert_eq!(
            album_detail(detail(Some(1999))).tracks[0].release_date,
            "1999"
        );
        assert_eq!(album_detail(detail(None)).tracks[0].release_date, "");
    }
}
