#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// Higher priority first (Bug → High → Medium → Low → Wishlist)
    Asc,
    /// Lower priority first (Wishlist → Low → Medium → High → Bug)
    Desc,
}

impl SortOrder {
    pub fn toggle(self) -> Self {
        match self {
            SortOrder::Asc => SortOrder::Desc,
            SortOrder::Desc => SortOrder::Asc,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SortOrder::Asc => "↑",
            SortOrder::Desc => "↓",
        }
    }
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    Low,
    Medium,
    High,
    Bug,
    Wishlist,
}

impl Priority {
    pub fn label(&self) -> &'static str {
        match self {
            Priority::Low => "LOW",
            Priority::Medium => "MEDIUM",
            Priority::High => "HIGH",
            Priority::Bug => "BUG",
            Priority::Wishlist => "WISHLIST",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Priority::Low => "L",
            Priority::Medium => "M",
            Priority::High => "H",
            Priority::Bug => "BUG",
            Priority::Wishlist => "W",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "low" | "l" => Priority::Low,
            "high" | "h" => Priority::High,
            "bug" => Priority::Bug,
            "wishlist" | "wish" | "w" => Priority::Wishlist,
            _ => Priority::Medium,
        }
    }

    pub fn next(&self) -> Self {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Bug,
            Priority::Bug => Priority::Wishlist,
            Priority::Wishlist => Priority::Low,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Priority::Low => Priority::Wishlist,
            Priority::Medium => Priority::Low,
            Priority::High => Priority::Medium,
            Priority::Bug => Priority::High,
            Priority::Wishlist => Priority::Bug,
        }
    }

    /// Sort key: lower value = higher priority.
    pub fn sort_key(&self) -> u8 {
        match self {
            Priority::Bug => 0,
            Priority::High => 1,
            Priority::Medium => 2,
            Priority::Low => 3,
            Priority::Wishlist => 4,
        }
    }
}

pub struct Card {
    pub id: String,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub assignee: String,
}

pub struct Column {
    pub id: String,
    pub title: String,
    pub cards: Vec<Card>,
}

pub struct Board {
    pub columns: Vec<Column>,
}

impl Board {
    /// Sort cards in every column by priority then title (ascending).
    pub fn sort_cards(&mut self) {
        self.sort_cards_with(SortOrder::Asc);
    }

    /// Sort cards in every column by priority in the given order, then title (ascending).
    pub fn sort_cards_with(&mut self, order: SortOrder) {
        for col in &mut self.columns {
            col.cards.sort_by(|a, b| {
                let prio_cmp = match order {
                    SortOrder::Asc => a.priority.sort_key().cmp(&b.priority.sort_key()),
                    SortOrder::Desc => b.priority.sort_key().cmp(&a.priority.sort_key()),
                };
                prio_cmp.then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
        }
    }
}
