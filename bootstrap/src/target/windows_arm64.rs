//! Windows on arm64: a PE executable for the console that reaches the
//! kernel through `KERNEL32.dll`, which the system's loader links. Its
//! instructions are the arm64 ones the other arm64 targets use, and its
//! file is written by the PE writer `windows_x64` uses.

use super::Part;
use super::arm64::{self, Asm, EQ, LS, NE, Place, System, X0, X1, X2, X3, X4, X5, X19, X20, X21};
use super::pe::{
    self, ACCEPT, BIND, CLOSE_HANDLE, CLOSE_SOCKET, CONNECT, CREATE_FILE, EXIT_PROCESS,
    GET_COMMAND_LINE, GET_FILE_SIZE, GET_LAST_ERROR, GET_SOCK_NAME, GET_STD_HANDLE, LISTEN,
    MEM_COMMIT_RESERVE, MOST_PER_WRITE, Machine, PAGE_READWRITE, READ_FILE, RECV_FROM, SEND_TO,
    SET_SOCK_OPT, SOCKET, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, VIRTUAL_ALLOC,
    WRITE_FILE, WSA_GET_LAST_ERROR, WSA_STARTUP, process_prng, socket_slot,
};
use crate::scalar::{Program, Raw, Stream};

const MACHINE: Machine = Machine {
    machine: 0xaa64, // arm64
    // An executable image that can use addresses above 2 GiB. Windows on
    // arm64 loads an image at a place of its choosing, so the image does
    // not say its relocations are stripped. It needs none: the code
    // reaches its data and the import address table relative to itself.
    characteristics: 0x0002 | 0x0020,
    // Able to be loaded anywhere in a 64-bit address space, compatible
    // with data execution prevention, and aware of Terminal Server.
    dll_characteristics: 0x0020 | 0x0040 | 0x0100 | 0x8000,
    version: (6, 2), // Windows 8, the oldest a linker writes for arm64
};

/// The bytes at the bottom of each frame: the count `WriteFile` writes
/// back, and x19, x20, and x21, which the write keeps what it needs in and
/// puts back.
const RESERVED: u32 = 32;
const WRITTEN: u32 = 0;
const SAVED: u32 = 8;

/// The executable for the program.
pub fn emit(program: &Program) -> (Vec<u8>, Vec<Part>) {
    let net = program.asks_the_network();
    let files = program.asks_the_files();
    let (asm, mut data, parts) = match (net, program.reads(), files) {
        (true, _, true) => arm64::code::<Windows<true, true, true>>(program),
        (true, _, false) => arm64::code::<Windows<true, true, false>>(program),
        (false, true, true) => arm64::code::<Windows<true, false, true>>(program),
        (false, true, false) => arm64::code::<Windows<true, false, false>>(program),
        (false, false, true) => arm64::code::<Windows<false, false, true>>(program),
        (false, false, false) => arm64::code::<Windows<false, false, false>>(program),
    };
    let size = asm.len() + data.iter().map(|(_, d)| d.len()).sum::<usize>();
    let imports = pe::imports(
        program.allocates(),
        program.takes_arguments(),
        program.asks_for_entropy(),
        program.reads(),
        net,
        files,
    );
    // The image is loaded at a place of the system's choosing, so each address
    // among the data is written as if it were loaded where it asks to be, and
    // the loader moves each by as far as it moved the image.
    let code = asm.code_len();
    let load = pe::IMAGE_BASE + u64::from(pe::CODE_RVA);
    let moved = super::write_addresses(program, &mut data, code, load);
    let mut at = 0;
    let executable = pe::executable(&MACHINE, size, &imports, &moved, |code, iat| {
        at = code;
        asm.finish(&data, code, iat)
    });
    (executable, super::shifted(parts, pe::IMAGE_BASE + at))
}

/// Whether the program reads standard input, which moves the slot of
/// `ProcessPrng`, whether it carries `net`, which asks for the sockets
/// of `WS2_32.dll` and moves it again, and whether it asks for the files, which
/// imports more of `KERNEL32.dll` and moves them both.
struct Windows<const READS: bool, const NET: bool, const FILES: bool>;

impl<const READS: bool, const NET: bool, const FILES: bool> System for Windows<READS, NET, FILES> {
    const RESERVED: u32 = RESERVED;
    const STACK_BY_PAGE: bool = true;

