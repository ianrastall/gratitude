"""Source-specific track facts, conservative links, and companion-file exports."""
from collections import Counter
from datetime import date
from decimal import Decimal, InvalidOperation
import gzip
import hashlib
import html
import json
from pathlib import PurePosixPath
import re
import unicodedata

from lxml import etree

NS = 'urn:berthaml:1'
N = {'b': NS}


def norm(value):
    return re.sub('[^a-z0-9]', '', unicodedata.normalize('NFKD', value or '')
                  .encode('ascii', 'ignore').decode().lower())


def plain(value):
    if isinstance(value, list):
        return '; '.join(plain(item) for item in value)
    text = html.unescape(str(value))
    text = re.sub(r'(?i)<br\s*/?>', '\n', text)
    return re.sub(r'(?i)</?(?:p|div|span|a|b|i|strong|em)\b[^>]*>', '', text)


def recording_date(provider, key, title):
    value = key if provider == 'Archive.org' else title
    patterns = [r'(?i)gd((?:19)?\d{2})[-_.](\d{2})[-_.](\d{2})']
    if provider != 'Archive.org':
        patterns.append(r'(?<!\d)(19\d{2})-(\d{2})-(\d{2})(?!\d)')
    found = set()
    for pattern in patterns:
        for match in re.finditer(pattern, value or ''):
            y, m, d = map(int, match.groups())
            y = y + 1900 if y < 100 else y
            try:
                day = date(y, m, d).isoformat()
            except ValueError:
                continue
            if '1967-01-01' <= day <= '1995-07-09':
                found.add(day)
    return next(iter(found)) if len(found) == 1 else None


def duration(value):
    """Metadata measurements only. Preserve fractions; never infer missing timing."""
    if value is None or str(value).strip() == '':
        return None
    try:
        parts = str(value).strip().split(':')
        if len(parts) > 3:
            return None
        seconds = Decimal(0)
        for i, part in enumerate(parts):
            component = Decimal(part)
            if not component.is_finite() or component < 0:
                return None
            if len(parts) > 1 and i > 0 and component >= 60:
                return None
            if i < len(parts) - 1 and component != int(component):
                return None
            seconds = seconds * 60 + component
        if not seconds.is_finite() or seconds < 0:
            return None
        value = format(seconds, 'f').rstrip('0').rstrip('.') if '.' in format(seconds, 'f') else str(seconds)
        return 'PT' + (value or '0') + 'S'
    except InvalidOperation:
        return None


def seconds(value):
    if not value:
        return None
    match = re.fullmatch(r'PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?', value)
    if not match:
        raise ValueError('Invalid track duration: ' + value)
    h, m, s = match.groups()
    return Decimal(h or 0) * 3600 + Decimal(m or 0) * 60 + Decimal(s or 0)


def timing(value):
    total = seconds(value)
    if total is None:
        return '--:--'
    whole = int(total)
    return f'{whole // 60}:{whole % 60:02}'


def cache_name(identifier):
    return hashlib.sha256(identifier.encode()).hexdigest() + '.json.gz'


def candidate_covers(title, show, songs):
    pieces = [norm(x) for x in re.split(r'\s*(?:>| / )\s*', title or '') if norm(x)]
    performances = show.findall('.//b:performance', N)
    names = []
    for item in performances:
        song = songs[item.get('song')]
        names.append({norm(item.get('titleVariant')), *[norm(n.text) for n in
                      song.findall('b:title', N) + song.findall('b:altTitle', N)]} - {''})
    starts = [i for i in range(len(names) - len(pieces) + 1)
              if pieces and all(piece in names[i + j] for j, piece in enumerate(pieces))]
    if len(starts) != 1:
        return []
    start = starts[0]
    return [p.get('id') for p in performances[start:start + len(pieces)]]


def sub(parent, tag, text=None, **attrs):
    node = etree.SubElement(parent, f'{{{NS}}}{tag}', **{k: str(v) for k, v in attrs.items()})
    node.text = text
    return node


