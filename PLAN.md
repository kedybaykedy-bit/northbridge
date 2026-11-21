# northbridge — Birleşik Yerel Ağ Hub'ı

Barrier (KVM) + LocalSend (keşif/HTTPS/dosya) + KDE Connect (protokol/plugin/eşleşme)
proje parçalarını tek programda birleştirir. GPL-3.0-or-later.

## Kanallar (otomatik tanıma)

| Kanal | Mekanizma | Faz |
|---|---|---|
| LAN | LocalSend multicast `224.0.0.167:53317`, tüm interface'ler | M0 ✓ |
| Tailscale | `tailscale status --json` → peer 100.x/ fd7a:… adreslerine HTTP probe | M0 ✓ |
| USB | USB tethering interface'i (enx*/rndis*/usb*) subnet'i → aynı multicast | M0 ✓ (sınıflandırma) |
| Bluetooth | RFCOMM (KDE Connect multiplex'inden ilham) | M5 |

**Kimlik kanaldan bağımsız:** her cihazın kimliği sertifika fingerprint'i (LocalSend
modeli). Trust store'da fingerprint varsa hangi kanaldan gelirse gelsin "tanıdık".

## Mimari (workspace)

```
northbridge/
├── crates/nb-core/        # kimlik, NetworkPacket, trust/peers, Plugin trait, Channel
├── crates/nb-transport/   # LocalSend HTTPS sarmalayıcı + packet kanalı (mTLS + JSON satır, stream modu)
├── crates/nb-discovery/   # LocalSend discovery + tailscale probe + olay işleme
├── crates/nb-input/       # KVM/touchpad InputMsg enum'u + çerçeveleme + jump zone
├── crates/nb-cli/         # `northbridge` binary (input.rs touchpad/KVM, x11.rs XTEST enjeksiyonu)
├── plugins/nb-ping/       # ilk plugin (capability matching kanıtı)
├── plugins/nb-clipboard/  # pano paylaşımı (M1)
├── android/               # Android companion uygulaması (M2)
└── docs/                  # PROTOCOL.md, M3-RESEARCH.md
```

- **Portlar:** HTTP = `identity.port` (varsayılan 53317, LocalSend uyumlu),
  packet = `port + 1` (53318). NetworkPacket kanalı: mTLS, satır sonu JSON.
- **Eşleşme:** `nb.pair` istek/yanıt (senkron, tek bağlantı). Varsayılan
  auto-accept; `--no-auto-pair` ile reddet. KC timestamp + capability alanları.
- **Plugin:** `Plugin` trait (`incoming`/`outgoing` capability + `on_packet`
  yanıt üretir). KC `pluginsForCapabilities` eşleşmesi.
- **Durum:** `~/.config/northbridge/{identity.json,peers.json}`

## Milestone'lar

- **M0 (tamam):** workspace + kimlik + discovery (LAN/Tailscale/USB sınıflandırma)
  + trust store + eşleşme + `nb.ping` + CLI (`listen/discover/devices/pair/ping/status`)
- **M1 (tamam):** dosya paylaşımı (`send` + LocalSend v2 API, sadece paired
  cihazlardan kabul, `~/Downloads/northbridge` → fallback `<config>/downloads`)
  + pano paylaşımı (`nb.clipboard` plugin, 1 sn poll, echo-prevention)
- **M2 (masaüstü tamam ✓, Android yapım aşamasında):** touchpad + KVM —
  packet kanalına stream modu (`nb.touchpad.open` → çift yönlü satır akışı),
  `nb.touchpad`/`nb.kvm` capability'leri, X11/XTEST enjeksiyonu (relatif
  hareket, tekerlek, evdev+8 tuş, geçici keymap ile her unicode karakter),
  capture akışı (imleç sabitle + XFixes ile gizle, ~80 Hz telefona aktar,
  anchor'a geri warp). E2E: `examples/fake_phone.rs` (pair → stream → ping/pong
  → capture/enter/move/leave → rel_move enjeksiyonu birebir) ✓
  Protokol: `docs/PROTOCOL.md`
- **M3-a (tamam ✓):** PC→telefon ekran akışı — `nb-screen` crate: X11 `get_image`
  (SHM stratejisi hazır) → ≤1280 downscale → JPEG q70, drop-oldest kuyruk,
  ayrı mTLS frame portu (`http+2`=53319), `NBS1` 28-byte başlık, `nb.screen`
  kontrol mesajları (stream'de `open`/`release`). E2E: `fake_phone` gerçek
  X11'den 1280x720 kareleri alıp ayrıştırdı ✓. Android tarafı: `ui/screen/`
  (BAŞLAT/DURDUR, monokrom çerçeve)
- **M3-b (planlı):** telefon→PC penceresi (iPhone-Mirroring hissi) — scrcpy
  yaklaşımı: MediaCodec H.264 encode + Shizuku(root) shell izinleri + geri
  dokunma; referans kaynak: `../scrcpy` (GPL-2 → kod kopyası yok, yaklaşım var)
- **M3-b (kod tamam ✓):** telefon→PC penceresi — Android: root `screenrecord`
  stdout → Annex-B H.264 → `NBM1` (28B) → PC; geri gelen tık/tuş/scroll `su`
  ile enjekte (evdev→Android tablosu, kalıcı su süreci). Masaüstü: `nb-mirror`
  crate — openh264 decode → X11 penceresi → pencere olayları `nb.mirror`
  olarak geri. Port 53321, scrcpy yaklaşımı (kod kopyası yok, `../scrcpy`
  referans). E2E handshake+kare ✓; gerçek cihaz testi bekliyor.
- **M4 (kod tamam ✓):** `docs/M4-RESEARCH.md` uygulandı — `nb-audio` crate:
  NBA1 (24B LE) çift yönlü, port 53320, PCM garantili yol; pipewire/opus/cpal
  yolları default açık (paketler kuruldu); canlı sistem-sesi doğrulandı —
  probe 6 sn'de 299 kare, ton tepe değeri 11999/12000.
  Android: `AudioClient` (mikrofon + root sistem-sesi, AudioTrack, MediaCodec
  opus best-effort), MM-01 SES ekranı. E2E handshake+kare ✓
- **M4:** ses köprüsü (cpal + Opus/UDP)
- **M5:** Bluetooth RFCOMM transport

## Çıkış kriteri (M0)

İki makinede `northbridge listen` → birbirini LAN **ve** Tailscale üzerinden bulur,
`pair` ile eşleşir (fingerprint trust store'a yazılır, kanal etiketli),
`ping` çalışır, USB tethering takılınca aynı cihaz "tanıdık" kanal etiketiyle görünür.
