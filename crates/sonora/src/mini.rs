use gpui::{
    App, AppContext as _, Bounds, Global, Pixels, Point, Size, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowHandle, WindowKind, WindowOptions, point, px, size,
};
use state::{MiniCorner, Sonora};
use ui::ActiveTheme as _;
use views::MiniPlayer;

const MINI_SIZE: Size<Pixels> = size(px(360.), px(132.));
const INSET: Pixels = px(24.);

#[derive(Default)]
struct Mini(Option<WindowHandle<MiniPlayer>>);

impl Global for Mini {}

/// The window kind that keeps the mini player above other applications.
///
/// macOS gives `Floating` a real floating window level, Windows gives `PopUp`
/// the `WS_EX_TOPMOST` style; both stay under the window manager, so both stay
/// draggable. X11 has neither, and its `PopUp` is override-redirect — above
/// everything, but no longer managed, so no dragging and no focus. Linux
/// therefore opens an ordinary window and asks the window manager to raise it,
/// in [`crate::above::raise`].
#[cfg(target_os = "macos")]
const ABOVE: WindowKind = WindowKind::Floating;
#[cfg(windows)]
const ABOVE: WindowKind = WindowKind::PopUp;
#[cfg(not(any(target_os = "macos", windows)))]
const ABOVE: WindowKind = WindowKind::Normal;

fn kind(on_top: bool) -> WindowKind {
    match on_top {
        true => ABOVE,
        false => WindowKind::Normal,
    }
}

fn placement(corner: MiniCorner, cx: &App) -> Bounds<Pixels> {
    let screen = cx
        .primary_display()
        .map(|display| display.bounds())
        .unwrap_or_else(|| Bounds {
            origin: Point::default(),
            size: size(px(1920.), px(1080.)),
        });
    let left = screen.origin.x + INSET;
    let right = screen.origin.x + screen.size.width - MINI_SIZE.width - INSET;
    let top = screen.origin.y + INSET;
    let bottom = screen.origin.y + screen.size.height - MINI_SIZE.height - INSET;

    let origin = match corner {
        MiniCorner::TopLeft => point(left, top),
        MiniCorner::TopRight => point(right, top),
        MiniCorner::BottomLeft => point(left, bottom),
        MiniCorner::BottomRight => point(right, bottom),
    };

    Bounds {
        origin,
        size: MINI_SIZE,
    }
}

/// Reopens an open mini player whenever its corner or window kind changes.
pub fn watch(cx: &mut App) {
    let settings = Sonora::global(cx).settings.clone();
    let held = settings.read(cx);
    let mut placed = (held.mini_corner(), held.mini_on_top());

    cx.observe(&settings, move |settings, cx| {
        let held = settings.read(cx);
        let chosen = (held.mini_corner(), held.mini_on_top());
        if chosen == placed {
            return;
        }
        placed = chosen;
        reposition(cx);
    })
    .detach();
}

pub fn is_open(cx: &App) -> bool {
    cx.try_global::<Mini>().is_some_and(|mini| mini.0.is_some())
}

pub fn toggle(cx: &mut App) {
    match is_open(cx) {
        true => close(cx),
        false => open(cx),
    }
}

pub fn close(cx: &mut App) {
    let Some(handle) = cx.try_global::<Mini>().and_then(|mini| mini.0) else {
        return;
    };
    cx.set_global(Mini(None));
    handle
        .update(cx, |_, window, _| window.remove_window())
        .ok();
}

/// Reopens the window, because gpui can size a window but not move one.
pub fn reposition(cx: &mut App) {
    if !is_open(cx) {
        return;
    }
    close(cx);
    open(cx);
}

pub fn open(cx: &mut App) {
    if is_open(cx) {
        return;
    }
    let settings = Sonora::global(cx).settings.read(cx);
    let (corner, on_top) = (settings.mini_corner(), settings.mini_on_top());
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(placement(corner, cx))),
            titlebar: Some(TitlebarOptions {
                title: Some("Sonora".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_decorations: Some(WindowDecorations::Client),
            kind: kind(on_top),
            is_movable: true,
            is_resizable: false,
            is_minimizable: false,
            focus: false,
            app_id: Some("sonora".into()),
            ..Default::default()
        },
        |window, cx| {
            window.set_rem_size(cx.theme().font_size);
            cx.new(MiniPlayer::new)
        },
    );

    match opened {
        Ok(handle) => {
            if on_top {
                // after the window is mapped: the window manager drops a state
                // request aimed at a window it does not know yet
                handle
                    .update(cx, |_, window, _| crate::above::raise(window))
                    .ok();
            }
            cx.set_global(Mini(Some(handle)));
            cx.observe_release(&handle.entity(cx).unwrap(), |_, cx| {
                cx.set_global(Mini(None));
            })
            .detach();
        }
        Err(error) => log::warn!("mini: cannot open the window: {error:#}"),
    }
}
