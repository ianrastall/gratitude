"""Record the corpus's IDs as issued. Once issued an ID is permanent.

Run this after adding a show, venue, song, person or release. To rename or merge
one, change the data, then pass --retire OLD NEW so anything that stored the old ID
can still find its replacement. The validator refuses an issued ID that has gone
with no retirement, and a retired ID that comes back into use.
"""
import argparse
import json
from pathlib import Path
import sys

from lxml import etree

ROOT = Path(__file__).resolve().parents[1]
N = {'b': 'urn:berthaml:1'}
REGISTRIES = dict(people='person', songs='song', venues='venue', releases='release')


def current(directory=ROOT):
    ids = {name: sorted(node.get('id') for node in etree.parse(str(directory / f'data/{name}.xml')).getroot()
                        .findall('b:' + tag, N)) for name, tag in REGISTRIES.items()}
    ids['shows'] = sorted(p.stem for p in (directory / 'data/shows').rglob('*.xml'))
    return ids


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--retire', nargs=2, action='append', default=[], metavar=('OLD', 'NEW'))
    args = parser.parse_args()
    path = ROOT / 'curated/id-ledger.json'
    ledger = json.loads(path.read_text('utf-8'))
    ledger['retired'].update(args.retire)
    ids = current()
    known = {value for values in ids.values() for value in values}
    gone = [value for values in ledger['issued'].values() for value in values
            if value not in known and value not in ledger['retired']]
    if gone:
        print('Issued IDs are missing with no retirement: ' + ', '.join(gone[:10]), file=sys.stderr)
        return 1
    ledger['issued'] = {kind: sorted(set(ledger['issued'].get(kind, [])) | set(values)) for kind, values in ids.items()}
    ledger['retired'] = dict(sorted(ledger['retired'].items()))
    path.write_text(json.dumps(ledger, ensure_ascii=False, indent=1) + '\n', encoding='utf-8', newline='\n')
    print(f'{sum(map(len, ledger["issued"].values()))} IDs issued; {len(ledger["retired"])} retired')
    return 0


if __name__ == '__main__':
    sys.exit(main())
