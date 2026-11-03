-- Read-only runtime database compiled from the BerthaML corpus by compile_db.py.
-- Gratitude reads it; nothing writes to it after the build. user_version is the
-- database schema version, separate from the BerthaML language version in meta.
PRAGMA user_version = 1;

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE people (id TEXT PRIMARY KEY, name TEXT NOT NULL, sort_name TEXT NOT NULL);

-- place: the region label used in album names: a US state or Canadian province
-- code when the registry gives one, otherwise the country name.
CREATE TABLE venues (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    city TEXT NOT NULL,
    region TEXT NOT NULL,
    country TEXT NOT NULL,
    place TEXT NOT NULL
);
-- Registry name and every altName, for matching a venue written in show notes.
CREATE TABLE venue_names (venue TEXT NOT NULL REFERENCES venues, name TEXT NOT NULL);

CREATE TABLE songs (id TEXT PRIMARY KEY, title TEXT NOT NULL, kind TEXT NOT NULL);
-- Title and every altTitle.
CREATE TABLE song_titles (song TEXT NOT NULL REFERENCES songs, title TEXT NOT NULL);

-- corroboration is the build's derived level, never a review status.
CREATE TABLE shows (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,
    venue TEXT REFERENCES venues,
    billed TEXT NOT NULL,
    slot TEXT NOT NULL,
    status TEXT NOT NULL,
    coverage TEXT NOT NULL,
    corroboration TEXT NOT NULL
);
CREATE INDEX shows_date ON shows (date);
-- Canceled and private events, kept outside the show corpus. date may lack a day.
CREATE TABLE excluded_events (date TEXT NOT NULL, category TEXT NOT NULL, venue TEXT NOT NULL, href TEXT NOT NULL);
CREATE INDEX excluded_events_date ON excluded_events (date);
CREATE TABLE show_cites (show TEXT NOT NULL REFERENCES shows, n INTEGER NOT NULL, title TEXT NOT NULL, href TEXT NOT NULL);

-- n is the set's position in the show; number is its number within its kind.
CREATE TABLE sets (
    show TEXT NOT NULL REFERENCES shows,
    n INTEGER NOT NULL,
    kind TEXT NOT NULL,
    number INTEGER NOT NULL,
    mode TEXT NOT NULL,
    PRIMARY KEY (show, n)
);
-- A song performance (song set, event empty) or a non-song event such as tuning
-- (song empty). id is the corpus's permanent performance ID within the show.
CREATE TABLE performances (
    show TEXT NOT NULL REFERENCES shows,
    id TEXT NOT NULL,
    set_n INTEGER NOT NULL,
    n INTEGER NOT NULL,
    song TEXT REFERENCES songs,
    event TEXT NOT NULL,
    transition TEXT NOT NULL,
    continues TEXT NOT NULL,
    PRIMARY KEY (show, id)
);
CREATE INDEX performances_song ON performances (song);

-- Researched show lineups only. A show without rows has unresearched personnel.
CREATE TABLE lineup (show TEXT NOT NULL REFERENCES shows, n INTEGER NOT NULL, person TEXT NOT NULL REFERENCES people, instruments TEXT NOT NULL);
-- Membership-derived rosters. Always present these as roster-derived, never as who played.
CREATE TABLE roster_eras (start TEXT NOT NULL, finish TEXT NOT NULL, members TEXT NOT NULL);

-- kind uses Gratitude's codes: SBD, MTX, AUD, FM, VIDEO or UNKNOWN. inferred is 1
-- when the code comes from the corpus's inferredKind rather than a stated kind.
CREATE TABLE recordings (
    id TEXT PRIMARY KEY,
    show TEXT NOT NULL REFERENCES shows,
    provider TEXT NOT NULL,
    key TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL,
    inferred INTEGER NOT NULL,
    modifiers TEXT NOT NULL,
    credits TEXT NOT NULL,
    lineage TEXT NOT NULL
);
CREATE INDEX recordings_show ON recordings (show);
CREATE TABLE recording_ids (recording TEXT NOT NULL REFERENCES recordings, scheme TEXT NOT NULL, value TEXT NOT NULL, certainty TEXT NOT NULL);
CREATE INDEX recording_ids_value ON recording_ids (value);
-- Original-file track lists. seconds is NULL when the source gave no duration;
-- covers holds space-separated performance IDs and is empty while unmapped.
CREATE TABLE tracks (
    recording TEXT NOT NULL REFERENCES recordings,
    playlist TEXT NOT NULL,
    n INTEGER NOT NULL,
    title TEXT NOT NULL,
    file TEXT NOT NULL,
    seconds REAL,
    covers TEXT NOT NULL,
    mapping TEXT NOT NULL
);
CREATE INDEX tracks_recording ON tracks (recording);
