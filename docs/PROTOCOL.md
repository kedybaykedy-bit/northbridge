# northbridge input / KVM packet-channel protocol

Canonical reference for the **current implementation** (protocol version 1).
Sources: `crates/nb-transport/src/packet_channel.rs`, `crates/nb-transport/src/tls.rs`,
`crates/nb-core/src/packet.rs`, `crates/nb-input/src/lib.rs`, `crates/nb-cli/src/input.rs`,
`crates/nb-cli/src/x11.rs`, `crates/nb-cli/src/main.rs`.

---

## 1. Transport

| Property | Value |
|---|---|
| Socket | TCP, bound `0.0.0.0:<packet_port>` |
| Port | `packet_port = identity.port + 1` (default `53317 + 1 = 53318`) |
| TLS | rustls, **mutual TLS**. Server requires a client certificate (`client_auth_mandatory`), accepts any *valid* self-signed cert; client pins the server cert fingerprint (uppercase hex SHA-256 of DER) obtained from discovery, checked during the handshake before any packet byte is written |
| Peer identity | server reads the client cert (first cert of the chain) and derives the **fingerprint** — this is the device ID used everywhere (registry keys, trust store) |
| ALPN | none — raw TLS, no HTTP on this port |
| Framing | one compact JSON object per line, terminated by `\n` |
| Max line | 1 MiB (`MAX_LINE`); larger line ⇒ connection error `packet too large (N bytes)` |
| Read timeout | server: **none** on a connection (streams may idle for minutes); client `send_packet`: 5 s connect + 5 s first response (`DIAL_TIMEOUT`) |
| Disconnects | peer resets / `early eof` during handshake are swallowed silently |

Trust is checked **per packet** by the daemon handler: any packet type other than
`nb.pair` from an unpaired/untrusted fingerprint is answered with
`{"type":"nb.error","body":{"reason":"not paired — run \`northbridge pair\` first"}}`.

## 2. NetworkPacket envelope

```json
{"id":1795142400123,"type":"nb.touchpad","body":{"t":"rel_move","dx":-4.0,"dy":11.5}}
```

| Field | Type | Meaning |
|---|---|---|
| `id` | u64 | milliseconds since epoch, stamped at send time; **receivers ignore it** (no matching on `id`) |
| `type` | string | packet type / plugin namespace |
| `body` | any JSON | owned by the type; for input packets it is an `InputMsg` |

`PROTOCOL_VERSION = 1` travels inside `nb.pair` only, never as an envelope field.

> **Not this channel:** `nb-input::encode()` produces a 4-byte big-endian
> length-prefixed frame (`MAX_MESSAGE_LEN` = 1 MiB). That framing is a *separate*
> binary transport used elsewhere/tests. On the packet channel the framing is
> newline-delimited `NetworkPacket`; an `InputMsg` is the JSON inside
> `NetworkPacket.body`.

## 3. Connection modes

The **first line** of a connection selects the mode.

### 3.1 Classic (request → response → close)

Used by pairing, ping, clipboard, and any one-shot request.

1. Client connects (mTLS), writes one line, flushes.
2. Server runs the `PacketHandler`
   `Fn(&peer_fp, Channel, &SocketAddr, NetworkPacket) -> Option<NetworkPacket>`.
3. `Some(resp)` ⇒ exactly one response line is written, then the connection closes.
   `None` ⇒ connection closes with no response.

The client helper `send_packet(identity, host, packet_port, expected_fingerprint, pkt)`
does exactly this (5 s timeouts).

### 3.2 Stream (bidirectional input channel)

Entered **iff** the first line's type is `nb.touchpad.open` (`INPUT_OPEN_KIND`).

1. Server acks on the same connection:
   `{"type":"nb.touchpad.open","body":{"ok":true}}`.
2. `InputRegistry.open(peer_fp)` registers the connection — the registry maps
   **fingerprint → live stream writer** (one entry per fingerprint).
3. Every subsequent client line is passed to the packet handler; a `Some(resp)`
   is queued to the writer task and sent back. **No per-line response is
   required or expected** — input lines are fire-and-forget (except `ping`).
