CREATE TABLE career (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL,
    current_slot INTEGER NOT NULL CHECK (current_slot BETWEEN 0 AND 7),
    sound_volume INTEGER NOT NULL CHECK (sound_volume BETWEEN 0 AND 7),
    music_volume INTEGER NOT NULL CHECK (music_volume BETWEEN 0 AND 7),
    text_speed INTEGER NOT NULL CHECK (text_speed BETWEEN 1 AND 3),
    music_mode INTEGER NOT NULL CHECK (music_mode BETWEEN 0 AND 1)
);
CREATE TABLE players (
    slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 0 AND 7),
    name TEXT NOT NULL
);
CREATE TABLE chapters (
    slot INTEGER NOT NULL REFERENCES players(slot),
    section INTEGER NOT NULL,
    best INTEGER,
    seen INTEGER NOT NULL CHECK (seen IN (0, 1)),
    PRIMARY KEY (slot, section)
);
CREATE TABLE high_scores (
    level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 5),
    rank INTEGER NOT NULL CHECK (rank BETWEEN 0 AND 2),
    name TEXT NOT NULL,
    score INTEGER NOT NULL,
    PRIMARY KEY (level, rank)
);
