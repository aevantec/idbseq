//! Human-readable rendering of response data.

use serde_json::Value;

/// One value for display: `-` for null/missing, `yes`/`no` for booleans,
/// comma-joined arrays (`*` when empty, meaning "all").
#[must_use]
pub fn show(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "-".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => if *flag { "yes" } else { "no" }.to_owned(),
        Some(Value::Array(items)) if items.is_empty() => "*".to_owned(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| show(Some(item)))
            .collect::<Vec<_>>()
            .join(","),
        Some(other) => other.to_string(),
    }
}

/// Field of an object for display.
#[must_use]
pub fn field(object: &Value, name: &str) -> String {
    show(object.get(name))
}

/// Aligned `label: value` lines.
#[must_use]
pub fn pairs(rows: &[(&str, String)]) -> String {
    let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0) + 1;
    rows.iter()
        .map(|(label, value)| format!("{:<width$} {value}", format!("{label}:")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A MySQL-style boxed table with a header row; each item yields one cell per column.
/// Columns whose cells are all integers (or `-`) are right-aligned.
#[must_use]
pub fn table(headers: &[&str], items: &[Value], cells: impl Fn(&Value) -> Vec<String>) -> String {
    let rows: Vec<Vec<String>> = items.iter().map(cells).collect();
    let columns: Vec<Column> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            Column::measure(header, rows.iter().filter_map(|row| row.get(index)))
        })
        .collect();
    let border = columns.iter().fold(String::from("+"), |line, column| {
        line + &"-".repeat(column.width + 2) + "+"
    });
    let header = render_row(&columns, headers.iter().map(|h| (*h).to_owned()), false);
    let body = rows
        .iter()
        .map(|row| render_row(&columns, row.iter().cloned(), true));
    [border.clone(), header, border.clone()]
        .into_iter()
        .chain(body)
        .chain(std::iter::once(border))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Column {
    width: usize,
    is_numeric: bool,
}

impl Column {
    fn measure<'a>(header: &str, cells: impl Iterator<Item = &'a String> + Clone) -> Self {
        let width = cells
            .clone()
            .map(|c| c.chars().count())
            .fold(header.chars().count(), usize::max);
        let is_number = |c: &String| c.parse::<i128>().is_ok();
        let is_numeric =
            cells.clone().any(is_number) && cells.into_iter().all(|c| c == "-" || is_number(c));
        Self { width, is_numeric }
    }
}

fn render_row(columns: &[Column], cells: impl Iterator<Item = String>, is_body: bool) -> String {
    cells
        .zip(columns)
        .fold(String::from("|"), |line, (cell, column)| {
            let pad = " ".repeat(column.width - cell.chars().count());
            if is_body && column.is_numeric {
                format!("{line} {pad}{cell} |")
            } else {
                format!("{line} {cell}{pad} |")
            }
        })
}

/// `strict` or `block (size N)`.
#[must_use]
pub fn durability(value: Option<&Value>) -> String {
    match value.and_then(|d| d.get("mode")).and_then(Value::as_str) {
        Some("block") => format!("block (size {})", show(value.and_then(|d| d.get("size")))),
        Some(mode) => mode.to_owned(),
        None => "-".to_owned(),
    }
}

/// Footer for a page of results: count, total and the next cursor.
#[must_use]
pub fn page_footer(shown: usize, meta: &Value, noun: &str) -> String {
    let total = meta
        .get("total")
        .map_or_else(|| shown.to_string(), |t| show(Some(t)));
    let base = format!("{shown} of {total} {noun}");
    match meta.get("next_cursor").and_then(Value::as_str) {
        Some(cursor) => format!("{base}; next page: --cursor {cursor}"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn shows_values_for_humans() {
        assert_eq!(show(None), "-");
        assert_eq!(show(Some(&Value::Null)), "-");
        assert_eq!(show(Some(&json!("a"))), "a");
        assert_eq!(show(Some(&json!(true))), "yes");
        assert_eq!(show(Some(&json!(false))), "no");
        assert_eq!(
            show(Some(&json!(9_223_372_036_854_775_807_i64))),
            "9223372036854775807"
        );
        assert_eq!(show(Some(&json!([]))), "*");
        assert_eq!(show(Some(&json!(["read", "generate"]))), "read,generate");
    }

    #[test]
    fn aligns_pairs() {
        let text = pairs(&[("name", "orders".into()), ("last_id", "1".into())]);
        assert_eq!(text, "name:    orders\nlast_id: 1");
    }

    #[test]
    fn aligns_tables() {
        let items = [
            json!({ "n": "orders", "v": 1 }),
            json!({ "n": "a", "v": null }),
        ];

        let text = table(&["NAME", "LAST_ID"], &items, |item| {
            vec![field(item, "n"), field(item, "v")]
        });

        assert_eq!(
            text,
            "+--------+---------+\n\
             | NAME   | LAST_ID |\n\
             +--------+---------+\n\
             | orders |       1 |\n\
             | a      |       - |\n\
             +--------+---------+"
        );
    }

    #[test]
    fn tables_left_align_text_and_count_characters_not_bytes() {
        let items = [
            json!({ "n": "café", "v": "x" }),
            json!({ "n": "ab", "v": "12" }),
        ];

        let text = table(&["N", "V"], &items, |item| {
            vec![field(item, "n"), field(item, "v")]
        });

        assert_eq!(
            text,
            "+------+----+\n| N    | V  |\n+------+----+\n| café | x  |\n| ab   | 12 |\n+------+----+"
        );
    }

    #[test]
    fn empty_tables_keep_their_header() {
        let text = table(&["NAME"], &[], |item| vec![field(item, "n")]);

        assert_eq!(text, "+------+\n| NAME |\n+------+\n+------+");
    }

    #[test]
    fn renders_durability() {
        assert_eq!(durability(Some(&json!({ "mode": "strict" }))), "strict");
        assert_eq!(
            durability(Some(&json!({ "mode": "block", "size": 100 }))),
            "block (size 100)"
        );
        assert_eq!(durability(None), "-");
    }

    #[test]
    fn renders_page_footers() {
        assert_eq!(
            page_footer(2, &json!({ "total": 5, "next_cursor": "b" }), "keys"),
            "2 of 5 keys; next page: --cursor b"
        );
        assert_eq!(page_footer(2, &Value::Null, "keys"), "2 of 2 keys");
    }
}
