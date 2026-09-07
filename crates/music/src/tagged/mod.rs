//! The layer that puts a provider's slug on every id it hands the app.
//!
//! `Tagged` wraps any `MusicApi`, so `spotify`, `youtube`, `soundcloud` and
//! `local` never see a tag and never write one.
//!
//! `MusicApi` has default bodies, so a method left out here would silently
//! replace the wrapped provider's own with the trait's — an empty `Vec` or an
//! error, with nothing to point at. Every method is forwarded explicitly for
//! that reason; none may fall through to a default.

mod models;

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::tag;
use crate::{
    Album, AlbumDetail, Artist, ArtistProfile, Genre, GenreDetail, GenreSection, HomeFeed, Lyrics,
    MediaKind, MusicApi, Playlist, PlaylistDetail, SavedArtist, Track, TrackTags, UserDetail,
    UserProfile,
};

/// A `MusicApi` that speaks tagged ids to the app and bare ids to the
/// provider underneath.
///
/// Every method follows the same two steps: `untag` each id argument on the
/// way in, walk each returned model on the way out. A method that carries no
/// id in either direction delegates unchanged.
pub struct Tagged {
    slug: &'static str,
    inner: Arc<dyn MusicApi>,
}

pub fn new(slug: &'static str, inner: Arc<dyn MusicApi>) -> Arc<dyn MusicApi> {
    Arc::new(Tagged { slug, inner })
}

#[async_trait]
impl MusicApi for Tagged {
    fn alive(&self) -> bool {
        self.inner.alive()
    }

    fn share_url(&self, kind: MediaKind, id: &str) -> Option<String> {
        self.inner.share_url(kind, tag::untag(id))
    }

    async fn profile(&self) -> Result<UserProfile> {
        let mut value = self.inner.profile().await?;
        models::user_profile(self.slug, &mut value);
        Ok(value)
    }

    async fn user(&self, user_id: &str) -> Result<UserDetail> {
        let mut value = self.inner.user(tag::untag(user_id)).await?;
        models::user_detail(self.slug, &mut value);
        Ok(value)
    }

    async fn artist(&self, artist_id: &str) -> Result<Artist> {
        let mut value = self.inner.artist(tag::untag(artist_id)).await?;
        models::artist(self.slug, &mut value);
        Ok(value)
    }

    async fn artist_profile(&self, artist_id: &str) -> Result<ArtistProfile> {
        self.inner.artist_profile(tag::untag(artist_id)).await
    }

