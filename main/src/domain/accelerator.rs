use crate::domain::{DomainEventHandler, KeyMessage, Keystroke, SharedMainProcessors};
use reaper_high::Reaper;
use reaper_medium::{Accel, TranslateAccel, TranslateAccelArgs, TranslateAccelResult};

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
        let mut filter_out_event = false;
        for proc in &mut *self.main_processors.borrow_mut() {
            if proc.wants_keys() && proc.process_incoming_key_msg(msg) {
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
        // TODO-high Remove debug logging
        log_args(&args);
        let stroke = Keystroke::new(args.msg.behavior, args.msg.key);
        let msg = KeyMessage::new(args.msg.message, stroke);
        self.process_message(msg)
    }
}

fn log_args(args: &TranslateAccelArgs) {
    let accel = Accel {
        f_virt: args.msg.behavior,
        key: args.msg.key,
        cmd: 0,
    };
    let formatted = Reaper::get().medium_reaper().kbd_format_key_name(accel);
    tracing_debug!(
        "\
            Captured {:?}\n\
            Formatted: {},\n\
        ",
        &args,
        formatted
    );
}
