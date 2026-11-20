#!/bin/bash
# Decodes spacecomponentsbytype into a dump directory, using the client's own loader under Wine.
#
# This is the one container CCP's static data export does not carry and no library can read: it is
# FSD binary, and the only decoder in existence is a Python 2.7 extension for 64-bit Windows that
# ships inside the game client. Both it and the data are on CCP's CDN, so pairing them with Wine
# works - which is what the game's own macOS client does, since that is the Windows build in a
# translation layer.
#
# Needs an amd64 host with 4 KB memory pages. Notably that rules out Docker on Apple Silicon, whose
# Linux VM uses 16 KB pages and where Wine aborts in anon_mmap_fixed regardless of the emulator.
#
#     tools/space-components.sh <dump-dir>
set -euo pipefail

DUMP_DIR="${1:?usage: space-components.sh <dump-dir>}"
MSI_URL=https://www.python.org/ftp/python/2.7.18/python-2.7.18.amd64.msi
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-eve}"
export WINEARCH=win64
export WINEDEBUG=-all
PYTHON27="$WINEPREFIX/drive_c/Python27/python.exe"

if [ ! -f "$PYTHON27" ]; then
    # wineboot complains about a missing 32-bit rundll32 in a pure win64 prefix, which is expected
    # and harmless here: the interpreter and the loader are both x86-64.
    wineboot --init 2>&1 | grep -v syswow64 || true
    # Debian's Wine 10 (Ubuntu 26.04) no longer puts wineserver on PATH.
    "$(command -v wineserver || echo /usr/lib/x86_64-linux-gnu/wine/wineserver)" -w

    # The MSI is unpacked rather than installed: a headless MSI install under Wine is the flakiest
    # step in the whole business, and none of what it does - registry keys, PATH, file associations
    # - matters when the only thing that ever runs is python.exe by absolute path.
    curl -fsSLo "$WORK/python27.msi" "$MSI_URL"
    msiextract --directory "$WORK/python27" "$WORK/python27.msi" >/dev/null
    python_exe=$(find "$WORK/python27" -name python.exe -print -quit)
    test -n "$python_exe" || { echo "no python.exe in the MSI payload" >&2; exit 1; }
    install -d "$WINEPREFIX/drive_c/Python27"
    cp -a "$(dirname "$python_exe")/." "$WINEPREFIX/drive_c/Python27/"
fi
wine "$PYTHON27" -c "import sys; assert sys.version_info[:2] == (2, 7), sys.version"

# The network side runs on the host, so Wine's ancient Python never has to do TLS.
python3 "$HERE/fetch.py" "$WORK"

install -d "$DUMP_DIR/fsd_built"
wine "$PYTHON27" "$HERE/decode.py" \
    "$(winepath -w "$WORK/spaceComponentsByTypeLoader.pyd")" \
    "$(winepath -w "$WORK/spacecomponentsbytype.fsdbinary")" \
    "$(winepath -w "$DUMP_DIR/fsd_built/spacecomponentsbytype.json")"
