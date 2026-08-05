# Hardware Specification — Doorbell SMS

## 1. Components List

| Component | Qty | Purpose | Specs / Notes |
|-----------|-----|---------|---------------|
| ESP32-C3 SuperMini | 1 | MCU + WiFi | RISC-V 160 MHz, 400 KB SRAM, 4 MB flash, 802.11 b/g/n |
| H11AA1 optocoupler (DIP-6) | 1 | AC detection | Detects 8–24 VAC doorbell pulse; bidirectional LED input handles AC natively |
| 5V relay module (3.3V logic input) | 1 | Controls chime solenoid | Single-channel, normally-open contact rated for AC load |
| 47 kΩ resistor (1/4 W) | 2 | Current limiting | Limits current into H11AA1 internal LEDs on AC input side |
| 10 kΩ resistor (1/4 W) | 1 | Pull-down | Pull-down on optocoupler phototransistor output (collector) |
| SPST toggle switch | 1 | Local mode override | Selects chime mode locally without WiFi |
| 2× AA lithium battery holder | 1 | Power supply | Holds 2× AA cells in series (3.0 V nominal) |
| Energizer Ultimate Lithium AA | 2 | Battery | 3000 mAh, 1.5 V nominal, low self-discharge |
| 2-pin screw terminal block | 2–3 | Chime wire connections | Connects to existing doorbell transformer wiring |
| Protoboard / perfboard | 1 | Final assembly | Solder-in mounting for all components |

## 2. Wiring Diagram

```
                        ┌─────────────────────────────────────────────┐
                        │           DOORBELL TRANSFORMER               │
                        │          (16–24 VAC when pressed)            │
                        └────────┬──────────────────────┬─────────────┘
                                 │ Chime Wire A          │ Chime Wire B
                                 │                      │
                            ┌────┴────┐            ┌────┴────┐
                            │  47 kΩ  │            │  47 kΩ  │
                            └────┬────┘            └────┬────┘
                                 │                      │
                        ┌────────┴──────────────────────┴────────┐
                        │        H11AA1 Optocoupler              │
                        │                                        │
                        │   Pin 1 (Anode 1) ◄── 47kΩ ◄── Wire A │
                        │   Pin 2 (Anode 2) ◄── 47kΩ ◄── Wire B │
                        │                                        │
                        │   Pin 4 (Emitter)  ──► GND             │
                        │   Pin 5 (Collector) ──┬──► GPIO 2      │
                        │                       │    (wake pin)  │
                        └───────────────────────┼────────────────┘
                                                │
                                           ┌────┴────┐
                                           │  10 kΩ  │ (pull-down)
                                           └────┬────┘
                                                │
                                               GND

    ┌───────────────────────────────────────────────────────────────┐
    │                      ESP32-C3 SuperMini                        │
    │                                                               │
    │   GPIO 2  ◄── H11AA1 collector (wake-on-HIGH)                 │
    │   GPIO 3  ──► Relay module IN                                 │
    │   GPIO 4  ◄── Toggle switch (internal pull-up enabled)        │
    │   3.3V    ◄── Battery (+) (bypass regulator for efficiency)   │
    │   GND     ◄── Battery (-)                                     │
    └───────────────────────────────────────────────────────────────┘

    ┌───────────────────────────────────────────────────────────────┐
    │                      RELAY MODULE                              │
    │                                                               │
    │   IN   ◄── GPIO 3                                             │
    │   VCC  ◄── 3.3V                                               │
    │   GND  ◄── GND                                                │
    │                                                               │
    │   COM  ──► Chime solenoid terminal A                          │
    │   NO   ──► Chime wire (AC passes through when energized)      │
    │   NC   ── (unused)                                            │
    └───────────────────────────────────────────────────────────────┘

    ┌───────────────────┐
    │   TOGGLE SWITCH   │
    │                   │
    │   Terminal 1 ──► GPIO 4                                       │
    │   Terminal 2 ──► GND                                          │
    └───────────────────┘
    (GPIO 4 uses internal pull-up; switch closed = LOW = override)

    ┌───────────────────┐
    │  BATTERY PACK     │
    │  2× AA Lithium    │
    │                   │
    │   (+) ──► ESP32-C3 3.3V pin                                   │
    │   (-) ──► GND (common ground)                                 │
    └───────────────────┘
```

