use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use super::http::Http;
use super::search;
use crate::{
    Album, AlbumDetail, Artist, ArtistProfile, MediaKind, MusicApi, Playlist, PlaylistDetail,
    SavedArtist, Track, UserProfile,
};

#[allow(dead_code)]
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

    async fn saved_tracks(&self, _limit: u32) -> Result<Vec<Track>> {
        anyhow::bail!("not implemented yet")
    }

    async fn set_track_saved(&self, _track_id: &str, _saved: bool) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn track(&self, _track_id: &str) -> Result<Track> {
        anyhow::bail!("not implemented yet")
    }

    async fn track_playcount(&self, _track_id: &str) -> Result<Option<u64>> {
        anyhow::bail!("not implemented yet")
    }

    async fn playlists(&self, _limit: u32) -> Result<Vec<Playlist>> {
        anyhow::bail!("not implemented yet")
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

    async fn saved_albums(&self, _limit: u32) -> Result<Vec<Album>> {
        anyhow::bail!("not implemented yet")
    }

    async fn set_album_saved(&self, _album_id: &str, _saved: bool) -> Result<()> {
        anyhow::bail!("not implemented yet")
    }

    async fn saved_artists(&self, _limit: u32) -> Result<Vec<SavedArtist>> {
        anyhow::bail!("not implemented yet")
    }

    async fn set_artist_saved(&self, _artist_id: &str, _saved: bool) -> Result<()> {
        anyhow::bail!("not implemented yet")
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
