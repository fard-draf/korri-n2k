//! The bench oracle: Fast Packet reassembly and checks done on raw bytes, with nothing
//! from korri-n2k, so that a bug in the library's own assembler or builder cannot
//! hide itself. Ported from `check-captures.py` (audit 2026-11-06).

/// A captured or injected extended-ID frame. `data.len()` is the DLC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u32,
    pub data: Vec<u8>,
}

/// A reassembled Fast Packet message.
#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    pub sequence: u8,
    pub payload: Vec<u8>,
}

pub fn source_address(id: u32) -> u8 {
    (id & 0xFF) as u8
}

/// PDU1 (PF < 240): the PS byte is a destination, not part of the PGN.
pub fn pgn(id: u32) -> u32 {
    let pf = (id >> 16) & 0xFF;
    if pf < 240 {
        (id >> 8) & 0x3FF00
    } else {
        (id >> 8) & 0x3FFFF
    }
}

/// Reassembles the messages of one `pgn` sent by one `source`, in bus order. Frames of
/// other nodes and PGNs are skipped. Strict: any irregularity is an error.
///
/// A frame must have DLC 8. A message is a first frame (index 0, announced length
/// 1..=223) followed by indices 1, 2, … with the same sequence counter, and the bytes
/// after the announced length must be `0xFF`.
pub fn reassemble(frames: &[Frame], source: u8, pgn_wanted: u32) -> Result<Vec<Message>, String> {
    struct Open {
        sequence: u8,
        length: usize,
        next_index: u8,
        payload: Vec<u8>,
    }
    let mut messages = Vec::new();
    let mut open: Option<Open> = None;

    for (n, frame) in frames.iter().enumerate() {
        if source_address(frame.id) != source || pgn(frame.id) != pgn_wanted {
            continue;
        }
        let at = format!("frame {n} (id {:08X})", frame.id);
        if frame.data.len() != 8 {
            return Err(format!("{at}: DLC {} instead of 8", frame.data.len()));
        }
        let index = frame.data[0] & 0x1F;
        let sequence = frame.data[0] >> 5;

        if index == 0 {
            if open.is_some() {
                return Err(format!(
                    "{at}: new message while the previous one is incomplete"
                ));
            }
            let length = frame.data[1] as usize;
            if !(1..=223).contains(&length) {
                return Err(format!("{at}: announced length {length} outside 1..=223"));
            }
            open = Some(Open {
                sequence,
                length,
                next_index: 1,
                payload: frame.data[2..8].to_vec(),
            });
        } else {
            let Some(current) = open.as_mut() else {
                return Err(format!("{at}: continuation {index} without a first frame"));
            };
            if sequence != current.sequence || index != current.next_index {
                return Err(format!(
                    "{at}: sequence {sequence} index {index}, expected sequence {} index {}",
                    current.sequence, current.next_index
                ));
            }
            current.payload.extend_from_slice(&frame.data[1..8]);
            current.next_index += 1;
        }

        let current = open.as_ref().expect("open message");
        if current.payload.len() >= current.length {
            let current = open.take().expect("open message");
            if let Some(bad) = current.payload[current.length..]
                .iter()
                .find(|&&b| b != 0xFF)
            {
                return Err(format!("{at}: padding byte {bad:02X} instead of FF"));
            }
            messages.push(Message {
                sequence: current.sequence,
                payload: current.payload[..current.length].to_vec(),
            });
        }
    }

    match open {
        Some(current) => Err(format!(
            "message of {} bytes incomplete at the end of the capture",
            current.length
        )),
        None => Ok(messages),
    }
}

/// Splits `payload` into Fast Packet frames of `id`, byte by byte (the injection side).
pub fn fast_packet_frames(id: u32, sequence: u8, payload: &[u8]) -> Vec<Frame> {
    assert!((1..=223).contains(&payload.len()) && sequence < 8);
    let mut frames = Vec::new();
    let mut first = vec![sequence << 5, payload.len() as u8];
    let head = payload.len().min(6);
    first.extend_from_slice(&payload[..head]);
    first.resize(8, 0xFF);
    frames.push(Frame { id, data: first });
    for (i, chunk) in payload[head..].chunks(7).enumerate() {
        let mut data = vec![(sequence << 5) | (i as u8 + 1)];
        data.extend_from_slice(chunk);
        data.resize(8, 0xFF);
        frames.push(Frame { id, data });
    }
    frames
}

