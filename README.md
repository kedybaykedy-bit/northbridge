# northbridge

**One hub for every path between your devices** — discovery, pairing and
plugin-based sharing over LAN, Tailscale and USB, without a server.

northbridge combines ideas from three projects into one daemon + CLI +
Android companion + desktop GUI:

| Source | What was taken |
|---|---|
| [Barrier](https://github.com/debauchee/barrier) | KVM-style input forwarding: PC controls the phone and vice versa |
| [LocalSend](https://github.com/localsend/localsend) | Multicast discovery, HTTPS file transfer, cert-fingerprint identity |
| [KDE Connect](https://kde.org/projects/kdeconnect/) | NetworkPacket protocol, plugin/capability model, pairing |

## Features

### Connection
- **Discovery on every path** — LocalSend multicast on all interfaces (LAN,
  USB tethering), plus direct probing of [Tailscale](https://tailscale.com)
  peers. The same device seen on two paths is one device.
- **Channel-independent trust** — identity is the SHA-256 fingerprint of a
  self-signed certificate. Pair once over Wi-Fi; the device is recognized
  over USB or the tailnet too. No PKI, no accounts.
- **Self-healing addresses** — discovery keeps running for the lifetime of
  the app; a peer that changes address (DHCP, USB tethering) is re-adopted
  automatically on the next observation.
- **Pairing** — synchronous `nb.pair` exchange with capability lists
  (KDE Connect-style `incoming`/`outgoing` matching).

### Sharing
- **File transfer** — LocalSend v2 API over mTLS with fingerprint pinning.
  Transfers are only accepted from paired devices.
- **Clipboard sync** — text clipboard mirrors to paired devices, with
  state-based echo prevention. Backends: `wl-copy`, `xclip`, `xsel`.
- **Plugins** — features are plugins with declared packet-type
  capabilities; a peer only loads what both sides speak.

### Control (phone ↔ PC)
- **KVM input sharing** — the phone's touchpad and keyboard drive the PC;
  the PC's mouse drives the phone. The pointer takes the mouse's *raw*
  movement (XInput2 RawMotion) instead of mirroring the PC pointer, so
  motion is resolution-independent, proportional, and can never hit an
  invisible wall at the desktop edge.
- **Remote control panel** — wake, open app, list apps, sleep, from the
  phone.
- **Turkish keyboard support** — the full X11 legacy keysym range
  (`ğ İ ı Ş` and friends) maps to characters, plus a built-in input method
  that commits Unicode text directly — no clipboard round-trip, no toast.

### Media (phone ↔ PC)
- **Screen stream** — the phone mirrors the PC screen (H.264 over a
  dedicated channel) with full-screen and landscape modes.
- **Mirror with taps** — the PC screen in the palm of your hand, tappable.
- **Two-way audio bridge** — microphone ↔ speakers over LAN, low latency,
  echo-guarded.

### Apps
- **Desktop GUI** — egui-based remote control surface (lightest
  cross-platform option).
- **Linux integration** — `.desktop` entry + icon in `packaging/`.

## Building

```bash
git clone https://github.com/kedybaykedy-bit/northbridge.git
cd northbridge
cargo build --release
```

Requirements: Rust toolchain, a LocalSend checkout next to this repo
(the `localsend` crate is a path dependency: `../localsend/packages/core`).

Android companion:

```bash
cd android
JAVA_HOME=... gradle assembleDebug     # APK: app/build/outputs/apk/debug/
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

NDK is required for the native pointer module (`ndkVersion` is pinned in
`build.gradle`).

## Usage

```bash
# on each machine
northbridge listen                  # daemon: discover + pair + plugins

# one-shot
northbridge discover                # what's out there?
northbridge devices                 # known devices
northbridge pair phone              # pair by alias or fingerprint prefix
northbridge ping phone              # RTT check over the packet channel
northbridge send phone report.pdf   # file transfer (paired devices only)
northbridge wake phone              # remote control actions
northbridge apps phone              # list launchable apps
northbridge status                  # identity, ports, tailscale state
```

State lives in `~/.config/northbridge/` (`identity.json`, `peers.json`);
received files go to `~/Downloads/northbridge` (override with
`NORTHBRIDGE_DOWNLOAD_DIR`).

Ports: LocalSend HTTP `53317` (compatible with LocalSend apps), packet
channel `53318` (`http + 1`), screen `53319`, audio `53320`, mirror `53321`.

## Architecture

```
crates/nb-core        identity, NetworkPacket, trust store, Plugin trait, channels
crates/nb-transport   LocalSend HTTPS reuse + mTLS packet channel (fingerprint pinning)
crates/nb-discovery   multicast on all interfaces + tailscale status probing
crates/nb-input       shared input protocol (keys, pointers, gestures)
crates/nb-mirror      mirror + KVM server: X11 grab, XI2 RawMotion, frame pump
crates/nb-screen      screen stream (H.264 capture → channel)
crates/nb-audio       audio bridge (PipeWire capture → channel)
crates/nb-gui         egui desktop remote control
crates/nb-cli         listen / discover / devices / pair / ping / send / wake / apps / status
plugins/nb-ping       smallest plugin — proves capability matching
plugins/nb-clipboard  clipboard text sync

android/               Compose companion: discovery, touchpad+keyboard,
                       KVM, mirror, screen, audio, files, IME
```

- **Channels** (`lan`, `tailscale`, `usb`, `bluetooth`) are labels on
  observations, never part of identity — trust follows the fingerprint.
- **Packet channel**: newline-delimited JSON over mTLS; the server reads the
  peer fingerprint off the handshake, the client pins the server's.
- **Pairing** records the remote's capabilities; plugin dispatch uses
  KDE Connect's `pluginsForCapabilities` intersection rule.
- **KVM pointer path**: mouse → XInput2 RawMotion (unclamped) → `md` delta
  packets → phone integrates the deltas directly (no PC-pointer mirroring,
  no per-report damping — the pointer can only stop at the phone's own
  screen edge).

## Roadmap

Battery sharing, notification mirroring, virtual display, remote lock
pattern, shared memory screen streaming, multi-device (2 PCs ↔ 2 phones).
See [PLAN.md](PLAN.md) for the milestone detail.

## License

[GPL-3.0-or-later](LICENSE).

This is possible thanks to the projects in the table above:
Barrier (GPL-2.0+), KDE Connect (GPL-2.0+), LocalSend (Apache-2.0).
