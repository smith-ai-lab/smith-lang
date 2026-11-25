//! Windows on x86-64: a PE executable for the console that reaches the
//! kernel through `KERNEL32.dll`, which the system's loader links. The
//! executable is written by `pe`.

use super::Part;
use super::pe::{
    self, ACCEPT, BIND, CLOSE_SOCKET, CONNECT, EXIT_PROCESS, GET_COMMAND_LINE, GET_SOCK_NAME,
    GET_STD_HANDLE, LISTEN, MEM_COMMIT_RESERVE, MOST_PER_WRITE, Machine, PAGE_READWRITE, READ_FILE,
    RECV_FROM, SEND_TO, SET_SOCK_OPT, SOCKET, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE, VIRTUAL_ALLOC, WRITE_FILE, WSA_GET_LAST_ERROR, WSA_STARTUP, process_prng,
    socket_slot,
};
use super::x86_64::{
    self, Asm, NE, Place, R8, R9, R10, RAX, RBX, RCX, RDI, RDX, RSI, Reg, System, Z,
};
use crate::scalar::{Program, Raw, Stream};

const MACHINE: Machine = Machine {
    machine: 0x8664, // x86-64
    // An executable image with no relocations, that can use addresses
    // above 2 GiB.
    characteristics: 0x0001 | 0x0002 | 0x0020,
    // Compatible with data execution prevention, and aware of Terminal
    // Server.
    dll_characteristics: 0x0100 | 0x8000,
    version: (6, 0), // Vista
};

/// The bytes at the bottom of each frame: 32 the callee may use, the fifth
/// argument of `WriteFile`, the count it writes back, and rbx, which the
/// write keeps the handle in and puts back.
const RESERVED: u32 = 64;
const FIFTH_ARGUMENT: i8 = 32;
const WRITTEN: i8 = 40;
const RBX_AT: u32 = 48;

/// The executable for the program.
pub fn emit(program: &Program) -> (Vec<u8>, Vec<Part>) {
    let net = program.asks_the_network();
    let (asm, data, parts) = if net {
        x86_64::code::<Windows<true, true>>(program)
    } else if program.reads() {
        x86_64::code::<Windows<true, false>>(program)
    } else {
        x86_64::code::<Windows<false, false>>(program)
    };
    let size = asm.len() + data.iter().map(|(_, d)| d.len()).sum::<usize>();
    let imports = pe::imports(
        program.allocates(),
        program.takes_arguments(),
        program.asks_for_entropy(),
        program.reads(),
        net,
    );
    let mut at = 0;
    let executable = pe::executable(&MACHINE, size, &imports, |code, iat| {
        at = code;
        asm.finish(&data, code, iat)
    });
    (executable, super::shifted(parts, pe::IMAGE_BASE + at))
}

/// Whether the program reads standard input, which moves the slot of
/// `ProcessPrng`, and whether it carries `net`, which asks for the sockets
/// of `WS2_32.dll` and moves it again.
struct Windows<const READS: bool, const NET: bool>;

impl<const READS: bool, const NET: bool> System for Windows<READS, NET> {
    const RESERVED: u32 = RESERVED;
    const STACK_BY_PAGE: bool = true;
    /// rsi and rdi, which calls preserve.
    const TEXT: (Reg, Reg) = (RSI, RDI);

    /// Writes every byte, retrying a write that is cut short, and jumps to
    /// `fail` if the system refuses it or writes nothing. A call goes
    /// through the import address table, slot by slot, and the handle is
    /// kept in rbx, which calls preserve.
    fn write(asm: &mut Asm, stream: Stream, fail: usize) {
        let again = asm.label();
        let done = asm.label();
        let failed = asm.label();
        asm.store(Place::Frame(RBX_AT), RBX, 8);
        asm.mov32(RCX, handle(stream));
        asm.call_slot(GET_STD_HANDLE);
        asm.mov(RBX, RAX);
        asm.bind(again);
        asm.test(RDI);
        asm.jcc(Z, done);
        // WriteFile(handle, text, at most MOST_PER_WRITE bytes, &written, NULL)
        asm.mov(R8, RDI);
        asm.mov32(RAX, MOST_PER_WRITE);
        asm.cmp(R8, RAX);
        asm.cmova(R8, RAX);
        asm.mov(RCX, RBX);
        asm.mov(RDX, RSI);
        asm.stack_address(R9, WRITTEN);
        asm.store_stack_imm32(FIFTH_ARGUMENT, 0);
        asm.call_slot(WRITE_FILE);
        asm.test32(RAX);
        asm.jcc(Z, failed);
        asm.load32_stack(RAX, WRITTEN);
        asm.test32(RAX);
        asm.jcc(Z, failed);
        asm.add(RSI, RAX);
        asm.sub(RDI, RAX);
        asm.jmp(again);
        // What the function kept in rbx is put back before it goes on, since a
        // landing that follows the write reads it.
        asm.bind(failed);
        asm.load(RBX, 8, false, Place::Frame(RBX_AT));
        asm.jmp(fail);
        asm.bind(done);
        asm.load(RBX, 8, false, Place::Frame(RBX_AT));
    }

