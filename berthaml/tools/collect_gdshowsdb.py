"""Extract the licensed, pinned show facts; keep enough input to rebuild offline."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
from urllib.request import Request, urlopen
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[1]
REVISION = '2ccd86c1f0db284ed1f3967df17f8defc80a387e'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--archive', type=Path, help='use an already captured revision zip')
    args = p.parse_args()
    url = f'https://codeload.github.com/jefmsmit/gdshowsdb/zip/{REVISION}'
    raw = args.archive.read_bytes() if args.archive else urlopen(
        Request(url, headers={'User-Agent': 'Gratitude-BerthaML/1.0'}), timeout=60).read()
    archive = zipfile.ZipFile(io.BytesIO(raw))
    prefix = f'gdshowsdb-{REVISION}/'
    records = []
    for year in range(1967, 1996):
        data = yaml.load(archive.read(prefix + f'data/gdshowsdb/{year}.yaml'),
                         Loader=yaml.CSafeLoader)
        for key, row in data.items():
            records.append(dict(key=str(key), **row))
    payload = json.dumps(records, ensure_ascii=False, sort_keys=True,
                         separators=(',', ':')).encode('utf-8')
    sources = ROOT / 'sources'
    sources.mkdir(parents=True, exist_ok=True)
    (sources / 'gdshowsdb.json.gz').write_bytes(gzip.compress(payload, mtime=0))
    (sources / 'gdshowsdb-LICENSE.txt').write_bytes(archive.read(prefix + 'LICENSE.txt'))
    lock = dict(revision=REVISION, url=url, archive_sha256=hashlib.sha256(raw).hexdigest(),
                facts_sha256=hashlib.sha256(payload).hexdigest(), records=len(records),
                scope='1967-01-01 through 1995-07-09', license='MIT')
    (sources / 'gdshowsdb-lock.json').write_text(json.dumps(lock, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps(lock, indent=2))


if __name__ == '__main__':
    main()
