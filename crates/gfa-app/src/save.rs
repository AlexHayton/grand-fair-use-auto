//! One SQLite schema and migration path for native and web careers.
use crate::career::{Career, Entry};
use gfa_assets::AssetSource;
use rusqlite::{Connection, TransactionBehavior, params};
use std::fmt::Display;

const MIGRATIONS: &[&str] = &[
    include_str!("save/001_career.sql"),
    include_str!("save/002_controls.sql"),
];
const APP_ID: i32 = 0x47464143; // GFAC
type Result<T, E = String> = std::result::Result<T, E>;

fn message(error: impl Display) -> String {
    error.to_string()
}

pub(crate) struct SaveStore {
    db: Connection,
    revision: i64,
}

impl SaveStore {
    #[cfg(any(test, target_arch = "wasm32"))]
    pub fn memory() -> Result<Self> {
        Ok(Self {
            db: Connection::open_in_memory().map_err(message)?,
            revision: 0,
        })
    }

    fn open(&mut self, seed: impl FnOnce() -> Career) -> Result<Career> {
        self.db
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;")
            .map_err(message)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(message)?;
        let version: u32 = tx
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(message)?;
        let id: i32 = tx
            .pragma_query_value(None, "application_id", |row| row.get(0))
            .map_err(message)?;
        if version as usize > MIGRATIONS.len() {
            return Err(format!("career schema {version} requires a newer game"));
        }
        let fresh = version == 0;
        if fresh {
            let count: i64 = tx
                .query_row("SELECT count(*) FROM sqlite_schema", [], |r| r.get(0))
                .map_err(message)?;
            if count != 0 || id != 0 {
                return Err("unrecognized career database".into());
            }
            tx.pragma_update(None, "application_id", APP_ID)
                .map_err(message)?;
        } else if id != APP_ID {
            return Err("unrecognized career database".into());
        }
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            tx.execute_batch(sql).map_err(message)?;
            tx.pragma_update(None, "user_version", (index + 1) as i64)
                .map_err(message)?;
        }
        let career = if fresh {
            let career = seed();
            write_rows(&tx, &career, 0)?;
            career
        } else {
            read_rows(&tx)?
        };
        self.revision = tx
            .query_row("SELECT revision FROM career WHERE id=1", [], |r| r.get(0))
            .map_err(message)?;
        tx.commit().map_err(message)?;
        if (version as usize) < MIGRATIONS.len() {
            self.flush()?;
        }
        Ok(career)
    }

    fn commit(&mut self, career: &Career) -> Result<()> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(message)?;
        let version: u32 = tx
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(message)?;
        let revision: i64 = tx
            .query_row("SELECT revision FROM career WHERE id=1", [], |r| r.get(0))
            .map_err(message)?;
        if revision != self.revision || version as usize != MIGRATIONS.len() {
            return Err("another session changed the career. Reopen the game.".into());
        }
        write_rows(&tx, career, revision + 1)?;
        tx.commit().map_err(message)?;
        self.revision += 1;
        self.flush()
    }

    fn flush(&mut self) -> Result<()> {
        #[cfg(target_arch = "wasm32")]
        {
            let bytes = self.db.serialize(rusqlite::MAIN_DB).map_err(message)?;
            browser::write(&bytes).map_err(|e| format!("{e:?}"))?;
        }
        Ok(())
    }
}

fn write_rows(db: &Connection, career: &Career, revision: i64) -> Result<()> {
    let o = &career.options;
    let bindings = toml::to_string(&o.keybindings).map_err(message)?;
    let filter = toml::Value::try_from(o.hud_filter).map_err(message)?;
    db.execute(
        "INSERT OR REPLACE INTO career
         (id, revision, current_slot, sound_volume, music_volume, text_speed, music_mode, keybindings, hud_filter)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            revision, career.current_slot as i64, o.sound_volume, o.music_volume,
            o.text_speed, o.music_mode, bindings, filter.as_str().ok_or("invalid HUD filter")?,
        ],
    ).map_err(message)?;
    db.execute("DELETE FROM chapters", []).map_err(message)?;
    for (slot, p) in career.players.iter().enumerate() {
        let slot = slot as i64;
        db.execute("INSERT INTO players(slot,name) VALUES(?1,?2) ON CONFLICT(slot) DO UPDATE SET name=excluded.name",
            params![slot,p.name]).map_err(message)?;
        for section in p
            .best
            .keys()
            .chain(p.seen.iter())
            .collect::<std::collections::BTreeSet<_>>()
        {
            db.execute(
                "INSERT INTO chapters(slot,section,best,seen) VALUES(?1,?2,?3,?4)",
                params![slot, section, p.best.get(section), p.seen.contains(section)],
            )
            .map_err(message)?;
        }
    }
    for (level, table) in career.high_scores.iter().enumerate() {
        for (rank, entry) in table.iter().enumerate() {
            db.execute(
                "INSERT OR REPLACE INTO high_scores(level,rank,name,score) VALUES(?1,?2,?3,?4)",
                params![level as i64, rank as i64, entry.name, entry.score],
            )
            .map_err(message)?;
        }
    }
    Ok(())
}

