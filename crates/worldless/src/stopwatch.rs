use std::{collections::HashMap, time::Instant};

use crate::resource::Identifier;

#[derive(Debug)]
pub(crate) struct StopwatchState {
    epoch: Instant,
    started_at_ms: HashMap<Identifier, u128>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StopwatchQuery {
    pub(crate) elapsed_seconds: f64,
    pub(crate) result: i32,
}

impl StopwatchState {
    pub(crate) fn new() -> Self {
        Self {
            epoch: Instant::now(),
            started_at_ms: HashMap::new(),
        }
    }

    pub(crate) fn create(&mut self, id: Identifier) -> bool {
        let now_ms = self.now_ms();
        self.create_at(id, now_ms)
    }

    pub(crate) fn query(&self, id: &Identifier, scale: f64) -> Option<StopwatchQuery> {
        self.started_at_ms.get(id)?;
        let now_ms = self.now_ms();
        self.query_at(id, scale, now_ms)
    }

    pub(crate) fn elapsed_seconds(&self, id: &Identifier) -> Option<f64> {
        self.started_at_ms.get(id)?;
        let now_ms = self.now_ms();
        self.elapsed_seconds_at(id, now_ms)
    }

    pub(crate) fn restart(&mut self, id: &Identifier) -> bool {
        if !self.started_at_ms.contains_key(id) {
            return false;
        }
        let now_ms = self.now_ms();
        self.restart_at(id, now_ms)
    }

    pub(crate) fn remove(&mut self, id: &Identifier) -> bool {
        self.started_at_ms.remove(id).is_some()
    }

    fn now_ms(&self) -> u128 {
        self.epoch.elapsed().as_millis()
    }

    fn create_at(&mut self, id: Identifier, now_ms: u128) -> bool {
        match self.started_at_ms.entry(id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(now_ms);
                true
            }
            std::collections::hash_map::Entry::Occupied(_) => false,
        }
    }

    fn query_at(&self, id: &Identifier, scale: f64, now_ms: u128) -> Option<StopwatchQuery> {
        let elapsed_seconds = self.elapsed_seconds_at(id, now_ms)?;
        Some(StopwatchQuery {
            elapsed_seconds,
            result: (elapsed_seconds * scale) as i32,
        })
    }

    fn elapsed_seconds_at(&self, id: &Identifier, now_ms: u128) -> Option<f64> {
        let started_at_ms = *self.started_at_ms.get(id)?;
        let elapsed_ms = now_ms
            .checked_sub(started_at_ms)
            .expect("stopwatch observations must be monotonic");
        Some(elapsed_ms as f64 / 1000.0)
    }

    fn restart_at(&mut self, id: &Identifier, now_ms: u128) -> bool {
        let Some(started_at_ms) = self.started_at_ms.get_mut(id) else {
            return false;
        };
        *started_at_ms = now_ms;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: &str) -> Identifier {
        Identifier::parse(value).unwrap()
    }

    fn state() -> StopwatchState {
        StopwatchState::new()
    }

    #[test]
    fn query_uses_integer_millisecond_observations() {
        let timer = id("example:timer");
        let mut state = state();
        assert!(state.create_at(timer.clone(), 100));

        assert_eq!(
            state.query_at(&timer, 1.0, 100),
            Some(StopwatchQuery {
                elapsed_seconds: 0.0,
                result: 0,
            })
        );
        assert_eq!(
            state.query_at(&timer, 1.0, 1099),
            Some(StopwatchQuery {
                elapsed_seconds: 0.999,
                result: 0,
            })
        );
        assert_eq!(
            state.query_at(&timer, 1.0, 1100),
            Some(StopwatchQuery {
                elapsed_seconds: 1.0,
                result: 1,
            })
        );
    }

    #[test]
    fn duplicate_create_does_not_replace_the_start_time() {
        let timer = id("example:timer");
        let mut state = state();
        assert!(state.create_at(timer.clone(), 100));
        assert!(!state.create_at(timer.clone(), 900));
        assert_eq!(state.elapsed_seconds_at(&timer, 1100), Some(1.0));
    }

    #[test]
    fn restart_requires_an_existing_stopwatch_and_resets_it() {
        let timer = id("example:timer");
        let mut state = state();
        assert!(!state.restart_at(&timer, 100));

        assert!(state.create_at(timer.clone(), 0));
        assert!(state.restart_at(&timer, 1500));
        assert_eq!(state.elapsed_seconds_at(&timer, 2499), Some(0.999));
        assert_eq!(state.elapsed_seconds_at(&timer, 2500), Some(1.0));
    }

    #[test]
    fn remove_requires_an_existing_stopwatch() {
        let timer = id("example:timer");
        let mut state = state();
        assert!(!state.remove(&timer));

        assert!(state.create_at(timer.clone(), 0));
        assert!(state.remove(&timer));
        assert!(!state.remove(&timer));
        assert_eq!(state.query_at(&timer, 1.0, 1000), None);
    }

    #[test]
    fn scaled_query_uses_java_double_to_int_conversion() {
        let timer = id("example:timer");
        let mut state = state();
        assert!(state.create_at(timer.clone(), 0));

        assert_eq!(state.query_at(&timer, -1.0, 1500).unwrap().result, -1);
        assert_eq!(
            state.query_at(&timer, f64::MAX, 1000).unwrap().result,
            i32::MAX
        );
        assert_eq!(
            state.query_at(&timer, -f64::MAX, 1000).unwrap().result,
            i32::MIN
        );
    }
}