## 3. Power Budget

| State | Current Draw | Duration | Notes |
|-------|-------------|----------|-------|
| Deep sleep | ~5 μA | 99.9%+ of time | ESP32-C3 with GPIO wake configured |
| Active (WiFi + MQTT) | ~80–130 mA | ~3–5 seconds per event | Connect, publish, disconnect |

**Duty cycle analysis (10 presses/day):**
- Active time: 10 × 5 s = 50 s/day
- Active energy: 130 mA × 50 s = 6500 mAs = 1.8 mAh/day
- Sleep energy: 5 μA × 86350 s = 0.43 As = 0.12 mAh/day
- Total daily: ~2 mAh/day

**Battery life estimate:**
- Capacity: 2× AA lithium in series = 3000 mAh at ~3.0 V
- Theoretical at 5 μA standby only: 3000 mAh / 0.005 mA ≈ 68 years
- Limited by battery self-discharge: 3–5 years practical ceiling
- **Conservative real-world estimate: 1–2 years**

## 4. Relay Wiring Detail

The relay is wired **in series** with the chime solenoid. This gives the ESP32 full control over whether the chime can sound:

| Mode | Relay State | Behavior |
|------|-------------|----------|
| SMS-only | De-energized (NO open) | Chime circuit is broken — chime cannot sound even if AC is present |
| Chime / Both | Energized briefly (~500 ms) | Relay closes NO contact, completing the AC circuit through the solenoid |

**Sequence when doorbell is pressed (chime mode):**
1. AC pulse detected via H11AA1 → GPIO 2 goes HIGH
2. ESP32-C3 wakes from deep sleep
3. ESP32 drives GPIO 3 HIGH → relay energizes → chime sounds
4. After ~500 ms, ESP32 drives GPIO 3 LOW → relay de-energizes
5. ESP32 sends MQTT/SMS notification
6. ESP32 returns to deep sleep

## 5. Pre-Build Verification

Before soldering the final board:

1. **Measure chime wire voltage** — Use a multimeter (AC mode) across the two chime wires while pressing the doorbell button. Expect 16–24 VAC.
2. **Measure pulse duration** — Note how long voltage is present (typically 0.5–2 seconds depending on button hold time).
3. **Verify ESP32-C3 pinout** — Confirm GPIO 2, 3, and 4 locations on your specific SuperMini board match the wiring plan. Pinouts vary between manufacturers.
4. **Test H11AA1 on breadboard** — Verify the optocoupler triggers reliably at the measured voltage with the chosen resistor values.
5. **Confirm relay module logic** — Verify the relay module activates with a 3.3V HIGH signal (some modules are active-LOW).

## 6. Notes

- **Why H11AA1 over PC817:** The H11AA1 has bidirectional internal LEDs, allowing it to handle AC input natively without a bridge rectifier. The PC817 is DC-only and would require additional components.

- **Resistor sizing concern:** The 47 kΩ resistors are sized for worst-case 24 VAC:
  - I = 24 V / (47 kΩ + 47 kΩ) = 24 V / 94 kΩ ≈ 0.25 mA
  - The H11AA1 typically needs ~1 mA to trigger reliably (CTR spec)
  - **This may be insufficient** — if the measured voltage is lower (e.g., 16 VAC), current drops to ~0.17 mA
  - May need to reduce resistors to **10 kΩ** each (giving ~1.2 mA at 24 V, ~0.8 mA at 16 V)
  - ⚠️ **Recalculate resistor values after measuring actual transformer voltage**

- **Battery bypass:** Connecting 2× AA lithium (3.0 V) directly to the 3.3V pin bypasses the onboard LDO regulator, eliminating its quiescent current (~30–100 μA). The ESP32-C3 operates reliably from 3.0–3.6 V, and lithium AAs maintain a flat discharge curve near 1.5 V/cell (3.0 V total) for most of their life.

- **Wake pin selection:** GPIO 2 supports RTC GPIO wake from deep sleep on the ESP32-C3. Verify this in the datasheet for your specific module revision.