4. Independently, any server component can push lines to that peer through
   `InputRegistry::send(peer_fp, pkt) -> bool` (the KVM watcher pushes `nb.kvm`
   cursor messages through it). `false` = no live stream ⇒ producer treats the
   peer as gone and aborts (the capture watcher drops capture and restores the
   cursor).
5. Read EOF or a write error ends the stream; the registry entry is removed
   only if its `conn_id` still matches (a stale connection cannot evict a newer
   one). A second `open` from the same fingerprint **replaces** the entry —
   pushes then go to the newest connection.

Server→client pushes are only possible on a stream connection; classic
connections are gone after their single response.

## 4. Pairing and capabilities

Pairing is a classic-mode `nb.pair` exchange. Capabilities are the union of
each plugin's declared `incoming` / `outgoing` packet types, recorded in
`peers.json` and used for KDE-Connect-style plugin activation:
a plugin loads iff `remote_outgoing ∩ my_incoming ≠ ∅` **or**
`remote_incoming ∩ my_outgoing ≠ ∅` (both remote lists empty ⇒ all load).

Request body (initiator):

```json
{"protocol":1,"alias":"pixel","timestamp":1795142400000,
 "incoming":["nb.kvm"],"outgoing":["nb.touchpad"]}
```

Response body (desktop, auto-pair on by default; `--no-auto-pair` ⇒
`accepted:false` + `reason`):

```json
{"protocol":1,"accepted":true,"alias":"workstation","timestamp":1795142400012,
 "incoming":["nb.ping","nb.clipboard","nb.touchpad"],
 "outgoing":["nb.ping","nb.pong","nb.clipboard","nb.kvm"]}
```

Input capabilities:

| Side | `incoming` | `outgoing` | Plugin id |
|---|---|---|---|
| Desktop (`TouchpadPlugin`) | `nb.touchpad` | `nb.kvm` | `nb.touchpad` |
| Phone (mirror declaration) | `nb.kvm` | `nb.touchpad` | — |

Other kinds in play: `nb.pair` (classic), `nb.ping`/`nb.pong`,
`nb.clipboard` (classic), `nb.error` (server → client, always advisory).

## 5. Message catalog

`body` is an `InputMsg`: internally tagged `{"t": "<variant>"}` with
`snake_case` names.

### 5.1 Kind routing

| `type` | Direction | Meaning |
|---|---|---|
| `nb.touchpad.open` | phone → desktop | first line only; opens stream mode |
| `nb.touchpad` | phone → desktop | carries every phone→PC `InputMsg` |
| `nb.kvm` | desktop → phone | carries PC→phone cursor messages (server push) |
| `nb.error` | desktop → phone | `{"reason": "…"}`; advisory, skip it |

### 5.2 Phone → PC (`type = "nb.touchpad"`)

| `t` | Payload | Desktop effect |
|---|---|---|
| `ping` | — | responds `{"type":"nb.touchpad","body":{"t":"pong"}}` |
| `pong` | — | ignored, no response |
| `rel_move` | `dx`,`dy`: f64, pixels | warp pointer by `(dx as i32, dy as i32)` from current position; **dropped while capture is active** |
| `click_abs` | `x`,`y`: 0..1 | clamp → pixel `(x·(w−1), y·(h−1))`, warp + left tap; **dropped while capture is active** |
| `tap` | `b`, `count`: u8 | `count` clamped 1..3, XTEST press/release pairs, 10 ms apart |
| `button` | `b`, `down`: bool | XTEST press (`down`) / release |
| `scroll` | `dx`,`dy`: f64 | fractional accumulator per axis; each message emits up to 10 whole steps per axis as wheel buttons: `dy<0`→4, `dy>0`→5, `dx<0`→6, `dx>0`→7 |
| `key` | `code`: u16 (Linux evdev), `down` | XTEST key, **X keycode = `code + 8`**; silently ignored if `> max_keycode` |
| `type` | `s`: string | dedicated typing thread (second X connection): tap found keysym (with shift), else temporarily `ChangeKeyboardMapping` onto a free keycode, tap, then unmap |
| `capture` | — | start KVM capture (§7) |
| `release` | — | stop KVM capture (§7) |
| `enter`,`leave`,`move` | — | **ignored** coming from the phone (direction markers only) |

