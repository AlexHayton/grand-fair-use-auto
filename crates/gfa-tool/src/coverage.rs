//! The mission coverage report (`gfa-tool mission coverage`): every command
//! and declaration kind a mission file uses, how often and in which
//! sections, whether the original has it, and whether gfa-sim does it yet.
//! It is the mission VM's work list (M7); `docs/MISSION_OPCODES.md` carries
//! its table for GTA's `MISSION.INI`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use gfa_formats::mission::Section;
use gfa_sim::mission::{DeclKind, Op, Status};

/// Sections numbered from here on are the multiplayer ones (deathmatch and
/// the Canonball Runs) in all three games. Multiplayer is out of scope.
pub const FIRST_MULTIPLAYER: u32 = 1000;

/// One name's uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub name: String,
    pub lines: usize,
    /// Lines in each of [`Report::sections`], in that order.
    pub per_section: Vec<usize>,
    /// The first use: section and line in the file.
    pub first: (u32, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub file: String,
    /// The sections counted, in file order.
    pub sections: Vec<u32>,
    /// The multiplayer sections left out.
    pub left_out: Vec<u32>,
    /// Most used first.
    pub cmds: Vec<Tally>,
    pub decls: Vec<Tally>,
    /// Command lines that do not carry exactly five numbers, which the
    /// original reads for every command whatever the line holds: section,
    /// line, numbers.
    pub odd_lines: Vec<(u32, usize, usize)>,
}