/// What `tx_fast_packet` must put on the bus: a claim (60928, 8 bytes) of `source`, then
/// `expected` messages of 127489, 26 bytes each, whose engine hours (offset 11, u32 LE)
/// are `hours_base`, `hours_base + 1`, … and whose sequence counter advances by one.
pub fn check_tx(
    frames: &[Frame],
    source: u8,
    expected: usize,
    hours_base: u32,
) -> Result<(), String> {
    if !frames
        .iter()
        .any(|f| source_address(f.id) == source && pgn(f.id) == 60928 && f.data.len() == 8)
    {
        return Err(format!("no address claim from {source}"));
    }
    let messages = reassemble(frames, source, 127_489)?;
    if messages.len() != expected {
        return Err(format!(
            "{} messages of 127489, expected {expected}",
            messages.len()
        ));
    }
    for (i, message) in messages.iter().enumerate() {
        if message.payload.len() != 26 {
            return Err(format!(
                "message {i}: {} bytes instead of 26",
                message.payload.len()
            ));
        }
        let hours = u32::from_le_bytes(message.payload[11..15].try_into().expect("4 bytes"));
        if hours != hours_base + i as u32 {
            return Err(format!(
                "message {i}: engine hours {hours}, expected {}",
                hours_base + i as u32
            ));
        }
        if i > 0 && message.sequence != (messages[i - 1].sequence + 1) % 8 {
            return Err(format!(
                "message {i}: sequence {} after {}",
                message.sequence,
                messages[i - 1].sequence
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 127489 = 0x1F201, priority 2, source 90.
    const ID: u32 = (2 << 26) | (0x1F201 << 8) | 90;

    fn frame(id: u32, data: [u8; 8]) -> Frame {
        Frame {
            id,
            data: data.to_vec(),
        }
    }

    /// Payload 00 01 … 19 (26 bytes), sequence 2, written by hand.
    fn valid_capture() -> Vec<Frame> {
        vec![
            frame(ID, [0x40, 0x1A, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05]),
            frame(ID, [0x41, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C]),
            frame(ID, [0x42, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13]),
            frame(ID, [0x43, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0xFF]),
        ]
    }

    #[test]
    fn pgn_and_source_of_a_pdu2_id() {
        assert_eq!(pgn(ID), 127_489);
        assert_eq!(source_address(ID), 90);
        // PDU1: the destination byte is not part of the PGN (59904 to address 0xFF).
        assert_eq!(pgn(0x18EA_FF5A), 59_904);
    }

    #[test]
    fn valid_capture_reassembles() {
        let messages = reassemble(&valid_capture(), 90, 127_489).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sequence, 2);
        assert_eq!(messages[0].payload, (0u8..26).collect::<Vec<_>>());
    }

    #[test]
    fn other_nodes_and_pgns_interleaved_are_skipped() {
        let mut capture = valid_capture();
        capture.insert(
            1,
            frame(
                (2 << 26) | (0x1F201 << 8) | 81,
                [0xC0, 0x1A, 0, 0, 0, 0, 0, 0],
            ),
        );
        capture.insert(2, frame((2 << 26) | (0x1F200 << 8) | 90, [0; 8]));
        assert_eq!(reassemble(&capture, 90, 127_489).unwrap().len(), 1);
    }

    #[test]
    fn wrong_padding_is_rejected() {
        let mut capture = valid_capture();
        capture[3].data[7] = 0x00;
        let error = reassemble(&capture, 90, 127_489).unwrap_err();
        assert!(error.contains("padding"), "{error}");
    }

    #[test]
    fn incomplete_capture_is_rejected() {
        let mut capture = valid_capture();
        capture.pop();
        let error = reassemble(&capture, 90, 127_489).unwrap_err();
        assert!(error.contains("incomplete"), "{error}");
    }

    #[test]
    fn short_dlc_and_wrong_index_are_rejected() {
        let mut capture = valid_capture();
        capture[1].data.pop();
        assert!(reassemble(&capture, 90, 127_489)
            .unwrap_err()
            .contains("DLC"));
        let mut capture = valid_capture();
        capture.swap(1, 2);
        assert!(reassemble(&capture, 90, 127_489)
            .unwrap_err()
            .contains("index"));
    }

    #[test]
    fn injection_frames_match_the_hand_written_capture() {
        let payload: Vec<u8> = (0u8..26).collect();
        assert_eq!(fast_packet_frames(ID, 2, &payload), valid_capture());
    }

    #[test]
    fn check_tx_reads_hours_and_sequence() {
        let claim = frame((6 << 26) | (0xEE00 << 8) | 90, [0; 8]);
        let message = |hours: u32, sequence: u8| {
            let mut payload = vec![0xFF; 26];
            payload[11..15].copy_from_slice(&hours.to_le_bytes());
            fast_packet_frames(ID, sequence, &payload)
        };
        let mut capture = vec![claim];
        capture.extend(message(1000, 7));
        capture.extend(message(1001, 0));
        check_tx(&capture, 90, 2, 1000).unwrap();
        assert!(check_tx(&capture, 90, 3, 1000)
            .unwrap_err()
            .contains("expected 3"));
        assert!(check_tx(&capture, 90, 2, 999)
            .unwrap_err()
            .contains("engine hours"));
        capture.truncate(1 + 4);
        capture.extend(message(1001, 2));
        assert!(check_tx(&capture, 90, 2, 1000)
            .unwrap_err()
            .contains("sequence"));
        assert!(check_tx(&capture[1..], 90, 2, 1000)
            .unwrap_err()
            .contains("no address claim"));
    }
}
