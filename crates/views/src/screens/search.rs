use std::cell::Cell;
use std::rc::Rc;

use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, ElementId, Entity, FocusHandle, Focusable, FontWeight,
    MouseButton, MouseDownEvent, Pixels, Point, Render, ScrollHandle, SharedString, WeakEntity,
    Window, div,
};
use i18n::t;
use input::{SEARCH_CONTEXT, SearchAlbums, SearchAll, SearchArtists, SearchPlaylists, SearchSongs};
use music::{Album, Playlist, SavedArtist, Track};
use router::{Destination, NavEntry, navigate};
use ui::Input;

use crate::chrome::Chrome;
use crate::chrome::tools::{self, Sliders};
use crate::shared::menus::{ItemMenu, album_menu, artist_menu, playlist_menu};
use state::{
    AlbumHit, ArtistHit, Axis, Found, Genres, Hit, Kind, Lens, Network, Playback, PlaylistHit,
    Search, SearchHistory, Sonora,
};
use ui::ActiveTheme as _;
use ui::{
    Activate, Button, Card, Deck, Deselect, Dismiss, Dropdown, Filter, FilterChange, FlagAxis,
    Pinnable, Popovers, Popup, Room, Scrollbar, Scroller, SelectLeft, SelectNext, SelectPrevious,
    SelectRight, Separator, SortAxis, Submit, TabBar, Text, Theme, VAST, clock, eyebrow, scrolled,
    snapped, tabular, vacant,
};

use crate::shared::cards;
use crate::shared::cells;
use crate::shared::nav_label;
use crate::shared::pins::Pinned as _;
use crate::shared::shelves;
use crate::shared::trouble;

const RAIL: Pixels = gpui::px(12.);
const LANE: Pixels = gpui::px(246.);
/// How tall the band is where results fade out as they scroll up under what sits above them.
const FADE: Pixels = gpui::px(12.);
/// How many past searches the panel lists.
const PAST: usize = 8;
/// How many songs the panel suggests.
const SUGGESTED: usize = 6;
/// How many placeholder rows stand in while the first suggestions load.
const SUGGESTED_WAITING: usize = 3;
const ROW_GAP: f32 = 0.25;
const SONGS: &[Kind] = &[Kind::Song];
const ARTISTS: &[Kind] = &[Kind::Artist];
const RELEASES: &[Kind] = &[Kind::Album, Kind::Playlist];
const GROUPS: [&[Kind]; 3] = [SONGS, ARTISTS, RELEASES];
/// The type tabs, in order, with their label key.
const TABS: [(Option<Kind>, &str); 5] = [
    (None, "search-all"),
    (Some(Kind::Song), "search-songs"),
    (Some(Kind::Artist), "search-artists"),
    (Some(Kind::Album), "search-albums"),
    (Some(Kind::Playlist), "search-playlists"),
];
/// The filter key of "Only in your library"; provider filters are keyed by their slug.
const MINE: &str = "mine";

enum Seat {
    Head(&'static str),
    Item { at: usize, compact: bool },
}

impl Seat {
    fn at(&self) -> Option<usize> {
        match self {
            Seat::Head(_) => None,
            Seat::Item { at, .. } => Some(*at),
        }
    }
}

fn group_key(group: &[Kind]) -> &'static str {
    match group.first() {
        Some(Kind::Song) => "search-songs",
        Some(Kind::Artist) => "search-artists",
        _ => "search-albums-playlists",
    }
}
use crate::shared::tracks::{PlaybackStatus, playback_status};

type Play = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

enum Press {
    Song(Box<Track>),
    Artist(String),
    Album(String),
    Playlist(String),
}

#[derive(Clone)]
enum HitMenu {
    Song(Box<Track>),
    Album(AlbumHit),
    Playlist(PlaylistHit),
    Artist(ArtistHit),
}

impl HitMenu {
    fn of(hit: &Hit) -> Option<Self> {
        match hit {
            Hit::Song(track) => Some(Self::Song(Box::new(track.clone()))),
            Hit::Album(album) => Some(Self::Album(album.clone())),
            Hit::Playlist(list) => Some(Self::Playlist(list.clone())),
            Hit::Artist(artist) if artist.id.is_some() => Some(Self::Artist(artist.clone())),
            Hit::Artist(_) => None,
        }
    }
}

/// One row of the panel under the search field: a past query while the field is empty, a
/// suggested hit of any kind while something is typed.
#[derive(Clone)]
enum Offer {
    Past(String),
    Hit(Box<Found>),
}

impl Offer {
    /// What tells one row from another when the suggestions fill in under it.
    fn key(&self) -> String {
        match self {
            Offer::Past(query) => format!("past:{query}"),
            Offer::Hit(found) => match &found.hit {
                Hit::Song(track) => {
                    format!("song:{}", track.id.as_deref().unwrap_or(&track.name))
                }
                Hit::Artist(artist) => {
                    format!("artist:{}", artist.id.as_deref().unwrap_or(&artist.name))
                }
                Hit::Album(album) => format!("album:{}", album.id),
                Hit::Playlist(list) => format!("playlist:{}", list.id),
            },
        }
    }
}

/// The search page. Typing asks `suggestions`, whose best hits show in a panel under the field,
/// and only Enter or the Search button asks `search` for the full results the page lists.
pub(crate) struct SearchView {
    input: Entity<Input>,
    search: Entity<Search>,
    suggestions: Entity<Search>,
    /// The text the field last held, so a change of caret or selection is not taken for typing.
    typed: String,
    /// Whether the panel under the field was put away, by Escape, a press outside or a submit.
    /// Typing or focusing the field again brings it back.
    dismissed: bool,
    /// Keeps the next focus of the field from reopening the panel, for a focus the page moves
    /// there itself.
    hush: bool,
    /// The panel row the arrow keys are on.
    pick: Option<usize>,
    /// The `key` of the row `pick` is on, so the pick stays with it as the suggestions reorder.
    picked: Option<String>,
    /// Whether the arrow keys are on the best match, the stop between the field and the lists.
    on_best: bool,
    genres: Entity<Genres>,
    history: Entity<SearchHistory>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    bars: Vec<(&'static str, Entity<Scrollbar>)>,
    mixed: Entity<Scrollbar>,
    browsing: Entity<Scrollbar>,
    track_menu: ItemMenu,
    context_menu: Option<(HitMenu, Point<Pixels>)>,
    focus: FocusHandle,
    cursor: Option<(usize, usize)>,
    rows: Vec<usize>,
    lead: Rc<Cell<Pixels>>,
    popovers: Popovers,
    sliders: Sliders,
}

impl SearchView {
    pub(crate) fn new(
        search: Entity<Search>,
        suggestions: Entity<Search>,
        genres: Entity<Genres>,
        playback: Entity<Playback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&genres, |_, _, cx| cx.notify()).detach();
        let input = cx.new(|cx| {
            Input::new("search-placeholder", cx)
                .icon("icons/search.svg")
                .clearable()
        });

        cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text().to_owned();
            if query == this.typed {
                return;
            }
            this.typed = query.clone();
            this.dismissed = false;
            this.pick = None;
            this.suggestions
                .update(cx, |suggestions, cx| suggestions.ask(&query, cx));
            // An emptied field goes back to browsing rather than keeping the last results.
            if query.trim().is_empty() {
                this.search.update(cx, |search, cx| search.ask("", cx));
            }
            cx.notify();
        })
        .detach();
        cx.observe(&suggestions, |this, _, cx| {
            this.keep_pick(cx);
            cx.notify();
        })
        .detach();
        let field = input.read(cx).focus_handle(cx);
        cx.on_focus(&field, window, |this, _, cx| {
            this.dismissed = std::mem::take(&mut this.hush);
            this.pick = None;
            cx.notify();
        })
        .detach();
        cx.on_blur(&field, window, |_, _, cx| cx.notify()).detach();

        cx.observe(&search, |this, _, cx| {
            this.track_menu.reset(cx);
            this.context_menu = None;
            this.cursor = None;
            this.on_best = false;
            this.rows.clear();
            cx.notify();
        })
        .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();
        let history = Sonora::global(cx).search_history.clone();
        cx.observe(&history, |_, _, cx| cx.notify()).detach();
        let library = Sonora::global(cx).library.clone();
        cx.observe(&library, |_, _, cx| cx.notify()).detach();
        let current_playback = playback_status(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status != current {
                this.playback_status = current;
                cx.notify();
            }
        })
        .detach();

        let asked = input.read(cx).text().to_owned();
        search.update(cx, |search, cx| search.ask(&asked, cx));

        let me = cx.entity_id();
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset().watching(me));
        let registered = Sonora::global(cx).session.read(cx).registered_slugs();

        Self {
            input,
            search,
            suggestions,
            typed: String::new(),
            dismissed: false,
            hush: false,
            pick: None,
            picked: None,
            on_best: false,
            genres,
            history,
            playback,
            playback_status: current_playback,
            bars: registered
                .into_iter()
                .map(|slug| {
                    (
                        slug,
                        cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
                    )
                })
                .collect(),
            mixed: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
            browsing: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
            track_menu: ItemMenu::new(playlist_scrollbar, cx),
            context_menu: None,
            focus: cx.focus_handle(),
            cursor: None,
            rows: Vec::new(),
            lead: Rc::new(Cell::new(Pixels::ZERO)),
            popovers: Popovers::default(),
            sliders: Sliders::default(),
        }
    }

    /// Puts the keyboard in the field when the page opens, without the panel, so the categories
    /// stay in view until the listener clicks the field, presses Down or starts typing.
    pub(crate) fn focus(&mut self, window: &mut Window, cx: &mut App) {
        if !self.typing(window, cx) {
            self.hush = true;
        }
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Keeps the query the results answer in the recent searches, once the user acted on it.
    fn remember_query(&self, cx: &mut App) {
        let query = self.search.read(cx).query().to_owned();
        remember(&query, cx);
    }

    /// Moves the keyboard back to the field without opening the panel, for the arrow keys and
    /// Escape coming back up out of the results.
    fn return_to_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hush = true;
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn typing(&self, window: &Window, cx: &App) -> bool {
        self.input.read(cx).focus_handle(cx).is_focused(window)
    }

    /// The rows of the panel: past queries while the field is empty, the best suggested hits
    /// once something is typed.
    fn offers(&self, cx: &App) -> Vec<Offer> {
        if self.typed.trim().is_empty() {
            return self
                .history
                .read(cx)
                .entries()
                .iter()
                .take(PAST)
                .cloned()
                .map(Offer::Past)
                .collect();
        }
        self.suggestions
            .read(cx)
            .hits()
            .iter()
            .take(SUGGESTED)
            .map(|found| Offer::Hit(Box::new(found.clone())))
            .collect()
    }

    /// Whether the panel shows: the field has the keyboard, nothing put the panel away, and it
    /// has rows or is still waiting on the first suggestions.
    fn panel_open(&self, window: &Window, cx: &App) -> bool {
        if self.dismissed || !self.typing(window, cx) {
            return false;
        }
        let waiting = !self.typed.trim().is_empty() && self.suggestions.read(cx).is_loading();
        waiting || !self.offers(cx).is_empty()
    }

    /// Puts the arrow keys on panel row `at` and notes which row that is.
    fn choose(&mut self, at: Option<usize>, cx: &App) {
        self.pick = at;
        self.picked = at.and_then(|at| self.offers(cx).get(at).map(Offer::key));
    }

    /// Keeps the pick on the row it was on after the suggestions changed, or on the same place
    /// when that row is gone.
    fn keep_pick(&mut self, cx: &App) {
        let Some(at) = self.pick else {
            return;
        };
        let offers = self.offers(cx);
        let found = self
            .picked
            .as_ref()
            .and_then(|key| offers.iter().position(|offer| offer.key() == *key));
        let kept = found.or_else(|| offers.len().checked_sub(1).map(|last| at.min(last)));
        self.choose(kept, cx);
    }

    fn close_panel(&mut self, cx: &mut Context<Self>) {
        self.dismissed = true;
        self.pick = None;
        cx.notify();
    }

    /// Runs `query` as a full search, files it in the history and puts the panel away.
    fn run(&mut self, query: &str, cx: &mut Context<Self>) {
        let query = query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        remember(&query, cx);
        self.search.update(cx, |search, cx| search.ask(&query, cx));
        self.close_panel(cx);
    }

    /// Enter in the field: the panel row the arrow keys are on, or else a full search for what
    /// is typed.
    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        let picked = self
            .pick
            .filter(|_| self.panel_open(window, cx))
            .and_then(|at| self.offers(cx).into_iter().nth(at));
        match picked {
            Some(offer) => self.take(offer, cx),
            None => {
                let query = self.typed.clone();
                self.run(&query, cx);
            }
        }
    }

    /// Acts on a panel row. A past query fills the field and runs again, and a hit opens the way
    /// it does in the results, which files what was typed in the history as a search would.
    fn take(&mut self, offer: Offer, cx: &mut Context<Self>) {
        match offer {
            Offer::Past(query) => {
                // Set ahead of the field, so its observer does not take this for typing and
                // open the panel again.
                self.typed = query.clone();
                self.input
                    .update(cx, |input, cx| input.set_text(query.clone(), cx));
                self.suggestions
                    .update(cx, |suggestions, cx| suggestions.ask(&query, cx));
                self.run(&query, cx);
            }
            Offer::Hit(found) => {
                remember(&self.typed, cx);
                self.open_hit(found.hit, cx);
                self.close_panel(cx);
            }
        }
    }

    fn forget(&mut self, query: &str, cx: &mut Context<Self>) {
        self.history
            .update(cx, |history, cx| history.remove(query, cx));
        self.pick = None;
        cx.notify();
    }

    /// Escape on the page: it puts the panel away first and lets go of the field second. An
    /// escape with neither goes on to the workspace.
    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_open(window, cx) {
            self.close_panel(cx);
            return;
        }
        if self.typing(window, cx) {
            window.focus(&self.focus, cx);
            return;
        }
        cx.propagate();
    }

    fn offer_row(
        &self,
        offer: &Offer,
        at: usize,
        total: usize,
        me: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let chosen = self.pick == Some(at);
        let query = match offer {
            Offer::Hit(found) => return self.row(found, at, at, total, true, chosen, me, cx),
            Offer::Past(query) => query,
        };
        let theme = *cx.theme();
        let taken = offer.clone();
        let forgotten = query.clone();
        let take = me.clone();
        let forget = me.clone();

        Card::new(("search-past", at), query.clone())
            .tucked()
            .fallback("icons/rotate-ccw-clock.svg")
            .action(
                Button::new(("search-forget", at))
                    .ghost()
                    .small()
                    .mr_1()
                    .icon("icons/x.svg")
                    .tooltip("search-forget")
                    .tint(theme.muted_foreground)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        forget
                            .update(cx, |this, cx| this.forget(&forgotten, cx))
                            .ok();
                    }),
            )
            .press(move |_, _, cx| {
                take.update(cx, |this, cx| this.take(taken.clone(), cx))
                    .ok();
            })
            .chosen(chosen)
            .when(chosen, |card| card.aria_active_descendant())
            .into_any_element()
    }

    /// The panel's rows, with placeholder rows while the first suggestions load. The heading only
    /// names the list for screen readers.
    fn panel_rows(&self, cx: &Context<Self>) -> AnyElement {
        let offers = self.offers(cx);
        let me = cx.entity().downgrade();
        let heading = match self.typed.trim().is_empty() {
            true => t!("search-recent"),
            false => t!("search-results"),
        };
        let waiting = offers.is_empty().then(|| {
            (0..SUGGESTED_WAITING).map(|at| {
                Card::skeleton(("search-offer-waiting", at))
                    .tucked()
                    .into_any_element()
            })
        });

        div()
            .id("search-offers")
            .role(Role::ListBox)
            .aria_label(heading)
            .flex()
            .flex_col()
            .gap_px()
            // A hit opens through its own card, so the panel only files the query and goes away
            // once a click inside it has landed.
            .on_click(cx.listener(|this, _, _, cx| {
                remember(&this.typed, cx);
                this.close_panel(cx);
            }))
            .children(waiting.into_iter().flatten())
            .children(
                offers
                    .iter()
                    .enumerate()
                    .map(|(at, offer)| self.offer_row(offer, at, offers.len(), &me, cx)),
            )
            .into_any_element()
    }

    /// The search field with its panel and the button that runs the full search. Once the page
    /// is too narrow for two columns of categories, the panel spans the button too, so the
    /// suggestions get the whole width. Once results are listed, their sort and filter menus sit
    /// after the button.
    fn field(
        &self,
        asked: bool,
        gutter: Pixels,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let open = self.panel_open(window, cx);
        let me = cx.entity().downgrade();
        let input = div()
            .flex_1()
            .min_w_0()
            .on_mouse_down(
                MouseButton::Left,
                // A click in the field asks for the panel, even when the focus it brings was
                // going to arrive quietly.
                cx.listener(|this, _, _, cx| {
                    this.hush = false;
                    if this.dismissed {
                        this.dismissed = false;
                        cx.notify();
                    }
                }),
            )
            .child(self.input.clone());
        let button = Button::new("search-submit")
            .icon("icons/search.svg")
            .label(t!("search-submit"))
            .primary()
            .h(cx.theme().metrics.field)
            .disabled(self.typed.trim().is_empty())
            .on_click(cx.listener(|this, _, _, cx| {
                let query = this.typed.clone();
                this.run(&query, cx);
            }));
        let row = || div().flex().items_center().gap_2();
        let dropdown = |anchor: AnyElement| {
            Dropdown::new("search-field", anchor)
                .flex_1()
                .min_w_0()
                .open(open)
                .on_close(move |_, cx| {
                    me.update(cx, |this, cx| this.close_panel(cx)).ok();
                })
                .when(open, |this| this.child(self.panel_rows(cx)))
        };

        let width = cells::content_width(window, cx.theme().metrics.inset * 2., cx);
        let field = match shelves::lanes(width) == 1 {
            true => row().child(dropdown(
                row().child(input).child(button).into_any_element(),
            )),
            false => row()
                .child(dropdown(input.into_any_element()))
                .child(button),
        };
        field
            .when(asked, |this| {
                this.child(self.sorts(cx)).child(self.filters(cx))
            })
            .flex_none()
            .px(gutter)
            .into_any_element()
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_open(window, cx) {
            if let Some(last) = self.offers(cx).len().checked_sub(1) {
                self.choose(Some(self.pick.map_or(0, |at| (at + 1).min(last))), cx);
                cx.notify();
            }
            return;
        }
        if self.search.read(cx).query().trim().is_empty() {
            // With no results to step into, Down asks for the panel the page opened without.
            if self.typing(window, cx) && self.dismissed {
                self.dismissed = false;
                cx.notify();
            }
            return;
        }
        let stacked = self.stacked(window, cx);
        let best = self.search.read(cx).best().is_some();
        match self.cursor {
            None if best && !self.on_best => self.on_best = true,
            None => {
                let Some(column) = self.first_filled(0, stacked, cx) else {
                    return;
                };
                let seats = self.column_seats(column, stacked, cx);
                let Some(first) = after(&seats, 0) else {
                    return;
                };
                self.on_best = false;
                self.place(column, first);
            }
            Some((column, row)) => {
                let seats = self.column_seats(column, stacked, cx);
                if let Some(next) = after(&seats, row + 1) {
                    self.place(column, next);
                }
            }
        }
        window.focus(&self.focus, cx);
        self.reveal(window, cx);
        cx.notify();
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_open(window, cx) {
            if let Some(at) = self.pick {
                self.choose(at.checked_sub(1), cx);
                cx.notify();
            }
            return;
        }
        let Some((column, row)) = self.cursor else {
            if std::mem::take(&mut self.on_best) {
                self.return_to_field(window, cx);
                cx.notify();
            }
            return;
        };
        let stacked = self.stacked(window, cx);
        let seats = self.column_seats(column, stacked, cx);
        let Some(previous) = before(&seats, row) else {
            self.cursor = None;
            match self.search.read(cx).best().is_some() {
                true => {
                    self.on_best = true;
                    self.reveal(window, cx);
                }
                false => self.return_to_field(window, cx),
            }
            cx.notify();
            return;
        };
        self.place(column, previous);
        window.focus(&self.focus, cx);
        self.reveal(window, cx);
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let stacked = self.stacked(window, cx);
        let Some((column, _)) = self.cursor else {
            return;
        };
        let Some(next) = self.prev_filled(column, stacked, cx) else {
            return;
        };
        self.hop(next, stacked, cx);
        window.focus(&self.focus, cx);
        self.reveal(window, cx);
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let stacked = self.stacked(window, cx);
        let Some((column, _)) = self.cursor else {
            return;
        };
        let Some(next) = self.next_filled(column, stacked, cx) else {
            return;
        };
        self.hop(next, stacked, cx);
        window.focus(&self.focus, cx);
        self.reveal(window, cx);
        cx.notify();
    }

    fn activate_hit(&mut self, _: &Activate, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_best {
            if let Some(best) = self.search.read(cx).best().cloned() {
                self.remember_query(cx);
                self.open_hit(best.hit, cx);
            }
            return;
        }
        let Some((column, row)) = self.cursor else {
            return;
        };
        let stacked = self.stacked(window, cx);
        let seats = self.column_seats(column, stacked, cx);
        let Some(at) = seats.get(row).and_then(Seat::at) else {
            return;
        };
        let Some(found) = self.search.read(cx).hits().get(at).cloned() else {
            return;
        };
        self.remember_query(cx);
        self.open_hit(found.hit, cx);
    }

    /// What choosing a hit does: a song plays its radio, or pauses and resumes when it is the one
    /// playing, and anything else opens its page.
    fn open_hit(&mut self, hit: Hit, cx: &mut Context<Self>) {
        match hit {
            Hit::Song(track) => {
                let current = track.id.is_some() && track.id == self.playback_status.0;
                self.playback.update(cx, |playback, cx| match current {
                    true => playback.toggle_play(cx),
                    false => playback.play_radio(&track, cx),
                });
            }
            Hit::Artist(artist) => {
                if let Some(id) = artist.id {
                    navigate(Destination::Artist(id.into()), cx);
                }
            }
            Hit::Album(album) => navigate(Destination::Album(album.id.into()), cx),
            Hit::Playlist(list) => navigate(Destination::Playlist(list.id.into()), cx),
        }
    }

    fn deselect_hit(&mut self, _: &Deselect, window: &mut Window, cx: &mut Context<Self>) {
        let on_best = std::mem::take(&mut self.on_best);
        if self.cursor.take().is_none() && !on_best {
            return;
        }
        self.return_to_field(window, cx);
        cx.notify();
    }

    fn place(&mut self, column: usize, row: usize) {
        if self.rows.len() <= column {
            self.rows.resize(column + 1, 0);
        }
        self.rows[column] = row;
        self.cursor = Some((column, row));
    }

    fn hop(&mut self, column: usize, stacked: bool, cx: &App) {
        let seats = self.column_seats(column, stacked, cx);
        let wanted = self.rows.get(column).copied().unwrap_or(0);
        let Some(row) = before(&seats, wanted + 1).or_else(|| after(&seats, wanted)) else {
            return;
        };
        self.place(column, row);
    }

    fn lanes(&self, cx: &App) -> Vec<&'static str> {
        self.search.read(cx).lanes(cx)
    }

    fn stacked(&self, window: &Window, cx: &App) -> bool {
        let lanes = self.lanes(cx).len().max(1);

        Chrome::content(window, cx) < LANE * lanes as f32
    }

    fn column_seats(&self, column: usize, stacked: bool, cx: &App) -> Vec<Seat> {
        match stacked {
            true => self.seats(None, cx),
            false => match self.lanes(cx).get(column) {
                Some(slug) => self.seats(Some(slug), cx),
                None => Vec::new(),
            },
        }
    }

    fn columns(&self, stacked: bool, cx: &App) -> usize {
        match stacked {
            true => 1,
            false => self.lanes(cx).len(),
        }
    }

    fn first_filled(&self, start: usize, stacked: bool, cx: &App) -> Option<usize> {
        (start..self.columns(stacked, cx))
            .find(|&column| after(&self.column_seats(column, stacked, cx), 0).is_some())
    }

    fn next_filled(&self, column: usize, stacked: bool, cx: &App) -> Option<usize> {
        self.first_filled(column + 1, stacked, cx)
    }

    fn prev_filled(&self, column: usize, stacked: bool, cx: &App) -> Option<usize> {
        (0..column)
            .rev()
            .find(|&at| after(&self.column_seats(at, stacked, cx), 0).is_some())
    }

    /// Scrolls the list the arrow keys are in so their row shows. The best match heads the
    /// mixed list, so standing on it scrolls that list to the top.
    fn reveal(&self, window: &Window, cx: &App) {
        let stacked = self.stacked(window, cx);
        if self.on_best && stacked {
            let scroll = self.mixed.read(cx).scroll().clone();
            scroll.set_offset(gpui::point(scroll.offset().x, Pixels::ZERO));
            return;
        }
        let Some((column, row)) = self.cursor else {
            return;
        };
        let scroll = self.bar(column, stacked, cx).read(cx).scroll().clone();
        let theme = *cx.theme();
        let gap = theme.font_size * ROW_GAP;
        let heights = self.heights(&self.column_seats(column, stacked, cx), window, cx);
        let Some(&height) = heights.get(row) else {
            return;
        };
        let above = match stacked {
            true => self.lead.get(),
            false => Pixels::ZERO,
        };
        let top = Deck::tops(&heights, gap)[row] + above;
        let visible = scroll.bounds().size.height;
        if visible <= Pixels::ZERO {
            return;
        }
        let offset = scroll.offset();
        let shown = -offset.y;
        let delta = if top < shown {
            top - shown
        } else if top + height > shown + visible {
            top + height - (shown + visible)
        } else {
            return;
        };
        scroll.set_offset(gpui::point(offset.x, offset.y - delta));
    }

    fn bar(&self, column: usize, stacked: bool, cx: &App) -> &Entity<Scrollbar> {
        if stacked {
            return &self.mixed;
        }

        match self.lanes(cx).get(column) {
            Some(slug) => self.bar_for(slug),
            None => &self.mixed,
        }
    }

    fn bar_for(&self, slug: &str) -> &Entity<Scrollbar> {
        self.bars
            .iter()
            .find(|(known, _)| *known == slug)
            .map_or(&self.mixed, |(_, bar)| bar)
    }

    fn heights(&self, seats: &[Seat], window: &Window, cx: &App) -> Vec<Pixels> {
        let theme = *cx.theme();
        let row = snapped(theme.metrics.list_row, window);
        let head = snapped(theme.metrics.header, window);

        seats
            .iter()
            .map(|seat| match seat {
                Seat::Head(_) => head,
                Seat::Item { .. } => row,
            })
            .collect()
    }

    /// The line under a hit's name: its artists or owner, led by the kind on the All tab. A
    /// release date sits in the row's trailing slot instead, so it never squeezes the artists.
    fn subtitle(&self, hit: &Hit, place: usize, compact: bool, theme: &Theme) -> AnyElement {
        let (kind, id, artists, fallback) = match hit {
            Hit::Song(track) => (
                Kind::Song,
                format!("song-artist-{place}"),
                track.artist_refs.clone(),
                track.artists.clone(),
            ),
            Hit::Album(album) => (
                Kind::Album,
                format!("album-artist-{place}"),
                album.artist_refs.clone(),
                album.artists.clone(),
            ),
            Hit::Playlist(list) if !list.owner.is_empty() => (
                Kind::Playlist,
                format!("playlist-owner-{place}"),
                Vec::new(),
                list.owner.clone(),
            ),
            Hit::Artist(_) | Hit::Playlist(_) => {
                return meta(hit, compact).into_any_element();
            }
        };

        let links = cells::artist_links(id, artists, fallback, theme.muted_foreground).truncate();

        match compact {
            true => div()
                .flex()
                .min_w_0()
                .gap_1()
                .child(div().flex_none().child(tag(kind)))
                .child(links)
                .into_any_element(),
            false => links.into_any_element(),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the row reports its seat in the list, so position and total ride along"
    )]
    fn row(
        &self,
        found: &Found,
        place: usize,
        index: usize,
        total: usize,
        compact: bool,
        chosen: bool,
        me: &WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let hit = &found.hit;
        let theme = *cx.theme();
        let meta = self.subtitle(hit, place, compact, &theme);
        let mark = crate::shared::provider_mark_of(found.slug, cx);

        let card = match hit {
            Hit::Song(track) => {
                let current = track.id.is_some() && track.id == self.playback_status.0;
                let tint = match current {
                    true => theme.primary,
                    false => theme.foreground,
                };
                Card::new(("song", place), track.name.clone())
                    .cover(track.cover.clone())
                    .tint(tint)
                    .meta(meta)
                    .when_some(mark, Card::mark)
                    .when(track.explicit, |card| card.explicit())
                    .trailing(
                        div()
                            .flex()
                            .flex_none()
                            .gap_3()
                            .when_some(release(&track.release_date, 0), |this, day| {
                                this.child(aside(day, &theme))
                            })
                            .child(aside(clock(track.duration), &theme)),
                    )
                    .press(pressed(Press::Song(Box::new(track.clone())), me))
            }
            Hit::Artist(artist) => {
                let card = Card::new(("artist", place), artist.name.clone())
                    .cover(artist.cover.clone())
                    .circle()
                    .underline()
                    .meta(meta)
                    .when_some(mark, Card::mark);
                match &artist.id {
                    Some(id) => card.press(pressed(Press::Artist(id.clone()), me)),
                    None => card,
                }
            }
            Hit::Album(album) => Card::new(("album", place), album.name.clone())
                .cover(album.cover.clone())
                .underline()
                .meta(meta)
                .when_some(mark, Card::mark)
                .when_some(release(&album.release_date, album.year), |card, day| {
                    card.trailing(aside(day, &theme))
                })
                .press(pressed(Press::Album(album.id.clone()), me)),
            Hit::Playlist(list) => Card::new(("playlist", place), list.name.clone())
                .cover(list.cover.clone())
                .underline()
                .meta(meta)
                .when_some(mark, Card::mark)
                .press(pressed(Press::Playlist(list.id.clone()), me)),
        };

        card.when_some(self.transport(hit, me, cx), |card, (playing, play)| {
            card.play(playing, play)
        })
        .when_some(hit.pin(), Pinnable::pin)
        .when_some(HitMenu::of(hit), |card, target| card.menu(menu(target, me)))
        .chosen(chosen)
        .when(compact, Card::tucked)
        // The keyboard cursor stays on the container, so the chosen card claims
        // the active descendant while every card reports its seat in the list.
        .aria_position_in_set(index + 1)
        .aria_size_of_set(total)
        // The mixed list tags each row with its kind, which the card's name leaves out.
        .when(compact, |card| card.aria_description(noun(hit.kind())))
        .when(chosen, |card| {
            card.aria_selected(true).aria_active_descendant()
        })
        .into_any_element()
    }

    fn transport(&self, hit: &Hit, me: &WeakEntity<Self>, cx: &App) -> Option<(bool, Play)> {
        let me = me.clone();
        let origin = match hit {
            Hit::Song(track) => {
                let current = track.id.is_some() && track.id == self.playback_status.0;
                let playing = current && self.playback_status.1 == Some(true);
                let track = track.clone();
                let play: Play = Box::new(move |_, _, cx| {
                    me.update(cx, |this, cx| {
                        this.remember_query(cx);
                        this.playback.update(cx, |playback, cx| match current {
                            true => playback.toggle_play(cx),
                            false => playback.play_radio(&track, cx),
                        });
                    })
                    .ok();
                });
                return Some((playing, play));
            }
            Hit::Artist(artist) => {
                state::Origin::artist(artist.id.clone()?).named(artist.name.clone())
            }
            Hit::Album(album) => state::Origin::album(album.id.clone()).named(album.name.clone()),
            Hit::Playlist(list) => {
                state::Origin::playlist(list.id.clone()).named(list.name.clone())
            }
        };
        let playing = self.playback.read(cx).playing_from(&origin, cx) == Some(true);
        let play: Play = Box::new(move |_, _, cx| {
            me.update(cx, |this, cx| {
                this.remember_query(cx);
                this.playback
                    .update(cx, |playback, cx| playback.toggle_origin(&origin, cx));
            })
            .ok();
        });

        Some((playing, play))
    }

    fn best(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *cx.theme();
        let found = self.search.read(cx).best()?;
        let hit = &found.hit;
        let (kind, title, artists, target) = match hit {
            Hit::Song(track) => (
                Kind::Song,
                track.name.clone(),
                track.artist_refs.clone(),
                Some(Press::Song(Box::new(track.clone()))),
            ),
            Hit::Artist(artist) => (
                Kind::Artist,
                artist.name.clone(),
                Vec::new(),
                artist.id.clone().map(Press::Artist),
            ),
            Hit::Album(album) => (
                Kind::Album,
                album.name.clone(),
                album.artist_refs.clone(),
                Some(Press::Album(album.id.clone())),
            ),
            Hit::Playlist(list) => (
                Kind::Playlist,
                list.name.clone(),
                Vec::new(),
                Some(Press::Playlist(list.id.clone())),
            ),
        };

        let me = cx.entity().downgrade();
        let card = Card::new("best", title)
            .art(theme.metrics.cover * 0.45)
            .cover(cover(hit).clone())
            .when(matches!(kind, Kind::Artist), Card::circle)
            .eyebrow(noun(kind))
            .size(Text::Title)
            .weight(FontWeight::BOLD)
            .bare_meta(
                cells::artist_links(
                    "best-artist-link",
                    artists,
                    meta(hit, false),
                    theme.muted_foreground,
                )
                .text_size(theme.text(Text::Small))
                .truncate(),
            )
            .when_some(crate::shared::provider_mark_of(found.slug, cx), Card::mark)
            .flat()
            .gap_4()
            .p_3()
            .bg(match self.on_best {
                true => theme.table_active,
                false => theme.secondary,
            })
            .when(self.on_best, |card| {
                card.aria_selected(true).aria_active_descendant()
            })
            .when_some(self.transport(hit, &me, cx), |card, (playing, play)| {
                card.play(playing, play)
            })
            .when_some(hit.pin(), Pinnable::pin)
            .when_some(target, |card, target| card.press(pressed(target, &me)))
            .when_some(HitMenu::of(hit), |card, target| {
                card.on_mouse_down(MouseButton::Right, menu(target, &me))
            })
            .aria_description(noun(kind))
            .into_any_element();

        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_2()
                .child({
                    let label = t!("search-best-match");
                    eyebrow(label.clone(), cx)
                        .id("search-best-heading")
                        .role(Role::Heading)
                        .aria_level(2)
                        .aria_label(label)
                        .pb_1()
                })
                .child(card)
                .into_any_element(),
        )
    }

    /// The page search shows in place of its results. Nothing here works without the network,
    /// so the No connection state covers the whole screen the moment it is gone, and a failed
    /// search or a failed browse shows its own reason otherwise. Asking again is the retry,
    /// since neither loader counts a failure as served.
    fn failure(&self, asked: bool, gutter: Pixels, cx: &Context<Self>) -> Option<AnyElement> {
        let offline = Network::lost(cx);
        let (id, reason, retry) = match asked {
            true => {
                let reason = match offline {
                    true => None,
                    false => Some(self.search.read(cx).error()?.to_owned()),
                };
                let search = self.search.clone();
                let query = search.read(cx).query().to_owned();
                let retry: Box<dyn Fn(&mut App)> = Box::new(move |cx| {
                    search.update(cx, |search, cx| search.ask(&query, cx));
                });
                ("search-lost", reason, retry)
            }
            false => {
                let reason = match offline {
                    true => None,
                    false => Some(self.genres.read(cx).error()?.to_owned()),
                };
                let genres = self.genres.clone();
                let retry: Box<dyn Fn(&mut App)> = Box::new(move |cx| {
                    genres.update(cx, |genres, cx| genres.load(cx));
                });
                ("search-browse-lost", reason, retry)
            }
        };

        Some(
            trouble::lost(
                id,
                t!("trouble-not-loaded"),
                reason.as_deref(),
                move |_, _, cx| retry(cx),
                cx,
            )
            .flex_1()
            .min_h_0()
            .px(gutter)
            .into_any_element(),
        )
    }

    fn shell(
        &self,
        id: impl Into<ElementId>,
        title: SharedString,
        body: AnyElement,
        gutter: Pixels,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_1()
            .child(
                div()
                    .id(id)
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(title.clone())
                    .pl(gutter)
                    .child(eyebrow(title, cx).pb_1()),
            )
            .child(body)
            .into_any_element()
    }

    fn column(&self, lane: &'static str, window: &Window, cx: &Context<Self>) -> AnyElement {
        let session = Sonora::global(cx).session.clone();
        let title = nav_label(NavEntry::Library(lane), session.read(cx));
        let id = SharedString::from(format!("search-{lane}-heading"));
        let body = self.deck(Some(lane), RAIL, None, title.clone(), window, cx);

        self.shell(id, title, body, RAIL, cx)
    }

    fn browse(&self, gutter: Pixels, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let found = self.genres.read(cx).genres();
        let theme = *cx.theme();
        let pad = theme.metrics.inset;
        let width = cells::content_width(window, pad * 2., cx);
        let plates = shelves::grid("genre", found, width, window, cx);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .children(self.recent(gutter, cx))
            .child(div().px(gutter).child({
                let label = t!("search-browse");
                div()
                    .id("search-browse-heading")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(label.clone())
                    .child(eyebrow(label, cx))
            }))
            .child(
                Scroller::new("search-browse", &self.browsing)
                    .bottom_inset(crate::chrome::Chrome::bottom(cx))
                    .fade_top(FADE)
                    .px(gutter)
                    .pb(pad)
                    .child(plates),
            )
            .into_any_element()
    }

    /// The recent searches above Browse all, or nothing when there are none.
    fn recent(&self, gutter: Pixels, cx: &Context<Self>) -> Option<AnyElement> {
        let entries = self.history.read(cx).entries();
        if entries.is_empty() {
            return None;
        }
        let rows = entries.iter().enumerate().map(|(at, query)| {
            let asked = query.clone();
            let gone = query.clone();
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new(("search-recent", at))
                        .label(SharedString::from(query.clone()))
                        .small()
                        .ghost()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.take(Offer::Past(asked.clone()), cx);
                        })),
                )
                .child(
                    Button::new(("search-recent-remove", at))
                        .icon("icons/x.svg")
                        .small()
                        .ghost()
                        .tooltip("search-recent-remove")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.history
                                .update(cx, |history, cx| history.remove(&gone, cx));
                        })),
                )
        });

        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap_1()
                .px(gutter)
                .pb_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child({
                            let label = t!("search-recent");
                            div()
                                .id("search-recent-heading")
                                .role(Role::Heading)
                                .aria_level(2)
                                .aria_label(label.clone())
                                .child(eyebrow(label, cx))
                        })
                        .child(
                            Button::new("search-recent-clear")
                                .label(t!("search-recent-clear"))
                                .small()
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history.update(cx, |history, cx| history.clear(cx));
                                })),
                        ),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    fn everything(&self, gutter: Pixels, window: &Window, cx: &Context<Self>) -> AnyElement {
        let lead = div()
            .flex()
            .flex_col()
            .gap_6()
            .children(self.best(cx))
            .child({
                let label = t!("search-results");
                eyebrow(label.clone(), cx)
                    .id("search-results-heading")
                    .role(Role::Heading)
                    .aria_level(2)
                    .aria_label(label)
                    .pb_1()
            })
            .into_any_element();

        self.deck(None, gutter, Some(lead), t!("search-results"), window, cx)
    }

    /// Applies a change to the search lens.
    fn relens(&mut self, change: impl FnOnce(Lens) -> Lens, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            let lens = change(search.lens().clone());
            search.set_lens(lens, cx);
        });
    }

    /// Narrows the results to one kind, or back to all of them.
    fn show(&mut self, kind: Option<Kind>, cx: &mut Context<Self>) {
        if self.search.read(cx).query().trim().is_empty() {
            return;
        }
        self.relens(|lens| lens.with_kind(kind), cx);
    }

    fn tabs(&self, cx: &Context<Self>) -> TabBar {
        let current = self.search.read(cx).lens().kind;
        TabBar::new("search-kinds")
            .flex_none()
            .items(TABS.iter().map(|&(kind, key)| {
                Button::new(SharedString::from(format!("{key}-tab")))
                    .label(i18n::lookup(key, None))
                    .small()
                    .ghost()
                    .selected(kind == current)
                    .on_click(cx.listener(move |this, _, _, cx| this.show(kind, cx)))
            }))
    }

    fn sorts(&self, cx: &Context<Self>) -> AnyElement {
        let lens = self.search.read(cx).lens();
        let axes = Axis::of(lens.kind)
            .iter()
            .map(|&axis| SortAxis {
                key: axis_key(axis),
                label: i18n::lookup(axis_label(axis), None),
                order: lens
                    .order
                    .filter(|(current, _)| *current == axis)
                    .map(|(_, sort)| sort),
            })
            .collect();
        let me = cx.entity().downgrade();
        tools::sorts(
            &self.popovers,
            axes,
            move |key, cx| {
                let Some(axis) = axis_of(key) else {
                    return;
                };
                me.update(cx, |this, cx| this.relens(|lens| lens.cycled(axis), cx))
                    .ok();
            },
            cx,
        )
    }

    fn filters(&self, cx: &Context<Self>) -> AnyElement {
        let search = self.search.read(cx);
        let lens = search.lens();
        let session = Sonora::global(cx).session.read(cx);
        let mut axes: Vec<Filter> = search
            .sources(cx)
            .into_iter()
            .map(|slug| {
                Filter::Flag(FlagAxis {
                    key: slug,
                    label: SharedString::from(session.provider_name_for(slug).unwrap_or(slug)),
                    on: !lens.hidden.contains(&slug),
                })
            })
            .collect();
        axes.push(Filter::Flag(FlagAxis {
            key: MINE,
            label: t!("search-mine-only"),
            on: lens.mine_only,
        }));
        let narrowed = !lens.hidden.is_empty() || lens.mine_only;
        let me = cx.entity().downgrade();
        tools::filters_tinted(
            &self.popovers,
            &self.sliders,
            axes,
            narrowed,
            move |change, cx| {
                me.update(cx, |this, cx| {
                    this.relens(
                        |lens| match change {
                            FilterChange::Flag(MINE, on) => Lens {
                                mine_only: on,
                                ..lens
                            },
                            FilterChange::Flag(slug, on) => lens.with_source(slug, on),
                            FilterChange::Reset => lens.unfiltered(),
                            FilterChange::Range(..) => lens,
                        },
                        cx,
                    )
                })
                .ok();
            },
            cx,
        )
    }

    /// What a lane or the mixed list shows when nothing is left: a plain "No matches", or,
    /// when the lens narrowed it, why, with a way back.
    fn empty(&self, id: &str, cx: &Context<Self>) -> AnyElement {
        let lens = self.search.read(cx).lens();
        let narrowed = !lens.hidden.is_empty() || lens.mine_only;
        let title = match narrowed {
            true => t!("search-filtered-empty"),
            false => t!("search-no-matches"),
        };
        let status = div()
            .id(SharedString::from(format!("{id}-empty")))
            .role(Role::Status)
            .aria_label(title.clone())
            .flex_none()
            .child(vacant(title, cx));
        match narrowed {
            false => status.into_any_element(),
            true => div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(status)
                .child(
                    Button::new("search-filters-reset")
                        .label(t!("filter-reset"))
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.relens(Lens::unfiltered, cx);
                        })),
                )
                .into_any_element(),
        }
    }

    /// Where each hit of `lane`, or of every lane when `None`, sits in its list, under a head
    /// per kind on the All tab. The best match is drawn above the lists, so they leave it out.
    fn seats(&self, lane: Option<&str>, cx: &App) -> Vec<Seat> {
        let search = self.search.read(cx);
        let hits = search.hits();
        let single = search.lens().kind.is_some();
        let skipped = usize::from(search.best().is_some());
        let Some(lane) = lane else {
            return (skipped..hits.len())
                .map(|at| Seat::Item {
                    at,
                    compact: !single,
                })
                .collect();
        };
        if single {
            return hits
                .iter()
                .enumerate()
                .filter(|(_, found)| found.slug == lane)
                .map(|(at, _)| Seat::Item { at, compact: false })
                .collect();
        }

        let mut seats = Vec::new();
        for group in GROUPS {
            let compact = group.len() > 1;
            let mut opened = false;
            for (at, found) in hits.iter().enumerate().skip(skipped) {
                if found.slug != lane || !group.contains(&found.kind()) {
                    continue;
                }
                if !opened {
                    seats.push(Seat::Head(group_key(group)));
                    opened = true;
                }
                seats.push(Seat::Item { at, compact });
            }
        }

        seats
    }

    fn deck(
        &self,
        lane: Option<&'static str>,
        gutter: Pixels,
        lead: Option<AnyElement>,
        label: SharedString,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (id, bar) = match lane {
            None => (SharedString::new_static("search-all"), &self.mixed),
            Some(lane) => (
                SharedString::from(format!("search-{lane}")),
                self.bar_for(lane),
            ),
        };
        let seats = self.seats(lane, cx);
        if after(&seats, 0).is_none() {
            let empty = self.empty(&id, cx);
            return match lead {
                Some(lead) => div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px(gutter)
                    .child(lead)
                    .child(empty)
                    .into_any_element(),
                None => empty,
            };
        }

        let leading = lead.is_some();
        let total = seats.iter().filter(|seat| seat.at().is_some()).count();
        let column = match lane {
            None => 0,
            Some(lane) => self
                .lanes(cx)
                .iter()
                .position(|known| *known == lane)
                .unwrap_or(0),
        };
        let theme = *cx.theme();
        let heights = self.heights(&seats, window, cx);
        let scroll = bar.read(cx).scroll().clone();
        let me = cx.entity().downgrade();
        let measured = self.lead.clone();
        let tracked = scroll.clone();
        let deck = Deck::new(format!("{id}-deck"))
            .rows(heights)
            .gap(theme.font_size * ROW_GAP)
            .when(lead.is_some(), |deck| {
                deck.on_measure(move |top, _, _| {
                    let content = tracked.bounds().origin.y - scrolled(&tracked);
                    measured.set(top - content);
                })
            })
            .draw(move |index, _, cx| {
                let Some(view) = me.upgrade() else {
                    return div().into_any_element();
                };
                let (at, compact) = match seats.get(index) {
                    Some(Seat::Item { at, compact }) => (*at, *compact),
                    Some(Seat::Head(key)) => {
                        return div()
                            .flex()
                            .items_end()
                            .h_full()
                            .child(eyebrow(i18n::lookup(key, None), cx))
                            .into_any_element();
                    }
                    None => return div().into_any_element(),
                };
                let this = view.read(cx);
                let Some(found) = this.search.read(cx).hits().get(at) else {
                    return div().into_any_element();
                };

                this.row(
                    found,
                    at,
                    seats[..index]
                        .iter()
                        .filter(|seat| seat.at().is_some())
                        .count(),
                    total,
                    compact,
                    this.cursor == Some((column, index)),
                    &view.downgrade(),
                    cx,
                )
            });

        div()
            .id(SharedString::from(format!("{id}-results")))
            .role(Role::ListBox)
            .aria_label(label)
            .flex_1()
            .min_h_0()
            // The mixed list sits right under the field, so its fade spends the gap above it.
            .when(leading, |this| this.mt(-FADE))
            .child(
                Scroller::new(id.clone(), bar)
                    .bottom_inset(crate::chrome::Chrome::bottom(cx))
                    .fade_top(FADE)
                    .px(gutter)
                    .pb(theme.metrics.inset)
                    .child(div().flex().flex_col().gap_1().children(lead).child(deck)),
            )
            .into_any_element()
    }
}

