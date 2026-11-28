//! The PE executable that both Windows targets write: an image for the
//! console that reaches the kernel through `KERNEL32.dll`, which the
//! system's loader links.
//!
//! From `KERNEL32.dll` the program uses only thin wrappers over the kernel:
//! `GetStdHandle`, `WriteFile`, `ExitProcess`, and, in a program that reads
//! standard input, `ReadFile`, and, in a program that asks
//! for memory, `VirtualAlloc`, and, in a program that takes the words it was
//! started with, `GetCommandLineW`. In a program that asks for random bytes it
//! uses `ProcessPrng` of `bcryptprimitives.dll`, the system's generator of
//! them, which is the one thing it takes from another library. It uses no C
//! runtime.
//! Nothing from the system is needed to build: the import tables that ask
//! the loader for the library and its symbols are written here.

/// Where the image asks to be loaded.
/// Where the executable asks to be loaded.
pub const IMAGE_BASE: u64 = 0x1_4000_0000;
/// Sections start on this boundary in memory.
const SECTION_ALIGNMENT: u32 = 0x1000;
/// Where the code is, as an offset from the image's base: the first section.
pub const CODE_RVA: u32 = SECTION_ALIGNMENT;
/// Sections start on this boundary in the file.
const FILE_ALIGNMENT: u32 = 0x200;

/// Where the PE signature is, just after the DOS header.
const PE_OFFSET: u32 = 0x40;
const COFF_HEADER_SIZE: u32 = 20;
const OPTIONAL_HEADER_SIZE: u32 = 240;
const SECTION_HEADER_SIZE: u32 = 40;
/// The most sections an image has: the code, the imports, and the
/// relocations of an image that has any.
const SECTIONS: u32 = 3;
/// The headers, which fill the start of the file up to the first section.
const HEADERS_SIZE: u32 =
    (PE_OFFSET + 4 + COFF_HEADER_SIZE + OPTIONAL_HEADER_SIZE + SECTIONS * SECTION_HEADER_SIZE)
        .next_multiple_of(FILE_ALIGNMENT);