    /// Writes every byte, retrying a write that is cut short, and branches
    /// to `fail` if the system refuses it or writes nothing. A call goes
    /// through the import address table, slot by slot. The place, the
    /// length still to write, and the handle are kept in x19, x20, and x21,
    /// which calls preserve.
    fn write(asm: &mut Asm, stream: Stream, fail: usize) {
        let again = asm.label();
        let done = asm.label();
        let failed = asm.label();
        for (i, r) in [X19, X20, X21].into_iter().enumerate() {
            asm.store(Place::Frame(SAVED + 8 * i as u32), r, 8);
        }
        asm.mov(X19, X1);
        asm.mov(X20, X2);
        asm.mov64(X0, u64::from(handle(stream)));
        asm.call(GET_STD_HANDLE);
        asm.mov(X21, X0);
        asm.bind(again);
        asm.cbz(X20, done);
        // WriteFile(handle, text, at most MOST_PER_WRITE bytes, &written, NULL)
        asm.mov64(X2, u64::from(MOST_PER_WRITE));
        asm.cmp(X20, X2);
        asm.csel(X2, X20, X2, LS);
        asm.mov(X0, X21);
        asm.mov(X1, X19);
        asm.stack_address(X3, WRITTEN);
        asm.mov_w(X4, 0);
        asm.call(WRITE_FILE);
        // It gives a BOOL, which is 32 bits.
        asm.cbz_w(X0, failed);
        asm.load(X0, 4, false, Place::Frame(WRITTEN));
        asm.cbz_w(X0, failed);
        // add x19, x19, x0; sub x20, x20, x0; b again
        asm.word(0x8b00_0000 | X0 << 16 | X19 << 5 | X19);
        asm.word(0xcb00_0000 | X0 << 16 | X20 << 5 | X20);
        asm.branch(again);
        // What the function kept in them is put back before it goes on, since a
        // landing that follows the write reads it.
        asm.bind(failed);
        for (i, r) in [X19, X20, X21].into_iter().enumerate() {
            asm.load(r, 8, false, Place::Frame(SAVED + 8 * i as u32));
        }
        asm.branch(fail);
        asm.bind(done);
        for (i, r) in [X19, X20, X21].into_iter().enumerate() {
            asm.load(r, 8, false, Place::Frame(SAVED + 8 * i as u32));
        }
    }

    /// Reads from standard input with `ReadFile`, which gives a `BOOL` and
    /// writes how many bytes it read. A read that fails is the end of the
    /// input, as `ReadFile` reports the end of a pipe that way, and one that
    /// reads nothing is. The place and the count are kept in x19 and x20,
    /// which calls preserve.
    fn read(asm: &mut Asm, _: usize) {
        let ended = asm.label();
        let done = asm.label();
        for (i, r) in [X19, X20].into_iter().enumerate() {
            asm.store(Place::Frame(SAVED + 8 * i as u32), r, 8);
        }
        asm.mov(X19, X1);
        asm.mov(X20, X2);
        asm.mov64(X0, u64::from(STD_INPUT_HANDLE));
        asm.call(GET_STD_HANDLE);
        // ReadFile(handle, bytes, at most MOST_PER_WRITE bytes, &read, NULL)
        asm.mov64(X2, u64::from(MOST_PER_WRITE));
        asm.cmp(X20, X2);
        asm.csel(X2, X20, X2, LS);
        asm.mov(X1, X19);
        asm.stack_address(X3, WRITTEN);
        asm.mov_w(X4, 0);
        asm.call(READ_FILE);
        asm.cbz_w(X0, ended);
        asm.load(X0, 4, false, Place::Frame(WRITTEN));
        asm.branch(done);
        asm.bind(ended);
        asm.mov_w(X0, 0);
        asm.bind(done);
        for (i, r) in [X19, X20].into_iter().enumerate() {
            asm.load(r, 8, false, Place::Frame(SAVED + 8 * i as u32));
        }
    }

    /// Asks `ProcessPrng` for every byte at once, which takes any count and
    /// gives a `BOOL` that is not 0 when it filled them. The address and the
    /// count are in x1 and x2.
    fn entropy(asm: &mut Asm) {
        let refused = asm.label();
        let end = asm.label();
        asm.mov(X0, X1);
        asm.mov(X1, X2);
        asm.call(process_prng(READS, NET, FILES));
        asm.cbz_w(X0, refused);
        asm.mov_w(X0, 0);
        asm.branch(end);
        asm.bind(refused);
        asm.mov_w(X0, 1);
        asm.bind(end);
    }

