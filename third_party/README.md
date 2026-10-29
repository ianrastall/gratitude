# Third-party binaries

Binaries under `libmpv/` and `tools/` are local build prerequisites and are not committed. This file records their provenance. The installer includes the tools and accompanying license files. The app source is MIT; each external tool retains its own license.

## Download and packaging tools

| Tool | Build/source | License | Binary SHA-256 |
| --- | --- | --- | --- |
| aria2c | Official `aria2-1.37.0-win-64bit-build1.zip`, https://github.com/aria2/aria2/releases/tag/release-1.37.0 | GPL-2.0-or-later with the included OpenSSL exception/license; `COPYING`, `aria2-LICENSE.OpenSSL.txt` and authors ship in tools | `BE2099C214F63A3CB4954B09A0BECD6E2E34660B886D4C898D260FEBFE9D70C2` |
| ffmpeg | Existing local Chocolatey installation, Gyan essentials build 8.0.1, https://www.gyan.dev/ffmpeg/builds/ | GPL version 3 (`--enable-gpl --enable-version3`); `FFmpeg-LICENSE.txt` ships in tools | `5AF82A0D4FE2B9EAE211B967332EA97EDFC51C6B328CA35B827E73EAC560DC0D` |
| ffprobe | Same FFmpeg 8.0.1 distribution | Same GPL version 3 | `192A1D6899059765AC8C39764FC3148D4E6049955956DC2029F81F4BD6A8972D` |

aria2 corresponding source: https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0.tar.xz. FFmpeg upstream source: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz; Gyan build scripts and dependency details: https://github.com/GyanD/codexffmpeg. The executables are unmodified. Record their complete `ffmpeg -version` configuration when replacing them. Public binary redistribution needs the exact corresponding build/dependency sources made available; this initial delivery is installed locally and has not been published as a public binary release.

For a local rebuild on this machine, the same FFmpeg files remain at `C:/ProgramData/chocolatey/lib/ffmpeg/tools/ffmpeg/bin/`; the libmpv build is also available in the sibling Audio Bookshelf checkout. Obtain aria2 from the pinned official release above. Place executables in `third_party/tools/` and retain the license files alongside them.

## libmpv

`third_party/libmpv/libmpv-2.dll` comes from the official mpv GitHub release
assets, LGPL variant. The tagged v0.41.0 release carries no libmpv package;
only the rolling `git-release` does, so the exact asset must be recorded here
each time it is replaced.

| Field | Value |
| --- | --- |
| Asset | `libmpv-v0.41.0-dev-gc3ac7f395-38326895870-x86_64-w64-mingw32-lgpl.zip` |
| Source | https://github.com/mpv-player/mpv/releases/tag/git-release, downloaded 2026-10-13 |
| Commit | `c3ac7f395` (client API 2.5, FFmpeg `git-2026-10-12-0eeb79f`) |
| License | LGPL (`LICENSE.LGPL` in the zip) |
| SHA-256 | `9ffbed7a8aa013da07a925dfbb046116683796af76a9b635e4e7b3328d44f9d5` |
| Size | zip 56,765,949 bytes; `libmpv-2.dll` 127,147,520 bytes |
| `dumpbin /dependents` | Windows system DLLs and UCRT API sets only; no MinGW or third-party runtime DLLs |

The zip also contains `libmpv.dll.a` (MinGW import library, unused: we load
the DLL through a function table), the `include/mpv` headers, and two
libbluray `.jar` files that are not shipped.