fn read_rows(db: &Connection) -> Result<Career> {
    let mut c = Career::from_original(None);
    let mut settings = db
        .prepare("SELECT * FROM career WHERE id=1")
        .map_err(message)?;
    let mut rows = settings.query([]).map_err(message)?;
    let row = rows
        .next()
        .map_err(message)?
        .ok_or("missing career settings")?;
    c.current_slot = usize::from(row.get::<_, u8>("current_slot").map_err(message)?);
    let o = &mut c.options;
    o.sound_volume = row.get("sound_volume").map_err(message)?;
    o.music_volume = row.get("music_volume").map_err(message)?;
    o.text_speed = row.get("text_speed").map_err(message)?;
    o.music_mode = row.get("music_mode").map_err(message)?;
    if c.current_slot >= c.players.len()
        || o.sound_volume > 7
        || o.music_volume > 7
        || !(1..=3).contains(&o.text_speed)
        || o.music_mode > 1
    {
        return Err("invalid career options".into());
    }
    let bindings: String = row.get("keybindings").map_err(message)?;
    o.keybindings = toml::from_str(&bindings).map_err(message)?;
    let filter: String = row.get("hud_filter").map_err(message)?;
    o.hud_filter = toml::Value::String(filter).try_into().map_err(message)?;
    for (slot, player) in c.players.iter_mut().enumerate() {
        player.name = db
            .query_row(
                "SELECT name FROM players WHERE slot=?1",
                [slot as i64],
                |r| r.get(0),
            )
            .map_err(message)?;
        player.best.clear();
    }
    let mut stmt = db
        .prepare("SELECT slot,section,best,seen FROM chapters ORDER BY section")
        .map_err(message)?;
    let mut rows = stmt.query([]).map_err(message)?;
    while let Some(r) = rows.next().map_err(message)? {
        let slot: u8 = r.get(0).map_err(message)?;
        let p = c
            .players
            .get_mut(usize::from(slot))
            .ok_or("invalid player slot")?;
        let section: u32 = r.get(1).map_err(message)?;
        if let Some(score) = r.get(2).map_err(message)? {
            p.best.insert(section, score);
        }
        if r.get(3).map_err(message)? {
            p.seen.push(section);
        }
    }
    for (level, table) in c.high_scores.iter_mut().enumerate() {
        for (rank, entry) in table.iter_mut().enumerate() {
            *entry = db
                .query_row(
                    "SELECT name,score FROM high_scores WHERE level=?1 AND rank=?2",
                    params![level as i64, rank as i64],
                    |r| {
                        Ok(Entry {
                            name: r.get(0)?,
                            score: r.get(1)?,
                        })
                    },
                )
                .map_err(message)?;
        }
    }
    Ok(c)
}

pub(crate) struct CareerSave {
    pub career: Career,
    store: Option<SaveStore>,
    pub error: String,
}

