#![allow(dead_code)]

#[derive(Clone, Debug)]
pub struct TimeSeries<T> {
    pub entries: Vec<TimeSeriesEntry<T>>,
}

impl<T> TimeSeries<T> {
    pub fn new(events: Vec<TimeSeriesEntry<T>>) -> Self {
        Self { entries: events }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TimeSeriesEntry<T> {
    pub frame: u64,
    pub msg: T,
}

impl<T> TimeSeriesEntry<T> {
    pub fn new(frame: u64, msg: T) -> Self {
        Self { frame, msg }
    }
}

impl<T> TimeSeries<T> {
    pub fn find_entries(&self, start_frame: u64, frame_count: u64) -> &[TimeSeriesEntry<T>] {
        if frame_count == 0 {
            return &[];
        }
        let exclusive_end_frame = start_frame + frame_count;
        // Determine inclusive start index
        let start_index_approximation =
            match self.entries.binary_search_by_key(&start_frame, |e| e.frame) {
                Ok(i) | Err(i) => i,
            };
        let start_index = self.entries[0..start_index_approximation]
            .iter()
            .rposition(|e| e.frame < start_frame)
            .map(|i| i + 1)
            .unwrap_or(start_index_approximation);
        // Determine exclusive end index
        let exclusive_end_index_approximation = match self
            .entries
            .binary_search_by_key(&exclusive_end_frame, |e| e.frame)
        {
            Ok(i) | Err(i) => i,
        };
        let exclusive_end_index = self.entries[0..exclusive_end_index_approximation]
            .iter()
            .rposition(|e| e.frame < exclusive_end_frame)
            .map(|i| i + 1)
            .unwrap_or(exclusive_end_index_approximation);
        // Return slice
        &self.entries[start_index..exclusive_end_index]
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
        assert_eq!(time_series.find_entries(5, 10), &[e(5, 'a'), e(6, 'b'),]);
        assert_eq!(time_series.find_entries(0, 0), &[]);
        assert_eq!(time_series.find_entries(0, 1000), &time_series.entries);
        assert_eq!(time_series.find_entries(1000, 5000), &[]);
        assert_eq!(
            time_series.find_entries(15, 20),
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
            time_series.find_entries(5, 10),
            &[e(5, 'a'), e(6, 'b'), e(14, 'c')]
        );
        assert_eq!(
            time_series.find_entries(15, 20),
            &[e(15, 'd'), e(15, 'e'), e(17, 'f'),]
        );
    }

    fn e<T>(frame: u64, msg: T) -> TimeSeriesEntry<T> {
        TimeSeriesEntry::new(frame, msg)
    }
}
