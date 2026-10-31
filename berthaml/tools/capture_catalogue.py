"""Pin the supplied catalogue and acquired factual snapshots for offline builds."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, help='optional supplied catalogue; otherwise retain the pinned snapshot')
    args = parser.parse_args()
    target = ROOT / 'sources/catalogue.xml.gz'
    if args.input:
        raw = args.input.read_bytes()
    elif target.exists():
        raw = gzip.decompress(target.read_bytes())
    else:
        raw = (ROOT.parent / 'data/gd-shows.xml').read_bytes()
    target.write_bytes(gzip.compress(raw, mtime=0))
    files = ['catalogue.xml.gz', 'gdshowsdb.json.gz', 'gdshowsdb-lock.json',
             'gdshowsdb-LICENSE.txt', 'jerrybase-index.json', 'jerrybase-facts.json.gz',
             'legacy-identity-map.json', 'pre-consolidation-lock.json']
    if (ROOT / 'sources/recording-playlists/index.json').exists():
        files.append('recording-playlists/index.json')
    if (ROOT / 'sources/official-release-index.json').exists():
        files.append('official-release-index.json')
    files.extend(p.relative_to(ROOT / 'sources').as_posix()
                 for p in sorted((ROOT / 'sources/official-playlists').glob('*.json')))
    lock = dict(version=1, scope='Public Grateful Dead events, 1967 through 1995-07-09',
                supplied_catalogue=dict(path='../../data/gd-shows.xml',
                                        sha256=hashlib.sha256(raw).hexdigest()),
                files={name: hashlib.sha256((ROOT / 'sources' / name).read_bytes()).hexdigest()
                       for name in files})
    (ROOT / 'sources/inputs.lock.json').write_text(json.dumps(lock, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(f'Pinned {len(files)} inputs; supplied catalogue SHA-256 {lock["supplied_catalogue"]["sha256"]}')


if __name__ == '__main__':
    main()