impl CareerSave {
    pub fn open(source: &dyn AssetSource, store: Result<SaveStore>) -> Self {
        let seed = || {
            let original = source
                .read("GTADATA/PLAYER_A.DAT")
                .ok()
                .and_then(|b| gfa_formats::career::Career::parse(&b).ok());
            Career::from_original(original.as_ref())
        };
        match store.and_then(|mut s| Ok((s.open(seed)?, s))) {
            Ok((career, store)) => Self {
                career,
                store: Some(store),
                error: String::new(),
            },
            Err(e) => Self {
                career: seed(),
                store: None,
                error: format!("Career saves unavailable: {e}"),
            },
        }
    }
    pub fn commit(&mut self) {
        if let Some(store) = &mut self.store {
            self.error = match store.commit(&self.career) {
                Ok(()) => String::new(),
                Err(e) => {
                    log::warn!("Career save failed: {e}");
                    format!("Career save failed: {e}")
                }
            };
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn platform_store() -> Result<SaveStore> {
    let dir = gfa_assets::datapath::settings_dir().ok_or("no settings directory")?;
    std::fs::create_dir_all(&dir).map_err(message)?;
    let db = Connection::open(dir.join("career.sqlite3")).map_err(message)?;
    Ok(SaveStore { db, revision: 0 })
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_namespace = window, js_name = gfaReadCareer)]
        pub fn read() -> Result<Option<Vec<u8>>, JsValue>;
        #[wasm_bindgen(catch, js_namespace = window, js_name = gfaWriteCareer)]
        pub fn write(bytes: &[u8]) -> Result<(), JsValue>;
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn platform_store() -> Result<SaveStore> {
    let mut store = SaveStore::memory()?;
    if let Some(bytes) = browser::read().map_err(|e| format!("{e:?}"))? {
        if !bytes.starts_with(b"SQLite format 3\0") {
            return Err("invalid career database".into());
        }
        store
            .db
            .deserialize_read_exact(rusqlite::MAIN_DB, bytes.as_slice(), bytes.len(), false)
            .map_err(message)?;
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::Action;
    use winit::keyboard::KeyCode;

    fn version_one() -> SaveStore {
        let s = SaveStore::memory().unwrap();
        s.db.execute_batch(MIGRATIONS[0]).unwrap();
        s.db.pragma_update(None, "application_id", APP_ID).unwrap();
        s.db.execute_batch(
            "
            PRAGMA user_version=1;
            INSERT INTO career VALUES(1,7,4,5,3,2,1);
            WITH RECURSIVE n(i) AS (VALUES(0) UNION ALL SELECT i+1 FROM n WHERE i<7)
              INSERT INTO players SELECT i, 'Player ' || i FROM n;
            INSERT INTO chapters VALUES(4,102,9001,1);
            WITH RECURSIVE n(i) AS (VALUES(0) UNION ALL SELECT i+1 FROM n WHERE i<17)
              INSERT INTO high_scores SELECT i/3,i%3,'Old name',i FROM n;
        ",
        )
        .unwrap();
        s
    }

    #[test]
    fn migrates_old_career_then_reloads_new_options_and_scores() {
        let mut s = version_one();
        let mut c = s.open(|| panic!("existing profile must survive")).unwrap();
        assert_eq!(c.current_slot, 4);
        assert_eq!(c.players[4].name, "Player 4");
        assert_eq!(c.players[4].best[&102], 9001);
        assert_eq!(c.players[4].seen, [102]);
        assert_eq!(c.options.music_volume, 3);
        assert!(c.options.keybindings.is_empty());
        c.options.keybindings.insert(Action::Fire, KeyCode::KeyG);
        c.options.hud_filter = gfa_render::hud::HudFilter::Epx;
        c.record_result(2, 12000, false);
        s.commit(&c).unwrap();
        let reloaded = s.open(|| panic!()).unwrap();
        assert_eq!(reloaded.options.keybindings[&Action::Fire], KeyCode::KeyG);
        assert_eq!(reloaded.options.hud_filter, gfa_render::hud::HudFilter::Epx);
        assert_eq!(reloaded.players[4].best[&102], 12000);
        assert_eq!(reloaded.high_scores[2][2].name, "Player 4");
    }

    #[test]
    fn failed_migration_rolls_back_and_future_versions_are_preserved() {
        let mut s = version_one();
        s.db.execute("DELETE FROM players WHERE slot=0", [])
            .unwrap();
        assert!(s.open(|| panic!()).is_err());
        let version = |db: &Connection| {
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(version(&s.db), 1);
        assert!(s.db.prepare("SELECT keybindings FROM career").is_err());
        s.db.pragma_update(None, "user_version", 99).unwrap();
        assert!(s.open(|| panic!()).is_err());
        assert_eq!(version(&s.db), 99);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_relaunch_and_concurrent_writer() {
        let path = std::env::temp_dir().join(format!(
            "gfa-career-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let connect = || SaveStore {
            db: Connection::open(&path).unwrap(),
            revision: 0,
        };
        let mut first = connect();
        let mut c = first.open(|| Career::from_original(None)).unwrap();
        let mut second = connect();
        let stale = second.open(|| panic!()).unwrap();
        c.players[1].name = "Ada".into();
        first.commit(&c).unwrap();
        assert!(second.commit(&stale).is_err());
        drop(first);
        drop(second);
        assert_eq!(connect().open(|| panic!()).unwrap().players[1].name, "Ada");
        std::fs::remove_file(path).unwrap();
    }
}
