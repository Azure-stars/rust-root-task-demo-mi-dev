#![no_std]
#![no_main]
#![feature(never_type)]

extern crate alloc;
extern crate sel4_panicking;

mod mime;
mod server;

use crate_consts::INIT_EP;
use embedded_io_async::ReadExactError;
use sel4_async_block_io_fat as fat;

use alloc_helper::define_allocator;
use sel4::{debug_println, set_ipc_buffer, IPCBuffer};
use sel4_async_network::{TcpSocket, TcpSocketError};
use server::Server;

sel4_panicking_env::register_debug_put_char!(sel4::sys::seL4_DebugPutChar);

/// Default size of the global allocator
const DEFAULT_ALLOCATOR_SIZE: usize = 0x1_8000;
define_allocator! {
    /// Define a new global allocator
    /// Size is [DEFAULT_ALLOCATOR_SIZE]
    (GLOBAL_ALLOCATOR, DEFAULT_ALLOCATOR_SIZE)
}

/// Default port of http, TCP PORT.
const HTTP_PORT: usize = 6379;

async fn use_socket_for_http<D: fat::BlockDevice + 'static, T: fat::TimeSource + 'static>(
    server: Server<D, T>,
    mut socket: TcpSocket,
) -> Result<(), ReadExactError<TcpSocketError>> {
    // socket.accept(HTTP_PORT).await?;
    // server
    //     .handle_connection(&mut EmbeddedIOAsyncAdapter(&mut socket))
    //     .await?;
    socket.close();
    Ok(())
}

pub fn fmt_with_module(record: &log::Record, f: &mut core::fmt::Formatter) -> core::fmt::Result {
    let target = match record.target().is_empty() {
        true => record.module_path().unwrap_or_default(),
        false => record.target(),
    };
    let color_code = match record.level() {
        log::Level::Error => 31u8, // Red
        log::Level::Warn => 93,    // BrightYellow
        log::Level::Info => 34,    // Blue
        log::Level::Debug => 32,   // Green
        log::Level::Trace => 90,   // BrightBlack
    };

    write!(
        f,
        "\u{1B}[{}m\
            [{}] [{}] {}\
            \u{1B}[0m",
        color_code,
        record.level(),
        target,
        record.args()
    )
}

#[export_name = "_start"]
fn main(ipc_buffer: IPCBuffer) -> sel4::Result<!> {
    static LOGGER: sel4_logging::Logger = sel4_logging::LoggerBuilder::const_default()
        .write(|s| sel4::debug_print!("{}", s))
        .level_filter(log::LevelFilter::Trace)
        .fmt(fmt_with_module)
        .build();
    LOGGER.set().unwrap();
    set_ipc_buffer(ipc_buffer);
    log::debug!("[HTTP Server] Server Initializing !");

    // INIT_EP.send(info)

    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    debug_println!("Task Error");
    loop {}
}