impl Report {
    /// Counts `sections` of `file`, leaving the multiplayer ones out unless
    /// `multiplayer`.
    pub fn new(file: &str, sections: &[Section], multiplayer: bool) -> Report {
        let (counted, left_out): (Vec<&Section>, Vec<&Section>) = sections
            .iter()
            .partition(|s| multiplayer || s.id < FIRST_MULTIPLAYER);
        let n = counted.len();
        let mut cmds: BTreeMap<String, Tally> = BTreeMap::new();
        let mut decls: BTreeMap<String, Tally> = BTreeMap::new();
        let mut odd_lines = Vec::new();
        let count = |map: &mut BTreeMap<String, Tally>, name: &str, k: usize, at: (u32, usize)| {
            let t = map.entry(name.to_string()).or_insert_with(|| Tally {
                name: name.to_string(),
                lines: 0,
                per_section: vec![0; n],
                first: at,
            });
            t.lines += 1;
            t.per_section[k] += 1;
        };
        for (k, s) in counted.iter().enumerate() {
            for d in &s.decls {
                count(&mut decls, &d.kind, k, (s.id, d.source_line));
            }
            for c in &s.cmds {
                count(&mut cmds, &c.name, k, (s.id, c.source_line));
                if c.nums.len() != 5 {
                    odd_lines.push((s.id, c.source_line, c.nums.len()));
                }
            }
        }
        let sorted = |map: BTreeMap<String, Tally>| {
            let mut v: Vec<Tally> = map.into_values().collect();
            v.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.name.cmp(&b.name)));
            v
        };
        Report {
            file: file.to_string(),
            sections: counted.iter().map(|s| s.id).collect(),
            left_out: left_out.iter().map(|s| s.id).collect(),
            cmds: sorted(cmds),
            decls: sorted(decls),
            odd_lines,
        }
    }

    /// Names the original's tables do not have: what, name, section, line.
    pub fn unknown(&self) -> Vec<(&'static str, &str, u32, usize)> {
        let cmds = self
            .cmds
            .iter()
            .filter(|t| Op::from_name(&t.name).is_none())
            .map(|t| ("command", t.name.as_str(), t.first.0, t.first.1));
        let decls = self
            .decls
            .iter()
            .filter(|t| DeclKind::from_name(&t.name).is_none())
            .map(|t| ("declaration", t.name.as_str(), t.first.0, t.first.1));
        cmds.chain(decls).collect()
    }

    /// Of the commands: (names done, names, lines done, lines).
    pub fn cmd_coverage(&self) -> (usize, usize, usize, usize) {
        coverage(&self.cmds, |t| {
            Op::from_name(&t.name).is_some_and(Op::implemented)
        })
    }

    /// Of the commands done for some of their uses: (names, lines).
    pub fn cmd_partial(&self) -> (usize, usize) {
        let (n, _, l, _) = coverage(&self.cmds, |t| {
            Op::from_name(&t.name).is_some_and(|op| op.status() == Status::Part)
        });
        (n, l)
    }

    /// Of the declarations: (kinds read, kinds, lines read, lines).
    pub fn decl_coverage(&self) -> (usize, usize, usize, usize) {
        coverage(&self.decls, |t| {
            DeclKind::from_name(&t.name).is_some_and(DeclKind::read)
        })
    }

    /// The report for a terminal.
    pub fn text(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "{}", self.file);
        let _ = writeln!(o, "  sections {}", ids(&self.sections));
        if !self.left_out.is_empty() {
            let _ = writeln!(
                o,
                "  left out, multiplayer (--multiplayer counts them): {}",
                ids(&self.left_out)
            );
        }
        o.push('\n');
        o.push_str(&self.summary());
        o.push('\n');
        let unknown = self.unknown();
        if unknown.is_empty() {
            let _ = writeln!(o, "Every name is in GTA8.EXE's tables.");
        } else {
            let _ = writeln!(
                o,
                "Not in GTA8.EXE's tables (GTA8.EXE stops on these; London's executables have their own tables):"
            );
            for (what, name, sec, line) in unknown {
                let _ = writeln!(o, "  {what} {name}: first [{sec}] line {line}");
            }
        }
        if self.odd_lines.is_empty() {
            let _ = writeln!(o, "Every command line carries five numbers.");
        } else {
            let _ = writeln!(o, "Command lines without exactly five numbers:");
            for (sec, line, n) in &self.odd_lines {
                let _ = writeln!(o, "  [{sec}] line {line}: {n}");
            }
        }
        o.push('\n');
        self.text_table(&mut o, "command", &self.cmds, cmd_mark);
        o.push('\n');
        self.text_table(&mut o, "declaration", &self.decls, decl_mark);
        o
    }

    fn summary(&self) -> String {
        let (cd, cn, cld, cl) = self.cmd_coverage();
        let (cp, clp) = self.cmd_partial();
        let (dd, dn, dld, dl) = self.decl_coverage();
        format!(
            "Commands: {cn} of the original's {} in use, on {cl} lines. gfa-sim runs {cd} of them, \
             {cld} of the {cl} lines ({}), and {cp} more in part ({clp} lines).\n\
             Declarations: {dn} of the original's {} kinds in use, on {dl} lines. gfa-sim reads {dd} \
             of them, {dld} of the {dl} lines ({}).\n",
            Op::ALL.len(),
            percent(cld, cl),
            DeclKind::ALL.len(),
            percent(dld, dl),
        )
    }

    fn text_table(
        &self,
        o: &mut String,
        what: &str,
        rows: &[Tally],
        look: impl Fn(&str) -> Option<(u8, &'static str)>,
    ) {
        let width = rows
            .iter()
            .map(|t| t.name.len())
            .max()
            .unwrap_or(0)
            .max(what.len());
        let cols: Vec<String> = self.sections.iter().map(|s| format!("[{s}]")).collect();
        let _ = write!(o, " op  {what:<width$}  lines");
        for c in &cols {
            let _ = write!(o, " {c:>6}");
        }
        let _ = writeln!(o, "  gfa-sim  first use");
        for t in rows {
            let (op, done) = match look(&t.name) {
                Some((code, mark)) => (code.to_string(), if mark.is_empty() { "-" } else { mark }),
                None => ("?".to_string(), "unknown"),
            };
            let _ = write!(o, "{op:>3}  {:<width$}  {:>5}", t.name, t.lines);
            for n in &t.per_section {
                let _ = write!(o, " {n:>6}");
            }
            let _ = writeln!(o, "  {done:<7}  [{}] line {}", t.first.0, t.first.1);
        }
    }

    /// The tables of `docs/MISSION_OPCODES.md`, between its coverage markers.
    pub fn markdown(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(o, "Sections {}.", ids(&self.sections));
        o.push('\n');
        for line in self.summary().lines() {
            let _ = writeln!(o, "{line}");
            o.push('\n');
        }
        self.markdown_table(&mut o, "Command", &self.cmds, cmd_mark);
        let unused: Vec<&str> = Op::ALL
            .iter()
            .map(|op| op.name())
            .filter(|n| !self.cmds.iter().any(|t| t.name == *n))
            .collect();
        let _ = writeln!(
            o,
            "\nIn the original's table but used by none of these sections: {}.\n",
            tick_list(&unused)
        );
        self.markdown_table(&mut o, "Declaration", &self.decls, decl_mark);
        let unused: Vec<&str> = DeclKind::ALL
            .iter()
            .map(|k| k.name())
            .filter(|n| !self.decls.iter().any(|t| t.name == *n))
            .collect();
        let _ = writeln!(
            o,
            "\nIn the original's table but used by none of these sections: {}.",
            tick_list(&unused)
        );
        o
    }

    /// Columns padded as prettier pads them, so formatting the doc changes
    /// nothing.
    fn markdown_table(
        &self,
        o: &mut String,
        what: &str,
        rows: &[Tally],
        look: impl Fn(&str) -> Option<(u8, &'static str)>,
    ) {
        let mut head = vec!["No.".to_string(), what.to_string(), "Lines".to_string()];
        head.extend(self.sections.iter().map(|s| format!("[{s}]")));
        head.push("gfa-sim".to_string());
        let right: Vec<bool> = (0..head.len())
            .map(|c| c != 1 && c != head.len() - 1)
            .collect();
        let mut table = vec![head];
        for t in rows {
            let (op, done) = match look(&t.name) {
                Some((code, mark)) => (code.to_string(), mark),
                None => ("?".to_string(), "unknown"),
            };
            let mut row = vec![op, format!("`{}`", t.name), t.lines.to_string()];
            row.extend(t.per_section.iter().map(usize::to_string));
            row.push(done.to_string());
            table.push(row);
        }
        let widths: Vec<usize> = (0..right.len())
            .map(|c| table.iter().map(|r| r[c].len()).max().unwrap_or(0).max(3))
            .collect();
        let line = |o: &mut String, cells: &[String]| {
            o.push('|');
            for (c, cell) in cells.iter().enumerate() {
                let w = widths[c];
                let _ = if right[c] {
                    write!(o, " {cell:>w$} |")
                } else {
                    write!(o, " {cell:<w$} |")
                };
            }
            o.push('\n');
        };
        line(o, &table[0]);
        let rule: Vec<String> = widths
            .iter()
            .zip(&right)
            .map(|(&w, &r)| {
                if r {
                    format!("{}:", "-".repeat(w - 1))
                } else {
                    "-".repeat(w)
                }
            })
            .collect();
        line(o, &rule);
        for row in &table[1..] {
            line(o, row);
        }
    }
}

/// Tables compared cell by cell and prose line by line, so the doc may be
/// reformatted. Used by the data test that keeps `docs/MISSION_OPCODES.md`
/// current.
pub fn normalize_markdown(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            if !l.starts_with('|') {
                return l.split_whitespace().collect::<Vec<_>>().join(" ");
            }
            let cells: Vec<&str> = l.trim_matches('|').split('|').map(str::trim).collect();
            if cells
                .iter()
                .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
            {
                let marks: Vec<&str> = cells
                    .iter()
                    .map(|c| if c.ends_with(':') { "-:" } else { "-" })
                    .collect();
                return format!("|{}|", marks.join("|"));
            }
            format!("|{}|", cells.join("|"))
        })
        .collect()
}