/// Files `query` at the top of the search history.
fn remember(query: &str, cx: &mut App) {
    Sonora::global(cx)
        .search_history
        .clone()
        .update(cx, |history, cx| history.record(query, cx));
}

fn axis_key(axis: Axis) -> &'static str {
    match axis {
        Axis::Title => "title",
        Axis::Artist => "artist",
        Axis::Duration => "duration",
        Axis::Released => "released",
    }
}

fn axis_label(axis: Axis) -> &'static str {
    match axis {
        Axis::Title => "column-title",
        Axis::Artist => "column-artist",
        Axis::Duration => "column-length",
        Axis::Released => "column-released",
    }
}

fn axis_of(key: &str) -> Option<Axis> {
    [Axis::Title, Axis::Artist, Axis::Duration, Axis::Released]
        .into_iter()
        .find(|axis| axis_key(*axis) == key)
}

fn after(seats: &[Seat], from: usize) -> Option<usize> {
    (from..seats.len()).find(|&at| seats[at].at().is_some())
}

fn before(seats: &[Seat], until: usize) -> Option<usize> {
    (0..until.min(seats.len()))
        .rev()
        .find(|&at| seats[at].at().is_some())
}

fn pressed(
    target: Press,
    me: &WeakEntity<SearchView>,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let me = me.clone();

    move |_, _, cx| {
        me.update(cx, |this, cx| {
            this.remember_query(cx);
            match &target {
                Press::Song(track) => this
                    .playback
                    .update(cx, |playback, cx| playback.play_radio(track, cx)),
                Press::Artist(id) => navigate(Destination::Artist(id.clone().into()), cx),
                Press::Album(id) => navigate(Destination::Album(id.clone().into()), cx),
                Press::Playlist(id) => navigate(Destination::Playlist(id.clone().into()), cx),
            }
        })
        .ok();
    }
}

