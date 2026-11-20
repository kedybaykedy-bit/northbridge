# M3 research — live screen streaming, desktop → Android

Research for the next milestone: a VNC-like live view of the desktop on the
phone (one direction: desktop → Android; input already works via `nb.touchpad`
/ `nb.kvm`, see [PROTOCOL.md](PROTOCOL.md)).

**Current state assumed:** Linux desktop with a confirmed X11 session (Wayland
is a future concern, not today's target); `x11rb` already in the tree for input
injection; identity = self-signed cert fingerprint with mTLS on ports 53317
(LocalSend HTTPS) and 53318 (packet channel); workspace licensed
GPL-3.0-or-later; Android companion app in M2.

---

## 1. Desktop capture options

| Approach | Cost / latency | New dependencies | License | Wayland readiness | Notes |
|---|---|---|---|---|---|
| **x11rb + MIT-SHM (`GetImage`/shm on the root window)** | Lowest: one shared-memory copy of the framebuffer per frame (~2–8 ms for 1080p, ballpark) | **None** — `x11rb` is already a dependency | MIT | ✗ X11 only | Full control of rate/region; **cursor is not part of the image** (X draws the pointer separately) — overlay it from XFixes cursor-image data, the trick x11vnc uses |
| **`xcap` crate** | Good; API-level convenience (region, video recorder), one extra copy | `libxcb`, `libdbus`, `libpipewire`, `libwayland`, `clang` at build time | Apache-2.0 | **Partial**: README implementation matrix shows Linux/X11 ✅, Linux/Wayland ⛔ (available but "not fully supported in some special scenarios"); Linux backend is PipeWire-based | Video recorder API still WIP; screen capture is solid — https://crates.io/crates/xcap |
| **`scap` crate** | Good; explicit fps/resolution/crop options, frame types (BGRA) | PipeWire on Linux | MIT | ✓ — Linux = PipeWire by design (same path as the portal) | Young (0.0.8 stable, 0.1.0-beta); permission model built in — https://crates.io/crates/scap |
| **`screenshots` crate** | — | — | Apache-2.0 | ✗ | **Deprecated**, description is literally "Move to XCap" — skip: https://crates.io/crates/screenshots |
| **ffmpeg subprocess** (`-f x11grab`, or PipeWire input) | One frame of pipe latency + encode inside the child; process isolation | Runtime `ffmpeg` binary only (no build-time Rust deps) | Depends on build (LGPL core; encoders vary — see §2 licensing) | `x11grab` ✗, ffmpeg's PipeWire capture ✓ | Easiest access to VAAPI/x264 with correct low-latency flags; crash-isolated |
| **GStreamer** (`ximagesrc` / `pipewiresrc` → encoder → `appsink`) | Comparable to ffmpeg; framework overhead | `libgstreamer` + plugins | MIT (Rust bindings) | ✓ via `pipewiresrc` | Heaviest runtime footprint; overkill for one stream |
| **XDG ScreenCast portal + PipeWire** | ~1 frame extra, plus a one-time consent dialog | `libpipewire`, D-Bus (`ashpd`/`zbus`) | MIT | ✓ **The** Wayland-safe path | Flow: `CreateSession → SelectSources → Start → OpenPipeWireRemote()` returns a PipeWire fd; `restore_token`/`persist_mode` avoid re-prompting every session — https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html |
| **DRM/KMS direct** | Fastest possible (Sunshine uses it) | root/device access | — | ✗ | Too invasive for a user-space hub daemon; ignore |

Sunshine's own capture matrix (X11, KMS/DRM, NvFBC, wlroots-Wayland, XDG
portal, KWin screencast) is a useful roadmap of what production code supports:
https://github.com/LizardByte/Sunshine

**Verdict for capture:** implement a `ScreenSource` trait; ship the **XShm
implementation first** (zero new deps, matches the confirmed X11 session), and
add a **PipeWire-portal implementation behind a feature flag** as the Wayland
answer — the portal path is also usable on X11 GNOME/KDE, so it can eventually
become the default without touching transport.

## 2. Encoding for lowest latency to Android

Ballpark figures (order of magnitude, desktop UI content, LAN — not benchmarks):