/// A command's number and its gfa-sim column: `yes`, `part` or nothing.
fn cmd_mark(name: &str) -> Option<(u8, &'static str)> {
    Op::from_name(name).map(|op| {
        let mark = match op.status() {
            Status::Done => "yes",
            Status::Part => "part",
            Status::Missing => "",
        };
        (op.code(), mark)
    })
}

/// A declaration kind's number and its gfa-sim column.
fn decl_mark(name: &str) -> Option<(u8, &'static str)> {
    DeclKind::from_name(name).map(|k| (k.code(), if k.read() { "yes" } else { "" }))
}

fn coverage(rows: &[Tally], done: impl Fn(&Tally) -> bool) -> (usize, usize, usize, usize) {
    let finished: Vec<&Tally> = rows.iter().filter(|t| done(t)).collect();
    (
        finished.len(),
        rows.len(),
        finished.iter().map(|t| t.lines).sum(),
        rows.iter().map(|t| t.lines).sum(),
    )
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "none to do".to_string();
    }
    // Tenths, rounded down, so nothing short of all of it shows as 100%.
    let tenths = part * 1000 / whole;
    format!("{}.{}%", tenths / 10, tenths % 10)
}

fn ids(v: &[u32]) -> String {
    v.iter()
        .map(|s| format!("[{s}]"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each name once, in the order given, in backticks.
fn tick_list(v: &[&str]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for n in v {
        if !seen.contains(n) {
            seen.push(n);
        }
    }
    if seen.is_empty() {
        return "none".to_string();
    }
    seen.iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[1]\nT, 22, nyc.cmp, 0,\n1 1\n\
        0 1 (1,2,3) PLAYER 0 0\n5 (1,2,3) PARKED 0 0\n6 (1,2,3) PARKED 0 0\n-1\n\
        1000 DONOWT 0 0 0 0 0\n1010 BRIEF 1 0 0 0 0\n1020 BRIEF 1 0 0 0\n1030 NOT_A_COMMAND 0 0 0 0 0\n-1\n\
        [1001]\nDEATHMATCH1, 22, nyc.cmp, 0,\n1 1\n-1\n1 BRIEF 0 0 0 0 0\n-1\n";

    fn report(multiplayer: bool) -> Report {
        let s = gfa_formats::mission::parse(SAMPLE.as_bytes()).unwrap();
        Report::new("MISSION.INI", &s, multiplayer)
    }

    #[test]
    fn counts_uses_by_section_and_finds_the_odd_ones() {
        let r = report(false);
        assert_eq!(
            (r.sections.clone(), r.left_out.clone()),
            (vec![1], vec![1001])
        );
        let brief = &r.cmds[0];
        assert_eq!(
            (brief.name.as_str(), brief.lines, brief.first),
            ("BRIEF", 2, (1, 9))
        );
        assert_eq!(r.decls[0].name, "PARKED");
        assert_eq!(r.unknown(), vec![("command", "NOT_A_COMMAND", 1, 11)]);
        assert_eq!(r.odd_lines, vec![(1, 10, 4)]);
        assert_eq!(r.cmd_coverage(), (1, 3, 1, 4), "DONOWT is done");
        assert_eq!(r.cmd_partial(), (0, 0));
        assert_eq!(r.decl_coverage(), (1, 2, 1, 3), "PLAYER is read");
        assert_eq!(report(true).cmds[0].per_section, vec![2, 1]);
    }

    #[test]
    fn tables_survive_reformatting() {
        let md = report(false).markdown();
        assert!(
            md.contains("| No. | Command         | Lines | [1] | gfa-sim |"),
            "{md}"
        );
        assert!(
            md.contains("| --: | --------------- | ----: | --: | ------- |"),
            "{md}"
        );
        assert!(
            md.contains("|   ? | `NOT_A_COMMAND` |     1 |   1 | unknown |"),
            "{md}"
        );
        let squashed = md
            .replace("  ", " ")
            .replace(" |", "|")
            .replace("--:", "-:");
        assert_eq!(normalize_markdown(&md), normalize_markdown(&squashed));
        assert_ne!(
            normalize_markdown(&md),
            normalize_markdown(&md.replace("|     2 |   2 |", "|     2 |   3 |"))
        );
        assert_eq!(md.matches("`XXXX`").count(), 1, "XXXX listed once");
    }

    #[test]
    fn percentages_round_down() {
        assert_eq!(percent(0, 4), "0.0%");
        assert_eq!(percent(1, 3), "33.3%");
        assert_eq!(percent(1999, 2000), "99.9%");
        assert_eq!(percent(2, 2), "100.0%");
    }
}
