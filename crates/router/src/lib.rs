mod link;
mod navigation;
mod uri;

pub use link::Link;
pub use navigation::{Navigation, NavigationEvent};
pub use uri::destination;

use gpui::{App, AppContext as _, Entity, Global, SharedString};

pub const LOCAL: &str = "local";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryTab {
    Songs,
    Favorites,
    Albums,
    Playlists,
    Artists,
}

impl LibraryTab {
    pub const ALL: [Self; 5] = [
        Self::Favorites,
        Self::Songs,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
    ];

    pub fn tabs(all_tracks: bool) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|tab| all_tracks || *tab != Self::Songs)
            .collect()
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Songs => "nav-songs",
            Self::Favorites => "nav-favorites",
            Self::Albums => "nav-albums",
            Self::Artists => "nav-artists",
            Self::Playlists => "nav-playlists",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavEntry {
    Home,
    Search,
    History,
    Library(&'static str),
}

impl NavEntry {
    pub fn entries(slugs: &[&'static str]) -> Vec<Self> {
        let mut entries = vec![Self::Home, Self::Search, Self::History];
        entries.extend(slugs.iter().copied().map(Self::Library));
        entries
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Search => "search",
            Self::History => "history",
            Self::Library(slug) => slug,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Search => "nav-search",
            Self::History => "nav-history",
            Self::Library(slug) if slug == LOCAL => "nav-local",
            Self::Library(_) => "nav-library",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Search,
    History,
    Songs,
    Albums,
    Playlists,
    Artists,
    Imported,
}

impl Screen {
    pub const ALL: [Self; 8] = [
        Self::Home,
        Self::Search,
        Self::Songs,
        Self::Albums,
        Self::Artists,
        Self::Playlists,
        Self::Imported,
        Self::History,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Search => "search",
            Self::History => "history",
            Self::Songs => "songs",
            Self::Albums => "albums",
            Self::Playlists => "playlists",
            Self::Artists => "artists",
            Self::Imported => "imported",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Home => "nav-home",
            Self::Search => "nav-search",
            Self::History => "nav-history",
            Self::Songs => "nav-favorites",
            Self::Albums => "nav-albums",
            Self::Playlists => "nav-playlists",
            Self::Artists => "nav-artists",
            Self::Imported => "nav-local",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let name = music::tag::split(id).map_or(id, |(_, name)| name);
        Self::ALL.into_iter().find(|screen| screen.id() == name)
    }

    pub fn stored(self, slug: Option<&str>) -> String {
        match (self.follows_provider(), slug) {
            (true, Some(slug)) => format!("{slug}:{}", self.id()),
            _ => self.id().to_owned(),
        }
    }

    pub fn destination(self, slug: &'static str) -> Destination {
        match self {
            Self::Home => Destination::Home,
            Self::Search => Destination::Search,
            Self::History => Destination::History,
            Self::Songs => Destination::Library(slug, LibraryTab::Favorites),
            Self::Albums => Destination::Library(slug, LibraryTab::Albums),
            Self::Playlists => Destination::Library(slug, LibraryTab::Playlists),
            Self::Artists => Destination::Library(slug, LibraryTab::Artists),
            Self::Imported => Destination::Library(LOCAL, LibraryTab::Songs),
        }
    }

    fn follows_provider(self) -> bool {
        matches!(
            self,
            Self::Songs | Self::Albums | Self::Playlists | Self::Artists
        )
    }
}

pub fn startup(stored: &str, remembered: &str, slugs: &[&'static str]) -> Destination {
    let known = |slug: &str| music::tag::SLUGS.into_iter().find(|found| *found == slug);
    let slug = music::tag::slug_of(stored)
        .and_then(known)
        .filter(|slug| *slug == remembered || slugs.contains(slug))
        .or_else(|| known(remembered))
        .or_else(|| slugs.first().copied())
        .unwrap_or(music::tag::SLUGS[0]);
    Screen::from_id(stored)
        .unwrap_or(Screen::Home)
        .destination(slug)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Appearance,
    Playback,
    Privacy,
    About,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Home,
    History,
    Library(&'static str, LibraryTab),
    Album(SharedString),
    Song(SharedString),
    Playlist(SharedString),
    Artist(SharedString),
    User(SharedString),
    Genre(SharedString),
    Search,
    Settings(SettingsTab),
    Fullscreen,
}

impl From<&ui::Pin> for Destination {
    fn from(pin: &ui::Pin) -> Self {
        let id = SharedString::from(pin.id.clone());
        match pin.kind {
            ui::PinKind::Album => Destination::Album(id),
            ui::PinKind::Artist => Destination::Artist(id),
            ui::PinKind::Playlist => Destination::Playlist(id),
            ui::PinKind::Song => Destination::Song(id),
        }
    }
}

impl Destination {
    pub fn same_section(&self, other: &Destination) -> bool {
        match (self, other) {
            (Destination::Library(here, _), Destination::Library(there, _)) => here == there,
            (Destination::Settings(_), Destination::Settings(_)) => true,
            _ => self == other,
        }
    }
}

#[derive(Clone)]
struct Router(Entity<Navigation>);

impl Global for Router {}

pub fn init(start: Destination, cx: &mut App) {
    let navigation = cx.new(|_| Navigation::new(start));
    cx.set_global(Router(navigation));
}

pub fn trail(cx: &App) -> Entity<Navigation> {
    cx.global::<Router>().0.clone()
}

pub fn navigate(destination: Destination, cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.go(destination, cx));
}

pub fn back(cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.back(cx));
}

pub fn forward(cx: &mut App) {
    trail(cx).update(cx, |navigation, cx| navigation.forward(cx));
}
