use crate::domain::{
    BackboneState, ControlEvent, ControlEventTimestamp, DomainEventHandler, KeyMessage, Keystroke,
    SharedMainProcessors,
};
use helgoboss_learn::AbstractTimestamp;
use reaper_low::raw;
use reaper_medium::{
    AccelMsg, AccelMsgKind, AcceleratorBehavior, TranslateAccel, TranslateAccelArgs,
    TranslateAccelResult,
};
use swell_ui::Window;

pub trait RealearnWindowSnitch {
    fn focused_realearn_window(&self) -> Option<Window>;
}

#[derive(Debug)]
pub struct RealearnAccelerator<EH: DomainEventHandler, S> {
    main_processors: SharedMainProcessors<EH>,
    snitch: S,
}

impl<EH: DomainEventHandler, S> RealearnAccelerator<EH, S> {
    pub fn new(main_processors: SharedMainProcessors<EH>, snitch: S) -> Self {
        Self {
            main_processors,
            snitch,
        }
    }
}

impl<EH, S> RealearnAccelerator<EH, S>
where
    EH: DomainEventHandler,
    S: RealearnWindowSnitch,
{
    /// Sends the message to all main processors as control events.
    ///
    /// Returns `true` if at least one main processor used the message, that is if at least one
    /// main processor had control input set to "Keyboard" and a mapping matched the given key.
    ///
    /// If yes, we should completely "eat" the message and don't do anything else with it.
    fn process_control(&mut self, msg: KeyMessage) -> bool {
        let evt = ControlEvent::new(msg, ControlEventTimestamp::now());
        let mut filter_out_event = false;
        let mut notified_backbone = false;
        for proc in &mut *self.main_processors.borrow_mut() {
            if !proc.wants_keys() {
                continue;
            }
            let result = proc.process_incoming_key_msg(evt);
            // Notify backbone that ReaLearn mappings successfully matched keyboard input in this
            // main loop cycle. We need to do that only once.
            if !notified_backbone && result.match_outcome.matched() {
                notified_backbone = true;
                BackboneState::get().set_keyboard_input_match_flag();
            }
            // If at least one instance wants to filter the key out, we filter it out, not
            // passing it forward to the rest of the keyboard processing chain!
            if result.filter_out_event {
                filter_out_event = true;
            }
        }
        filter_out_event
    }

    /// Decides what to do with the key if no main processor used it.
    fn process_unmatched(&self, msg: AccelMsg) -> TranslateAccelResult {
        if msg.behavior().contains(AcceleratorBehavior::VirtKey)
            && msg.key().get() as u32 == raw::VK_ESCAPE
        {
            // Don't process escape in special ways. We want the normal close behavior. Especially
            // important for the floating ReaLearn FX window where closing the main panel would not
            // close the surrounding floating window.
            TranslateAccelResult::NotOurWindow
        } else if let Some(w) = self.snitch.focused_realearn_window() {
            // A ReaLearn window is focused. We want to get almost all keyboard input! We don't want
            // REAPER to execute actions or the system to execute menu commands. This is achieved
            // in different ways depending on the OS.
            #[cfg(target_os = "macos")]
            {
                // On macOS, we must explicitly send the message to the focused view. If we
                // let the system take care of it (e.g. via NotOurWindow, PassOnToWindow or
                // ProcessEventRaw), REAPER's main menu shortcuts have priority, which we don't
                // want! Especially important for egui text edit because Cmd+C and Cmd+X would not
                // work in there.
                if w.process_current_app_event_if_no_text_field() {
                    TranslateAccelResult::Eat
                } else {
                    // However, if the focused view is a text field, this would break copy & paste
                    // in that text field ;) So in this special case, we need the system to do its
                    // job.
                    TranslateAccelResult::NotOurWindow
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                w.process_raw_message(msg.raw());
                TranslateAccelResult::Eat
            }
        } else {
            // A non-ReaLearn window is focused. Act normally.
            TranslateAccelResult::NotOurWindow
        }
    }
}

impl<EH, S> TranslateAccel for RealearnAccelerator<EH, S>
where
    EH: DomainEventHandler,
    S: RealearnWindowSnitch,
{
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        if args.msg.message() != AccelMsgKind::Char {
            // If it's not a char message, it could be interesting for main processors.
            // (Char messages are only sent on Windows and for all relevant control purposes,
            // they are preceded by a KeyDown event, so we must ignore them).
            let stroke = Keystroke::new(args.msg.behavior(), args.msg.key());
            let normalized_stroke = stroke.normalized();
            let normalized_msg = KeyMessage::new(args.msg.message(), normalized_stroke);
            let matched = self.process_control(normalized_msg);
            if matched {
                return TranslateAccelResult::Eat;
            }
        }
        // If we end up here, no main processor was interested in that key.
        self.process_unmatched(args.msg)
    }
}