| Codec / mode | ~720p30 | ~1080p30 | Desktop encode | Android decode | Failure behaviour | Latency profile |
|---|---|---|---|---|---|---|
| **MJPEG** (per-frame JPEG) | 20–70 Mbit/s | 40–140 Mbit/s | Pure Rust, ~3–10 ms/frame (`zune-jpeg` 0.5.x, MIT/Apache/Zlib; `jpeg-encoder` 0.7.x) | Trivial (`BitmapFactory`), any device, no codec negotiation | **Perfect** — every frame independent, no recovery logic | ~1 frame; no GOP, no state |
| **Raw RGBA + LZ4** | 90–220 Mbit/s | 200–500 Mbit/s | LZ4 block compress ~1–3 ms (`lz4` crate, MIT; lib BSD-2-Clause) | Decompress + blit; pure CPU copy of 3–8 MB/frame | Perfect | Lowest codec delay, but eats WiFi for breakfast at 1080p |
| **H.264 (baseline/main, no B-frames, short GOP)** | 3–8 Mbit/s | 5–15 Mbit/s | In-process `openh264` 0.9.x (**BSD-2-Clause**, wraps Cisco's codec) ~5–20 ms/frame; VAAPI hardware < 5 ms | **MediaCodec hardware**, zero-copy into a `Surface`; add low-latency mode (API 30+, see §4) | TCP ⇒ no packet loss; only late frames ⇒ drop-oldest + IDR on (re)start | 1–3 frames with low-latency mode |
| **VP8/VP9 via WebRTC** | 2–6 Mbit/s | 4–12 Mbit/s | libvpx | MediaCodec HW | WebRTC owns recovery | Jitter buffer + ICE + congestion control add delay and massive integration cost — wrong tool for a trusted LAN |
| **AV1** | — | — | Software encode too slow today | HW on recent phones only | — | Not for M3 |

**Bandwidth sanity checks** (arithmetic, not measurements):
raw RGBA 1280×720×4 B×30 fps ≈ **885 Mbit/s**; 1920×1080 ≈ **2.0 Gbit/s** —
raw is only viable downsampled/short-burst. Gigabit LAN and 5 GHz WiFi (50–300
Mbit/s usable) comfortably carry MJPEG at 720p and H.264 at 1080p; raw+LZ4 at
1080p does not.

**Pragmatic default:** **MJPEG at ≤1280×720, 10–15 fps, quality knob** for the
first stream (simple, stateless, trivially decodable), with **H.264** as the
M3-b upgrade for 1080p30 on WiFi. Quality/latency knobs to expose from day one:
`fps`, `max_width`, `quality` (JPEG) / `bitrate`+`gop` (H.264), plus an
explicit *frame-drop-oldest* policy (never queue more than one pending frame).

**Licensing note (GPL-3.0 workspace):**
* `openh264` (BSD-2-Clause) and `zune-jpeg`/`jpeg-encoder`/`lz4` are safe to link.
* x264 is "released under the terms of the GNU GPL" (https://www.videolan.org/developers/x264.html)
  — do **not** link x264 into this GPL-3.0 binary; use it only out-of-process
  (ffmpeg as a separate executable), or use ffmpeg's own encoders/VAAPI.
* VAAPI encode (`h264_vaapi`) lives in the GPU driver stack — no license entanglement.

## 3. End-to-end latency budget (estimates)

| Stage | M3-a (MJPEG) | M3-b (H.264) |
|---|---|---|
| Capture (XShm copy + downscale) | 5–15 ms | 5–15 ms |
| Encode | 3–10 ms (JPEG) | 5–20 ms software / <5 ms VAAPI |
| TCP + LAN/WiFi | 1–5 ms gigabit / 5–30 ms WiFi | same |
| Android decode | 5–15 ms (JPEG) | 8–16 ms (HW, 1–2 frames) |
| Render | 0–33 ms (1–2 vsync) | 0–16 ms (`SurfaceView`, direct) |
| **Realistic total** | **~120–250 ms** | **~60–150 ms** |

Two structural rules dominate: (a) never let more than one frame sit in a
queue, (b) TCP head-of-line blocking on WiFi is the main jitter source — bounded
queues, not retransmission logic, are the mitigation.

## 4. Android receive side

| Approach | Latency | Effort | Verdict |
|---|---|---|---|
| **`SurfaceView` + `MediaCodec` (decode to Surface)** | Lowest: zero-copy HW decode straight onto the surface; enable low-latency mode — `MediaCodec.PARAMETER_KEY_LOW_LATENCY` / `MediaFormat.KEY_LOW_LATENCY` ("the decoder doesn't hold input and output data more than required by the codec standards", API 30) and `CodecCapabilities.FEATURE_LowLatency` | Medium: feed Annex-B from a socket, handle `INFO_OUTPUT_FORMAT_CHANGED`, IDR on start | **M3-b target** — https://developer.android.com/reference/android/media/MediaCodec |
| **`BitmapFactory`/`ImageDecoder` → `SurfaceView`/`ImageView` canvas** | Low (JPEG decode is single-digit ms at 720p) | Trivial | **M3-a target** — no codec lifecycle, no format negotiation |
| **Media3 / ExoPlayer** | Higher: player buffer, live-config tuning needed; designed around segmented/adaptive playback, not raw Annex-B-over-TCP | Low for HLS, medium-high for custom | Use only if we later wrap the stream in HLS/DASH or want an off-the-shelf player — https://developer.android.com/media/media3/exoplayer |
| **WebView + MJPEG (`<img src>` on `multipart/x-mixed-replace`)** | ~100–200 ms (browser paint loop) | Near zero | Great as a *debug/web viewer* (the noVNC idea); weak TLS story for client-cert auth, so secondary on Android |
| **WebRTC (libwebrtc)** | Good but jitter-buffered; ICE/DTLS/SRTP stack | Very high (build size, maintenance) | Reject for a trusted LAN |

Also plan for: rotation / aspect-ratio mismatch (letterbox by default, `fit`/
`crop` option), and re-encoding or stream restart when the phone rotates.

## 5. Transport integration

| Option | Auth | Pros | Cons |
|---|---|---|---|
| **A. Packet channel (JSON lines) carries frames** | Existing mTLS + fingerprint | Zero new ports | Base64/JSON ≈ +33 % bytes, 1 MiB line cap, JSON parse per frame — control plane only, never the payload |
| **B. Dedicated mTLS stream port** (`packet_port + 1` = 53319 default) reusing `tls::server_config` / `tls::client_config` | Same identity model: client cert mandatory, server derives fingerprint, gate on `peers[fp].trusted` (same rule as file transfers — `cert_fingerprint … unspoofable (mTLS)`) | Binary frames, no per-frame parsing, reuses everything already built, one listener | One more port to firewall/announce |
| **C. MJPEG over the existing LocalSend HTTPS (53317)** | Same mTLS client-cert fingerprint | No new port; a browser/`<img>` client can view it | Endpoint code lives in the `localsend` crate (upstream coupling); MJPEG keep-alive semantics bolted onto a request/response API |

**Recommendation:** **B for frames, A for control.**

Control plane (on the existing stream connection from PROTOCOL.md §3.2):

```json
→ {"id":1795142410000,"type":"nb.screen.open","body":{"fps":15,"max_width":1280,"codec":"mjpeg","quality":75}}
← {"id":1795142410012,"type":"nb.screen.open","body":{"ok":true,"stream_port":53319,"codec":"mjpeg"}}
```

plus `nb.screen.close` / `nb.screen.config`, capability `nb.screen` exchanged at
pairing (absent capability ⇒ hide the feature on both sides). Data plane frame
header (28 bytes, little-endian) — same shape for both codecs:

| magic `NBS1` | ver | codec (0=mjpeg, 1=h264) | flags (bit0 = keyframe) | seq u32 | pts_ms u64 | w u16 | h u16 | len u32 |

For H.264 the payload is raw Annex-B (SPS/PPS + IDR on connect, periodic IDR).

## 6. Open-source references worth borrowing from

| Project | License | What to copy (structurally) |
|---|---|---|
| **Barrier** — https://github.com/debauchee/barrier | GPL-2.0+ | Input-only KVM; confirms there is *no* video path to learn from. Its separation of input protocol from transport is already mirrored in northbridge |
| **Deskreen** — https://github.com/pavlobu/deskreen | AGPL-3.0 | Capture → WebRTC → *browser* viewer; short pairing-code UX; "any device with a browser is a second screen". Take ideas only (AGPL), never code |
| **x11vnc** — https://github.com/LibVNC/x11vnc + **noVNC** — https://github.com/novnc/noVNC | GPL-2.0 / see repo | Rectangle+damage update model, cursor handled as a separate XFixes overlay (not in the framebuffer), and noVNC as the template for an optional web viewer |
| **LibVNCServer** — https://github.com/LibVNC/libvncserver | GPL-2.0 | Encoding negotiation and incremental-update framing discipline |
| **RustDesk** — https://github.com/rustdesk/rustdesk | AGPL-3.0 | *Structure*: separate control and video sockets, per-frame header (seq/pts/keyframe), codec negotiation at session start, drop-late-frames client policy |
| **Moonlight / Sunshine** — https://github.com/moonlight-stream/moonlight-qt, https://github.com/LizardByte/Sunshine | GPL-3.0 / GPLv3 host | The full pipeline shape: capture → (HW) encode → packetize → decode-to-surface; input on its own channel (northbridge already does this); adaptive bitrate for WiFi; Sunshine's capture/encoder compatibility matrix as a checklist |
| **waypipe** — https://gitlab.freedesktop.org/mstoeckl/waypipe | see repo | Forwards *Wayland clients* over SSH, not screen content — not applicable to M3 |

## 7. Recommendation

**Reuse northbridge's existing identity and mTLS for both planes: signal
screen sessions as a new `nb.screen` capability over the packet channel's
stream mode, and push frames over a dedicated mTLS TCP port (`packet_port+1`)
that reuses `tls::server_config` and gates on the peer fingerprint's `trusted`
bit — no new auth model, and a stolen-port scanner still faces mandatory client
certs.** For **M3-a**, capture with the already-present `x11rb` via MIT-SHM
(X11 is the confirmed session), downscale to ≤1280×720, and send **MJPEG at
10–15 fps** with a one-frame, drop-oldest queue: no codec negotiation, no
inter-frame state, no loss-recovery logic, trivial Android decode
(`BitmapFactory` → `SurfaceView`), ~20–70 Mbit/s that both gigabit and 5 GHz
WiFi absorb, and a realistic ≤250 ms end-to-end budget — an M3-a you can build
in days with zero new runtime dependencies. Then in **M3-b** step up to
**H.264 Annex-B** (no B-frames, short GOP, IDR-on-connect) encoded in-process
with `openh264` (BSD-2, GPL-3-safe) — or out-of-process ffmpeg/VAAPI when
hardware encoding is wanted, which also keeps GPL-2 x264 out of the binary —
and decoded by Android's `MediaCodec` straight onto a `Surface` with
`PARAMETER_KEY_LOW_LATENCY`, landing 1080p30 at ~5–15 Mbit/s and ≤150 ms.
Everything sits behind a `ScreenSource` / `ScreenEncoder` trait pair so the XShm
capture can later be swapped for the XDG ScreenCast portal (PipeWire) when
Wayland arrives, without touching transport or the Android app.

## 8. Staged milestone plan

### M3-a — minimal viable stream ("see my desktop on the phone")

| # | Deliverable | Detail |
|---|---|---|
| 1 | `ScreenSource` trait + `XShmSource` | x11rb MIT-SHM grab of the root window, region = chosen monitor, downscale ×2 to ≤720p |
| 2 | Capture loop | timer-driven (10–15 fps), **drop-oldest single-frame queue**, optional cheap dirty-tile check to skip identical frames |
| 3 | MJPEG encoder | `zune-jpeg`, quality knob, `fps`/`max_width` knobs |
| 4 | Data port | mTLS accept via `tls::server_config`, `trusted` fingerprint gate, 28-byte `NBS1` frame header, `sendfile`-free plain `write_all` + flush per frame |
| 5 | Control | `nb.screen` capability at pairing; `nb.screen.open`/`close` over the existing stream connection |
| 6 | Android | connect → read header → decode JPEG → draw on `SurfaceView`; Start/Stop in UI; coexists with the input stream |

**Exit criteria:** end-to-end ≤ 250 ms on LAN at 720p15; stable ≥ 30 min
(no fd/memory growth); stream refused for any fingerprint that is not paired
and stops the moment the peer disconnects or is unpaired; no regression in the
touchpad/KVM path.

### M3-b — polished stream ("actually pleasant to use")

| # | Deliverable | Detail |
|---|---|---|
| 1 | `ScreenEncoder` trait + H.264 | `openh264` in-process default; optional `ffmpeg` subprocess feature (`-tune zerolatency -bf0 -g 30`, `h264_vaapi` when present); codec chosen in `nb.screen.open` (`mjpeg` \| `h264`) |
| 2 | Android MediaCodec playback | Annex-B feed, `PARAMETER_KEY_LOW_LATENCY`, render-on-`onFrameAvailable`, IDR on start and on reconnect |
| 3 | Adaptive quality | measure capture→queue time; shed fps first, then resolution; expose bitrate/GOP knobs |
| 4 | Robustness | monitor selection, rotation & resolution-change handling (stream restart), XFixes cursor overlay, disconnect/reconnect with IDR request |
| 5 | Observability | `nb.screen.stats` (fps, encode ms, queue depth, bytes/s) + CLI `northbridge screen phone` for desktop-side debugging |
| 6 | Wayland prototype | `PortalSource` behind a cargo feature (ScreenCast portal + PipeWire), parity at ≥ 720p15 |

**Exit criteria:** 1080p30 ≤ 150 ms on gigabit and ≤ 250 ms on 5 GHz WiFi with
bounded queue (latency does not grow under WiFi jitter); cursor visible in the
stream; rotation handled without a stuck frame; Wayland prototype streams via
the portal; H.264 chosen automatically for ≥1080p, MJPEG still available as
fallback.

*(Possible M3-c: UDP/FEC transport à la Moonlight, WebRTC interop, multi-viewer.)*

## 9. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Wayland kills XShm **and** XTEST input | Whole M3 design breaks on Wayland sessions | `ScreenSource` trait from day one; portal+PipeWire implementation (M3-b feature flag); input already has a future path via the portal's Remote Desktop/libei |
| X11 pointer cursor not in the framebuffer | Stream looks "cursor-less" | XFixes `GetCursorImage` overlay at capture time (x11vnc approach) |
| TCP head-of-line blocking on WiFi | Latency spikes, frozen video | Single-slot queue, drop-oldest, small `SO_SNDBUF`, fps shedding; consider UDP only if M3-b numbers fail |
| Encoder licensing (GPL-2 x264 vs GPL-3.0 project) | Legal | In-process `openh264` (BSD-2) / `zune-jpeg` / `lz4`; x264 only via out-of-process ffmpeg |
| Screen content is sensitive | Privacy | Paired-fingerprint gate (same rule as file transfer), explicit start, stream dies with the connection, visible "streaming" indicator on both ends |
| Codec/device fragmentation on Android | Black frames on odd devices | Keep MJPEG as universal fallback; query decoder capabilities and fall back |
| Aspect-ratio mismatch (desktop wide, phone tall) | Unreadable view | Letterbox default; `fit`/`crop` option; optional downscale-by-width |
| Multi-monitor / HiDPI | Wrong pixels, huge frames | Monitor index in `nb.screen.open`; downscale to `max_width` before encode |
| Rotation / resolution change mid-stream | Encoder/decoder desync | Detect → stream restart with new dimensions (IDR) |
| `xcap`/`scap` churn (young crates) | Build breakage | Prefer own XShm capture; treat crates as optional backends behind the trait |
| Extra runtime deps (ffmpeg/GStreamer) | Deploy friction | Optional features only; M3-a must run with zero new system packages |

## 10. Sources

Links checked 2026-11-20 (a few vendor sites serve bot challenges; URLs are the
canonical locations).

**Capture / crates**
- https://crates.io/crates/xcap — X11+Wayland, Apache-2.0, video WIP; README status matrix: https://github.com/nashaofu/xcap
- https://crates.io/crates/scap — MIT, Linux capture via PipeWire
- https://crates.io/crates/screenshots — deprecated ("Move to XCap")
- https://docs.rs/zune-jpeg, https://docs.rs/jpeg-encoder, https://docs.rs/lz4 — JPEG/LZ4 encoders + licenses
- https://docs.rs/openh264 — Cisco OpenH264 Rust wrapper, BSD-2-Clause

**Wayland / portal**
- https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html — ScreenCast portal API (session → `OpenPipeWireRemote()`, cursor modes, `restore_token`)
- https://pipewire.org — PipeWire

**Android**
- https://developer.android.com/reference/android/media/MediaCodec — `PARAMETER_KEY_LOW_LATENCY` (API 30): decoder holds I/O "no longer than required by the codec standards"
- https://developer.android.com/reference/android/media/MediaCodecInfo.CodecCapabilities — `FEATURE_LowLatency` (API 30, constant `"low-latency"`)
- https://developer.android.com/reference/android/media/MediaFormat — `KEY_LOW_LATENCY` (API 30, value 0/1)
- https://developer.android.com/media/media3/exoplayer — Media3/ExoPlayer

**References**
- https://github.com/LizardByte/Sunshine — capture method + encoding API compatibility matrices (X11, KMS/DRM, wlroots, XDG portal; VAAPI/NVENC/software encode)
- https://github.com/moonlight-stream/moonlight-qt (GPL-3.0)
- https://github.com/rustdesk/rustdesk (AGPL-3.0)
- https://github.com/pavlobu/deskreen (AGPL-3.0, WebRTC→browser)
- https://github.com/LibVNC/x11vnc (GPL-2.0), https://github.com/novnc/noVNC, https://github.com/LibVNC/libvncserver (GPL-2.0)
- https://github.com/debauchee/barrier — input-only KVM, no video path
- https://gitlab.freedesktop.org/mstoeckl/waypipe — Wayland client forwarding (not screen capture)

**Codec / licensing**
- https://www.videolan.org/developers/x264.html — x264 "released under the terms of the GNU GPL" (+ commercial license)
- https://github.com/cisco/openh264 (BSD-2-Clause)
- https://github.com/lz4/lz4 — `lib/` BSD 2-Clause, programs GPL-2.0-or-later
- https://trac.ffmpeg.org/wiki/HWAccelIntro — ffmpeg hardware acceleration overview (VAAPI etc.)
