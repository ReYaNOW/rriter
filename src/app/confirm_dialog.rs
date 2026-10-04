// The unsaved-changes confirmation flow: one state machine that owns the dialog
// surface (second window or headless frame), the armed action, the Save-As
// queue and the "ready to run" hand-off to `about_to_wait`. `impl App` only
// routes events here and runs the cross-feature effects (window creation,
// saving files, running the action).

use glutin::surface::{Surface, WindowSurface};
use std::sync::Arc;
use winit::window::Window;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PendingAction {
    None,
    Quit,
    OpenFile,
    OpenLinkedFile,
    CloseFile,
    CloseTab(usize),
    CloseAllTabs,
    ResetKeymap,
}

/// Where the question is shown. Dropped together with its phase, so "dialog
/// visible" can never disagree with "action armed".
enum DialogSurface {
    /// Armed; the host loop has not created the window yet (`about_to_wait`
    /// owns the event loop, see `ConfirmDialog::needs_window`).
    Requested,
    /// Windowed mode: second OS window plus its GL surface, never one without the other.
    Window {
        window: Arc<Window>,
        gl: Surface<WindowSurface>,
    },
    /// Headless: drawn into the main frame.
    Frame,
}

#[derive(Default)]
enum ConfirmPhase {
    #[default]
    Idle,
    /// The question is on screen (or requested for the next host-loop turn).
    Asking {
        action: PendingAction,
        surface: DialogSurface,
    },
    /// Dialog closed, the native Save-As picker works for `action`; `queue`
    /// holds the tab indices still to save, head first (empty for a single document).
    SavingAs {
        action: PendingAction,
        queue: Vec<usize>,
    },
    /// Saving/discarding finished; `about_to_wait` takes the action and runs it.
    Ready { action: PendingAction },
}

/// The confirmation flow. Every transition replaces the whole phase, so a
/// hidden dialog with a still-armed action is unrepresentable.
#[derive(Default)]
pub struct ConfirmDialog {
    phase: ConfirmPhase,
}

impl ConfirmDialog {
    /// Test fixture: a question already drawn in the frame for `action`.
    #[cfg(test)]
    pub(crate) fn armed_with(action: PendingAction) -> Self {
        let mut dialog = Self::default();
        dialog.request(action);
        dialog.attach_frame();
        dialog
    }

    /// The dialog blocks the main window: its second window exists, or the
    /// headless frame draws it. Same as the former `dialog_window.is_some() ||
    /// headless_dialog_open`: false while the window is only requested
    /// (`CloseTab` until `about_to_wait` creates it), while the Save-As picker
    /// is open and once the answer is ready to run.
    pub fn is_open(&self) -> bool {
        matches!(
            self.phase,
            ConfirmPhase::Asking {
                surface: DialogSurface::Window { .. } | DialogSurface::Frame,
                ..
            }
        )
    }

    /// The armed action; `PendingAction::None` only when no flow runs.
    pub fn action(&self) -> PendingAction {
        match &self.phase {
            ConfirmPhase::Idle => PendingAction::None,
            ConfirmPhase::Asking { action, .. }
            | ConfirmPhase::SavingAs { action, .. }
            | ConfirmPhase::Ready { action } => *action,
        }
    }

    pub fn window(&self) -> Option<&Arc<Window>> {
        match &self.phase {
            ConfirmPhase::Asking {
                surface: DialogSurface::Window { window, .. },
                ..
            } => Some(window),
            _ => None,
        }
    }

    pub fn gl_surface(&self) -> Option<&Surface<WindowSurface>> {
        match &self.phase {
            ConfirmPhase::Asking {
                surface: DialogSurface::Window { gl, .. },
                ..
            } => Some(gl),
            _ => None,
        }
    }

    /// Headless: the dialog is drawn over the main frame and answers the `dialog` command.
    pub fn drawn_in_frame(&self) -> bool {
        matches!(
            self.phase,
            ConfirmPhase::Asking {
                surface: DialogSurface::Frame,
                ..
            }
        )
    }

    /// The Save-As picker's result belongs to this flow.
    pub fn waiting_for_save_as(&self) -> bool {
        matches!(self.phase, ConfirmPhase::SavingAs { .. })
    }

