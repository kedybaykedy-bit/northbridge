# M4 research — audio bridge, desktop ↔ phone (both directions)

Milestone M4 adds a **duplex audio bridge**: desktop system sound → phone
speaker, and phone mic (or playback) → PC speakers. Transport and identity
follow the conventions already established in [PROTOCOL.md](PROTOCOL.md) and
[M3-RESEARCH.md](M3-RESEARCH.md): fingerprint identity, mTLS everywhere,
control on the packet channel, binary frames on a dedicated data port.

**Assumed state:** Linux desktop, confirmed **X11 session, Fedora 43,
PipeWire-based distro**; phone is **rooted (Magisk, root shell available)**;
ports in use: LocalSend HTTPS `53317`, packet channel `53318`
(`http + 1`), screen-frame `53319` (`packet_port + 1`, M3).
Workspace licensed **GPL-3.0-or-later**; Android companion app exists (M2).

---

## 1. RECOMMENDATION (one stack — build this)

| Layer | Choice | Why |
|---|---|---|
| Desktop capture (system sound) | **`pipewire` crate (`pipewire-rs`, MIT)**, native Stream API, `STREAM_CAPTURE_SINK=true` on the default sink | Only path that reliably captures the **full PipeWire mix** (app sounds). `cpal` ALSA-only misses PipeWire apps; `cpal`'s own docs warn the ALSA `default` is held by PipeWire/Pulse and must go through bridge devices (`pipewire`/`pulse`) instead. Native sink-capture is what `pw-record -P '{"stream.capture.sink": true}'` does |
| Desktop capture (mic) | `cpal` input stream (any backend) | Mic is a plain source; no monitor semantics needed |
| Desktop playback (phone → PC speakers) | **`cpal` output stream** (ALSA host → PipeWire bridge, or `pipewire`/`pulseaudio` cpal feature when it ships) | Simplest correct: one callback filling PCM. No graph knowledge needed |
| Android capture (system sound, rooted) | **`REMOTE_SUBMIX` `AudioRecord` from a privileged context** — scrcpy model: `app_process` server or systemized app via Magisk module granting `CAPTURE_AUDIO_OUTPUT`, **no MediaProjection dialog** | Only rooted path that captures the real output mix including opt-out apps. `AudioPlaybackCapture` always needs the user consent dialog and misses `ALLOW_CAPTURE_BY_*`-opted-out apps (Spotify/DRM) |
| Android capture (mic) | Plain `AudioRecord` (`MIC` / `VOICE_COMMUNICATION`) | No privilege needed |
| Android playback | **`AudioTrack` PCM** fed by **`MediaCodec` `audio/opus` decoder** (system decoder = system libopus, zero new deps) | Lowest-effort low-latency path. Oboe/AAudio only as M4-b tuning |
| Codec | **Opus, 48 kHz, stereo, 20 ms frames** — `opus` crate 0.4.x on desktop (libopus bindings), MediaCodec on Android | 26.5 ms default algorithmic delay, per-packet mode/bitrate switching, in-band FEC + PLC + DTX (RFC 6716). Music sweet spots 64–128 kb/s stereo; voice 16–40 kb/s |
| Transport M4-a | **Dedicated TCP mTLS port `53320` (= http + 3 = `packet_port + 2`)**, reusing `tls::server_config` / fingerprint `trusted` gate | Same auth model as M3's frame port. No new crypto, no firewall surprise, trivially correct behind one listener |
| Transport M4-b | **Same port, UDP framing option** (seq + jitter buffer + Opus PLC/FEC) negotiated in `nb.audio.open` | TCP head-of-line blocking is the main WiFi jitter source (same lesson as M3 §9). UDP only pays off once the TCP baseline is measured |
| Control | `nb.audio` capability + `nb.audio.open` / `close` / `config` on the packet channel (classic or stream) | Mirrors `nb.screen.open` exactly |
| Frame header | **`NBA1`, 24 bytes LE** (see §4) | Same shape discipline as M3's `NBS1` |
| Fallbacks | PCM16 fallback codec id; `AudioPlaybackCapture`+MediaProjection fallback on non-root phones; `parec`/`pw-record` subprocess only as a debug tool, never shipped | Keeps every stage debuggable without new deps |

