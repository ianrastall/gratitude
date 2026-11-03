"""Capture Jerrybase's Grateful Dead tour groupings: each tour's name, dates and events.

Jerrybase says its tours sometimes come from band documents and sometimes from its own
grouping of shows, so a tour is a source's grouping, not a fact the corpus asserts.
One cached request for the tour list and one per tour, spaced serially; offline reruns
use only the cache. Only names, date ranges and event links are kept.

    python tools/collect_tours.py --fetch
"""
import argparse
from datetime import datetime, timezone
import gzip
import hashlib
import html as entities
import json
from pathlib import Path
import re
import time
from urllib.request import Request, urlopen

from lxml import html

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT.parent / 'work' / 'berthaml-tours'
LIST_URL = 'https://jerrybase.com/tours'
AGENT = 'Gratitude-BerthaML/1.0 (historical tour grouping research)'


def fetch(url, path, accept=None):
    headers = {'User-Agent': AGENT}
    if accept:
        headers.update({'Accept': accept, 'X-Requested-With': 'XMLHttpRequest'})
    data = urlopen(Request(url, headers=headers), timeout=45).read()
    path.write_bytes(data)
    time.sleep(1.5)
    return data


def tour_list(data):
    rows = json.loads(data.decode('utf-8'))['data']
    tours = []
    for link, span, count, _, act in rows:
        found = re.search(r'href="/tours/(\d+)"[^>]*>([^<]*)<', link)
        if not found or not re.fullmatch(r'\d{4}-\d\d-\d\d to \d{4}-\d\d-\d\d', span):
            raise ValueError(f'unexpected tour row: {link} {span}')
        if act == 'GD':
            first, last = span.split(' to ')
            tours.append(dict(source_id=int(found[1]), name=' '.join(entities.unescape(found[2]).split()),
                              first=first, last=last, listed_shows=count))
    if not tours:
        raise ValueError('no Grateful Dead tours; refuse an empty capture')
    return tours


def tour_events(data, source_id):
    root = html.fromstring(data.decode('utf-8'))
    tables = root.xpath('//table[@id="datatable_events"]')
    if len(tables) != 1:
        raise ValueError(f'tour {source_id}: event table missing or duplicated')
    urls = sorted({'https://jerrybase.com' + h for h in tables[0].xpath('.//a/@href')
                   if re.fullmatch(r'/events/\d{8}-\d\d', h)})
    if not urls:
        raise ValueError(f'tour {source_id}: no events')
    return urls


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--fetch', action='store_true', help='fetch uncached pages')
    p.add_argument('--cache', type=Path, default=CACHE)
    p.add_argument('--output', type=Path, default=ROOT / 'sources/jerrybase-tours.json.gz')
    args = p.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)

    def cached(name, url, accept=None):
        path = args.cache / name
        if not path.exists():
            if not args.fetch:
                raise FileNotFoundError(f'{path}; run with --fetch to acquire it')
            fetch(url, path, accept)
        data = path.read_bytes()
        return data, dict(url=url, sha256=hashlib.sha256(data).hexdigest(),
                          fetched_utc=datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat())

    data, list_capture = cached('tours.json', LIST_URL, 'application/json')
    tours = tour_list(data)
    for tour in tours:
        url = f'{LIST_URL}/{tour["source_id"]}'
        page, capture = cached(f'{tour["source_id"]}.html', url)
        tour['url'] = url
        tour['events'] = tour_events(page, tour['source_id'])
        tour['capture'] = capture
        print(tour['source_id'], tour['first'], len(tour['events']), tour['name'], flush=True)
    tours.sort(key=lambda t: (t['first'], t['source_id']))
    document = dict(source='Jerrybase tour groupings', list=list_capture, tours=tours,
                    note='Jerrybase groups some tours from band documents and others itself.')
    args.output.write_bytes(gzip.compress(
        (json.dumps(document, ensure_ascii=False, indent=1) + '\n').encode('utf-8'), mtime=0))
    print(f'{len(tours)} tours, {sum(len(t["events"]) for t in tours)} event links: {args.output}')


if __name__ == '__main__':
    main()
