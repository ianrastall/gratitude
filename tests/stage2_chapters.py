"""Stage 2 gate: chapter navigation, seeking and clean-exit resume on real books.

Drives abharness.exe with a silent audio output and a throwaway data folder.
Usage: python tests/stage2_chapters.py <book> [<book> ...]
"""
import json
import pathlib
import random
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
HARNESS = ROOT / 'target' / 'x86_64-pc-windows-msvc' / 'release' / 'abharness.exe'
TOLERANCE = 0.050       # seconds, from the acceptance tests
PREVIOUS_THRESHOLD = 5.0


class Session:
    def __init__(self, data_dir):
        self.proc = subprocess.Popen(
            [str(HARNESS), '--data', str(data_dir), '--ao', 'null'],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, encoding='utf-8', bufsize=1)
        self.events = []

    def send(self, line):
        self.proc.stdin.write(line + '\n')
        self.proc.stdin.flush()

    def read_until(self, kind):
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError(f'harness exited while waiting for {kind}')
            event = json.loads(line)
            self.events.append(event)
            if event['ev'] == kind:
                return event

    def status(self, *commands):
        """Runs commands, lets the core settle, and returns the status."""
        for command in commands:
            self.send(command)
        self.send('settle')
        self.send('status')
        return self.read_until('status')

    def chapters(self):
        self.send('chapters')
        return self.read_until('chapters')['list']

    def quit(self):
        self.send('quit')
        self.proc.stdin.close()
        rest = [json.loads(line) for line in self.proc.stdout]
        self.events += rest
        code = self.proc.wait(timeout=10)
        shutdown = [e for e in rest if e['ev'] == 'reply' and e['command'] == 'shutdown']
        return code == 0 and bool(shutdown) and shutdown[0]['status'] == 0


class Checks:
    def __init__(self):
        self.passed = 0
        self.failures = []
        self.session = None
        self.trace = None

    def check(self, ok, message):
        if ok:
            self.passed += 1
            return
        if not self.failures and self.session:
            self.trace = [e for e in self.session.events if e['ev'] != 'position'][-25:]
        self.failures.append(message)

    def at(self, status, position, chapter, what):
        self.check(abs(status['pos'] - position) <= TOLERANCE and status['chapter'] == chapter,
                   f"{what}: expected {position:.3f} in chapter {chapter}, "
                   f"got {status['pos']:.3f} in chapter {status['chapter']} "
                   f"(state {status['state']}, error {status['error']}/{status['engine_error']}, t={status['t']})")


def chapter_of(chapters, position):
    index = -1
    for chapter in chapters:
        if chapter['start'] > position:
            break
        index += 1
    return index


def run_book(book, rng):
    c = Checks()
    with tempfile.TemporaryDirectory(prefix='abtest-') as data:
        s = c.session = Session(data)
        first = s.status(f'open {book}')
        c.check(first['state'] == 'paused' and first['pos'] == 0, f'fresh open: {first}')
        chapters = s.chapters()
        count, duration = len(chapters), first['dur']
        c.check(count == first['chapter_count'] and count > 0, 'chapter list size')
        c.check(all(a['end'] == b['start'] for a, b in zip(chapters, chapters[1:])) and
                abs(chapters[-1]['end'] - duration) < 0.001, 'chapter ends follow the next start')
        print(f"  {first['title']} | {first['author']} | {first['codec']} | "
              f"{duration / 3600:.2f} h | {count} chapters", flush=True)

        # Every chapter can be selected and reports itself.
        for i, chapter in enumerate(chapters):
            c.at(s.status(f'chapter {i}'), chapter['start'], i, f'select {i}')

        sample = sorted({0, 1, count - 1, *rng.sample(range(count), min(6, count))})
        for i in sample:
            start = chapters[i]['start']
            length = chapters[i]['end'] - start
            # The engine really is there: a second of playback moves on from the start.
            played = s.status(f'chapter {i}', 'play', 'sleep 1000', 'pause')
            c.check(start + 0.3 < played['pos'] < start + 1.8 and played['chapter'] == i,
                    f"play from {i}: {played['pos'] - start:.3f} s in, chapter {played['chapter']}")
            # Previous: early in a chapter goes back one, later restarts it.
            if length > 12:
                early = s.status(f'seek {start + 2}', 'settle', 'prev')
                c.at(early, chapters[max(i - 1, 0)]['start'], max(i - 1, 0), f'prev 2 s into {i}')
                late = s.status(f'seek {start + 10}', 'settle', 'prev')
                c.at(late, start, i, f'prev 10 s into {i}')
            # Next: on the last chapter it does nothing.
            before = s.status(f'seek {start + min(3, length / 2)}')
            after = s.status('next')
            if i + 1 < count:
                c.at(after, chapters[i + 1]['start'], i + 1, f'next from {i}')
            else:
                c.at(after, before['pos'], i, 'next on the last chapter')
                c.check(after['state'] == 'paused', f"next on the last chapter: state {after['state']}")

        # Absolute seeks land on target and in the right chapter.
        for _ in range(25):
            target = round(rng.uniform(0, duration - 5), 3)
            c.at(s.status(f'seek {target}'), target, chapter_of(chapters, target), f'seek {target}')
        relative = s.status(f'seek {chapters[1]["start"] + 30}', 'settle', 'rel -45')
        c.at(relative, chapters[1]['start'] - 15, 0, 'relative seek back across a boundary')

        # Clean-exit resume, including exactly on a chapter boundary.
        targets = [round(rng.uniform(0, duration - 5), 3) for _ in range(4)] + [chapters[count // 2]['start']]
        for target in targets:
            s.status(f'seek {target}')
            c.check(s.quit(), 'clean shutdown')
            s = c.session = Session(data)
            c.at(s.status(f'open {book}'), target, chapter_of(chapters, target), f'resume at {target}')
        c.check(s.quit(), 'clean shutdown')
    return c


def main():
    books = sys.argv[1:]
    if not books:
        sys.exit(__doc__)
    rng = random.Random(20261013)
    failed = 0
    for book in books:
        print(book, flush=True)
        c = run_book(book, rng)
        print(f'  {c.passed} checks passed, {len(c.failures)} failed', flush=True)
        for failure in c.failures[:20]:
            print('    FAIL ' + failure, flush=True)
        if c.failures and c.trace:
            print('    events before the first failure:', flush=True)
            for event in c.trace:
                print('      ' + json.dumps(event, ensure_ascii=False), flush=True)
        failed += len(c.failures)
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
