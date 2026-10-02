use crate::model::{Difficulty, MonsterType};

/// A monster's desecrated (Terror Zone) level for a given character level: only actually terrorized
/// once the character is at or above the monster's normal effective level, then bumped by the
/// monster-type-specific adjustment and capped per difficulty.
pub fn desecrated_level(
    monster_type: MonsterType,
    difficulty: Difficulty,
    normal_effective_level: i32,
    character_level: i32,
) -> i32 {
    if character_level < normal_effective_level {
        return normal_effective_level;
    }
    let new_level = character_level + monster_type.desecrated_level_adjustment();
    new_level.min(monster_type.desecrated_level_limit(difficulty))
}
