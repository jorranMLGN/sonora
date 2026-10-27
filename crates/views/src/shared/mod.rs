pub(crate) mod about;
pub(crate) mod adaptive;
pub(crate) mod album_grid;
pub(crate) mod ambient;
pub(crate) mod canvas;
pub(crate) mod cards;
pub(crate) mod cells;
pub(crate) mod codec;
pub(crate) mod confirm;
pub(crate) mod device_code;
pub(crate) mod equalizer;
pub(crate) mod hero;
pub(crate) mod local;
pub(crate) mod menus;
pub(crate) mod nav;
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
pub(crate) mod visualizer;
pub(crate) mod widevine;

use gpui::{App, SharedString};
use router::{LOCAL, NavEntry};
use state::{Session, Sonora};

pub(crate) use ui::effects;

/// The brand colours of the providers' marks.
const SPOTIFY: u32 = 0x1ed760;
const YOUTUBE: u32 = 0xff0033;
const DEEZER: u32 = 0xa238ff;
const APPLE: u32 = 0xfa2d48;
const SUBSONIC: u32 = 0xffa31a;
const SOUNDCLOUD: u32 = 0xff5500;

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
        "youtube-video" => "icons/youtube.svg",
        LOCAL => "icons/file-music.svg",
        "subsonic" => "icons/subsonic.svg",
        "deezer" => "icons/deezer.svg",
        "apple" => "icons/applemusic.svg",
        "tidal" => "icons/tidal.svg",
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

/// The brand colour of the provider `slug`'s mark. Tidal's mark is plain black or white, so it
/// takes the foreground, as does any provider without a colour of its own.
pub(crate) fn provider_brand(slug: &str, cx: &gpui::App) -> gpui::Hsla {
    match slug {
        "spotify" => gpui::rgb(SPOTIFY).into(),
        "youtube" | "youtube-video" => gpui::rgb(YOUTUBE).into(),
        "soundcloud" => gpui::rgb(SOUNDCLOUD).into(),
        "deezer" => gpui::rgb(DEEZER).into(),
        "apple" => gpui::rgb(APPLE).into(),
        "subsonic" => gpui::rgb(SUBSONIC).into(),
        _ => ui::ActiveTheme::theme(cx).foreground,
    }
}
