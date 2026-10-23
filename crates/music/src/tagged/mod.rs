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
    Album, AlbumDetail, Artist, ArtistCatalogue, ArtistProfile, Genre, GenreDetail, GenreSection,
    HomeFeed, MediaKind, MusicApi, Playlist, PlaylistDetail, SavedArtist, Track, TrackTags,
    UserDetail, UserProfile,
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

    async fn artist_catalogue(&self, artist_id: &str, known: &[Track]) -> Result<ArtistCatalogue> {
        let mut bare = known.to_vec();
        for value in &mut bare {
            models::bare::track(value);
        }
        let mut value = self
            .inner
            .artist_catalogue(tag::untag(artist_id), &bare)
            .await?;
        models::artist_catalogue(self.slug, &mut value);
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

    async fn saved_tracks(&self) -> Result<Vec<Track>> {
        let mut values = self.inner.saved_tracks().await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok(values)
    }

    fn has_all_tracks(&self) -> bool {
        self.inner.has_all_tracks()
    }

    async fn all_tracks(&self) -> Result<Vec<Track>> {
        let mut values = self.inner.all_tracks().await?;
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

    async fn canvas(&self, track_id: &str) -> Result<Option<String>> {
        self.inner.canvas(tag::untag(track_id)).await
    }

    async fn video(&self, track_id: &str, max_height: u32) -> Result<Option<crate::VideoFeed>> {
        self.inner.video(tag::untag(track_id), max_height).await
    }

    async fn has_video(&self, track_id: &str) -> Result<bool> {
        self.inner.has_video(tag::untag(track_id)).await
    }

    async fn waveform(&self, track_id: &str) -> Result<Option<crate::Waveform>> {
        self.inner.waveform(tag::untag(track_id)).await
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

    async fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut values = self.inner.playlists().await?;
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

    async fn saved_albums(&self) -> Result<Vec<Album>> {
        let mut values = self.inner.saved_albums().await?;
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

    async fn saved_artists(&self) -> Result<Vec<SavedArtist>> {
        let mut values = self.inner.saved_artists().await?;
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

    async fn track_radio(
        &self,
        track_id: &str,
        from: Option<&str>,
    ) -> Result<(Vec<Track>, Option<String>)> {
        let (mut values, next) = self.inner.track_radio(tag::untag(track_id), from).await?;
        for value in &mut values {
            models::track(self.slug, value);
        }
        Ok((values, next))
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

    async fn search_artists(&self, query: &str) -> Result<Vec<SavedArtist>> {
        let mut values = self.inner.search_artists(query).await?;
        for value in &mut values {
            models::saved_artist(self.slug, value);
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;

    /// A provider that answers only `artist_catalogue`, and remembers what it was asked.
    #[derive(Default)]
    struct Catalogued {
        asked: Mutex<Option<(String, Vec<String>)>>,
    }

    fn track(id: &str) -> Track {
        Track {
            id: Some(id.to_owned()),
            name: id.to_owned(),
            playable: true,
            artists: String::new(),
            artist_refs: Vec::new(),
            album: String::new(),
            album_id: None,
            cover: None,
            duration: Duration::from_secs(1),
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
        }
    }

    #[async_trait]
    impl MusicApi for Catalogued {
        fn share_url(&self, _kind: MediaKind, _id: &str) -> Option<String> {
            None
        }

        async fn profile(&self) -> Result<UserProfile> {
            unimplemented!()
        }
        async fn artist(&self, _: &str) -> Result<Artist> {
            unimplemented!()
        }
        async fn artist_profile(&self, _: &str) -> Result<ArtistProfile> {
            unimplemented!()
        }
        async fn artist_images(&self, _: Vec<String>) -> Result<HashMap<String, String>> {
            unimplemented!()
        }
        async fn saved_tracks(&self) -> Result<Vec<Track>> {
            unimplemented!()
        }
        async fn set_track_saved(&self, _: &str, _: bool) -> Result<()> {
            unimplemented!()
        }
        async fn track(&self, _: &str) -> Result<Track> {
            unimplemented!()
        }
        async fn track_playcount(&self, _: &str) -> Result<Option<u64>> {
            unimplemented!()
        }
        async fn playlists(&self) -> Result<Vec<Playlist>> {
            unimplemented!()
        }
        async fn create_playlist(&self, _: &str) -> Result<String> {
            unimplemented!()
        }
        async fn rename_playlist(&self, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn delete_playlist(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn remove_playlist_from_library(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn add_playlist_to_library(&self, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn set_playlist_public(&self, _: &str, _: bool) -> Result<()> {
            unimplemented!()
        }
        async fn add_track_to_playlist(&self, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn remove_track_from_playlist(&self, _: &str, _: &str) -> Result<()> {
            unimplemented!()
        }
        async fn saved_albums(&self) -> Result<Vec<Album>> {
            unimplemented!()
        }
        async fn set_album_saved(&self, _: &str, _: bool) -> Result<()> {
            unimplemented!()
        }
        async fn saved_artists(&self) -> Result<Vec<SavedArtist>> {
            unimplemented!()
        }
        async fn set_artist_saved(&self, _: &str, _: bool) -> Result<()> {
            unimplemented!()
        }
        async fn album(&self, _: &str) -> Result<AlbumDetail> {
            unimplemented!()
        }
        async fn album_tracks(&self, _: &str) -> Result<Vec<Track>> {
            unimplemented!()
        }
        async fn playlist(&self, _: &str) -> Result<PlaylistDetail> {
            unimplemented!()
        }
        async fn playlist_tracks(&self, _: &str) -> Result<Vec<Track>> {
            unimplemented!()
        }
        async fn playlist_covers(&self, _: &str, _: usize) -> Result<Vec<String>> {
            unimplemented!()
        }
        async fn track_radio(
            &self,
            _: &str,
            _: Option<&str>,
        ) -> Result<(Vec<Track>, Option<String>)> {
            unimplemented!()
        }
        async fn search(&self, _: &str) -> Result<Vec<Track>> {
            unimplemented!()
        }

        async fn artist_catalogue(
            &self,
            artist_id: &str,
            known: &[Track],
        ) -> Result<ArtistCatalogue> {
            let known = known.iter().filter_map(|track| track.id.clone()).collect();
            *self.asked.lock().unwrap() = Some((artist_id.to_owned(), known));
            Ok(ArtistCatalogue {
                top_tracks: vec![track("song")],
                ..Default::default()
            })
        }
    }

    #[tokio::test]
    async fn artist_catalogue_reaches_the_provider_with_bare_ids_and_comes_back_tagged() {
        let inner = Arc::new(Catalogued::default());
        let tagged = new("soundcloud", inner.clone());

        let catalogue = tagged
            .artist_catalogue(
                &tag::tag("soundcloud", "artist"),
                &[track(&tag::tag("soundcloud", "known"))],
            )
            .await
            .unwrap();

        assert_eq!(
            inner.asked.lock().unwrap().clone(),
            Some(("artist".to_owned(), vec!["known".to_owned()]))
        );
        assert_eq!(
            catalogue.top_tracks[0].id,
            Some(tag::tag("soundcloud", "song"))
        );
    }
}
