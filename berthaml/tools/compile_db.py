"""Compile the BerthaML corpus into the read-only SQLite database Gratitude ships.

Reads the hand-owned show files and registries, the generated recordings, and the
build's derived corroboration levels and era rosters. Writes nothing in the corpus.
Standard library only, so the application build needs no extra Python packages.

    python tools/compile_db.py ../target/berthaml/berthaml.db
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import sys
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
NS = {"b": "urn:berthaml:1"}
SCHEMA = Path(__file__).with_name("berthaml-db.sql")

KIND_CODES = {
    "soundboard": "SBD",
    "matrix": "MTX",
    "audience": "AUD",
    "fm": "FM",
    "video": "VIDEO",
    "unknown": "UNKNOWN",
}
ROLE_LABELS = {
    "recordist": "recording",
    "taper": "recording",
    "transfer": "transfer",
    "mastering": "mastering",
    "transfer-mastering": "transfer/mastering",
}
# Album region for a venue outside the US and Canada. Earlier Gratitude releases named
# British shows by nation, and album names already written keep that spelling.
COUNTRIES = {
    "DE": "Germany",
    "DK": "Denmark",
    "EG": "Egypt",
    "ES": "Spain",
    "FR": "France",
    "JM": "Jamaica",
    "LU": "Luxembourg",
    "NL": "Netherlands",
    "SE": "Sweden",
    "CA": "Canada",
}
SCOTTISH = {"Edinburgh", "Glasgow"}


def tag(name):
    return f"{{urn:berthaml:1}}{name}"


def text(node):
    return " ".join((node.text or "").split()) if node is not None else ""


def place(region, country, city):
    if region:
        return region
    if country == "GB":
        return "Scotland" if city in SCOTTISH else "England"
    return COUNTRIES.get(country, country)


def seconds(duration):
    match = re.fullmatch(r"PT(?:(\d+)H)?(?:(\d+)M)?(?:([\d.]+)S)?", duration or "")
    if not duration or not match:
        return None
    h, m, s = match.groups()
    return int(h or 0) * 3600 + int(m or 0) * 60 + float(s or 0)


def corpus_digest(files):
    digest = hashlib.sha256()
    for path in files:
        digest.update(path.relative_to(ROOT).as_posix().encode() + b"\0")
        digest.update(path.read_bytes())
    return digest.hexdigest()


def load_registries(db):
    people = ET.parse(ROOT / "data/people.xml").getroot()
    for p in people.findall("b:person", NS):
        name = text(p.find("b:name", NS))
        db.execute("INSERT INTO people VALUES (?,?,?)", (p.get("id"), name, text(p.find("b:sortName", NS)) or name))
    venues = ET.parse(ROOT / "data/venues.xml").getroot()
    for v in venues.findall("b:venue", NS):
        loc = v.find("b:locality", NS)
        city, region, country = (loc.get(k, "") if loc is not None else "" for k in ("city", "region", "country"))
        name = text(v.find("b:name", NS))
        db.execute(
            "INSERT INTO venues VALUES (?,?,?,?,?,?)", (v.get("id"), name, city, region, country, place(region, country, city))
        )
        for alias in [name] + [text(a) for a in v.findall("b:altName", NS)]:
            db.execute("INSERT INTO venue_names VALUES (?,?)", (v.get("id"), alias))
    songs = ET.parse(ROOT / "data/songs.xml").getroot()
    for s in songs.findall("b:song", NS):
        title = text(s.find("b:title", NS))
        db.execute("INSERT INTO songs VALUES (?,?,?)", (s.get("id"), title, s.get("kind", "song")))
        for alias in [title] + [text(a) for a in s.findall("b:altTitle", NS)]:
            db.execute("INSERT INTO song_titles VALUES (?,?)", (s.get("id"), alias))


def load_show(db, path, levels):
    show = ET.parse(path).getroot()
    sid = show.get("id")
    record = show.find("b:record", NS)
    venue = show.find("b:venue", NS)
    program = show.find("b:program", NS)
    db.execute(
        "INSERT INTO shows VALUES (?,?,?,?,?,?,?,?)",
        (
            sid,
            show.get("date"),
            venue.get("ref") if venue is not None else None,
            venue.get("nameAsBilled", "") if venue is not None else "",
            text(show.find("b:slot", NS)),
            record.get("status"),
            program.get("coverage", "none") if program is not None else "none",
            levels.get(sid, ""),
        ),
    )
    for n, cite in enumerate(record.findall("b:cite", NS), 1):
        db.execute("INSERT INTO show_cites VALUES (?,?,?,?)", (sid, n, cite.get("title", ""), cite.get("href", "")))
    lineup = show.find("b:lineup", NS)
    if lineup is not None:
        for n, player in enumerate(lineup.findall("b:player", NS), 1):
            instruments = " ".join(text(i) for i in player.findall("b:instrument", NS))
            db.execute("INSERT INTO lineup VALUES (?,?,?,?)", (sid, n, player.get("person"), instruments))
    if program is None:
        return
    for set_n, s in enumerate(program.findall("b:set", NS), 1):
        db.execute("INSERT INTO sets VALUES (?,?,?,?,?)", (sid, set_n, s.get("kind"), int(s.get("n")), s.get("mode", "unknown")))
        items = [e for e in s if e.tag in (tag("performance"), tag("event"))]
        for n, item in enumerate(items, 1):
            song = item.get("song") if item.tag == tag("performance") else None
            event = "" if song else item.get("kind", "")
            db.execute(
                "INSERT INTO performances VALUES (?,?,?,?,?,?,?,?)",
                (sid, item.get("id"), set_n, n, song, event, item.get("transition", "unknown"), item.get("continues", "")),
            )


def load_recordings(db, path):
    doc = ET.parse(path).getroot()
    sid = doc.get("show")
    for r in doc.findall("b:recording", NS):
        identifiers = r.findall("b:identifier", NS)
        listing = r.find("b:listing", NS)
        provider = listing.get("provider", "") if listing is not None else ""
        primary = next((text(i) for i in identifiers if i.get("scheme") != "shnid"), "")
        kind = r.get("kind", "unknown")
        inferred = kind == "unknown" and r.get("inferredKind") is not None
        code = KIND_CODES[r.get("inferredKind") if inferred else kind]
        credits = [f"{m.get('name')} ({ROLE_LABELS.get(m.get('role'), 'role unspecified')})" for m in r.findall("b:personMatch", NS)]
        for c in r.findall("b:credit", NS):
            credits.append(f"{ROLE_LABELS.get(c.get('role'), 'role unspecified').capitalize()} credit: {text(c)}")
        db.execute(
            "INSERT INTO recordings VALUES (?,?,?,?,?,?,?,?,?,?)",
            (
                r.get("id"),
                sid,
                provider,
                primary,
                listing.get("title", primary) if listing is not None else primary,
                code,
                int(inferred),
                r.get("modifiers", ""),
                " | ".join(credits),
                text(r.find("b:lineage", NS)),
            ),
        )
        for i in identifiers:
            db.execute("INSERT INTO recording_ids VALUES (?,?,?,?)", (r.get("id"), i.get("scheme"), text(i), i.get("certainty", "certain")))
        for playlist in r.findall("b:playlist", NS):
            for t in playlist.findall("b:track", NS):
                db.execute(
                    "INSERT INTO tracks VALUES (?,?,?,?,?,?,?,?)",
                    (
                        r.get("id"),
                        playlist.get("id"),
                        int(t.get("n")),
                        t.get("title", ""),
                        t.get("file", ""),
                        seconds(t.get("duration")),
                        t.get("covers", ""),
                        t.get("mappingStatus", ""),
                    ),
                )


def compile_db(output):
    shows = sorted((ROOT / "data/shows").glob("*/*.xml"))
    recordings = sorted((ROOT / "data/recordings").glob("*/*.xml"))
    registries = [ROOT / f"data/{name}.xml" for name in ("people", "songs", "venues", "releases")]
    reports = [ROOT / "reports/corroboration.jsonl", ROOT / "reports/roster-eras.json", ROOT / "reports/event-exclusions.jsonl"]
    dataset = json.loads((ROOT / "dataset.json").read_text(encoding="utf-8"))
    levels = {}
    for line in reports[0].read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        levels[row["show"]] = row["level"]

    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temp = output.with_name(output.name + ".tmp")
    temp.unlink(missing_ok=True)
    db = sqlite3.connect(temp)
    try:
        db.executescript(SCHEMA.read_text(encoding="utf-8"))
        load_registries(db)
        for path in shows:
            load_show(db, path, levels)
        for path in recordings:
            load_recordings(db, path)
        for era in json.loads(reports[1].read_text(encoding="utf-8")):
            db.execute("INSERT INTO roster_eras VALUES (?,?,?)", (era["start"], era["to"], " ".join(era["members"])))
        for line in reports[2].read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            date = row["date_label"].split(" ")[0]
            db.execute("INSERT INTO excluded_events VALUES (?,?,?,?)", (date, row["category"], row["venue"], row["url"]))
        meta = {
            "dataset": dataset["name"],
            "dataset_version": dataset["version"],
            "as_of": dataset["as_of"],
            "status": dataset["status"],
            "language_version": "1.1",
            "corpus_sha256": corpus_digest(shows + recordings + registries + reports),
            "attribution": "Show facts from the BerthaML draft corpus, compiled from Jerrybase factual captures, "
            "gdshowsdb (MIT, Copyright Jeff Smith) and Archive.org metadata. Curation is incomplete; "
            "records marked draft or skeleton are imported, not reviewed.",
        }
        db.executemany("INSERT INTO meta VALUES (?,?)", sorted(meta.items()))
        problems = db.execute("PRAGMA foreign_key_check").fetchall()
        if problems:
            raise SystemExit(f"Unresolved references in the compiled database: {problems[:5]}")
        db.commit()
        db.execute("VACUUM")
    finally:
        db.close()
    temp.replace(output)
    counts = sqlite3.connect(f"file:{output}?mode=ro", uri=True)
    summary = {t: counts.execute(f"SELECT count(*) FROM {t}").fetchone()[0] for t in ("shows", "performances", "recordings", "tracks")}
    counts.close()
    print(f"{output}: " + ", ".join(f"{v:,} {k}" for k, v in summary.items()))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("output", help="Path of the database to write")
    compile_db(parser.parse_args().output)


if __name__ == "__main__":
    sys.exit(main())
