use std::any::TypeId;
use std::panic::Location;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Action, AnyElement, App, Bounds, Context, DefiniteLength, DispatchPhase, Edges,
    EdgesRefinement, Element, ElementId, Entity, GlobalElementId, InspectorElementId,
    InteractiveElement as _, IntoElement, IsZero, LayoutId, MouseButton, ParentElement as _,
    Pixels, Rems, RenderOnce, StyleRefinement, Styled, TextAlign, Visibility, WeakEntity, Window,
    div, px, relative,
};

use crate::button::{Button, ButtonVariants as _};
use crate::input::clear_button;
use crate::input::element::{LINE_NUMBER_RIGHT_MARGIN, RIGHT_MARGIN};
use crate::scroll::Scrollbar;
use crate::spinner::Spinner;
use crate::{ActiveTheme, v_flex};
use crate::{IconName, Size};
use crate::{Selectable, StyledExt, h_flex};
use crate::{Sizable, StyleSized};

use super::InputState;

/// A text input element bind to an [`InputState`].
#[derive(IntoElement)]
pub struct Input {
    state: Entity<InputState>,
    style: StyleRefinement,
    size: Size,
    prefix: Option<AnyElement>,
    suffix: Option<AnyElement>,
    height: Option<DefiniteLength>,
    appearance: bool,
    cleanable: bool,
    mask_toggle: bool,
    disabled: bool,
    bordered: bool,
    focus_bordered: bool,
    tab_index: isize,
    selected: bool,
}

