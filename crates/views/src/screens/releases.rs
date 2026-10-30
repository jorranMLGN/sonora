use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, Pixels, Render, ScrollHandle, Window, div};
use i18n::t;
use state::{Playback, Releases, ReleasesState, Session, Sonora};
use ui::{ActiveTheme as _, Card, Scrollbar, Scroller, Vacancy};

use crate::chrome::Chrome;
use crate::shared::album_grid::{AlbumGrid, CardGrid};
use crate::shared::cells;
use crate::shared::hero::{HeroMetaStrip, PageHero};
use crate::shared::trouble;

/// How many rows of placeholder cards the page shows while it loads.
const PENDING_ROWS: usize = 2;

/// The New releases page: a grid of the recent albums, singles and EPs `Releases` gathered from
/// every signed-in provider, with its loading, empty and error states.
pub(crate) struct ReleasesView {
    releases: Entity<Releases>,
    session: Entity<Session>,
    playback: Entity<Playback>,
    scrollbar: Entity<Scrollbar>,
}

impl ReleasesView {
    pub(crate) fn new(
        releases: Entity<Releases>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&releases, |_, _, cx| cx.notify()).detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();
        let id = cx.entity_id();
        let scrollbar = cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(id));
        Self {
            releases,
            session: Sonora::global(cx).session.clone(),
            playback,
            scrollbar,
        }
    }

    /// Tells the list whether the page is on screen, which fetches it when it is stale.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.releases
            .update(cx, |releases, cx| releases.set_visible(visible, cx));
    }

    pub(crate) fn is_loading(&self, cx: &App) -> bool {
        self.releases.read(cx).is_loading()
    }

    /// Asks every artist again.
    pub(crate) fn reload(&mut self, cx: &mut Context<Self>) {
        self.releases.update(cx, |releases, cx| releases.reload(cx));
    }

    fn header(&self, count: usize, cx: &App) -> AnyElement {
        let strip = HeroMetaStrip::new()
            .when(count > 0, |strip| {
                strip.text(t!("count-releases", count = count))
            })
            .text(t!("releases-window", days = Releases::DAYS));
        PageHero::new("releases-hero", t!("nav-releases"))
            .fallback("icons/disc-3.svg")
            .accent()
            .eyebrow(t!("releases-eyebrow"))
            .meta(strip)
            .pending(count == 0 && self.releases.read(cx).is_loading())
            .into_any_element()
    }

    /// Rows of placeholder cards, laid out the way the grid will be.
    fn pending(available: Pixels) -> AnyElement {
        let layout = CardGrid::layout(available);
        div()
            .flex()
            .flex_wrap()
            .w_full()
            .gap_x(layout.gap)
            .gap_y_6()
            .children(
                (0..layout.columns * PENDING_ROWS)
                    .map(|place| Card::skeleton(("releases-pending", place)).tile(layout.card)),
            )
            .into_any_element()
    }

    /// What the page says when it has no release to show.
    fn vacancy(&self, state: ReleasesState, cx: &App) -> Option<Vacancy> {
        let signed_in = !self.session.read(cx).active_slugs().is_empty();
        match state {
            _ if !signed_in => {
                Some(Vacancy::new(t!("releases-signed-out")).icon("icons/disc-3.svg"))
            }
            ReleasesState::Idle | ReleasesState::Loading => None,
            ReleasesState::Ready => Some(
                Vacancy::new(t!("releases-empty"))
                    .detail(t!("releases-empty-detail"))
                    .icon("icons/disc-3.svg"),
            ),
            ReleasesState::Failed => {
                let releases = self.releases.clone();
                Some(trouble::lost(
                    "releases-retry",
                    t!("releases-failed"),
                    self.releases.read(cx).reason(),
                    move |_, _, cx| releases.update(cx, |releases, cx| releases.reload(cx)),
                    cx,
                ))
            }
        }
    }
}

impl Render for ReleasesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inset = cx.theme().metrics.inset;
        let available = cells::content_width(window, inset * 2., cx);
        let (state, albums) = {
            let releases = self.releases.read(cx);
            (releases.state(), releases.albums())
        };

        let body = match albums.is_empty() {
            false => AlbumGrid::new(
                "releases-album",
                available,
                albums.iter().cloned().enumerate(),
                self.playback.clone(),
            )
            .dated()
            .into_any_element(),
            true => match self.vacancy(state, cx) {
                Some(vacancy) => vacancy.into_any_element(),
                None => Self::pending(available),
            },
        };

        Scroller::new("releases-page", &self.scrollbar)
            .bottom_inset(Chrome::bottom(cx))
            .p(inset)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .child(self.header(albums.len(), cx))
                    .child(body),
            )
    }
}
