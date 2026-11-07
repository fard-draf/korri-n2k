//! `can0` through SocketCAN: a capture thread and an injector.

use crate::oracle::Frame;
use socketcan::{CanFrame, CanSocket, EmbeddedFrame, ExtendedId, Frame as _, Socket};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Records every extended-ID data frame of the interface until `stop`.
pub struct Capture {
    frames: Arc<Mutex<Vec<Frame>>>,
    running: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Capture {
    pub fn start(interface: &str) -> Result<Self, String> {
        let socket =
            CanSocket::open(interface).map_err(|e| format!("cannot open {interface}: {e}"))?;
        socket
            .set_read_timeout(Duration::from_millis(100))
            .map_err(|e| format!("cannot set the read timeout: {e}"))?;
        let frames = Arc::new(Mutex::new(Vec::new()));
        let running = Arc::new(AtomicBool::new(true));
        let thread = {
            let (frames, running) = (frames.clone(), running.clone());
            std::thread::spawn(move || {
                while running.load(Ordering::Relaxed) {
                    // A timeout is the normal idle case; any other error also just retries.
                    if let Ok(CanFrame::Data(frame)) = socket.read_frame() {
                        if frame.is_extended() {
                            frames.lock().expect("capture lock").push(Frame {
                                id: frame.raw_id(),
                                data: frame.data().to_vec(),
                            });
                        }
                    }
                }
            })
        };
        Ok(Self {
            frames,
            running,
            thread,
        })
    }

    pub fn snapshot(&self) -> Vec<Frame> {
        self.frames.lock().expect("capture lock").clone()
    }

    pub fn stop(self) -> Vec<Frame> {
        self.running.store(false, Ordering::Relaxed);
        self.thread.join().expect("capture thread");
        let frames = self.frames.lock().expect("capture lock").clone();
        frames
    }
}

pub struct Injector(CanSocket);

impl Injector {
    pub fn open(interface: &str) -> Result<Self, String> {
        let socket =
            CanSocket::open(interface).map_err(|e| format!("cannot open {interface}: {e}"))?;
        socket
            .set_write_timeout(Duration::from_millis(100))
            .map_err(|e| format!("cannot set the write timeout: {e}"))?;
        Ok(Self(socket))
    }

    pub fn send(&self, frame: &Frame) -> Result<(), String> {
        let id = ExtendedId::new(frame.id).ok_or("identifier above 29 bits")?;
        let frame = CanFrame::new(id, &frame.data).ok_or("frame refused by SocketCAN")?;
        self.0
            .write_frame(&frame)
            .map_err(|e| format!("write on the bus failed: {e}"))
    }
}

/// `candump -L`-like text, one frame per line, without timestamps.
pub fn write_log(path: &Path, frames: &[Frame]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    for frame in frames {
        let data: String = frame.data.iter().map(|b| format!("{b:02X}")).collect();
        writeln!(file, "{:08X}#{data}", frame.id)?;
    }
    Ok(())
}
