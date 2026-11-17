//! Access the native system.
use crate::core::theme;
use crate::futures::futures::channel::{mpsc, oneshot};
use crate::futures::subscription::{self, Subscription};
use crate::task::{self, Task};

/// An operation to be performed on the system.
#[derive(Debug)]
pub enum Action {
    /// Register one bounded metadata-only sink for exhausted renderer recovery.
    RegisterRendererErrors(mpsc::Sender<RendererEvent>),
    /// Send available system information.
    GetInformation(oneshot::Sender<Information>),

    /// Send the current system theme mode.
    GetTheme(oneshot::Sender<theme::Mode>),

    /// Notify to the runtime that the system theme has changed.
    NotifyTheme(theme::Mode),
}

/// A renderer failure code, without driver messages, paths or user data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererFailure {
    /// The window or graphics context could not be initialized.
    Initialization,
    /// A frame could not be allocated.
    OutOfMemory,
    /// Repeated acquisition timeouts exhausted the finite recovery budget.
    Timeout,
    /// Surface recreation exhausted the finite recovery budget.
    SurfaceLost,
    /// Surface reconfiguration exhausted the finite recovery budget.
    SurfaceOutdated,
    /// Other repeated frame acquisition failures exhausted recovery.
    Other,
}

/// Ordered renderer registration and one failure, without user text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererEvent {
    /// The host has installed its error sink; native preparation may begin.
    Ready,
    /// Recovery stopped; retain the event loop until owned cleanup completes.
    Failed(RendererFailure),
}

/// Register a single bounded sink using the existing event-driven task channel.
/// The host delivers at most one failure, then continues application cleanup.
pub fn renderer_errors() -> Task<RendererEvent> {
    // Capacity explicitly accommodates both Ready and the single Failure even
    // when the application has not consumed Ready. No periodic task is created.
    let (sender, receiver) = mpsc::channel(2);
    task::effect(crate::Action::System(Action::RegisterRendererErrors(
        sender,
    )))
    .chain(Task::stream(receiver))
}

/// Contains information about the system (e.g. system name, processor, memory, graphics adapter).
#[derive(Clone, Debug)]
pub struct Information {
    /// The operating system name
    pub system_name: Option<String>,
    /// Operating system kernel version
    pub system_kernel: Option<String>,
    /// Long operating system version
    ///
    /// Examples:
    /// - MacOS 10.15 Catalina
    /// - Windows 10 Pro
    /// - Ubuntu 20.04 LTS (Focal Fossa)
    pub system_version: Option<String>,
    /// Short operating system version number
    pub system_short_version: Option<String>,
    /// Detailed processor model information
    pub cpu_brand: String,
    /// The number of physical cores on the processor
    pub cpu_cores: Option<usize>,
    /// Total RAM size, in bytes
    pub memory_total: u64,
    /// Memory used by this process, in bytes
    pub memory_used: Option<u64>,
    /// Underlying graphics backend for rendering
    pub graphics_backend: String,
    /// Model information for the active graphics adapter
    pub graphics_adapter: String,
}

/// Returns available system information.
pub fn information() -> Task<Information> {
    task::oneshot(|channel| {
        crate::Action::System(Action::GetInformation(channel))
    })
}

/// Returns the current system theme.
pub fn theme() -> Task<theme::Mode> {
    task::oneshot(|sender| crate::Action::System(Action::GetTheme(sender)))
}

/// Subscribes to system theme changes.
pub fn theme_changes() -> Subscription<theme::Mode> {
    #[derive(Hash)]
    struct ThemeChanges;

    subscription::filter_map(ThemeChanges, |event| {
        let subscription::Event::SystemThemeChanged(mode) = event else {
            return None;
        };

        Some(mode)
    })
}

#[cfg(test)]
mod renderer_tests {
    use super::*;
    use crate::futures::futures::task::noop_waker;
    use crate::{Action as RuntimeAction, task::into_stream};
    use std::task::{Context, Poll};

    #[test]
    fn readiness_and_failure_are_bounded_and_ordered_before_either_is_consumed()
    {
        let mut stream = into_stream(renderer_errors()).unwrap();
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let Poll::Ready(Some(RuntimeAction::System(
            Action::RegisterRendererErrors(mut sink),
        ))) = stream.as_mut().poll_next(&mut cx)
        else {
            panic!("registration")
        };
        sink.try_send(RendererEvent::Ready).unwrap();
        sink.try_send(RendererEvent::Failed(RendererFailure::OutOfMemory))
            .unwrap();
        drop(sink);
        // The pinned Task::stream deliberately yields once before consuming
        // its source. Both queued metadata events survive that scheduling turn.
        assert!(stream.as_mut().poll_next(&mut cx).is_pending());
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(RuntimeAction::Output(RendererEvent::Ready)))
        ));
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(RuntimeAction::Output(RendererEvent::Failed(
                RendererFailure::OutOfMemory
            ))))
        ));
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(None)
        ));
    }
}
