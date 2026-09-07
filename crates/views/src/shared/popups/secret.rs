use std::rc::Rc;

use gpui::{App, Entity, IntoElement, RenderOnce, Window, px};
use gpui::{div, prelude::*};
use i18n::t;
use ui::{ActiveTheme as _, Button, Input, Modal, Text};

use crate::shared::steps::steps;

type Submit = Rc<dyn Fn(&(), &mut Window, &mut App)>;
type Cancel = Rc<dyn Fn(&(), &mut Window, &mut App)>;

#[derive(IntoElement)]
pub(crate) struct SecretPrompt {
    secret: Entity<Input>,
    keys: crate::shared::Secret,
    submit: Option<Submit>,
    cancel: Option<Cancel>,
}

impl SecretPrompt {
    pub(crate) fn new(secret: Entity<Input>, slug: &str) -> Self {
        Self {
            secret,
            keys: crate::shared::secret(slug),
            submit: None,
            cancel: None,
        }
    }

    pub(crate) fn on_submit(
        mut self,
        handler: impl Fn(&(), &mut Window, &mut App) + 'static,
    ) -> Self {
        self.submit = Some(Rc::new(handler));
        self
    }

    pub(crate) fn on_cancel(
        mut self,
        handler: impl Fn(&(), &mut Window, &mut App) + 'static,
    ) -> Self {
        self.cancel = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SecretPrompt {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            secret,
            keys,
            submit,
            cancel,
        } = self;
        let dismissed = cancel.clone();
        let theme = *cx.theme();

        Modal::new("secret-prompt", i18n::lookup(keys.title, None))
            .w(px(560.))
            .child(steps(keys.steps.map(|key| i18n::lookup(key, None))))
            .child(
                div()
                    .child(i18n::lookup(keys.note, None))
                    .flex_1()
                    .min_w_0()
                    .text_size(theme.text(Text::Small))
                    .text_color(theme.muted_foreground),
            )
            .child(secret)
            .action(
                Button::new("cancel-secret")
                    .ghost()
                    .label(t!("common-cancel"))
                    .on_click(move |_, window, cx| {
                        if let Some(cancel) = &cancel {
                            cancel(&(), window, cx);
                        }
                    }),
            )
            .action(
                Button::new("submit-secret")
                    .label(t!("login-cookie-submit"))
                    .primary()
                    .on_click(move |_, window, cx| {
                        if let Some(submit) = &submit {
                            submit(&(), window, cx);
                        }
                    }),
            )
            .on_dismiss(move |_, window, cx| {
                if let Some(dismissed) = &dismissed {
                    dismissed(&(), window, cx);
                }
            })
    }
}
