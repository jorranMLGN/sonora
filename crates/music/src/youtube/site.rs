use std::sync::Arc;

use ytmusic::YtMusic;

use super::YouTubeClient;
use crate::MusicApi;

/// Builds the client a site talks through. The second argument is the signed-in owner's display
/// name, or nothing for a guest.
type MakeClient = fn(Arc<YtMusic>, Option<String>) -> Arc<dyn MusicApi>;

/// Which YouTube a provider signs in to and talks to. Sign-in, cookie rotation, PO tokens and
/// playback are shared, so a site only names itself and supplies its client.
pub(crate) struct Site {
    /// The display name of the provider.
    pub name: &'static str,
    /// The provider's slug, which also names its credentials folder and tags its ids.
    pub slug: &'static str,
    /// The host the provider reaches, shown to the user and probed when offline.
    pub reach: &'static str,
    /// Google's sign-in page, told to come back to this site once the account is in.
    pub sign_in_url: &'static str,
    /// The host the sign-in window lands on once the account is in.
    pub landing: &'static str,
    /// The profile name of a guest session.
    pub guest_name: &'static str,
    /// Wraps a session's `YtMusic` in this site's `MusicApi`.
    pub client: MakeClient,
}

/// YouTube Music, whose sign-in url is the one its own Sign in button opens, with the
/// `www.youtube.com/signin` hop included so the second load the sign-in window falls back on runs
/// exactly what that button would.
pub(crate) const MUSIC: Site = Site {
    name: "YouTube Music",
    slug: "youtube",
    reach: "music.youtube.com",
    sign_in_url: "https://accounts.google.com/ServiceLogin?ltmpl=music&service=youtube&passive=true&continue=https%3A%2F%2Fwww.youtube.com%2Fsignin%3Faction_handle_signin%3Dtrue%26next%3Dhttps%253A%252F%252Fmusic.youtube.com%252F",
    landing: "music.youtube.com",
    guest_name: "YouTube Music",
    client: music_client,
};

fn music_client(api: Arc<YtMusic>, owner: Option<String>) -> Arc<dyn MusicApi> {
    let client = YouTubeClient::new(api);
    Arc::new(match owner {
        Some(owner) => client.owned_by(owner),
        None => client,
    })
}
