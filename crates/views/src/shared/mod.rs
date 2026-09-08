pub(crate) mod about;
pub(crate) mod adaptive;
pub(crate) mod album_grid;
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
pub(crate) mod visualizer;

use gpui::prelude::*;
use gpui::{App, Div, Pixels, SharedString, div, px, svg};
use i18n::t;
use router::{LOCAL, NavEntry};
use state::Session;
use ui::{ActiveTheme as _, Text};

const NOTE: Pixels = px(14.);

pub(crate) fn effects() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("SONORA_BLUR").as_deref() != Ok("0"))
}

pub(crate) fn firefox_note(cx: &App) -> Div {
    let theme = *cx.theme();
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_size(theme.text(Text::Small))
        .text_color(theme.muted_foreground)
        .child(
            svg()
                .path(icons::path("icons/firefoxbrowser.svg"))
                .size(NOTE)
                .flex_none()
                .text_color(theme.muted_foreground),
        )
        .child(t!("login-browser-firefox"))
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
        _ => "icons/music.svg",
    }
}

/// The Fluent keys for a provider's manual-paste sign-in flow.
///
/// `SignIn::Secret` means "paste the credential yourself", but the credential
/// differs: SoundCloud takes an OAuth token, the others take cookies. So does
/// every string that walks the user through finding it.
#[derive(Clone, Copy)]
pub(crate) struct Secret {
    pub label: &'static str,
    pub title: &'static str,
    pub steps: [&'static str; 4],
    pub note: &'static str,
    pub hint: &'static str,
}

const COOKIES: Secret = Secret {
    label: "login-connect-cookies",
    title: "login-cookie-title",
    steps: [
        "login-cookie-step-1",
        "login-cookie-step-2",
        "login-cookie-step-3",
        "login-cookie-step-4",
    ],
    note: "login-cookie-step-note",
    hint: "login-cookie-hint",
};

const TOKEN: Secret = Secret {
    label: "login-connect-token",
    title: "login-token-title",
    steps: [
        "login-token-step-1",
        "login-token-step-2",
        "login-token-step-3",
        "login-token-step-4",
    ],
    note: "login-token-step-note",
    hint: "login-token-hint",
};

pub(crate) fn secret(slug: &str) -> Secret {
    match slug {
        "soundcloud" => TOKEN,
        _ => COOKIES,
    }
}

pub(crate) fn secret_label(slug: &str) -> &'static str {
    secret(slug).label
}

#[cfg(test)]
mod tests {
    use super::secret_label;

    #[test]
    fn soundcloud_pastes_a_token() {
        assert_eq!(secret_label("soundcloud"), "login-connect-token");
    }

    #[test]
    fn other_providers_paste_cookies() {
        assert_eq!(secret_label("youtube"), "login-connect-cookies");
        assert_eq!(secret_label("spotify"), "login-connect-cookies");
        assert_eq!(secret_label("anything-else"), "login-connect-cookies");
    }
}
