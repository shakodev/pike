//! One emergency prompt per existing timeout token; the original Wait/Force
//! policy remains the sole decision consumer.
use super::*;
use crate::native_cleanup::{Decision, Prompt};
use std::sync::Arc;

impl Drop for Policy {
    fn drop(&mut self) {
        if let Some((_, prompt)) = self.native_prompt.take() {
            prompt.cancel();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::executor::SharedExecutor;
    use iced::Executor;
    use std::time::Duration;

    #[test]
    fn delayed_native_response_is_cancelled_by_real_pool_cleanup_and_old_force_is_inert() {
        let executor = SharedExecutor::new().unwrap();
        let mut app = App::default();
        let (release, gate) = std::sync::mpsc::channel();
        let (created, observed) = std::sync::mpsc::channel();
        let request = executor
            .enter(|| {
                app.launches.start_with(move |id| {
                    let prepared = iced_term::PreparedTerminal::new(
                        id.get(),
                        iced_term::settings::BackendSettings {
                            program: "/bin/sh".into(),
                            args: vec!["-c".into(), "read pike34".into()],
                            ..Default::default()
                        },
                    )?;
                    created.send(prepared.shutdown_handle()).unwrap();
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(prepared)
                })
            })
            .unwrap();
        let handle = observed.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(app.renderer_shutdown());
        let token = app.close_policy.timer.unwrap();
        app.close_timeout(token);
        assert!(app.native_cleanup_prompt(token).units() > 0);
        assert_eq!(app.native_cleanup_prompt(token).units(), 0);
        let first = app.close_policy.native_prompt.as_ref().unwrap().1.clone();
        assert_eq!(
            app.native_cleanup_decision(token + 1, Decision::Force)
                .units(),
            0
        );
        assert!(app.native_cleanup_decision(token, Decision::Wait).units() > 0);
        assert_eq!(first.show(), Decision::Cancelled);
        let next = app.close_policy.timer.unwrap();
        assert_ne!(token, next);
        assert_eq!(
            app.native_cleanup_decision(token, Decision::Force).units(),
            0
        );
        app.close_timeout(next);
        drop(app.native_cleanup_prompt(next));
        let delayed = app.close_policy.native_prompt.as_ref().unwrap().1.clone();
        release.send(()).unwrap();
        executor.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), app.launches.shutdown())
                .await
                .unwrap();
        });
        assert!(handle.completion().is_some());
        drop(app.shutdown_complete());
        assert_eq!(delayed.show(), Decision::Cancelled);
        assert_eq!(
            app.native_cleanup_decision(next, Decision::Force).units(),
            0
        );
        assert_eq!(app.native_cleanup_prompt(next).units(), 0);
        assert!(executor.block_on(request.receive()).is_err());
    }
}

impl App {
    pub(in crate::app) fn native_cleanup_prompt(&mut self, token: u64) -> Task<Message> {
        if !self.exiting
            || !self.close_policy.renderer_failed
            || !self.close_policy.timed_out
            || self.close_policy.timer != Some(token)
            || self.close_policy.native_prompt.is_some()
            || self.close_policy.native_reported == Some(token)
            || self.launches.shutdown_complete()
        {
            return Task::none();
        }
        let prompt = Arc::new(Prompt::default());
        self.close_policy.native_prompt = Some((token, prompt.clone()));
        self.close_policy.native_reported = Some(token);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || prompt.show())
                    .await
                    .unwrap_or(Decision::Unavailable(None))
            },
            move |decision| Message::NativeCleanup(token, decision),
        )
    }

    pub(in crate::app) fn cancel_native_prompt(&mut self) {
        if let Some((_, prompt)) = self.close_policy.native_prompt.take() {
            prompt.cancel();
        }
    }

    pub(in crate::app) fn native_cleanup_decision(
        &mut self,
        token: u64,
        decision: Decision,
    ) -> Task<Message> {
        if self
            .close_policy
            .native_prompt
            .as_ref()
            .map(|(token, _)| *token)
            != Some(token)
            || self.close_policy.timer != Some(token)
            || !self.close_policy.timed_out
            || !self.exiting
        {
            return Task::none();
        }
        self.cancel_native_prompt();
        if self.launches.shutdown_complete() {
            return Task::none();
        }
        match decision {
            Decision::Wait => self.close_action(CloseAction::Wait(token)),
            Decision::Force => self.close_action(CloseAction::Force(token)),
            Decision::Cancelled => Task::none(),
            Decision::Unavailable(_) => {
                eprintln!(
                    "Pike could not display its cleanup choice. Cleanup remains pending; no automatic force close was attempted."
                );
                Task::none()
            }
        }
    }
}
