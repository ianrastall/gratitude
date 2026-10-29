"""Stage 3 gate: resume torture. Acceptance tests R1 to R7.

Drives abharness.exe with a silent audio output and throwaway data folders,
kills it at awkward moments, and reads the store back with Python's sqlite3.
R12 (power loss) needs a virtual machine and is not covered here.

Usage: python tests/stage3_resume.py --books <a> <b> <c> [--only R2 R3] [--scale 0.1]
                                     [--harness <exe>] [--gc-stress]
  --scale multiplies the run counts, for a quick smoke run.
  --harness runs another program that speaks the harness protocol, such as
  the C# binding check; --gc-stress passes it "--gc-stress yes".
"""
import argparse
import json
import pathlib
import random
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
HARNESS = ROOT / 'target' / 'x86_64-pc-windows-msvc' / 'release' / 'abharness.exe'
HARNESS_ARGS = []
TOLERANCE = 0.050     # seconds
SAVE_INTERVAL = 5.0   # the core's default
COLUMNS = 'position_ms, duration_ms, finished, save_seq, last_played_utc, file_size, missing_since_utc'


class Session:
    def __init__(self, data_dir):
        self.proc = subprocess.Popen(
            [str(HARNESS), '--data', str(data_dir), '--ao', 'null', *HARNESS_ARGS],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, encoding='utf-8', bufsize=1)
        self.events = []
        self.changed = threading.Condition()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.proc.stdout:
            event = json.loads(line)
            with self.changed:
                self.events.append(event)
                self.changed.notify_all()

    def send(self, *lines):
        try:
            for line in lines:
                self.proc.stdin.write(line + '\n')
            self.proc.stdin.flush()
        except OSError:
            pass

    def wait_for(self, kind, start, timeout=30):
        deadline = time.monotonic() + timeout
        with self.changed:
            while True:
                for event in self.events[start:]:
                    if event['ev'] == kind:
                        return event
                left = deadline - time.monotonic()
                if left <= 0 or self.proc.poll() is not None and not self.reader.is_alive():
                    raise RuntimeError(f'no {kind} event from the harness')
                self.changed.wait(min(left, 0.5))

    def status(self, *commands):
        start = len(self.events)
        self.send(*commands, 'settle', 'status')
        return self.wait_for('status', start)

    def chapters(self):
        start = len(self.events)
        self.send('chapters')
        return self.wait_for('chapters', start)['list']

    def last_position(self):
        """The newest position the core has reported for a loaded book."""
        with self.changed:
            for event in reversed(self.events):
                if 'pos' in event and event.get('state') in ('paused', 'playing'):
                    return event['pos']
        return None

    def kill(self):
        self.proc.kill()
        self.proc.wait()
        self.reader.join(5)

    def quit(self):
        """Clean exit. Returns (exit code, seconds taken)."""
        begin = time.monotonic()
        self.send('quit')
        try:
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            code = self.proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            code = -999
        self.reader.join(5)
        return code, time.monotonic() - begin


def records(data_dir):
    """Every book row, keyed by path, read without going through the core."""
    db = pathlib.Path(data_dir) / 'bookshelf.db'
    try:
        con = sqlite3.connect(f'file:{db.as_posix()}?mode=ro', uri=True, timeout=10)
        rows = con.execute(f'SELECT path, {COLUMNS} FROM book').fetchall()
    except sqlite3.OperationalError:
        con = sqlite3.connect(str(db), timeout=10)
        rows = con.execute(f'SELECT path, {COLUMNS} FROM book').fetchall()
    con.close()
    return {row[0].lower(): row[1:] for row in rows}


def stored(data_dir, book):
    return records(data_dir)[book.lower()][0] / 1000.0


class Result:
    def __init__(self, name, title):
        self.name, self.title = name, title
        self.runs = self.failed_runs = 0
        self.messages = []
        self.run_ok = True

    def check(self, ok, message):
        if not ok:
            self.run_ok = False
            if len(self.messages) < 12:
                self.messages.append(f'run {self.runs + 1}: {message}')

    def end_run(self):
        self.runs += 1
        self.failed_runs += 0 if self.run_ok else 1
        self.run_ok = True

    def report(self):
        verdict = 'PASS' if not self.failed_runs else 'FAIL'
        print(f'{self.name} {self.title}: {verdict}, {self.runs - self.failed_runs} of {self.runs} runs', flush=True)
        for message in self.messages:
            print('    ' + message, flush=True)
        return not self.failed_runs


def seed(data_dir, book, position):
    s = Session(data_dir)
    s.status(f'open {book}')
    s.status(f'seek {position}')
    s.quit()


def r1_resume_accuracy(ctx, r):
    for book in ctx.books:
        data = ctx.data_dir()
        s = Session(data)
        duration = s.status(f'open {book}')['dur']
        for _ in range(ctx.runs(50)):
            target = round(ctx.rng.uniform(0, duration - 5), 3)
            s.status(f'seek {target}', 'settle', 'pause')
            code, _ = s.quit()
            r.check(code == 0, f'exit code {code}')
            saved = stored(data, book)
            r.check(abs(saved - target) <= TOLERANCE, f'stored {saved:.3f}, wanted {target:.3f}')
            s = Session(data)
            restored = s.status(f'open {book}')
            r.check(abs(restored['pos'] - target) <= TOLERANCE and restored['state'] == 'paused',
                    f"restored {restored['pos']:.3f} {restored['state']}, wanted {target:.3f}")
            r.end_run()
        s.quit()