const DLL: &[u8] = b"KERNEL32.dll";
/// The symbols a program may import, in the order of their slots in the
/// import address table. A program that never asks for memory imports only
/// the first three, and one that asks for memory and takes no words imports
/// the first four.
const IMPORTS: [&str; 6] = [
    "GetStdHandle",
    "WriteFile",
    "ExitProcess",
    "VirtualAlloc",
    "GetCommandLineW",
    "ReadFile",
];
/// The symbols of `KERNEL32.dll` of a program that reads or writes files, which
/// are those of `IMPORTS` and then the four that the files ask for, in the order
/// of their slots.
const ALL_IMPORTS: [&str; 10] = [
    "GetStdHandle",
    "WriteFile",
    "ExitProcess",
    "VirtualAlloc",
    "GetCommandLineW",
    "ReadFile",
    "CreateFileW",
    "CloseHandle",
    "GetFileSizeEx",
    "GetLastError",
];
pub const CREATE_FILE: usize = 6;
pub const CLOSE_HANDLE: usize = 7;
pub const GET_FILE_SIZE: usize = 8;
pub const GET_LAST_ERROR: usize = 9;
/// How many symbols of `KERNEL32.dll` a program imports when it takes every one
/// of them, which one that carries `net` or asks for the files does.
pub const fn kernel_len(files: bool) -> usize {
    if files {
        ALL_IMPORTS.len()
    } else {
        IMPORTS.len()
    }
}
pub const GET_STD_HANDLE: usize = 0;
pub const WRITE_FILE: usize = 1;
pub const EXIT_PROCESS: usize = 2;
pub const VIRTUAL_ALLOC: usize = 3;
pub const GET_COMMAND_LINE: usize = 4;
pub const READ_FILE: usize = 5;
/// The library of `ProcessPrng`, which fills bytes with bytes nobody can
/// predict and always succeeds.
const PRNG_DLL: &[u8] = b"bcryptprimitives.dll";
const PRNG: &str = "ProcessPrng";
/// The library of the sockets, which only a program that carries `net` asks
/// the loader for, and the functions of it that such a program calls, in the
/// order of their slots. They are the thin wrappers over the system's sockets:
/// `WSAStartup`, which a program calls once before its first operation of `net`,
/// `WSAGetLastError`, which says why a call refused, and one for each call of
/// the kernel that `net` asks for.
const SOCKETS_DLL: &[u8] = b"WS2_32.dll";
const SOCKETS: [&str; 12] = [
    "WSAStartup",
    "WSAGetLastError",
    "socket",
    "bind",
    "listen",
    "accept",
    "connect",
    "getsockname",
    "setsockopt",
    "sendto",
    "recvfrom",
    "closesocket",
];
pub const WSA_STARTUP: usize = 0;
pub const WSA_GET_LAST_ERROR: usize = 1;
pub const SOCKET: usize = 2;
pub const BIND: usize = 3;
pub const LISTEN: usize = 4;
pub const ACCEPT: usize = 5;
pub const CONNECT: usize = 6;
pub const GET_SOCK_NAME: usize = 7;
pub const SET_SOCK_OPT: usize = 8;
pub const SEND_TO: usize = 9;
pub const RECV_FROM: usize = 10;
pub const CLOSE_SOCKET: usize = 11;
/// The slot of a function of `WS2_32.dll`. A program that carries `net` imports
/// every symbol of `KERNEL32.dll`, so the library of sockets is the second, after
/// the slot that ends the first.
pub const fn socket_slot(function: usize, files: bool) -> usize {
    kernel_len(files) + 1 + function
}
/// `ProcessPrng`'s slot in the import address table, which follows the slots
/// of `KERNEL32.dll` and the one that ends them, and, in a program that
/// carries `net`, those of `WS2_32.dll` and the one that ends them. A program that
/// asks for it imports the first five symbols of `KERNEL32.dll`, and
/// `ReadFile` too when it reads standard input, and all of them when it
/// carries `net`.
pub const fn process_prng(reads: bool, net: bool, files: bool) -> usize {
    if net {
        kernel_len(files) + 1 + SOCKETS.len() + 1
    } else if files {
        ALL_IMPORTS.len() + 1
    } else if reads {
        IMPORTS.len() + 1
    } else {
        IMPORTS.len()
    }
}

/// What a program imports.
pub struct Needs {
    /// The symbols of `KERNEL32.dll`.
    pub kernel: &'static [&'static str],
    /// Whether it imports `ProcessPrng`.
    pub prng: bool,
    /// Whether it imports the sockets of `WS2_32.dll`.
    pub net: bool,
}

/// The symbols a program imports: `VirtualAlloc` only if it asks for memory,
/// and `GetCommandLineW` only if it takes the words it was started with,
/// which asks for memory too, and `ProcessPrng` only if it asks for random
/// bytes, which asks for memory to say that it could not get them, and so
/// imports the first five, and `ReadFile` only if it reads standard input,
/// which asks for memory to hold what it reads, and so imports all. The
/// sockets are asked of `WS2_32.dll` only by a program that carries `net`,
/// which asks for memory to build what it reads, and so imports all.
pub fn imports(
    allocates: bool,
    takes_arguments: bool,
    entropy: bool,
    reads: bool,
    net: bool,
    files: bool,
) -> Needs {
    let kernel: &'static [&'static str] = if files {
        &ALL_IMPORTS
    } else if reads || net {
        &IMPORTS
    } else if takes_arguments || entropy {
        &IMPORTS[..5]
    } else if allocates {
        &IMPORTS[..4]
    } else {
        &IMPORTS[..3]
    };
    Needs {
        kernel,
        prng: entropy,
        net,
    }
}

/// `VirtualAlloc`'s request to reserve and commit pages at once, and its
/// protection for pages that are read and written.
pub const MEM_COMMIT_RESERVE: u32 = 0x1000 | 0x2000;
pub const PAGE_READWRITE: u32 = 0x04;

