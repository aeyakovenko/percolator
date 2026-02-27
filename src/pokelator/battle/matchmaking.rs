use super::{Battle, BattleEntry, BattleStatus};

/// Maximum power rating difference for a valid match.
const MAX_POWER_DIFF: u64 = 500;

/// Maximum time in slots before a queued entry expires.
const QUEUE_EXPIRY_SLOTS: u64 = 300; // ~2 minutes at 400ms slots

/// Matchmaking queue for pairing battle entries.
pub struct MatchmakingQueue {
    pub entries: Vec<QueueEntry>,
    pub next_battle_id: u64,
}

#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub entry: BattleEntry,
    pub power_rating: u64,
    pub queued_slot: u64,
}

impl MatchmakingQueue {
    pub fn new() -> Self {
        MatchmakingQueue {
            entries: Vec::new(),
            next_battle_id: 1,
        }
    }

    /// Adds a player to the matchmaking queue.
    pub fn enqueue(
        &mut self,
        entry: BattleEntry,
        power_rating: u64,
        current_slot: u64,
    ) {
        self.entries.push(QueueEntry {
            entry,
            power_rating,
            queued_slot: current_slot,
        });
    }

    /// Attempts to find a match for a given queue entry.
    /// Returns a Battle if a suitable opponent is found.
    pub fn try_match(&mut self, current_slot: u64) -> Option<Battle> {
        // Remove expired entries first
        self.entries.retain(|e| {
            current_slot - e.queued_slot < QUEUE_EXPIRY_SLOTS
        });

        if self.entries.len() < 2 {
            return None;
        }

        // Find the closest power-rated pair
        let mut best_pair: Option<(usize, usize, u64)> = None;

        for i in 0..self.entries.len() {
            for j in (i + 1)..self.entries.len() {
                // Don't match players against themselves
                if self.entries[i].entry.owner == self.entries[j].entry.owner {
                    continue;
                }

                let diff = if self.entries[i].power_rating > self.entries[j].power_rating {
                    self.entries[i].power_rating - self.entries[j].power_rating
                } else {
                    self.entries[j].power_rating - self.entries[i].power_rating
                };

                if diff <= MAX_POWER_DIFF {
                    match best_pair {
                        None => best_pair = Some((i, j, diff)),
                        Some((_, _, best_diff)) if diff < best_diff => {
                            best_pair = Some((i, j, diff));
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some((i, j, _)) = best_pair {
            // Remove in reverse order to preserve indices
            let entry_b = self.entries.remove(j);
            let entry_a = self.entries.remove(i);

            let battle_id = self.next_battle_id;
            self.next_battle_id += 1;

            Some(Battle::new(
                battle_id,
                entry_a.entry,
                entry_b.entry,
                current_slot,
            ))
        } else {
            None
        }
    }

    /// Returns the current queue depth.
    pub fn queue_depth(&self) -> usize {
        self.entries.len()
    }

    /// Cancels a queued entry by owner. Returns true if found and removed.
    pub fn cancel(&mut self, owner: &[u8; 32]) -> bool {
        let initial_len = self.entries.len();
        self.entries.retain(|e| &e.entry.owner != owner);
        self.entries.len() < initial_len
    }
}