`b` is `left` | `right` | `middle` → X buttons 1 | 3 | 2.

### 5.3 PC → phone (`type = "nb.kvm"`, pushed)

| `t` | Payload | Meaning |
|---|---|---|
| `enter` | `x`,`y`: 0..1 | capture started; `x` is always `0.0`, `y = anchor_y / screen_h`. Phone should place its cursor here |
| `move` | `x`,`y`: 0..1 | virtual cursor position; pushed only on ticks where the **physical** pointer moved away from the anchor |
| `leave` | — | capture ended; desktop cursor restored |

Rate: watcher tick = 12 ms ⇒ ≤ ~83 `move` lines/s (~80 Hz in practice), only
while the physical mouse drifts.

## 6. Desktop injection semantics (X11 / XTEST via x11rb)

| Message | Injection |
|---|---|
| `rel_move` | `warp_pointer` by truncated delta (deltas are `as i32`, fractional part lost) |
| `click_abs` | warp + `tap(left, 1)` |
| `tap` | `xtest_fake_input` press+release |
| `button` | press/release, device = pointer (2) |
| `scroll` | wheel buttons 4/5/6/7 via XTEST, accumulator persists across messages |
| `key` | `xtest_fake_input` key press/release, device = keyboard (1), keycode `code+8` |
| `type` | keysym lookup → tap; unknown char → temporary keycode remap (max 10 chars free-keycode pressure handled by reuse) |
| coordinates | all floats normalized 0..1 over the **whole screen**; warps clamp to `[0, w−1] × [0, h−1]` |

XFixes (≥ 5.0) is used to hide/show the system cursor during capture; if XFixes
is unavailable the cursor simply stays visible.

## 7. KVM capture flow

```
phone                                   desktop
  │  {"t":"capture"}   (nb.touchpad)      │
  │──────────────────────────────────────>│  anchor = query_pointer()
  │                                       │  hide cursor (XFixes)
  │  {"t":"enter","x":0.0,"y":ny} (nb.kvm)│  vpos = (0.0, ny)
  │<──────────────────────────────────────│  spawn watcher thread (12 ms)
  │                                       │
  │        … physical mouse moves …       │  every tick: query pointer
  │  {"t":"move","x":…,"y":…}   (nb.kvm)  │   p ≠ anchor ⇒ vpos += Δ/size,
  │<──────────────────────────────────────│   clamp, warp back to anchor,
  │                                       │   push move, repeat
  │  {"t":"release"} (nb.touchpad)        │
  │──────────────────────────────────────>│  take capture (only if same fp)
  │  {"t":"leave"}   (nb.kvm)             │  show cursor, stop watcher
  │<──────────────────────────────────────│
```

Rules:

- **One capture at a time**; a second `capture` (or any `capture` while the X
  display is unavailable) is ignored — no reply, no `enter`.
