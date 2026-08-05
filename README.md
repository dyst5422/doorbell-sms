# doorbell-sms

A Rust embedded project that intercepts the chime signal from a Blink doorbell using an ESP32-C3 microcontroller and sends SMS notifications via AWS IoT Core. An optocoupler isolates the doorbell's 3.3V chime line from the MCU, allowing non-invasive detection without modifying the doorbell hardware. The firmware connects over Wi-Fi to publish MQTT messages that trigger SMS delivery through AWS SNS.

## Architecture

```
┌──────────┐     ┌──────────────┐     ┌───────────┐     ┌──────────────┐     ┌─────┐     ┌─────┐
│ Doorbell │────▶│ Optocoupler  │────▶│ ESP32-C3  │────▶│ AWS IoT Core │────▶│ SNS │────▶│ SMS │
│ (chime)  │     │ (isolation)  │     │ (firmware)│     │   (MQTT)     │     │     │     │     │
└──────────┘     └──────────────┘     └───────────┘     └──────────────┘     └─────┘     └─────┘
```

## Modes

| Mode     | Description                              |
|----------|------------------------------------------|
| `sms`    | Send SMS notification only               |
| `chime`  | Play local chime only                    |
| `both`   | Send SMS and play local chime            |
| `silent` | Suppress all notifications (still logs)  |

## Tech Stack

- **MCU:** ESP32-C3 SuperMini
- **Language:** Rust (`no_std`)
- **Frameworks:** embassy, esp-hal, esp-wifi
- **Cloud:** AWS IoT Core (MQTT) → SNS (SMS delivery)

## Spec Documents

- [`specs/`](specs/) — Design and requirement documents

## Hardware Cost

| Component              | Approximate Cost |
|------------------------|-----------------|
| ESP32-C3 SuperMini     | ~$4             |
| Optocoupler + passives | ~$2             |
| PCB / protoboard       | ~$3             |
| Enclosure + misc       | ~$9–11          |
| **Total**              | **~$18–20**     |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Author

Dylan Stewart <jdylanstewart@gmail.com>
