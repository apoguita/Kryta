"""Make a client load Kryta.dll by itself, for patch-gw.py.

The patch on disk does two things only: the connection patch of patch-gw.py, and this - one entry in
the executable's import table, so Windows loads Kryta.dll when the client starts, with no launcher
(docs/port/22-client-platform.md). Everything else Kryta adds to the client is in the DLL, which
finds the client's functions by byte pattern while the client runs. So nothing here depends on the
client's build: no address, no machine code, no check of which Gw.exe this is.

The import table is rebuilt in a new section because the old one has no room for another entry:
the existing entries are copied and keep pointing at their own names and address tables, and one
entry is added for the DLL. This is the import half of what the first title patch did
(tools/client/title_patch.py at commit 7679ba1, `imports` and `patch_titles`), with the title hooks
and the single-build check left out.
"""

import os
from pathlib import Path
import shutil
import struct
import subprocess

import pefile


DLL_NAME = "Kryta.dll"
# The one symbol the client imports. Importing a symbol is what makes Windows load the DLL; the
# function does nothing (tools/client-dll/src/lib.rs, `kryta_client`).
EXPORTS = (b"kryta_client",)
TARGET = "i686-pc-windows-msvc"
SECTION_NAME = b".kryta"


def align(size, boundary):
    return (size + boundary - 1) // boundary * boundary


def import_table(pe, rva):
    """The client's import entries followed by one for the DLL, laid out at `rva`."""
    descriptors = b"".join(entry.struct.__pack__() for entry in pe.DIRECTORY_ENTRY_IMPORT)
    data = bytearray(descriptors + bytes(40))  # the DLL's entry, and the empty entry that ends the table

    def append(blob):
        address = rva + len(data)
        data.extend(blob)
        return address

    name = append(DLL_NAME.encode("ascii") + b"\0")
    symbols = []
    for export in EXPORTS:
        data.extend(bytes(-len(data) % 2))
        symbols.append(append(b"\0\0" + export + b"\0"))
    data.extend(bytes(-len(data) % 4))
    thunks = struct.pack(f"<{len(symbols) + 1}I", *symbols, 0)
    lookup, address_table = append(thunks), append(thunks)
    struct.pack_into("<IIIII", data, len(descriptors), lookup, 0, 0, name, address_table)
    return len(descriptors) + 40, data


def add_import(source):
    """`source` with Kryta.dll added to its import table. Returns the new file's bytes."""
    pe = pefile.PE(data=source)
    if any(entry.dll.lower() == DLL_NAME.lower().encode() for entry in pe.DIRECTORY_ENTRY_IMPORT):
        raise ValueError(f"this file already imports {DLL_NAME}; patch the stock Gw.exe")
    optional = pe.OPTIONAL_HEADER
    security = optional.DATA_DIRECTORY[4]

    # The only data allowed after the last section is the file's signature, which is dropped: a
    # changed file no longer matches it.
    raw_end = pe.get_overlay_data_start_offset()
    if raw_end is None:
        raw_end = len(source)
    elif raw_end != security.VirtualAddress or raw_end + security.Size != len(source):
        raise ValueError("unexpected data after the client sections")
    if raw_end % optional.FileAlignment:
        raise ValueError("unexpected client section alignment")

    header = pe.sections[-1].get_file_offset() + 40
    if header + 40 > optional.SizeOfHeaders or any(source[header:header + 40]):
        raise ValueError("no room for one more section header")

    rva = optional.SizeOfImage
    table_size, data = import_table(pe, rva)
    raw_size = align(len(data), optional.FileAlignment)

    image = bytearray(source[:raw_end])
    # Initialized data, readable and writable: Windows writes the DLL's addresses into the table.
    struct.pack_into("<8sIIIIIIHHI", image, header, SECTION_NAME, len(data), rva, raw_size,
                     len(image), 0, 0, 0, 0, 0xC0000040)
    image.extend(data)
    image.extend(bytes(raw_size - len(data)))

    struct.pack_into("<II", image, optional.DATA_DIRECTORY[1].get_file_offset(), rva, table_size)
    struct.pack_into("<II", image, security.get_file_offset(), 0, 0)
    struct.pack_into("<H", image, pe.FILE_HEADER.get_field_absolute_offset("NumberOfSections"),
                     pe.FILE_HEADER.NumberOfSections + 1)
    for field, value in (
        ("SizeOfImage", align(rva + len(data), optional.SectionAlignment)),
        ("SizeOfInitializedData", optional.SizeOfInitializedData + raw_size),
        ("CheckSum", 0),
    ):
        struct.pack_into("<I", image, optional.get_field_absolute_offset(field), value)
    return bytes(image)


def build_dll():
    """Build Kryta.dll from tools/client-dll with cargo, for the 32-bit client. Returns its bytes."""
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo.exe")
    if not Path(cargo).is_file():
        raise ValueError("cargo was not found; install Rust, or pass a built DLL with --dll")
    try:
        subprocess.run([cargo, "build", "-p", "kryta-client", "--release", "--target", TARGET],
                       cwd=root, check=True)
    except subprocess.CalledProcessError as error:
        raise ValueError(
            f"Kryta.dll could not be built ({error}). It is built for the 32-bit client: "
            f"`rustup target add {TARGET}` installs that target."
        ) from error
    target = Path(os.environ.get("CARGO_TARGET_DIR", root / "target"))
    return (target / TARGET / "release" / "kryta_client.dll").read_bytes()


def validate_dll(image):
    """Refuse a file that is not a 32-bit DLL exporting what the client imports."""
    with pefile.PE(data=image) as pe:
        directory = getattr(pe, "DIRECTORY_ENTRY_EXPORT", None)
        exports = {symbol.name for symbol in directory.symbols} if directory else set()
        if pe.FILE_HEADER.Machine != 0x14C or not pe.is_dll() or not set(EXPORTS) <= exports:
            raise ValueError(f"expected a 32-bit {DLL_NAME} that exports kryta_client")
