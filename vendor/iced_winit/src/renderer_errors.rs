//! Finite recovery around the existing compositor, without a timer or fallback.
use crate::futures::futures::channel::mpsc;
use crate::graphics::{self, compositor::SurfaceError};
use crate::runtime::system::{RendererEvent, RendererFailure};

const RETRIES: u8 = 3;

#[derive(Default)]
pub(crate) struct Recovery {
    failures: u8,
    generation: u64,
}

pub(crate) enum Decision {
    Presented,
    Retry(SurfaceError),
    Failed(RendererFailure),
}

impl Recovery {
    pub(crate) fn presented(
        &mut self,
        generation: u64,
        result: Result<(), SurfaceError>,
    ) -> Decision {
        if self.generation != generation {
            self.generation = generation;
            self.failures = 0;
        }
        let error = match result {
            Ok(()) => {
                self.failures = 0;
                return Decision::Presented;
            },
            Err(error) => error,
        };
        if error != SurfaceError::OutOfMemory && self.failures < RETRIES {
            self.failures += 1;
            return Decision::Retry(error);
        }
        Decision::Failed(match error {
            SurfaceError::OutOfMemory => RendererFailure::OutOfMemory,
            SurfaceError::Timeout => RendererFailure::Timeout,
            SurfaceError::Lost => RendererFailure::SurfaceLost,
            SurfaceError::Outdated => RendererFailure::SurfaceOutdated,
            SurfaceError::Other => RendererFailure::Other,
        })
    }
}

#[derive(Default)]
pub(crate) struct Host {
    sink: Option<mpsc::Sender<RendererEvent>>,
    failure: Option<RendererFailure>,
}

impl Host {
    pub(crate) fn register(
        &mut self,
        mut sink: mpsc::Sender<RendererEvent>,
    ) -> bool {
        if sink.try_send(RendererEvent::Ready).is_err() {
            return false;
        }
        self.sink = Some(sink);
        true
    }

    pub(crate) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// Return true only when the application owns the reliable failure receipt.
    /// No registered host means startup must return its existing safe Result.
    pub(crate) fn fail(&mut self, failure: RendererFailure) -> bool {
        if self.failure.is_some() {
            return true;
        }
        self.failure = Some(failure);
        self.sink.as_mut().is_some_and(|sink| {
            sink.try_send(RendererEvent::Failed(failure)).is_ok()
        })
    }

    pub(crate) fn exit_error(&self) -> Option<crate::Error> {
        self.failure.map(|_| safe_error())
    }
}

pub(crate) fn safe_error() -> crate::Error {
    crate::Error::GraphicsCreationFailed(graphics::Error::BackendError("Renderer recovery exhausted; application-owned terminal cleanup requested.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_and_surface_recovery_is_finite_and_success_or_new_surface_resets_it()
     {
        for error in [
            SurfaceError::Timeout,
            SurfaceError::Other,
            SurfaceError::Lost,
            SurfaceError::Outdated,
        ] {
            let mut recovery = Recovery::default();
            for _ in 0..RETRIES {
                assert!(matches!(
                    recovery.presented(1, Err(error.clone())),
                    Decision::Retry(_)
                ));
            }
            assert!(matches!(
                recovery.presented(1, Err(error.clone())),
                Decision::Failed(_)
            ));
            assert!(matches!(
                recovery.presented(1, Ok(())),
                Decision::Presented
            ));
            assert!(matches!(
                recovery.presented(1, Err(error.clone())),
                Decision::Retry(_)
            ));
            assert!(matches!(
                recovery.presented(2, Err(error.clone())),
                Decision::Retry(_)
            ));
        }
    }

    #[test]
    fn oom_is_a_safe_failure_and_one_bounded_receipt_precedes_exit_error() {
        assert!(matches!(
            Recovery::default().presented(0, Err(SurfaceError::OutOfMemory)),
            Decision::Failed(RendererFailure::OutOfMemory)
        ));
        let mut host = Host::default();
        assert!(!host.fail(RendererFailure::Initialization));
        assert!(host.exit_error().is_some());
        let (sink, mut receiver) = mpsc::channel(2);
        let mut host = Host::default();
        assert!(host.register(sink));
        assert!(host.fail(RendererFailure::OutOfMemory));
        assert!(host.fail(RendererFailure::Other));
        assert_eq!(receiver.try_recv().unwrap(), RendererEvent::Ready);
        assert_eq!(
            receiver.try_recv().unwrap(),
            RendererEvent::Failed(RendererFailure::OutOfMemory)
        );
        assert!(receiver.try_recv().is_err());
        assert!(host.failed());
        assert!(host.exit_error().is_some());
    }
}