impl Sizable for Input {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl Selectable for Input {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Input {
    /// Create a new [`Input`] element bind to the [`InputState`].
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            size: Size::default(),
            style: StyleRefinement::default(),
            prefix: None,
            suffix: None,
            height: None,
            appearance: true,
            cleanable: false,
            mask_toggle: false,
            disabled: false,
            bordered: true,
            focus_bordered: true,
            tab_index: 0,
            selected: false,
        }
    }

    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_any_element());
        self
    }

    pub fn suffix(mut self, suffix: impl IntoElement) -> Self {
        self.suffix = Some(suffix.into_any_element());
        self
    }

    /// Set full height of the input (Multi-line only).
    pub fn h_full(mut self) -> Self {
        self.height = Some(relative(1.));
        self
    }

    /// Set height of the input (Multi-line only).
    pub fn h(mut self, height: impl Into<DefiniteLength>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Set the appearance of the input field, if false the input field will no border, background.
    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    /// Set the bordered for the input, default: true
    pub fn bordered(mut self, bordered: bool) -> Self {
        self.bordered = bordered;
        self
    }

    /// Set focus border for the input, default is true.
    pub fn focus_bordered(mut self, bordered: bool) -> Self {
        self.focus_bordered = bordered;
        self
    }

    /// Set whether to show the clear button when the input field is not empty, default is false.
    pub fn cleanable(mut self, cleanable: bool) -> Self {
        self.cleanable = cleanable;
        self
    }

    /// Set to enable toggle button for password mask state.
    pub fn mask_toggle(mut self) -> Self {
        self.mask_toggle = true;
        self
    }

    /// Set to disable the input field.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Set the tab index for the input, default is 0.
    pub fn tab_index(mut self, index: isize) -> Self {
        self.tab_index = index;
        self
    }

    fn render_toggle_mask_button(state: Entity<InputState>) -> impl IntoElement {
        Button::new("toggle-mask")
            .icon(IconName::Eye)
            .xsmall()
            .ghost()
            .tab_stop(false)
            .on_mouse_down(MouseButton::Left, {
                let state = state.clone();
                move |_, window, cx| {
                    state.update(cx, |state, cx| {
                        state.set_masked(false, window, cx);
                    })
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let state = state.clone();
                move |_, window, cx| {
                    state.update(cx, |state, cx| {
                        state.set_masked(true, window, cx);
                    })
                }
            })
    }

    /// This method must after the refine_style.
    fn render_editor(
        paddings: EdgesRefinement<DefiniteLength>,
        input_state: &Entity<InputState>,
        state: &InputState,
        window: &Window,
        _cx: &App,
    ) -> impl IntoElement {
        let base_size = window.text_style().font_size;
        let rem_size = window.rem_size();

        let paddings = Edges {
            left: paddings
                .left
                .map(|v| v.to_pixels(base_size, rem_size))
                .unwrap_or(px(0.)),
            right: paddings
                .right
                .map(|v| v.to_pixels(base_size, rem_size))
                .unwrap_or(px(0.)),
            top: paddings
                .top
                .map(|v| v.to_pixels(base_size, rem_size))
                .unwrap_or(px(0.)),
            bottom: paddings
                .bottom
                .map(|v| v.to_pixels(base_size, rem_size))
                .unwrap_or(px(0.)),
        };

        v_flex()
            .size_full()
            .children(state.search_panel.clone())
            .child(div().flex_1().child(input_state.clone()).map(|this| {
                if let Some(last_layout) = state.last_layout.as_ref() {
                    let left = if last_layout.line_number_width.is_zero() {
                        px(0.)
                    } else {
                        // Align left edge to the Line number.
                        paddings.left + last_layout.line_number_width - LINE_NUMBER_RIGHT_MARGIN
                    };

                    let scroll_size = gpui::Size {
                        width: state.scroll_size.width - left + paddings.right + RIGHT_MARGIN,
                        height: state.scroll_size.height,
                    };

                    let scrollbar = if !state.soft_wrap {
                        Scrollbar::new(&state.scroll_handle)
                    } else {
                        Scrollbar::vertical(&state.scroll_handle)
                    };

                    this.relative().child(
                        div()
                            .absolute()
                            .top(-paddings.top)
                            .left(left)
                            .right(-paddings.right)
                            .bottom(-paddings.bottom)
                            .child(scrollbar.scroll_size(scroll_size)),
                    )
                } else {
                    this
                }
            }))
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        const LINE_HEIGHT: Rems = Rems(1.25);
        let text_align = self.style.text.text_align.unwrap_or(TextAlign::Left);

        // Only update when something changes: an update while drawing counts
        // as a change to the input state view, which would then be built
        // again on the next frame.
        let changed = {
            let state = self.state.read(cx);
            state.disabled != self.disabled
                || state.size != self.size
                || (state.mode.is_single_line() && state.text_align != text_align)
        };
        if changed {
            self.state.update(cx, |state, _| {
                state.disabled = self.disabled;
                state.size = self.size;
                // Only for single line mode
                if state.mode.is_single_line() {
                    state.text_align = text_align;
                }
            });
        }

        let state = self.state.read(cx);
        let focused = state.focus_handle.is_focused(window) && !state.disabled;
        let gap_x = match self.size {
            Size::Small => px(4.),
            Size::Large => px(8.),
            _ => px(6.),
        };

        let bg = if state.disabled {
            cx.theme().muted
        } else {
            if state.mode.is_code_editor() {
                cx.theme().editor_background()
            } else {
                cx.theme().background
            }
        };

        let prefix = self.prefix;
        let suffix = self.suffix;
        let show_clear_button = self.cleanable
            && !state.disabled
            && !state.loading
            && state.text.len() > 0
            && state.mode.is_single_line();
        let has_suffix = suffix.is_some() || state.loading || self.mask_toggle || show_clear_button;
        let editable = !state.disabled;
        let multi_line = state.mode.is_multi_line();
        let hidden = self.style.visibility == Some(Visibility::Hidden);

        let input = div()
            .id(("input", self.state.entity_id()))
            .flex()
            .key_context(crate::input::CONTEXT)
            .track_focus(&state.focus_handle.clone())
            .tab_index(self.tab_index)
            .on_key_down(window.listener_for(&self.state, InputState::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&self.state, InputState::on_mouse_down),
            )
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(&self.state, InputState::on_mouse_down),
            )
            .on_mouse_up(
                MouseButton::Left,
                window.listener_for(&self.state, InputState::on_mouse_up),
            )
            .on_mouse_up(
                MouseButton::Right,
                window.listener_for(&self.state, InputState::on_mouse_up),
            )
            // A drag selection can be released anywhere — the autoscroll it may
            // have started has to end with it, not run on until the pointer
            // happens to come back over the input.
            .on_mouse_up_out(
                MouseButton::Left,
                window.listener_for(&self.state, InputState::on_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Right,
                window.listener_for(&self.state, InputState::on_mouse_up),
            )
            .on_mouse_move(window.listener_for(&self.state, InputState::on_mouse_move))
            .on_scroll_wheel(window.listener_for(&self.state, InputState::on_scroll_wheel))
            .size_full()
            .line_height(LINE_HEIGHT)
            .input_px(self.size)
            .input_py(self.size)
            .input_h(self.size)
            .input_text_size(self.size)
            .cursor_text()
            .items_center()
            .when(state.mode.is_multi_line(), |this| {
                this.h_auto()
                    .when_some(self.height, |this, height| this.h(height))
            })
            .when(self.appearance, |this| {
                this.bg(bg)
                    .rounded(cx.theme().radius)
                    .when(self.bordered, |this| {
                        this.border_color(cx.theme().input)
                            .border_1()
                            .when(cx.theme().shadow, |this| this.shadow_xs())
                            .when(focused && self.focus_bordered, |this| {
                                this.focused_border(cx)
                            })
                    })
            })
            .items_center()
            .gap(gap_x)
            .refine_style(&self.style)
            .children(prefix)
            .when(state.mode.is_multi_line(), |mut this| {
                let paddings = this.style().padding.clone();
                this.child(Self::render_editor(
                    paddings,
                    &self.state,
                    &state,
                    window,
                    cx,
                ))
            })
            .when(!state.mode.is_multi_line(), |this| {
                this.child(self.state.clone())
            })
            .when(has_suffix, |this| {
                this.pr(self.size.input_px()).child(
                    h_flex()
                        .id("suffix")
                        .gap(gap_x)
                        .when(self.appearance, |this| this.bg(bg))
                        .items_center()
                        .when(state.loading, |this| {
                            this.child(Spinner::new().color(cx.theme().muted_foreground))
                        })
                        .when(self.mask_toggle, |this| {
                            this.child(Self::render_toggle_mask_button(self.state.clone()))
                        })
                        .when(show_clear_button, |this| {
                            this.child(clear_button(cx).on_click({
                                let state = self.state.clone();
                                move |_, window, cx| {
                                    state.update(cx, |state, cx| {
                                        state.clean(window, cx);
                                        state.focus(window, cx);
                                    })
                                }
                            }))
                        })
                        .children(suffix),
                )
            });

        InputActions {
            state: self.state.downgrade(),
            editable,
            multi_line,
            hidden,
            child: input.into_any_element(),
        }
    }
}

struct InputActions {
    state: WeakEntity<InputState>,
    editable: bool,
    multi_line: bool,
    hidden: bool,
    child: AnyElement,
}

impl InputActions {
    fn on_action<A: Action>(
        &self,
        window: &mut Window,
        listener: impl Fn(&mut InputState, &A, &mut Window, &mut Context<InputState>) + 'static,
    ) {
        let state = self.state.clone();
        window.on_action(TypeId::of::<A>(), move |action, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if let Some(action) = action.downcast_ref::<A>() {
                state
                    .update(cx, |state, cx| listener(state, action, window, cx))
                    .ok();
            }
        });
    }

    fn register(&self, window: &mut Window) {
        if self.editable {
            self.on_action(window, InputState::backspace);
            self.on_action(window, InputState::delete);
            self.on_action(window, InputState::delete_to_beginning_of_line);
            self.on_action(window, InputState::delete_to_end_of_line);
            self.on_action(window, InputState::delete_previous_word);
            self.on_action(window, InputState::delete_next_word);
            self.on_action(window, InputState::enter);
            self.on_action(window, InputState::escape);
            self.on_action(window, InputState::paste);
            self.on_action(window, InputState::cut);
            self.on_action(window, InputState::undo);
            self.on_action(window, InputState::redo);
            if self.multi_line {
                self.on_action(window, InputState::indent_inline);
                self.on_action(window, InputState::outdent_inline);
                self.on_action(window, InputState::indent_block);
                self.on_action(window, InputState::outdent_block);
            }
            self.on_action(window, InputState::on_action_toggle_code_actions);
        }
        self.on_action(window, InputState::left);
        self.on_action(window, InputState::right);
        self.on_action(window, InputState::select_left);
        self.on_action(window, InputState::select_right);
        if self.multi_line {
            self.on_action(window, InputState::up);
            self.on_action(window, InputState::down);
            self.on_action(window, InputState::select_up);
            self.on_action(window, InputState::select_down);
            self.on_action(window, InputState::page_up);
            self.on_action(window, InputState::page_down);
            self.on_action(window, InputState::on_action_go_to_definition);
        }
        self.on_action(window, InputState::select_all);
        self.on_action(window, InputState::select_to_start_of_line);
        self.on_action(window, InputState::select_to_end_of_line);
        self.on_action(window, InputState::select_to_previous_word);
        self.on_action(window, InputState::select_to_next_word);
        self.on_action(window, InputState::home);
        self.on_action(window, InputState::end);
        self.on_action(window, InputState::move_to_start);
        self.on_action(window, InputState::move_to_end);
        self.on_action(window, InputState::move_to_previous_word);
        self.on_action(window, InputState::move_to_next_word);
        self.on_action(window, InputState::select_to_start);
        self.on_action(window, InputState::select_to_end);
        self.on_action(window, InputState::show_character_palette);
        self.on_action(window, InputState::copy);
        self.on_action(window, InputState::on_action_search);
    }
}

