//! Read-only live checks that an artist page's catalogue reaches past the overview, as a guest.
//! No account is signed in to, nothing is written to any account, and SoundCloud keeps its
//! harvested client id in a throwaway cache rather than the app's.

use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use tokio::sync::mpsc::unbounded_channel;
use ytmusic::YtMusic;

use crate::soundcloud::SoundCloudProvider;
use crate::youtube::YouTubeClient;
use crate::{MusicApi, MusicProvider as _, SignIn};

/// Flume on SoundCloud, a public account with a few hundred uploads, more than one page.
const SOUNDCLOUD_ARTIST: &str = "2976616";
/// A song whose first credited artist has far more songs than the overview shows.
const YOUTUBE_MUSIC_SONG: &str = "radiohead creep";

/// The artist's whole catalogue holds more tracks than its overview, each listed once.
async fn reaches_past_the_overview(api: &dyn MusicApi, artist_id: &str) -> Result<()> {
    let overview = api.artist(artist_id).await?.top_tracks;
    let catalogue = api.artist_catalogue(artist_id, &overview).await?;
    let tracks = &catalogue.top_tracks;
    if tracks.len() <= overview.len() {
        bail!(
            "the catalogue of {artist_id} has {} tracks, no more than the overview's {}",
            tracks.len(),
            overview.len()
        );
    }
    let mut ids: Vec<_> = tracks.iter().filter_map(|track| track.id.clone()).collect();
    ids.sort();
    ids.dedup();
    if ids.len() != tracks.len() {
        bail!("the catalogue of {artist_id} lists a track twice");
    }
    eprintln!(
        "{artist_id}: overview {}, catalogue {}",
        overview.len(),
        tracks.len()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "reads a public soundcloud artist as a guest"]
async fn live_soundcloud_catalogue_reaches_past_the_first_page() -> Result<()> {
    let (_input, input) = unbounded_channel();
    let cache = std::env::temp_dir().join("sonora-live-artist-catalogue");
    let session = SoundCloudProvider::at(cache)
        .sign_in(SignIn::Anonymous, Arc::new(|_| {}), input)
        .await
        .context("cannot sign in to soundcloud as a guest")?;
    reaches_past_the_overview(session.api.as_ref(), SOUNDCLOUD_ARTIST).await
}

#[tokio::test]
#[ignore = "reads a public youtube music artist as a guest"]
async fn live_youtube_music_catalogue_opens_the_songs_playlist() -> Result<()> {
    let api = Arc::new(YtMusic::anonymous());
    let songs = api.search_songs(YOUTUBE_MUSIC_SONG).await?;
    let artist = songs
        .iter()
        .find_map(|song| song.artists.first()?.id.clone())
        .context("no song with a credited artist")?;
    reaches_past_the_overview(&YouTubeClient::new(api), &artist).await
}

#[cfg(feature = "youtube-video")]
#[tokio::test]
#[ignore = "reads a public youtube channel as a guest"]
async fn live_youtube_channel_catalogue_reaches_past_the_first_page() -> Result<()> {
    let client = crate::youtube_video::YouTubeVideoClient::new(Arc::new(YtMusic::anonymous()));
    let channel = client
        .search_artists("boiler room")
        .await?
        .into_iter()
        .next()
        .context("no channel found")?;
    reaches_past_the_overview(&client, &channel.id).await
}
