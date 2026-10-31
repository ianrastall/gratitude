"""Writes a small Shorten (.shn, version 2) file for the workflow check.

No free Shorten encoder ships with FFmpeg, so this holds the few format rules
the check needs: a verbatim WAV header, then Rice-coded DIFF0 blocks.

Usage: python tests/make_shn.py <folder>
"""
import math
import pathlib
import struct
import sys

FN_DIFF0, FN_QUIT, FN_BLOCKSIZE, FN_VERBATIM = 0, 4, 5, 9
BLOCK = 256
RICE = 13


class Bits:
    def __init__(self):
        self.out = bytearray()
        self.current = 0
        self.count = 0

    def bit(self, value):
        self.current = (self.current << 1) | (value & 1)
        self.count += 1
        if self.count == 8:
            self.out.append(self.current)
            self.current = self.count = 0

    def unsigned(self, value, width):
        for _ in range(value >> width):
            self.bit(0)
        self.bit(1)
        for i in range(width - 1, -1, -1):
            self.bit(value >> i)

    def signed(self, value, width):
        self.unsigned(value << 1 if value >= 0 else (-value << 1) - 1, width + 1)

    def long(self, value):
        width = 0
        while (value >> width) >= 4 and width < 31:
            width += 1
        self.unsigned(width, 2)
        self.unsigned(value, width)

    def finish(self):
        while self.count:
            self.bit(0)
        return bytes(self.out)


def write(path, frequency, seconds, rate=44100, channels=2):
    frames = int(rate * seconds)
    pcm = [[int(9000 * math.sin(2 * math.pi * (frequency + c * 3) * i / rate)) for i in range(frames)]
           for c in range(channels)]
    data = struct.pack('<' + 'h' * (frames * channels), *[pcm[c][i] for i in range(frames) for c in range(channels)])
    header = (b'RIFF' + struct.pack('<I', 36 + len(data)) + b'WAVEfmt '
              + struct.pack('<IHHIIHH', 16, 1, channels, rate, rate * channels * 2, channels * 2, 16)
              + b'data' + struct.pack('<I', len(data)))
    bits = Bits()
    for value in (5, channels, BLOCK, 0, 0, 0):  # s16le, channels, block size, LPC, mean, skip
        bits.long(value)
    bits.unsigned(FN_VERBATIM, 2)
    bits.unsigned(len(header), 5)
    for byte in header:
        bits.unsigned(byte, 8)
    for start in range(0, frames, BLOCK):
        size = min(BLOCK, frames - start)
        if size != BLOCK:
            bits.unsigned(FN_BLOCKSIZE, 2)
            bits.long(size)
        for c in range(channels):
            bits.unsigned(FN_DIFF0, 2)
            bits.unsigned(RICE, 3)
            for i in range(size):
                bits.signed(pcm[c][start + i], RICE)
    bits.unsigned(FN_QUIT, 2)
    path.write_bytes(b'ajkg' + bytes([2]) + bits.finish() + bytes(16))


folder = pathlib.Path(sys.argv[1])
folder.mkdir(parents=True, exist_ok=True)
write(folder / 'gd77-12-27d1t01.shn', 440, 3)
write(folder / 'gd77-12-27d1t02.shn', 550, 2.5)
(folder / 'gd77-12-27.txt').write_text(
    'Grateful Dead\n12-27-1977\nWinterland Arena\nSan Francisco  CA\n'
    'SBD>>MR>>DAT>>CDA/SHN\n\nSet 1:\n101-d1t01 - Bertha ->\n102-d1t02 - Sugaree (1)\n')