- While captured, incoming `rel_move` and `click_abs` are **ignored**: the
  physical mouse drives the KVM cursor (it is warped back to the anchor each
  tick, so the desktop user's pointer is pinned).
- `release` from a *different* fingerprint than the captor is rejected
  (capture state is kept).
- The watcher ends capture (and restores the cursor) if the pointer query
  fails or `InputRegistry::send` reports the peer's stream gone.
- `enter`/`move`/`leave` require an open stream (`§3.2`); without one,
  `start_capture` refuses with `capture: peer has no input stream`.

## 8. Wire examples

Exact lines (`…` marks omitted `id`s; `→` phone→desktop, `←` desktop→phone).

### Open / ack

```json
→ {"id":1795142400100,"type":"nb.touchpad.open","body":{}}
← {"id":1795142400118,"type":"nb.touchpad.open","body":{"ok":true}}
```

The open line itself is never passed to the plugin handler.

### Ping / pong

```json
→ {"id":1795142400500,"type":"nb.touchpad","body":{"t":"ping"}}
← {"id":1795142400506,"type":"nb.touchpad","body":{"t":"pong"}}
```

### Touchpad gesture sequence

```json
→ {"id":1795142401000,"type":"nb.touchpad","body":{"t":"rel_move","dx":-4.0,"dy":11.5}}
→ {"id":1795142401040,"type":"nb.touchpad","body":{"t":"rel_move","dx":0.5,"dy":6.0}}
→ {"id":1795142401080,"type":"nb.touchpad","body":{"t":"tap","b":"left","count":1}}
→ {"id":1795142401120,"type":"nb.touchpad","body":{"t":"scroll","dx":0.0,"dy":-3.5}}
→ {"id":1795142401160,"type":"nb.touchpad","body":{"t":"key","code":28,"down":true}}
→ {"id":1795142401180,"type":"nb.touchpad","body":{"t":"key","code":28,"down":false}}
→ {"id":1795142401220,"type":"nb.touchpad","body":{"t":"type","s":"merhaba"}}
```

### Capture → enter → move → leave

```json
→ {"id":1795142402000,"type":"nb.touchpad","body":{"t":"capture"}}
← {"id":1795142402007,"type":"nb.kvm","body":{"t":"enter","x":0.0,"y":0.4166}}
← {"id":1795142402015,"type":"nb.kvm","body":{"t":"move","x":0.0089,"y":0.4215}}
← {"id":1795142402027,"type":"nb.kvm","body":{"t":"move","x":0.0231,"y":0.4310}}
   … up to ~83 move lines per second while the physical pointer drifts …
→ {"id":1795142405000,"type":"nb.touchpad","body":{"t":"release"}}
← {"id":1795142405004,"type":"nb.kvm","body":{"t":"leave"}}
```

`capture`, `release` and all touchpad gestures produce **no** response line —
delivery is TCP-acknowledged; only `ping` (→ `pong`) answers inline, and
`nb.kvm` pushes arrive whenever the desktop wants them.

### Classic pair (for contrast)

```json
→ {"id":1795142399000,"type":"nb.pair","body":{"protocol":1,"alias":"pixel","timestamp":1795142399000,"incoming":["nb.kvm"],"outgoing":["nb.touchpad"]}}
← {"id":1795142399014,"type":"nb.pair","body":{"protocol":1,"accepted":true,"alias":"workstation","timestamp":1795142399014,"incoming":["nb.ping","nb.clipboard","nb.touchpad"],"outgoing":["nb.ping","nb.pong","nb.clipboard","nb.kvm"]}}
   (connection closes)
```

## 9. Known quirks / implementation notes

1. **`nb.error` only for truly unmatched types.** A line gets one `nb.error`
   reply (`"unknown or inactive packet type: <kind>"`) when *no active plugin
   declares that type* (including capability-inactive or plain unknown types).
   Handled-but-silent input lines (`capture`, gestures, …) produce **no**
   response at all. Clients must still treat `nb.error` as advisory and skip
   it — `nb-cli/examples/fake_phone.rs` matches on `type` predicates and
   ignores everything else.
2. **Direction is enforced by convention, not on the wire.** The desktop parses
   any `InputMsg` out of an `nb.touchpad` body and ignores `enter`/`leave`/
   `move`; the phone must ignore anything in `nb.kvm` other than
   `enter`/`move`/`leave`.
3. **`enter.x` is hard-coded to `0.0`**; the desktop only forwards the anchor's
   normalized `y`.
4. **`rel_move` truncates to integer pixels**; sub-pixel deltas are lost
   (accumulate on the phone side if needed).
5. **One stream per fingerprint.** Re-opening replaces the previous stream;
   pushes follow the newest connection.
6. **No read timeout** on stream connections: keepalives are the app's job
   (`ping`), the connection can idle indefinitely.
7. `id` values are informational; never correlate request and response by `id`.

## 10. Versioning

`PROTOCOL_VERSION = 1` (sent in `nb.pair.protocol`). New packet types and new
`InputMsg` variants are additive; receivers must ignore unknown `t` values
(desktop: unknown body in a declared type ⇒ `on_packet` returns `None`, i.e.
no line back; unknown *type* ⇒ one advisory `nb.error`).
