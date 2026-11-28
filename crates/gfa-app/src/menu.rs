//! The base game's title, player slots and chapter picker.
mod art;
use art::{Art, Canvas};
use egui::{Align, Key};
use gfa_assets::{AssetSource, Dataset, level};
use gfa_audio::MenuSfx;
use gfa_formats::{career::Career, fxt::Fxt};

const NAMES: [&str; 8] = [
    "Ulrika", "Travis", "Katie", "Mikki", "Divine", "Bubba", "Troy", "Kivlov",
];
const SECTIONS: [u32; 6] = [1, 2, 102, 103, 202, 203];

#[derive(PartialEq)]
enum Page {
    Title,
    Player,
    Chapter,
    Options,
    Rename,
    Reset,
}

pub struct Menu {
    pub source: Box<dyn AssetSource>,
    page: Page,
    pub player: usize,
    career: Option<Career>,
    chapters: Vec<(u32, String)>,
    chapter: usize,
    art: Art,
    title_row: usize,
    option_row: usize,
    settings: [u8; 4],
    draft: [u8; 4],
    names: [String; 8],
    scores: [[Option<i32>; 6]; 8],
    edit_name: String,
    reset_row: usize,
    sounds: Vec<MenuSfx>,
    pub quit: bool,
    error: String,
}

