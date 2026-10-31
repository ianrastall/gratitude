#!/usr/bin/env python3
"""
BerthaML validator.

Validates a BerthaML data directory in two passes:

1. Every file against schemas/berthaml.xsd (XSD 1.0).
2. Rules XSD 1.0 cannot express: references between files, show ID/date
   agreement, set ordering, mode inheritance, personnel inheritance,
   reprise targets, membership dates, and release track mappings.

Expected layout:
    <data>/people.xml
    <data>/songs.xml
    <data>/venues.xml
    <data>/releases.xml
    <data>/shows/<YYYY>/<show-id>.xml

Usage:
    python validate.py <data-dir> [--schema PATH] [--strict]

Exit code 0 when there are no errors (and no warnings with --strict),
1 otherwise, 2 for usage problems.
"""

import argparse
import calendar
import re
import sys
from datetime import date
from pathlib import Path

from lxml import etree

NS = "urn:berthaml:1"
NSMAP = {"b": NS}


def q(tag):
    return f"{{{NS}}}{tag}"


REGISTRIES = {
    "people.xml": "people",
    "songs.xml": "songs",
    "venues.xml": "venues",
    "releases.xml": "releases",
}

SET_RANK = {"soundcheck": 0, "set": 1, "encore": 2}

SHOW_ID_RE = re.compile(r"^gd(\d{4})-(\d{2}|xx)-(\d{2}|xx)(?:-([a-z0-9]+))?$")


class Report:
    def __init__(self, root):
        self.root = root
        self.errors = 0
        self.warnings = 0

    def _emit(self, level, path, line, msg):
        try:
            shown = path.relative_to(self.root)
        except ValueError:
            shown = path
        loc = f"{shown}:{line}" if line else f"{shown}"
        print(f"{level} {loc}: {msg}")

    def error(self, path, line, msg):
        self.errors += 1
        self._emit("ERROR", path, line, msg)

    def warn(self, path, line, msg):
        self.warnings += 1
        self._emit("WARN ", path, line, msg)


def fuzzy_bounds(text):
    """Return (first_day, last_day) for YYYY, YYYY-MM or YYYY-MM-DD."""
    parts = text.split("-")
    year = int(parts[0])
    if len(parts) == 1:
        return date(year, 1, 1), date(year, 12, 31)
    month = int(parts[1])
    if len(parts) == 2:
        return date(year, month, 1), date(year, month, calendar.monthrange(year, month)[1])
    day = int(parts[2])
    return date(year, month, day), date(year, month, day)


def load(path, schema, report):
    try:
        tree = etree.parse(str(path))
    except etree.XMLSyntaxError as exc:
        report.error(path, exc.lineno, f"not well-formed XML: {exc.msg}")
        return None
    if not schema.validate(tree):
        for entry in schema.error_log:
            report.error(path, entry.line, f"schema: {entry.message}")
        return None
    return tree


def instruments_of(player):
    return tuple(i.text for i in player.findall("b:instrument", NSMAP))


# ---------------------------------------------------------------------------
# Registries
# ---------------------------------------------------------------------------

def check_people(path, root, report):
    people = {}
    for person in root.findall("b:person", NSMAP):
        pid = person.get("id")
        periods = []
        for m in person.findall("b:membership", NSMAP):
            lo, _ = fuzzy_bounds(m.get("from"))
            _, hi = fuzzy_bounds(m.get("to"))
            if lo > hi:
                report.error(path, m.sourceline, f"{pid}: membership 'from' is after 'to'")
            periods.append((lo, hi, m.get("role")))
        people[pid] = periods
    return people


