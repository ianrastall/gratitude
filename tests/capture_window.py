"""Saves a PNG of a process's main window without bringing it forward.

Uses PrintWindow with PW_RENDERFULLCONTENT, which renders WinUI content even
when the window is covered. For visual review during development.

Usage: python tests/capture_window.py <pid> <out.png> [--title-contains <text>]
"""
import ctypes
import sys
from ctypes import wintypes

from PIL import Image

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32
user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # per-monitor v2: real pixel sizes
EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def find(pid, contains):
    found = []

    def visit(handle, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(handle, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(handle):
            text = ctypes.create_unicode_buffer(512)
            user32.GetWindowTextW(handle, text, 512)
            if contains in text.value:
                found.append(handle)
        return True

    user32.EnumWindows(EnumProc(visit), 0)
    return found[0] if found else None


def capture(handle, path):
    rect = wintypes.RECT()
    user32.GetWindowRect(handle, ctypes.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    screen = user32.GetDC(0)
    dc = gdi32.CreateCompatibleDC(screen)
    bitmap = gdi32.CreateCompatibleBitmap(screen, width, height)
    gdi32.SelectObject(dc, bitmap)
    user32.PrintWindow(handle, dc, 2)  # PW_RENDERFULLCONTENT

    class Header(ctypes.Structure):
        _fields_ = [('size', wintypes.DWORD), ('width', wintypes.LONG), ('height', wintypes.LONG),
                    ('planes', wintypes.WORD), ('bits', wintypes.WORD), ('compression', wintypes.DWORD),
                    ('image_size', wintypes.DWORD), ('xppm', wintypes.LONG), ('yppm', wintypes.LONG),
                    ('used', wintypes.DWORD), ('important', wintypes.DWORD)]

    header = Header(ctypes.sizeof(Header), width, -height, 1, 32, 0, 0, 0, 0, 0, 0)
    buffer = ctypes.create_string_buffer(width * height * 4)
    gdi32.GetDIBits(dc, bitmap, 0, height, buffer, ctypes.byref(header), 0)
    Image.frombuffer('RGB', (width, height), buffer, 'raw', 'BGRX', 0, 1).save(path)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(dc)
    user32.ReleaseDC(0, screen)
    return width, height


if __name__ == '__main__':
    contains = sys.argv[sys.argv.index('--title-contains') + 1] if '--title-contains' in sys.argv else ''
    handle = find(int(sys.argv[1]), contains)
    if not handle:
        sys.exit('no visible window')
    print(capture(handle, sys.argv[2]))