impl Menu {
    pub fn new(source: Box<dyn AssetSource>) -> Self {
        let career = source
            .read("GTADATA/PLAYER_A.DAT")
            .ok()
            .and_then(|b| Career::parse(&b).ok());
        let art = Art::load(source.as_ref());
        let english = source
            .read("GTADATA/ENGLISH.FXT")
            .ok()
            .and_then(|b| Fxt::parse(&b).ok());
        let settings = career.as_ref().map_or([7, 6, 2, 0], |c| {
            [
                c.sound_volume().min(7),
                c.music_volume().min(7),
                c.text_speed().clamp(1, 3),
                c.bytes[3].min(1),
            ]
        });
        let player = career.as_ref().map_or(1, Career::current_slot);
        let mut error = String::new();
        let mut chapters = Vec::new();
        match level::sections(source.as_ref(), Dataset::Gta) {
            Ok(sections) => {
                for id in SECTIONS {
                    if let Some(s) = sections.iter().find(|s| s.id == id && s.map().is_some()) {
                        chapters.push((
                            id,
                            english
                                .as_ref()
                                .and_then(|f| f.string(&format!("mission{id}")))
                                .unwrap_or_else(|| s.name().to_owned()),
                        ));
                    }
                }
                if chapters.is_empty() {
                    error = "No single-player chapters found".into();
                }
            }
            Err(e) => error = e.to_string(),
        }
        let names = std::array::from_fn(|i| {
            career
                .as_ref()
                .map(|c| c.name(i))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| NAMES[i].into())
        });
        let scores = std::array::from_fn(|p| {
            std::array::from_fn(|i| career.as_ref().and_then(|c| c.best(p, i)))
        });
        Self {
            names,
            scores,
            edit_name: String::new(),
            reset_row: 0,
            sounds: Vec::new(),
            source,
            page: Page::Title,
            career,
            player,
            chapters,
            chapter: 0,
            art,
            title_row: 0,
            option_row: 0,
            settings,
            draft: settings,
            quit: false,
            error,
        }
    }

    pub fn name(&self) -> String {
        self.names[self.player].clone()
    }

    pub fn sound_volume(&self) -> u8 {
        if self.page == Page::Options {
            self.draft[0]
        } else {
            self.settings[0]
        }
    }

    pub fn music_volume(&self) -> u8 {
        if self.page == Page::Options {
            self.draft[1]
        } else {
            self.settings[1]
        }
    }

    pub fn music_mode(&self) -> u8 {
        self.settings[3]
    }

    pub fn text_speed(&self) -> u8 {
        self.settings[2]
    }

    pub fn drain_sounds(&mut self) -> Vec<MenuSfx> {
        std::mem::take(&mut self.sounds)
    }

    pub fn draw(&mut self, ui: &mut egui::Ui, status: &str) -> Option<u32> {
        use Align::{Center, Max, Min};
        let name = self.name();
        let mut c = Canvas::new(ui, &mut self.art, self.source.as_ref());
        let (frame, enter, esc, dx, dy) = ui.input(|i| {
            (
                (i.time * 9.1) as usize,
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i32::from(i.key_pressed(Key::ArrowRight))
                    - i32::from(i.key_pressed(Key::ArrowLeft)),
                i32::from(i.key_pressed(Key::ArrowDown)) - i32::from(i.key_pressed(Key::ArrowUp)),
            )
        });
        c.picture(&format!("F_LOGO{}", frame % 8), 0., 0.);
        c.picture(
            if self.page == Page::Chapter {
                "F_LOWER1"
            } else {
                "F_LOWER0"
            },
            0.,
            168.,
        );
        match self.page {
            Page::Title => {
                push_nav(&mut self.sounds, dy);
                self.title_row = cycle(self.title_row, dy, 2);
                let mut choose = enter;
                for (row, label) in ["Play", "Options"].iter().enumerate() {
                    let r = c.action("MHEAD", label, 196., 216. + 40. * row as f32, Min);
                    if r.clicked() {
                        self.title_row = row;
                        choose = true;
                    }
                }
                c.icon("KEY", frame, 114., 198. + 40. * self.title_row as f32);
                c.picture("F_BMG", 27., 408.);
                c.picture("F_DMA", 538., 360.);
                if c.action("MTEXT", "Esc: Quit", 320., 450., Center).clicked() || esc {
                    self.sounds.push(MenuSfx::Cancel);
                    self.quit = true;
                }
                if choose && self.title_row == 1 {
                    self.sounds.push(MenuSfx::Confirm);
                    self.page = Page::Options;
                    self.option_row = 0;
                    self.draft = self.settings;
                } else if choose && !self.chapters.is_empty() {
                    self.sounds.push(MenuSfx::Confirm);
                    self.page = Page::Player;
                }
            }
            Page::Options => {
                c.text("MHEAD", "Options", 320., 176., Center);
                push_nav(&mut self.sounds, dy);
                self.option_row = cycle(self.option_row, dy, 4);
                let labels = [
                    format!("Sound: {}", self.draft[0]),
                    format!("Music: {}", self.draft[1]),
                    format!(
                        "Text: {}",
                        ["Slow", "Normal", "Fast"][usize::from(self.draft[2] - 1)]
                    ),
                    format!(
                        "Music Mode: {}",
                        ["Radio", "Constant"][usize::from(self.draft[3])]
                    ),
                ];
                let mut adjust = dx;
                for (row, label) in labels.iter().enumerate() {
                    if c.action("MTEXT", label, 196., 224. + row as f32 * 30., Min)
                        .clicked()
                    {
                        self.option_row = row;
                        adjust = 1;
                    }
                }
                c.icon("KEY", frame, 114., 198. + self.option_row as f32 * 30.);
                let row = self.option_row;
                let (min, max) = [(0, 7), (0, 7), (1, 3), (0, 1)][row];
                let prev = self.draft[row];
                self.draft[row] = (i32::from(self.draft[row]) + adjust - min)
                    .rem_euclid(max - min + 1) as u8
                    + min as u8;
                if adjust != 0 && self.draft[row] != prev {
                    match row {
                        0 => self.sounds.push(MenuSfx::SoundTest),
                        1 => {} // Music volume changes live with no SFX.
                        2 => self.sounds.push(MenuSfx::for_text_speed(self.draft[2])),
                        3 => self.sounds.push(MenuSfx::for_music_mode(self.draft[3])),
                        _ => {}
                    }
                }
                if c.action("MTEXT", "Enter: Save", 640., 450., Max).clicked() || enter {
                    self.sounds.push(MenuSfx::Confirm);
                    self.settings = self.draft;
                    self.page = Page::Title;
                }
                if c.action("MTEXT", "Esc: Cancel", 0., 450., Min).clicked() || esc {
                    self.sounds.push(MenuSfx::Cancel);
                    self.page = Page::Title;
                }
            }
            Page::Reset => {
                c.picture(&format!("F_PLAY{}", self.player + 1), 396., 192.);
                c.picture("F_PLAYN", 356., 342.);
                c.text("MTEXT", &name, 446., 356., Center);
                push_nav(&mut self.sounds, dy);
                self.reset_row = cycle(self.reset_row, dy, 2);
                let mut confirm = enter;
                for (row, label) in ["Cancel", "Reset"].iter().enumerate() {
                    if c.action("MHEAD", label, 196., 216. + row as f32 * 40., Min)
                        .clicked()
                    {
                        self.reset_row = row;
                        confirm = true;
                    }
                }
                c.icon("KEY", frame, 114., 198. + 40. * self.reset_row as f32);
                if confirm && self.reset_row == 1 {
                    self.names[self.player] = NAMES[self.player].into();
                    self.scores[self.player] = [Some(0), None, None, None, None, None];
                }
                if confirm {
                    self.sounds.push(MenuSfx::Confirm);
                    self.page = Page::Player;
                } else if esc {
                    self.sounds.push(MenuSfx::Cancel);
                    self.page = Page::Player;
                }
            }
            Page::Player | Page::Rename => {
                c.picture(&format!("F_PLAY{}", self.player + 1), 268., 192.);
                c.text("MTEXT", "Scores", 500., 202., Center);
                {
                    let mut y = 226.;
                    for (index, id) in SECTIONS.iter().enumerate() {
                        if let Some(score) = self.scores[self.player][index]
                            && let Some((_, title)) = self.chapters.iter().find(|(s, _)| s == id)
                        {
                            c.text("MMISS", title, 414., y, Min);
                            c.text("MMISS", &score.to_string(), 612., y, Max);
                            y += 12.;
                        }
                    }
                }
                if self.page == Page::Rename {
                    let mut backspaced = false;
                    let mut typed = false;
                    ui.input(|i| {
                        if i.key_pressed(Key::Backspace) && self.edit_name.pop().is_some() {
                            backspaced = true;
                        }
                        for event in &i.events {
                            if let egui::Event::Text(s) = event {
                                for ch in s.chars().filter(|ch| ch.is_ascii_graphic() || *ch == ' ')
                                {
                                    if self.edit_name.len() < 14 {
                                        self.edit_name.push(ch);
                                        typed = true;
                                    }
                                }
                            }
                        }
                    });
                    if backspaced {
                        self.sounds.push(MenuSfx::Backspace);
                    }
                    if typed {
                        self.sounds.push(MenuSfx::TypeChar);
                    }
                    c.text("MHEAD", &self.edit_name, 318., 356., Center);
                    if c.action("MTEXT", "Esc: Cancel", 0., 450., Min).clicked() || esc {
                        self.sounds.push(MenuSfx::Cancel);
                        self.page = Page::Player;
                    }
                    let finish = if self.edit_name.is_empty() {
                        "Enter: Reset"
                    } else {
                        "Enter: Finish"
                    };
                    if c.action("MTEXT", finish, 640., 450., Max).clicked() || enter {
                        self.sounds.push(MenuSfx::Confirm);
                        self.names[self.player] = if self.edit_name.is_empty() {
                            NAMES[self.player].into()
                        } else {
                            self.edit_name.clone()
                        };
                        self.page = Page::Player;
                    }
                    return None;
                }
                c.picture("F_PLAYN", 228., 342.);
                c.text("MTEXT", &name, 320., 356., Center);
                let prev = c.action("MTEXT", "<", 216., 356., Center).clicked();
                let next = c.action("MTEXT", ">", 420., 356., Center).clicked();
                if c.action("MMISS", "Del: Rename", 440., 400., Min).clicked()
                    || ui.input(|i| i.key_pressed(Key::Delete))
                {
                    self.sounds.push(MenuSfx::Confirm);
                    self.edit_name.clear();
                    self.page = Page::Rename;
                }
                if c.action("MMISS", "R: Reset", 440., 415., Min).clicked()
                    || ui.input(|i| i.key_pressed(Key::R))
                {
                    self.sounds.push(MenuSfx::Confirm);
                    self.reset_row = 0;
                    self.page = Page::Reset;
                }
                let player_delta = dx + i32::from(next) - i32::from(prev);
                if player_delta != 0 {
                    self.sounds.push(MenuSfx::PlayerSwitch);
                }
                self.player = cycle(self.player, player_delta, NAMES.len());
                if c.action("MTEXT", "Enter: Play", 640., 450., Max).clicked() || enter {
                    self.sounds.push(MenuSfx::Confirm);
                    self.page = Page::Chapter;
                }
                if c.action("MTEXT", "Esc: Cancel", 0., 450., Min).clicked() || esc {
                    self.sounds.push(MenuSfx::Cancel);
                    self.page = Page::Title;
                }
            }
            Page::Chapter => {
                let group = |id: u32| id / 100;
                let cities: Vec<_> = (0..3)
                    .filter(|city| self.chapters.iter().any(|(id, _)| group(*id) == *city))
                    .collect();
                let mut city = group(self.chapters[self.chapter].0);
                if dx != 0 {
                    self.sounds.push(if dx < 0 {
                        MenuSfx::CityLeft
                    } else {
                        MenuSfx::CityRight
                    });
                    let current = cities.iter().position(|g| *g == city).unwrap_or_default();
                    city = cities[cycle(current, dx, cities.len())];
                    self.chapter = self
                        .chapters
                        .iter()
                        .position(|(id, _)| group(*id) == city)
                        .unwrap();
                }
                let rows: Vec<_> = self
                    .chapters
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (id, _))| (group(*id) == city).then_some(i))
                    .collect();
                let row = rows
                    .iter()
                    .position(|&i| i == self.chapter)
                    .unwrap_or_default();
                if dy != 0 && rows.len() > 1 {
                    push_nav(&mut self.sounds, dy);
                }
                self.chapter = rows[cycle(row, dy, rows.len())];
                c.text("MTEXT", "Public Enemies", 532., 130., Center);
                if let Some(career) = &self.career {
                    let index = SECTIONS
                        .iter()
                        .position(|&s| s == self.chapters[self.chapter].0)
                        .unwrap();
                    for (line, row) in (0..3).rev().enumerate() {
                        let (score, name) = career.high_score(index, row);
                        let y = 154. + 12. * line as f32;
                        c.text("MMISS", &name, 440., y, Min);
                        c.text("MMISS", &score.to_string(), 624., y, Max);
                    }
                }
                for city_id in cities {
                    let (name, icon, ix, iy, x, y, tx) = match city_id {
                        0 => ("Liberty City", "CITY4", 470., 242., 470., 306., 488.),
                        1 => ("San Andreas", "CITY1", 82., 230., 80., 302., 88.),
                        _ => ("Vice City", "CITY3", 374., 338., 368., 410., 368.),
                    };
                    c.icon(icon, frame, ix, iy);
                    if c.action("MTEXT", name, x, y, Min).clicked()
                        || ui
                            .interact(
                                c.rect(ix, iy, 64., 64.),
                                ui.scope_id().with(city_id),
                                egui::Sense::click(),
                            )
                            .clicked()
                    {
                        if city_id != city {
                            self.sounds.push(MenuSfx::CityRight);
                        }
                        self.chapter = self
                            .chapters
                            .iter()
                            .position(|(id, _)| group(*id) == city_id)
                            .unwrap();
                    }
                    if city_id == city {
                        for (r, &index) in rows.iter().enumerate() {
                            let ty = y + 30. + r as f32 * 15.;
                            if self.chapter == index {
                                c.icon("KEY", frame, tx - 70., ty - 31.);
                            }
                            if c.action(
                                "MMISS",
                                &format!("{}: {}", r + 1, self.chapters[index].1),
                                tx,
                                ty,
                                Min,
                            )
                            .clicked()
                            {
                                if self.chapter != index {
                                    self.sounds.push(MenuSfx::NavDown);
                                }
                                self.chapter = index;
                            }
                        }
                    }
                }
                if c.action("MTEXT", "Esc: Cancel", 0., 450., Min).clicked() || esc {
                    self.sounds.push(MenuSfx::Cancel);
                    self.page = Page::Player;
                } else if c.action("MTEXT", "Enter: Play", 640., 450., Max).clicked() || enter {
                    self.sounds.push(MenuSfx::Confirm);
                    return Some(self.chapters[self.chapter].0);
                }
            }
        }
        if !self.error.is_empty() || !status.is_empty() {
            c.text(
                "MTEXT",
                if status.is_empty() {
                    &self.error
                } else {
                    status
                },
                320.,
                420.,
                Center,
            );
        }
        None
    }
}
fn cycle(index: usize, delta: i32, len: usize) -> usize {
    (index as i32 + delta).rem_euclid(len as i32) as usize
}

