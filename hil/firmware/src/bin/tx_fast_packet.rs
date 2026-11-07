//! Claims `DUT_ADDRESS`, sends `MESSAGE_COUNT` Engine Parameters, Dynamic (127489) as Fast
//! Packet every 100 ms, then exits 0. The host checks the frames on the bus.
#![no_std]
#![no_main]

use embassy_time::{Duration, Ticker, Timer};
use korri_hil_firmware::{
    MESSAGE_COUNT, PGN, TX_HOURS_BASE, bring_up, exit, runner_task, wait_claimed,
};
use korri_n2k::protocol::messages::Pgn127489;

#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    let (runner, handle, _frames) = bring_up();
    spawner.spawn(runner_task(runner)).expect("spawn runner");
    wait_claimed(&handle).await;

    let mut ticker = Ticker::every(Duration::from_millis(100));
    for index in 0..MESSAGE_COUNT {
        ticker.next().await;
        let mut engine = Pgn127489::new();
        engine.total_engine_hours = TX_HOURS_BASE + index as u32;
        if let Err(error) = handle.send_pgn(&engine, PGN, 2, None).await {
            defmt::error!("send {=u8} refused: {}", index, defmt::Debug2Format(&error));
            exit(false);
        }
    }

    // The runner empties the command queue on its own: let the last frames leave the FIFO.
    Timer::after_millis(500).await;
    exit(true)
}
