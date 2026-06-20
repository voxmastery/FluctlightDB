//! Human-readable table output for REPL and CLI.

use std::io::{self, IsTerminal, Write};

pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    widths: Vec<usize>,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        let headers: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
        let widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        Self {
            headers,
            rows: Vec::new(),
            widths,
        }
    }

    pub fn push(&mut self, row: Vec<String>) {
        for (i, cell) in row.iter().enumerate() {
            if i < self.widths.len() {
                self.widths[i] = self.widths[i].max(cell.len());
            }
        }
        self.rows.push(row);
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn render(&self, w: &mut dyn Write) -> io::Result<()> {
        if self.headers.is_empty() {
            return Ok(());
        }
        let sep: String = self
            .widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ");
        write_row(w, &self.headers, &self.widths)?;
        writeln!(w, "{sep}")?;
        for row in &self.rows {
            write_row(w, row, &self.widths)?;
        }
        Ok(())
    }

    pub fn print(&self) {
        let _ = self.render(&mut io::stdout());
    }
}

fn write_row(w: &mut dyn Write, cells: &[String], widths: &[usize]) -> io::Result<()> {
    for (i, cell) in cells.iter().enumerate() {
        if i > 0 {
            write!(w, "  ")?;
        }
        let width = widths.get(i).copied().unwrap_or(cell.len());
        write!(w, "{cell:width$}")?;
    }
    writeln!(w)
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub fn short_id(id: &uuid::Uuid) -> String {
    let s = id.to_string();
    s.chars().take(8).collect()
}

pub fn yes_no(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}

pub fn is_tty() -> bool {
    io::stdout().is_terminal()
}

pub fn print_footer(rows: usize, elapsed_ms: f64) {
    println!("({rows} rows, {elapsed_ms:.1}ms)");
}

pub fn print_json<T: serde::Serialize + ?Sized>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
    );
}
