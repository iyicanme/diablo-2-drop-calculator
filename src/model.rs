#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Difficulty {
    Normal,
    Nightmare,
    Hell,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Normal, Difficulty::Nightmare, Difficulty::Hell];

    pub fn label(&self) -> &'static str {
        match self {
            Difficulty::Normal => "Normal",
            Difficulty::Nightmare => "Nightmare",
            Difficulty::Hell => "Hell",
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self {
            Difficulty::Normal => "",
            Difficulty::Nightmare => "(N)",
            Difficulty::Hell => "(H)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MonsterType {
    Regular,
    Champion,
    Unique,
    Minion,
    Boss,
    SuperUnique,
}

impl MonsterType {
    pub fn desecrated_level_adjustment(&self) -> i32 {
        match self {
            MonsterType::Regular => 2,
            MonsterType::Champion => 4,
            MonsterType::Unique | MonsterType::Minion | MonsterType::Boss | MonsterType::SuperUnique => 5,
        }
    }

    pub fn desecrated_level_limit(&self, difficulty: Difficulty) -> i32 {
        let (n, nm, h) = match self {
            MonsterType::Regular => (45, 71, 96),
            MonsterType::Champion => (47, 73, 98),
            MonsterType::Unique | MonsterType::Minion | MonsterType::Boss | MonsterType::SuperUnique => (48, 74, 99),
        };
        match difficulty {
            Difficulty::Normal => n,
            Difficulty::Nightmare => nm,
            Difficulty::Hell => h,
        }
    }

    pub fn level_adjustment(&self) -> i32 {
        match self {
            MonsterType::Boss => 0,
            MonsterType::Champion => 2,
            MonsterType::Unique | MonsterType::Minion | MonsterType::SuperUnique => 3,
            MonsterType::Regular => 0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            MonsterType::Regular => "Regular",
            MonsterType::Champion => "Champion",
            MonsterType::Unique => "Unique",
            MonsterType::Minion => "Minion",
            MonsterType::Boss => "Boss",
            MonsterType::SuperUnique => "Super Unique",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreasureClassType {
    Regular,
    Champion,
    Unique,
    Quest,
    DesecratedRegular,
    DesecratedChampion,
    DesecratedUnique,
    HeraldRegular,
}

impl TreasureClassType {
    pub fn valid_monster_types(&self) -> &'static [MonsterType] {
        match self {
            TreasureClassType::Regular => &[MonsterType::Regular, MonsterType::Boss, MonsterType::SuperUnique],
            TreasureClassType::Champion => &[MonsterType::Champion],
            TreasureClassType::Unique => &[MonsterType::Unique],
            TreasureClassType::Quest => &[MonsterType::Regular, MonsterType::Boss],
            TreasureClassType::DesecratedRegular => {
                &[MonsterType::Regular, MonsterType::Boss, MonsterType::SuperUnique]
            }
            TreasureClassType::DesecratedChampion => &[MonsterType::Champion],
            TreasureClassType::DesecratedUnique => &[MonsterType::Unique],
            TreasureClassType::HeraldRegular => &[MonsterType::Regular, MonsterType::Boss],
        }
    }

    pub fn is_desecrated(&self) -> bool {
        matches!(
            self,
            TreasureClassType::DesecratedRegular
                | TreasureClassType::DesecratedChampion
                | TreasureClassType::DesecratedUnique
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemQuality {
    White,
    Magic,
    Rare,
    Set,
    Unique,
}

impl ItemQuality {
    pub const ALL_DESCENDING: [ItemQuality; 5] = [
        ItemQuality::Unique,
        ItemQuality::Set,
        ItemQuality::Rare,
        ItemQuality::Magic,
        ItemQuality::White,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ItemQuality::White => "White",
            ItemQuality::Magic => "Magic",
            ItemQuality::Rare => "Rare",
            ItemQuality::Set => "Set",
            ItemQuality::Unique => "Unique",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemTier {
    Normal,
    Exceptional,
    Elite,
    Consumable,
    Rune,
    Gem,
}

impl ItemTier {
    pub fn label(&self) -> &'static str {
        match self {
            ItemTier::Normal => "Normal",
            ItemTier::Exceptional => "Exceptional",
            ItemTier::Elite => "Elite",
            ItemTier::Consumable => "Consumable",
            ItemTier::Rune => "Rune",
            ItemTier::Gem => "Gem",
        }
    }
}

/// Unique/Set/Rare/Magic quality-ratio weights (out of 1024), merged (max) down a TC path.
#[derive(Debug, Clone, Copy, Default)]
pub struct QualityRatios {
    pub unique: i64,
    pub set: i64,
    pub rare: i64,
    pub magic: i64,
}

impl QualityRatios {
    pub fn merge(&self, other: &QualityRatios) -> QualityRatios {
        QualityRatios {
            unique: self.unique.max(other.unique),
            set: self.set.max(other.set),
            rare: self.rare.max(other.rare),
            magic: self.magic.max(other.magic),
        }
    }

    pub fn get(&self, quality: ItemQuality) -> i64 {
        match quality {
            ItemQuality::Unique => self.unique,
            ItemQuality::Set => self.set,
            ItemQuality::Rare => self.rare,
            ItemQuality::Magic => self.magic,
            ItemQuality::White => 0,
        }
    }
}
