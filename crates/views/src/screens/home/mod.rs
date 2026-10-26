use std::rc::Rc;

mod featured;

use crate::chrome::Chrome;
use crate::shared::menus::{Item, ItemMenu};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Hsla, MouseDownEvent, Pixels, Point, Render, ScrollHandle,
    SharedString, Task, WeakEntity, Window, div, px,
};
use i18n::t;
use music::{GenreItem, GenreSection};
use state::{Discover, Home, Network, Playback, SessionState, Sonora};
use ui::{ActiveTheme as _, Lazy, Mode, Popup, Scrollbar, Scroller};

use crate::shared::cells;
use crate::shared::picks::{Picks, Shape};
use crate::shared::shelves::Shelves;
use crate::shared::tracks::{PlaybackStatus, playback_status};
use crate::shared::trouble;

const STEADY: Pixels = px(0.5);

pub(crate) struct HomeView {
    home: Entity<Home>,
    discover: Entity<Discover>,
    /// Shelves for New releases, drawn above Quick picks; `shelves` draws the rest.
    fresh: Entity<Shelves>,
    resume: Pager,
    /// The hero's cover and its sampled hue, kept so a cover is sampled once.
    tint: Option<(String, Option<Hsla>)>,
    sampling: Option<Task<()>>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    shelves: Entity<Shelves>,
    width: Pixels,
    quick_picks: Pager,
    scrollbar: Entity<Scrollbar>,
    /// The submenu state of a track's context menu.
    menus: ItemMenu,
    context_menu: Option<(Item, Point<Pixels>)>,
}

#[derive(Default)]
struct Pager {
    columns: usize,
    page: usize,
}

impl Pager {
    fn fit(&mut self, columns: usize, pages: usize) -> usize {
        if self.columns != columns {
            self.columns = columns;
            self.page = 0;
        }
        self.page = self.page.min(pages.saturating_sub(1));
        self.page
    }
}

impl HomeView {
    pub(crate) fn new(
        home: Entity<Home>,
        discover: Entity<Discover>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let me = cx.entity_id();
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset().watching(me));
        let menus = ItemMenu::new(playlist_scrollbar, cx);

        cx.observe(&home, |this, _, cx| {
            this.menus.reset(cx);
            this.context_menu = None;
            cx.notify();
        })
        .detach();
        cx.observe(&discover, |_, _, cx| cx.notify()).detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let library = Sonora::global(cx).library.clone();
        cx.observe(&library, |_, _, cx| cx.notify()).detach();

        let shelves = cx.new(|_| Shelves::new("home-shelf", me, playback.clone()));
        cx.observe(&shelves, |_, _, cx| cx.notify()).detach();

        let fresh = cx.new(|_| Shelves::new("home-fresh", me, playback.clone()));
        cx.observe(&fresh, |_, _, cx| cx.notify()).detach();