/// `GetStdHandle`'s names for standard input, standard output, and standard
/// error.
pub const STD_INPUT_HANDLE: u32 = -10i32 as u32;
pub const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
pub const STD_ERROR_HANDLE: u32 = -12i32 as u32;
/// The most one `WriteFile` is asked to write.
pub const MOST_PER_WRITE: u32 = 1 << 30;

/// What differs between the machines a PE executable is for.
pub struct Machine {
    /// The machine field of the COFF header.
    pub machine: u16,
    /// The characteristics of the COFF header.
    pub characteristics: u16,
    /// The DLL characteristics of the optional header.
    pub dll_characteristics: u16,
    /// The oldest Windows it runs on, as a major and a minor version.
    pub version: (u16, u16),
}

/// The import tables, laid out from the start of their section.
struct Imports {
    bytes: Vec<u8>,
    /// Where the import address table is, which the loader fills.
    iat: u32,
    iat_size: u32,
    directory_size: u32,
}

/// The executable, whose code and data take `text_size` bytes, which
/// imports `needs`. `text` is given where the code is and where the import
/// address table is, each as an offset from the image's base, and gives the
/// code and its data. The loader moves each address at a place of `moved`,
/// counted from the start of the code, by as far as it moved the image.
pub fn executable(
    machine: &Machine,
    text_size: usize,
    needs: &Needs,
    moved: &[u64],
    text: impl FnOnce(u64, u64) -> Vec<u8>,
) -> Vec<u8> {
    let text_rva = CODE_RVA;
    let idata_rva = (text_rva + text_size as u32).next_multiple_of(SECTION_ALIGNMENT);
    let imports = import_tables(idata_rva, needs);
    let iat_rva = idata_rva + imports.iat;
    let text = text(u64::from(text_rva), u64::from(iat_rva));
    assert_eq!(text.len(), text_size);

    let text_raw = (text.len() as u32).next_multiple_of(FILE_ALIGNMENT);
    let idata_raw = (imports.bytes.len() as u32).next_multiple_of(FILE_ALIGNMENT);
    let reloc_rva = idata_rva + (imports.bytes.len() as u32).next_multiple_of(SECTION_ALIGNMENT);
    let relocations = relocations(text_rva, moved);
    let reloc_raw = (relocations.len() as u32).next_multiple_of(FILE_ALIGNMENT);
    let image_size = reloc_rva + (relocations.len() as u32).next_multiple_of(SECTION_ALIGNMENT);
    let mut sections = vec![
        Section {
            name: *b".text\0\0\0",
            size: text.len() as u32,
            rva: text_rva,
            raw_size: text_raw,
            raw_offset: HEADERS_SIZE,
            // Code, readable and executable.
            characteristics: 0x6000_0020,
        },
        Section {
            name: *b".idata\0\0",
            size: imports.bytes.len() as u32,
            rva: idata_rva,
            raw_size: idata_raw,
            raw_offset: HEADERS_SIZE + text_raw,
            // Initialized data, readable and writable.
            characteristics: 0xc000_0040,
        },
    ];
    if !relocations.is_empty() {
        sections.push(Section {
            name: *b".reloc\0\0",
            size: relocations.len() as u32,
            rva: reloc_rva,
            raw_size: reloc_raw,
            raw_offset: HEADERS_SIZE + text_raw + idata_raw,
            // Initialized data, readable, and not needed once it is loaded.
            characteristics: 0x4200_0040,
        });
    }

    let mut out = Vec::with_capacity((HEADERS_SIZE + text_raw + idata_raw + reloc_raw) as usize);
    dos_header(&mut out);
    out.extend_from_slice(b"PE\0\0");
    coff_header(&mut out, machine, sections.len() as u16);
    let optional = OptionalHeader {
        code_size: text_raw,
        data_size: idata_raw + reloc_raw,
        entry: text_rva,
        image_size,
        imports: (idata_rva, imports.directory_size),
        iat: (iat_rva, imports.iat_size),
        relocations: match relocations.len() {
            0 => (0, 0),
            size => (reloc_rva, size as u32),
        },
    };
    optional_header(&mut out, machine, &optional);
    for section in &sections {
        section_header(&mut out, section);
    }
    out.resize(HEADERS_SIZE as usize, 0);
    out.extend_from_slice(&text);
    out.resize((HEADERS_SIZE + text_raw) as usize, 0);
    out.extend_from_slice(&imports.bytes);
    out.resize((HEADERS_SIZE + text_raw + idata_raw) as usize, 0);
    out.extend_from_slice(&relocations);
    out.resize(
        (HEADERS_SIZE + text_raw + idata_raw + reloc_raw) as usize,
        0,
    );
    out
}

