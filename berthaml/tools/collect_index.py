"""Capture public GD event identity facts for reconciliation, never review prose.

One cached request per year, spaced serially. Offline reruns use only the cache.
The resulting index is evidence, not an instruction to accept every listed event.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import time
import unicodedata
from urllib.request import Request, urlopen

from lxml import html

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT.parent / 'work' / 'berthaml-index'


def normalize(text):
    return re.sub('[^a-z0-9]', '', unicodedata.normalize('NFKD', text)
                  .encode('ascii', 'ignore').decode().lower())


def fingerprint(text):
    normalized = normalize(text)
    return hashlib.sha256(normalized.encode()).hexdigest() if normalized else None


def parse(data, year):
    root = html.fromstring(data.decode('utf-8'))
    tables = root.xpath('//table[@id="datatable_events"]')
    if len(tables) != 1:
        raise ValueError(f'{year}: event table missing or duplicated')
    result = []
    for row in tables[0].xpath('./tbody/tr'):
        cells = row.xpath('./td')
        if len(cells) != 7:
            raise ValueError(f'{year}: unexpected event row')
        text = lambda i: ' '.join(cells[i].text_content().split())
        if text(2) != 'Grateful Dead' or text(5) != 'GD':
            continue
        event_links = cells[0].xpath('./a/@href')
        venue_links = cells[1].xpath('./a/@href')
        if len(event_links) != 1 or len(venue_links) != 1:
            raise ValueError(f'{year}: event/venue link missing')
        # Hash only the factual song sequence, excluding set-heading labels.
        # The comparison index does not redistribute the source's setlist text.
        for heading in cells[3].xpath('.//b'):
            heading.text = ''
        result.append(dict(date_label=text(0), venue_label=text(1),
                           category=text(4), source_show_id=text(6),
                           program_fingerprint=fingerprint(cells[3].text_content()),
                           url='https://jerrybase.com' + event_links[0],
                           venue_url='https://jerrybase.com' + venue_links[0]))
    if not result:
        raise ValueError(f'{year}: no GD events; refuse an empty capture')
    if len({r['url'] for r in result}) != len(result):
        raise ValueError(f'{year}: duplicate event URLs')
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--fetch', action='store_true', help='fetch uncached year pages')
    p.add_argument('--cache', type=Path, default=CACHE)
    p.add_argument('--output', type=Path, default=ROOT / 'sources/jerrybase-index.json')
    args = p.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    events, captures = [], []
    for year in range(1967, 1996):
        path = args.cache / f'{year}.html'
        url = f'https://jerrybase.com/events?year={year}'
        if not path.exists():
            if not args.fetch:
                raise FileNotFoundError(f'{path}; run with --fetch to acquire it')
            data = urlopen(Request(url, headers={'User-Agent':
                'Gratitude-BerthaML/1.0 (historical event identity research)'}), timeout=45).read()
            parse(data, year)  # Do not cache an error page.
            path.write_bytes(data)
            time.sleep(1.5)
        data = path.read_bytes()
        rows = parse(data, year)
        events.extend(rows)
        captures.append(dict(year=year, url=url, sha256=hashlib.sha256(data).hexdigest(),
                             fetched_utc=datetime.fromtimestamp(path.stat().st_mtime, timezone.utc).isoformat(),
                             events=len(rows)))
        print(year, len(rows), flush=True)
    output = args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(dict(captures=captures, events=events), ensure_ascii=False,
                                 indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'{len(events)} event identity records: {output}')


if __name__ == '__main__':
    main()
