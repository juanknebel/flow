//! JSON export/import of a single project's cards, for backup and
//! restore (or copying a board into a different project). Column ids are
//! kept as-is since every tenant is seeded with the same standard columns;
//! card ids are not preserved on import (providers assign their own), so
//! `depends_on` references are remapped from the exported ids to the
//! newly created ones.

use serde::{Deserialize, Serialize};

use crate::model::Board;

#[derive(Serialize, Deserialize)]
pub struct ExportCard {
    pub id: String,
    pub column_id: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub assignee: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExportBoard {
    pub project: String,
    pub cards: Vec<ExportCard>,
}

/// Builds the export DTO for every card currently in `board` that belongs
/// to `project` (cards are matched by the `project` field, same as the
/// provider's own tenant scoping).
pub fn export_board(board: &Board, project: &str) -> ExportBoard {
    let mut cards = Vec::new();
    for col in &board.columns {
        for card in &col.cards {
            cards.push(ExportCard {
                id: card.id.clone(),
                column_id: col.id.clone(),
                title: card.title.clone(),
                description: card.description.clone(),
                priority: card.priority.label().to_string(),
                assignee: card.assignee.clone(),
                depends_on: card.depends_on.clone(),
            });
        }
    }
    ExportBoard {
        project: project.to_string(),
        cards,
    }
}

pub fn parse_export(json: &str) -> Result<ExportBoard, serde_json::Error> {
    serde_json::from_str(json)
}

pub fn to_json(export: &ExportBoard) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(export)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Card, Column, Priority};

    fn board() -> Board {
        Board {
            columns: vec![Column {
                id: "todo".into(),
                title: "TO DO".into(),
                cards: vec![Card {
                    id: "A-1".into(),
                    title: "t".into(),
                    description: "d".into(),
                    priority: Priority::High,
                    assignee: "me".into(),
                    project: "acme".into(),
                    depends_on: vec!["A-2".into()],
                }],
            }],
        }
    }

    #[test]
    fn export_then_import_round_trips_through_json() {
        let exported = export_board(&board(), "acme");
        let json = to_json(&exported).unwrap();
        let parsed = parse_export(&json).unwrap();

        assert_eq!(parsed.project, "acme");
        assert_eq!(parsed.cards.len(), 1);
        assert_eq!(parsed.cards[0].id, "A-1");
        assert_eq!(parsed.cards[0].column_id, "todo");
        assert_eq!(parsed.cards[0].priority, "HIGH");
        assert_eq!(parsed.cards[0].depends_on, vec!["A-2".to_string()]);
    }

    #[test]
    fn parse_export_rejects_malformed_json() {
        assert!(parse_export("not json").is_err());
    }
}