    /// The action whose dialog is armed but has no surface yet.
    pub fn needs_window(&self) -> Option<PendingAction> {
        match &self.phase {
            ConfirmPhase::Asking {
                action,
                surface: DialogSurface::Requested,
            } => Some(*action),
            _ => None,
        }
    }

    /// Idle -> asking `action` (surface still to attach). `false` in any other
    /// phase and for `PendingAction::None`; the current flow is left untouched.
    pub fn request(&mut self, action: PendingAction) -> bool {
        if action == PendingAction::None || !matches!(self.phase, ConfirmPhase::Idle) {
            return false;
        }
        self.phase = ConfirmPhase::Asking {
            action,
            surface: DialogSurface::Requested,
        };
        true
    }

    /// A new question while no question is on screen but a flow still runs
    /// (dialog requested but not shown, Save-As picker open, answer not yet
    /// run): the old flow is dropped whole and `action` is asked instead.
    /// `false` when idle (use `request`), when a question is on screen (it
    /// keeps its own action) and for `PendingAction::None`.
    pub fn supersede(&mut self, action: PendingAction) -> bool {
        if action == PendingAction::None {
            return false;
        }
        match self.phase {
            ConfirmPhase::Asking {
                surface: DialogSurface::Requested,
                ..
            }
            | ConfirmPhase::SavingAs { .. }
            | ConfirmPhase::Ready { .. } => {
                self.phase = ConfirmPhase::Asking {
                    action,
                    surface: DialogSurface::Requested,
                };
                true
            }
            ConfirmPhase::Idle | ConfirmPhase::Asking { .. } => false,
        }
    }

    /// Shows the requested question in its own window; ignored unless requested.
    pub fn attach_window(&mut self, window: Arc<Window>, gl: Surface<WindowSurface>) {
        self.attach(DialogSurface::Window { window, gl });
    }

    /// Shows the requested question in the headless frame; ignored unless requested.
    pub fn attach_frame(&mut self) {
        self.attach(DialogSurface::Frame);
    }

    fn attach(&mut self, shown: DialogSurface) {
        if let ConfirmPhase::Asking { surface, .. } = &mut self.phase
            && matches!(surface, DialogSurface::Requested)
        {
            *surface = shown;
        }
    }

    /// Asking -> Save-As for `queue` (tab indices to save, head first); the
    /// dialog surface is dropped with the phase. No-op when idle or ready.
    pub fn begin_save_as(&mut self, queue: Vec<usize>) {
        if let ConfirmPhase::Asking { action, .. } | ConfirmPhase::SavingAs { action, .. } =
            self.phase
        {
            self.phase = ConfirmPhase::SavingAs { action, queue };
        }
    }

    /// The next tab the Save-As flow has to save.
    pub fn save_as_target(&self) -> Option<usize> {
        match &self.phase {
            ConfirmPhase::SavingAs { queue, .. } => queue.first().copied(),
            _ => None,
        }
    }

    /// `index` is saved (or needs no save): drop it from the queue.
    pub fn finish_save_as_target(&mut self, index: usize) {
        if let ConfirmPhase::SavingAs { queue, .. } = &mut self.phase {
            queue.retain(|queued| *queued != index);
        }
    }

    /// Save-As could not run (picker failed to start or vanished): the save
    /// flow is cancelled whole. Returns whether a save flow was running; any
    /// other phase is left untouched.
    pub fn abort_save_as(&mut self) -> bool {
        if !self.waiting_for_save_as() {
            return false;
        }
        self.phase = ConfirmPhase::Idle;
        true
    }

    /// Saved or discarded: the action runs on the next `about_to_wait`; the
    /// surface and the queue are dropped. No-op when idle.
    pub fn mark_ready(&mut self) {
        if !matches!(self.phase, ConfirmPhase::Idle) {
            let action = self.action();
            self.phase = ConfirmPhase::Ready { action };
        }
    }

    /// Any phase -> idle: the dialog, the action and the queue are dropped together.
    pub fn cancel(&mut self) {
        self.phase = ConfirmPhase::Idle;
    }