/// The base relocations that move each 8-byte address at a place of `moved`,
/// counted from `text_rva`: a block for each page that holds one, which names
/// the page and then each place in it, each block a whole number of 4 bytes.
fn relocations(text_rva: u32, moved: &[u64]) -> Vec<u8> {
    /// A relocation that adds the distance to all 64 bits of the place.
    const DIR64: u16 = 10 << 12;
    let mut rvas: Vec<u32> = moved.iter().map(|&at| text_rva + at as u32).collect();
    rvas.sort_unstable();
    let mut out = Vec::new();
    let mut rest = &rvas[..];
    while let Some(&first) = rest.first() {
        let page = first & !0xfff;
        let count = rest.partition_point(|&rva| rva & !0xfff == page);
        let entries = count.next_multiple_of(2);
        put32(&mut out, page);
        put32(&mut out, 8 + 2 * entries as u32);
        for &rva in &rest[..count] {
            put16(&mut out, DIR64 | (rva & 0xfff) as u16);
        }
        // An entry of type 0 does nothing, and pads the block.
        out.resize(out.len() + 2 * (entries - count), 0);
        rest = &rest[count..];
    }
    out
}

fn put16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// The DOS header, which names where the PE header is and holds no
/// program of its own.
fn dos_header(out: &mut Vec<u8>) {
    out.extend_from_slice(b"MZ");
    out.resize(0x3c, 0);
    put32(out, PE_OFFSET);
}

fn coff_header(out: &mut Vec<u8>, machine: &Machine, sections: u16) {
    put16(out, machine.machine);
    put16(out, sections);
    put32(out, 0); // no time stamp, so the bytes are the same every build
    put32(out, 0); // no symbol table
    put32(out, 0); // no symbols
    put16(out, OPTIONAL_HEADER_SIZE as u16);
    put16(out, machine.characteristics);
}

struct OptionalHeader {
    code_size: u32,
    data_size: u32,
    entry: u32,
    image_size: u32,
    imports: (u32, u32),
    iat: (u32, u32),
    relocations: (u32, u32),
}

fn optional_header(out: &mut Vec<u8>, machine: &Machine, header: &OptionalHeader) {
    let start = out.len();
    put16(out, 0x20b); // PE32+
    out.extend_from_slice(&[0, 0]); // linker version
    put32(out, header.code_size);
    put32(out, header.data_size);
    put32(out, 0); // no uninitialized data
    put32(out, header.entry);
    put32(out, header.entry); // where the code starts
    put64(out, IMAGE_BASE);
    put32(out, SECTION_ALIGNMENT);
    put32(out, FILE_ALIGNMENT);
    let (major, minor) = machine.version;
    put16(out, major); // the operating system's version
    put16(out, minor);
    put16(out, 0); // image version
    put16(out, 0);
    put16(out, major); // subsystem version
    put16(out, minor);
    put32(out, 0); // reserved
    put32(out, header.image_size);
    put32(out, HEADERS_SIZE);
    put32(out, 0); // no checksum
    put16(out, 3); // the console subsystem
    put16(out, machine.dll_characteristics);
    put64(out, 0x10_0000); // stack reserved
    put64(out, 0x1000); // stack committed
    put64(out, 0x10_0000); // heap reserved
    put64(out, 0x1000); // heap committed
    put32(out, 0); // loader flags
    put32(out, 16); // data directories
    for i in 0..16 {
        let (rva, size) = match i {
            1 => header.imports,
            5 => header.relocations,
            12 => header.iat,
            _ => (0, 0),
        };
        put32(out, rva);
        put32(out, size);
    }
    assert_eq!(out.len() - start, OPTIONAL_HEADER_SIZE as usize);
}

