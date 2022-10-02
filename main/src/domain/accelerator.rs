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

pub trait RealearnWindowSnitch {
    fn realearn_window_is_focused(&self) -> bool;
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
            // Don't process escape raw. We want the normal close behavior. Especially important
            // for the floating ReaLearn FX window where closing the main panel would not close
            // the surrounding floating window.
            TranslateAccelResult::NotOurWindow
        } else if self.snitch.realearn_window_is_focused() {
            if cfg!(target_os = "macos") {
                // Only ProcessEventRaw seems to work when pressing Return key in an egui text edit.
                // Everything else breaks the complete text field, it doesn't receive input anymore.
                TranslateAccelResult::ProcessEventRaw
            } else {
                TranslateAccelResult::ForcePassOnToWindow
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
