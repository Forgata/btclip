# btclip

> **Minimalist, 100% offline bidirectional clipboard and screenshot synchronization between Android and PC over Bluetooth RFCOMM.**

`btclip` allows you to seamlessly share clipboard state between your Android phone and PC without relying on Wi-Fi, routers, internet connections, or third-party cloud servers. Built for low latency, security, and low resource overhead.

---

## Features

- **100% Offline & Private:** Direct peer-to-peer communication over local Bluetooth radio waves (`RFCOMM/SPP`).
- **Bidirectional Text Sync:** Copy text on Android to paste on PC, or copy on PC to paste on Android.
- **Instant Screenshot Transfer:** Take a screenshot on your phone, and it is instantly available in your PC clipboard ready to paste (`Ctrl + V`).
- **Ultra-Lean Footprint:**
  - **PC Host:** Built in **Rust** as a single, lightweight binary with minimal RAM usage.
  - **Phone Client:** Native **Kotlin** Android Foreground Service using zero-permission `MediaStore` triggers.
- **Echo Loop Prevention:** Smart internal write flags prevent local clipboard change loops.

## Tech Stack & Architecture

| Component      | Technology                 | Role                                                                                                             |
| :------------- | :------------------------- | :--------------------------------------------------------------------------------------------------------------- |
| **PC Engine**  | **Rust**                   | RFCOMM Server, WinRT/Win32 APIs, native clipboard management (`arboard`), and image decoding (`image` crate).    |
| **Mobile App** | **Kotlin / Android**       | RFCOMM Client, Android `ForegroundService`, `ClipboardManager`, and `MediaStore ContentObserver`.                |
| **Transport**  | **Bluetooth RFCOMM (SPP)** | Emulates a raw stream-oriented socket directly between devices over UUID `8ce255c0-200a-11e0-ac64-0800200c9a66`. |

## Wire Protocol Spec

`btclip` streams custom binary frames over a raw RFCOMM socket. Each message is prepended with a 5-byte header to handle packet framing and fragmentation.

```text
 0                   1                   5
 +-------------------+-------------------+-----------------------------------+
 |  Type (1 Byte)    | Length (4 Bytes)  |         Payload Data              |
 |  0x01 = Text      | Big-Endian u32    |         (N Bytes)                 |
 |  0x02 = Image     |                   |                                   |
 +-------------------+-------------------+-----------------------------------+

```

- **Header (5 Bytes):**
- **Byte 0 (`Type`):** `0x01` for UTF-8 Plain Text, `0x02` for PNG/JPEG Image bytes.
- **Bytes 1–4 (`Length`):** Big-Endian unsigned 32-bit integer indicating payload size in bytes.

- **Payload ($N$ Bytes):** The raw UTF-8 string or compressed image bytes.

## Project Structure

```text
btclip/
├── desktop/                  # PC Host Engine (Rust)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs           # Entry point & event loop
│       ├── bluetooth.rs      # WinRT RFCOMM socket listener
│       ├── clipboard.rs      # Arboard & Win32 system clipboard handler
│       └── protocol.rs       # Binary frame parser & encoder
└── mobile/                   # Android Client (Kotlin)
    ├── app/
    │   └── src/main/java/com/btclip/
    │       ├── MainActivity.kt
    │       ├── Service.kt    # Persistent Foreground Service
    │       ├── Observer.kt   # MediaStore screenshot detector
    │       └── Socket.kt     # Bluetooth RFCOMM client writer/reader
    └── build.gradle

```

---

## Getting Started

### Prerequisites

1. **Bluetooth Adapter:** Both your PC and phone must have active Bluetooth hardware.
2. **OS Pairing:** Pair your phone and PC once through your operating system settings.
3. **Rust Toolchain (PC):** Install Rust via [rustup](https://rustup.rs/).
4. **Android SDK (Mobile):** Android 8.0+ (API Level 26+).

---

### Building the Desktop Host (PC)

1. Navigate to the desktop directory:

```bash
cd desktop

```

2. Build the optimized release binary:

```bash
cargo build --release
```

3. Run the executable:

```bash
./target/release/btclip.exe
```

---

### Building the Mobile Client (Android)

1. Open the `mobile/` directory in **Android Studio**.
2. Build and install the APK onto your Android device:

```bash
./gradlew installDebug
```

3. Open the `btclip` app on your phone, grant the required Notification/Foreground Service permissions, and tap **Connect**.

---

## How It Works

### 1. Copying Text (Phone to PC)

When you copy text on either device, `btclip` reads the string, packs it into a `0x01` frame, and streams it over Bluetooth. The receiving device temporarily sets a lock flag (`is_internal_write`), updates its local clipboard, and suppresses echoing the change back.

### 2. Screenshot Transfer (Phone to PC)

1. You take a system screenshot on your phone (`Power + Vol Down`).
2. The phone OS saves the file to `/sdcard/Pictures/Screenshots`.
3. An active `ContentObserver` watching `MediaStore.Images.Media.EXTERNAL_CONTENT_URI` instantly catches the file creation event.
4. The Android app reads the image bytes and sends a `0x02` frame over RFCOMM.
5. The PC Rust engine receives the raw image bytes, decodes them into an uncompressed RGBA pixel buffer, and writes them directly to the OS clipboard via `arboard`.
6. Press `Ctrl + V` on your PC in any image-supported app (Paint, Discord, Teams, Slack) to paste immediately.

---

## Security & Privacy

- **Zero Internet Traffic:** No data leaves your personal physical space.
- **No Cloud Middleware:** Data is transferred directly through local RFCOMM socket streams without third-party signaling or relay servers.
- **Transient Storage:** Received images and text are held purely in volatile RAM buffers before being handed over to the local OS clipboard.

---

## License

Distributed under the MIT License. See `LICENSE` for more information.
