//! Claims `DUT_ADDRESS`, then reassembles with the library the `MESSAGE_COUNT` Fast Packet
//! messages the host injects from `INJECTOR_ADDRESS`. Exit 0 when all arrive intact, in order
//! (`injected_payload`), before the deadline; 1 on a corrupt message or at the deadline.
#![no_std]
#![no_main]

use embassy_time::{Duration, Instant, with_deadline};
use korri_hil_firmware::{
    INJECTOR_ADDRESS, MESSAGE_COUNT, PAYLOAD_LEN, PGN, bring_up, exit, injected_payload,
    runner_task, wait_claimed,
};
use korri_n2k::protocol::transport::fast_packet::assembler::{FastPacketAssembler, ProcessResult};

/// From the claim to the last message.
const DEADLINE: Duration = Duration::from_secs(20);

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    let (runner, handle, mut frames) = bring_up();
    spawner.spawn(runner_task(runner)).expect("spawn runner");
    wait_claimed(&handle).await;
    defmt::info!("ready");

    let deadline = Instant::now() + DEADLINE;
    let mut assembler = FastPacketAssembler::new();
    let mut received = 0u8;

    loop {
        let Ok(frame) = with_deadline(deadline, frames.recv()).await else {
            defmt::error!("deadline: {=u8} of {=u8} messages", received, MESSAGE_COUNT);
            exit(false);
        };
        // Other nodes of the bench talk too: only the injector counts.
        if frame.id.source_address() != INJECTOR_ADDRESS || frame.id.pgn() != PGN {
            continue;
        }

        // `process_frame` takes eight bytes and never sees the DLC.
        let mut data = [0xFF; 8];
        let len = frame.len.min(8);
        data[..len].copy_from_slice(&frame.data[..len]);
        let now_ms = Instant::now().as_millis();
        if let ProcessResult::MessageComplete(message) =
            assembler.process_frame(now_ms, PGN, INJECTOR_ADDRESS, &data)
        {
            if message.len != PAYLOAD_LEN
                || message.payload[..PAYLOAD_LEN] != injected_payload(received)
            {
                defmt::error!(
                    "message {=u8} corrupt, length {=usize}",
                    received,
                    message.len
                );
                exit(false);
            }
            received += 1;
            defmt::info!("message {=u8} intact", received);
            if received == MESSAGE_COUNT {
                exit(true);
            }
        }
    }
}