def apply_recording_playlists(document, show, root, songs, manifest):
    issues, missing, imported, unmapped = [], [], 0, 0
    for recording in document.findall('b:recording', N):
        identifier = recording.findtext('b:identifier[@scheme="archive-org"]', namespaces=N)
        if not identifier:
            if not recording.findall('b:track', N) and not recording.findall('b:playlist', N):
                missing.append(recording.findtext('b:identifier', namespaces=N) or recording.get('id'))
            continue
        entry = manifest.get(identifier)
        if not entry or entry.get('status') != 'captured':
            missing.append(identifier)
            continue
        path = root / 'sources/recording-playlists' / cache_name(identifier)
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != entry['sha256']:
            raise ValueError('Playlist source hash mismatch: ' + identifier)
        fact = json.loads(gzip.decompress(raw))
        if fact['identifier'] != identifier:
            raise ValueError('Playlist source identity mismatch')
        if not fact['variants']:
            missing.append(identifier)
            continue
        for variant in fact['variants']:
            pid = 'pl-' + hashlib.sha256((identifier + ':' + variant['format']).encode()).hexdigest()[:20]
            if recording.find(f'b:playlist[@id="{pid}"]', N) is not None:
                continue  # A curated playlist overrides its acquired version.
            playlist = etree.Element(f'{{{NS}}}playlist', id=pid,
                                     label='Archive original ' + variant['format'].upper() + ' files',
                                     format=variant['format'], sourceHref=fact['url'])
            for i, item in enumerate(variant['tracks'], 1):
                attrs = dict(n=i, title=item['title'], file=item['file'], mappingStatus='unreviewed')
                for name in ['disc', 'sourceNumber', 'duration']:
                    if item.get(name) is not None:
                        attrs[name] = item[name]
                candidates = candidate_covers(item['title'], show, songs)
                if candidates:
                    attrs.update(covers=' '.join(candidates), mappingStatus='candidate', certainty='probable')
                else:
                    unmapped += 1
                sub(playlist, 'track', **attrs)
            sub(playlist, 'note', text='Captured original-file metadata, not an auditioned track list. '
                'No derivative MP3 files are mixed into the original source. '
                'Order basis: ' + variant['order_basis'] + '. Performance links are review candidates.')
            if variant.get('order_uncertain'):
                issues.append(('playlist-order', pid, dict(source=fact['url'])))
            position = next((i for i, child in enumerate(recording) if child.tag == f'{{{NS}}}note'), len(recording))
            recording.insert(position, playlist)
            imported += 1
        for name, value in fact.get('credits', {}).items():
            sub(recording, 'note', text=f'Archive source metadata ({name}): {plain(value)}')
        rec = document.find('b:record', N)
        if not any(c.get('href') == fact['url'] for c in rec.findall('b:cite', N)):
            sub(rec, 'cite', title='Archive original-file playlist metadata', href=fact['url'],
                accessed=fact['captured_utc'][:10], text='Track filenames, supplied titles and measured durations. '
                'Snapshot SHA-256 ' + entry['sha256'])
    if missing:
        issues.append(('recording-playlists-missing', show.get('id'), dict(recordings=len(missing),
                                                                         identifiers=sorted(missing))))
    if unmapped:
        issues.append(('playlist-mappings', show.get('id'), dict(unmapped_tracks=unmapped)))
    if imported:
        issues.append(('playlist-review', show.get('id'), dict(acquired_playlists=imported)))
    return issues


def safe_file(value):
    if not value or any(c in value for c in '\r\n\x00\\:'):
        return False
    path = PurePosixPath(value)
    return not path.is_absolute() and '..' not in path.parts and not value.startswith('#')


