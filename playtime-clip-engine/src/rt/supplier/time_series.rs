#![allow(dead_code)]

#[derive(Clone, Debug)]
pub struct TimeSeries<T> {
    pub events: Vec<TimeSeriesEvent<T>>,
}

impl<T> Default for TimeSeries<T> {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl<T> TimeSeries<T> {
    pub fn new(events: Vec<TimeSeriesEvent<T>>) -> Self {
        Self { events }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TimeSeriesEvent<T> {
    pub frame: u64,
    pub payload: T,
}

impl<T> TimeSeriesEvent<T> {
    pub fn new(frame: u64, payload: T) -> Self {
        Self { frame, payload }
    }
}

impl<T> TimeSeries<T> {
    pub fn insert(&mut self, frame: u64, payload: T) {
        let event = TimeSeriesEvent::new(frame, payload);
        // Optimization: If frame is larger or equal than last frame, just push.
        let add = self.events.last().map(|l| frame >= l.frame).unwrap_or(true);
        if add {
            self.events.push(event);
            return;
        }
        // In all other cases, insert at correct position in order to maintain sort order
        let insertion_index = self.events.partition_point(|e| e.frame < frame);
        self.events.insert(insertion_index, event);
    }

    pub fn find_events_in_range(
        &self,
        start_frame: u64,
        frame_count: u64,
    ) -> &[TimeSeriesEvent<T>] {
        if frame_count == 0 {
            return &[];
        }
        let exclusive_end_frame = start_frame + frame_count;
        // Determine inclusive start index
        let start_index = self.events.partition_point(|e| e.frame < start_frame);
        // Determine exclusive end index
        let exclusive_end_index = self
            .events
            .partition_point(|e| e.frame < exclusive_end_frame);
        // Return slice
        &self.events[start_index..exclusive_end_index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_events_1() {
        // Given
        let time_series = TimeSeries::new(vec![
            e(5, 'a'),
            e(6, 'b'),
            e(15, 'c'),
            e(15, 'd'),
            e(17, 'd'),
            e(50, 'e'),
        ]);
        // When
        // Then
        assert_eq!(
            time_series.find_events_in_range(5, 10),
            &[e(5, 'a'), e(6, 'b'),]
        );
        assert_eq!(time_series.find_events_in_range(0, 0), &[]);
        assert_eq!(
            time_series.find_events_in_range(0, 1000),
            &time_series.events
        );
        assert_eq!(time_series.find_events_in_range(1000, 5000), &[]);
        assert_eq!(
            time_series.find_events_in_range(15, 20),
            &[e(15, 'c'), e(15, 'd'), e(17, 'd'),]
        );
    }

    #[test]
    fn find_events_2() {
        // Given
        let time_series = TimeSeries::new(vec![
            e(5, 'a'),
            e(6, 'b'),
            e(14, 'c'),
            e(15, 'd'),
            e(15, 'e'),
            e(17, 'f'),
            e(50, 'g'),
        ]);
        // When
        // Then
        assert_eq!(
            time_series.find_events_in_range(5, 10),
            &[e(5, 'a'), e(6, 'b'), e(14, 'c')]
        );
        assert_eq!(
            time_series.find_events_in_range(15, 20),
            &[e(15, 'd'), e(15, 'e'), e(17, 'f'),]
        );
    }

    fn e<T>(frame: u64, payload: T) -> TimeSeriesEvent<T> {
        TimeSeriesEvent::new(frame, payload)
    }
}
