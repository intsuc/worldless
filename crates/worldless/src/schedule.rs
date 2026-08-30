use std::collections::{BTreeMap, HashMap, VecDeque, btree_map::Entry};

use crate::resource::FunctionReference;

#[derive(Debug, Default)]
pub(crate) struct ScheduleState {
    current_tick: i64,
    sequential_id: u64,
    queue: BTreeMap<(i64, u64), VecDeque<FunctionReference>>,
    events: HashMap<FunctionReference, BTreeMap<i64, u64>>,
}

impl ScheduleState {
    pub(crate) fn schedule(
        &mut self,
        reference: FunctionReference,
        delay: i32,
        replace: bool,
    ) -> i64 {
        let due_tick = self.current_tick.wrapping_add(i64::from(delay));
        if replace {
            self.clear(&reference);
        }

        let due_events = self.events.entry(reference.clone()).or_default();
        if let Entry::Vacant(event) = due_events.entry(due_tick) {
            self.sequential_id = self.sequential_id.wrapping_add(1);
            event.insert(self.sequential_id);
            self.queue
                .entry((due_tick, self.sequential_id))
                .or_default()
                .push_back(reference);
        }
        due_tick
    }

    pub(crate) fn clear(&mut self, reference: &FunctionReference) -> usize {
        let Some(due_events) = self.events.remove(reference) else {
            return 0;
        };
        let removed = due_events.len();
        for (due_tick, sequential_id) in due_events {
            let key = (due_tick, sequential_id);
            let remove_bucket = if let Some(callbacks) = self.queue.get_mut(&key) {
                if let Some(index) = callbacks.iter().position(|callback| callback == reference) {
                    callbacks.remove(index);
                }
                callbacks.is_empty()
            } else {
                false
            };
            if remove_bucket {
                self.queue.remove(&key);
            }
        }
        removed
    }

    pub(crate) fn advance(&mut self) {
        self.current_tick = self.current_tick.wrapping_add(1);
    }

    pub(crate) fn pop_due(&mut self) -> Option<FunctionReference> {
        let key = *self.queue.first_key_value()?.0;
        let due_tick = key.0;
        if due_tick > self.current_tick {
            return None;
        }

        let callbacks = self
            .queue
            .get_mut(&key)
            .expect("first scheduled tick must remain present");
        let callback = callbacks
            .pop_front()
            .expect("scheduled tick must contain a callback");
        if callbacks.is_empty() {
            self.queue.remove(&key);
        }
        // TimerQueue.tick removes the index cell at the tick being processed,
        // even when long overflow made the priority event due at another tick.
        let remove_row = if let Some(due_events) = self.events.get_mut(&callback) {
            due_events.remove(&self.current_tick);
            due_events.is_empty()
        } else {
            false
        };
        if remove_row {
            self.events.remove(&callback);
        }
        Some(callback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::Identifier;

    fn function(value: &str) -> FunctionReference {
        FunctionReference::Function(Identifier::parse(value).unwrap())
    }

    fn tag(value: &str) -> FunctionReference {
        FunctionReference::Tag(Identifier::parse(value).unwrap())
    }

    #[test]
    fn callbacks_are_popped_by_due_tick_then_registration_order() {
        let mut state = ScheduleState::default();
        let first = function("example:first");
        let second = function("example:second");
        let earlier = function("example:earlier");

        state.schedule(first.clone(), 2, false);
        state.schedule(second.clone(), 2, false);
        state.schedule(earlier.clone(), 1, false);

        assert_eq!(state.pop_due(), None);
        state.advance();
        assert_eq!(state.pop_due(), Some(earlier));
        assert_eq!(state.pop_due(), None);
        state.advance();
        assert_eq!(state.pop_due(), Some(first));
        assert_eq!(state.pop_due(), Some(second));
        assert_eq!(state.pop_due(), None);
    }

    #[test]
    fn append_deduplicates_only_the_same_identity_at_the_same_tick() {
        let mut state = ScheduleState::default();
        let function = function("example:callback");
        let tag = tag("example:callback");

        state.schedule(function.clone(), 1, false);
        state.schedule(function.clone(), 1, false);
        state.schedule(tag.clone(), 1, false);

        state.advance();
        assert_eq!(state.pop_due(), Some(function));
        assert_eq!(state.pop_due(), Some(tag));
        assert_eq!(state.pop_due(), None);
    }

    #[test]
    fn replace_removes_every_tick_for_only_the_replaced_identity() {
        let mut state = ScheduleState::default();
        let replaced = function("example:replaced");
        let preserved = function("example:preserved");

        state.schedule(replaced.clone(), 1, false);
        state.schedule(replaced.clone(), 2, false);
        state.schedule(preserved.clone(), 2, false);
        state.schedule(replaced.clone(), 3, true);

        state.advance();
        assert_eq!(state.pop_due(), None);
        state.advance();
        assert_eq!(state.pop_due(), Some(preserved));
        assert_eq!(state.pop_due(), None);
        state.advance();
        assert_eq!(state.pop_due(), Some(replaced));
        assert_eq!(state.pop_due(), None);
    }

    #[test]
    fn clear_reports_removed_callbacks_and_can_remove_later_due_work() {
        let mut state = ScheduleState::default();
        let first = function("example:first");
        let cleared = function("example:cleared");

        state.schedule(first.clone(), 1, false);
        state.schedule(cleared.clone(), 1, false);
        state.schedule(cleared.clone(), 2, false);
        state.advance();

        assert_eq!(state.pop_due(), Some(first));
        assert_eq!(state.clear(&cleared), 2);
        assert_eq!(state.clear(&cleared), 0);
        assert_eq!(state.pop_due(), None);
        state.advance();
        assert_eq!(state.pop_due(), None);
    }

    #[test]
    fn due_tick_and_advancement_wrap_as_java_long_arithmetic() {
        let mut state = ScheduleState {
            current_tick: i64::MAX,
            ..ScheduleState::default()
        };
        let callback = function("example:wrapped");

        assert_eq!(state.schedule(callback.clone(), 1, false), i64::MIN);
        state.advance();
        assert_eq!(state.current_tick, i64::MIN);
        assert_eq!(state.pop_due(), Some(callback));
    }

    #[test]
    fn an_early_overflow_callback_retains_the_targets_stale_index_entry() {
        let mut state = ScheduleState {
            current_tick: i64::MAX - 1,
            ..ScheduleState::default()
        };
        let callback = function("example:wrapped");

        assert_eq!(state.schedule(callback.clone(), 2, false), i64::MIN);
        state.advance();
        assert_eq!(state.pop_due(), Some(callback.clone()));
        assert_eq!(state.clear(&callback), 1);
    }

    #[test]
    fn wrapped_unsigned_sequence_ids_still_determine_same_tick_order() {
        let mut state = ScheduleState {
            sequential_id: u64::MAX - 1,
            ..ScheduleState::default()
        };
        let older = function("example:older");
        let wrapped = function("example:wrapped");

        state.schedule(older.clone(), 1, false);
        state.schedule(wrapped.clone(), 1, false);
        state.advance();
        assert_eq!(state.pop_due(), Some(wrapped));
        assert_eq!(state.pop_due(), Some(older));
    }
}
