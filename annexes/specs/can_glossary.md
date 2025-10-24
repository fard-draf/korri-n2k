# Glossary

## NMEA 2000 Header
Each field plays a specific role in bus arbitration, message identification, and routing. Understanding this layout is essential to build dependable firmware.

### **P (Priority)**

- **Definition:** A **3-bit** field that sets CAN bus priority. Values range from 0 (highest priority) to 7 (lowest).
- **Technical Role:** This field drives CAN bus arbitration. When a device transmits it monitors the bus. If another node sends an identifier with a higher priority (transmitting `0` while this node drives `1`), the lower-priority device immediately stops transmitting.
- **Architectural Impact:** Critical traffic (autopilot commands, engine alarms) must use high priority (low numeric value) to ensure delivery even on a saturated bus.

### **R (Reserved)**

- **Definition:** A single **reserved** bit.
- **Technical Role:** Under NMEA 2000 this bit MUST be **0**. Other J1939-based protocols (for example truck FMS) can reuse it as the Extended Data Page (EDP).
- **Architectural Impact:** Parsing code should enforce the bit to 0 to remain strictly compliant with the NMEA 2000 specification.

### **DP (Data Page)**

- **Definition:** A one-bit **data page** selector.
- **Technical Role:** Combined with the `R` bit it forms the two most significant bits of the PGN. It allows the PGN space to exceed 65 536 identifiers and reach higher ranges used by fast-packet traffic.
- **Architectural Impact:** Often 0 for common PGNs, yet it must be accounted for when reconstructing the PGN to support the full standard.

### **PF (PDU Format)**

- **Definition:** An **8-bit** field that indicates the **frame format** (PDU – Protocol Data Unit).
- **Technical Role:** The key switch for decoding; it acts as the router:
  - If `PF < 240` (`0xF0`) the frame uses **PDU1** and targets a specific destination.
  - If `PF >= 240` the frame uses **PDU2** and is broadcast to every node.
- **Architectural Impact:** Parsing logic must read `PF` first to determine how to interpret the following `PS` field.

### **PS (PDU Specific)**

- **Definition:** An **8-bit** field whose meaning depends on `PF`.
- **Technical Role:** The chameleon field of the header:
  - In **PDU1** mode, `PS` contains the destination address (0–251).
  - In **PDU2** mode, `PS` is the least significant byte of the PGN.
- **Architectural Impact:** A Rust representation of an NMEA 2000 header should model this duality explicitly. An `enum` or an `Option<u8>` for the destination is idiomatic and safe.

### **SA (Source Address)**

- **Definition:** An **8-bit** field containing the **address of the transmitting device**.
- **Technical Role:** The identity of the sender for the current frame. Each device must own a unique address in the range 0–251.
- **Architectural Impact:** It is central to the **Address Claiming** process (PGN 60928). Firmware must be able to claim and defend an address during start-up.

---

### **Related Terms**

- **PGN (Parameter Group Number):** Not a direct header field but the **message identifier** (e.g. 129025 for position, 127250 for heading) reconstructed from `R`, `DP`, `PF`, and `PS` (in PDU2 mode). It determines whether a device should consume the payload bytes that follow.
- **CAN ID (CAN Identifier):** The **29-bit** integer made of all the fields above. The CAN controller (e.g. ESP32 TWAI) relies on it for arbitration and message filtering.
- **PDU (Protocol Data Unit):** A generic networking term for a data block. In NMEA 2000 it denotes the frame format (PDU1 for addressed delivery, PDU2 for broadcast).
