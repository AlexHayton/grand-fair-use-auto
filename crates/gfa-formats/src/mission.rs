//! `MISSION.INI` (and London's `MISSUK.INI`, `missuke.ini`, `misse8.ini`):
//! the level scripts.
//!
//! Sources: AmiGTA1 `native/gta_script.c` and `gta_script_run.c` (MIT) for
//! the layout. Implementation ours. The parser keeps names as written and
//! every number on a line; giving them meaning is the mission VM's job (M7).
//!
//! A section looks like:
//!
//! ```text
//! [1]
//! Easy Liberty City NEW, 22, nyc.cmp, 0,      title line, comma-separated
//! 100 1 1 1 1 0                               settings line
//! 0 1 (108,108,4) TELEPHONE 0 768             declarations: id [flag] (x,y,z) NAME numbers…
//! -1                                          end of the declarations
//! 1000 DONOWT 0 0 0 0 0                       commands: id NAME numbers…
//! -1                                          end of the commands
//! ```
//!
//! Quirks handled:
//! - a section header can follow `-1` on the same line (`-1[1003]`);
//! - the flag can touch the bracket (`9000 1(122,155,1) POWERUP 13 0`);
//! - declarations can carry five numbers. AmiGTA1 reads four, which loses
//!   SPECIFIC_DOOR's remap; we keep all of them;
//! - a full stop typed for a comma in coordinates (`(57,124.4)`, once);
//! - the file ends with a DOS end-of-file byte (`0x1a`).

use crate::reader::FormatError;

const WHAT: &str = "MISSION.INI";

/// A thing that exists in the world: `id [flag] (x,y,z) NAME numbers…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    /// The line's label, which other lines use to refer to it.
    pub id: i32,
    /// 1 = survives the script's RESET; 2 = also a kill-frenzy car.
    pub flag: Option<i32>,
    /// As written: blocks or pixels depending on the kind (AmiGTA1's
    /// `gta_script_is_block`).
    pub pos: [i32; 3],
    pub kind: String,
    pub nums: Vec<i32>,
    /// 1-based line in the file.
    pub source_line: usize,
}

/// A step of the logic: `id NAME numbers…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub id: i32,
    pub name: String,
    pub nums: Vec<i32>,
    pub source_line: usize,
}

impl Cmd {
    /// Parameter `n` (1-based, as `p1`…`p5` in AmiGTA1), 0 when absent.
    pub fn p(&self, n: usize) -> i32 {
        n.checked_sub(1)
            .and_then(|i| self.nums.get(i))
            .copied()
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub id: u32,
    /// The title line's fields: name, a number, the map file, a number.
    pub title: Vec<String>,
    /// The settings line's numbers.
    pub settings: Vec<i32>,
    pub decls: Vec<Decl>,
    pub cmds: Vec<Cmd>,
}

impl Section {
    pub fn name(&self) -> &str {
        self.title.first().map_or("", String::as_str)
    }

    /// The `.CMP` the section plays on, relative to `GTADATA/`. London writes
    /// it with a backslash (`uk\uk.cmp`); this returns forward slashes.
    pub fn map(&self) -> Option<String> {
        self.title
            .iter()
            .find(|f| f.to_ascii_lowercase().ends_with(".cmp"))
            .map(|f| f.replace('\\', "/"))
    }

    pub fn decl(&self, id: i32) -> Option<&Decl> {
        self.decls.iter().find(|d| d.id == id)
    }

