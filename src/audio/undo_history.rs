// src/audio/undo_history.rs
// 5-level circular buffer undo/redo history for audio layers

use std::collections::VecDeque;

/// Represents a complete state snapshot of an audio layer
#[derive(Debug, Clone)]
pub struct LayerSnapshot {
    pub buffer: Vec<f32>,
    pub volume: f32,
    pub loop_start: usize,
    pub loop_end: usize,
    pub playback_position: usize,
    pub is_muted: bool,
    pub is_solo: bool,
}

impl LayerSnapshot {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            volume: 1.0,
            loop_start: 0,
            loop_end: 0,
            playback_position: 0,
            is_muted: false,
            is_solo: false,
        }
    }
}

impl Default for LayerSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// 5-level circular buffer undo/redo history
#[derive(Debug, Clone)]
pub struct UndoHistory {
    history: VecDeque<LayerSnapshot>,
    max_levels: usize,
    current_index: isize, // -1 means no current state, 0+ is index in history
}

impl UndoHistory {
    const DEFAULT_MAX_LEVELS: usize = 5;

    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(Self::DEFAULT_MAX_LEVELS),
            max_levels: Self::DEFAULT_MAX_LEVELS,
            current_index: -1,
        }
    }

    pub fn new_with_levels(max_levels: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_levels),
            max_levels,
            current_index: -1,
        }
    }

    /// Save current state to history (creates new snapshot)
    pub fn save_state(&mut self, snapshot: LayerSnapshot) {
        // If we're not at the end of history, truncate future states
        if self.current_index >= 0 {
            let truncate_from = (self.current_index + 1) as usize;
            if truncate_from < self.history.len() {
                self.history.truncate(truncate_from);
            }
        }

        // Add new state
        self.history.push_back(snapshot);
        self.current_index = (self.history.len() - 1) as isize;

        // Maintain max history size
        if self.history.len() > self.max_levels {
            self.history.pop_front();
            self.current_index = (self.history.len() - 1) as isize;
        }
    }

    /// Undo to previous state
    pub fn undo(&mut self) -> Option<LayerSnapshot> {
        if self.can_undo() {
            self.current_index -= 1;
            Some(self.history[self.current_index as usize].clone())
        } else {
            None
        }
    }

    /// Redo to next state
    pub fn redo(&mut self) -> Option<LayerSnapshot> {
        if self.can_redo() {
            self.current_index += 1;
            Some(self.history[self.current_index as usize].clone())
        } else {
            None
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.current_index > 0
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.current_index >= 0 && (self.current_index as usize) < self.history.len() - 1
    }

    /// Get current state without modifying history
    pub fn get_current(&self) -> Option<LayerSnapshot> {
        if self.current_index >= 0 && (self.current_index as usize) < self.history.len() {
            Some(self.history[self.current_index as usize].clone())
        } else {
            None
        }
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.history.clear();
        self.current_index = -1;
    }

    /// Get number of undo levels available
    pub fn undo_levels(&self) -> usize {
        if self.current_index > 0 {
            self.current_index as usize
        } else {
            0
        }
    }

    /// Get number of redo levels available
    pub fn redo_levels(&self) -> usize {
        if self.current_index >= 0 && (self.current_index as usize) < self.history.len() - 1 {
            (self.history.len() - 1) - (self.current_index as usize)
        } else {
            0
        }
    }

    /// Get total history size
    pub fn history_size(&self) -> usize {
        self.history.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_undo_redo() {
        let mut history = UndoHistory::new();

        // Save initial state
        let mut snapshot = LayerSnapshot::new();
        snapshot.buffer = vec![1.0, 2.0, 3.0];
        history.save_state(snapshot);

        // Save second state
        let mut snapshot2 = LayerSnapshot::new();
        snapshot2.buffer = vec![4.0, 5.0, 6.0];
        history.save_state(snapshot2);

        // Test redo (should go back to first state)
        let undo_result = history.undo().unwrap();
        assert_eq!(undo_result.buffer, vec![1.0, 2.0, 3.0]);

        // Test redo (should go forward to second state)
        let redo_result = history.redo().unwrap();
        assert_eq!(redo_result.buffer, vec![4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_history_limits() {
        let mut history = UndoHistory::new_with_levels(3);

        // Add more than max levels
        for i in 0..6 {
            let mut snapshot = LayerSnapshot::new();
            snapshot.buffer = vec![i as f32];
            history.save_state(snapshot);
        }

        // Should only keep last 3 states
        assert_eq!(history.history_size(), 3);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Should be able to undo 2 levels
        assert_eq!(history.undo_levels(), 2);
    }

    #[test]
    fn test_can_undo_redo() {
        let mut history = UndoHistory::new();

        // Initially no undo/redo available
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // After one state, no undo available
        let snapshot = LayerSnapshot::new();
        history.save_state(snapshot);
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // After second state, undo available
        let snapshot2 = LayerSnapshot::new();
        history.save_state(snapshot2);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // After undo, redo available
        history.undo().unwrap();
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn test_future_truncation() {
        let mut history = UndoHistory::new();

        // Add 3 states
        for i in 0..3 {
            let mut snapshot = LayerSnapshot::new();
            snapshot.buffer = vec![i as f32];
            history.save_state(snapshot);
        }

        // Undo one step
        history.undo().unwrap();
        assert_eq!(history.redo_levels(), 1);

        // Save new state (should truncate future)
        let mut snapshot = LayerSnapshot::new();
        snapshot.buffer = vec![99.0];
        history.save_state(snapshot);

        // Should not be able to redo to old future state
        assert!(!history.can_redo());
    }
}