    fn end(asm: &mut Asm) {
        asm.call(EXIT_PROCESS);
    }

    /// Asks the system's sockets to start, once, before the first operation of
    /// `net`: `WSAStartup(0x0202, &data)`, with room on the stack for the
    /// bytes it writes.
    fn start(asm: &mut Asm) {
        if NET {
            asm.reserve_stack(512);
            asm.mov_w(X0, 0x0202);
            asm.stack_address(X1, 0);
            asm.call(socket_slot(WSA_STARTUP, FILES));
            asm.release_stack(512);
        }
    }

    /// The kernel for a program that carries `net` is Windows's, which is 2.
    /// A call is a function of `WS2_32.dll`, whose words are in x0 to x5, as
    /// the call takes them. A call that refuses gives -1, and `WSAGetLastError`
    /// says why, which the call gives as minus the number. A call that gives an
    /// `int` leaves the high half of x0 as it likes, so the low half is made a
    /// 64-bit number first. Those that give a socket give 64 bits.
    fn raw(asm: &mut Asm, call: Raw) {
        if call == Raw::Kind {
            asm.mov_w(X0, if NET || FILES { 2 } else { 0 });
            return;
        }
        if FILES && files_call(asm, call) {
            return;
        }
        let (function, int) = match call {
            Raw::Socket => (SOCKET, false),
            Raw::Accept => (ACCEPT, false),
            Raw::Bind => (BIND, true),
            Raw::Listen => (LISTEN, true),
            Raw::Connect => (CONNECT, true),
            Raw::GetSockName => (GET_SOCK_NAME, true),
            Raw::SetSockOpt => (SET_SOCK_OPT, true),
            Raw::SendTo => (SEND_TO, true),
            Raw::RecvFrom => (RECV_FROM, true),
            Raw::Close => (CLOSE_SOCKET, true),
            _ => {
                // The calls of the other kernels are asked where a scheduler is
                // built for every machine, and are never run here.
                assert!(
                    !matches!(call, Raw::ClockGetTime | Raw::NanoSleep),
                    "a target that has no such call refuses the program before it is built"
                );
                asm.mov64(X0, -38i64 as u64);
                return;
            }
        };
        asm.call(socket_slot(function, FILES));
        if int {
            // sxtw x0, w0
            asm.word(0x9340_7c00 | X0 << 5 | X0);
        }
        let given = asm.label();
        // cmn x0, #1; b.ne given
        asm.word(0xb100_041f);
        asm.cond(NE, given);
        asm.call(socket_slot(WSA_GET_LAST_ERROR, FILES));
        // sxtw x0, w0; neg x0, x0
        asm.word(0x9340_7c00 | X0 << 5 | X0);
        asm.word(0xcb00_03e0 | X0 << 16);
        asm.bind(given);
    }

    /// Reserves and commits fresh pages. A failure gives no address.
    fn alloc(asm: &mut Asm, fail: usize) {
        asm.mov_w(X0, 0);
        asm.mov64(X2, u64::from(MEM_COMMIT_RESERVE));
        asm.mov_w(X3, PAGE_READWRITE as u16);
        asm.call(VIRTUAL_ALLOC);
        asm.cbz(X0, fail);
    }

    fn arguments_on_entry(_: &mut Asm) {}

    /// The system gives one command line and no words, which a count of all
    /// ones says.
    fn arguments_by_call(asm: &mut Asm) {
        asm.call(GET_COMMAND_LINE);
        asm.mov(X2, X0);
        asm.mov64(X0, u64::MAX);
        asm.mov_w(X1, 0);
    }
}

fn handle(stream: Stream) -> u32 {
    match stream {
        Stream::Stdout => STD_OUTPUT_HANDLE,
        Stream::Stderr => STD_ERROR_HANDLE,
    }
}

const X6: u32 = 6;

/// `FILE_ATTRIBUTE_NORMAL` and `FILE_FLAG_BACKUP_SEMANTICS`, which opens a
/// directory too, so that what is read of it is refused as the other systems do.
const OPEN_FLAGS: u64 = 0x80 | 0x0200_0000;
/// The errors that say a read has nothing more: the end of the file, and a pipe
/// whose other end is closed.
const ERROR_HANDLE_EOF: u32 = 38;
const ERROR_BROKEN_PIPE: u32 = 109;