    async fn artist_images(&self, ids: Vec<String>) -> Result<HashMap<String, String>> {
        let bare = ids.iter().map(|id| tag::untag(id).to_owned()).collect();
        let images = self.inner.artist_images(bare).await?;
        Ok(images
            .into_iter()
            .map(|(id, image)| (tag::tag(self.slug, &id), image))
            .collect())
    }

    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let mut values = self.inner.saved_tracks(limit).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn all_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let mut values = self.inner.all_tracks(limit).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        self.inner
            .set_track_saved(tag::untag(track_id), saved)
            .await
    }

    async fn track_tags(&self, track_id: &str) -> Result<TrackTags> {
        self.inner.track_tags(tag::untag(track_id)).await
    }

    async fn set_track_tags(&self, track_id: &str, tags: TrackTags) -> Result<()> {
        self.inner.set_track_tags(tag::untag(track_id), tags).await
    }

    async fn track(&self, track_id: &str) -> Result<Track> {
        let mut value = self.inner.track(tag::untag(track_id)).await?;
        models::track(self.slug, &mut value);
        Ok(value)
    }

    async fn track_playcount(&self, track_id: &str) -> Result<Option<u64>> {
        self.inner.track_playcount(tag::untag(track_id)).await
    }

    async fn track_lyrics(&self, track_id: &str) -> Result<Option<Lyrics>> {
        self.inner.track_lyrics(tag::untag(track_id)).await
    }

    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>> {
        let mut values = self.inner.playlists(limit).await?;
        for value in &mut values {
            models::playlist(self.slug, value);
        }
        Ok(values)
    }

    async fn create_playlist(&self, name: &str) -> Result<String> {
        let id = self.inner.create_playlist(name).await?;
        Ok(tag::tag(self.slug, &id))
    }

    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()> {
        self.inner
            .rename_playlist(tag::untag(playlist_id), name)
            .await
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<()> {
        self.inner.delete_playlist(tag::untag(playlist_id)).await
    }

    async fn remove_playlist_from_library(&self, playlist_id: &str) -> Result<()> {
        self.inner
            .remove_playlist_from_library(tag::untag(playlist_id))
            .await
    }

    async fn add_playlist_to_library(&self, playlist_id: &str) -> Result<()> {
        self.inner
            .add_playlist_to_library(tag::untag(playlist_id))
            .await
    }

    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()> {
        self.inner
            .set_playlist_public(tag::untag(playlist_id), public)
            .await
    }

    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        self.inner
            .add_track_to_playlist(tag::untag(playlist_id), tag::untag(track_id))
            .await
    }

    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        self.inner
            .remove_track_from_playlist(tag::untag(playlist_id), tag::untag(track_id))
            .await
    }

    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>> {
        let mut values = self.inner.saved_albums(limit).await?;
        for value in &mut values {
            models::album(self.slug, value);
        }
        Ok(values)
    }

    async fn set_album_saved(&self, album_id: &str, saved: bool) -> Result<()> {
        self.inner
            .set_album_saved(tag::untag(album_id), saved)
            .await
    }

    async fn saved_artists(&self, limit: u32) -> Result<Vec<SavedArtist>> {
        let mut values = self.inner.saved_artists(limit).await?;
        for value in &mut values {
            models::saved_artist(self.slug, value);
        }
        Ok(values)
    }

    async fn set_artist_saved(&self, artist_id: &str, saved: bool) -> Result<()> {
        self.inner
            .set_artist_saved(tag::untag(artist_id), saved)
            .await
    }

    async fn album(&self, album_id: &str) -> Result<AlbumDetail> {
        let mut value = self.inner.album(tag::untag(album_id)).await?;
        models::album_detail(self.slug, &mut value);
        Ok(value)
    }

    async fn album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        let mut values = self.inner.album_tracks(tag::untag(album_id)).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        let mut value = self.inner.playlist(tag::untag(playlist_id)).await?;
        models::playlist_detail(self.slug, &mut value);
        Ok(value)
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let mut values = self.inner.playlist_tracks(tag::untag(playlist_id)).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn playlist_covers(&self, playlist_id: &str, wanted: usize) -> Result<Vec<String>> {
        self.inner
            .playlist_covers(tag::untag(playlist_id), wanted)
            .await
    }

    async fn track_radio(&self, track_id: &str) -> Result<Vec<Track>> {
        let mut values = self.inner.track_radio(tag::untag(track_id)).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>> {
        let mut values = self.inner.search(query).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<Album>> {
        let mut values = self.inner.search_albums(query).await?;
        for value in &mut values {
            models::album(self.slug, value);
        }
        Ok(values)
    }

    async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        let mut values = self.inner.search_playlists(query).await?;
        for value in &mut values {
            models::playlist(self.slug, value);
        }
        Ok(values)
    }

    async fn home(&self) -> Result<HomeFeed> {
        let mut value = self.inner.home().await?;
        models::home_feed(self.slug, &mut value);
        Ok(value)
    }

    async fn name_home_playlists(&self, mut sections: Vec<GenreSection>) -> Vec<GenreSection> {
        for section in &mut sections {
            models::bare::genre_section(section);
        }
        let mut values = self.inner.name_home_playlists(sections).await;
        for value in &mut values {
            models::genre_section(self.slug, value);
        }
        values
    }

    async fn genres(&self) -> Result<Vec<Genre>> {
        let mut values = self.inner.genres().await?;
        for value in &mut values {
            models::genre(self.slug, value);
        }
        Ok(values)
    }

    async fn genre(&self, genre_id: &str) -> Result<GenreDetail> {
        let mut value = self.inner.genre(tag::untag(genre_id)).await?;
        models::genre_detail(self.slug, &mut value);
        Ok(value)
    }
}
