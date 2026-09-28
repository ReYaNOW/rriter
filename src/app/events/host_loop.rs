use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event_loop::{ActiveEventLoop, ControlFlow};

pub struct HeadlessLoopState {
    pub exit_requested: AtomicBool,
    pub last_control_flow: Cell<ControlFlow>,
    /// A background result (highlight, hover, API, Database job) is still expected. The
    /// native loop just sleeps until the `UiWaker` event; `settle` must not call that idle.
    pub awaiting_background: Cell<bool>,
}

impl Default for HeadlessLoopState {
    fn default() -> Self {
        Self {
            exit_requested: AtomicBool::new(false),
            last_control_flow: Cell::new(ControlFlow::Wait),
            awaiting_background: Cell::new(false),
        }
    }
}

pub enum HostLoop<'a> {
    Native(&'a ActiveEventLoop),
    Headless(&'a HeadlessLoopState),
}

impl<'a> HostLoop<'a> {
    pub fn headless(state: &'a HeadlessLoopState) -> Self {
        Self::Headless(state)
    }

    pub fn exit(&self) {
        match self {
            Self::Native(event_loop) => event_loop.exit(),
            Self::Headless(state) => state.exit_requested.store(true, Ordering::Relaxed),
        }
    }

    pub fn set_control_flow(&self, flow: ControlFlow) {
        match self {
            Self::Native(event_loop) => event_loop.set_control_flow(flow),
            Self::Headless(state) => state.last_control_flow.set(flow),
        }
    }

    /// Headless only: the native loop is woken by `UiWaker` and needs no such hint.
    pub fn set_awaiting_background(&self, awaiting: bool) {
        if let Self::Headless(state) = self {
            state.awaiting_background.set(awaiting);
        }
    }

    pub fn native(&self) -> Option<&ActiveEventLoop> {
        match self {
            Self::Native(event_loop) => Some(event_loop),
            Self::Headless(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HeadlessLoopState, HostLoop};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    use winit::event_loop::ControlFlow;

    #[test]
    fn host_loop_headless_exit_sets_flag() {
        let state = HeadlessLoopState::default();
        HostLoop::headless(&state).exit();
        assert!(state.exit_requested.load(Ordering::Relaxed));
    }

    #[test]
    fn host_loop_headless_records_control_flow() {
        let state = HeadlessLoopState::default();
        assert_eq!(state.last_control_flow.get(), ControlFlow::Wait);
        let wake_at = Instant::now() + Duration::from_millis(100);
        HostLoop::headless(&state).set_control_flow(ControlFlow::WaitUntil(wake_at));
        assert_eq!(state.last_control_flow.get(), ControlFlow::WaitUntil(wake_at));
    }

    #[test]
    fn host_loop_headless_has_no_native() {
        let state = HeadlessLoopState::default();
        assert!(HostLoop::headless(&state).native().is_none());
    }
}
