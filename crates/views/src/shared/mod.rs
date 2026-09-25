pub(crate) mod about;
pub(crate) mod adaptive;
pub(crate) mod album_grid;
pub(crate) mod ambient;
pub(crate) mod cards;
pub(crate) mod cells;
pub(crate) mod confirm;
pub(crate) mod hero;
pub(crate) mod local;
pub(crate) mod menus;
pub(crate) mod page;
pub(crate) mod picks;
pub(crate) mod pins;
pub(crate) mod playlist_editor;
pub(crate) mod popups;
pub(crate) mod shelves;
pub(crate) mod steps;
pub(crate) mod tag_editor;
pub(crate) mod text;
pub(crate) mod track_card;
pub(crate) mod tracks;
pub(crate) mod transport;
pub(crate) mod trouble;
pub(crate) mod veil;
pub(crate) mod visualizer;
pub(crate) mod widevine;

use gpui::{App, SharedString};
use router::{LOCAL, NavEntry};
use state::{Session, Sonora};

pub(crate) fn effects() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("SONORA_BLUR").as_deref() != Ok("0"))
}

pub(crate) fn nav_label(entry: NavEntry, session: &Session) -> SharedString {
    match entry {
        NavEntry::Library(slug) if slug != LOCAL => session
            .provider_name_for(slug)
            .map_or_else(|| i18n::lookup(entry.key(), None), SharedString::from),
        _ => i18n::lookup(entry.key(), None),
    }
}

pub(crate) fn provider_logo(slug: &str) -> &'static str {
    match slug {
        "soundcloud" => "icons/soundcloud.svg",
        "spotify" => "icons/spotify.svg",
        "youtube" => "icons/youtubemusic.svg",
        LOCAL => "icons/file-music.svg",
        "subsonic" => "icons/subsonic.svg",
        "deezer" => "icons/deezer.svg",
        "apple" => "icons/applemusic.svg",
        _ => "icons/music.svg",
    }
}

pub(crate) fn provider_mark(id: &str, cx: &App) -> Option<&'static str> {
    let slug = Sonora::global(cx).session.read(cx).slug_for(id)?;

    provider_mark_of(slug, cx)
}

pub(crate) fn provider_mark_of(slug: &str, cx: &App) -> Option<&'static str> {
    let session = Sonora::global(cx).session.read(cx);

    match session.active_slugs().len() < 2 {
        true => None,
        false => Some(provider_logo(slug)),
    }
}