    pub fn cmd_index(&self, id: i32) -> Option<usize> {
        self.cmds.iter().position(|c| c.id == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Title,
    Settings,
    Decls,
    Cmds,
    Done,
}

/// Parses a whole mission file into its sections, in file order.
pub fn parse(data: &[u8]) -> Result<Vec<Section>, FormatError> {
    let mut sections: Vec<Section> = Vec::new();
    let mut stage = Stage::Done;
    for (n, raw) in data.split(|&b| b == b'\n').enumerate() {
        let line_no = n + 1;
        let text: String = raw.iter().map(|&b| b as char).collect();
        // A header glued to the end of a line splits it in two.
        let (before, header) = match text.find('[') {
            Some(0) | None => (text.as_str(), None),
            Some(i) => (&text[..i], Some(&text[i..])),
        };
        for part in std::iter::once(before).chain(header) {
            let line = part.trim_matches(|c: char| c.is_whitespace() || c == '\x1a');
            if line.is_empty() {
                continue;
            }
            let bad =
                |what: &str| FormatError::invalid(WHAT, format!("line {line_no}: {what}: {line}"));
            if let Some(rest) = line.strip_prefix('[') {
                let id = rest
                    .split(']')
                    .next()
                    .and_then(|s| s.trim().parse().ok())
                    .ok_or_else(|| bad("bad section header"))?;
                sections.push(Section {
                    id,
                    title: Vec::new(),
                    settings: Vec::new(),
                    decls: Vec::new(),
                    cmds: Vec::new(),
                });
                stage = Stage::Title;
                continue;
            }
            let Some(sec) = sections.last_mut() else {
                return Err(bad("text before the first section"));
            };
            match stage {
                Stage::Title => {
                    sec.title = line
                        .split(',')
                        .map(|f| f.trim().to_string())
                        .filter(|f| !f.is_empty())
                        .collect();
                    stage = Stage::Settings;
                }
                Stage::Settings => {
                    sec.settings = numbers(line).ok_or_else(|| bad("bad settings line"))?;
                    stage = Stage::Decls;
                }
                Stage::Decls | Stage::Cmds if line.starts_with('-') => {
                    stage = if stage == Stage::Decls {
                        Stage::Cmds
                    } else {
                        Stage::Done
                    };
                }
                Stage::Decls => {
                    let d = parse_decl(line, line_no).ok_or_else(|| bad("bad declaration"))?;
                    sec.decls.push(d);
                }
                Stage::Cmds => {
                    let c = parse_cmd(line, line_no).ok_or_else(|| bad("bad command"))?;
                    sec.cmds.push(c);
                }
                Stage::Done => return Err(bad("text after the end of a section")),
            }
        }
    }
    if stage != Stage::Done
        && let Some(sec) = sections.last()
    {
        return Err(FormatError::invalid(
            WHAT,
            format!("the file ends inside section [{}]", sec.id),
        ));
    }
    Ok(sections)
}

fn numbers(s: &str) -> Option<Vec<i32>> {
    s.split_whitespace().map(|w| w.parse().ok()).collect()
}

/// `id [flag] (x,y,z) NAME numbers…`
fn parse_decl(line: &str, source_line: usize) -> Option<Decl> {
    let open = line.find('(')?;
    let close = open + line[open..].find(')')?;
    let mut head = line[..open].split_whitespace();
    let id = head.next()?.parse().ok()?;
    let flag = head.next().map(str::parse).transpose().ok()?;
    if head.next().is_some() {
        return None;
    }
    // Coordinates are integers; one line in MISSION.INI types a full stop for
    // the last comma (`(57,124.4)`), so both separate.
    let mut coords = line[open + 1..close]
        .split([',', '.'])
        .map(|v| v.trim().parse().ok());
    let pos = [coords.next()??, coords.next()??, coords.next()??];
    if coords.next().is_some() {
        return None;
    }
    let mut tail = line[close + 1..].split_whitespace();
    let kind = tail.next()?.to_string();
    if !kind.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let nums = tail.map(|w| w.parse().ok()).collect::<Option<_>>()?;
    Some(Decl {
        id,
        flag,
        pos,
        kind,
        nums,
        source_line,
    })
}

/// `id NAME numbers…`
fn parse_cmd(line: &str, source_line: usize) -> Option<Cmd> {
    let mut words = line.split_whitespace();
    let id = words.next()?.parse().ok()?;
    let name = words.next()?.to_string();
    if !name.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    let nums = words.map(|w| w.parse().ok()).collect::<Option<_>>()?;
    Some(Cmd {
        id,
        name,
        nums,
        source_line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[1] \r\n\r\nEasy Liberty City NEW, 22, nyc.cmp, 0, \r\n100 1 1 1 1 0 \r\n\r\n\
        0 1 (108,108,4) TELEPHONE 0 768 \r\n\
        117 1 (65,155,3) SPECIFIC_DOOR 2 4 0 116 -1 \r\n\
        9000 1(122,155,1) POWERUP 13 0\r\n\
        14 (57,124.4) MIDPOINT_MULTI 23 2\r\n\
        5 (10,20,4) CAR 3 0\r\n\
        -1 \r\n\
        1000 DONOWT 0 0 0 0 0 \r\n\
        1010 ARROW 117 1020 -1 0 50\r\n\
        -1[2]\r\nMedium, 22, nyc.cmp, 0,\r\n100 1 1 1 1 0\r\n-1\r\n-1\r\n\x1a";

    #[test]
    fn parses_sections_declarations_and_commands() {
        let s = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(s.len(), 2, "the glued [2] header is found");
        let one = &s[0];
        assert_eq!(one.id, 1);
        assert_eq!(one.name(), "Easy Liberty City NEW");
        assert_eq!(one.map().as_deref(), Some("nyc.cmp"));
        assert_eq!(one.settings, vec![100, 1, 1, 1, 1, 0]);
        assert_eq!(one.decls.len(), 5);
        assert_eq!(one.decl(14).unwrap().pos, [57, 124, 4]);
        let door = one.decl(117).unwrap();
        assert_eq!(door.kind, "SPECIFIC_DOOR");
        assert_eq!(door.nums, vec![2, 4, 0, 116, -1], "all five numbers kept");
        assert_eq!(one.decl(9000).unwrap().flag, Some(1));
        assert_eq!(one.decl(5).unwrap().flag, None);
        assert_eq!(one.cmds.len(), 2);
        let arrow = &one.cmds[one.cmd_index(1010).unwrap()];
        assert_eq!(
            (arrow.p(1), arrow.p(3), arrow.p(5), arrow.p(6)),
            (117, -1, 50, 0)
        );
        assert_eq!(s[1].id, 2);
        assert!(s[1].decls.is_empty() && s[1].cmds.is_empty());
    }

    #[test]
    fn rejects_broken_files() {
        assert!(
            parse(b"[1]\nT, 1\n1 2\n0 1 (1,2) CAR 0\n-1\n-1\n").is_err(),
            "two coordinates"
        );
        assert!(
            parse(b"[1]\nT, 1\n1 2\n-1\n").is_err(),
            "no end of commands"
        );
        assert!(parse(b"junk\n").is_err());
    }
}