def r2_kill_while_playing(ctx, r):
    data = ctx.data_dir()
    previous = {}
    for book in ctx.books:
        previous[book] = round(ctx.rng.uniform(600, 3000), 3)
        seed(data, book, previous[book])
    for i in range(ctx.runs(200)):
        book = ctx.books[i % len(ctx.books)]
        s = Session(data)
        resumed = s.status(f'open {book}')
        r.check(abs(resumed['pos'] - previous[book]) <= TOLERANCE,
                f"resumed at {resumed['pos']:.3f}, store had {previous[book]:.3f}")
        s.send('play')
        time.sleep(ctx.rng.uniform(0.2, 7.0))
        heard = s.last_position()
        s.kill()
        saved = stored(data, book)
        # The store may run ahead of the last event this script read, never
        # ahead of the audio by more than that reporting lag.
        r.check(saved > 0, 'stored position is 0')
        r.check(saved <= heard + 0.5, f'stored {saved:.3f} is after the kill-time position {heard:.3f}')
        r.check(saved >= heard - (SAVE_INTERVAL + 1), f'stored {saved:.3f} is too far before {heard:.3f}')
        r.check(saved >= previous[book] - 0.001, f'stored {saved:.3f} went back from {previous[book]:.3f}')
        previous[book] = saved
        r.end_run()


def r3_kill_while_loading(ctx, r):
    data = ctx.data_dir()
    for book in ctx.books:
        seed(data, book, round(ctx.rng.uniform(600, 3000), 3))
    for i in range(ctx.runs(200)):
        before = records(data)
        s = Session(data)
        s.send(f'open {ctx.books[i % len(ctx.books)]}')
        time.sleep(ctx.rng.uniform(0, 0.5))
        s.kill()
        r.check(records(data) == before, 'a record changed')
        r.end_run()


def r4_pause_then_kill(ctx, r):
    data = ctx.data_dir()
    for book in ctx.books:
        seed(data, book, round(ctx.rng.uniform(600, 3000), 3))
    for i in range(ctx.runs(100)):
        book = ctx.books[i % len(ctx.books)]
        s = Session(data)
        s.status(f'open {book}')
        s.send('play')
        time.sleep(ctx.rng.uniform(0.3, 1.5))
        paused = s.status('pause')
        time.sleep(0.3)
        s.kill()
        saved = stored(data, book)
        r.check(paused['state'] == 'paused' and abs(saved - paused['pos']) <= TOLERANCE,
                f"stored {saved:.3f}, paused at {paused['pos']:.3f} ({paused['state']})")
        r.end_run()


def r5_seek_then_kill(ctx, r):
    data = ctx.data_dir()
    for i in range(ctx.runs(100)):
        book = ctx.books[i % len(ctx.books)]
        s = Session(data)
        s.status(f'open {book}')
        chapters = s.chapters()
        index = ctx.rng.randrange(len(chapters))
        s.send(f'chapter {index}')
        time.sleep(0.6)
        s.kill()
        saved = stored(data, book)
        # 2 ms of slack: the store rounds down to the millisecond and the
        # harness prints chapter starts rounded to the nearest one.
        r.check(chapters[index]['start'] - 0.002 <= saved < chapters[index]['end'],
                f"stored {saved:.3f} is outside chapter {index} "
                f"({chapters[index]['start']:.3f} to {chapters[index]['end']:.3f})")
        s = Session(data)
        shown = s.status(f'open {book}')
        s.kill()
        r.check(shown['chapter'] == index, f"chapter {shown['chapter']} shown after restart, selected {index}")
        r.end_run()


def r6_book_switching(ctx, r):
    data = ctx.data_dir()
    books = ctx.books + ctx.synthetic_books()
    low = {book: 1000.0 * k + 100 for k, book in enumerate(books)}
    high = {book: 1000.0 * k + 900 for k, book in enumerate(books)}
    for book in books:
        seed(data, book, low[book] + 400)

    def check_store(where):
        for path, row in records(data).items():
            book = next(b for b in books if b.lower() == path)
            r.check(low[book] <= row[0] / 1000.0 <= high[book],
                    f'{where}: {pathlib.Path(book).name} stored {row[0] / 1000.0:.3f}, '
                    f'outside {low[book]:.0f} to {high[book]:.0f}')

    s = Session(data)
    opened = []  # opened[g - 1] is the book of generation g
    for i in range(ctx.runs(1000)):
        book = ctx.rng.choice(books)
        opened.append(book)
        commands = [f'open {book}']
        if ctx.rng.random() < 0.6:
            commands.append(f'seek {ctx.rng.uniform(low[book], high[book] - 100):.3f}')
        if ctx.rng.random() < 0.5:
            commands.append('play')
        s.send(*commands)
        time.sleep(ctx.rng.choice([0, 0, 0.01, 0.05, 0.1, 0.2, 0.3, 0.5, 1.0, 2.0]))
        if i % 100 == 99:
            s.status()
            check_store(f'after {i + 1} switches')
        r.end_run()
    code, _ = s.quit()
    r.check(code == 0, f'exit code {code}')
    check_store('at the end')
    # No event may carry one book's position under another book's generation.
    for event in s.events:
        if event.get('state') in ('paused', 'playing') and event['ev'] in ('position', 'state', 'chapter', 'book_loaded'):
            book = opened[event['gen'] - 1]
            r.check(low[book] - 1 <= event['pos'] <= high[book] + 1,
                    f"event for {pathlib.Path(book).name} (generation {event['gen']}) at {event['pos']:.3f}")
    r.end_run()


