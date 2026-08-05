# Integration & Testing Specification

## 1. Overview

This project has **3 independent workstreams** that come together in integration:

1. **AWS Infrastructure** — IoT Core, Lambda, SNS, Device Shadow
2. **Firmware** — ESP32-C3 running Rust, deep sleep, WiFi, MQTT
3. **Hardware** — Optocoupler detection circuit, relay for chime, power management

Each workstream can be tested in isolation before combining. This document defines the tests for each workstream independently, then the integration tests that validate the complete system.

---

## 2. Workstream 1: AWS Infrastructure Testing

### Prerequisites

- AWS CLI configured with appropriate credentials
- Account access to IoT Core, Lambda, SNS, and CloudWatch
- At least one phone number subscribed to the SNS topic

### Test 1: Publish Fake MQTT Message via CLI

**Procedure:**

```bash
aws iot-data publish \
  --topic 'doorbell/ring' \
  --payload '{"event":"ring","timestamp":1234567890,"device":"test"}' \
  --region us-west-2
```

**Expected Result:**

- SMS arrives on all subscribed phones within 5–10 seconds
- CloudWatch logs show Lambda invocation with correct payload

### Test 2: Shadow CRUD

**Procedure:**

1. Update desired state:
   ```bash
   aws iot-data update-thing-shadow \
     --thing-name doorbell-device \
     --payload '{"state":{"desired":{"mode":"sms"}}}' \
     --region us-west-2 \
     output.json
   ```
2. Read shadow back:
   ```bash
   aws iot-data get-thing-shadow \
     --thing-name doorbell-device \
     --region us-west-2 \
     output.json
   cat output.json
   ```
3. Verify structure contains `state.desired.mode` and `state.reported` sections

**Expected Result:**

- Shadow document has correct structure
- Desired state reflects the update
- Metadata timestamps are present

### Test 3: Verify IoT Policy

**Procedure:**

1. Attempt publish to unauthorized topic:
   ```bash
   aws iot-data publish \
     --topic 'doorbell/unauthorized' \
     --payload '{"test":"should_fail"}' \
     --region us-west-2
   ```
   - **Expected:** Request is denied or message is silently dropped (depending on policy configuration)

2. Attempt connect with wrong client ID (using MQTT test client or mosquitto):
   - Configure a client with the device certificate but a mismatched client ID
   - **Expected:** Connection is rejected by IoT Core

---

## 3. Workstream 2: Firmware Testing

### Prerequisites

- ESP32-C3 dev board on breadboard
- Push button connected to **GPIO 2** (simulates doorbell press / wake source)
- LED connected to **GPIO 3** (simulates relay output for chime)
- USB cable for flashing and serial/defmt log output
- Multimeter for current measurement

### Test 1: Deep Sleep and Wake

**Procedure:**

1. Flash firmware to ESP32-C3
2. Verify it enters deep sleep:
   - Measure current with multimeter in series with power supply
   - **Expected:** ~5 μA in deep sleep
3. Press button on GPIO 2
4. Verify device wakes and runs boot sequence (observe defmt logs)

**Pass Criteria:**

- Sleep current ≤ 10 μA
- Wake-on-button is reliable (10/10 presses wake the device)

### Test 2: WiFi Connection

**Procedure:**

1. Press button to wake device
2. Observe defmt logs for WiFi connection events

**Expected Result:**

- Connects to configured WiFi network within 3 seconds
- DHCP address obtained
- Log shows successful connection with RSSI value

### Test 3: MQTT + TLS

**Procedure:**

1. Wake device via button press
2. Check AWS IoT Core console → "MQTT test client" → subscribe to `doorbell/ring`
3. Observe device connects and publishes

**Expected Result:**

- IoT Core console shows device as "connected"
- Message appears on topic `doorbell/ring` with correct JSON payload:
  ```json
  {
    "event": "ring",
    "timestamp": <unix_epoch>,
    "device": "<thing_name>",
    "battery_mv": <value>
  }
  ```

### Test 4: Shadow Read

**Procedure:**