def check_songs(path, root, people, report):
    songs = {}
    for song in root.findall("b:song", NSMAP):
        songs[song.get("id")] = song
    for song in root.findall("b:song", NSMAP):
        sid = song.get("id")
        for credit in song.findall("b:credit", NSMAP):
            person = credit.get("person")
            text = (credit.text or "").strip()
            if person and text:
                report.error(path, credit.sourceline, f"{sid}: credit has both @person and a text name")
            elif not person and not text:
                report.error(path, credit.sourceline, f"{sid}: credit has neither @person nor a text name")
            if person and person not in people:
                report.error(path, credit.sourceline, f"{sid}: unknown person '{person}'")
        for lead in song.findall("b:defaultLead", NSMAP):
            if lead.get("person") not in people:
                report.error(path, lead.sourceline, f"{sid}: unknown person '{lead.get('person')}'")
        part_of = song.find("b:partOf", NSMAP)
        if part_of is not None:
            target = songs.get(part_of.get("song"))
            if target is not None and target.get("kind") != "suite":
                report.warn(path, part_of.sourceline, f"{sid}: partOf target '{part_of.get('song')}' is not kind=\"suite\"")
    return songs


# ---------------------------------------------------------------------------
# Shows
# ---------------------------------------------------------------------------

def apply_override(current, container, path, report, label):
    """Apply a <personnel> or <personnelChanges> child of container, if any."""
    explicit = container.find("b:personnel", NSMAP)
    if explicit is not None:
        result = {}
        for p in explicit.findall("b:player", NSMAP):
            person = p.get("person")
            if person in result:
                report.error(path, p.sourceline, f"{label}: {person} listed twice")
            result[person] = (p.get("role") or "member", instruments_of(p))
        return result

    changes = container.find("b:personnelChanges", NSMAP)
    if changes is None:
        return current

    result = dict(current)
    for c in changes:
        if not isinstance(c.tag, str):
            continue
        person = c.get("person")
        kind = etree.QName(c).localname
        if kind == "remove":
            if person not in result:
                report.error(path, c.sourceline, f"{label}: remove {person}, who is not in the inherited personnel")
            result.pop(person, None)
        elif kind == "add":
            if person in result:
                report.error(path, c.sourceline, f"{label}: add {person}, who is already in the inherited personnel (use change)")
            result[person] = (c.get("role") or "member", instruments_of(c))
        elif kind == "change":
            if person not in result:
                report.error(path, c.sourceline, f"{label}: change {person}, who is not in the inherited personnel (use add)")
                result[person] = (c.get("role") or "member", instruments_of(c))
            else:
                result[person] = (result[person][0], instruments_of(c))
    return result


def all_person_refs(root):
    xpath = (".//b:player/@person | .//b:participant/@person | .//b:add/@person | "
             ".//b:change/@person | .//b:remove/@person | .//b:lead/@person | .//b:tease/@by")
    for attr in root.xpath(xpath, namespaces=NSMAP):
        yield attr.getparent(), str(attr)


