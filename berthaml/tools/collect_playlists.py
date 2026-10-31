"""Resume metadata-only acquisition of original-file recording playlists.

No audio is downloaded. Failed items remain retryable, completed items are
reused, and pinned canonical XML is never modified by acquisition.
"""
import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import json
from pathlib import Path
import re
import time
from urllib.parse import quote
from urllib.request import Request, urlopen

from playlist_data import cache_name, duration

ROOT = Path(__file__).resolve().parents[1]
AUDIO = {'.flac', '.shn', '.wav', '.aif', '.aiff', '.mp3', '.ogg', '.opus', '.m4a'}


def natural(value):
    return tuple((0, int(part)) if part.isdigit() else (1, part.casefold())
                 for part in re.split(r'(\d+)', value))


def extract(identifier, raw, captured):
    data = json.loads(raw)
    metadata = data.get('metadata', {})
    if metadata.get('identifier') != identifier or 'files' not in data:
        raise ValueError('API did not return the requested recording')
    by_format = {}
    for item in data['files']:
        filename = item.get('name', '')
        extension = Path(filename).suffix.lower()
        if item.get('source') != 'original' or extension not in AUDIO:
            continue
        match = re.search(r'(?i)(?:d|cd)(\d+)[._ -]?t(\d+)', filename)
        disc, number = (int(match[1]), int(match[2])) if match else (None, None)
        if number is None:
            tag = str(item.get('track', ''))
            match = re.fullmatch(r'0*(\d+)(?:/\d+)?', tag)
            number = int(match[1]) if match else None
        if not number:
            number = None
        title = str(item.get('title') or Path(filename).stem).strip()
        track = dict(file=filename, title=title)
        if number is not None:
            track['sourceNumber'] = number
        if disc is not None:
            track['disc'] = disc
        measured = duration(item.get('length'))
        if measured:
            track['duration'] = measured
        if item.get('md5'):
            track['md5'] = item['md5']
        by_format.setdefault(extension[1:], []).append(track)
    variants = []
    for fmt, tracks in sorted(by_format.items()):
        numbered = all(t.get('sourceNumber') for t in tracks)
        unique = len({(t.get('disc', 1), t.get('sourceNumber')) for t in tracks}) == len(tracks)
        certain_order = numbered and unique
        tracks.sort(key=lambda t: (t.get('disc', 1), t['sourceNumber']) if certain_order else natural(t['file']))
        variants.append(dict(format=fmt, tracks=tracks, order_uncertain=not certain_order,
                             order_basis='explicit disc/track numbers' if certain_order else 'natural filename order'))
    credits = {k: metadata[k] for k in ['source', 'lineage', 'taper', 'transferer'] if metadata.get(k)}
    return dict(identifier=identifier, url='https://archive.org/metadata/' + quote(identifier, safe=''),
                captured_utc=captured, response_sha256=hashlib.sha256(raw).hexdigest(),
                title=metadata.get('title'), variants=variants, credits=credits)


def collect(identifiers, limit, fetch, delay, refresh=False):
    directory = ROOT / 'sources/recording-playlists'
    directory.mkdir(parents=True, exist_ok=True)
    index_path = directory / 'index.json'
    index = json.loads(index_path.read_text('utf-8')) if index_path.exists() else {}
    pending = [i for i in identifiers if refresh or index.get(i, {}).get('status') != 'captured']
    if not fetch:
        print(f'{len(identifiers)} requested identifiers; {len(pending)} pending; use --fetch to acquire up to {limit}.')
        return 0
    work = ROOT.parent / 'work/berthaml-playlist-metadata'
    work.mkdir(parents=True, exist_ok=True)
    failures = 0
    for identifier in pending[:limit]:
        url = 'https://archive.org/metadata/' + quote(identifier, safe='')
        try:
            raw_cache = work / (cache_name(identifier).replace('.json.gz', '.json'))
            if raw_cache.exists() and not refresh:
                raw = raw_cache.read_bytes()
                captured = datetime.fromtimestamp(raw_cache.stat().st_mtime, timezone.utc).isoformat()
            else:
                last = None
                for attempt in range(3):
                    try:
                        raw = urlopen(Request(url, headers={'User-Agent':
                            'Gratitude-BerthaML/1.0 (historical recording playlist metadata; no audio)'}), timeout=45).read()
                        last = None
                        break
                    except Exception as exc:
                        last = exc
                        if attempt < 2:
                            time.sleep(2 ** (attempt + 1))
                if last:
                    raise last
                captured = datetime.now(timezone.utc).isoformat()
                extract(identifier, raw, captured)  # Never cache an API error as successful metadata.
                raw_cache.write_bytes(raw)
            fact = extract(identifier, raw, captured)
            payload = json.dumps(fact, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')
            content = gzip.compress(payload, mtime=0)
            path = directory / cache_name(identifier)
            path.write_bytes(content)
            count = sum(len(v['tracks']) for v in fact['variants'])
            index[identifier] = dict(status='captured', sha256=hashlib.sha256(content).hexdigest(),
                                     tracks=count, variants=len(fact['variants']), captured_utc=captured)
            print(f'{identifier}: {count} tracks in {len(fact["variants"])} original-file variant(s)', flush=True)
        except Exception as exc:
            failures += 1
            index[identifier] = dict(status='failed', error=str(exc), url=url)
            print(f'{identifier}: FAILED {exc}', flush=True)
        temporary = index_path.with_suffix('.json.new')
        temporary.write_text(json.dumps(index, ensure_ascii=False, indent=2, sort_keys=True) + '\n', encoding='utf-8', newline='\n')
        temporary.replace(index_path)
        time.sleep(delay)
    print(f'Captured {sum(v["status"] == "captured" for v in index.values())} identifiers; '
          f'{failures} failures in this batch. Sources remain unchanged until an explicit corpus rebuild.')
    return 1 if failures else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--identifier', action='append', help='specific Archive recording identifier; repeatable')
    parser.add_argument('--limit', type=int, default=25)
    parser.add_argument('--fetch', action='store_true')
    parser.add_argument('--refresh', action='store_true', help='reacquire instead of reusing captured metadata')
    parser.add_argument('--delay', type=float, default=1.5)
    args = parser.parse_args()
    if args.limit < 1 or args.delay < 1:
        parser.error('limit must be positive and delay must be at least one second')
    if args.identifier:
        identifiers = list(dict.fromkeys(args.identifier))
    else:
        # The supplied factual snapshot retains every source key, including unassigned dates.
        import lxml.etree as etree
        catalogue = etree.fromstring(gzip.decompress((ROOT / 'sources/catalogue.xml.gz').read_bytes()))
        identifiers = sorted({r.get('key') for r in catalogue.findall('./shows/show/recording')
                              if r.get('provider') == 'Archive.org'})
    return collect(identifiers, args.limit, args.fetch, args.delay, args.refresh)


if __name__ == '__main__':
    raise SystemExit(main())