For each mode (`sms`, `chime`, `both`, `silent`):

1. Set shadow desired state via CLI:
   ```bash
   aws iot-data update-thing-shadow \
     --thing-name doorbell-device \
     --payload '{"state":{"desired":{"mode":"<MODE>"}}}' \
     --region us-west-2 \
     output.json
   ```
2. Press button to wake device
3. Observe behavior:
   - `sms` → MQTT publish only, LED on GPIO 3 stays OFF
   - `chime` → LED on GPIO 3 turns ON briefly, no MQTT publish
   - `both` → LED ON briefly AND MQTT publish
   - `silent` → no LED, no MQTT publish

**Pass Criteria:**

- All 4 modes produce correct behavior
- Device reports mode back in shadow `state.reported`

### Test 5: Full Cycle Timing

**Procedure:**

1. Use a stopwatch or logic analyzer
2. Measure time from button press (GPIO 2 goes LOW) to MQTT message appearing in IoT Core console

**Target:** < 5 seconds total (wake + WiFi + TLS + MQTT publish)

**Breakdown targets:**

| Phase | Target |
|-------|--------|
| Wake from deep sleep | < 100 ms |
| WiFi connect | < 3 s |
| TLS + MQTT connect | < 1.5 s |
| Publish message | < 200 ms |
| **Total** | **< 5 s** |

---

## 4. Workstream 3: Hardware Testing

### Prerequisites

- Multimeter (AC voltage capable)
- Optocoupler circuit on breadboard (H11AA1 + resistors)
- Access to doorbell chime wires (front terminals)
- Oscilloscope (optional but helpful for pulse duration)
- Relay module for chime circuit

### Test 1: Measure Chime Voltage

**Procedure:**

1. Set multimeter to **AC voltage** mode
2. Connect probes to chime terminals (front doorbell wires)
3. Press doorbell button

**Expected Result:**

- Voltage reading between **16–24 VAC**
- Record exact value: _____ VAC

### Test 2: Measure Pulse Duration

**Procedure:**

1. Use multimeter (watch for peak) or oscilloscope connected to chime terminals
2. Press doorbell button
3. Note how long voltage is present

**Expected Result:**

- Record pulse duration: _____ ms
- **CRITICAL:** If pulse is < 100 ms, a small capacitor (100 nF – 1 μF) must be added on the optocoupler output to hold the wake signal long enough for ESP32-C3 GPIO wake detection

### Test 3: Optocoupler Output

**Procedure:**

1. Connect H11AA1 optocoupler to chime wires with calculated resistors
2. Connect output side with pull-up to 3.3V
3. Measure output pin voltage with multimeter

**Expected Result:**

- Idle state: output pin reads ~0V (LOW)
- When doorbell pressed: output pin goes HIGH (~3.3V)
- Verify signal is clean (no chatter/bounce that would cause multiple wakes)

### Test 4: Relay Function

**Procedure:**

1. Manually apply 3.3V to relay input pin
2. Listen for relay click
3. Use multimeter continuity mode between COM and NO terminals

**Expected Result:**

- Relay clicks audibly when 3.3V applied
- Continuity confirmed between COM and NO when energized
- No continuity between COM and NO when de-energized

### Test 5: Resistor Value Verification

**Procedure:**

Using the measured voltage from Test 1, calculate required resistor values:

```
I = V / (2 × R)
```

The H11AA1 needs **~1–5 mA** through its internal LEDs to trigger reliably.

**Example calculations:**

| Measured Voltage | Resistor | Current | Result |
|-----------------|----------|---------|--------|
| 16 VAC | 47 kΩ | 0.17 mA | ❌ TOO LOW |
| 16 VAC | 8.2 kΩ | 0.98 mA | ✅ OK |
| 24 VAC | 8.2 kΩ | 1.46 mA | ✅ OK |
| 24 VAC | 4.7 kΩ | 2.55 mA | ✅ OK |

**⚠️ IMPORTANT:** This calculation must be done with real measured values from Test 1. Do not assume voltage — measure it.

