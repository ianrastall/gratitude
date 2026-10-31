"""Audit a published corpus; --require-complete is the historical curation gate."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys

from lxml import etree
import issue_ids
from playlist_data import cache_name

ROOT = Path(__file__).resolve().parents[1]
N = {'b': 'urn:berthaml:1'}


def audit(require_complete=False, directory=None):
    directory = directory or ROOT
    manifest = json.loads((directory / 'corpus-manifest.json').read_text('utf-8'))
    errors = []
    lock = json.loads((ROOT / 'sources/inputs.lock.json').read_text('utf-8'))
    for name, expected in lock['files'].items():
        path = ROOT / 'sources' / name
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append('Pinned source changed or missing: ' + name)
    playlist_index = ROOT / 'sources/recording-playlists/index.json'
    if playlist_index.exists():
        for identifier, entry in json.loads(playlist_index.read_text('utf-8')).items():
            if entry.get('status') == 'captured':
                path = playlist_index.parent / cache_name(identifier)
                if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
                    errors.append('Playlist source changed or missing: ' + identifier)
    for name, expected in manifest['files'].items():
        path = directory / name
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append('Published file changed or missing: ' + name)
    stats = json.loads((directory / 'reports/coverage.json').read_text('utf-8'))
    shows = [etree.parse(str(p)).getroot() for p in sorted((directory / 'data/shows').rglob('*.xml'))]
    if not shows or len(shows) != stats['shows']:
        errors.append('Actual show count disagrees with coverage report')
    if sum(len(s.findall('.//b:performance', N)) for s in shows) != stats['performances']:
        errors.append('Actual performance count disagrees with coverage report')
    if dict(Counter(s.find('b:record', N).get('status') for s in shows)) != stats['show_status']:
        errors.append('Actual review statuses disagree with coverage report')
    recordings = [etree.parse(str(p)).getroot() for p in sorted((directory / 'data/recordings').rglob('*.xml'))]
    if sum(len(r.findall('b:recording', N)) for r in recordings) != stats['recordings']:
        errors.append('Actual recording count disagrees with coverage report')
    if {r.get('show') for r in recordings} - {s.get('id') for s in shows}:
        errors.append('Recording file names a show that does not exist')
    if stats['assigned_recording_references'] + stats['unassigned_recording_references'] != stats['supplied_recordings']:
        errors.append('Recording reference accounting does not balance')
    crosswalk = [json.loads(line) for line in (directory / 'reports/source-crosswalk.jsonl').read_text('utf-8').splitlines()]
    if {r['show'] for r in crosswalk} != {s.get('id') for s in shows} or len(crosswalk) != len(shows):
        errors.append('Source identity crosswalk does not account for every show exactly once')
    queue = [json.loads(line) for line in (directory / 'reports/review-queue.jsonl').read_text('utf-8').splitlines()]
    if len({r['id'] for r in queue}) != len(queue):
        errors.append('Duplicate issue IDs')
    if sum(r['state'] == 'open' for r in queue) != stats['open_issues']:
        errors.append('Open issue accounting disagrees with coverage report')
    ledger_path = directory / 'curated/id-ledger.json'
    if ledger_path.exists():
        issued = {value for values in json.loads(ledger_path.read_text('utf-8'))['issued'].values() for value in values}
        unissued = issue_ids.current(directory)
        unissued = {value for values in unissued.values() for value in values} - issued
        if unissued:
            errors.append(f'{len(unissued)} IDs have not been issued; run tools/issue_ids.py: '
                          + ', '.join(sorted(unissued)[:5]))
    for show in shows:
        rec = show.find('b:record', N)
        if not rec.findall('b:cite', N):
            errors.append('Uncited show: ' + show.get('id'))
    if require_complete:
        if stats['playlists']['unreviewed_track_mappings']:
            errors.append(f'{stats["playlists"]["unreviewed_track_mappings"]} track mappings remain unreviewed')
        if not stats['playlists']['official_release_inventory_complete']:
            errors.append('Official edition inventory remains incomplete')
        if stats['open_issues']:
            errors.append(f'{stats["open_issues"]} curation issues remain open')
        unreviewed = sum(s.find('b:record', N).get('status') not in ('reviewed', 'verified') for s in shows)
        if unreviewed:
            errors.append(f'{unreviewed} show records have not been reviewed')
        for registry in ['people', 'songs', 'venues', 'releases']:
            rec = etree.parse(str(directory / f'data/{registry}.xml')).getroot().find('b:record', N)
            if rec.get('status') not in ('reviewed', 'verified'):
                errors.append(f'{registry} registry has not been reviewed')
        if stats['completion'] != 'complete':
            errors.append('The published corpus is explicitly marked incomplete')
    for message in errors:
        print('ERROR ' + message)
    print(f'{len(shows)} shows; {stats["performances"]} performances; '
          f'{stats["open_issues"]} open curation issues; {len(errors)} audit errors.')
    return 1 if errors else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--require-complete', action='store_true')
    parser.add_argument('--corpus', type=Path, default=ROOT)
    args = parser.parse_args()
    return audit(args.require_complete, args.corpus.resolve())


if __name__ == '__main__':
    sys.exit(main())
