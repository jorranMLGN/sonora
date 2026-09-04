use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::http::Http;
use super::{library, search};
use crate::{
    Album, AlbumDetail, Artist, ArtistProfile, MediaKind, MusicApi, Playlist, PlaylistDetail,
    SavedArtist, Track, UserProfile,
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
    fn share_url(&self, _kind: MediaKind, _id: &str) -> Option<String> {
        None
    }

    async fn profile(&self) -> Result<UserProfile> {
        anyhow::bail!("not implemented yet")
    }

    async fn artist(&self, _artist_id: &str) -> Result<Artist> {
        anyhow::bail!("not implemented yet")
    }

    async fn artist_profile(&self, _artist_id: &str) -> Result<ArtistProfile> {
        anyhow::bail!("not implemented yet")
    }

    async fn artist_images(&self, _ids: Vec<String>) -> Result<HashMap<String, String>> {
        anyhow::bail!("not implemented yet")
    }

    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        let user = self.user_id()?;
        library::liked_tracks(&self.http, user, limit).await
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        library::set_track_liked(&self.http, track_id, saved).await
    }

    async fn track(&self, _track_id: &str) -> Result<Track> {
        anyhow::bail!("not implemented yet")
    }

    async fn track_playcount(&self, _track_id: &str) -> Result<Option<u64>> {
        anyhow::bail!("not implemented yet")
    }

    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>> {
        let user = self.user_id()?;
        let (playlists, _albums) = library::saved_sets(&self.http, user, limit).await?;
        Ok(playlists)
    }

    async fn create_playlist(&self, _name: &str) -> Result<String> {
        anyhow::bail!("not implemented yet")
    }

    async fn rename_playlist(&self, _playlist_id: &str, _name: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn delete_playlist(&self, _playlist_id: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn remove_playlist_from_library(&self, _playlist_id: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn add_playlist_to_library(&self, _playlist_id: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn set_playlist_public(&self, _playlist_id: &str, _public: bool) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn add_track_to_playlist(&self, _playlist_id: &str, _track_id: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn remove_track_from_playlist(&self, _playlist_id: &str, _track_id: &str) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>> {
        let user = self.user_id()?;
        let (_playlists, albums) = library::saved_sets(&self.http, user, limit).await?;
        Ok(albums)
    }

    async fn set_album_saved(&self, album_id: &str, saved: bool) -> Result<()> {
        library::set_saved(&self.http, album_id, saved).await
    }

    async fn saved_artists(&self, limit: u32) -> Result<Vec<SavedArtist>> {
        let user = self.user_id()?;
        library::followed(&self.http, user, limit).await
    }

    async fn set_artist_saved(&self, artist_id: &str, saved: bool) -> Result<()> {
        library::set_followed(&self.http, artist_id, saved).await
    }

    async fn album(&self, _album_id: &str) -> Result<AlbumDetail> {
        anyhow::bail!("not implemented yet")
    }

    async fn album_tracks(&self, _album_id: &str) -> Result<Vec<Track>> {
        anyhow::bail!("not implemented yet")
    }

    async fn playlist(&self, _playlist_id: &str) -> Result<PlaylistDetail> {
        anyhow::bail!("not implemented yet")
    }

    async fn playlist_tracks(&self, _playlist_id: &str) -> Result<Vec<Track>> {
        anyhow::bail!("not implemented yet")
    }

    async fn playlist_covers(&self, _playlist_id: &str, _wanted: usize) -> Result<Vec<String>> {
        anyhow::bail!("not implemented yet")
    }

    async fn track_radio(&self, _track_id: &str) -> Result<Vec<Track>> {
        anyhow::bail!("not implemented yet")
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