impl IntoElement for InputActions {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InputActions {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.hidden {
            self.register(window);
        }
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AppContext as _, Context, Entity, Render, TestAppContext, VisualTestContext};

    use super::*;
    use crate::input::{Backspace, SelectAll};

    struct InputView {
        state: Entity<InputState>,
        disabled: bool,
    }

    impl Render for InputView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(600.))
                .child(Input::new(&self.state).disabled(self.disabled))
        }
    }

    fn build(cx: &mut TestAppContext) -> (Entity<InputView>, &mut VisualTestContext) {
        cx.update(crate::init);
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let state = cx.new(|cx| InputState::new(window, cx).multi_line(true));
            let input_view = cx.new(|_| InputView {
                state,
                disabled: false,
            });
            view = Some(input_view.clone());
            crate::Root::new(input_view, window, cx)
        });
        (view.expect("input view"), cx)
    }

    fn value(view: &Entity<InputView>, cx: &mut VisualTestContext) -> String {
        view.read_with(cx, |view, cx| view.state.read(cx).value().to_string())
    }

    #[gpui::test]
    fn test_input_actions_dispatch(cx: &mut TestAppContext) {
        let (view, cx) = build(cx);
        let state = view.read_with(cx, |view, _| view.state.clone());
        let focus_handle = state.read_with(cx, |state, _| state.focus_handle.clone());
        state.update_in(cx, |state, window, cx| state.set_value("hello", window, cx));
        cx.run_until_parked();

        cx.update(|window, cx| {
            focus_handle.dispatch_action(&SelectAll, window, cx);
            focus_handle.dispatch_action(&Backspace, window, cx);
        });
        assert_eq!(value(&view, cx), "");

        state.update_in(cx, |state, window, cx| state.focus(window, cx));
        cx.run_until_parked();
        cx.simulate_input("abc");
        cx.simulate_keystrokes("backspace");
        assert_eq!(value(&view, cx), "ab");

        view.update(cx, |view, cx| {
            view.disabled = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            focus_handle.dispatch_action(&SelectAll, window, cx);
            focus_handle.dispatch_action(&Backspace, window, cx);
        });
        assert_eq!(value(&view, cx), "ab");
    }
}