def check_show(path, root, ctx, report):
    sid = root.get("id")
    raw_date = root.get("date")

    # File name and directory agree with the ID.
    if path.stem != sid:
        report.error(path, root.sourceline, f"file name does not match show id '{sid}'")
    m = SHOW_ID_RE.match(sid)
    if m and path.parent.name != m.group(1):
        report.warn(path, root.sourceline, f"show is not in a directory named for its year ({m.group(1)})")

    # ID agrees with date precision and value.
    parts = raw_date.split("-")
    expected = "gd" + parts[0] + "-" + (parts[1] if len(parts) > 1 else "xx") + "-" + (parts[2] if len(parts) > 2 else "xx")
    if not m or sid[: len(expected)] != expected or (len(sid) > len(expected) and sid[len(expected)] != "-"):
        report.error(path, root.sourceline, f"show id '{sid}' does not agree with date '{raw_date}' (expected '{expected}' plus optional suffix)")

    if sid in ctx["shows"]:
        report.error(path, root.sourceline, f"duplicate show id; also in {ctx['shows'][sid]['path']}")
    lo, hi = fuzzy_bounds(raw_date)

    # Registry references.
    venue = root.find("b:venue", NSMAP)
    if venue.get("ref") not in ctx["venues"]:
        report.error(path, venue.sourceline, f"unknown venue '{venue.get('ref')}'")
    for el, person in all_person_refs(root):
        if person not in ctx["people"]:
            report.error(path, el.sourceline, f"unknown person '{person}'")
    for el in root.iterfind(".//b:performance", NSMAP):
        if el.get("song") not in ctx["songs"]:
            report.error(path, el.sourceline, f"unknown song '{el.get('song')}'")
        elif ctx["songs"][el.get("song")].get("kind") == "suite":
            report.warn(path, el.sourceline, f"performance of suite '{el.get('song')}'; prefer listing its parts")
    for el in root.iterfind(".//b:tease", NSMAP):
        if el.get("song") not in ctx["songs"]:
            report.error(path, el.sourceline, f"unknown teased song '{el.get('song')}'")
    for el in root.iterfind("b:sources/b:release", NSMAP):
        if el.get("ref") not in ctx["releases"]:
            report.error(path, el.sourceline, f"unknown release '{el.get('ref')}'")

    # Lineup.
    lineup = {}
    for p in root.findall("b:lineup/b:player", NSMAP):
        person = p.get("person")
        if person in lineup:
            report.error(path, p.sourceline, f"lineup: {person} listed twice")
        lineup[person] = (p.get("role") or "member", instruments_of(p))
        role = p.get("role") or "member"
        if role in ("member", "touring-member") and person in ctx["people"]:
            periods = [x for x in ctx["people"][person] if x[2] == role]
            if not any(plo <= hi and lo <= phi for plo, phi, _ in periods):
                report.warn(path, p.sourceline, f"lineup: {person} has role '{role}' but no {role} period covers {raw_date}")
    if lo == hi and root.find("b:lineup", NSMAP) is not None:
        for person, periods in ctx["people"].items():
            for plo, phi, role in periods:
                if role == "member" and plo <= lo <= phi and person not in lineup:
                    report.warn(path, root.find("b:lineup", NSMAP).sourceline,
                                f"lineup: member {person} is not listed; confirm the absence")

    # Program.
    program = root.find("b:program", NSMAP)
    sets = program.findall("b:set", NSMAP)
    coverage = program.get("coverage")
    if coverage == "none" and sets:
        report.error(path, program.sourceline, "coverage=\"none\" but sets are listed")
    if coverage != "none" and not sets:
        report.error(path, program.sourceline, f"coverage=\"{coverage}\" but no sets are listed")

    last_rank = -1
    seen_n = {}
    for s in sets:
        kind, n = s.get("kind"), int(s.get("n"))
        if kind in SET_RANK:
            if SET_RANK[kind] < last_rank:
                report.error(path, s.sourceline, f"{kind} {n} appears after a later kind of set")
            last_rank = max(last_rank, SET_RANK[kind])
        expected_n = seen_n.get(kind, 0) + 1
        if n != expected_n:
            report.error(path, s.sourceline, f"{kind} numbered {n}; expected {expected_n}")
        seen_n[kind] = n

    local_items = {}
    performances = []
    for s in sets:
        label_set = f"{s.get('kind')} {s.get('n')}"
        set_personnel = apply_override(lineup, s, path, report, label_set)
        for item in s:
            if not isinstance(item.tag, str):
                continue
            name = etree.QName(item).localname
            if name == "event":
                local_items[item.get("id")] = item
            elif name == "performance":
                local_items[item.get("id")] = item
                performances.append(item)
                label = f"{label_set}, {item.get('id')}"
                if s.get("mode") == "mixed" and item.get("mode") is None:
                    report.error(path, item.sourceline, f"{label}: set mode is mixed, so the performance must state its mode")
                effective = apply_override(set_personnel, item, path, report, label)
                for lead in item.findall("b:lead", NSMAP):
                    if lead.get("person") not in effective:
                        report.warn(path, lead.sourceline, f"{label}: lead {lead.get('person')} is not in the effective personnel")

    # Reprises.
    order = {p.get("id"): i for i, p in enumerate(performances)}
    for p in performances:
        target_id = p.get("continues")
        if target_id is None:
            continue
        target = local_items.get(target_id)
        if target is None or etree.QName(target).localname != "performance":
            continue  # XSD keyref already reports a missing target
        if order[target_id] >= order[p.get("id")]:
            report.error(path, p.sourceline, f"{p.get('id')} continues {target_id}, which does not come earlier")
        if target.get("song") != p.get("song"):
            report.error(path, p.sourceline, f"{p.get('id')} continues {target_id}, which is a different song")

    if performances and performances[-1].get("transition") == "segue":
        report.warn(path, performances[-1].sourceline, "last performance of the show is marked as a segue")

    # Recording tracks must cover performances or events, not other elements.
    for recording in root.findall('b:sources/b:recording', NSMAP):
        check_track_list(path, recording.findall('b:track', NSMAP), report, recording.get('id'))
        for playlist in recording.findall('b:playlist', NSMAP):
            check_track_list(path, playlist.findall('b:track', NSMAP), report, playlist.get('id'))
    for track in root.xpath('b:sources/b:recording/b:track | b:sources/b:recording/b:playlist/b:track', namespaces=NSMAP):
        for ref in (track.get("covers") or '').split():
            if ref not in local_items:
                report.error(path, track.sourceline, f"track {track.get('n')} covers '{ref}', which is not a performance or event")

    ctx["shows"][sid] = {
        "path": path,
        "items": local_items,
        "releases": {el.get("ref") for el in root.iterfind("b:sources/b:release", NSMAP)},
        "date": raw_date,
        "full_date": lo == hi,
        "slot": root.findtext("b:slot", namespaces=NSMAP),
    }


