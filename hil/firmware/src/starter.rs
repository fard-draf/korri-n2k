use defmt::info;
use embassy_stm32::can::OperatingMode;

use crate::conf::*;

embassy_stm32::bind_interrupts!(struct CanIrqs {
    FDCAN1_IT0 => embassy_stm32::can::IT0InterruptHandler<embassy_stm32::peripherals::FDCAN1>;
    FDCAN1_IT1 => embassy_stm32::can::IT1InterruptHandler<embassy_stm32::peripherals::FDCAN1>;
});

static TX_BUF: static_cell::StaticCell<embassy_stm32::can::TxBuf<CAN_TX_BUF_DEPTH>> =
    static_cell::StaticCell::new();
static RX_BUF: static_cell::StaticCell<embassy_stm32::can::RxBuf<CAN_RX_BUF_DEPTH>> =
    static_cell::StaticCell::new();

pub fn init_bsp() -> embassy_stm32::Peripherals {
    //==============================================================RCC
    let mut rcc_cfg = embassy_stm32::Config::default();

    rcc_cfg.rcc.hse = Some(embassy_stm32::rcc::Hse {
        freq: embassy_stm32::time::Hertz(8_000_000),
        mode: embassy_stm32::rcc::HseMode::Oscillator,
    });
    rcc_cfg.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_R;
    rcc_cfg.rcc.pll = Some(embassy_stm32::rcc::Pll {
        source: embassy_stm32::rcc::PllSource::HSE,
        prediv: embassy_stm32::rcc::PllPreDiv::DIV1,
        mul: embassy_stm32::rcc::PllMul::MUL40,
        divp: None,
        divq: Some(embassy_stm32::rcc::PllQDiv::DIV8),
        divr: Some(embassy_stm32::rcc::PllRDiv::DIV2),
    });

    rcc_cfg.rcc.ahb_pre = embassy_stm32::rcc::AHBPrescaler::DIV1;
    rcc_cfg.rcc.apb1_pre = embassy_stm32::rcc::APBPrescaler::DIV2;
    rcc_cfg.rcc.apb2_pre = embassy_stm32::rcc::APBPrescaler::DIV1;
    rcc_cfg.rcc.mux.fdcansel = embassy_stm32::rcc::mux::Fdcansel::HSE;
    rcc_cfg.rcc.boost = true;

    embassy_stm32::init(rcc_cfg)
}

pub fn init_can<'d>(
    p: embassy_stm32::Peripherals,
) -> embassy_stm32::can::BufferedCan<'d, CAN_TX_BUF_DEPTH, CAN_RX_BUF_DEPTH> {
    //==============================================================INIT CAN
    let irqs = CanIrqs;
    let can = init_fdcan(p.FDCAN1, p.PA11, p.PA12, irqs);
    info!(
        "FDCAN initialised ({} kbps). Err TX {}, Err RX {}",
        N2K_BITRATE / 1000,
        can.properties().tx_error_count(),
        can.properties().rx_error_count()
    );
    can
}

fn init_fdcan<'d>(
    fdcan: embassy_stm32::Peri<'d, embassy_stm32::peripherals::FDCAN1>,
    rx: embassy_stm32::Peri<'d, embassy_stm32::peripherals::PA11>,
    tx: embassy_stm32::Peri<'d, embassy_stm32::peripherals::PA12>,
    irqs: CanIrqs,
) -> embassy_stm32::can::BufferedCan<'d, CAN_TX_BUF_DEPTH, CAN_RX_BUF_DEPTH> {
    let mut cfg = embassy_stm32::can::CanConfigurator::new(fdcan, rx, tx, irqs);
    cfg.set_bitrate(N2K_BITRATE);
    cfg.properties().set_standard_filter(
        embassy_stm32::can::filter::StandardFilterSlot::_0,
        embassy_stm32::can::filter::StandardFilter::reject_all(),
    );
    cfg.properties().set_extended_filter(
        embassy_stm32::can::filter::ExtendedFilterSlot::_0,
        embassy_stm32::can::filter::ExtendedFilter::accept_all_into_fifo0(),
    );

    let can = cfg.start(OperatingMode::NormalOperationMode);

    let tx_buf = TX_BUF.init(embassy_stm32::can::TxBuf::new());
    let rx_buf = RX_BUF.init(embassy_stm32::can::RxBuf::new());

    can.buffered(tx_buf, rx_buf)
}
