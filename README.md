# btclip

> **Offline bidirectional clipboard and screenshot sync between Android and Windows over Bluetooth RFCOMM.**

`btclip` pairs a Windows Rust host and an Android Kotlin client using raw Bluetooth SPP without internet or cloud services.

---

## Features

- **Private Bluetooth Sync:** Clipboard and image data move directly over RFCOMM.
- **Bidirectional Text:** Copy from Android to PC or from PC to Android.
- **Screenshot Image Transfer:** Send screenshots from phone to PC as image clipboard payloads.
- **Lightweight:** Rust host plus Kotlin client keep the app footprint small.
- **Frame Protocol:** Uses a fixed 5-byte frame header for robust stream parsing.

## Architecture

| Component      | Location         | Role                                                                                     |
| :------------- | :--------------- | :--------------------------------------------------------------------------------------- |
| **PC Host**    | Repository root  | Rust RFCOMM server, Windows clipboard bridge, binary frame parser/encoder.               |
| **Android App**| `client/`        | Kotlin foreground client, Bluetooth SPP, clipboard bridge, FileProvider image sharing.   |
| **Transport**  | Bluetooth RFCOMM | Standard SPP UUID `00001101-0000-0000-0000-00805F9B34FB` for raw socket transport.       |

## Wire Protocol Spec

Frames are encoded as:

```text
[1-byte Type][4-byte Big-Endian Length][Payload]
```

- `0x01` = UTF-8 text
- `0x02` = PNG/JPEG image bytes
- Length = Big-endian unsigned 32-bit payload size
- Payload = exact text or image bytes

## Project Structure

```text
btclip/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── bluetooth.rs
│   ├── clipboard.rs
│   └── protocol.rs
└── client/
    ├── build.gradle.kts
    ├── gradle.properties
    ├── settings.gradle.kts
    ├── gradle/
    │   └── wrapper/
    ├── gradlew
    ├── gradlew.bat
    └── app/
        ├── build.gradle.kts
        └── src/main/
            ├── AndroidManifest.xml
            ├── java/com/forgata/btclip/
            │   ├── MainActivity.kt
            │   └── ProtocolParser.kt
            └── res/xml/file_paths.xml
```

## Android Client Notes

The Android code is isolated under `client/`. The client includes:

- `MainActivity.kt` for runtime permission requests
- `ProtocolParser.kt` for RFCOMM frame parsing over an `InputStream`
- `AndroidManifest.xml` with Bluetooth and foreground service permissions
- `file_paths.xml` to share cache files through `FileProvider`

## Installation

### Build the PC Host

From the repository root:

```powershell
cargo build --release
```

Run the host binary:

```powershell
.\target\release\btclip.exe
```

### Build the Android Client

From the `client/` directory:

```powershell
cd client
.\gradlew.bat assembleDebug --no-daemon
```

Or on Unix-like shells:

```bash
cd client
./gradlew assembleDebug --no-daemon
```

### Install the Android APK

Using the Gradle wrapper:

```powershell
cd client
.\gradlew.bat installDebug
```

Or directly with ADB after building:

```powershell
adb install -r app\build\outputs\apk\debug\app-debug.apk
```

## Usage

1. Pair your Android phone and Windows PC in system Bluetooth settings.
2. Start the PC host binary.
3. Install and run the Android client.
4. Grant Bluetooth and notification permissions when prompted.
5. Use clipboard copy on either side to sync text or images.

## Notes

- Keep Android source strictly inside `client/`.
- The host lives in the repository root as a Rust crate.
- `client/` already includes Gradle wrapper files for CLI builds.