def check_same_day(ctx, report):
    by_date = {}
    for sid, info in ctx["shows"].items():
        if info["full_date"]:
            by_date.setdefault(info["date"], []).append(sid)
    for day, ids in by_date.items():
        if len(ids) < 2:
            continue
        for sid in ids:
            info = ctx["shows"][sid]
            if info["slot"] is None:
                report.error(info["path"], None, f"{len(ids)} shows on {day}; this one needs <slot>")
            if sid == "gd" + day:
                report.error(info["path"], None, f"{len(ids)} shows on {day}; this one needs an id suffix")


def check_release_tracks(path, root, ctx, report):
    for release in root.findall("b:release", NSMAP):
        rid = release.get("id")
        check_track_list(path, release.findall('b:track', NSMAP), report, rid)
        for edition in release.findall('b:edition', NSMAP):
            check_track_list(path, edition.findall('b:track', NSMAP), report, edition.get('id') or rid)
            if edition.findall('b:track', NSMAP) and not edition.get('id'):
                report.error(path, edition.sourceline, f'{rid}: an edition with tracks requires a stable @id')
        for track in release.xpath('b:track | b:edition/b:track', namespaces=NSMAP):
            loc = f"{rid} disc {track.get('disc') or '-'} track {track.get('n')}"
            if not track.get('show'):
                if track.get('covers'):
                    report.error(path, track.sourceline, f'{loc}: covers requires a show reference')
                continue
            show = ctx["shows"].get(track.get("show"))
            if show is None:
                report.error(path, track.sourceline, f"{loc}: unknown show '{track.get('show')}'")
                continue
            for ref in (track.get("covers") or '').split():
                if ref not in show["items"]:
                    report.error(path, track.sourceline, f"{loc}: '{ref}' is not a performance or event in {track.get('show')}")
            if rid not in show["releases"]:
                report.warn(path, track.sourceline, f"{loc}: show {track.get('show')} does not list {rid} in its sources")


def check_track_list(path, tracks, report, label):
    seen = set()
    for track in tracks:
        identity = (track.get('disc') or '1', track.get('n'))
        if identity in seen:
            report.error(path, track.sourceline, f'{label}: duplicate disc/track number {identity}')
        seen.add(identity)
        mapping = track.get('mappingStatus')
        if mapping in ('candidate', 'reviewed') and not track.get('covers'):
            report.error(path, track.sourceline, f'{label}: {mapping} mapping requires covers')
        if mapping == 'not-applicable' and track.get('covers'):
            report.error(path, track.sourceline, f'{label}: non-applicable mapping cannot have covers')
        if not track.get('covers') and not (track.get('title') or track.get('file')):
            report.error(path, track.sourceline, f'{label}: unmapped track requires a source title or filename')


# ---------------------------------------------------------------------------