    /// Ready -> idle, returning the action to run.
    pub fn take_ready(&mut self) -> Option<PendingAction> {
        let ConfirmPhase::Ready { action } = self.phase else {
            return None;
        };
        self.phase = ConfirmPhase::Idle;
        Some(action)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfirmDialog, PendingAction};

    fn assert_idle(dialog: &ConfirmDialog) {
        assert_eq!(dialog.action(), PendingAction::None);
        assert!(!dialog.is_open());
        assert!(!dialog.drawn_in_frame());
        assert!(!dialog.waiting_for_save_as());
        assert_eq!(dialog.needs_window(), None);
        assert_eq!(dialog.save_as_target(), None);
    }

    fn saving_as(action: PendingAction, queue: Vec<usize>) -> ConfirmDialog {
        let mut dialog = ConfirmDialog::armed_with(action);
        dialog.begin_save_as(queue);
        dialog
    }

    fn ready(action: PendingAction) -> ConfirmDialog {
        let mut dialog = ConfirmDialog::armed_with(action);
        dialog.mark_ready();
        dialog
    }

    #[test]
    fn requested_dialog_is_not_open_until_a_surface_is_attached() {
        let mut dialog = ConfirmDialog::default();
        assert_idle(&dialog);
        assert!(dialog.request(PendingAction::CloseTab(2)));
        assert_eq!(dialog.needs_window(), Some(PendingAction::CloseTab(2)));
        assert_eq!(dialog.action(), PendingAction::CloseTab(2));
        assert!(!dialog.is_open());

        dialog.attach_frame();
        assert!(dialog.is_open());
        assert!(dialog.drawn_in_frame());
        assert_eq!(dialog.needs_window(), None);
        assert!(dialog.window().is_none());
        assert!(dialog.gl_surface().is_none());
    }

    #[test]
    fn attach_outside_a_request_changes_nothing() {
        let mut dialog = ConfirmDialog::default();
        dialog.attach_frame();
        assert_idle(&dialog);

        let mut dialog = saving_as(PendingAction::Quit, vec![0]);
        dialog.attach_frame();
        assert!(!dialog.is_open());
        assert!(dialog.waiting_for_save_as());
    }

    #[test]
    fn request_none_is_rejected_in_every_entry_point() {
        let mut dialog = ConfirmDialog::default();
        assert!(!dialog.request(PendingAction::None));
        assert_idle(&dialog);

        let mut dialog = saving_as(PendingAction::Quit, vec![1]);
        assert!(!dialog.supersede(PendingAction::None));
        assert_eq!(dialog.action(), PendingAction::Quit);
        assert_eq!(dialog.save_as_target(), Some(1));
    }

    /// B2: a second request while a question is on screen keeps the first action.
    #[test]
    fn request_while_asking_keeps_the_open_question() {
        let mut dialog = ConfirmDialog::armed_with(PendingAction::CloseTab(2));
        assert!(!dialog.request(PendingAction::Quit));
        assert!(!dialog.supersede(PendingAction::Quit));
        assert_eq!(dialog.action(), PendingAction::CloseTab(2));
        assert!(dialog.drawn_in_frame());

        dialog.mark_ready();
        assert_eq!(dialog.take_ready(), Some(PendingAction::CloseTab(2)));
        assert_idle(&dialog);
    }

    #[test]
    fn request_outside_idle_leaves_the_flow_untouched() {
        let mut dialog = saving_as(PendingAction::CloseAllTabs, vec![0, 1]);
        assert!(!dialog.request(PendingAction::Quit));
        assert_eq!(dialog.action(), PendingAction::CloseAllTabs);
        assert_eq!(dialog.save_as_target(), Some(0));

        let mut dialog = ready(PendingAction::CloseFile);
        assert!(!dialog.request(PendingAction::Quit));
        assert_eq!(dialog.take_ready(), Some(PendingAction::CloseFile));
    }

