use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateGCAux, CreateWindowAux, WindowClass};
use x11rb::rust_connection::RustConnection;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT};

fn main() -> anyhow::Result<()> {
    let (conn, screen): (RustConnection, usize) = RustConnection::connect(None)?;
    let setup = conn.setup();
    let root = setup.roots[screen].root;
    let black = setup.roots[screen].black_pixel;
    let window: u32 = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        320,
        240,
        0,
        WindowClass::INPUT_OUTPUT,
        COPY_FROM_PARENT,
        &CreateWindowAux::new().background_pixel(black),
    )?
    .check()?;
    println!("window created {window:#x}");

    let gc: u32 = conn.generate_id()?;
    let swapped = conn.create_gc(gc, window, &CreateGCAux::new())?.check();
    match swapped {
        Ok(()) => println!("SWAPPED-ARGS gc {gc:#x} on window OK"),
        Err(e) => println!("SWAPPED-ARGS FAILED: {e}"),
    }
    Ok(())
}
