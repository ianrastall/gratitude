"""Report semantic event changes in a candidate capture; never replace data."""
import argparse
import gzip
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def compare(old, new):
    fields = ['date_label', 'venue_label', 'venue_url', 'category', 'source_show_id',
              'sections', 'unmodeled_contexts']
    before = {e['url']: e for e in old['events']}
    after = {e['url']: e for e in new['events']}
    return dict(added=[after[url] for url in sorted(after.keys() - before.keys())],
                removed=[before[url] for url in sorted(before.keys() - after.keys())],
                changed=[dict(url=url, changes={field: dict(before=before[url].get(field),
                                                          after=after[url].get(field))
                                               for field in fields if before[url].get(field) != after[url].get(field)})
                         for url in sorted(before.keys() & after.keys())
                         if any(before[url].get(field) != after[url].get(field) for field in fields)])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--baseline', type=Path, default=ROOT / 'sources/jerrybase-facts.json.gz')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = compare(json.loads(gzip.decompress(args.baseline.read_bytes())),
                     json.loads(gzip.decompress(args.candidate.read_bytes())))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(', '.join(f'{len(value)} {name}' for name, value in result.items()))


if __name__ == '__main__':
    main()
