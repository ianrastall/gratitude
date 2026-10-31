"""Validate and package the data release; no application binaries or audio."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    for command in [
        [sys.executable, str(ROOT / 'tools/build_corpus.py'), '--check'],
        [sys.executable, str(ROOT / 'tools/audit_corpus.py')],
        [sys.executable, '-m', 'unittest', 'discover', '-s', str(ROOT / 'tools'), '-p', 'test_*.py']
    ]:
        subprocess.run(command, check=True)
    if sys.platform == 'win32':
        subprocess.run(['pwsh', '-NoProfile', '-File', str(ROOT / 'tools/validate-dotnet.ps1')], check=True)
    return package()


def package():
    metadata = json.loads((ROOT / 'dataset.json').read_text('utf-8'))
    destination = ROOT.parent / 'dist' / f'berthaml-data-{metadata["version"]}.zip'
    destination.parent.mkdir(parents=True, exist_ok=True)
    entries = {p.relative_to(ROOT).as_posix(): p.read_bytes() for p in sorted(ROOT.rglob('*'))
               if p.is_file() and not any(part in {'__pycache__', '.venv', 'snapshots'}
                                         for part in p.relative_to(ROOT).parts)
               and p.name != 'MANIFEST-SHA256.txt'
               and p.suffix not in {'.pyc', '.new'}}
    license_path = ROOT.parent / 'LICENSE'
    if not license_path.exists():
        license_path = ROOT / 'LICENSE.txt'
    entries['LICENSE.txt'] = license_path.read_bytes()
    checksums = ''.join(f'{hashlib.sha256(raw).hexdigest()}  {name}\n' for name, raw in sorted(entries.items()))
    entries['MANIFEST-SHA256.txt'] = checksums.encode('utf-8')
    temporary = destination.with_suffix('.zip.new')
    with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, raw in sorted(entries.items()):
            entry = zipfile.ZipInfo('berthaml/' + name, date_time=(1980, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, raw, compresslevel=9)
    with zipfile.ZipFile(temporary) as archive:
        if archive.testzip() is not None:
            raise ValueError('Data release zip failed CRC validation')
    temporary.replace(destination)
    sha = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_suffix('.zip.sha256').write_text(sha + '  ' + destination.name + '\n', encoding='utf-8', newline='\n')
    print(f'{destination}: {len(entries)} entries, {destination.stat().st_size:,} bytes; SHA-256 {sha}')
    return destination


if __name__ == '__main__':
    main()
