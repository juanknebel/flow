use serde::Serialize;

use crate::model::{Board, Card};

#[derive(Clone, Copy)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Format {
    Plain,
    Json,
    Xml,
    Csv,
    Table,
    Markdown,
}

// ---- Board listing ----

#[derive(Serialize)]
struct BoardOut {
    columns: Vec<ColumnOut>,
}

#[derive(Serialize)]
struct ColumnOut {
    id: String,
    title: String,
    cards: Vec<CardOut>,
}

#[derive(Serialize)]
struct CardOut {
    id: String,
    title: String,
    priority: String,
    project: String,
}

pub fn format_board(board: &Board, fmt: Format) -> Result<String, serde_json::Error> {
    Ok(match fmt {
        Format::Plain => {
            let mut out = String::new();
            for col in &board.columns {
                out.push_str(&format!("== {} ({}) ==\n", col.title, col.cards.len()));
                let mut last_project: Option<&str> = None;
                for card in &col.cards {
                    let proj = if card.project.is_empty() { "(sin proyecto)" } else { &card.project };
                    if last_project != Some(proj) {
                        out.push_str(&format!("  [{}]\n", proj));
                        last_project = Some(proj);
                    }
                    out.push_str(&format!("  {}  {}  {}\n", card.id, card.priority.label(), card.title));
                }
                out.push('\n');
            }
            chomp(out)
        }
        Format::Json => serde_json::to_string_pretty(&board_dto(board))?,
        Format::Xml => {
            let mut out = String::from("<board>\n");
            for col in &board.columns {
                out.push_str(&format!(
                    "  <column id=\"{}\" title=\"{}\">\n",
                    xml_esc(&col.id),
                    xml_esc(&col.title)
                ));
                for card in &col.cards {
                    out.push_str(&format!(
                        "    <card id=\"{}\" title=\"{}\" priority=\"{}\" project=\"{}\"/>\n",
                        xml_esc(&card.id),
                        xml_esc(&card.title),
                        xml_esc(card.priority.label()),
                        xml_esc(&card.project),
                    ));
                }
                out.push_str("  </column>\n");
            }
            out.push_str("</board>");
            out
        }
        Format::Csv => {
            let mut out = String::from("column_id,column_title,card_id,card_title,priority,project\n");
            for col in &board.columns {
                for card in &col.cards {
                    out.push_str(&format!(
                        "{},{},{},{},{},{}\n",
                        csv_esc(&col.id),
                        csv_esc(&col.title),
                        csv_esc(&card.id),
                        csv_esc(&card.title),
                        csv_esc(card.priority.label()),
                        csv_esc(&card.project),
                    ));
                }
            }
            chomp(out)
        }
        Format::Table => {
            let headers = ["COLUMN", "PROJECT", "ID", "PRIORITY", "TITLE"];
            let mut rows = Vec::new();
            for col in &board.columns {
                if col.cards.is_empty() {
                    rows.push(vec![col.title.clone(), String::new(), String::new(), String::new(), String::new()]);
                } else {
                    for card in &col.cards {
                        rows.push(vec![
                            col.title.clone(),
                            card.project.clone(),
                            card.id.clone(),
                            card.priority.label().to_string(),
                            card.title.clone(),
                        ]);
                    }
                }
            }
            format_table(&headers, &rows)
        }
        Format::Markdown => {
            let mut out = String::new();
            for col in &board.columns {
                out.push_str(&format!("## {} ({})\n", col.title, col.cards.len()));
                let mut last_project: Option<&str> = None;
                for card in &col.cards {
                    let proj = if card.project.is_empty() { "(sin proyecto)" } else { &card.project };
                    if last_project != Some(proj) {
                        out.push_str(&format!("### {}\n", proj));
                        last_project = Some(proj);
                    }
                    out.push_str(&format!("- **{}** [{}] {}\n", card.id, card.priority.label(), card.title));
                }
                out.push('\n');
            }
            chomp(out)
        }
    })
}