**Target current:** 1–3 mA (reliable triggering without exceeding H11AA1 max ratings)

---

## 5. Integration Testing

### Prerequisites

- All three workstreams passing individually
- Complete assembled system (ESP32-C3 + optocoupler + relay + power)
- System installed at doorbell location (or bench equivalent with transformer)

### Test 1: End-to-End SMS

**Procedure:**

1. Set shadow to `sms` mode
2. Press actual doorbell button
3. Start timer when doorbell pressed

**Expected Result:**

- SMS arrives on both subscribed phones
- Measure total latency from press to SMS received: _____ seconds
- Target: < 10 seconds

### Test 2: End-to-End Chime

**Procedure:**

1. Set shadow to `chime` mode
2. Press actual doorbell button

**Expected Result:**

- Physical chime sounds (relay closes chime circuit)
- No SMS is sent (verify no message received after 30 seconds)

### Test 3: End-to-End Both

**Procedure:**

1. Set shadow to `both` mode
2. Press actual doorbell button

**Expected Result:**

- Physical chime sounds AND SMS arrives on both phones
- Both events triggered from single press

### Test 4: Silent Mode

**Procedure:**

1. Set shadow to `silent` mode
2. Press actual doorbell button

**Expected Result:**

- No chime sounds
- No SMS sent
- Device still wakes, reads shadow, and goes back to sleep (verify via CloudWatch or defmt if connected)

### Test 5: Local Override

**Procedure:**

1. Set shadow to `sms` mode via CLI
2. Flip local toggle switch to override position
3. Press doorbell

**Expected Result:**

- Local switch overrides shadow mode
- Behavior matches whatever the local switch dictates
- Shadow reported state reflects the override

### Test 6: Remote Mode Change

**Procedure:**

1. Press doorbell → note behavior (e.g., SMS sent)
2. Change shadow mode via CLI to a different mode
3. Press doorbell again

**Expected Result:**

- Second press uses the new mode
- No reboot or manual intervention required
- Mode change takes effect on next wake cycle

### Test 7: Battery Life Baseline

**Procedure:**

1. With everything connected (optocoupler, relay, ESP32-C3), measure deep sleep current
2. Record: _____ μA

**Calculation:**

```
Battery capacity (mAh) / sleep current (mA) = hours of standby
Factor in wake events: assume 20 presses/day × 5 seconds × active current
```

**Expected Result:**

- Deep sleep current < 10 μA with full circuit
- Projected battery life > 1 year with typical usage

### Test 8: Reliability

**Procedure:**

1. Set shadow to `sms` mode
2. Press doorbell 20 times over the course of 1 hour (roughly every 3 minutes)
3. Log each press time and SMS arrival time

**Expected Result:**

- All 20 events captured (20 SMS messages received)
- No missed presses
- No duplicate SMS messages
- Consistent latency (note any outliers)

---

## 6. Known Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Chime pulse too short for reliable GPIO wake | Device misses doorbell presses | Add 100 nF – 1 μF capacitor on optocoupler output to extend pulse duration |
| WiFi connect time varies (2–8 s) | SMS latency unpredictable | Acceptable for SMS use case; set 10 s timeout, then sleep to conserve battery |
| AWS IoT Core rate limits | Messages throttled | Not a concern at doorbell-press frequency (~20/day max) |
| Battery voltage drops below ESP32-C3 minimum (2.7V) | Device stops functioning | Report `battery_mv` in shadow; set up CloudWatch alarm when value is low |
| WiFi router reboots or changes password | Device can't connect, drains battery retrying | After 3 failed WiFi attempts, go back to sleep (don't drain battery retrying indefinitely) |

---

## 7. Acceptance Criteria

All of the following must be verified before the project is considered complete:

- [ ] Doorbell press triggers SMS within 10 seconds
- [ ] All 4 modes work correctly (sms, chime, both, silent)
- [ ] Remote mode change takes effect on next ring
- [ ] Local switch override works
- [ ] Deep sleep current < 10 μA measured
- [ ] No missed events in 20-press reliability test
- [ ] Projected battery life > 1 year