    #[test]
    fn supersede_replaces_a_flow_without_a_visible_question_whole() {
        let mut dialog = saving_as(PendingAction::CloseAllTabs, vec![0, 1]);
        assert!(dialog.supersede(PendingAction::Quit));
        assert_eq!(dialog.needs_window(), Some(PendingAction::Quit));
        assert!(!dialog.waiting_for_save_as());
        assert_eq!(dialog.save_as_target(), None);

        let mut dialog = ready(PendingAction::CloseFile);
        assert!(dialog.supersede(PendingAction::CloseTab(0)));
        assert_eq!(dialog.take_ready(), None);
        assert_eq!(dialog.needs_window(), Some(PendingAction::CloseTab(0)));

        let mut dialog = ConfirmDialog::default();
        assert!(dialog.request(PendingAction::CloseTab(1)));
        assert!(dialog.supersede(PendingAction::Quit));
        assert_eq!(dialog.needs_window(), Some(PendingAction::Quit));

        let mut dialog = ConfirmDialog::default();
        assert!(!dialog.supersede(PendingAction::Quit));
        assert_idle(&dialog);
    }

    #[test]
    fn cancel_from_every_phase_returns_to_idle() {
        let mut requested = ConfirmDialog::default();
        requested.request(PendingAction::CloseTab(0));
        let phases = [
            ConfirmDialog::default(),
            requested,
            ConfirmDialog::armed_with(PendingAction::Quit),
            saving_as(PendingAction::CloseAllTabs, vec![0, 3]),
            ready(PendingAction::OpenFile),
        ];
        for mut dialog in phases {
            dialog.cancel();
            assert_idle(&dialog);
            assert_eq!(dialog.take_ready(), None);
        }
    }

    #[test]
    fn save_as_queue_walks_head_first_then_becomes_ready() {
        let mut dialog = ConfirmDialog::armed_with(PendingAction::Quit);
        dialog.begin_save_as(vec![0, 2]);
        assert!(!dialog.is_open());
        assert!(dialog.waiting_for_save_as());
        assert_eq!(dialog.action(), PendingAction::Quit);
        assert_eq!(dialog.save_as_target(), Some(0));

        dialog.finish_save_as_target(0);
        assert_eq!(dialog.save_as_target(), Some(2));
        dialog.finish_save_as_target(2);
        assert_eq!(dialog.save_as_target(), None);
        assert!(dialog.waiting_for_save_as());

        dialog.mark_ready();
        assert!(!dialog.waiting_for_save_as());
        assert_eq!(dialog.take_ready(), Some(PendingAction::Quit));
        assert_idle(&dialog);
    }

    #[test]
    fn begin_save_as_without_a_flow_is_a_no_op() {
        let mut dialog = ConfirmDialog::default();
        dialog.begin_save_as(vec![0]);
        assert_idle(&dialog);

        let mut dialog = ready(PendingAction::CloseFile);
        dialog.begin_save_as(vec![0]);
        assert!(!dialog.waiting_for_save_as());
        assert_eq!(dialog.take_ready(), Some(PendingAction::CloseFile));
    }

    /// B4/B5: a failed or vanished Save-As picker cancels the save flow whole.
    #[test]
    fn abort_save_as_cancels_only_a_running_save_flow() {
        let mut dialog = saving_as(PendingAction::CloseAllTabs, vec![0, 1]);
        assert!(dialog.abort_save_as());
        assert_idle(&dialog);

        let mut dialog = ConfirmDialog::armed_with(PendingAction::CloseTab(1));
        assert!(!dialog.abort_save_as());
        assert!(dialog.drawn_in_frame());
        assert_eq!(dialog.action(), PendingAction::CloseTab(1));

        let mut dialog = ConfirmDialog::default();
        assert!(!dialog.abort_save_as());
        assert_idle(&dialog);
    }

    #[test]
    fn mark_ready_drops_surface_and_queue_and_ignores_idle() {
        let mut dialog = ConfirmDialog::default();
        dialog.mark_ready();
        assert_eq!(dialog.take_ready(), None);

        let mut dialog = ConfirmDialog::armed_with(PendingAction::CloseFile);
        dialog.mark_ready();
        assert!(!dialog.is_open());
        assert_eq!(dialog.action(), PendingAction::CloseFile);
        assert_eq!(dialog.take_ready(), Some(PendingAction::CloseFile));
        assert_eq!(dialog.take_ready(), None);
    }
}