fn menu(
    target: HitMenu,
    me: &WeakEntity<SearchView>,
) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
    let me = me.clone();

    move |event: &MouseDownEvent, window, cx| {
        window.prevent_default();
        me.update(cx, |this, cx| {
            this.track_menu.reset(cx);
            this.context_menu = Some((target.clone(), event.position));
            cx.notify();
        })
        .ok();
    }
}

fn album_of(hit: &AlbumHit, cx: &App) -> Album {
    Sonora::global(cx)
        .library
        .read(cx)
        .album(&hit.id)
        .cloned()
        .unwrap_or_else(|| Album {
            id: hit.id.clone(),
            name: hit.name.clone(),
            artists: hit.artists.clone(),
            artist_refs: hit.artist_refs.clone(),
            cover: hit.cover.clone(),
            cover_large: hit.cover.clone(),
            year: hit.year,
            release_date: hit.release_date.clone(),
            ..Default::default()
        })
}

fn playlist_of(hit: &PlaylistHit, cx: &App) -> Playlist {
    Sonora::global(cx)
        .library
        .read(cx)
        .playlist(&hit.id)
        .cloned()
        .unwrap_or_else(|| Playlist {
            id: hit.id.clone(),
            name: hit.name.clone(),
            owner: hit.owner.clone(),
            cover: hit.cover.clone(),
            ..Default::default()
        })
}

