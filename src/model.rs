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
    /// Sort cards in every column by priority (descending) then title (ascending).
    pub fn sort_cards(&mut self) {
        for col in &mut self.columns {
            col.cards.sort_by(|a, b| {
                a.priority.sort_key().cmp(&b.priority.sort_key())
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
        }
    }
}
