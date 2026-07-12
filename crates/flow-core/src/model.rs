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
    pub project: String,
    /// Ids of other cards this one depends on (blocks on). An empty vec
    /// means no dependencies.
    pub depends_on: Vec<String>,
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

    /// Sort cards in every column grouped by project, then by priority in the given order,
    /// then title (ascending). Cards without a project are placed last.
    pub fn sort_cards_with(&mut self, order: SortOrder) {
        for col in &mut self.columns {
            col.cards.sort_by(|a, b| {
                let proj_cmp = match (a.project.is_empty(), b.project.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => a.project.to_lowercase().cmp(&b.project.to_lowercase()),
                };
                proj_cmp
                    .then_with(|| {
                        match order {
                            SortOrder::Asc => a.priority.sort_key().cmp(&b.priority.sort_key()),
                            SortOrder::Desc => b.priority.sort_key().cmp(&a.priority.sort_key()),
                        }
                    })
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            });
        }
    }

    /// Return all unique project names across all columns, sorted alphabetically.
    pub fn projects(&self) -> Vec<String> {
        let mut set = std::collections::BTreeSet::new();
        for col in &self.columns {
            for card in &col.cards {
                if !card.project.is_empty() {
                    set.insert(card.project.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// Filter out cards that don't match the given project filter.
    /// An empty filter means show all cards.
    pub fn apply_project_filter(&mut self, filter: &[String]) {
        if filter.is_empty() {
            return;
        }
        for col in &mut self.columns {
            col.cards.retain(|card| {
                if card.project.is_empty() {
                    filter.iter().any(|f| f.is_empty())
                } else {
                    filter.contains(&card.project)
                }
            });
        }
    }

    /// Find a card by id anywhere on the board.
    pub fn find_card(&self, id: &str) -> Option<&Card> {
        self.columns
            .iter()
            .flat_map(|col| col.cards.iter())
            .find(|c| c.id == id)
    }

    /// All cards (other than `id` itself) whose `depends_on` list contains `id`.
    pub fn dependents_of(&self, id: &str) -> Vec<&Card> {
        self.columns
            .iter()
            .flat_map(|col| col.cards.iter())
            .filter(|c| c.id != id && c.depends_on.iter().any(|d| d == id))
            .collect()
    }

    /// Check whether setting `card_id`'s dependencies to `new_deps` would
    /// introduce a dependency cycle (direct or transitive) that loops back
    /// to `card_id`. Only `card_id`'s outgoing edges are hypothetically
    /// replaced; every other card's `depends_on` is taken from the current
    /// board state. Returns the cycle path (e.g. `["A", "B", "C", "A"]`) if
    /// one is found.
    pub fn find_dependency_cycle(&self, card_id: &str, new_deps: &[String]) -> Option<Vec<String>> {
        use std::collections::{HashMap, HashSet};

        let mut graph: HashMap<String, Vec<String>> = HashMap::new();
        for col in &self.columns {
            for card in &col.cards {
                graph.insert(card.id.clone(), card.depends_on.clone());
            }
        }
        graph.insert(card_id.to_string(), new_deps.to_vec());

        fn dfs(
            graph: &HashMap<String, Vec<String>>,
            current: &str,
            target: &str,
            visited: &mut HashSet<String>,
            path: &mut Vec<String>,
        ) -> Option<Vec<String>> {
            let deps = graph.get(current)?;
            for dep in deps {
                if dep == target {
                    let mut cycle = path.clone();
                    cycle.push(dep.clone());
                    return Some(cycle);
                }
                if visited.contains(dep) {
                    continue;
                }
                visited.insert(dep.clone());
                path.push(dep.clone());
                if let Some(cycle) = dfs(graph, dep, target, visited, path) {
                    return Some(cycle);
                }
                path.pop();
            }
            None
        }

        let mut visited = HashSet::new();
        visited.insert(card_id.to_string());
        let mut path = vec![card_id.to_string()];
        dfs(&graph, card_id, card_id, &mut visited, &mut path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, deps: &[&str]) -> Card {
        Card {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            priority: Priority::Medium,
            assignee: String::new(),
            project: String::new(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn board(cards: Vec<Card>) -> Board {
        Board {
            columns: vec![Column {
                id: "todo".into(),
                title: "Todo".into(),
                cards,
            }],
        }
    }

    #[test]
    fn find_card_locates_card_across_columns() {
        let b = board(vec![card("A", &[]), card("B", &[])]);
        assert!(b.find_card("A").is_some());
        assert!(b.find_card("MISSING").is_none());
    }

    #[test]
    fn dependents_of_finds_all_cards_pointing_at_id() {
        let b = board(vec![
            card("A", &[]),
            card("B", &["A"]),
            card("C", &["A", "B"]),
        ]);
        let deps: Vec<&str> = b.dependents_of("A").iter().map(|c| c.id.as_str()).collect();
        assert_eq!(deps, vec!["B", "C"]);
    }

    #[test]
    fn find_dependency_cycle_detects_direct_cycle() {
        let b = board(vec![card("A", &["B"]), card("B", &[])]);
        // Proposing B -> A would create A -> B -> A.
        let cycle = b.find_dependency_cycle("B", &["A".to_string()]);
        assert_eq!(cycle, Some(vec!["B".to_string(), "A".to_string(), "B".to_string()]));
    }

    #[test]
    fn find_dependency_cycle_detects_transitive_cycle() {
        let b = board(vec![
            card("A", &["B"]),
            card("B", &["C"]),
            card("C", &[]),
        ]);
        // Proposing C -> A would create A -> B -> C -> A.
        let cycle = b.find_dependency_cycle("C", &["A".to_string()]);
        assert_eq!(
            cycle,
            Some(vec!["C".to_string(), "A".to_string(), "B".to_string(), "C".to_string()])
        );
    }

    #[test]
    fn find_dependency_cycle_none_when_acyclic() {
        let b = board(vec![
            card("A", &["B"]),
            card("B", &["C"]),
            card("C", &[]),
        ]);
        assert_eq!(b.find_dependency_cycle("A", &["B".to_string()]), None);
    }
}
