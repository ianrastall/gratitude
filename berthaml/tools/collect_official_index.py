"""Capture factual release candidates by recording date, without review prose."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
from urllib.parse import urljoin
from urllib.request import Request, urlopen

from lxml import html

ROOT = Path(__file__).resolve().parents[1]
URL = 'https://www.deaddisc.com/GDFD_Dead_By_Date.htm'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fetch', action='store_true')
    args = parser.parse_args()
    cache = ROOT.parent / 'work/berthaml-official/dead-by-date.html'
    if not cache.exists():
        if not args.fetch:
            raise SystemExit('No cached inventory; use --fetch')
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_bytes(urlopen(Request(URL, headers={'User-Agent': 'Gratitude-BerthaML/1.0'}), timeout=45).read())
    raw = cache.read_bytes()
    # The source is an older Latin-1 HTML document, not UTF-8.
    doc = html.fromstring(raw.decode('windows-1252', errors='replace'))
    year, rows = None, []
    for row in doc.xpath('//tr'):
        cells = row.xpath('./td')
        if len(cells) != 3:
            continue
        heading = cells[0].xpath('./a/@name')
        if heading and re.fullmatch(r'19\d{2}', heading[0]):
            year = int(heading[0])
        if year is None or not 1967 <= year <= 1995:
            continue
        links = cells[2].xpath('.//a[@href]')
        for link in links:
            url = urljoin(URL, link.get('href'))
            if '/disc/' not in url:
                continue
            rows.append(dict(recording_year=year,
                             recording_date_label=' '.join(cells[0].text_content().split()),
                             content=' '.join(cells[1].text_content().split()),
                             release_label=' '.join(link.text_content().split()), url=url))
    if len(rows) < 100:
        raise ValueError('Unexpected official release table; refuse an incomplete capture')
    candidates = sorted({r['url'] for r in rows if not ('studio' in r['content'].lower()
                                                     and 'live' not in r['content'].lower())})
    value = dict(source=URL, captured_utc=datetime.fromtimestamp(cache.stat().st_mtime, timezone.utc).isoformat(),
                 response_sha256=hashlib.sha256(raw).hexdigest(), rows=rows,
                 candidate_urls=candidates, exhaustive=False,
                 scope='Candidate inventory for live 1967–1995 material; includes compilations and videos. '
                 'Release-page identity is not an exhaustive list of physical/digital editions.')
    path = ROOT / 'sources/official-release-index.json'
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'{len(rows)} factual recording-date associations; {len(candidates)} live-release page candidates.')


if __name__ == '__main__':
    main()
