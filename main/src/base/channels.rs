use crate::base::channel_util::{try_send_on_named_channel, NamedChannelTrySendError};
use crossbeam_channel::{Receiver, Sender};
use reaper_high::Reaper;

#[derive(Debug)]
pub struct RealTimeSender<T> {
    channel_name: &'static str,
    sender: Sender<T>,
}

impl<T> Clone for RealTimeSender<T> {
    fn clone(&self) -> Self {
        Self {
            channel_name: self.channel_name,
            sender: self.sender.clone(),
        }
    }
}

impl<T> RealTimeSender<T> {
    pub fn new_channel(name: &'static str, capacity: usize) -> (Self, Receiver<T>) {
        let (sender, receiver) = crossbeam_channel::bounded(capacity);
        (
            Self {
                channel_name: name,
                sender,
            },
            receiver,
        )
    }

    pub fn send_if_space(&self, task: T) {
        let _ = self.send_internal(task);
    }

    pub fn send_complaining(&self, task: T) {
        self.send_internal(task).unwrap();
    }

    fn send_internal(&self, task: T) -> Result<(), NamedChannelTrySendError<T>> {
        if Reaper::get().audio_is_running() {
            // Audio is running so sending should always work. If not, it's an unexpected error and
            // we must return it.
            try_send_on_named_channel(&self.sender, self.channel_name, task)
        } else {
            // Audio is not running. Maybe this is just a very temporary outage or a short initial
            // non-running state.
            if self.channel_still_has_some_headroom() {
                // Channel still has some headroom, so we send the task in order to support a
                // temporary outage. This should not fail unless another sender has exhausted the
                // channel in the meanwhile. Even then, so what. See "else" branch.
                let _ = self.sender.try_send(task);
                Ok(())
            } else {
                // Channel has already accumulated lots of tasks. Don't send!
                // It's not bad if we don't send this task because the real-time processor will
                // not be able to process it anyway at the moment (it's not going to be called
                // because the audio engine is stopped). Fear not, ReaLearn's audio hook has logic
                // that detects a "rebirth" - the moment when the audio cycle starts again. In this
                // case it will request a full resync of everything so nothing should get lost
                // in theory.
                Ok(())
            }
        }
    }

    fn channel_still_has_some_headroom(&self) -> bool {
        self.sender.len() <= self.sender.capacity().unwrap() / 2
    }
}