def r7_exit_paths(ctx, r):
    data = ctx.data_dir()
    for book in ctx.books:
        seed(data, book, round(ctx.rng.uniform(600, 3000), 3))
    for i in range(ctx.runs(200)):
        book = ctx.books[i % len(ctx.books)]
        mode = ('load', 'seek', 'playing', 'paused')[i % 4]
        before = stored(data, book)
        s = Session(data)
        if mode == 'load':
            s.send(f'open {book}')
            time.sleep(ctx.rng.uniform(0, 0.3))
            low_bound = high_bound = before
        else:
            duration = s.status(f'open {book}')['dur']
            if mode == 'seek':
                target = round(ctx.rng.uniform(0, duration - 5), 3)
                s.send(f'seek {target}')
                time.sleep(ctx.rng.uniform(0, 0.2))
                low_bound = high_bound = target
            elif mode == 'playing':
                s.send('play')
                time.sleep(ctx.rng.uniform(0.3, 1.5))
                low_bound = s.last_position()
                high_bound = low_bound + 0.6
            else:
                low_bound = high_bound = s.status('play', 'sleep 500', 'pause')['pos']
        code, seconds = s.quit()
        saved = stored(data, book)
        r.check(code == 0, f'{mode}: exit code {code}')
        r.check(seconds <= 2.0, f'{mode}: exit took {seconds:.2f} s')
        r.check(low_bound - TOLERANCE <= saved <= high_bound + TOLERANCE,
                f'{mode}: stored {saved:.3f}, wanted {low_bound:.3f} to {high_bound:.3f}')
        r.end_run()


TESTS = [
    ('R1', 'resume accuracy', r1_resume_accuracy),
    ('R2', 'kill while playing', r2_kill_while_playing),
    ('R3', 'kill while loading', r3_kill_while_loading),
    ('R4', 'pause then kill', r4_pause_then_kill),
    ('R5', 'seek then kill', r5_seek_then_kill),
    ('R6', 'book switching', r6_book_switching),
    ('R7', 'exit paths', r7_exit_paths),
]


class Context:
    def __init__(self, args, work):
        self.books = args.books
        self.scale = args.scale
        self.rng = random.Random(args.seed)
        self.work = pathlib.Path(work)
        self.count = 0

    def runs(self, full):
        return max(2, round(full * self.scale))

    def data_dir(self):
        self.count += 1
        return self.work / f'data{self.count}'

    def synthetic_books(self):
        """Two generated books, so the switching test has five."""
        made = []
        for n, frequency in enumerate((330, 550)):
            path = self.work / f'synthetic{n}.mka'
            if not path.exists():
                subprocess.run(
                    ['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-f', 'lavfi', '-i',
                     f'sine=frequency={frequency}:sample_rate=48000:duration=6000',
                     '-ac', '1', '-c:a', 'libopus', '-b:a', '16k', str(path)], check=True)
            made.append(str(path))
        return made


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--books', nargs='+', required=True)
    parser.add_argument('--only', nargs='*', default=[])
    parser.add_argument('--scale', type=float, default=1.0)
    parser.add_argument('--seed', type=int, default=20261013)
    parser.add_argument('--harness')
    parser.add_argument('--gc-stress', action='store_true')
    args = parser.parse_args()
    global HARNESS
    if args.harness:
        HARNESS = pathlib.Path(args.harness).resolve()
    if args.gc_stress:
        HARNESS_ARGS.extend(['--gc-stress', 'yes'])

    work = tempfile.mkdtemp(prefix='abstage3-')
    ctx = Context(args, work)
    ok = True
    try:
        for name, title, body in TESTS:
            if args.only and name not in args.only:
                continue
            result = Result(name, title)
            begin = time.monotonic()
            try:
                body(ctx, result)
            except Exception as error:  # a hung or dead harness fails the test, not the script
                result.check(False, f'aborted: {error!r}')
                result.end_run()
            ok = result.report() and ok
            print(f'    ({time.monotonic() - begin:.0f} s)', flush=True)
    finally:
        subprocess.run(['taskkill', '/F', '/IM', HARNESS.name], capture_output=True)
        shutil.rmtree(work, ignore_errors=True)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
