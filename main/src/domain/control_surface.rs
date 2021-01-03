use crate::domain::{DomainEventHandler, Global, MainProcessor};
use crossbeam_channel::Receiver;
use reaper_high::{
    ChangeDetector, ControlSurfaceEvent, ControlSurfaceMiddleware, ControlSurfacePerformanceMonitor,
};
use reaper_rx::ControlSurfaceRxDriver;
use std::time::Duration;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    logger: slog::Logger,
    change_detector: ChangeDetector,
    rx_driver: ControlSurfaceRxDriver,
    main_processors: Vec<MainProcessor<EH>>,
    performance_monitor: reaper_high::ControlSurfacePerformanceMonitor,
    log_next_metrics: bool,
    task_receiver: Receiver<RealearnControlSurfaceTask<EH>>,
}

pub enum RealearnControlSurfaceTask<EH: DomainEventHandler> {
    AddMainProcessor(MainProcessor<EH>),
    RemoveMainProcessor(String),
    LogDebugInfo,
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    pub fn new(
        parent_logger: &slog::Logger,
        task_receiver: Receiver<RealearnControlSurfaceTask<EH>>,
    ) -> Self {
        let logger = parent_logger.new(slog::o!("struct" => "RealearnConrolSurfaceMiddleware"));
        Self {
            logger: logger.clone(),
            change_detector: ChangeDetector::new(),
            rx_driver: ControlSurfaceRxDriver::new(Global::control_surface_rx().clone()),
            main_processors: Default::default(),
            performance_monitor: ControlSurfacePerformanceMonitor::new(
                logger,
                Duration::from_secs(30),
            ),
            log_next_metrics: false,
            task_receiver,
        }
    }
}

impl<EH: DomainEventHandler> ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware<EH> {
    fn run(&mut self) {
        for t in self.task_receiver.try_iter().take(10) {
            use RealearnControlSurfaceTask::*;
            match t {
                AddMainProcessor(p) => {
                    self.main_processors.push(p);
                }
                RemoveMainProcessor(id) => {
                    self.main_processors.retain(|p| p.instance_id() != id);
                }
                LogDebugInfo => {
                    self.log_next_metrics = true;
                }
            }
        }
        for mut p in &mut self.main_processors {
            p.run();
        }
    }

    fn handle_event(&self, event: ControlSurfaceEvent) {
        self.change_detector.process(event, |e| {
            self.rx_driver.handle_change(e);
        });
    }

    fn handle_metrics(&mut self, metrics: &reaper_medium::ControlSurfaceMetrics) {
        self.performance_monitor.handle_metrics(metrics);
        if self.log_next_metrics {
            self.performance_monitor.log_metrics(metrics);
            self.log_next_metrics = false;
        }
    }
}
