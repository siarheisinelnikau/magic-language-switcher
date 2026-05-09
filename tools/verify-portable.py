"""Check a Windows x64 GUI PE for unexpected non-system DLL dependencies."""
from pathlib import Path
import struct
import sys

SYSTEM_DLLS = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "comctl32.dll", "comdlg32.dll",
    "crypt32.dll", "dbghelp.dll", "dwmapi.dll", "gdi32.dll", "imm32.dll", "kernel32.dll",
    "msimg32.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "rpcrt4.dll", "secur32.dll",
    "shell32.dll", "shlwapi.dll", "user32.dll", "userenv.dll", "uxtheme.dll", "version.dll",
    "winhttp.dll", "winmm.dll", "ws2_32.dll",
}


def inspect(path: Path) -> list[str]:
    data = path.read_bytes()
    if data[:2] != b"MZ":
        raise ValueError("Missing DOS signature")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("Missing PE signature")
    machine, sections = struct.unpack_from("<HH", data, pe + 4)
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    if machine != 0x8664 or struct.unpack_from("<H", data, optional)[0] != 0x20B:
        raise ValueError("Expected a Windows x64 PE32+ executable")
    if struct.unpack_from("<H", data, optional + 68)[0] != 2:
        raise ValueError("Expected a GUI executable without a console window")
    headers_size = struct.unpack_from("<I", data, optional + 60)[0]
    table = optional + optional_size

    def offset(rva: int) -> int:
        if rva < headers_size:
            return rva
        for index in range(sections):
            virtual_size, virtual_address, raw_size, raw_pointer = struct.unpack_from("<IIII", data, table + index * 40 + 8)
            if virtual_address <= rva < virtual_address + min(virtual_size, raw_size):
                return raw_pointer + rva - virtual_address
        raise ValueError(f"Unmapped RVA: {rva:#x}")

    imports_rva, imports_size = struct.unpack_from("<II", data, optional + 120)
    imports = []
    for index in range(min(imports_size // 20 + 1, 256)):
        descriptor = struct.unpack_from("<IIIII", data, offset(imports_rva) + index * 20)
        if not any(descriptor):
            break
        start = offset(descriptor[3])
        end = data.index(0, start, start + 512)
        imports.append(data[start:end].decode("ascii").lower())
    if not imports:
        raise ValueError("No import descriptors found")
    unexpected = [name for name in imports if name not in SYSTEM_DLLS and not name.startswith(("api-ms-win-", "ext-ms-win-"))]
    if unexpected:
        raise ValueError("Unexpected external DLL dependency: " + ", ".join(unexpected))
    return sorted(set(imports))


if __name__ == "__main__":
    try:
        binary = Path(sys.argv[1])
        dependencies = inspect(binary)
        print(f"PASS: {binary.name} ({binary.stat().st_size:,} bytes), x64 GUI, Windows system DLLs only")
        print("Imports: " + ", ".join(dependencies))
    except (IndexError, OSError, ValueError, struct.error) as error:
        raise SystemExit(f"Portable EXE validation failed: {error}") from error