        let current_playback = playback_status(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status != current {
                this.playback_status = current;
                cx.notify();
            }
        })
        .detach();

        Self {
            home,
            discover,
            fresh,
            resume: Pager::default(),
            tint: None,
            sampling: None,
            playback,
            playback_status: current_playback,
            shelves,
            width: Pixels::ZERO,
            quick_picks: Pager::default(),
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
            menus,
            context_menu: None,
        }
    }

    /// Tells the feed whether this page is the one on screen, so it lands changes to what is
    /// drawn only while nobody is looking.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.home
            .update(cx, |home, cx| home.set_visible(visible, cx));
        self.discover
            .update(cx, |discover, cx| discover.set_visible(visible, cx));
    }

    /// Samples the featured cover's hue once per cover; the hero redraws when it lands.
    fn sample(&mut self, cover: Option<String>, cx: &mut Context<Self>) {
        let Some(cover) = cover else {
            self.tint = None;
            return;
        };
        if self
            .tint
            .as_ref()
            .is_some_and(|(sampled, _)| *sampled == cover)
        {
            return;
        }
        self.tint = Some((cover.clone(), None));
        let task = ui::tint(SharedString::from(cover.clone()), cx);
        self.sampling = Some(cx.spawn(async move |this, cx| {
            let tint = task.await;
            this.update(cx, |this, cx| {
                this.tint = Some((cover, tint));
                cx.notify();
            })
            .ok();
        }));
    }

    /// Jump back in: one page of what the user played last, across every provider.
    fn resume(&mut self, width: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items = self.home.read(cx).resume(cx);
        if items.is_empty() {
            return None;
        }
        let shape = Shape::new(width, items.len());
        let pages = shape.pages;
        let page = self.resume.fit(shape.columns, shape.pages);
        let opened = items.clone();
        let home = cx.entity().downgrade();

        Some(
            Picks::mixed("home-resume", items, self.playback.clone(), width, page)
                .title("home-resume")
                .on_previous(cx.listener(|this, _, _, cx| {
                    this.resume.page = this.resume.page.saturating_sub(1);
                    cx.notify();
                }))
                .on_next(cx.listener(move |this, _, _, cx| {
                    this.resume.page = (this.resume.page + 1).min(pages.saturating_sub(1));
                    cx.notify();
                }))
                .on_context_menu(move |place, event, _, cx| {
                    open_item_menu(&home, &opened, place, event, cx);
                })
                .into_any_element(),
        )
    }

    /// Fetches the feed and the library it mixes from again.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.refresh(cx));
    }

    /// Whether Quick picks or the shelves are still on their way.
    pub(crate) fn is_loading(&self, cx: &App) -> bool {
        let home = self.home.read(cx);
        home.is_loading(cx) || home.is_feeding()
    }

    /// The Quick picks deck, headed by the name of the account it was drawn from. It is the
    /// one deck on the page: what the provider lists as played lately and its picks after
    /// them, a skeleton while they are on their way.
    fn quick_picks(&mut self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let items = self.home.read(cx).quick_picks();
        let shape = Shape::new(width, items.len());
        let pages = shape.pages;
        let page = self.quick_picks.fit(shape.columns, shape.pages);
        let session = Sonora::global(cx).session.read(cx);
        let name = match (self.home.read(cx).is_local(cx), session.state()) {
            (false, SessionState::SignedIn) => session
                .provider_slug()
                .and_then(|slug| session.profile_for(slug))
                .map(|profile| profile.display_name.clone()),
            _ => None,
        };
        let opened = items.clone();
        let home = cx.entity().downgrade();

        Picks::mixed("quick-picks", items, self.playback.clone(), width, page)
            .title("home-quick-picks")
            .vacancy("home-quick-picks-empty")
            .loading(self.home.read(cx).is_loading(cx))
            .when_some(name.filter(|name| !name.is_empty()), |deck, name| {
                deck.eyebrow(name)
            })
            .on_previous(cx.listener(|this, _, _, cx| {
                this.quick_picks.page = this.quick_picks.page.saturating_sub(1);
                this.context_menu = None;
                cx.notify();
            }))
            .on_next(cx.listener(move |this, _, _, cx| {
                this.quick_picks.page = (this.quick_picks.page + 1).min(pages.saturating_sub(1));
                this.context_menu = None;
                cx.notify();
            }))
            .on_context_menu(move |place, event, _, cx| {
                open_item_menu(&home, &opened, place, event, cx);
            })
            .into_any_element()
    }

    /// Whether every section of the page is empty, so a failure has nothing to stand beside.
    fn blank(&self, cx: &Context<Self>) -> bool {
        let discover = self.discover.read(cx);
        discover.releases().is_empty()
            && discover.mixes().is_empty()
            && self.home.read(cx).resume(cx).is_empty()
    }

    /// The page home shows in place of its shelves: the No connection state the moment the
    /// network is gone, since every row here comes from the provider, and the reason its feed
    /// failed otherwise. Asking for the feed again is the retry, and it starts the pauses
    /// between tries over.
    fn failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let reason = match Network::lost(cx) {
            true => None,
            false => Some(self.home.read(cx).error()?.to_owned()),
        };
        let home = self.home.clone();

        Some(
            trouble::lost(
                "home-lost",
                t!("trouble-not-loaded"),
                reason.as_deref(),
                move |_, _, cx| {
                    home.update(cx, |home, cx| home.retry(cx));
                },
                cx,
            )
            .size_full()
            .into_any_element(),
        )
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(failure) = self.failure(cx)
            && self.blank(cx)
        {
            return div().size_full().child(failure).into_any_element();
        }

        let theme = *cx.theme();
        let available = cells::content_width(window, theme.metrics.inset * 2., cx);
        if (available - self.width).abs() >= STEADY {
            self.width = available;
        }
        let context_menu = self.context_menu.clone().map(|(item, position)| {
            let menu = item.menu(&self.menus, self.playback.clone(), false, cx);
            Popup::new(position, menu).on_close(cx.listener(|this, _, _, cx| {
                this.context_menu = None;
                cx.notify();
            }))
        });

        let discover = self.discover.read(cx);
        let featured = discover.featured();
        let cover = featured
            .as_ref()
            .and_then(|featured| featured::cover(featured, discover));
        let releases = discover.releases();
        let mixes = discover.mixes();
        self.sample(cover, cx);
        let tint = self.tint.as_ref().and_then(|(_, tint)| *tint);
        let hero = featured.as_ref().and_then(|featured| {
            featured::hero(featured, self.discover.read(cx), tint, &self.playback, cx)
        });

        let resume = self.resume(available, cx);
        let view = cx.entity();
        let picks = Lazy::new("home-quick-picks", move |_, cx| {
            view.update(cx, |this, cx| this.quick_picks(available, cx))
        });
        let width = self.width;

        let fresh: Vec<GenreSection> = (!releases.is_empty())
            .then(|| GenreSection {
                title: t!("home-new-releases").to_string(),
                items: releases.iter().cloned().map(GenreItem::Album).collect(),
                provider: None,
            })
            .into_iter()
            .collect();
        let fresh = (!fresh.is_empty()).then(|| {
            self.fresh.update(cx, |shelves, cx| {
                shelves.render(Rc::new(fresh), Mode::Grid, width, window, cx)
            })
        });

        let feed = self.home.read(cx).sections();
        let feeding = self.home.read(cx).is_feeding();
        let mut lower: Vec<GenreSection> = mixes
            .iter()
            .map(|mix| GenreSection {
                title: t!("home-because", artist = mix.seed.artists.as_str()).to_string(),
                items: mix.tracks.iter().cloned().map(GenreItem::Track).collect(),
                provider: None,
            })
            .collect();
        lower.extend(feed.iter().cloned());
        let shelves = self
            .shelves
            .update(cx, |shelves, cx| match lower.is_empty() && feeding {
                true => shelves.pending(width, cx),
                false => vec![shelves.render(Rc::new(lower), Mode::Grid, width, window, cx)],
            });

        Scroller::new("home-page", &self.scrollbar)
            .bottom_inset(crate::chrome::Chrome::bottom(cx))
            .p(theme.metrics.inset)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .children(hero)
                    .children(resume)
                    .children(fresh)
                    .child(picks)
                    .children(shelves),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
            .into_any_element()
    }
}

fn open_item_menu(
    home: &WeakEntity<HomeView>,
    items: &Rc<Vec<GenreItem>>,
    place: usize,
    event: &MouseDownEvent,
    cx: &mut gpui::App,
) {
    let Some(item) = items.get(place).and_then(Item::of) else {
        return;
    };
    let Some(home) = home.upgrade() else {
        return;
    };
    home.update(cx, |this, cx| {
        this.menus.reset(cx);
        this.context_menu = Some((item, event.position));
        cx.notify();
    });
}
