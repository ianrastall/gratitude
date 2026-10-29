"""Stage 5 gate: closing the real window keeps the position.

Launches the WinUI app with a silent audio output and a throwaway data
folder, closes its window with WM_CLOSE (the same message as the X button)
during load, during a seek, while playing and while paused, and reads the
store back. --seek and --play are the app's test hooks.

Usage: python tests/stage5_window.py --book <book> [--runs 25] [--app <exe>]
"""
import argparse
import ctypes
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from stage3_resume import ROOT, TOLERANCE, Result, seed, stored  # noqa: E402

APP = ROOT / 'dist' / 'ShowPlayer' / 'ShowPlayer.exe'
WM_CLOSE = 0x0010
user32 = ctypes.windll.user32
EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def main_window(pid):
    """(handle, title) of the process's visible top-level window, or (None, '')."""
    found = []

    def visit(handle, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(handle, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(handle):
            text = ctypes.create_unicode_buffer(512)
            user32.GetWindowTextW(handle, text, 512)
            found.append((handle, text.value))
        return True

    user32.EnumWindows(EnumProc(visit), 0)
    return found[0] if found else (None, '')


class Window:
    def __init__(self, app, data, book, *options):
        """`book` may be None: the app then reopens the last book by itself."""
        self.proc = subprocess.Popen(
            [str(app), '--data', str(data), '--ao', 'null', *options, *([book] if book else [])])
        self.handle = None

    def wait(self, loaded, timeout=60):
        """Waits for the window; with `loaded`, until it shows a book's title."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline and self.proc.poll() is None:
            self.handle, title = main_window(self.proc.pid)
            if self.handle and (not loaded or title.endswith(' - Show Player')):
                return True
            time.sleep(0.02)
        return False

    def close(self):
        """Closes the window. Returns (exit code, seconds until the process ended)."""
        begin = time.monotonic()
        user32.PostMessageW(self.handle, WM_CLOSE, 0, 0)
        try:
            code = self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            code = -999
        return code, time.monotonic() - begin


def extra_checks(args, data):
    """Launch behaviour: last book, second launch, space bar."""
    results = []

    last = Result('S5a', 'a launch with no arguments reopens the last book')
    before = stored(data, args.book)
    window = Window(args.app, data, None)
    last.check(window.wait(loaded=True, timeout=30), 'no book was loaded')
    results.append(last)

    second = Result('S5b', 'a second launch defers to the running window')
    if window.handle:
        other = Window(args.app, data, None)
        try:
            code = other.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            other.proc.kill()
            code = -999
        second.check(code == 0, f'the second process ended with {code}')
        second.check(window.proc.poll() is None, 'the first window went away')
        print(f'    first window in the foreground afterwards: {user32.GetForegroundWindow() == window.handle}',
              flush=True)
    second.end_run()
    results.append(second)

    if window.handle and window.proc.poll() is None:
        code, _ = window.close()
        last.check(code == 0 and stored(data, args.book) == before, 'the position changed without any input')
    last.end_run()

    # The space key is posted to the window's input surface, so it goes
    # through the app's own key handling without taking the focus from
    # whatever the person at this machine is doing.
    space = Result('S5c', 'the space bar starts and stops playback')
    for _ in range(5):
        window = Window(args.app, data, None)
        before = stored(data, args.book)
        surface = None
        if window.wait(loaded=True, timeout=30):
            time.sleep(0.5)
            surface = input_surface(window.handle)
        if surface:
            press_space(surface)
            time.sleep(1.5)
            press_space(surface)
            time.sleep(1.0)
            window.close()
            played = stored(data, args.book) - before
            # About 1.5 s of playback, then paused for a second before closing.
            space.check(1.0 <= played <= 2.2, f'played {played:.3f} s, expected about 1.5')
        else:
            window.proc.kill()
            space.check(False, 'the window or its input surface never appeared')
        space.end_run()
    results.append(space)
    return results


def input_surface(window):
    found = []

    def visit(handle, _):
        name = ctypes.create_unicode_buffer(128)
        user32.GetClassNameW(handle, name, 128)
        if name.value == 'InputSiteWindowClass':
            found.append(handle)
        return True

    user32.EnumChildWindows(window, EnumProc(visit), 0)
    return found[0] if found else None


def press_space(surface):
    user32.PostMessageW(surface, 0x0100, 0x20, 0x00390001)  # WM_KEYDOWN
    user32.PostMessageW(surface, 0x0101, 0x20, 0xC0390001)  # WM_KEYUP


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--book', required=True)
    parser.add_argument('--runs', type=int, default=25, help='runs per close path')
    parser.add_argument('--app', default=str(APP))
    parser.add_argument('--seed', type=int, default=20261013)

    args = parser.parse_args()
    rng = random.Random(args.seed)
    work = tempfile.mkdtemp(prefix='abstage5-')
    data = pathlib.Path(work) / 'data'
    result = Result('S5', 'window close keeps the position')
    slowest = 0.0
    try:
        seed(data, args.book, 1234.5)
        # One uncounted launch first: the first start after a build is slow
        # (cold binaries), which says nothing about the close path.
        warmup = Window(args.app, data, args.book)
        if warmup.wait(loaded=True):
            warmup.close()
        for i in range(args.runs * 4):
            mode = ('load', 'seek', 'playing', 'paused')[i % 4]
            before = stored(data, args.book)
            if mode == 'load':
                window = Window(args.app, data, args.book)
                ready = window.wait(loaded=False)
                low = high = before
            elif mode == 'seek':
                target = round(rng.uniform(600, 200000), 3)
                window = Window(args.app, data, args.book, '--seek', str(target))
                ready = window.wait(loaded=True)
                low = high = target
            elif mode == 'playing':
                window = Window(args.app, data, args.book, '--play')
                ready = window.wait(loaded=True)
                started = time.monotonic()
                time.sleep(rng.uniform(0.6, 1.6))
                low = before + 0.1
            else:
                window = Window(args.app, data, args.book)
                ready = window.wait(loaded=True)
                time.sleep(0.2)
                low = high = before
            result.check(ready, f'{mode}: the window never appeared')
            if ready:
                code, seconds = window.close()
                slowest = max(slowest, seconds)
                saved = stored(data, args.book)
                if mode == 'playing':
                    # It cannot have played for longer than it was open.
                    high = before + (time.monotonic() - started) + 0.3
                result.check(code == 0, f'{mode}: exit code {code}')
                result.check(seconds <= 2.0, f'{mode}: exit took {seconds:.2f} s')
                result.check(low - TOLERANCE <= saved <= high + TOLERANCE,
                             f'{mode}: stored {saved:.3f}, wanted {low:.3f} to {high:.3f}')
            else:
                window.proc.kill()
            result.end_run()
        extras = extra_checks(args, data)
    finally:
        subprocess.run(['taskkill', '/F', '/IM', pathlib.Path(args.app).name], capture_output=True)
        shutil.rmtree(work, ignore_errors=True)
    ok = result.report()
    print(f'    slowest exit {slowest:.2f} s', flush=True)
    for extra in extras:
        ok = extra.report() and ok
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
