# Roadmap

Features planned before 1.0, by priority within each section.
`[ ]` to do, `[~]` in progress, `[x]` done.

## 1. Protocol: behave like a certified node

- [x] Several addresses on one node (`feat/multi_address`).
- [ ] Pseudo-random delay before the first claim. Nodes starting together
      currently transmit at the same time.
- [ ] ISO Transport Protocol (60160, 60416): BAM and RTS/CTS sessions, send
      and reassembly.
- [ ] ISO Commanded Address (65240), over ISO TP.
- [ ] Group Function (126208): Request, Command, Acknowledge. At minimum, NAK
      what the node does not support. Needs runtime-sized fields and multiple
      repeating groups (section 3).
- [ ] Mandatory PGNs answered by default: PGN List (126464), Product
      Information (126996), Heartbeat (126993). Exact set to check against the
      NMEA 2000 certification requirements.

## 2. Transport

- [ ] Fast Packet reassembly wired into the receive path.
- [ ] Fast Packet send that does not block reception. A maximum-size message
      leaves the runner deaf for at least 62 ms.

## 3. Codec

- [ ] Runtime-sized fields (`DYNAMIC_FIELD_VALUE`, `VARIABLE`).
- [ ] Multiple repeating groups per PGN.
- [ ] Repeating groups without a counter (126464, 129796, 129805).
- [ ] "Not available" values exposed as `Option`, not as raw sentinels.
- [ ] `new()` on a proprietary variant sets its `Match` fields, so the message
      decodes back as that variant.
- [ ] Build fails if the optional tail of a `MinLength` PGN holds a signed,
      string or float field: its 0xFF padding would read as a valid value.
- [ ] The 33 PGNs listed as unsupported in `pgn_manifest.full.json`.

## 4. Validation

- [x] Shared local and GitHub software checks through `just ci`, including
      offline tests for the HIL host oracle.
- [x] Local STM32G431 hardware bench (`just hil`): Fast Packet TX and RX,
      independent host payload/framing checks and firmware exit status.
      Both scenarios validated with J-Link and WCH-Link. See `hil/README.md`.
- [ ] Extend the hardware bench with malformed/reordered Fast Packet frames,
      address conflicts and Commanded Address scenarios as their fixes land.
- [ ] Interoperability tests on a real bus against certified devices: claim,
      arbitration, Fast Packet.
- [ ] CANboat `analyzer` as an oracle: encode with korri-n2k, decode with
      `analyzer`, compare.
- [ ] Real captures from more manufacturers.

## 5. 1.0

- [ ] Public API frozen: breaking changes only in major releases.
- [x] Track CANboat releases through its release notifications.