**EnCodec verdict (explicitly rejected for transport):** Meta's EnCodec
(1.5–24 kbps, 48 kHz stereo, higher MUSHRA than Opus at equal bitrate) is a
research/music-generation codec: PyTorch model, GPU-oriented, no stable
Rust crate, no Android system decoder, no packet-loss tooling. It wins on
bitrate, loses on everything M4 needs (deterministic CPU encode, PLC/FEC,
20 ms framing, zero-dependency Android decode). Opus wins. Revisit only if
M4 ever needs sub-10 kbps music trickle — it won't on LAN.

### Port scheme (follows M3 §5 exactly)

| Port | Value | Use |
|---|---|---|
| http | `identity.port` (53317) | LocalSend HTTPS, discovery |
| packet | `port + 1` (53318) | mTLS JSON control |
| screen-frame | `packet_port + 1` (53319) | M3 binary frames |
| **audio** | **`packet_port + 2` (53320)** | **M4 binary audio frames (TCP M4-a, UDP M4-b)** |

Control plane (on the packet channel):

```json
→ {"id":1795142410000,"type":"nb.audio.open","body":{"dir":"both","codec":"opus","sample_rate":48000,"channels":2,"frame_ms":20,"bitrate":64000,"transport":"tcp"}}
← {"id":1795142410012,"type":"nb.audio.open","body":{"ok":true,"audio_port":53320,"codec":"opus","frame_ms":20}}
```

plus `nb.audio.close` / `nb.audio.config` (bitrate, dir mute), capability
`nb.audio` exchanged at pairing (absent ⇒ hide the feature on both sides).

---

## 2. What was researched and decided

### 2.1 Desktop capture (Fedora 43, PipeWire)

