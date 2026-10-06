//! Windows can omit a requested RedrawRequested event for an unfocused window.
//! Keep its oldest request until a paint actually happens, rather than losing it
//! at request_redraw(). A later delivered paint can service older requests.
//! Adapted from https://github.com/emilk/egui/pull/8650 (MIT OR Apache-2.0).
use std::{collections::HashMap, hash::Hash};

pub(super) struct RedrawLedger<Id> {
    next: u64,
    asked: HashMap<Id, u64>,
}

impl<Id> Default for RedrawLedger<Id> {
    fn default() -> Self {
        Self {
            next: 0,
            asked: HashMap::new(),
        }
    }
}

impl<Id: Copy + Eq + Hash> RedrawLedger<Id> {
    pub fn asked(&mut self, window: Id) {
        let order = self.next;
        self.next += 1;
        self.asked.entry(window).or_insert(order);
    }

    pub fn painted(&mut self, window: Id) -> Option<u64> {
        self.asked.remove(&window)
    }

    pub fn next_older_than(&mut self, order: u64) -> Option<Id> {
        let (_, window) = self
            .asked
            .iter()
            .filter_map(|(&window, &asked)| (asked < order).then_some((asked, window)))
            .min_by_key(|(asked, _)| *asked)?;
        self.asked.remove(&window);
        Some(window)
    }
}

#[cfg(test)]
mod tests {
    use super::RedrawLedger;

    #[test]
    fn delivered_panel_paint_recovers_overlay_with_no_os_redraw_event() {
        let mut ledger = RedrawLedger::default();
        ledger.asked("overlay");
        ledger.asked("panel");
        // No overlay RedrawRequested arrives. Panel's paint must still recover it.
        let order = ledger.painted("panel").unwrap();
        assert_eq!(ledger.next_older_than(order), Some("overlay"));
        assert_eq!(ledger.next_older_than(order), None);
    }

    #[test]
    fn delivered_overlay_paint_also_recovers_panel() {
        let mut ledger = RedrawLedger::default();
        ledger.asked("panel");
        ledger.asked("overlay");
        let order = ledger.painted("overlay").unwrap();
        assert_eq!(ledger.next_older_than(order), Some("panel"));
    }

    #[test]
    fn repeated_requests_preserve_oldest_order_and_budget_defers_remaining() {
        let mut ledger = RedrawLedger::default();
        ledger.asked(1_u8);
        ledger.asked(2_u8);
        ledger.asked(1_u8);
        ledger.asked(3_u8);
        let order = ledger.painted(3).unwrap();
        assert_eq!(ledger.next_older_than(order), Some(1));
        // Simulate budget exhaustion: 2 stays pending until a later delivered paint.
        ledger.asked(3);
        let later = ledger.painted(3).unwrap();
        assert_eq!(ledger.next_older_than(later), Some(2));
        assert_eq!(ledger.next_older_than(later), None);
    }

    #[test]
    fn newer_requests_are_not_painted_early() {
        let mut ledger = RedrawLedger::default();
        ledger.asked(1_u8);
        ledger.asked(2_u8);
        let order = ledger.painted(1).unwrap();
        assert_eq!(ledger.next_older_than(order), None);
        assert!(ledger.painted(2).is_some());
    }
}
