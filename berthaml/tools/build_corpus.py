"""Build the attributed BerthaML corpus offline; retain uncertainty and edits.

Public Jerrybase event identities define the working roster, not a claim that
its current historical research is final. The supplied catalogue contributes
recording references. Licensed gdshowsdb is retained and reconciled separately.
Hand-curated records in curated/ take precedence over acquired facts.
"""
import argparse
from collections import Counter, defaultdict
import copy
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unicodedata

from lxml import etree

from playlist_data import apply_recording_playlists, export_playlists, recording_date
from validate import validate_memory

ROOT = Path(__file__).resolve().parents[1]
NS = 'urn:berthaml:1'
N = {'b': NS}
UPDATED = '2026-10-30'
COUNTRIES = {'US': 'US', 'Canada': 'CA', 'England': 'GB', 'Scotland': 'GB',
             'Germany': 'DE', 'France': 'FR', 'Denmark': 'DK', 'Netherlands': 'NL',
             'Egypt': 'EG', 'Luxembourg': 'LU', 'Spain': 'ES', 'Jamaica': 'JM', 'Sweden': 'SE'}
KINDS = {'SBD': 'soundboard', 'MTX': 'matrix', 'AUD': 'audience', 'FM': 'fm', 'UNKNOWN': 'unknown'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def norm(value):
    return re.sub('[^a-z0-9]', '', unicodedata.normalize('NFKD', value)
                  .encode('ascii', 'ignore').decode().lower())


def slug(value):
    value = unicodedata.normalize('NFKD', value).encode('ascii', 'ignore').decode().lower()
    return re.sub('[^a-z0-9]+', '-', value).strip('-') or 'untitled'


def el(tag, parent=None, text=None, **attrs):
    node = etree.Element(f'{{{NS}}}{tag}', **{k: str(v) for k, v in attrs.items()})
    if text is not None:
        node.text = text
    if parent is not None:
        parent.append(node)
    return node


def root(tag, **attrs):
    return etree.Element(f'{{{NS}}}{tag}', nsmap={None: NS}, schemaVersion='1.0', **attrs)


def xml(node):
    # Keep curated documents readable while normalizing whitespace for reproducibility.
    node = etree.fromstring(etree.tostring(node), etree.XMLParser(remove_blank_text=True))
    return etree.tostring(node, encoding='UTF-8', xml_declaration=True, pretty_print=True)


def record(parent, status, title, href, scope, accessed=UPDATED):
    rec = el('record', parent, status=status, updated=UPDATED)
    el('cite', rec, title=title, href=href, accessed=accessed, text=scope)
    return rec


def jbytes(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n').encode('utf-8')


def lines(rows):
    return ''.join(json.dumps(r, ensure_ascii=False, sort_keys=True) + '\n' for r in rows).encode('utf-8')


def show_date(label):
    day = label[:10]
    parts = day.split('-')
    if len(parts) != 3:
        raise ValueError(f'Unexpected date label: {label}')
    while parts[-1] == '00':
        parts.pop()
    return '-'.join(parts)


def show_id(event):
    day = show_date(event['date_label']).split('-')
    padded = day + ['xx'] * (3 - len(day))
    # The URL suffix is an opaque stable source identity, never a time-of-day claim.
    return 'gd' + '-'.join(padded) + '-jb' + event['url'].rsplit('-', 1)[1]


def locality(label):
    parts = label.rsplit(', ', 2)
    if len(parts) != 3:
        return label, {}
    name, city, region = parts
    if region in COUNTRIES and region != 'US':
        return name, dict(city=city, country=COUNTRIES[region])
    if re.fullmatch('[A-Z]{2}', region):
        return name, dict(city=city, region=region, country='US')
    raise ValueError(f'Unknown locality format: {label}')


def sequence(row, licensed=False):
    if licensed:
        return tuple(norm(s[':name']) for t in row[':sets'] for s in t[':songs'])
    return tuple(norm(s['title']) for t in row['sections'] for s in t['items'])


def reconcile(licensed, events):
    """Unique factual matches only; never equate /0 with Early or /1 with Late."""
    ga, jb = defaultdict(list), defaultdict(list)
    for row in licensed:
        ga[row['key'][:10].replace('/', '-')].append(row)
    for event in events:
        jb[event['date_label'][:10]].append(event)
    matched, remaining = [], []
    for day in sorted(ga.keys() | jb.keys()):
        left, right = ga[day][:], jb[day][:]
        def same_venue(a, b):
            name, loc = locality(b['venue_label'])
            return norm(a[':venue']) == norm(name) and norm(a[':city']) == norm(loc.get('city', ''))
        rules = [('venue', same_venue), ('song-sequence',
                  lambda a, b: bool(sequence(a, True)) and sequence(a, True) == sequence(b))]
        for method, predicate in rules:
            while True:
                pairs = [(a, b) for a in left for b in right if predicate(a, b)]
                unique = [(a, b) for a, b in pairs
                          if sum(x is a for x, _ in pairs) == 1
                          and sum(y is b for _, y in pairs) == 1]
                if not unique:
                    break
                for a, b in unique:
                    matched.append((a, b, method))
                    left.remove(a)
                    right.remove(b)
        if len(left) == len(right) == 1:
            matched.append((left.pop(), right.pop(), 'date-only-candidate'))
        remaining.append(dict(date=day, gdshowsdb=[r['key'] for r in left],
                              jerrybase=[r['url'] for r in right]))
    return matched, [r for r in remaining if r['gdshowsdb'] or r['jerrybase']]


def recording(item):
    key, provider = item.get('key'), item.get('provider')
    inferred = item.get('inferredType') == 'true'
    node = el('recording', id='r-' + digest((provider + ':' + key).encode())[:20],
              kind='unknown' if inferred else KINDS[item.get('type')])
    el('identifier', node, scheme='archive-org' if provider == 'Archive.org' else 'other',
       text=key)
    # The legacy input combines transfer and mastering, so do not choose a role.
    for credit in item.findall('creditText'):
        if credit.get('role') == 'recording' and credit.text:
            el('credit', node, role='recordist', text=credit.text)
    chain = [item.findtext(tag, '') for tag in ['sourceChain', 'lineage']]
    chain = list(dict.fromkeys(x.strip() for x in chain if x.strip()))
    if chain:
        el('lineage', node, text=' | '.join(chain))
    el('note', node, text=f'Supplied catalogue listing: {provider}; {item.get("title")}. '
       f'Source references: {item.get("sourceRefs")}. Metadata attribution is unreviewed.')
    if item.get('modifiers'):
        el('note', node, text='Source modifiers: ' + item.get('modifiers'))
    if inferred:
        el('note', node, certainty='uncertain', text='Legacy inferred type: ' + item.get('type'))
    for credit in item.findall('creditText'):
        if credit.get('role') != 'recording' and credit.text:
            el('note', node, text=f'Source credit ({credit.get("role")}): {credit.text}')
    for credit in item.findall('person'):
        el('note', node, certainty='uncertain',
           text=f'Legacy name match: {credit.get("name")}; role {credit.get("role")}.')
    for ref in item.findall('reference'):
        if ref.get('url'):
            el('note', node, text='Recording reference: ' + ref.get('url'))
    return node


def make_issue(kind, subject, **detail):
    identity = digest((kind + ':' + subject).encode())[:20]
    value = dict(id=identity, kind=kind, subject=subject, **detail)
    value['evidence_sha256'] = digest(jbytes(value))
    value['state'] = 'open'
    return value


def build():
    lock = json.loads((ROOT / 'sources/inputs.lock.json').read_text('utf-8'))
    for name, expected in lock['files'].items():
        if digest((ROOT / 'sources' / name).read_bytes()) != expected:
            raise ValueError(f'Input hash mismatch: {name}; review and recapture sources explicitly')
    facts = json.loads(gzip.decompress((ROOT / 'sources/jerrybase-facts.json.gz').read_bytes()))
    gd = json.loads(gzip.decompress((ROOT / 'sources/gdshowsdb.json.gz').read_bytes()))
    raw_catalogue = gzip.decompress((ROOT / 'sources/catalogue.xml.gz').read_bytes())
    if digest(raw_catalogue) != lock['supplied_catalogue']['sha256']:
        raise ValueError('Supplied catalogue content hash mismatch')
    catalogue = etree.fromstring(raw_catalogue)
    events = sorted((r for r in facts['events'] if r['category'] == 'Public'), key=lambda e: e['url'])
    if {int(e['date_label'][:4]) for e in events} != set(range(1967, 1996)):
        raise ValueError('Public event capture does not cover every requested year')
    if len({e['url'] for e in events}) != len(events):
        raise ValueError('Duplicate source event URLs')
    if any(not ('1967' <= show_date(e['date_label']) <= '1995-07-09') for e in events):
        raise ValueError('Public event outside the requested era')
    captures = {x['year']: x for x in facts['captures']}
    access = {year: value['fetched_utc'][:10] for year, value in captures.items()}
    # The snapshot UTC date may be the following day; dates of records use Ian's local date.
    curated = ROOT / 'curated'
    regs = {name: etree.parse(str(curated / f'{name}.xml')).getroot()
            for name in ['people', 'songs', 'venues', 'releases']}
    overrides = json.loads((curated / 'identities.json').read_text('utf-8'))
    preserved_ids = json.loads((ROOT / 'sources/legacy-identity-map.json').read_text('utf-8'))['shows']
    for event in events:
        event['id'] = overrides['shows'].get(event['url'], preserved_ids.get(event['url'], show_id(event)))
        event['venue_id'] = overrides['venues'].get(event['venue_url'],
                                                 'v-jb-' + event['venue_url'].rsplit('/', 1)[1])
    song_nodes = {s.get('id'): s for s in regs['songs'].findall('b:song', N)}
    song_names = {}
    for song in song_nodes.values():
        for name in song.findall('b:title', N) + song.findall('b:altTitle', N):
            key = norm(name.text)
            if key in song_names and song_names[key] != song.get('id'):
                raise ValueError('Curated song alias collision: ' + name.text)
            song_names[key] = song.get('id')
    for alias, sid in overrides.get('songs', {}).items():
        if sid not in song_nodes:
            raise ValueError('Alias targets missing curated song: ' + sid)
        song_names[norm(alias)] = sid
    for title in sorted({s['title'] for e in events for t in e['sections'] for s in t['items']}):
        if norm(title) in song_names:
            continue
        sid = 's-' + slug(title)
        if sid in song_nodes:
            sid += '-' + digest(title.encode())[:8]
        song = el('song', regs['songs'], id=sid, kind='unknown')
        el('title', song, text=title)
        el('note', song, text='Title attested in the locked Jerrybase setlist facts. '
           'Authorship, work identity, vocal defaults and classification require review.')
        song_nodes[sid] = song
        song_names[norm(title)] = sid
    cite = el('cite', title='Jerrybase year-table factual capture', href='https://jerrybase.com/events',
              accessed=UPDATED, text='Imported titles only; see sources/jerrybase-facts.json.gz and inputs.lock.json. '
              'Existing seed credits and lead defaults retain their original research status.')
    regs['songs'].find('b:record', N).append(cite)
    venues = {v.get('id'): v for v in regs['venues'].findall('b:venue', N)}
    for event in events:
        vid = event['venue_id']
        name, loc = locality(event['venue_label'])
        if vid not in venues:
            venue = el('venue', regs['venues'], id=vid)
            el('name', venue, text=name)
            el('locality', venue, **loc)
            el('note', venue, text='Event venue identity: ' + event['venue_url'] + '. '
               'Physical-site identity, renamings and coordinates remain unreviewed.')
            venues[vid] = venue
        known = [v.text for v in venues[vid].findall('b:name', N) + venues[vid].findall('b:altName', N)]
        if name not in known:
            # altName precedes locality in the schema.
            node = el('altName', text=name)
            venues[vid].insert(list(venues[vid]).index(venues[vid].find('b:locality', N)), node)
    regs['venues'].find('b:record', N).append(copy.deepcopy(cite))
    matches, unmatched = reconcile(gd, events)
    matched_by_url = {b['url']: (a, method) for a, b, method in matches}
    issues, crosswalk, output = [], [], {}
    for row in unmatched:
        issues.append(make_issue('event-identity', row['date'], **row))
    official_path = ROOT / 'sources/official-release-index.json'
    official = json.loads(official_path.read_text('utf-8')) if official_path.exists() else dict(candidate_urls=[], rows=[])
    release_candidates = []
    for url in official['candidate_urls']:
        associations = [r for r in official['rows'] if r['url'] == url]
        release_candidates.append(dict(url=url, label=associations[0]['release_label'],
                                       recording_years=sorted({r['recording_year'] for r in associations}),
                                       state='pending-edition-research'))
        issues.append(make_issue('official-release-editions', url, label=associations[0]['release_label']))
    by_day = defaultdict(list)
    for event in events:
        by_day[show_date(event['date_label'])].append(event)
    recordings_by_day = {s.get('date'): s.findall('recording')
                         for s in catalogue.findall('./shows/show')}
    unassigned, assigned_keys, xml_shows = [], [], []
    assignment_reasons = {}
    for event in events:
        sid = event['id']
        day = show_date(event['date_label'])
        source_day = access[int(day[:4])]
        override = curated / 'shows' / day[:4] / f'{sid}.xml'
        if override.exists():
            show = etree.parse(str(override)).getroot()
            if show.get('id') != sid or show.get('date') != day:
                raise ValueError(f'Curated identity mismatch: {override}')
        else:
            show = root('show', id=sid, date=day)
            if len(day) < 10:
                show.set('dateCertainty', 'uncertain')
            record(show, 'draft' if event['sections'] else 'skeleton',
                   'Jerrybase event and year-table factual capture', event['url'],
                   f'Working event identity, venue and setlist facts; source year-page SHA-256 '
                   f'{captures[int(day[:4])]["sha256"]}. Imported, not independently verified.', source_day)
            slot = re.search(r'\]\s+(Early|Late|Afternoon|Evening)$', event['date_label'], re.I)
            if slot or len(by_day[day]) > 1:
                el('slot', show, text=slot[1].lower() if slot else 'unknown')
            name, _ = locality(event['venue_label'])
            el('venue', show, ref=event['venue_id'], nameAsBilled=name)
            program = el('program', show, coverage='partial' if event['sections'] else 'none')
            counters = Counter()
            for section in event['sections']:
                label = section['label']
                if 'Soundcheck' in label:
                    kind = 'soundcheck'
                elif 'Encore' in label:
                    kind = 'encore'
                elif 'Set' in label or 'Show' in label:
                    kind = 'set'
                else:
                    kind = 'unknown'
                counters[kind] += 1
                mode = 'acoustic' if 'Acoustic' in label else 'electric' if 'Electric' in label else 'unknown'
                node = el('set', program, kind=kind, n=counters[kind], mode=mode)
                if label == 'Known Songs':
                    node.set('certainty', 'uncertain')
                for i, item in enumerate(section['items'], 1):
                    attrs = dict(id=f'p{len(program):02}-{i:02}', song=song_names[norm(item['title'])],
                                 transition=item['transition'])
                    canonical = song_nodes[attrs['song']].findtext('b:title', namespaces=N)
                    if item['title'] != canonical:
                        attrs['titleVariant'] = item['title']
                    if item.get('mode'):
                        attrs['mode'] = item['mode']
                    el('performance', node, **attrs)
                el('note', node, text='Source section label: ' + label)
                if label == 'Known Songs':
                    el('note', node, text='Source identifies known songs; their performance order is unconfirmed.')
            performances = program.findall('.//b:performance', N)
            if performances and performances[-1].get('transition') == 'segue':
                performances[-1].set('transition', 'unknown')
                el('note', performances[-1], text='Source ends the known program with a segue marker; '
                   'the destination is not identified. Marker retained in the factual snapshot.')
                issues.append(make_issue('dangling-segue', sid, source=event['url']))
            el('note', show, text='Imported factual record. No membership-derived lineup, vocalists, '
               'reprises, track durations or release mappings are inferred. Program completeness '
               'requires review; an absent segue marker does not prove a stop.')
        rec = show.find('b:record', N)
        if not any(c.get('href') == event['url'] for c in rec.findall('b:cite', N)):
            el('cite', rec, title='Jerrybase event identity', href=event['url'], accessed=source_day,
               text='Current comparison snapshot; original curated facts retain their citations.')
        legacy = matched_by_url.get(event['url'])
        entry = dict(show=sid, date=day, jerrybase=event['url'],
                     source_show_id=event['source_show_id'], venue_source=event['venue_url'])
        if legacy:
            row, method = legacy
            entry.update(gdshowsdb_key=row['key'], gdshowsdb_uuid=row[':uuid'], match_method=method,
                         sequence_agrees=sequence(row, True) == sequence(event))
            if not entry['sequence_agrees']:
                issues.append(make_issue('setlist-disagreement', sid,
                                         gdshowsdb_key=row['key'], jerrybase=event['url'],
                                         gdshowsdb_program_sha256=digest(jbytes(row[':sets'])),
                                         jerrybase_program_sha256=digest(jbytes(event['sections']))))
            old_name, old_loc = row[':venue'], dict(city=row[':city'], country=COUNTRIES[row[':country']])
            new_name, new_loc = locality(event['venue_label'])
            if norm(old_name) != norm(new_name) or any(old_loc[k] != new_loc.get(k) for k in old_loc):
                issues.append(make_issue('venue-disagreement', sid,
                                         gdshowsdb=dict(name=old_name, **old_loc),
                                         jerrybase=dict(name=new_name, **new_loc), match_method=method))
        crosswalk.append(entry)
        refs = recordings_by_day.get(day, [])
        if len(by_day[day]) == 1 and len(day) == 10:
            if refs:
                sources = show.find('b:sources', N)
                if sources is None:
                    sources = el('sources')
                    show.insert(list(show).index(show.find('b:program', N)) + 1, sources)
                present = {i.text for i in sources.findall('b:recording/b:identifier', N)}
                for item in refs:
                    actual_day = recording_date(item.get('provider'), item.get('key'), item.get('title'))
                    if actual_day != day:
                        assignment_reasons[(item.get('provider'), item.get('key'))] = (
                            'identifier-date-unknown-or-out-of-scope' if actual_day is None
                            else 'identifier-date-disagrees-with-catalogue-group')
                        continue
                    if item.get('key') not in present:
                        position = next((i for i, e in enumerate(sources) if e.tag == f'{{{NS}}}release'), len(sources))
                        sources.insert(position, recording(item))
                    assigned_keys.append((item.get('provider'), item.get('key')))
                el('cite', rec, title='Ian supplied Gratitude catalogue, 2026-10-29 snapshot',
                   href='urn:sha256:' + lock['supplied_catalogue']['sha256'], accessed=UPDATED,
                   text='Recording references associated by sole public event on this date only; '
                   'individual metadata dates and provenance remain unreviewed. Input SHA-256 '
                   + lock['supplied_catalogue']['sha256'])
        playlist_issues = apply_recording_playlists(show, ROOT, song_nodes)
        issues.extend(make_issue(kind, subject, **details) for kind, subject, details in playlist_issues)
        missing = []
        for tag, label in [('lineup', 'personnel'), ('bill', 'billing')]:
            if show.find('b:' + tag, N) is None:
                missing.append(label)
        if show.find('b:program', N).get('coverage') != 'complete':
            missing.append('program-completeness')
        missing.extend(['performance-vocals-instruments-reprises', 'recording-attribution', 'official-releases'])
        if len(day) < 10:
            missing.append('date-precision')
        issues.append(make_issue('show-review', sid, record_status=rec.get('status'),
                                 fields=missing, source=event['url'], show_sha256=digest(xml(show))))
        if event['unmodeled_contexts']:
            issues.append(make_issue('context-review', sid, count=event['unmodeled_contexts'],
                                     source=event['url'], explanation='Source context may describe music, '
                                     'guests or another act. It was not converted into asserted stage facts.'))
        path = f'data/shows/{day[:4]}/{sid}.xml'
        output[path] = xml(show)
        xml_shows.append(show)
    assigned_set = set(assigned_keys)
    total_recordings = 0
    for date, items in recordings_by_day.items():
        for item in items:
            total_recordings += 1
            if (item.get('provider'), item.get('key')) not in assigned_set:
                unassigned.append(dict(date=date, provider=item.get('provider'), key=item.get('key'),
                                       reason=assignment_reasons.get((item.get('provider'), item.get('key')),
                                           'multiple-public-events' if len(by_day.get(date, [])) > 1
                                           else 'no-exact-public-event'),
                                       candidate_shows=[e['id'] for e in by_day.get(date, [])]))
    for day in sorted({x['date'] for x in unassigned}):
        issues.append(make_issue('recording-date-assignment', day,
                                 count=sum(x['date'] == day for x in unassigned)))
    for name, node in regs.items():
        output[f'data/{name}.xml'] = xml(node)
        if node.find('b:record', N).get('status') not in ('reviewed', 'verified'):
            issues.append(make_issue('registry-review', name, content_sha256=digest(xml(node))))
    playlist_outputs, playlist_stats = export_playlists(xml_shows, regs)
    scope = json.loads((curated / 'research-scope.json').read_text('utf-8'))
    inventory_complete = scope.get('official_release_inventory_complete', False)
    if inventory_complete and (not scope.get('reviewed') or not scope.get('citations')):
        raise ValueError('A complete official edition inventory requires a review date and citations')
    playlist_stats['official_release_inventory_complete'] = inventory_complete
    output.update(playlist_outputs)
    resolutions = json.loads((curated / 'resolutions.json').read_text('utf-8'))
    issue_map = {i['id']: i for i in issues}
    if len(issue_map) != len(issues):
        raise ValueError('Duplicate issue identity')
    for resolution in resolutions:
        issue = issue_map.get(resolution['id'])
        if not issue or issue['evidence_sha256'] != resolution['evidence_sha256']:
            raise ValueError('Stale or unknown resolution: ' + resolution['id'])
        if resolution.get('state') not in ('resolved', 'accepted-unknown', 'excluded'):
            raise ValueError('Unexpected resolution state')
        if not resolution.get('citations') or not resolution.get('reason') or not resolution.get('reviewed'):
            raise ValueError('Resolutions require citations, reason and review date')
        issue.update(state=resolution['state'], resolution=resolution)
    complete = (not any(i['state'] == 'open' for i in issues) and inventory_complete
                and not playlist_stats['unreviewed_track_mappings']
                and all(s.find('b:record', N).get('status') in ('reviewed', 'verified') for s in xml_shows)
                and all(r.find('b:record', N).get('status') in ('reviewed', 'verified') for r in regs.values()))
    stats = dict(version=1, scope=lock['scope'], updated=UPDATED,
                 completion='complete' if complete else 'incomplete', source_events=len(facts['events']), shows=len(events),
                 show_status=dict(sorted(Counter(s.find('b:record', N).get('status') for s in xml_shows).items())),
                 program_coverage=dict(sorted(Counter(s.find('b:program', N).get('coverage') for s in xml_shows).items())),
                 performances=sum(len(s.findall('.//b:performance', N)) for s in xml_shows),
                 songs=len(song_nodes), venues=len(venues), people=len(regs['people'].findall('b:person', N)),
                 official_releases=len(regs['releases'].findall('b:release', N)),
                 supplied_recordings=total_recordings, assigned_recording_references=len(assigned_keys),
                 unassigned_recording_references=len(unassigned),
                 legacy_unplaced_records=len(catalogue.findall('./unplacedRecords/record')),
                 gdshowsdb_records=len(gd), source_matches=len(matches),
                 issues=dict(sorted(Counter(i['kind'] for i in issues if i['state'] == 'open').items())),
                 open_issues=sum(i['state'] == 'open' for i in issues),
                 source_exclusions=dict(Counter(r['category'] for r in facts['events'] if r['category'] != 'Public')),
                 playlists=playlist_stats,
                 official_release_page_candidates=len(release_candidates),
                 years={year: sum(show_date(e['date_label']).startswith(str(year)) for e in events)
                        for year in range(1967, 1996)})
    output['reports/coverage.json'] = jbytes(stats)
    output['reports/official-release-candidates.jsonl'] = lines(release_candidates)
    output['reports/review-queue.jsonl'] = lines(sorted(issues, key=lambda i: (i['kind'], i['subject'])))
    output['reports/source-crosswalk.jsonl'] = lines(crosswalk)
    output['reports/recordings-unassigned.jsonl'] = lines(unassigned)
    output['reports/event-exclusions.jsonl'] = lines([dict(url=e['url'], date_label=e['date_label'],
                                                         category=e['category'], venue=e['venue_label'])
                                                     for e in facts['events'] if e['category'] != 'Public'])
    md = ['# Corpus coverage', '', f'Updated {UPDATED}. **Curation is incomplete.**', '',
          'The working roster is the public GD event list in the pinned Jerrybase capture. '
          'It includes public broadcasts and dates known only to the month. Canceled events, '
          'private sessions and recording-only dates are kept as evidence outside that roster.', '',
          '| Measure | Count |', '| --- | ---: |']
    for key in ['shows', 'performances', 'songs', 'venues', 'people', 'official_releases',
                'assigned_recording_references', 'unassigned_recording_references', 'open_issues']:
        md.append(f'| {key.replace("_", " ")} | {stats[key]:,} |')
    md.extend(['', '## Review status', '',
               *[f'- {k}: {v:,}' for k, v in stats['show_status'].items()], '',
               'Schema validity and source agreement do not constitute historical verification. '
               'The two show sources may share upstream research. Most personnel, guest appearances, '
               'vocalists, instruments, song authorship, release mappings and durations still need research. '
               'No duration is inferred from a show or setlist.', '', '## Open work', '',
               *[f'- {k}: {v:,}' for k, v in stats['issues'].items()], '',
               'Machine-readable evidence: coverage.json, review-queue.jsonl, source-crosswalk.jsonl, '
               'recordings-unassigned.jsonl and event-exclusions.jsonl in this directory.', '',
               '## Shows by year', '', '| Year | Events |', '| --- | ---: |',
               *[f'| {y} | {n} |' for y, n in stats['years'].items()], ''])
    md.extend(['## Edition-specific playlists', '',
               *[f'- {key.replace("_", " ")}: {value}' for key, value in playlist_stats.items()], '',
               f'Official release-page candidates: {len(release_candidates)}. This discovery index is not '
               'an exhaustive physical/digital edition inventory.', '',
               'TXT and JSON companion files are under playlists/. M3U8 files are emitted only when '
               'actual relative source filenames are known; copy the M3U8 alongside the matching source audio. '
               'No audio is acquired. Candidate performance mappings require review.', ''])
    output['reports/coverage.md'] = '\n'.join(md).encode('utf-8')
    return output, stats


def publish(output, check=False, destination=None):
    destination = (destination or ROOT).resolve()
    manifest_path = destination / 'corpus-manifest.json'
    previous = json.loads(manifest_path.read_text('utf-8'))['files'] if manifest_path.exists() else {}
    for path in sorted(set(previous) | set(output)):
        target = destination / path
        if not target.resolve().is_relative_to(destination):
            raise ValueError('Manifest path escapes the corpus directory: ' + path)
        current = target.read_bytes() if target.exists() else None
        if check:
            if current != output.get(path):
                raise ValueError('Rebuild differs from working corpus: ' + path)
            continue
        if path in previous:
            if current is None or digest(current) != previous[path]:
                if current != output.get(path):
                    raise ValueError('Preserve the local edit in curated/ before rebuilding: ' + path)
        elif current is not None and current != output.get(path):
            # Initial publication may expand the seed data only if the original is preserved.
            seed = ROOT / 'curated' / path.removeprefix('data/')
            if not path.startswith('data/') or not seed.exists() or seed.read_bytes() != current:
                raise ValueError('Refuse to overwrite an unmanaged file: ' + path)
    planned_xml = {p for p in output if p.startswith('data/') and p.endswith('.xml')}
    existing_xml = {p.relative_to(destination).as_posix() for p in (destination / 'data').rglob('*.xml')}
    if existing_xml - planned_xml - set(previous):
        raise ValueError('Unmanaged XML in data/: ' + ', '.join(sorted(existing_xml - planned_xml - set(previous))))
    manifest = jbytes(dict(version=1, files={p: digest(raw) for p, raw in sorted(output.items())}))
    if check:
        if not manifest_path.exists() or manifest_path.read_bytes() != manifest:
            raise ValueError('Corpus manifest differs')
        return
    for path, raw in output.items():
        target = destination / path
        if target.exists() and target.read_bytes() == raw:
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = target.with_suffix(target.suffix + '.new')
        if temporary.exists():
            raise ValueError('Unmanaged temporary file exists: ' + str(temporary))
        temporary.write_bytes(raw)
        os.replace(temporary, target)
    for path in set(previous) - set(output):
        (destination / path).unlink(missing_ok=True)  # Confinement and prior owned-file hash checked above.
    temporary = manifest_path.with_suffix('.json.new')
    temporary.write_bytes(manifest)
    os.replace(temporary, manifest_path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='rebuild offline and compare without changing files')
    parser.add_argument('--output', type=Path, default=ROOT,
                        help='corpus output directory (default: berthaml)')
    args = parser.parse_args()
    output, stats = build()
    # Validate all prospective data before publishing, without temporary copies of every XML file.
    report = validate_memory(output, ROOT / 'schemas/berthaml.xsd', args.output.resolve() / 'data')
    if report.errors or report.warnings:
        raise ValueError('Prospective corpus failed strict validation; published data is unchanged')
    publish(output, args.check, args.output.resolve())
    print(f'{stats["shows"]} shows; {stats["performances"]} performances; {stats["open_issues"]} open issues. '
          + ('Offline rebuild matches.' if args.check else 'Corpus published locally.'))


if __name__ == '__main__':
    main()