| Approach | Verdict |
|---|---|
| **`pipewire-rs` Stream API, `STREAM_CAPTURE_SINK=true`** (or connecting to the default sink's monitor ports) | **Ship this.** It is the documented native mechanism: the `audio-capture` example builds a `Capture`/`Music`-role stream; setting the capture-sink key captures sink output instead of a source. `pw-record -P '{"stream.capture.sink": true}'` is the same call from the shell. Finds the default sink via `pw-dump` / `pactl get-default-sink` |
| `pactl`/`parec`/`pw-record` subprocess (`@DEFAULT_MONITOR@`) | Works everywhere today (`parecord -d @DEFAULT_MONITOR@`, `pw-record --target`), and is the right **debug/fallback** path, but a subprocess per direction is crash/ownership debt — do not ship as primary |
| `cpal` with default ALSA host | **Reject for system capture.** cpal's own docs: when PipeWire/Pulse holds ALSA `default`, a second ALSA open fails `DeviceBusy`; apps must use bridge devices or a native backend. cpal historically had no PipeWire backend (issue #554, open since 2021); the 2026-native backend treats `Audio/Sink` nodes as duplex inputs with `STREAM_CAPTURE_SINK` set transparently — fine when it reaches crates.io, but `pipewire-rs` direct is available now and exposes the exact monitor semantic |
| ffmpeg `-f pulse` / `-f pipewire` subprocess | Viable M3-style out-of-process fallback for encode+capture in one (`-fragment_size` tunes latency down to near-instant), but adds a runtime binary dep; keep as an undocumented escape hatch only |
| PulseAudio `module-rtp` / module-native-protocol | Era-appropriate reference (multicast PCM), not a capture API for us |

Decision: new `AudioSource` trait on desktop with **`PipeWireSource`
(first impl)** + `CpalMicSource`; `SubprocessMonitorSource`
(`pw-record --raw`) kept behind a cfg/debug flag for bring-up.

### 2.2 Desktop playback

`cpal` output stream, 48 kHz stereo, small fixed buffer. That is the whole
M4-a playback story. No graph API, no latency knob beyond buffer size.
Tuning (exclusive/low-latency paths) belongs to M4-b and is measured, not
guessed.

### 2.3 Android capture — what actually works rooted (14/15)

| Approach | Verdict |
|---|---|
| **`REMOTE_SUBMIX` `AudioRecord` with `CAPTURE_AUDIO_OUTPUT`, from shell/system context (scrcpy model)** | **Ship this.** scrcpy 2.x captures device output exactly this way: `AudioRecord` source `REMOTE_SUBMIX`, encoded to Opus/AAC/RAW via `MediaCodec`, muxed with a 12-byte frame header (config/keyframe flag, PTS, size) on its own socket. It runs as an `app_process` server, so no app sandbox, no consent dialog. The rooted equivalent for northbridge: companion Magisk module that systemizes the audio helper + grants `CAPTURE_AUDIO_OUTPUT` (precedent: RecordYou Magisk module does exactly systemize + grant for `REMOTE_SUBMIX`) |
| `AudioPlaybackCapture` + `MediaProjection` consent dialog | **Fallback for non-root / debugging only.** Needs `RECORD_AUDIO` + one-time user approval per session, capturing and playing apps must share a user profile, and players can opt out (`allowAudioPlaybackCapture=false`, `ALLOW_CAPTURE_BY_SYSTEM/NONE`, targetSdk ≤ 28 defaults). DRM apps (Spotify/Netflix) stay silent. Also conflicts with M3 screen capture over the single active `MediaProjection` token |
| `tinycap`/tinyalsa, `/dev/snd` direct | Reject: SELinux + HAL routing make raw ALSA unreliable on modern phones; fights the audio server instead of using it |
| `AudioFlinger` / policy hooks, `NEW_INTERFACE_REMOTE_SUBMIX` hacks | Reject: version-fragile internals (sndcpy threads show the breakage); root should buy privilege, not patches |
| `VOICE_UPLINK/DOWNLINK` for call audio | Out of scope v1 (needs privileged telephony path); note as M4-c |

Rooted path of least resistance: reuse the **scrcpy server pattern**
verbatim — tiny `app_process` (or root-started foreground service) binary
that opens `REMOTE_SUBMIX`, encodes with `MediaCodec audio/opus`, and ships
frames to the desktop over the M4 data port. The existing Kotlin app only
needs mic-capture + playback + control UI; the privileged tap lives outside
the APK sandbox, exactly where root already is.

### 2.4 Android playback

| Approach | Verdict |
|---|---|
| **`AudioTrack` PCM + `MediaCodec` Opus decode** | **Ship this.** Opus encoder on Android also emits 960-sample (20 ms) blocks, matching our frame size with no re-chunking. `MediaCodec` async API, feed `NBA1` payloads, write PCM to `AudioTrack` |
| Oboe / AAudio `LowLatency` + `Exclusive` + callback + `framesPerBurst×2` buffer | M4-b tuning. Oboe is a routing/config helper, not lower latency by itself; adopt only if M4-a measurements show the Java path missing the budget. The Rust `oboe` crate (0.6) additionally drags a C++ toolchain into the build |
| ExoPlayer / Media3 | Reject: player buffering model, wrong tool (same call as M3 §4) |

Manifest additions: `RECORD_AUDIO`, `FOREGROUND_SERVICE[_MICROPHONE|MEDIA_PROJECTION]`
(only the ones used), foreground-service type per direction. Root path needs
no new Play-Store-visible permission beyond that — the `CAPTURE_AUDIO_OUTPUT`
grant lives in the Magisk module, not the APK.

### 2.5 Codec decision: Opus (and why not the others)

| Codec | Latency / bitrate | Fit |
|---|---|---|
| **Opus (libopus)** | 20 ms frame + ~6.5 ms lookahead = **26.5 ms** default; down to 5 ms CELT-only; 6–510 kb/s; per-packet mode/frame switching; in-band FEC (LBRR), PLC, DTX | **Ship.** RFC 6716, BSD-licensed, `opus` crate 0.4 on crates.io, system decoder on every Android via MediaCodec, scrcpy/Moonlight/rustdesk already prove it on this exact link |
| PCM16 raw | Zero codec delay, ~1.5 Mbit/s stereo 48 kHz | **Fallback codec id only** (LAN can carry it; useful for A/B and for debugging the transport without an encoder) |
| AAC | ~21 ms frame, worse low-bitrate quality than Opus, no per-packet switching | Only if a device lacked Opus — none do |
| EnCodec / AudioDec / SoundStream / Lyra / Mimi | 1.5–24 kbps hi-fi claims, but GPU/PyTorch, no RFC, no PLC/FEC framing, no Android system support | **Reject** (see §1). Neural codecs are a bitrate story; M4 on LAN has a latency story |
| Speex / G.722 legacy | Obsolete vs Opus at every bitrate | Reject |

Operating point: **48 kHz, stereo, 20 ms, CBR 64 kb/s music / 32 kb/s
voice, VBR allowed M4-b**. 20 ms is the congestion sweet spot: 50 pps keeps
header tax (~20 kb/s over IPv4/UDP/RTP-ish framing) sane; 10 ms doubles
packet rate for 10 ms less delay — measure before touching it.

### 2.6 Transport decision: TCP mTLS now, UDP when measured

| Option | Verdict |
|---|---|
| **A. Dedicated TCP mTLS port 53320** reusing `tls::server_config`, fingerprint `trusted` gate, `NBA1` frames | **M4-a.** Binary frames, no base64 tax (M3 §5 measured +33 % for JSON), no per-frame parse, one listener, stolen-port scanners still face mandatory client certs. WiFi HoL blocking is accepted and bounded (drop-oldest, small `SO_SNDBUF`) |
| B. UDP raw + seq + jitter buffer + Opus PLC/FEC | **M4-b upgrade**, negotiated per-direction in `nb.audio.open` (`transport: tcp\|udp`). Moonlight's shape is the template: Opus over RTP with periodic FEC parity. DTLS optional — on a paired LAN, fingerprint-gated UDP + tested sequence integrity is proportionate; do not invent a new handshake |
| C. Frames over the JSON packet channel | Reject: 1 MiB line cap, base64 bloat — control plane only (M3 §5 precedent) |
| D. MJPEG-style reuse of the LocalSend HTTPS port | Reject: request/response semantics on a stream (M3 §5 precedent) |

### 2.7 Echo / loop safety (no AEC v1)

Both sides can play + capture simultaneously on one machine (desktop
speakers → desktop mic-monitor; phone speaker → phone mic). Same class of
bug as the clipboard echo M1 already solved with state comparison:

1. **Direction bit** in every `NBA1` header (`0` = desktop→phone, `1` =
   phone→desktop) — a receiver never routes its own direction back.
2. **Stream epoch + `pts_ms`**: each `nb.audio.open` mints an epoch;
   late/duplicate frames (epoch mismatch, `seq` regression) are dropped, so
   a reconnect never replays stale audio into the new session.
3. **Half-duplex default, duplex opt-in**: M4-a ships one direction at a
   time being *audible* (both can be open; UI mutes the return path by
   default, exactly like "mute while screen sharing"). Full-duplex echo
   cancellation is explicitly **not v1** — Opus DTX + push-to-talk/mute
   covers LAN use; revisit only with measured complaints.
4. Clock discipline follows snapcast's proven shape: sender tags
   `pts_ms` (monotonic), receiver plays at `pts + bufferMs` and trims/expands
   by resampling — never by unbounded queue growth.

### 2.8 Licensing

| Dependency | License | GPL-3.0 workspace risk |
|---|---|---|
| libopus (via `opus` crate 0.4 / system MediaCodec) | BSD-3-Clause | **None** — permissive, links freely into GPL-3.0 |
| `pipewire-rs` / libpipewire / libspa | MIT | **None** |
| `cpal` | MIT / Apache-2.0 | **None** |
| Oboe (if ever) | Apache-2.0 | **None** |
| scrcpy/sndcpy structural ideas | Apache-2.0 | Ideas only, no code copied — **none** |
| EnCodec weights/code (not used) | Mixed research/demo licensing | Avoided by rejection |
| x264-style GPL-2 codecs (not used) | GPL-2.0 | Same rule as M3 §2: out-of-process only, never linked |

Project is GPL-3.0-or-later anyway; every shipped dep is permissive, so
there is no contamination vector. The only copyleft-adjacent references
(rustdesk AGPL, snapcast GPL) are **structural inspiration** (frame
headers, chunk timestamps), never linked code.

---

## 3. Frame format `NBA1` (24 bytes, little-endian)

Same discipline as M3's `NBS1` (§5): fixed header, one Opus packet per
frame M4-a, raw PCM fallback.

| Field | Size | Meaning |
|---|---|---|
| `magic` | 4 (`NBA1`) | Framing resync |
| `ver` | u8 (1) | Header version |
| `codec` | u8 (0 = Opus, 1 = PCM16LE interleaved) | Decoder select |
| `flags` | u8 (bit0 = config/keyframe, bit1 = direction 0 = desk→phone / 1 = phone→desk, bit2 = DTX/silence) | Routing + decode |
| `channels` | u8 (1/2) | Channel count |
| `seq` | u32 | Per-direction sequence; gaps ⇒ PLC; regression ⇒ drop |
| `pts_ms` | u64 | Sender monotonic clock; playout = `pts + bufferMs` |
| `len` | u32 | Payload bytes that follow |
| *payload* | `len` | One 20 ms Opus packet (or 20 ms PCM16 frames) |

`sample_rate` and `frame_ms` are session properties from `nb.audio.open`,
not per-frame fields. Config frames (`flags & 1`, Opus TOC / decoder reset)
are sent on connect and on `nb.audio.config`.

---

## 4. Staged plan

### M4-a — one direction, TCP, Opus-or-PCM ("hear my desktop on the phone")

| # | Deliverable | Detail |
|---|---|---|
| 1 | `AudioSource` trait + `PipeWireSource` | `pipewire-rs` capture stream, `STREAM_CAPTURE_SINK=true`, default sink, 48 kHz stereo F32 → S16 |
| 2 | `opus` crate encode | 20 ms frames, CBR 64 kb/s, one encoder per direction; PCM16 codec id alongside for transport debug |
| 3 | Data port `53320` | mTLS accept via `tls::server_config`, `trusted` fingerprint gate, `NBA1` write + flush per frame, drop-oldest single-frame queue |
| 4 | Control | `nb.audio` capability at pairing; `nb.audio.open`/`close` over packet channel; desktop→phone dir only |
| 5 | Android | `MediaCodec audio/opus` → `AudioTrack`; Start/Stop UI; works with M3 stream + input concurrently |
| 6 | Rooted capture helper | `app_process`/`su -c` prototype opening `REMOTE_SUBMIX`, shipping to desktop (phone→desktop dir, mic first if submix needs the Magisk module) |

**Exit criteria (measurable):**
end-to-end ≤ **250 ms** on gigabit LAN at 48 kHz/stereo/20 ms (sender
capture → audible); stable ≥ **30 min** (no fd/memory growth, no seq
regression); glitch rate < **1 audible dropout / min** on idle WiFi;
stream refused for unpaired fingerprints and dies with the connection;
no regression in touchpad/KVM/screen paths.

### M4-b — duplex, jitter buffer, tuned ("call-quality bridge")

| # | Deliverable | Detail |
|---|---|---|
| 1 | Duplex sessions | Both directions live; per-direction seq/epoch/pts; UI mute per direction (echo rule §2.7) |
| 2 | UDP option | Same `53320`, `transport: udp` negotiation; adaptive jitter buffer 20–120 ms; Opus in-band FEC + PLC on gaps; TCP stays as fallback |
| 3 | Tuning | Bitrate ladder (32 ↔ 64 ↔ 96 kb/s), DTX on silence, bufferMs auto-trim; optional Oboe/AAudio output on Android if M4-a misses budget |
| 4 | Robustness | Sink/source hot-plug (Bluetooth/USB headset), sample-rate change, reconnect with epoch bump + decoder reset |
| 5 | Observability | `nb.audio.stats` (capture/encode/net/decode ms, jitter, loss, queue depth) + `northbridge audio phone` CLI debug |
| 6 | Privilege productization | Magisk module shipping `CAPTURE_AUDIO_OUTPUT` grant + helper install; MediaProjection fallback documented for non-root |

**Exit criteria (measurable):**
duplex ≤ **150 ms** one-way on gigabit, ≤ **250 ms** on 5 GHz WiFi;
glitch rate < **1 / 5 min** under light WiFi contention with bounded queue
(latency must not grow — shed/FEC instead); clock drift < **2 ms/min**
without resync clicks; backgrounded phone holds audio ≥ **10 min**
(foreground service, root path unaffected by Doze).

*(Possible M4-c: call-audio uplink/downlink, Bluetooth SCO bridge, WebRTC
interop — none committed.)*

---

## 5. Open questions needing real hardware

1. **Default-sink monitor vs per-app tap on this exact Fedora 43 box** —
   does `STREAM_CAPTURE_SINK` on the default sink deliver the full mix at
   48 kHz without resampling clicks, or do we need an explicit monitor-port
   connect? (`pw-dump` + 5-minute capture decides.)
2. **cpal output backend on this box** — ALSA-bridge latency vs native
   PipeWire backend: is `cpal` playback inside the 250 ms M4-a budget, or
   does playback also need `pipewire-rs`? (Loopback timing test.)
3. **`REMOTE_SUBMIX` behavior on this phone's Android 14/15 build** — does
   the rooted context capture media + game + notification mix, and are
   DRM apps silent (expected) or fatal (must handle)? Needs the Magisk
   module + `dumpsys media.audio_flinger` session check.
4. **Opus encoder block alignment Android-side** — scrcpy notes ~20 ms
   production granularity from `AudioRecord`; confirm our 20 ms framing
   needs no re-chunking on the actual device.
5. **WiFi jitter profile of the real LAN** — TCP-vs-UDP crossover point:
   at what loss % does M4-a TCP audibly beat/block vs early UDP prototype?
   (Drive with `tc qdisc netem`, then decide M4-b scope.)
6. **Battery/thermal cost** of continuous Opus + `AudioTrack` on the phone
   (30 min drain test) and whether Doze/background limits touch the
   root-path helper.

---

## 6. References worth borrowing structurally (ideas only, no code)

| Project | License | What to copy |
|---|---|---|
| **scrcpy 2.x audio** (incl. rom1v's audio blog + `AudioCapture`/`AudioSource` design) | Apache-2.0 | The whole rooted-capture shape: `REMOTE_SUBMIX` `AudioRecord` → `MediaCodec` Opus → 12-byte frame header (config/keyframe, PTS, size) → own socket; 20 ms block granularity finding |
| **sndcpy** (rom1v) | Apache-2.0 | The *limitation* catalog: MediaProjection-era constraints, `CAPTURE_AUDIO_OUTPUT`/system-app analysis, `NEW_INTERFACE_REMOTE_SUBMIX` fragility — i.e. why root buys privilege, not hacks |
| **Moonlight / Sunshine (Wolf) audio** | GPL-3.0 | Opus-over-RTP + AES + periodic FEC parity packets; RTSP port-exchange then fixed audio port — closest transport cousin to `53320` |
| **rustdesk audio** | AGPL-3.0 | Opus-by-default at low bitrate, per-session audio permission gating (`enable-audio`), audio-track-into-recorder discipline |
| **snapcast** | GPL-3.0 | Timestamped PCM/Opus chunks + `bufferMs` playout (`audible at t_s + bufferMs`), time-sync + resample-to-clock correction — the clock model for §2.7 |
| **PulseAudio `module-rtp` / `module-null-sink`** | LGPL-2.1 | Historical proof that monitor-tap + RTP multicast works; `pactl load-module module-null-sink` as a test fixture for routing without hardware |
| **SoundWire (Georg Seifert) app-pair model** | Proprietary | Product shape only: ALSA-hook server + phone-as-speaker UX, UDP + Opus + client-side buffer; confirms the latency knob is buffer size, not codec |
| **EnCodec / AudioDec papers + demos** | Research/demo terms | Negative reference: what neural codecs buy (bitrate) vs cost (no streaming/PLC story) — documents the rejection |

---

## 7. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| PipeWire graph changes under us (sink rename, Bluetooth switch, rate hop) | Capture taps the wrong node / clicks | Resolve default sink at `open` + watch for change → epoch bump + reconnect; list sinks in `nb.audio.stats` |
| `cpal` backend roulette on Linux (ALSA vs PipeWire feature timing) | Build works, latency doesn't | Pin measured config in M4-a; `pipewire-rs` playback backend as M4-b option |
| TCP HoL blocking on WiFi | Latency spikes, frozen audio | Single-frame queue, drop-oldest, small `SO_SNDBUF`, fps→bitrate shed; UDP in M4-b |
| Clock drift between devices | Growing delay or periodic glitch | Sender `pts_ms` + `bufferMs` playout + resample trim (snapcast model); drift logged, never queued |
| WiFi contention + screen stream + input concurrently | All three degrade together | Audio has priority: smallest frames, own port, independent queue; measure trio test in M4-b exit |
| Android background/Doze kills capture | Silence after screen-off | Foreground service (correct type); root helper outside the app lifecycle; 10-min background exit test |
| Battery/thermal on continuous duplex | Hot phone, throttled CPU | DTX, 32 kb/s voice mode default for mic-return; drain test gates M4-b |
| Echo howl in full duplex (speaker→mic loop) | Unusable duplex | Half-duplex-default UX (§2.7); no AEC promised v1; revisit only on measured complaints |
| DRM/opt-out apps silent on phone capture | "No sound in app X" reports | Document expected behavior (same as scrcpy); MediaProjection fallback has identical limits — no code can fix policy |
| Extra port `53320` blocked / double-NAT weirdness | No audio despite pairing | `nb.audio.open` returns the port; fall back to TCP `packet_port` multiplex only as last resort (documented, not built v1) |
| `opus` crate native build (cmake/libopus) friction | CI/build breakage | Prefer system libopus via pkg-config where present; vendor only if CI proves need |

---

## 8. Sources

Links checked 2026-11-21.

**Desktop / PipeWire / cpal**
- https://github.com/RustAudio/cpal — ALSA-default-vs-PipeWire warning, `pipewire`/`pulse` bridge-device guidance
- https://github.com/RustAudio/cpal/issues/554 — PipeWire backend history
- https://github.com/RustAudio/cpal/issues/1145 — PipeWire backend: sinks-as-duplex + `STREAM_CAPTURE_SINK` transparency
- https://docs.rs/crate/pipewire/latest/source/examples/audio-capture.rs — canonical Stream-API capture example
- https://pipewire.pages.freedesktop.org/pipewire-rs/pipewire/keys/static.STREAM_CAPTURE_SINK.html — `STREAM_CAPTURE_SINK` key
- https://www.reline.so/blog/how-to-record-system-audio-on-linux — `pw-record -P '{"stream.capture.sink": true}'` vs `parecord -d @DEFAULT_MONITOR@`
- https://discussion.fedoraproject.org/t/what-is-the-equivalent-of-pulseaudios-monitor-sources-in-pipewire/68939 — monitor-source equivalence thread
- https://github.com/silverstein/minutes/issues/69 — why PortAudio/cpal enumeration can't see sink monitors (asks PipeWire directly instead)
- https://forum.endeavouros.com/t/audio-streaming-with-minimal-latency-for-desktop-sharing/27758 — `ffmpeg -f pulse -fragment_size 16` latency finding

**Android capture / playback**
- https://developer.android.com/media/platform/av-capture — `AudioPlaybackCapture` rules: consent dialog, usage/policy/profile limits
- https://developer.android.com/reference/android/media/AudioPlaybackCaptureConfiguration — capturability preconditions
- https://developer.android.com/media/platform/sharing-audio-input — concurrent-capture, accessibility/privileged exceptions
- https://blog.rom1v.com/2023/03/scrcpy-2-0-with-audio — scrcpy audio design: `REMOTE_SUBMIX` record, 20 ms block granularity, Opus default 128 kb/s, latency compensation
- https://scrcpyapp.org/en/guides/develop — server frame header (codec id, config/keyframe, PTS, size), async `MediaCodec` path
- https://deepwiki.com/Genymobile/scrcpy/3.3-audio-capture-and-encoding — `AudioCapture` interface, `OUTPUT→REMOTE_SUBMIX` / `PLAYBACK→AudioPlaybackCapture` mapping
- https://github.com/Genymobile/scrcpy/issues/4094 — root audio-routing request; protected-app capture discussion
- https://github.com/rom1v/sndcpy/issues/113 — `CAPTURE_AUDIO_OUTPUT` / `NEW_INTERFACE_REMOTE_SUBMIX` privilege analysis
- https://github.com/you-apps/RecordYou-Magisk-Module — Magisk systemize + grant precedent for `REMOTE_SUBMIX`
- https://xdaforums.com/t/request-instant-screen-recorder-internal-audio-no-dialog-root-shell-triggerable-android-16-pixel.4781377 — no-dialog rooted recipe: SurfaceFlinger + `REMOTE_SUBMIX` + Magisk module
- https://developer.android.com/games/sdk/oboe/low-latency-audio — AAudio/Oboe tuning: bursts, exclusive, callback, 20 ms floor table
- https://www.davthecoder.com/blog/low-latency-audio-dsp-rust-android-aaudio-oboe — Rust `oboe` 0.6 / cpal-backend notes

**Codec / network / protocol models**
- https://opus-codec.org — bitrates, frame sizes, PLC/FEC/DTX feature set
- https://datatracker.ietf.org/doc/html/rfc6716 — Opus spec (framing, LBRR/FEC, DTX)
- https://blog.blazingcdn.com/en-us/what-is-the-opus-codec-low-latency-audio-explained — 26.5 ms budget, header-tax-vs-frame-size table
- https://crates.io/crates/opus — `opus` crate (libopus bindings)
- https://github.com/lakelezz/audiopus — `audiopus` high-level alternative (cmake build note)
- https://games-on-whales.github.io/wolf/stable/protocols/rtp-opus.html — Moonlight audio: Opus + AES-CBC + RTP + 4+2 FEC grouping
- https://github.com/badaix/snapcast (+ snapserver `bufferMs`/`chunk_ms` docs, issue #782) — timestamped chunks, play-at-`t+bufferMs`, resample correction
- https://www.linux.com/topic/desktop/how-stream-audio-your-linux-pc-android — SoundWire product shape (UDP + buffer = latency knob)
- https://hexdocs.pm/snapcast/Snapcast.html — Elixir snapcast server: chunk/buffer model reference

**Neural codecs (rejected)**
- https://audiocraft.metademolab.com/encodec.html — EnCodec: 48 kHz stereo, 1.5–24 kbps, PyTorch/demo orientation
- https://arxiv.org/abs/2210.13438 (Défossez et al.) — EnCodec paper via forasoft summary: MUSHRA > SoundStream at matched bitrate, transformer recompression
- https://www.forasoft.com/learn/audio-for-video/articles-audio/neural-audio-codecs-lyra-encodec-soundstream — 2026 survey: EnCodec vs Lyra/SoundStream/Mimi latency/bitrate table
- https://bigpon.github.io/AudioDec_demo — AudioDec: 12 kbps/48 kHz at <6 ms GPU / 10 ms CPU — closest neural candidate, still no streaming/PLC/Android story
