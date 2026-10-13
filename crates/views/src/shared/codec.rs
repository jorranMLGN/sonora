//! The one question before Cisco's OpenH264 is downloaded for a canvas. `state::Codec` decides
//! when to ask; this only draws it.

use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Render, Window, div};
use i18n::t;
use state::{Codec, CodecState, Sonora};
use ui::{ActiveTheme as _, Button, Dismiss, FORM_CONTEXT, Modal, Submit, Text};

pub(crate) struct CodecPrompt {
    codec: Entity<Codec>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    shown: bool,
}

impl CodecPrompt {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let codec = Sonora::global(cx).codec.clone();
        cx.observe(&codec, |_, _, cx| cx.notify()).detach();
        Self {
            codec,
            focus: cx.focus_handle(),
            restore: None,
            shown: false,
        }
    }

    fn decline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.release(window, cx);
        self.codec.update(cx, |codec, cx| codec.dismiss(cx));
    }

    fn proceed(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.codec.read(cx).state() == CodecState::Wanted {
            self.codec.update(cx, |codec, cx| codec.install(cx));
        }
    }

    fn release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(focus) = self.restore.take() {
            window.focus(&focus, cx);
        }
        self.shown = false;
    }
}

impl Render for CodecPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.codec.read(cx).state();
        let open = self.codec.read(cx).prompting();
        if !open {
            if self.shown {
                self.release(window, cx);
            }
            return div().into_any_element();
        }
        if !self.shown {
            self.restore = window.focused(cx);
            window.focus(&self.focus, cx);
            self.shown = true;
        }

        let theme = *cx.theme();
        let small = theme.text(Text::Small);
        let installing = state == CodecState::Installing;

        div()
            .absolute()
            .inset_0()
            .key_context(FORM_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, window, cx| {
                cx.stop_propagation();
                this.decline(window, cx);
            }))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                cx.stop_propagation();
                this.proceed(window, cx);
            }))
            .child(
                Modal::new("canvas-prompt", t!("canvas-prompt-title"))
                    .detail(t!("canvas-prompt-detail"))
                    .when(installing, |modal| {
                        modal.child(
                            div()
                                .text_size(small)
                                .text_color(theme.muted_foreground)
                                .child(t!("canvas-prompt-installing")),
                        )
                    })
                    .action(
                        Button::new("decline-canvas")
                            .ghost()
                            .label(t!("canvas-prompt-later"))
                            .on_click(cx.listener(|this, _, window, cx| this.decline(window, cx))),
                    )
                    .action(
                        Button::new("proceed-canvas")
                            .primary()
                            .label(t!("canvas-prompt-download"))
                            .disabled(installing)
                            .on_click(cx.listener(|this, _, window, cx| this.proceed(window, cx))),
                    )
                    .on_dismiss(cx.listener(|this, _, window, cx| this.decline(window, cx))),
            )
            .into_any_element()
    }
}
