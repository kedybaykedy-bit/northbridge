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
├── crates/nb-transport/   # LocalSend HTTPS sarmalayıcı + packet kanalı (mTLS + JSON satır)
├── crates/nb-discovery/   # LocalSend discovery + tailscale probe + olay işleme
├── crates/nb-cli/         # `northbridge` binary
└── plugins/nb-ping/       # ilk plugin (capability matching kanıtı)
```

- **Portlar:** HTTP = `identity.port` (varsayılan 53317, LocalSend uyumlu),
  packet = `port + 1` (53318). NetworkPacket kanalı: mTLS, satır sonu JSON.
- **Eşleşme:** `nb.pair` istek/yanıt (senkron, tek bağlantı). Varsayılan
  auto-accept; `--no-auto-pair` ile reddet. KC timestamp + capability alanları.
- **Plugin:** `Plugin` trait (`incoming`/`outgoing` capability + `on_packet`
  yanıt üretir). KC `pluginsForCapabilities` eşleşmesi.
- **Durum:** `~/.config/northbridge/{identity.json,peers.json}`

## Milestone'lar

- **M0 (bu sürüm):** workspace + kimlik + discovery (LAN/Tailscale/USB sınıflandırma)
  + trust store + eşleşme + `nb.ping` + CLI (`listen/discover/devices/pair/ping/status`)
- **M1:** dosya paylaşımı (LocalSend transfer akışı) + pano
- **M2:** KVM (Barrier protokolü: 4-byte prefix, CINN/COUT, 1px jump zone) + Android
- **M3:** ekran (`rustvncserver` + DeskVNC `vnc-core`, Android MediaProjection)
- **M4:** ses köprüsü (cpal + Opus/UDP)
- **M5:** Bluetooth RFCOMM transport

## Çıkış kriteri (M0)

İki makinede `northbridge listen` → birbirini LAN **ve** Tailscale üzerinden bulur,
`pair` ile eşleşir (fingerprint trust store'a yazılır, kanal etiketli),
`ping` çalışır, USB tethering takılınca aynı cihaz "tanıdık" kanal etiketiyle görünür.
