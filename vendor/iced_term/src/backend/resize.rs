//! One current native operation. Frames deduplicate attempted geometry even on
//! failure; only an explicit retry creates another serial for the same size.
use super::*;
use alacritty_terminal::event::{ResizeError, ResizeResult};
use alacritty_terminal::event_loop::{EventLoopSendError, Msg};

#[derive(Default)]
pub(super) struct Resize {
    serial: u64,
    current: Option<ResizeResult>,
    pending: bool,
}

impl Resize {
    pub(super) fn begin(&mut self, size: WindowSize) -> Option<u64> {
        self.serial = self.serial.checked_add(1)?;
        self.current = Some(ResizeResult {
            serial: self.serial,
            size,
            result: Ok(()),
        });
        self.pending = true;
        Some(self.serial)
    }

    pub(super) fn complete(&mut self, result: ResizeResult) -> bool {
        let Some(current) = self.current else {
            return false;
        };
        if !self.pending
            || current.serial != result.serial
            || current.size != result.size
        {
            return false;
        }
        self.current = Some(result);
        self.pending = false;
        true
    }
}

impl Backend {
    pub(super) fn request_resize(&mut self, size: WindowSize) -> Action {
        let Some(serial) = self.resize.begin(size) else {
            return Action::ResizeCompleted(ResizeResult {
                serial: u64::MAX,
                size,
                result: Err(ResizeError {
                    kind: std::io::ErrorKind::Other,
                    code: None,
                }),
            });
        };
        self.pty_resizes = self.pty_resizes.saturating_add(1);
        match self.notifier.0.send(Msg::ResizeTracked { serial, size }) {
            Ok(()) => Action::Ignore,
            Err(error) => {
                let error = match error {
                    EventLoopSendError::Io(error) => error.into(),
                    _ => ResizeError {
                        kind: std::io::ErrorKind::BrokenPipe,
                        code: None,
                    },
                };
                let result = ResizeResult {
                    serial,
                    size,
                    result: Err(error),
                };
                self.resize.complete(result);
                Action::ResizeCompleted(result)
            },
        }
    }

    pub(super) fn retry_resize(&mut self) -> Action {
        if self.notifier.0.input_status().closed {
            return Action::Ignore;
        }
        if let Some(current) = self.resize.current.filter(|r| r.result.is_err())
        {
            self.request_resize(current.size)
        } else {
            Action::Ignore
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_size_serial_and_exhaustion_do_not_replace_current_operation() {
        let mut resize = Resize::default();
        let size = WindowSize {
            num_cols: 80,
            num_lines: 25,
            cell_width: 8,
            cell_height: 16,
        };
        let old = resize.begin(size).unwrap();
        let serial = resize.begin(size).unwrap();
        let error = Err(ResizeError {
            kind: std::io::ErrorKind::PermissionDenied,
            code: Some(5),
        });
        assert!(!resize.complete(ResizeResult {
            serial: old,
            size,
            result: error
        }));
        assert!(!resize.complete(ResizeResult {
            serial,
            size: WindowSize {
                num_cols: 1,
                ..size
            },
            result: error
        }));
        assert!(resize.complete(ResizeResult {
            serial,
            size,
            result: error
        }));
        resize.serial = u64::MAX;
        assert!(resize.begin(size).is_none());
        assert_eq!(resize.current.unwrap().serial, serial);
    }

    #[test]
    fn controlled_native_session_deduplicates_failed_size_and_explicitly_retries_once(
    ) {
        let (sender, receiver) = crate::event_bridge::channel();
        let mut backend = Backend::new(
            34001,
            sender,
            crate::settings::BackendSettings {
                #[cfg(windows)]
                program: "cmd.exe".into(),
                #[cfg(windows)]
                args: ["/D", "/Q", "/C", "set /p PIKE_WAIT="]
                    .map(str::to_owned)
                    .to_vec(),
                #[cfg(not(windows))]
                program: "/bin/sh".into(),
                #[cfg(not(windows))]
                args: ["-c", "read pike_wait"].map(str::to_owned).to_vec(),
                ..Default::default()
            },
        )
        .unwrap();
        let layout = Some(Size::new(800.0, 400.0));
        let font = Some(Size::new(8.0, 16.0));
        backend.handle(Command::Resize(layout, font));
        let attempted = backend.resize.current.unwrap();
        let failed = ResizeResult {
            result: Err(ResizeError {
                kind: std::io::ErrorKind::PermissionDenied,
                code: Some(5),
            }),
            ..attempted
        };
        assert_eq!(
            backend.handle(Command::ProcessAlacrittyEvent(
                Event::ResizeCompleted(failed)
            )),
            Action::ResizeCompleted(failed)
        );
        let count = backend.pty_resizes;
        for _ in 0..1000 {
            backend.handle(Command::Resize(layout, font));
        }
        assert_eq!(backend.pty_resizes, count);
        assert_eq!(backend.handle(Command::RetryResize), Action::Ignore);
        assert_eq!(backend.pty_resizes, count + 1);
        let current = backend.resize.current.unwrap();
        assert!(current.serial > attempted.serial);
        assert_eq!(
            backend.handle(Command::ProcessAlacrittyEvent(
                Event::ResizeCompleted(attempted)
            )),
            Action::Ignore
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    if let Some(crate::event_bridge::Delivery::Terminal(
                        Event::ResizeCompleted(result),
                    )) = receiver.recv().await
                    {
                        if result.serial == current.serial {
                            assert_eq!(result.result, Ok(()));
                            assert_eq!(
                                backend.handle(Command::ProcessAlacrittyEvent(
                                    Event::ResizeCompleted(result)
                                )),
                                Action::ResizeCompleted(result)
                            );
                            break;
                        }
                    }
                }
            })
            .await
            .unwrap();
        });
        assert_eq!(backend.handle(Command::RetryResize), Action::Ignore);
        assert_eq!(backend.pty_resizes, count + 1);
        backend.request_shutdown();
        runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                backend.shutdown_handle().wait(),
            )
            .await
            .unwrap();
        });
    }
}
