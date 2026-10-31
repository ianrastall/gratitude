"""Extract event and setlist facts from cached year tables, excluding prose.

Run collect_index.py --fetch first to acquire the 29 serial, cached requests.
No event detail pages, reviews, lyrics, images or audio are harvested.
"""
import copy
import argparse
import gzip
import hashlib
import json
import re
from pathlib import Path

from lxml import html

from collect_index import CACHE, ROOT


def clean(value):
    return ' '.join(value.split())


def program(cell, comma_titles):
    sections, current = [], None
    contexts = 0

    def walk(node):
        nonlocal current, contexts
        if node.tag == 'b':
            current = dict(label=clean(node.text_content()).rstrip(':'), text='')
            sections.append(current)
            return
        if 'song-context' in node.get('class', '').split():
            content = clean(node.text_content())
            if content in ('Acoustic:', 'Electric:'):
                if current is None:
                    raise ValueError('mode before section heading')
                current['text'] += '\x1e' + content[:-1].lower() + '\x1e'
            else:
                contexts += 1
            return
        if node.text and node.text.strip():
            if current is None:
                raise ValueError('setlist text before section heading')
            current['text'] += node.text
        for child in node:
            walk(child)
            if child.tail and child.tail.strip():
                if current is None:
                    raise ValueError('setlist text before section heading')
                current['text'] += child.tail

    walk(copy.deepcopy(cell))
    for section in sections:
        text = section.pop('text')
        # Commas inside documented titles are not song delimiters.
        for title in sorted(comma_titles, key=len, reverse=True):
            text = re.sub(re.escape(title), lambda m: m[0].replace(',', '\ue000'),
                          text, flags=re.IGNORECASE)
        mode, items = None, []
        for segment in re.split('(\x1e(?:acoustic|electric)\x1e)', text):
            if segment.startswith('\x1e'):
                mode = segment.strip('\x1e')
                continue
            tokens = re.split(r'([,>])', segment)
            for i in range(0, len(tokens), 2):
                title = clean(tokens[i].replace('\ue000', ','))
                if not title:
                    continue
                item = dict(title=title, transition='segue' if i + 1 < len(tokens)
                            and tokens[i + 1] == '>' else 'unknown')
                if mode:
                    item['mode'] = mode
                items.append(item)
        section['items'] = items
    return sections, contexts


def capture(cache=CACHE, index_path=ROOT / 'sources/jerrybase-index.json',
            output=ROOT / 'sources/jerrybase-facts.json.gz'):
    licensed = json.loads(gzip.decompress((ROOT / 'sources/gdshowsdb.json.gz').read_bytes()))
    comma_titles = {s[':name'] for r in licensed for t in r[':sets'] for s in t[':songs']
                    if ',' in s[':name']}
    index = json.loads(index_path.read_text('utf-8'))
    captures = {x['year']: x for x in index['captures']}
    events = []
    for year in range(1967, 1996):
        raw = (cache / f'{year}.html').read_bytes()
        if hashlib.sha256(raw).hexdigest() != captures[year]['sha256']:
            raise ValueError(f'{year}: cached page differs from locked identity capture')
        table = html.fromstring(raw.decode('utf-8')).xpath('//table[@id="datatable_events"]')
        if len(table) != 1:
            raise ValueError(f'{year}: unexpected table')
        for row in table[0].xpath('./tbody/tr'):
            cells = row.xpath('./td')
            if len(cells) != 7:
                raise ValueError(f'{year}: unexpected table row')
            if clean(cells[2].text_content()) != 'Grateful Dead' or clean(cells[5].text_content()) != 'GD':
                continue
            url = 'https://jerrybase.com' + cells[0].xpath('./a/@href')[0]
            sections, contexts = program(cells[3], comma_titles)
            events.append(dict(url=url, date_label=clean(cells[0].text_content()),
                               venue_label=clean(cells[1].text_content()),
                               venue_url='https://jerrybase.com' + cells[1].xpath('./a/@href')[0],
                               category=clean(cells[4].text_content()),
                               source_show_id=clean(cells[6].text_content()),
                               sections=sections, unmodeled_contexts=contexts))
    if {e['url'] for e in events} != {e['url'] for e in index['events']}:
        raise ValueError('fact capture and identity index disagree')
    payload = json.dumps(dict(captures=index['captures'], events=events), ensure_ascii=False,
                         sort_keys=True, separators=(',', ':')).encode('utf-8')
    path = output
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(gzip.compress(payload, mtime=0))
    print(f'{len(events)} events; SHA-256 {hashlib.sha256(payload).hexdigest()}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=CACHE)
    parser.add_argument('--index', type=Path, default=ROOT / 'sources/jerrybase-index.json')
    parser.add_argument('--output', type=Path, default=ROOT / 'sources/jerrybase-facts.json.gz')
    args = parser.parse_args()
    capture(args.cache, args.index, args.output)
