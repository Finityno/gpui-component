use std::time::Duration;

use gpui::{Context, Pixels, Task, px};

static INTERVAL: Duration = Duration::from_millis(500);
static PAUSE_DELAY: Duration = Duration::from_millis(300);

// Keep caret width consistent across platforms.
#[cfg(not(target_os = "macos"))]
pub(super) const CURSOR_WIDTH: Pixels = px(1.5);
#[cfg(target_os = "macos")]
pub(super) const CURSOR_WIDTH: Pixels = px(1.5);

/// To manage the Input cursor blinking.
///
/// It will start blinking with a interval of 500ms.
/// Every loop will notify the view to update the `visible`, and Input will observe this update to touch repaint.
///
/// The input painter will check if this in visible state, then it will draw the cursor.
pub(crate) struct BlinkCursor {
    visible: bool,
    paused: bool,
    /// Whether the toggle task is running, so that starting again (window
    /// activation and visibility can both report one return) leaves it be.
    blinking: bool,
    /// Whether the window is in the background or hidden: the cursor is
    /// drawn and held still, with no timer, until resumed.
    suspended: bool,
    epoch: usize,

    _task: Task<()>,
}

impl BlinkCursor {
    pub fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            blinking: false,
            suspended: false,
            epoch: 0,
            _task: Task::ready(()),
        }
    }

    /// Start the blinking
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.suspended {
            if !self.visible {
                self.visible = true;
                cx.notify();
            }
            return;
        }
        if self.paused {
            // The pause timer resumes blinking.
            self.visible = true;
            return;
        }
        if self.blinking {
            return;
        }
        // Shown at once, then toggled every interval.
        if !self.visible {
            self.visible = true;
            cx.notify();
        }
        self.toggle_every_interval(cx);
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch = 0;
        // Cancels a pause timer too, which would have cleared `paused`.
        self._task = Task::ready(());
        self.paused = false;
        self.blinking = false;
        // Drawn while still: a window losing activation blurs its focused
        // input, whose caret stays drawn in the background window.
        self.visible = true;
        cx.notify();
    }

    /// Stop blinking and keep the cursor drawn until the next `start`.
    pub fn hold_visible(&mut self, cx: &mut Context<Self>) {
        self.next_epoch();
        self._task = Task::ready(());
        self.blinking = false;
        // Canceling the pause timer must not leave reactivation paused.
        self.paused = false;
        if !self.visible {
            self.visible = true;
            cx.notify();
        }
    }

    /// Holds the cursor drawn and still while the window is in the
    /// background or hidden, through any `start` or pause meanwhile; or lets
    /// the next `start` blink again.
    pub fn set_suspended(&mut self, suspended: bool, cx: &mut Context<Self>) {
        self.suspended = suspended;
        if suspended {
            self.hold_visible(cx);
        }
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    /// Toggles the cursor every [`INTERVAL`] from now, in one task that
    /// sleeps between toggles: each blink costs one timer wake and one
    /// notification, and nothing in between.
    fn toggle_every_interval(&mut self, cx: &mut Context<Self>) {
        let epoch = self.next_epoch();
        self.blinking = true;
        self._task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(INTERVAL).await;
                let toggled = this.update(cx, |this, cx| {
                    if this.paused || this.epoch != epoch {
                        this.visible = true;
                        this.blinking = false;
                        return false;
                    }
                    this.visible = !this.visible;
                    cx.notify();
                    true
                });
                if !matches!(toggled, Ok(true)) {
                    break;
                }
            }
        });
    }

    pub fn visible(&self) -> bool {
        // Keep showing the cursor if paused
        self.paused || self.visible
    }

    /// Pause the blinking, and delay to resume the blinking.
    ///
    /// After the delay the cursor stays visible for one full blink interval
    /// before toggling, so it doesn't flash off immediately after a text edit.
    pub fn pause(&mut self, cx: &mut Context<Self>) {
        if self.suspended {
            self.hold_visible(cx);
            return;
        }
        self.paused = true;
        self.blinking = false;
        self.visible = true;
        cx.notify();

        // Advance epoch to cancel any in-flight blink task.
        self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE_DELAY).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.paused = false;
                    // Keep visible and schedule the first toggle after a full
                    // interval so the cursor doesn't disappear right away.
                    // Visible already (pausing showed it), so nothing changes
                    // on screen and nothing is notified.
                    this.visible = true;
                    if !this.suspended {
                        this.toggle_every_interval(cx);
                    }
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{BlinkCursor, INTERVAL};
    use gpui::{AppContext as _, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn holding_the_cursor_visible_stops_the_blink_timer(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update({
            let notifications = notifications.clone();
            |cx| {
                cx.observe(&cursor, move |_, _| {
                    notifications.set(notifications.get() + 1)
                })
            }
        });

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() >= 4, "the cursor blinks while running");

        cursor.update(cx, |cursor, cx| cursor.hold_visible(cx));
        cx.run_until_parked();
        let held_at = notifications.get();
        cx.executor().advance_clock(INTERVAL * 20);
        cx.run_until_parked();
        assert_eq!(
            notifications.get(),
            held_at,
            "a held cursor schedules no blinks"
        );
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() > held_at, "start resumes blinking");
    }

    #[gpui::test]
    fn holding_a_paused_cursor_resumes_after_reactivation(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update({
            let notifications = notifications.clone();
            |cx| {
                cx.observe(&cursor, move |_, _| {
                    notifications.set(notifications.get() + 1)
                })
            }
        });

        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cursor.update(cx, |cursor, cx| cursor.hold_visible(cx));
        cx.run_until_parked();
        let held_at = notifications.get();
        cx.executor().advance_clock(INTERVAL * 20);
        cx.run_until_parked();
        assert_eq!(notifications.get(), held_at);
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.executor().advance_clock(INTERVAL * 4);
        cx.run_until_parked();
        assert!(notifications.get() > held_at, "reactivation resumes blinking");
    }

    #[gpui::test]
    fn blinking_toggles_once_per_interval_and_starting_again_changes_nothing(
        cx: &mut TestAppContext,
    ) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let notifications = Rc::new(Cell::new(0));
        let _subscription = cx.update({
            let notifications = notifications.clone();
            |cx| {
                cx.observe(&cursor, move |_, _| {
                    notifications.set(notifications.get() + 1)
                })
            }
        });

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()), "shown at once");
        let started = notifications.get();
        // Window activation and visibility can both report the same return.
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert_eq!(notifications.get(), started, "a second start does not toggle");
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));

        for blink in 1..=6 {
            cx.executor().advance_clock(INTERVAL);
            cx.run_until_parked();
            assert_eq!(notifications.get(), started + blink, "one toggle per interval");
            assert_eq!(
                cursor.read_with(cx, |cursor, _| cursor.visible()),
                blink % 2 == 0
            );
        }

        // A focus after a blur shows the cursor at once.
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        assert!(cursor.read_with(cx, |cursor, _| cursor.visible()));
    }

    #[gpui::test]
    fn stopping_during_a_pause_does_not_leave_the_cursor_paused(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| {
            cursor.start(cx);
            cursor.pause(cx);
            cursor.stop(cx);
            cursor.start(cx);
        });
        let shown = cursor.read_with(cx, |cursor, _| cursor.visible());
        cx.executor().advance_clock(INTERVAL);
        cx.run_until_parked();
        assert_ne!(
            cursor.read_with(cx, |cursor, _| cursor.visible()),
            shown,
            "blinks after the pause was cut short"
        );
    }
}
