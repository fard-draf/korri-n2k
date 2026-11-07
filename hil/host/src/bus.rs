//! `can0` through SocketCAN: a capture thread and an injector.

use crate::oracle::Frame;
use socketcan::{CanFrame, CanSocket, EmbeddedFrame, ExtendedId, Frame as _, Socket};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Records every extended-ID data frame of the interface until `stop`.
pub struct Capture {
    frames: Arc<Mutex<Vec<Frame>>>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
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
            std::thread::spawn(move || capture_frames(|| socket.read_frame(), &frames, &running))
        };
        Ok(Self {
            frames,
            running,
            thread: Some(thread),
        })
    }

    pub fn snapshot(&self) -> Vec<Frame> {
        self.frames.lock().expect("capture lock").clone()
    }

    pub fn stop(mut self) -> (Vec<Frame>, Result<(), String>) {
        let result = self.join();
        let frames = self.frames.lock().expect("capture lock").clone();
        (frames, result)
    }

    fn join(&mut self) -> Result<(), String> {
        self.running.store(false, Ordering::Relaxed);
        match self.thread.take() {
            Some(thread) => thread
                .join()
                .unwrap_or_else(|_| Err("capture thread panicked".to_string())),
            None => Ok(()),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // Also stop the reader if starting the firmware failed.
        let _ = self.join();
    }
}

fn capture_frames(
    mut read: impl FnMut() -> io::Result<CanFrame>,
    frames: &Mutex<Vec<Frame>>,
    running: &AtomicBool,
) -> Result<(), String> {
    while running.load(Ordering::Relaxed) {
        match read() {
            Ok(CanFrame::Data(frame)) if frame.is_extended() => {
                frames.lock().expect("capture lock").push(Frame {
                    id: frame.raw_id(),
                    data: frame.data().to_vec(),
                });
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(format!("capture read failed: {e}")),
            _ => {}
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_capture_joins_the_reader_within_the_read_timeout() {
        let running = Arc::new(AtomicBool::new(true));
        let finished = Arc::new(AtomicBool::new(false));
        let thread = {
            let (running, finished) = (running.clone(), finished.clone());
            std::thread::spawn(move || {
                let result = capture_frames(
                    || {
                        std::thread::sleep(Duration::from_millis(100));
                        Err(io::ErrorKind::WouldBlock.into())
                    },
                    &Mutex::new(Vec::new()),
                    &running,
                );
                finished.store(true, Ordering::Relaxed);
                result
            })
        };
        let capture = Capture {
            frames: Arc::new(Mutex::new(Vec::new())),
            running,
            thread: Some(thread),
        };
        let start = std::time::Instant::now();
        drop(capture);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(finished.load(Ordering::Relaxed));
    }

    #[test]
    fn idle_capture_stops_without_an_error() {
        let running = AtomicBool::new(true);
        let result = capture_frames(
            || {
                running.store(false, Ordering::Relaxed);
                Err(io::ErrorKind::TimedOut.into())
            },
            &Mutex::new(Vec::new()),
            &running,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn capture_preserves_frames_and_reports_first_non_timeout_error() {
        let running = AtomicBool::new(true);
        let frames = Mutex::new(Vec::new());
        let frame = CanFrame::new(ExtendedId::new(0x09F2015A).unwrap(), &[0; 8]).unwrap();
        let mut reads = 0;
        let result = capture_frames(
            || {
                reads += 1;
                match reads {
                    1 => Err(io::ErrorKind::WouldBlock.into()),
                    2 => Err(io::ErrorKind::TimedOut.into()),
                    3 => Ok(frame),
                    _ => {
                        // Bound this test even if errors are silently retried.
                        running.store(false, Ordering::Relaxed);
                        Err(io::Error::other("first capture failure"))
                    }
                }
            },
            &frames,
            &running,
        );
        assert_eq!(reads, 4);
        assert_eq!(frames.lock().unwrap().len(), 1);
        assert!(result.unwrap_err().contains("first capture failure"));
    }
}
