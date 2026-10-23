use ytmusic::Thumbnail;
use ytmusic::web::{Channel, ChannelPage, PlaylistPage, Shelf, Video, WebPlaylist};

use crate::{Artist, ArtistProfile, ArtistRef, GenreItem, GenreSection, Playlist, PlaylistDetail};
use crate::{SavedArtist, Track};

/// A video as a track by its channel, or nothing for one without a length, such as a live
/// stream. A video has no album, and its cover is the widest thumbnail.
pub fn track(source: Video) -> Option<Track> {
    let duration = source.duration?;
    let (artists, artist_refs) = match source.channel {
        Some(channel) => (
            channel.name.clone(),
            vec![ArtistRef {
                name: channel.name,
                id: Some(channel.id),
            }],
        ),
        None => (String::new(), Vec::new()),
    };
    Some(Track {
        id: Some(source.id),
        name: source.title,
        playable: true,
        artists,
        artist_refs,
        album: String::new(),
        album_id: None,
        cover: widest(&source.thumbnails),
        duration,
        added_at: None,
        added_by: None,
        playcount: None,
        popularity: 0,
        explicit: false,
        track_number: 1,
        disc_number: 1,
        tags: Vec::new(),
        languages: Vec::new(),
        credits: Vec::new(),
    })
}

/// The videos that are tracks, numbered from one in the order they came.
pub fn tracks(videos: Vec<Video>) -> Vec<Track> {
    numbered(videos.into_iter().filter_map(track).collect())
}

/// `tracks` numbered from one in their order, for a list put together from several pages.
pub fn numbered(tracks: Vec<Track>) -> Vec<Track> {
    tracks
        .into_iter()
        .enumerate()
        .map(|(index, track)| Track {
            track_number: index as u32 + 1,
            ..track
        })
        .collect()
}

/// A channel as a followable artist, pictured by its avatar.
pub fn artist(source: Channel) -> SavedArtist {
    SavedArtist {
        cover: widest(&source.thumbnails),
        id: source.id,
        name: source.name,
        added_at: None,
    }
}

/// A playlist card. Whether the account owns it is the client's call, so it starts unowned.
pub fn playlist(source: WebPlaylist) -> Playlist {
    let (owner, owner_id) = source
        .owner
        .map(|owner| (owner.name, owner.id))
        .unwrap_or_default();
    Playlist {
        id: source.id,
        name: source.title,
        owner,
        owner_id,
        owned: false,
        collaborative: false,
        blend: false,
        public: false,
        cover: widest(&source.thumbnails),
        track_count: source.count.unwrap_or(0),
        modified_at: None,
    }
}

/// A playlist's first page of videos, with the token that leads to the next.
pub fn playlist_detail(source: PlaylistPage) -> PlaylistDetail {
    PlaylistDetail {
        playlist: playlist(source.playlist),
        tracks: tracks(source.videos),
        continuation: source.continuation,
    }
}

/// A channel page as an artist page: its uploads are the tracks, and it has no albums.
pub fn artist_page(source: ChannelPage) -> Artist {
    Artist {
        cover_large: widest(&source.channel.thumbnails),
        name: source.channel.name,
        biography: source.description,
        monthly_listeners: None,
        top_tracks: tracks(source.uploads),
        albums: Vec::new(),
    }
}

/// A channel page as the header of an artist page.
pub fn artist_profile(source: ChannelPage) -> ArtistProfile {
    ArtistProfile {
        cover_large: widest(&source.channel.thumbnails),
        name: source.channel.name,
        biography: source.description,
    }
}

/// A home shelf as a section of tracks, titled `untitled` when YouTube gave it no title.
/// Nothing for a shelf without a playable video.
pub fn section(source: Shelf, untitled: &str) -> Option<GenreSection> {
    let items: Vec<GenreItem> = tracks(source.videos)
        .into_iter()
        .map(GenreItem::Track)
        .collect();
    let title = match source.title.is_empty() {
        true => untitled.to_owned(),
        false => source.title,
    };
    (!items.is_empty()).then_some(GenreSection {
        title,
        items,
        provider: None,
    })
}

/// The url of the widest thumbnail, the sharpest picture YouTube has for the item.
fn widest(thumbnails: &[Thumbnail]) -> Option<String> {
    thumbnails
        .iter()
        .max_by_key(|thumbnail| thumbnail.width)
        .map(|thumbnail| thumbnail.url.clone())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ytmusic::web::ChannelRef;

    use super::*;

    fn thumb(url: &str, width: u32) -> Thumbnail {
        Thumbnail {
            url: url.to_owned(),
            width,
            height: width * 9 / 16,
        }
    }

    fn video(duration: Option<Duration>) -> Video {
        Video {
            id: "vid1".to_owned(),
            title: "Live set".to_owned(),
            channel: Some(ChannelRef {
                id: "UCx".to_owned(),
                name: "Boiler Room".to_owned(),
            }),
            duration,
            thumbnails: vec![thumb("wide", 1280), thumb("small", 120), thumb("mid", 480)],
        }
    }

    #[test]
    fn a_video_becomes_a_track_by_its_channel() {
        let track = track(video(Some(Duration::from_secs(7200)))).unwrap();
        assert_eq!(track.id.as_deref(), Some("vid1"));
        assert_eq!(track.name, "Live set");
        assert_eq!(track.artists, "Boiler Room");
        assert_eq!(track.artist_refs[0].id.as_deref(), Some("UCx"));
        assert_eq!(track.cover.as_deref(), Some("wide"));
        assert_eq!(track.duration, Duration::from_secs(7200));
        assert!(track.album.is_empty());
        assert!(track.album_id.is_none());
    }

    #[test]
    fn a_video_without_length_is_not_a_track() {
        assert!(track(video(None)).is_none());
    }

    #[test]
    fn a_channel_becomes_an_artist_with_its_avatar() {
        let artist = artist(Channel {
            id: "UCx".to_owned(),
            name: "Boiler Room".to_owned(),
            thumbnails: vec![thumb("small", 88), thumb("avatar", 176)],
            subscribed: Some(true),
        });
        assert_eq!(artist.id, "UCx");
        assert_eq!(artist.name, "Boiler Room");
        assert_eq!(artist.cover.as_deref(), Some("avatar"));
    }

    #[test]
    fn an_untitled_shelf_takes_the_fallback_title() {
        let shelf = Shelf {
            title: String::new(),
            videos: vec![video(Some(Duration::from_secs(60))), video(None)],
        };
        let section = section(shelf, "YouTube").unwrap();
        assert_eq!(section.title, "YouTube");
        assert_eq!(section.items.len(), 1);
    }
}
