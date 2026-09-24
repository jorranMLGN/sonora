use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;

use super::http::Http;
use super::{GUEST_ID, fetch_profile, library, playlists, search, users};
use crate::{
    Album, AlbumDetail, Artist, ArtistProfile, MediaKind, MusicApi, Playlist, PlaylistDetail,
    SavedArtist, Track, UserDetail, UserProfile,
};

pub struct SoundCloudClient {
    http: Arc<Http>,
    user: Option<String>,
}

impl SoundCloudClient {
    pub fn new(http: Arc<Http>) -> Self {
        Self { http, user: None }
    }

    pub fn as_user(mut self, id: String) -> Self {
        self.user = Some(id);
        self
    }

    /// The signed-in user's id, or a clear error for a guest session.
    fn user_id(&self) -> Result<&str> {
        self.user
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("signing in is required to read the library"))
    }
}

#[async_trait]
impl MusicApi for SoundCloudClient {
    fn share_url(&self, _kind: MediaKind, id: &str) -> Option<String> {
        self.http.permalink(id)
    }

    async fn profile(&self) -> Result<UserProfile> {
        if self.http.authenticated() {
            fetch_profile(&self.http).await
        } else {
            Ok(UserProfile {
                id: GUEST_ID.to_string(),
                display_name: "SoundCloud".to_string(),
            })
        }
    }

    async fn user(&self, user_id: &str) -> Result<UserDetail> {
        users::detail(&self.http, user_id).await
    }

    async fn artist(&self, artist_id: &str) -> Result<Artist> {
        users::artist(&self.http, artist_id).await
    }

    async fn artist_profile(&self, artist_id: &str) -> Result<ArtistProfile> {
        users::profile(&self.http, artist_id).await
    }

    async fn artist_images(&self, ids: Vec<String>) -> Result<HashMap<String, String>> {
        users::images(&self.http, ids).await
    }

    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let user = self.user_id()?;
        library::liked_tracks(&self.http, user, limit).await
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        let user = self.user_id()?;
        library::set_track_liked(&self.http, user, track_id, saved).await
    }

    async fn track(&self, track_id: &str) -> Result<Track> {
        users::track(&self.http, track_id).await
    }

    async fn track_playcount(&self, track_id: &str) -> Result<Option<u64>> {
        users::track_playcount(&self.http, track_id).await
    }

    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>> {
        let user = self.user_id()?;
        let (playlists, _albums) = library::saved_sets(&self.http, user, limit).await?;
        Ok(playlists)
    }

    async fn create_playlist(&self, name: &str) -> Result<String> {
        playlists::create(&self.http, name).await
    }

    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()> {
        playlists::rename(&self.http, playlist_id, name).await
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<()> {
        playlists::delete(&self.http, playlist_id).await
    }

    async fn remove_playlist_from_library(&self, playlist_id: &str) -> Result<()> {
        let user = self.user_id()?;
        library::set_saved(&self.http, user, playlist_id, false).await
    }

    async fn add_playlist_to_library(&self, playlist_id: &str) -> Result<()> {
        let user = self.user_id()?;
        library::set_saved(&self.http, user, playlist_id, true).await
    }

    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()> {
        playlists::set_public(&self.http, playlist_id, public).await
    }

    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        let track_id: u64 = track_id
            .parse()
            .context("cannot parse the soundcloud track id")?;
        playlists::add_track(&self.http, playlist_id, track_id).await
    }

    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        let track_id: u64 = track_id
            .parse()
            .context("cannot parse the soundcloud track id")?;
        playlists::remove_track(&self.http, playlist_id, track_id).await
    }

    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>> {
        let user = self.user_id()?;
        let (_playlists, albums) = library::saved_sets(&self.http, user, limit).await?;
        Ok(albums)
    }

    async fn set_album_saved(&self, album_id: &str, saved: bool) -> Result<()> {
        let user = self.user_id()?;
        library::set_saved(&self.http, user, album_id, saved).await
    }

    async fn saved_artists(&self, limit: u32) -> Result<Vec<SavedArtist>> {
        let user = self.user_id()?;
        library::followed(&self.http, user, limit).await
    }

    async fn set_artist_saved(&self, artist_id: &str, saved: bool) -> Result<()> {
        library::set_followed(&self.http, artist_id, saved).await
    }

    async fn album(&self, album_id: &str) -> Result<AlbumDetail> {
        playlists::album(&self.http, album_id).await
    }

    async fn album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        playlists::album_tracks(&self.http, album_id).await
    }

    async fn playlist(&self, playlist_id: &str) -> Result<PlaylistDetail> {
        playlists::detail(&self.http, playlist_id).await
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        playlists::tracks(&self.http, playlist_id).await
    }

    async fn playlist_covers(&self, playlist_id: &str, wanted: usize) -> Result<Vec<String>> {
        playlists::playlist_covers(&self.http, playlist_id, wanted).await
    }

    async fn track_radio(&self, track_id: &str) -> Result<Vec<Track>> {
        users::related(&self.http, track_id).await
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>> {
        search::tracks(&self.http, query).await
    }

    async fn search_albums(&self, query: &str) -> Result<Vec<Album>> {
        search::albums(&self.http, query).await
    }

    async fn search_playlists(&self, query: &str) -> Result<Vec<Playlist>> {
        search::playlists(&self.http, query).await
    }
}
