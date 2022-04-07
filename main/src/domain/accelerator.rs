use crate::domain::{
    ControlEvent, ControlEventTimestamp, DomainEventHandler, KeyMessage, Keystroke,
    SharedMainProcessors,
};
use reaper_medium::{AccelMsgKind, TranslateAccel, TranslateAccelArgs, TranslateAccelResult};

#[derive(Debug)]
pub struct RealearnAccelerator<EH: DomainEventHandler> {
    main_processors: SharedMainProcessors<EH>,
}

impl<EH: DomainEventHandler> RealearnAccelerator<EH> {
    pub fn new(main_processors: SharedMainProcessors<EH>) -> Self {
        Self { main_processors }
    }
}

impl<EH: DomainEventHandler> RealearnAccelerator<EH> {
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
        } else {
            TranslateAccelResult::NotOurWindow
        }
    }
}

impl<EH: DomainEventHandler> TranslateAccel for RealearnAccelerator<EH> {
    fn call(&mut self, args: TranslateAccelArgs) -> TranslateAccelResult {
        if args.msg.message == AccelMsgKind::Char {
            return TranslateAccelResult::NotOurWindow;
        }
        let stroke = Keystroke::new(args.msg.behavior, args.msg.key).normalized();
        let msg = KeyMessage::new(args.msg.message, stroke);
        self.process_message(msg)
    }
}
