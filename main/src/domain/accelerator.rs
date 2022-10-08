use crate::domain::{
    ControlEvent, ControlEventTimestamp, DomainEventHandler, KeyMessage, Keystroke,
    SharedMainProcessors,
};
use helgoboss_learn::AbstractTimestamp;
use reaper_high::AcceleratorKey;
use reaper_low::raw;
use reaper_medium::{
    AccelMsgKind, TranslateAccel, TranslateAccelArgs, TranslateAccelResult, VirtKey,
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
    fn process_message(&mut self, msg: KeyMessage) -> TranslateAccelResult {
        let evt = ControlEvent::new(msg, ControlEventTimestamp::now());
        let mut filter_out_event = false;
        for proc in &mut *self.main_processors.borrow_mut() {
            if proc.wants_keys() && proc.process_incoming_key_msg(evt) {
                filter_out_event = true;
            }
        }
        if filter_out_event {
            TranslateAccelResult::Eat
        } else if msg.stroke().accelerator_key() == ESCAPE_KEY {
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
                // Only ProcessEventRaw seems to work when pressing Return key in an egui text edit.
                // Everything else breaks the complete text field, it doesn't receive input anymore.
                // TODO-high Problem: It seems only "Eat" prevents the key from triggering
                //  REAPER actions. Everything else seems to make the key be processed by
                //  REAPER's action accelerator. If there's an action with a keyboard shortcut
                //  (e.g. Cmd+C), the action eats it and it doesn't arrive at our window.
                //  Now, we could simulate key presses to our windows here and eat. But it doesn't
                //  work for some reason. Then pressing keys will not do anything. Maybe because
                //  enters the key logic again, from outside the window.
                //  Interesting, macOS always shows the menu of the windows in focus, it seems. And
                //  when having the mapping panel or a child of it in focus, it shows *REAPER's*
                //  menu. Maybe the issue is that the mapping panel is a child of the REAPER window.
                if w.process_current_app_event_if_no_text_field() {
                    TranslateAccelResult::Eat
                } else {
                    TranslateAccelResult::NotOurWindow
                }
            }
            #[cfg(target_os = "windows")]
            {
                let _ = w;
                TranslateAccelResult::ForcePassOnToWindow
            }
            #[cfg(target_os = "linux")]
            {
                let _ = w;
                TranslateAccelResult::NotOurWindow
            }
        } else {
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
        if args.msg.message == AccelMsgKind::Char {
            return TranslateAccelResult::NotOurWindow;
        }
        let stroke = Keystroke::new(args.msg.behavior, args.msg.key).normalized();
        let msg = KeyMessage::new(args.msg.message, stroke);
        self.process_message(msg)
    }
}

const ESCAPE_KEY: AcceleratorKey = AcceleratorKey::VirtKey(VirtKey::new(raw::VK_ESCAPE as _));
