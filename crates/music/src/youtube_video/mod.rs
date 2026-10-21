//! Plain youtube.com: videos as tracks, channels as artists, playlists as playlists. Sign-in,
//! cookie rotation, PO tokens and playback are YouTube Music's, reached through its `Site`.

mod client;
mod wire;

use std::sync::Arc;

use ytmusic::YtMusic;

use crate::youtube::{Site, YouTubeProvider};
use crate::{MusicApi, credentials};
pub use client::YouTubeVideoClient;

/// youtube.com, whose sign-in url comes back to youtube.com through the same
/// `www.youtube.com/signin` hop its own Sign in button takes.
pub(crate) const VIDEO: Site = Site {
    name: "YouTube",
    slug: "youtube-video",
    reach: "www.youtube.com",
    sign_in_url: "https://accounts.google.com/ServiceLogin?service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26next%3Dhttps%253A%252F%252Fwww.youtube.com%252F",
    landing: "www.youtube.com",
    guest_name: "YouTube",
    client: video_client,
};

/// The plain-YouTube provider, with credentials of its own beside YouTube Music's.
pub fn provider() -> YouTubeProvider {
    YouTubeProvider::for_site(&VIDEO, credentials::dir(VIDEO.slug))
}

fn video_client(api: Arc<YtMusic>, owner: Option<String>) -> Arc<dyn MusicApi> {
    let client = YouTubeVideoClient::new(api);
    Arc::new(match owner {
        Some(owner) => client.owned_by(owner),
        None => client,
    })
}
