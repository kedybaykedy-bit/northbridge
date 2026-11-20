# northbridge

**One hub for every path between your devices** — discovery, pairing and
plugin-based sharing over LAN, Tailscale and USB, without a server.

northbridge combines ideas from three projects into one daemon + CLI:

| Source | What was taken |
|---|---|
| [Barrier](https://github.com/debauchee/barrier) | KVM-style input forwarding (planned, M2) |
| [LocalSend](https://github.com/localsend/localsend) | Multicast discovery, HTTPS file transfer, cert-fingerprint identity |
| [KDE Connect](https://kde.org/projects/kdeconnect/) | NetworkPacket protocol, plugin/capability model, pairing |

## Features

- **Discovery on every path** — LocalSend multicast on all interfaces (LAN,
  USB tethering), plus direct probing of [Tailscale](https://tailscale.com)
  peers. The same device seen on two paths is one device.
- **Channel-independent trust** — identity is the SHA-256 fingerprint of a
  self-signed certificate. Pair once over Wi-Fi; the device is recognized
  over USB or the tailnet too. No PKI, no accounts.
- **Pairing** — synchronous `nb.pair` exchange with capability lists
  (KDE Connect-style `incoming`/`outgoing` matching).
- **File transfer** — LocalSend v2 API over mTLS with fingerprint pinning.
  Transfers are only accepted from paired devices.
- **Clipboard sync** — text clipboard mirrors to paired devices, with
  state-based echo prevention. Backends: `wl-copy`, `xclip`, `xsel`.
- **Plugins** — features are plugins with declared packet-type
  capabilities; a peer only loads what both sides speak.

## Building

```bash
git clone https://github.com/kedybaykedy-bit/northbridge.git
cd northbridge
cargo build --release
```

Requirements: Rust toolchain, a LocalSend checkout next to this repo
(the `localsend` crate is a path dependency: `../localsend/packages/core`).

## Usage

```bash
# on each machine
northbridge listen                  # daemon: discover + pair + plugins
northbridge listen --no-auto-pair   # decline inbound pairing

# one-shot
northbridge discover                # what's out there?
northbridge devices                 # known devices
northbridge pair phone              # pair by alias or fingerprint prefix
northbridge ping phone              # RTT check over the packet channel
northbridge send phone report.pdf   # file transfer (paired devices only)
northbridge status                  # identity, ports, tailscale state
```

State lives in `~/.config/northbridge/` (`identity.json`, `peers.json`);
received files go to `~/Downloads/northbridge` (override with
`NORTHBRIDGE_DOWNLOAD_DIR`).

Ports: LocalSend HTTP `53317` (compatible with LocalSend apps), packet
channel `53318` (`http + 1`).

## Architecture

```
crates/nb-core        identity, NetworkPacket, trust store, Plugin trait, channels
crates/nb-transport   LocalSend HTTPS reuse + mTLS packet channel (fingerprint pinning)
crates/nb-discovery   multicast on all interfaces + tailscale status probing
crates/nb-cli         listen / discover / devices / pair / ping / send / status
plugins/nb-ping       smallest plugin — proves capability matching
plugins/nb-clipboard  clipboard text sync
```

- **Channels** (`lan`, `tailscale`, `usb`, `bluetooth`) are labels on
  observations, never part of identity — trust follows the fingerprint.
- **Packet channel**: newline-delimited JSON over mTLS; the server reads the
  peer fingerprint off the handshake, the client pins the server's.
- **Pairing** records the remote's capabilities; plugin dispatch uses
  KDE Connect's `pluginsForCapabilities` intersection rule.

See [PLAN.md](PLAN.md) for the milestone roadmap (M2: KVM / Barrier
protocol + Android companion, M3: screen, M4: audio bridge, M5: Bluetooth).

## License

[GPL-3.0-or-later](LICENSE).

This is possible thanks to the projects in the table above:
Barrier (GPL-2.0+), KDE Connect (GPL-2.0+), LocalSend (Apache-2.0).