struct Section {
    name: [u8; 8],
    size: u32,
    rva: u32,
    raw_size: u32,
    raw_offset: u32,
    characteristics: u32,
}

fn section_header(out: &mut Vec<u8>, section: &Section) {
    out.extend_from_slice(&section.name);
    put32(out, section.size);
    put32(out, section.rva);
    put32(out, section.raw_size);
    put32(out, section.raw_offset);
    put32(out, 0); // no relocations
    put32(out, 0); // no line numbers
    put16(out, 0);
    put16(out, 0);
    put32(out, section.characteristics);
}

/// A library the program imports, and the symbols of it.
struct Library {
    dll: &'static [u8],
    symbols: Vec<&'static str>,
}

/// The import directory, with one entry for `KERNEL32.dll`, one for the
/// library of the sockets when a program carries `net`, one for the library of
/// `ProcessPrng` when it is needed, and one that ends it; the lookup tables, then
/// the address tables, which follow one another so that a slot of one is
/// counted on from the one before, each a slot for every symbol and one that
/// ends it; then each symbol's hint and name, and the libraries' names. `rva` is
/// where the section is in memory.
fn import_tables(rva: u32, needs: &Needs) -> Imports {
    let mut libraries = vec![Library {
        dll: DLL,
        symbols: needs.kernel.to_vec(),
    }];
    if needs.net {
        libraries.push(Library {
            dll: SOCKETS_DLL,
            symbols: SOCKETS.to_vec(),
        });
    }
    if needs.prng {
        libraries.push(Library {
            dll: PRNG_DLL,
            symbols: vec![PRNG],
        });
    }
    let directory_size = (libraries.len() as u32 + 1) * 20;
    let sizes: Vec<u32> = libraries
        .iter()
        .map(|l| 8 * (l.symbols.len() as u32 + 1))
        .collect();
    let total: u32 = sizes.iter().sum();
    // The tables of 8-byte entries begin on a multiple of 8.
    let lookup = directory_size.next_multiple_of(8);
    let iat = lookup + total;
    let names_start = iat + total;
    let mut names = Vec::new();
    let mut name_offsets: Vec<Vec<u32>> = Vec::new();
    for library in &libraries {
        let mut offsets = Vec::new();
        for name in &library.symbols {
            offsets.push(names_start + names.len() as u32);
            names.extend_from_slice(&[0, 0]); // no hint
            names.extend_from_slice(name.as_bytes());
            names.push(0);
            if names.len() % 2 == 1 {
                names.push(0);
            }
        }
        name_offsets.push(offsets);
    }
    let mut dll_names = Vec::new();
    for library in &libraries {
        dll_names.push(names_start + names.len() as u32);
        names.extend_from_slice(library.dll);
        names.push(0);
    }

    let mut bytes = Vec::new();
    let mut at = 0;
    for ((size, dll), _) in sizes.iter().zip(&dll_names).zip(&libraries) {
        put32(&mut bytes, rva + lookup + at);
        put32(&mut bytes, 0); // no time stamp
        put32(&mut bytes, 0); // no forwarders
        put32(&mut bytes, rva + dll);
        put32(&mut bytes, rva + iat + at);
        at += size;
    }
    bytes.resize(lookup as usize, 0);
    for _ in 0..2 {
        for offsets in &name_offsets {
            for offset in offsets {
                put64(&mut bytes, u64::from(rva + offset));
            }
            put64(&mut bytes, 0);
        }
    }
    bytes.extend_from_slice(&names);
    Imports {
        bytes,
        iat,
        iat_size: total,
        directory_size,
    }
}