fn board_dto(board: &Board) -> BoardOut {
    BoardOut {
        columns: board
            .columns
            .iter()
            .map(|col| ColumnOut {
                id: col.id.clone(),
                title: col.title.clone(),
                cards: col
                    .cards
                    .iter()
                    .map(|c| CardOut {
                        id: c.id.clone(),
                        title: c.title.clone(),
                        priority: c.priority.label().to_string(),
                        project: c.project.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

// ---- Card detail ----

#[derive(Serialize)]
struct CardDetailOut {
    id: String,
    title: String,
    description: String,
    priority: String,
    assignee: String,
    project: String,
    column_id: String,
    column_title: String,
}

pub fn format_card(card: &Card, col_id: &str, col_title: &str, fmt: Format) -> Result<String, serde_json::Error> {
    Ok(match fmt {
        Format::Plain => {
            let mut out = format!("{}\n{}\npriority: {}\ncolumn: {} ({})\n", card.id, card.title, card.priority.label(), col_title, col_id);
            if !card.project.is_empty() {
                out.push_str(&format!("project: {}\n", card.project));
            }
            if !card.assignee.is_empty() {
                out.push_str(&format!("assignee: {}\n", card.assignee));
            }
            if !card.description.trim().is_empty() {
                out.push('\n');
                out.push_str(&card.description);
            }
            chomp(out)
        }
        Format::Json => serde_json::to_string_pretty(&CardDetailOut {
            id: card.id.clone(),
            title: card.title.clone(),
            description: card.description.clone(),
            priority: card.priority.label().to_string(),
            assignee: card.assignee.clone(),
            project: card.project.clone(),
            column_id: col_id.to_string(),
            column_title: col_title.to_string(),
        })
        ?,
        Format::Xml => {
            let mut out = format!(
                "<card id=\"{}\" title=\"{}\" priority=\"{}\" project=\"{}\" column_id=\"{}\" column_title=\"{}\">",
                xml_esc(&card.id),
                xml_esc(&card.title),
                xml_esc(card.priority.label()),
                xml_esc(&card.project),
                xml_esc(col_id),
                xml_esc(col_title),
            );
            if !card.description.trim().is_empty() {
                out.push_str(&format!(
                    "\n  <description>{}</description>\n",
                    xml_esc(&card.description)
                ));
            }
            out.push_str("</card>");
            out
        }
        Format::Csv => {
            let mut out = String::from("id,title,description,priority,assignee,project,column_id,column_title\n");
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}",
                csv_esc(&card.id),
                csv_esc(&card.title),
                csv_esc(&card.description),
                csv_esc(card.priority.label()),
                csv_esc(&card.assignee),
                csv_esc(&card.project),
                csv_esc(col_id),
                csv_esc(col_title),
            ));
            out
        }
        Format::Table => {
            let headers = ["FIELD", "VALUE"];
            let mut rows = vec![
                vec!["id".to_string(), card.id.clone()],
                vec!["title".to_string(), card.title.clone()],
                vec!["priority".to_string(), card.priority.label().to_string()],
                vec!["project".to_string(), card.project.clone()],
                vec!["column".to_string(), format!("{col_title} ({col_id})")],
            ];
            if !card.assignee.is_empty() {
                rows.push(vec!["assignee".to_string(), card.assignee.clone()]);
            }
            rows.push(vec!["description".to_string(), card.description.clone()]);
            format_table(&headers, &rows)
        }
        Format::Markdown => {
            let mut out = format!("# {}\n**{}** | Priority: {}\n\nColumn: {} (`{}`)\n", card.title, card.id, card.priority.label(), col_title, col_id);
            if !card.project.is_empty() {
                out.push_str(&format!("Project: {}\n", card.project));
            }
            if !card.assignee.is_empty() {
                out.push_str(&format!("Assignee: {}\n", card.assignee));
            }
            if !card.description.trim().is_empty() {
                out.push_str(&format!("\n---\n\n{}\n", card.description));
            }
            chomp(out)
        }
    })
}

// ---- Columns listing ----

#[derive(Serialize)]
struct ColumnsOut {
    columns: Vec<ColumnInfoOut>,
}

#[derive(Serialize)]
struct ColumnInfoOut {
    id: String,
    title: String,
    count: usize,
}

pub fn format_columns(board: &Board, fmt: Format) -> Result<String, serde_json::Error> {
    Ok(match fmt {
        Format::Plain => {
            let mut out = String::new();
            for col in &board.columns {
                out.push_str(&format!("{}  {}  ({})\n", col.id, col.title, col.cards.len()));
            }
            chomp(out)
        }
        Format::Json => serde_json::to_string_pretty(&columns_dto(board))?,
        Format::Xml => {
            let mut out = String::from("<columns>\n");
            for col in &board.columns {
                out.push_str(&format!(
                    "  <column id=\"{}\" title=\"{}\" count=\"{}\"/>\n",
                    xml_esc(&col.id),
                    xml_esc(&col.title),
                    col.cards.len(),
                ));
            }
            out.push_str("</columns>");
            out
        }
        Format::Csv => {
            let mut out = String::from("id,title,count\n");
            for col in &board.columns {
                out.push_str(&format!(
                    "{},{},{}\n",
                    csv_esc(&col.id),
                    csv_esc(&col.title),
                    col.cards.len(),
                ));
            }
            chomp(out)
        }
        Format::Table => {
            let headers = ["ID", "TITLE", "COUNT"];
            let rows: Vec<Vec<String>> = board
                .columns
                .iter()
                .map(|col| {
                    vec![
                        col.id.clone(),
                        col.title.clone(),
                        col.cards.len().to_string(),
                    ]
                })
                .collect();
            format_table(&headers, &rows)
        }
        Format::Markdown => {
            let mut out = String::from("| ID | Title | Count |\n| --- | --- | --- |\n");
            for col in &board.columns {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    col.id, col.title, col.cards.len()
                ));
            }
            chomp(out)
        }
    })
}

fn columns_dto(board: &Board) -> ColumnsOut {
    ColumnsOut {
        columns: board
            .columns
            .iter()
            .map(|col| ColumnInfoOut {
                id: col.id.clone(),
                title: col.title.clone(),
                count: col.cards.len(),
            })
            .collect(),
    }
}

// ---- Action result (move, create, edit) ----

pub fn format_result(pairs: &[(&str, &str)], fmt: Format) -> Result<String, serde_json::Error> {
    Ok(match fmt {
        Format::Plain => pairs
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Format::Json => {
            let map: serde_json::Map<String, serde_json::Value> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
                .collect();
            serde_json::to_string_pretty(&serde_json::Value::Object(map))?
        }
        Format::Xml => {
            let mut out = String::from("<result");
            for (k, v) in pairs {
                out.push_str(&format!(" {k}=\"{}\"", xml_esc(v)));
            }
            out.push_str("/>");
            out
        }
        Format::Csv => {
            let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
            let vals: Vec<String> = pairs.iter().map(|(_, v)| csv_esc(v)).collect();
            format!("{}\n{}", keys.join(","), vals.join(","))
        }
        Format::Table => {
            let headers: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
            let row: Vec<Vec<String>> = vec![pairs.iter().map(|(_, v)| v.to_string()).collect()];
            format_table(&headers, &row)
        }
        Format::Markdown => pairs
            .iter()
            .map(|(k, v)| format!("- **{k}**: {v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

// ---- Helpers ----

fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn csv_esc(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn format_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < cols {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut out = String::new();
    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&format!("{:width$}", h, width = widths[i]));
    }
    out.push('\n');

    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        for _ in 0..*w {
            out.push('-');
        }
    }
    out.push('\n');

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i >= cols {
                break;
            }
            if i > 0 {
                out.push_str("  ");
            }
            out.push_str(&format!("{:width$}", cell, width = widths[i]));
        }
        out.push('\n');
    }

    chomp(out)
}

fn chomp(mut s: String) -> String {
    while s.ends_with('\n') {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, Card, Column, Priority};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn sample_board() -> Board {
        Board {
            columns: vec![
                Column {
                    id: "todo".into(),
                    title: "To Do".into(),
                    cards: vec![Card {
                        id: "FLOW-1".into(),
                        title: "First task".into(),
                        description: "Details here".into(),
                        priority: Priority::Medium,
                        assignee: String::new(),
                        project: String::new(),
                    }],
                },
                Column {
                    id: "done".into(),
                    title: "Done".into(),
                    cards: vec![],
                },
            ],
        }
    }

    // ---- format_board ----

    #[test]
    fn board_plain() -> TestResult {
        let out = format_board(&sample_board(), Format::Plain)?;
        assert!(out.contains("== To Do (1) =="));
        assert!(out.contains("FLOW-1"));
        assert!(out.contains("MEDIUM"));
        assert!(out.contains("First task"));
        assert!(out.contains("== Done (0) =="));
        Ok(())
    }

    #[test]
    fn board_json_parses() -> TestResult {
        let out = format_board(&sample_board(), Format::Json)?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert_eq!(v["columns"][0]["cards"][0]["id"], "FLOW-1");
        assert_eq!(v["columns"][0]["cards"][0]["priority"], "MEDIUM");
        assert_eq!(v["columns"][0]["id"], "todo");
        assert_eq!(v["columns"].as_array().ok_or("not an array")?.len(), 2);
        Ok(())
    }

    #[test]
    fn board_csv_has_header_and_rows() -> TestResult {
        let out = format_board(&sample_board(), Format::Csv)?;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "column_id,column_title,card_id,card_title,priority,project");
        assert_eq!(lines[1], "todo,To Do,FLOW-1,First task,MEDIUM,");
        assert_eq!(lines.len(), 2);
        Ok(())
    }

    #[test]
    fn board_xml_structure() -> TestResult {
        let out = format_board(&sample_board(), Format::Xml)?;
        assert!(out.starts_with("<board>"));
        assert!(out.ends_with("</board>"));
        assert!(out.contains("id=\"FLOW-1\""));
        assert!(out.contains("priority=\"MEDIUM\""));
        assert!(out.contains("title=\"To Do\""));
        Ok(())
    }

    #[test]
    fn board_table_has_header_and_separator() -> TestResult {
        let out = format_board(&sample_board(), Format::Table)?;
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].contains("COLUMN"));
        assert!(lines[0].contains("PROJECT"));
        assert!(lines[0].contains("ID"));
        assert!(lines[0].contains("PRIORITY"));
        assert!(lines[0].contains("TITLE"));
        assert!(lines[1].contains("---"));
        assert!(lines[2].contains("FLOW-1"));
        assert!(lines[2].contains("MEDIUM"));
        Ok(())
    }

    #[test]
    fn board_markdown() -> TestResult {
        let out = format_board(&sample_board(), Format::Markdown)?;
        assert!(out.contains("## To Do (1)"));
        assert!(out.contains("- **FLOW-1** [MEDIUM] First task"));
        assert!(out.contains("## Done (0)"));
        Ok(())
    }

    // ---- format_card ----

    fn sample_card() -> Card {
        Card {
            id: "X-1".into(),
            title: "My task".into(),
            description: "Some details".into(),
            priority: Priority::High,
            assignee: String::new(),
            project: String::new(),
        }
    }

    #[test]
    fn card_plain_shows_fields() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Plain)?;
        assert!(out.contains("X-1"));
        assert!(out.contains("My task"));
        assert!(out.contains("priority: HIGH"));
        assert!(out.contains("column: To Do (todo)"));
        assert!(out.contains("Some details"));
        Ok(())
    }

    #[test]
    fn card_plain_omits_empty_description() -> TestResult {
        let card = Card {
            id: "X-2".into(),
            title: "No desc".into(),
            description: "  ".into(),
            priority: Priority::Low,
            assignee: String::new(),
            project: String::new(),
        };
        let out = format_card(&card, "done", "Done", Format::Plain)?;
        assert!(!out.contains("  \n"));
        assert!(out.ends_with("(done)"));
        Ok(())
    }

    #[test]
    fn card_json_parses_all_fields() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Json)?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert_eq!(v["id"], "X-1");
        assert_eq!(v["title"], "My task");
        assert_eq!(v["description"], "Some details");
        assert_eq!(v["priority"], "HIGH");
        assert_eq!(v["column_id"], "todo");
        assert_eq!(v["column_title"], "To Do");
        Ok(())
    }

    #[test]
    fn card_xml_structure() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Xml)?;
        assert!(out.starts_with("<card "));
        assert!(out.ends_with("</card>"));
        assert!(out.contains("<description>"));
        assert!(out.contains("priority=\"HIGH\""));
        Ok(())
    }

    #[test]
    fn card_csv_has_header_and_row() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Csv)?;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "id,title,description,priority,assignee,project,column_id,column_title");
        assert!(lines[1].contains("X-1"));
        assert!(lines[1].contains("HIGH"));
        Ok(())
    }

    #[test]
    fn card_table_shows_fields() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Table)?;
        assert!(out.contains("FIELD"));
        assert!(out.contains("VALUE"));
        assert!(out.contains("My task"));
        assert!(out.contains("HIGH"));
        Ok(())
    }

    #[test]
    fn card_markdown() -> TestResult {
        let out = format_card(&sample_card(), "todo", "To Do", Format::Markdown)?;
        assert!(out.contains("# My task"));
        assert!(out.contains("**X-1**"));
        assert!(out.contains("Priority: HIGH"));
        assert!(out.contains("Some details"));
        Ok(())
    }

    // ---- format_columns ----

    #[test]
    fn columns_plain() -> TestResult {
        let out = format_columns(&sample_board(), Format::Plain)?;
        assert!(out.contains("todo  To Do  (1)"));
        assert!(out.contains("done  Done  (0)"));
        Ok(())
    }

    #[test]
    fn columns_json_parses() -> TestResult {
        let out = format_columns(&sample_board(), Format::Json)?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert_eq!(v["columns"][0]["id"], "todo");
        assert_eq!(v["columns"][0]["count"], 1);
        assert_eq!(v["columns"][1]["count"], 0);
        Ok(())
    }

    #[test]
    fn columns_xml_structure() -> TestResult {
        let out = format_columns(&sample_board(), Format::Xml)?;
        assert!(out.starts_with("<columns>"));
        assert!(out.contains("count=\"1\""));
        Ok(())
    }

    #[test]
    fn columns_csv_has_header() -> TestResult {
        let out = format_columns(&sample_board(), Format::Csv)?;
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "id,title,count");
        assert!(lines[1].starts_with("todo,"));
        Ok(())
    }

    #[test]
    fn columns_table() -> TestResult {
        let out = format_columns(&sample_board(), Format::Table)?;
        assert!(out.contains("ID"));
        assert!(out.contains("TITLE"));
        assert!(out.contains("COUNT"));
        Ok(())
    }

    #[test]
    fn columns_markdown_table() -> TestResult {
        let out = format_columns(&sample_board(), Format::Markdown)?;
        assert!(out.contains("| todo | To Do | 1 |"));
        Ok(())
    }

    // ---- format_result ----

    #[test]
    fn result_plain() -> TestResult {
        let out = format_result(&[("action", "move"), ("card_id", "X-1")], Format::Plain)?;
        assert_eq!(out, "action: move\ncard_id: X-1");
        Ok(())
    }

    #[test]
    fn result_json_parses() -> TestResult {
        let out = format_result(&[("action", "move"), ("card_id", "X-1")], Format::Json)?;
        let v: serde_json::Value = serde_json::from_str(&out)?;
        assert_eq!(v["action"], "move");
        assert_eq!(v["card_id"], "X-1");
        Ok(())
    }

    #[test]
    fn result_xml() -> TestResult {
        let out = format_result(&[("action", "move"), ("card_id", "X-1")], Format::Xml)?;
        assert_eq!(out, "<result action=\"move\" card_id=\"X-1\"/>");
        Ok(())
    }

    #[test]
    fn result_csv() -> TestResult {
        let out = format_result(&[("action", "move"), ("card_id", "X-1")], Format::Csv)?;
        assert_eq!(out, "action,card_id\nmove,X-1");
        Ok(())
    }

    #[test]
    fn result_table() -> TestResult {
        let out = format_result(&[("action", "move")], Format::Table)?;
        assert!(out.contains("action"));
        assert!(out.contains("move"));
        Ok(())
    }

    #[test]
    fn result_markdown() -> TestResult {
        let out = format_result(&[("action", "move"), ("card_id", "X-1")], Format::Markdown)?;
        assert_eq!(out, "- **action**: move\n- **card_id**: X-1");
        Ok(())
    }

    // ---- helpers ----

    #[test]
    fn csv_escapes_commas() {
        assert_eq!(csv_esc("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn csv_escapes_quotes() {
        assert_eq!(csv_esc("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_escapes_newlines() {
        assert_eq!(csv_esc("a\nb"), "\"a\nb\"");
    }

    #[test]
    fn csv_no_escape_needed() {
        assert_eq!(csv_esc("hello"), "hello");
    }

    #[test]
    fn xml_escapes_special_chars() {
        assert_eq!(xml_esc("<a&b>"), "&lt;a&amp;b&gt;");
    }

    #[test]
    fn xml_escapes_quotes() {
        assert_eq!(xml_esc("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn table_aligns_columns() {
        let out = format_table(&["A", "BB"], &[vec!["long value".into(), "x".into()]]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0].find("BB"), lines[2].find("x"));
    }
}
