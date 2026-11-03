#!/usr/bin/env python3
"""Build a portable x64 package and inspect its actual PE imports, offline."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import zipfile

ROOT = Path(__file__).resolve().parents[1]
# Windows 10/11 system libraries used by the fixed GPUI/Win32 adapter. New
# external DLLs require an explicit packaging decision, not the CI host's PATH.
SYSTEM_LIBRARIES = frozenset('''advapi32 bcrypt bcryptprimitives comctl32 comdlg32
crypt32 d2d1 d3d11 d3d12 d3dcompiler_47 dbghelp dcomp dwrite dwmapi dxgi gdi32
imm32 iphlpapi kernel32 msvcrt ncrypt netapi32 normaliz ntdll ole32 oleaut32
opengl32 powrprof propsys psapi rpcrt4 secur32 setupapi shcore shell32 shlwapi
icu icuuc icuin ucrtbase user32 userenv usp10 uxtheme version winhttp wininet winmm winspool
wintrust ws2_32 wtsapi32 runtimeobject combase cfgmgr32 hid avrt oleacc
'''.split())


def pe_dependencies(executable: Path) -> dict:
    data = executable.read_bytes()

    def unpack(fmt: str, offset: int):
        size = struct.calcsize(fmt)
        if offset < 0 or offset + size > len(data):
            raise ValueError('Truncated PE structure')
        return struct.unpack_from(fmt, data, offset)

    if data[:2] != b'MZ':
        raise ValueError('Executable is not a Windows PE file')
    pe, = unpack('<I', 0x3c)
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Invalid PE signature')
    machine, section_count = unpack('<HH', pe + 4)
    optional_size, = unpack('<H', pe + 20)
    optional = pe + 24
    magic, = unpack('<H', optional)
    if machine != 0x8664 or magic != 0x20b:
        raise ValueError('This delivery requires a Windows x64 PE32+ executable')
    subsystem, = unpack('<H', optional + 68)
    sections = []
    for index in range(section_count):
        offset = optional + optional_size + index * 40
        virtual_size, virtual_address, raw_size, raw_offset = unpack('<IIII', offset + 8)
        sections.append((virtual_address, max(virtual_size, raw_size), raw_offset, raw_size))

    def file_offset(rva: int) -> int:
        for address, size, raw, raw_size in sections:
            if address <= rva < address + size:
                delta = rva - address
                if delta >= raw_size:
                    raise ValueError('PE import references an uninitialized section')
                return raw + delta
        raise ValueError(f'Unmapped PE import RVA: {rva:#x}')

    imports = set()
    import_rva, import_size = unpack('<II', optional + 112 + 8)
    if import_rva:
        start = file_offset(import_rva)
        for offset in range(start, start + import_size, 20):
            descriptor = unpack('<IIIII', offset)
            if not any(descriptor):
                break
            name_offset = file_offset(descriptor[3])
            end = data.find(b'\0', name_offset, name_offset + 260)
            if end < 0:
                raise ValueError('Unterminated PE import name')
            imports.add(data[name_offset:end].decode('ascii').lower())
    return {'machine': 'x86_64', 'subsystem': {2: 'windows-gui', 3: 'windows-console'}.get(subsystem, f'unknown-{subsystem}'), 'imports': sorted(imports)}


def package(executable: Path, output: Path, revision: str) -> Path:
    metadata = pe_dependencies(executable)
    if metadata['subsystem'] != 'windows-gui':
        raise ValueError('GUI subsystem required: the application must not open a console')
    external = [name for name in metadata['imports']
                if not name.startswith(('api-ms-win-', 'ext-ms-win-'))
                and name.removesuffix('.dll') not in SYSTEM_LIBRARIES]
    if external:
        raise ValueError('Unbundled non-system dependencies: ' + ', '.join(external))
    folder = output / 'altaed-windows-x86_64'
    folder.mkdir(parents=True, exist_ok=False)
    shutil.copy2(executable, folder / 'altaed.exe')
    shutil.copy2(ROOT / 'docs/windows-portable.md', folder / 'README.md')
    shutil.copy2(ROOT / 'THIRD_PARTY_NOTICES.md', folder)
    licenses = folder / 'licenses'
    licenses.mkdir()
    shutil.copy2(ROOT / 'crates/altaed/languages/LICENSE-GPL', licenses)
    shutil.copy2(ROOT / 'crates/altaed/assets/LICENSE-IBM-PLEX.txt', licenses)
    for source in sorted((ROOT / 'crates/altaed/assets/themes/sources').iterdir()):
        if not source.is_dir():
            continue
        destination = licenses / source.name
        destination.mkdir()
        for path in source.iterdir():
            if path.name.startswith('LICENSE') or path.name == 'SOURCE.json':
                shutil.copy2(path, destination)
    metadata['revision'] = revision
    (folder / 'build.json').write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
    files = sorted(path for path in folder.rglob('*') if path.is_file())
    (folder / 'SHA256SUMS.txt').write_text(''.join(
        hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.relative_to(folder).as_posix() + '\n'
        for path in files), encoding='utf-8')
    archive = output / (folder.name + '.zip')
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(folder.rglob('*')):
            if path.is_file():
                bundle.write(path, path.relative_to(output))
    archive.with_suffix('.zip.sha256').write_text(
        hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n', encoding='utf-8')
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist/windows')
    parser.add_argument('--revision', required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[0-9a-f]{40}', args.revision):
        parser.error('revision must be a full Git SHA')
    print(package(args.executable, args.output, args.revision))


if __name__ == '__main__':
    main()
