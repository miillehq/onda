//! Inspect only the X11 selection owner and its advertised formats. Arboard owns the text.
use crate::{
    text_clipboard::{x11_format, Format},
    PlatformError,
};
use std::time::{Duration, Instant};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{Atom, AtomEnum, ConnectionExt, CreateWindowAux, Window, WindowClass},
        Event,
    },
    rust_connection::RustConnection,
    COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE,
};

const MAX_FORMATS: u32 = 128;
const OFFER_TIMEOUT: Duration = Duration::from_millis(500);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) struct OfferReader {
    connection: RustConnection,
    window: Window,
    selection: Atom,
    targets: Atom,
    property: Atom,
}

fn unreadable<E>(_: E) -> PlatformError {
    PlatformError::ClipboardNotPreservable
}

impl OfferReader {
    pub(super) fn open() -> Result<Self, PlatformError> {
        let (connection, screen) = x11rb::connect(None).map_err(unreadable)?;
        let root = connection
            .setup()
            .roots
            .get(screen)
            .ok_or(PlatformError::ClipboardNotPreservable)?
            .root;
        let window = connection.generate_id().map_err(unreadable)?;
        connection
            .create_window(
                COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_ONLY,
                COPY_FROM_PARENT,
                &CreateWindowAux::new(),
            )
            .map_err(unreadable)?
            .check()
            .map_err(unreadable)?;
        let atom = |name: &[u8]| -> Result<Atom, PlatformError> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(unreadable)?
                .reply()
                .map_err(unreadable)?
                .atom)
        };
        let selection = atom(b"CLIPBOARD")?;
        let targets = atom(b"TARGETS")?;
        let property = atom(b"ONDA_CLIPBOARD_FORMATS")?;
        Ok(Self {
            connection,
            window,
            selection,
            targets,
            property,
        })
    }

    pub(super) fn owner(&self) -> Result<Window, PlatformError> {
        Ok(self
            .connection
            .get_selection_owner(self.selection)
            .map_err(unreadable)?
            .reply()
            .map_err(unreadable)?
            .owner)
    }

    pub(super) fn formats(&self) -> Result<Vec<Format>, PlatformError> {
        self.connection
            .convert_selection(
                self.window,
                self.selection,
                self.targets,
                self.property,
                CURRENT_TIME,
            )
            .map_err(unreadable)?
            .check()
            .map_err(unreadable)?;
        self.connection.flush().map_err(unreadable)?;
        let deadline = Instant::now() + OFFER_TIMEOUT;
        loop {
            if Instant::now() >= deadline {
                return Err(PlatformError::ClipboardNotPreservable);
            }
            if let Some(Event::SelectionNotify(event)) =
                self.connection.poll_for_event().map_err(unreadable)?
            {
                if event.requestor != self.window
                    || event.selection != self.selection
                    || event.target != self.targets
                {
                    continue;
                }
                if event.property == NONE || event.property != self.property {
                    return Err(PlatformError::ClipboardNotPreservable);
                }
                break;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        let reply = self
            .connection
            .get_property(
                true,
                self.window,
                self.property,
                AtomEnum::ATOM,
                0,
                MAX_FORMATS,
            )
            .map_err(unreadable)?
            .reply()
            .map_err(unreadable)?;
        if reply.type_ != u32::from(AtomEnum::ATOM) || reply.bytes_after != 0 {
            return Err(PlatformError::ClipboardNotPreservable);
        }
        let atoms = reply
            .value32()
            .ok_or(PlatformError::ClipboardNotPreservable)?;
        atoms
            .map(|atom| {
                let name = self
                    .connection
                    .get_atom_name(atom)
                    .map_err(unreadable)?
                    .reply()
                    .map_err(unreadable)?
                    .name;
                Ok(x11_format(std::str::from_utf8(&name).map_err(unreadable)?))
            })
            .collect()
    }
}