def validate_memory(files, schema_path, data):
    """Run the same XSD and semantic checks on prospective, unpublished XML."""
    schema = etree.XMLSchema(etree.parse(str(schema_path)))
    report = Report(data)

    def parse(name, expected):
        path = data / name
        try:
            node = etree.fromstring(files['data/' + name])
        except (KeyError, etree.XMLSyntaxError) as exc:
            report.error(path, None, f'missing or malformed generated XML: {exc}')
            return None
        if not schema.validate(node):
            for entry in schema.error_log:
                report.error(path, entry.line, f'schema: {entry.message}')
            return None
        if etree.QName(node).localname != expected:
            report.error(path, 1, f'root element must be <{expected}>')
            return None
        return node

    roots = {}
    for name, kind in REGISTRIES.items():
        node = parse(name, kind)
        if node is not None:
            roots[kind] = (data / name, node)
    ctx = dict(people={}, songs={}, venues=set(), releases=set(), shows={})
    if 'people' in roots:
        ctx['people'] = check_people(*roots['people'], report)
    if 'songs' in roots:
        ctx['songs'] = check_songs(*roots['songs'], ctx['people'], report)
    if 'venues' in roots:
        ctx['venues'] = {v.get('id') for v in roots['venues'][1].findall('b:venue', NSMAP)}
    if 'releases' in roots:
        ctx['releases'] = {r.get('id') for r in roots['releases'][1].findall('b:release', NSMAP)}
    names = sorted(name.removeprefix('data/') for name in files
                   if name.startswith('data/shows/') and name.endswith('.xml'))
    for name in names:
        node = parse(name, 'show')
        if node is not None:
            check_show(data / name, node, ctx, report)
    check_same_day(ctx, report)
    if 'releases' in roots:
        check_release_tracks(*roots['releases'], ctx, report)
    print(f'{len(names)} show file(s), {len(roots)} registries: '
          f'{report.errors} error(s), {report.warnings} warning(s)')
    return report


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description="Validate a BerthaML data directory.")
    parser.add_argument("data", type=Path, help="data directory")
    parser.add_argument("--schema", type=Path, default=here.parent / "schemas" / "berthaml.xsd")
    parser.add_argument("--strict", action="store_true", help="treat warnings as failures")
    args = parser.parse_args()

    data = args.data.resolve()
    if not data.is_dir():
        print(f"not a directory: {data}", file=sys.stderr)
        return 2
    try:
        schema = etree.XMLSchema(etree.parse(str(args.schema)))
    except (OSError, etree.XMLSchemaParseError, etree.XMLSyntaxError) as exc:
        print(f"cannot load schema {args.schema}: {exc}", file=sys.stderr)
        return 2

    report = Report(data)
    roots = {}
    for name, root_name in REGISTRIES.items():
        path = data / name
        if not path.is_file():
            report.error(path, None, "registry file is missing")
            continue
        tree = load(path, schema, report)
        if tree is None:
            continue
        if etree.QName(tree.getroot()).localname != root_name:
            report.error(path, 1, f"root element must be <{root_name}>")
            continue
        roots[root_name] = (path, tree.getroot())

    ctx = {"people": {}, "songs": {}, "venues": set(), "releases": set(), "shows": {}}
    if "people" in roots:
        ctx["people"] = check_people(*roots["people"], report)
    if "songs" in roots:
        ctx["songs"] = check_songs(*roots["songs"], ctx["people"], report)
    if "venues" in roots:
        ctx["venues"] = {v.get("id") for v in roots["venues"][1].findall("b:venue", NSMAP)}
    if "releases" in roots:
        ctx["releases"] = {r.get("id") for r in roots["releases"][1].findall("b:release", NSMAP)}

    show_files = sorted((data / "shows").rglob("*.xml")) if (data / "shows").is_dir() else []
    for path in show_files:
        tree = load(path, schema, report)
        if tree is None:
            continue
        if etree.QName(tree.getroot()).localname != "show":
            report.error(path, 1, "root element must be <show>")
            continue
        check_show(path, tree.getroot(), ctx, report)

    check_same_day(ctx, report)
    if "releases" in roots:
        check_release_tracks(*roots["releases"], ctx, report)

    print(f"{len(show_files)} show file(s), {len(roots)} registr{'y' if len(roots) == 1 else 'ies'}: "
          f"{report.errors} error(s), {report.warnings} warning(s)")
    if report.errors or (args.strict and report.warnings):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
