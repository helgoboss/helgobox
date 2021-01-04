use crate::domain::{DomainEventHandler, Global, MainProcessor};
use crossbeam_channel::Receiver;
use reaper_high::{
    ChangeDetector, ControlSurfaceEvent, ControlSurfaceMiddleware,
    ControlSurfacePerformanceMonitor, MeterControlSurfaceMiddleware,
};
use reaper_rx::ControlSurfaceRxDriver;
use std::collections::HashMap;
use std::time::Duration;
use wrap_debug::WrapDebug;

#[derive(Debug)]
pub struct RealearnControlSurfaceMiddleware<EH: DomainEventHandler> {
    logger: slog::Logger,
    change_detector: ChangeDetector,
    rx_driver: ControlSurfaceRxDriver,
    main_processors: Vec<MainProcessor<EH>>,
    performance_monitor: reaper_high::ControlSurfacePerformanceMonitor,
    log_next_metrics: bool,
    task_receiver: Receiver<RealearnControlSurfaceTask<EH>>,
    counter: u64,
    update_metrics_snapshot: WrapDebug<fn(&reaper_medium::ControlSurfaceMetrics)>,
    meter_middleware: MeterControlSurfaceMiddleware,
}

pub enum RealearnControlSurfaceTask<EH: DomainEventHandler> {
    AddMainProcessor(MainProcessor<EH>),
    RemoveMainProcessor(String),
    LogDebugInfo,
    ProvidePrometheusMetrics(tokio::sync::oneshot::Sender<String>),
}

impl<EH: DomainEventHandler> RealearnControlSurfaceMiddleware<EH> {
    pub fn new(
        parent_logger: &slog::Logger,
        task_receiver: Receiver<RealearnControlSurfaceTask<EH>>,
        update_metrics_snapshot: fn(&reaper_medium::ControlSurfaceMetrics),
    ) -> Self {
        let logger = parent_logger.new(slog::o!("struct" => "RealearnControlSurfaceMiddleware"));
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
            counter: 0,
            update_metrics_snapshot: WrapDebug(update_metrics_snapshot),
            meter_middleware: MeterControlSurfaceMiddleware::new(),
        }
    }
}

impl<EH: DomainEventHandler> ControlSurfaceMiddleware for RealearnControlSurfaceMiddleware<EH> {
    fn run(&mut self) {
        let elapsed = MeterControlSurfaceMiddleware::measure(|| {
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
                    ProvidePrometheusMetrics(sender) => {}
                }
            }
            for mut p in &mut self.main_processors {
                p.run();
            }
        });
        self.meter_middleware.record_run(elapsed);
    }

    fn handle_event(&self, event: ControlSurfaceEvent) {
        let elapsed = MeterControlSurfaceMiddleware::measure(|| {
            self.change_detector.process(event, |e| {
                self.rx_driver.handle_change(e);
            });
        });
        self.meter_middleware.record_event(event, elapsed);
    }

    fn handle_metrics(&mut self, metrics: &reaper_medium::ControlSurfaceMetrics) {
        // We know it's called roughly 30 times a second.
        if self.counter == 30 * 10 {
            (self.update_metrics_snapshot)(metrics);
            self.counter = 0;
        } else {
            self.counter += 1;
        }
        self.performance_monitor.handle_metrics(metrics);
        if self.log_next_metrics {
            self.performance_monitor.log_metrics(metrics);
            self.log_next_metrics = false;
        }
    }
}