    /// Reads from standard input with `ReadFile`, which gives a `BOOL` and
    /// writes how many bytes it read. A read that fails is the end of the
    /// input, as `ReadFile` reports the end of a pipe that way, and one that
    /// reads nothing is. The address and the count are in rsi and rdi, which
    /// calls preserve, and the handle is kept in rbx, which they preserve too.
    fn read(asm: &mut Asm, _: usize) {
        let ended = asm.label();
        let done = asm.label();
        asm.store(Place::Frame(RBX_AT), RBX, 8);
        asm.mov32(RCX, STD_INPUT_HANDLE);
        asm.call_slot(GET_STD_HANDLE);
        asm.mov(RBX, RAX);
        // ReadFile(handle, bytes, at most MOST_PER_WRITE bytes, &read, NULL)
        asm.mov(R8, RDI);
        asm.mov32(RAX, MOST_PER_WRITE);
        asm.cmp(R8, RAX);
        asm.cmova(R8, RAX);
        asm.mov(RCX, RBX);
        asm.mov(RDX, RSI);
        asm.stack_address(R9, WRITTEN);
        asm.store_stack_imm32(FIFTH_ARGUMENT, 0);
        asm.call_slot(READ_FILE);
        asm.test32(RAX);
        asm.jcc(Z, ended);
        asm.load32_stack(RAX, WRITTEN);
        asm.jmp(done);
        asm.bind(ended);
        asm.mov32(RAX, 0);
        asm.bind(done);
        asm.load(RBX, 8, false, Place::Frame(RBX_AT));
    }

    fn exit(asm: &mut Asm, status: u32) {
        asm.mov32(RCX, status);
        asm.call_slot(EXIT_PROCESS);
    }

    /// Asks `ProcessPrng` for every byte at once, which takes any count and
    /// gives a `BOOL` that is not 0 when it filled them. The address and the
    /// count are in rdi and rsi, which calls preserve.
    fn entropy(asm: &mut Asm) {
        let refused = asm.label();
        let end = asm.label();
        asm.mov(RCX, RDI);
        asm.mov(RDX, RSI);
        asm.call_slot(process_prng(READS, NET));
        asm.test32(RAX);
        asm.jcc(Z, refused);
        asm.mov32(RAX, 0);
        asm.jmp(end);
        asm.bind(refused);
        asm.mov32(RAX, 1);
        asm.bind(end);
    }

    fn end(asm: &mut Asm) {
        asm.mov(RCX, RDI);
        asm.call_slot(EXIT_PROCESS);
    }

    /// Asks the system's sockets to start, once, before the first operation of
    /// `net`: `WSAStartup(0x0202, &data)`, with room on the stack for the
    /// 408 bytes it writes, after the 32 the callee may use.
    fn start(asm: &mut Asm) {
        if NET {
            asm.reserve_stack(512);
            asm.mov32(RCX, 0x0202);
            asm.stack_address(RDX, 32);
            asm.call_slot(socket_slot(WSA_STARTUP));
            asm.release_stack(512);
        }
    }

    /// The kernel for a program that carries `net` is Windows's, which is 2.
    /// A call is a function of `WS2_32.dll`, which takes its words as Windows
    /// does: in rcx, rdx, r8, and r9, and then on the stack above the 32 bytes
    /// the callee may use. A call that refuses gives -1, and `WSAGetLastError`
    /// says why, which the call gives as minus the number. A call that gives an
    /// `int` leaves the high half of rax as it likes, so the low half is made a
    /// 64-bit number first. Those that give a socket give 64 bits.
    fn raw(asm: &mut Asm, call: Raw) {
        if call == Raw::Kind {
            asm.mov32(RAX, if NET { 2 } else { 0 });
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
                asm.mov64(RAX, -38i64 as u64);
                return;
            }
        };
        // The words are in rdi, rsi, rdx, r10, r8, and r9.
        asm.store(Place::Frame(40), R9, 8);
        asm.store(Place::Frame(32), R8, 8);
        asm.mov(R9, R10);
        asm.mov(R8, RDX);
        asm.mov(RDX, RSI);
        asm.mov(RCX, RDI);
        asm.call_slot(socket_slot(function));
        if int {
            asm.sign_extend32(RAX, RAX);
        }
        let given = asm.label();
        asm.cmp_imm8(RAX, -1);
        asm.jcc(NE, given);
        asm.call_slot(socket_slot(WSA_GET_LAST_ERROR));
        asm.sign_extend32(RCX, RAX);
        asm.mov32(RAX, 0);
        asm.sub(RAX, RCX);
        asm.bind(given);
    }

    /// Reserves and commits fresh pages. A failure gives no address.
    fn alloc(asm: &mut Asm, fail: usize) {
        asm.mov32(RCX, 0);
        asm.mov(RDX, RSI);
        asm.mov32(R8, MEM_COMMIT_RESERVE);
        asm.mov32(R9, PAGE_READWRITE);
        asm.call_slot(VIRTUAL_ALLOC);
        asm.test(RAX);
        asm.jcc(Z, fail);
    }

    fn arguments_on_entry(_: &mut Asm) {}

    /// The system gives one command line and no words, which a count of all
    /// ones says.
    fn arguments_by_call(asm: &mut Asm) {
        asm.call_slot(GET_COMMAND_LINE);
        asm.mov(R8, RAX);
        asm.mov64(RDI, u64::MAX);
        asm.mov32(RSI, 0);
    }
}

fn handle(stream: Stream) -> u32 {
    match stream {
        Stream::Stdout => STD_OUTPUT_HANDLE,
        Stream::Stderr => STD_ERROR_HANDLE,
    }
}