fn artist_of(hit: &ArtistHit, cx: &App) -> SavedArtist {
    let id = hit.id.clone().unwrap_or_default();
    Sonora::global(cx)
        .library
        .read(cx)
        .artist(&id)
        .cloned()
        .unwrap_or_else(|| SavedArtist {
            id,
            name: hit.name.clone(),
            cover: hit.cover.clone(),
            ..Default::default()
        })
}

fn cover(hit: &Hit) -> Option<String> {
    match hit {
        Hit::Song(track) => track.cover.clone(),
        Hit::Artist(artist) => artist.cover.clone(),
        Hit::Album(album) => album.cover.clone(),
        Hit::Playlist(list) => list.cover.clone(),
    }
}

fn meta(hit: &Hit, compact: bool) -> SharedString {
    match hit {
        Hit::Song(track) => SharedString::from(track.artists.clone()),
        Hit::Album(album) => SharedString::from(album.artists.clone()),
        Hit::Artist(artist) => match compact {
            true => noun(Kind::Artist),
            false => held(artist.saved),
        },
        Hit::Playlist(list) => match list.owner.is_empty() {
            true => noun(Kind::Playlist),
            false => SharedString::from(list.owner.clone()),
        },
    }
}

/// How a row names the day a hit came out: `date` in full, or `year` when the provider gave
/// no date. `None` when neither is known.
fn release(date: &str, year: i32) -> Option<SharedString> {
    match date.is_empty() {
        false => cards::when_label(cards::When::Day(date), None),
        true => cards::when_label(cards::When::Year(year), None),
    }
}