/// What a call of `KERNEL32.dll` that refused gives: minus the system's last
/// error, or 0 where the error is one of `ended`, which a read gives at the end
/// of what there is.
fn refused(asm: &mut Asm, ended: &[u32]) {
    let zero = asm.label();
    let done = asm.label();
    asm.call(GET_LAST_ERROR);
    // mov w0, w0, which gives the error as the 32-bit number it is
    asm.word(0x2a00_03e0);
    for &code in ended {
        // cmp x0, #code
        asm.word(0xf100_001f | code << 10);
        asm.cond(EQ, zero);
    }
    // neg x0, x0
    asm.word(0xcb00_03e0 | X0 << 16);
    asm.branch(done);
    asm.bind(zero);
    asm.mov_w(X0, 0);
    asm.bind(done);
}

/// Makes the call of a file that the words ask for, which are those of the
/// kernel of Linux, with `KERNEL32.dll`: `CreateFileW` with a path in UTF-16, an
/// access, and a disposition, `ReadFile` and `WriteFile`, which take a place by an
/// `OVERLAPPED`, `GetFileSizeEx` for the end of a file, and `CloseHandle`. It
/// gives what the call gave, or minus the system's last error, and says whether
/// the call was one of a file.
fn files_call(asm: &mut Asm, call: Raw) -> bool {
    let done = asm.label();
    let room = match call {
        Raw::OpenAt | Raw::CloseFile | Raw::LSeek => 16,
        Raw::PRead | Raw::PWrite | Raw::Read | Raw::Write => 64,
        Raw::FChmod => {
            // A file of this system has no mode to make it one that runs.
            asm.mov_w(X0, 0);
            return true;
        }
        _ => return false,
    };
    asm.reserve_stack(room);
    let failed = asm.label();
    match call {
        Raw::OpenAt => {
            // The words are the directory, the path, the access, and the disposition.
            asm.mov(X4, X3);
            asm.mov(X0, X1);
            asm.mov(X1, X2);
            asm.mov_w(X2, 7);
            asm.mov_w(X3, 0);
            asm.mov64(X5, OPEN_FLAGS);
            asm.mov_w(X6, 0);
            asm.call(CREATE_FILE);
            // cmn x0, #1; b.eq failed
            asm.word(0xb100_041f);
            asm.cond(EQ, failed);
            asm.branch(done);
            asm.bind(failed);
            refused(asm, &[]);
        }
        Raw::PRead | Raw::PWrite | Raw::Read | Raw::Write => {
            // The words are the handle, the bytes, the count, and the place.
            let places = matches!(call, Raw::PRead | Raw::PWrite);
            let reads = matches!(call, Raw::PRead | Raw::Read);
            if places {
                asm.mov_w(X5, 0);
                asm.store(Place::Frame(0), X5, 8);
                asm.store(Place::Frame(8), X5, 8);
                asm.store(Place::Frame(16), X3, 8);
                asm.store(Place::Frame(24), X5, 8);
            }
            asm.mov64(X5, u64::from(MOST_PER_WRITE));
            asm.cmp(X2, X5);
            asm.csel(X2, X2, X5, LS);
            asm.stack_address(X3, 32);
            if places {
                asm.stack_address(X4, 0);
            } else {
                asm.mov_w(X4, 0);
            }
            asm.call(if reads { READ_FILE } else { WRITE_FILE });
            asm.cbz_w(X0, failed);
            asm.load(X0, 4, false, Place::Frame(32));
            asm.branch(done);
            asm.bind(failed);
            if reads {
                refused(asm, &[ERROR_HANDLE_EOF, ERROR_BROKEN_PIPE]);
            } else {
                refused(asm, &[]);
            }
        }
        Raw::LSeek => {
            // The end of the file, which is what the kernel of Linux gives for a
            // place of nothing from the end.
            asm.stack_address(X1, 0);
            asm.call(GET_FILE_SIZE);
            asm.cbz_w(X0, failed);
            asm.load(X0, 8, false, Place::Frame(0));
            asm.branch(done);
            asm.bind(failed);
            refused(asm, &[]);
        }
        Raw::CloseFile => {
            asm.call(CLOSE_HANDLE);
            asm.cbz_w(X0, failed);
            asm.mov_w(X0, 0);
            asm.branch(done);
            asm.bind(failed);
            refused(asm, &[]);
        }
        _ => unreachable!("a call of a file"),
    }
    asm.bind(done);
    asm.release_stack(room);
    true
}
