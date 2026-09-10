//! Keeps a window above other applications on X11.

/// Asks the window manager to raise `window` above other applications.
///
/// X11 has no window level, only the `_NET_WM_STATE_ABOVE` hint, and gpui
/// exposes no setter for it. Its `WindowKind::PopUp` reaches for
/// `override_redirect` instead, which does put the window above everything —
/// by taking it out of the window manager's hands altogether, and with it the
/// dragging, the focus and the titlebar. So the request goes out over a
/// short-lived connection of our own. It is addressed to the root window, so
/// any connection can carry it.
///
/// A no-op elsewhere: Wayland has no equivalent protocol, and macOS and
/// Windows have real window levels (see [`crate::mini::kind`]).
#[cfg(target_os = "linux")]
pub fn raise(window: &gpui::Window) {
    use anyhow::Context as _;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};

    /// `_NET_WM_STATE_ADD`, from the EWMH specification.
    const ADD: u32 = 1;
    /// The source indication a normal application sends, likewise.
    const APPLICATION: u32 = 1;

    let raised = || -> anyhow::Result<()> {
        let handle = HasWindowHandle::window_handle(window).context("this window has no handle")?;
        let RawWindowHandle::Xcb(handle) = handle.as_raw() else {
            return Ok(());
        };
        let (xcb, screen) = x11rb::connect(None)?;
        let root = xcb.setup().roots[screen].root;
        let state = xcb.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
        let above = xcb
            .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
            .reply()?
            .atom;

        xcb.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
            ClientMessageEvent::new(
                32,
                handle.window.get(),
                state,
                [ADD, above, 0, APPLICATION, 0],
            ),
        )?
        .check()?;
        xcb.flush()?;
        Ok(())
    };

    if let Err(error) = raised() {
        log::warn!("mini: cannot raise the window above others: {error:#}");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn raise(_window: &gpui::Window) {}