fn push_nav(sounds: &mut Vec<MenuSfx>, dy: i32) {
    if dy < 0 {
        sounds.push(MenuSfx::NavUp);
    } else if dy > 0 {
        sounds.push(MenuSfx::NavDown);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_and_chapter_can_be_selected_before_launch() {
        let mut source = gfa_assets::MemSource::new();
        source.insert(
            "GTADATA/MISSION.INI",
            b"[1]\nFirst,1,NYC.CMP\n0\n-1\n-1\n[102]\nSecond,1,SANB.CMP\n0\n-1\n-1\n[1001]\nMultiplayer,1,NYC.CMP\n0\n-1\n-1\n"
                .to_vec(),
        );
        let mut menu = Menu::new(Box::new(source));
        assert_eq!(menu.chapters.len(), 2);
        let ctx = egui::Context::default();
        let mut press = |key| {
            let mut launch = None;
            for pressed in [true, false] {
                let mut input = egui::RawInput::default();
                input.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
                let mut output = ctx.run_ui(input, |ui| {
                    launch = launch.or(menu.draw(ui, ""));
                });
                output.textures_delta.clear();
            }
            launch
        };
        for key in [
            Key::Enter,
            Key::ArrowRight,
            Key::Enter,
            Key::Escape,
            Key::Enter,
            Key::ArrowRight,
        ] {
            assert_eq!(press(key), None);
        }
        assert_eq!(press(Key::Enter), Some(102));
        assert_eq!(menu.name(), "Katie");
        assert_eq!(
            menu.drain_sounds(),
            vec![
                MenuSfx::Confirm,
                MenuSfx::PlayerSwitch,
                MenuSfx::Confirm,
                MenuSfx::Cancel,
                MenuSfx::Confirm,
                MenuSfx::CityRight,
                MenuSfx::Confirm,
            ]
        );
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires the original game data"]
fn original_menu_art_and_chapters_load() {
    let source = crate::DataPath::resolve(None).unwrap().open().unwrap();
    let mut menu = Menu::new(source);
    assert_eq!(menu.chapters.len(), 6);
    assert!(menu.career.is_some());
    let ctx = egui::Context::default();
    for slot in 0..8 {
        menu.page = Page::Player;
        menu.player = slot;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            menu.draw(ui, "");
        });
        output.textures_delta.clear();
    }
    assert!(menu.art.all_loaded());
}
