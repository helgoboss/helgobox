use crate::domain::{
    Backbone, ControlEvent, ControlEventTimestamp, DomainEventHandler, KeyMessage, Keystroke,
    SharedMainProcessors,
};
use reaper_high::Reaper;
use reaper_low::raw;
use reaper_medium::{
    virt_keys, AccelMsg, AccelMsgKind, AcceleratorBehavior, TranslateAccel, TranslateAccelArgs,
    TranslateAccelResult,
};
use swell_ui::{SharedView, View, Window};

pub trait HelgoboxWindowSnitch {
    /// Goes up the window hierarchy trying to find an associated ReaLearn view, starting with
    /// the given window.
    fn find_closest_realearn_view(&self, window: Window) -> Option<SharedView<dyn View>>;
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
    S: HelgoboxWindowSnitch,
{
    /// Sends the message to all main processors as control events.
    ///
    /// Returns `true` if at least one main processor used the message, that is if at least one
    /// main processor had control input set to "Keyboard" and a mapping matched the given key.
    ///
    /// If yes, we should completely "eat" the message and don't do anything else with it.
    fn process_control(&mut self, msg: KeyMessage) -> bool {
        let evt = ControlEvent::new(msg, ControlEventTimestamp::from_main_thread());
        let mut filter_out_event = false;
        let mut notified_backbone = false;
        for proc in &mut *self.main_processors.borrow_mut() {
            if !proc.wants_keyboard_input() {
                continue;
            }
            let result = proc.process_incoming_key_msg(evt);
            // Notify backbone that ReaLearn mappings successfully matched keyboard input in this
            // main loop cycle. We need to do that only once.
            if !notified_backbone && result.match_outcome.matched() {
                notified_backbone = true;
                Backbone::get().set_keyboard_input_match_flag();
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
        let is_virt_key = msg.message() != AccelMsgKind::Char
            && msg.behavior().contains(AcceleratorBehavior::VirtKey);
        if is_virt_key && msg.key().get() as u32 == raw::VK_ESCAPE {
            // Don't process escape in special ways. We want the normal close behavior. Especially
            // important for the floating ReaLearn FX window where closing the main panel would not
            // close the surrounding floating window.
            return TranslateAccelResult::NotOurWindow;
        }
        let Some(focused_window) = Window::focused() else {
            // No window focused
            return TranslateAccelResult::NotOurWindow;
        };
        let Some(view) = self.snitch.find_closest_realearn_view(focused_window) else {
            // Not our window which is focused. Act normally.
            return TranslateAccelResult::NotOurWindow;
        };
        // Support F1 in our windows (key_down and key_up don't work very well on Linux and Windows if there's
        // a text field)
        if is_virt_key && msg.key().get() == virt_keys::F1.get() {
            if msg.message() == AccelMsgKind::KeyUp {
                view.help_requested();
            }
            return TranslateAccelResult::Eat;
        }
        if !view.wants_raw_keyboard_input() {
            return TranslateAccelResult::NotOurWindow;
        }
        let Some(w) = view.get_keyboard_event_receiver(focused_window) else {
            // No window is interested.
            return TranslateAccelResult::Eat;
        };
        // A ReaLearn window is focused.
        // - We want to get almost all keyboard input (without having to enable "Send all keyboard input to plug-in")
        // - We don't want REAPER to execute actions or the system to execute menu commands
        // - We want the egui window to receive raw keys
        //
        // All of this is achieved in different ways depending on the OS.
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
            // This is necessary for Pot Browser to receive keys
            w.process_raw_message(msg.raw());
            TranslateAccelResult::Eat
        }
    }
}

impl<EH, S> TranslateAccel for RealearnAccelerator<EH, S>
where
    EH: DomainEventHandler,
    S: HelgoboxWindowSnitch,
{
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        // Ignore char messages
        if args.msg.message() == AccelMsgKind::Char {
            // Char messages are only sent on Windows and for all relevant control purposes,
            // they are preceded by a KeyDown event, so we must ignore them.
            return self.process_unmatched(args.msg);
        }
        // If REAPER 7.23+, check if window is text field
        let reaper = Reaper::get().medium_reaper();
        if reaper.low().pointers().IsWindowTextField.is_some() {
            if let Some(window) = Window::focused() {
                let is_text_field = unsafe { reaper.is_window_text_field(window.raw_hwnd()) };
                if is_text_field {
                    return self.process_unmatched(args.msg);
                }
            }
        }
        // If we end up here, it could be interesting for the main processors
        let stroke = Keystroke::new(args.msg.behavior(), args.msg.key());
        let normalized_stroke = stroke.normalized();
        let normalized_msg = KeyMessage::new(args.msg.message(), normalized_stroke);
        let filter_out = self.process_control(normalized_msg);
        if filter_out {
            TranslateAccelResult::Eat
        } else {
            self.process_unmatched(args.msg)
        }
    }
}
