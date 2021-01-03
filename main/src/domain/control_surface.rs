use crate::domain::Global;
use reaper_high::{ChangeDetector, ControlSurfaceEvent, ControlSurfaceMiddleware};
use reaper_rx::ControlSurfaceRxDriver;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware {
    change_detector: ChangeDetector,
    rx_driver: ControlSurfaceRxDriver,
}

impl RealearnControlSurfaceMiddleware {
    pub fn new() -> Self {
        Self {
            change_detector: ChangeDetector::new(),
            rx_driver: ControlSurfaceRxDriver::new(Global::control_surface_rx().clone()),
        }
    }
}

impl ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware {
    fn handle_event(&self, event: ControlSurfaceEvent) {
        self.change_detector.process(event, |e| {
            self.rx_driver.handle_change(e);
        });
    }
}