def export_playlists(shows, documents, regs):
    outputs, inventory, published = {}, [], []
    titles = {s.get('id'): s.findtext('b:title', namespaces=N) for s in regs['songs'].findall('b:song', N)}
    venue_names = {v.get('id'): v.findtext('b:name', namespaces=N) for v in regs['venues'].findall('b:venue', N)}
    show_map = {s.get('id'): s for s in shows}

    def track_data(track, default_show=None):
        value = dict(track.attrib)
        sid = value.get('show', default_show)
        value['show'] = sid
        value['n'] = int(value['n'])
        for name in ['disc', 'sourceNumber']:
            if name in value:
                value[name] = int(value[name])
        refs = (value.pop('covers', '')).split()
        value['covers'] = refs
        value['mappingStatus'] = value.get('mappingStatus', 'unreviewed')
        if not value.get('title'):
            pieces = []
            for ref in refs:
                node = show_map[sid].find(f'.//*[@id="{ref}"]')
                if node is not None:
                    pieces.append(node.get('titleVariant') or titles.get(node.get('song'))
                                  or (node.text or node.get('kind') or 'Unidentified event'))
            value['title'] = ' > '.join(pieces) if pieces else 'Unidentified track'
        return value

    def emit(kind, identity, label, tracks, citations, notes, review_status='draft', **context):
        notes = [plain(note) for note in notes]
        value = dict(schemaVersion='1.0', id=identity, kind=kind, title=label,
                     status=review_status, tracks=tracks, citations=citations, notes=notes, **context)
        target = 'playlists/' + kind + '/' + identity
        outputs[target + '.json'] = (json.dumps(value, ensure_ascii=False, indent=2) + '\n').encode('utf-8')
        text = [label, 'Artist: Grateful Dead', 'Review status: ' + review_status, 'Playlist: ' + identity]
        for field in ['date', 'venue', 'identifier', 'format', 'edition']:
            if context.get(field):
                text.append(field.capitalize() + ': ' + str(context[field]))
        text.extend(['', 'Tracks'])
        for t in tracks:
            number = f'{t.get("disc", 1):02}.{t.get("sourceNumber", t["n"]):02}'
            mapping = t.get('mappingStatus', 'unreviewed')
            if t.get('certainty') and t.get('certainty') != 'certain':
                mapping += ', ' + t['certainty']
            text.append(f'{number}  {timing(t.get("duration"))}  {t["title"]}  [{mapping}]')
            if t.get('file'):
                text.append('    File: ' + t['file'])
            if t.get('covers'):
                text.append('    Show: ' + t['show'] + '; performances: ' + ' '.join(t['covers']))
        if notes:
            text.extend(['', 'Source notes and credits', *notes])
        text.extend(['', 'Sources', *[c['href'] + ' — ' + c.get('title', '') for c in citations if c.get('href')], ''])
        outputs[target + '.txt'] = '\n'.join(text).encode('utf-8')
        playable = bool(tracks) and all(safe_file(t.get('file')) for t in tracks)
        if playable:
            m3u = ['#EXTM3U']
            for t in tracks:
                length = seconds(t.get('duration'))
                m3u.extend([f'#EXTINF:{int(length) if length is not None else -1},'
                            + t['title'].replace('\n', ' ').replace('\r', ' '), t['file']])
            outputs[target + '.m3u8'] = ('\n'.join(m3u) + '\n').encode('utf-8')
        published.append(dict(id=identity, kind=kind, tracks=len(tracks), playable_m3u=playable))

    for show in shows:
        document = documents.get(show.get('id'))
        if document is None:
            continue
        venue = show.find('b:venue', N)
        citations = [dict(c.attrib) for c in (show.findall('b:record/b:cite', N)
                                              + document.findall('b:record/b:cite', N))]
        for rec in document.findall('b:recording', N):
            identifier = rec.findtext('b:identifier', namespaces=N)
            variants = [(p.get('id'), p.get('format'), p.findall('b:track', N)) for p in rec.findall('b:playlist', N)]
            if rec.findall('b:track', N):
                variants.append((rec.get('id'), None, rec.findall('b:track', N)))
            inventory.append(dict(kind='recording', show=show.get('id'), recording=rec.get('id'),
                                  identifier=identifier, playlists=len(variants),
                                  status='acquired-unreviewed' if variants else 'missing'))
            for pid, fmt, tracks in variants:
                notes = [n.text or '' for n in rec.findall('b:note', N)]
                if rec.findtext('b:lineage', namespaces=N):
                    notes.insert(0, 'Lineage: ' + rec.findtext('b:lineage', namespaces=N))
                notes.extend(f'{c.get("role")}: {c.text}' for c in rec.findall('b:credit', N))
                emit('recording', pid, f'Grateful Dead — {show.get("date")} — {identifier}',
                     [track_data(t, show.get('id')) for t in tracks], citations, notes,
                     date=show.get('date'), venue=venue_names[venue.get('ref')], identifier=identifier,
                     format=fmt, show=show.get('id'), recording=rec.get('id'),
                     review_status=document.find('b:record', N).get('status'))
    for release in regs['releases'].findall('b:release', N):
        rid = release.get('id')
        title = release.findtext('b:title', namespaces=N)
        release_status = regs['releases'].find('b:record', N).get('status')
        citations = [dict(c.attrib) for c in regs['releases'].findall('b:record/b:cite', N)]
        notes = [n.text or '' for n in release.findall('b:note', N)]
        for edition in release.findall('b:edition', N):
            eid = edition.get('id') or f'{rid}-{edition.get("date")}-{edition.get("format")}'
            tracks = edition.findall('b:track', N)
            inventory.append(dict(kind='official', release=rid, edition=eid,
                                  status='acquired-unreviewed' if tracks else 'missing'))
            if tracks:
                emit('official', eid, title, [track_data(t) for t in tracks], citations, notes,
                     release=rid, edition=eid, format=edition.get('format'), date=edition.get('date'),
                     review_status=release_status)
        tracks = release.findall('b:track', N)
        if tracks:
            emit('official', rid + '-reference', title, [track_data(t) for t in tracks], citations,
                 notes + ['Reference track list has not been assigned to a specific physical/digital edition.'],
                 release=rid, review_status=release_status)
    outputs['reports/playlist-coverage.jsonl'] = ''.join(json.dumps(x, ensure_ascii=False, sort_keys=True) + '\n'
                                                       for x in inventory).encode('utf-8')
    all_tracks = ([t for document in documents.values() for t in document.findall('.//b:track', N)]
                  + regs['releases'].findall('.//b:track', N))
    stats = dict(exported=len(published), tracks=sum(x['tracks'] for x in published),
                 m3u_files=sum(x['playable_m3u'] for x in published),
                 official_playlists=sum(x['kind'] == 'official' for x in published),
                 circulating_playlists=sum(x['kind'] == 'recording' for x in published),
                 recording_editions_missing=sum(x['kind'] == 'recording' and x['status'] == 'missing' for x in inventory),
                 known_official_editions_missing=sum(x['kind'] == 'official' and x['status'] == 'missing' for x in inventory),
                 unreviewed_track_mappings=sum(t.get('mappingStatus') not in ('reviewed', 'unknown', 'not-applicable')
                                              for t in all_tracks),
                 official_release_inventory_complete=False)
    return outputs, stats