/// A muted, tabular figure for a row's trailing slot, such as a length or a release day.
fn aside(text: impl Into<SharedString>, theme: &Theme) -> impl IntoElement {
    div()
        .flex_none()
        .whitespace_nowrap()
        .text_size(theme.text(Text::Small))
        .text_color(theme.muted_foreground)
        .font_features(tabular())
        .child(text.into())
}

fn tag(kind: Kind) -> SharedString {
    let noun = noun(kind);
    t!("search-tag", kind = &noun)
}

fn held(saved: usize) -> SharedString {
    match saved {
        0 => noun(Kind::Artist),
        count => t!("search-saved", count = count),
    }
}

fn noun(kind: Kind) -> SharedString {
    match kind {
        Kind::Song => t!("kind-song"),
        Kind::Artist => t!("kind-artist"),
        Kind::Album => t!("kind-album"),
        Kind::Playlist => t!("kind-playlist"),
    }
}

impl Focusable for SearchView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let pad = theme.metrics.inset;
        let width = cells::content_width(window, pad * 2., cx);
        let stacked = !Chrome::room(window, cx).fits(Room::Wide);
        let inset = match width > VAST {
            true => (width - VAST) / 2.,
            false => Pixels::ZERO,
        };
        let asked = !self.search.read(cx).query().trim().is_empty();
        let context_menu = self.context_menu.clone().map(|(target, position)| {
            let menu = match target {
                HitMenu::Song(track) => self.track_menu.for_track(&track, cx),
                HitMenu::Album(hit) => album_menu(
                    album_of(&hit, cx),
                    self.playback.clone(),
                    &self.track_menu,
                    cx,
                ),
                HitMenu::Playlist(hit) => {
                    playlist_menu(playlist_of(&hit, cx), self.playback.clone(), cx)
                }
                HitMenu::Artist(hit) => {
                    artist_menu(artist_of(&hit, cx), self.playback.clone(), false, cx)
                }
            };

            Popup::new(position, menu).on_close(cx.listener(|this, _, _, cx| {
                this.context_menu = None;
                cx.notify();
            }))
        });

        let gutter = pad + inset;
        let results = match self.failure(asked, gutter, cx) {
            Some(failure) => failure,
            None => match (asked, stacked) {
                (false, _) => self.browse(gutter, window, cx),
                (true, true) => self.everything(gutter, window, cx),
                (true, false) => {
                    let lanes = self.lanes(cx);
                    match lanes.is_empty() {
                        true => div()
                            .flex_1()
                            .px(gutter)
                            .child(self.empty("search-lanes", cx))
                            .into_any_element(),
                        false => div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .px(gutter)
                            .children(lanes.iter().enumerate().flat_map(|(at, lane)| {
                                [
                                    (at > 0).then(|| Separator::vertical().into_any_element()),
                                    Some(self.column(lane, window, cx)),
                                ]
                                .into_iter()
                                .flatten()
                            }))
                            .into_any_element(),
                    }
                }
            },
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_6()
            .pt(pad)
            .key_context(SEARCH_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::activate_hit))
            .on_action(cx.listener(Self::deselect_hit))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(|this, _: &SearchAll, _, cx| this.show(None, cx)))
            .on_action(cx.listener(|this, _: &SearchSongs, _, cx| this.show(Some(Kind::Song), cx)))
            .on_action(
                cx.listener(|this, _: &SearchArtists, _, cx| this.show(Some(Kind::Artist), cx)),
            )
            .on_action(
                cx.listener(|this, _: &SearchAlbums, _, cx| this.show(Some(Kind::Album), cx)),
            )
            .on_action(
                cx.listener(|this, _: &SearchPlaylists, _, cx| this.show(Some(Kind::Playlist), cx)),
            )
            .child(self.field(asked, gutter, window, cx))
            .when(asked, |this| {
                this.child(div().flex_none().px(gutter).child(self.tabs(cx)))
            })
            .when(!stacked, |this| {
                this.children(self.best(cx).map(|best| div().px(gutter).child(best)))
            })
            .child(results)
            .when_some(context_menu, |this, menu| this.child(menu))
    }
}
