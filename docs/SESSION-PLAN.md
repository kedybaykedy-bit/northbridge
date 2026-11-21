# SESSION-PLAN (self-prompt / compact sonrası hafıza)

Bu dosya chat compact'inden SONRA okunacak birincil bağlamdır. Oku, sonra devam et.
Son güncelleme: oturum ortası — çalışan subagent'lar + commit geçmişi aşağıda.

## 1. Proje
northbridge — KDE ekosisteminde beslenen, GPL-3.0, Rust workspace + Android (Compose,
MM-01 monokrom manşet: /home/baykedy/temp/design/MONOKROM_MANSET.md zorunlu).
Repo: /home/baykedy/git/kde/northbridge → https://github.com/kedybaykedy-bit/northbridge (private, main).
KDE Connect baz değil; ../scrcpy salt referans (GPL-2, kod kopyası YOK). Kod yorumu YOK.

## 2. Altyapı (her zaman geçerli)
- Rust: `cargo test --workspace` →23 paket yeşil beklenir (.cargo/config.toml BINDGEN env).
- Release daemon: `cargo build --release -p nb-cli` → **çalışan: ./target/release/northbridge listen**
  (DEBUG fps'i öldürüyordu:5→release15fps). Portlar53317(http)53318(packet)53319(screen)
  53320(audio)53321(mirror), config ~/.config/northbridge (peers.json).
- Android: `cd android && JAVA_HOME=/home/baykedy/opt/jdk-21.0.13.1+2 ~/opt/gradle-8.10.2/bin/gradle assembleDebug`
  (locale "BUİLD SUCCESSFUL" yazar) → `adb install -r app/build/outputs/apk/debug/app-debug.apk`.
- **NDK KURULDU**: /home/baykedy/Android/Sdk/ndk/28.2.13676358 → symlink (eski ~/android-sdk/ndk),
  cmake3.22.1 de bağlı. `ndkVersion = "28.2.13676358"` yazınca externalNativeBuild çalışır.
- Cihaz: Redmi Note10 Pro (sweet), Android16/LineageOS, **root (KernelSU, adb shell zaten root)**,
  `/dev/uinput` = root:uhid660 (root açar; Shizuku TELEFONDA YÜKLÜ DEĞİL — kurulacak),
  adb b1a403c6, IP DHCP (son:192.168.1.16), PC192.168.1.10 (eski192.168.1.111 — DHCP değişti).
- `pkill -f "[n]orthbridge listen"` **HER ZAMAN ayrı çağrı** (aynı zincirde start varsa kabuğu öldürür).
- Kullanıcı Türkçe, jargon sevmez; telefon UI etkileşimleri KULLANICIYA ait (anlaşma;
  am start/install/salt-okunur logcat/screencap serbest), "devam" deyince kaldığın yerden sürdür.
- sudo: kedy. PC X11 :0 (x11vnc + picom çalışıyor).

## 3. DOĞRULANMIŞ DURUM (kullanıcı onaylı)
- Keşif ✓, eşleşme ✓, touchpad+Türkçe-Q ✓, KVM ✓, PC→telefon EKRAN ✓, AYNA15fps tıklama doğru ✓,
  tek tap ✓, hover güvenli ✓, **kendi kendini onaran adresler ✓ (kanıt: .99 bozuldu→11sn'de .10'a döndü)**,
  **PC→telefon ping/pano ✓** (`northbridge ping M2101K6G →127.8ms`, clipboard uygulandı),
  **ses ÇİFT YÖNLÜ ✓** + gecikme245→110ms, **klavye yeni tasarım ✓** (ekran görüntüleriyle doğrulandı).
- E2E: `fake_phone`10/10; decode_sim/wintest örnekleri repo'da.

## 4. COMMIT'LENENLER (bu oturum)
- `55cebdc` — B4 eşleşmiş cihazda Pair kalktı (ESLESTI), B3 ayna meta yarışı (tek combine yazıcı),
  B5 stage60→15sn.
- `6989b64` — **R-A** birleşik klavye/touchpad yüzeyi, **B1** ses jitter buffer (kök: her tick'te
  kuyruk boşaltılıyordu + prebuffer yoktu), **X11 ölümü** sınıflandırılıp pencere kapanıyor
  (22k satır spin gitti).
- `e9309bb` — **Adresler kendini onarıyor**: DiscoveryHub uygulama ömrü boyunca keşif (ekran
  kapatınca durmaz), probe parmak izi DeviceStore.observeAddress ile yazar, InputChannel her
  yeniden denemede taze adres okur, telefon `nb.addr` ile kendi adresini bildirir, PC mTLS
  parmak iziyle peers.json'a yazar (dial_order artık dolu).

## 5. ÇALIŞAN SUBAGENT'LAR (compact sonrası DURUMU KONTROL ET — hâlâ koşuyor mu?)
1. **Ayna** (ses_e017d5ce8ffeCJo8inAY5fMwmg): swipe'ın gitmemesi + oturum çökmesi
   (`frame eof`9 kez, `feed error Native:16`1 kez; ekran kaydı60sn restart yarışı araştırılıyor).
   Sahip: crates/nb-mirror/**, ui/mirror/**, input/MirrorInjector.kt, transport/MirrorClient.kt.
2. **PC GUI** (ses_e01748194ffeD93zJgqDqF0sN6): **egui** ile masaüstü arayüz (kullanıcı seçti: en hafif).
   Yeni crate `crates/nb-gui`. main.rs'e DOKUNMAMASI gerekiyor (ayna agent'inin alanı).
3. **Screen** (ses_e016fcd85ffeUHjdf1amiB6ntu): tam ekran + yatay görüntüleme + akış kararlılığı
   (adapt.rs adaptif kalite, DisplaySelector seam'i — ek monitör seçimi İSTEMEDİ, sadece not).
   Sahip: crates/nb-screen/**, ui/screen/**, transport/ScreenClient.kt.

## 6. SIRADAKİ İŞLER
- **R-C NATIVE FARE** (kullanıcı onayladı): PC'den telefonu kontrol ederken telefonda **gerçek
  fare imleci** görünsün (OTG takılıymış gibi, Barrier gibi). **Ana yol: root + /dev/uinput**
  (uinput cihazı oluştur → Android gerçek imleç çizer). **Alternatif: Shizuku** (rootsuz —
  Shizuku telefonda kurulu değil, kullanıcı kuracak; shell'in uhid grubuna bakılacak).
  NDK hazır → JNI/C yardımcısı mümkün. **Ayna agent'i MirrorInjector'u bırakınca SAL** (çakışır).
- **R-E çoklu cihaz**:2 PC ↔2 telefon (adres altyapısı artık hazır).
- **R-F fps**: SHM PutImage denemesi (15fps kullanıcının hoşuna gidiyor, zorunlu değil).
- **Not (kullanıcı)**: ek monitör seçimi — sonra.
- Her iş bitiminde: `cargo test --workspace` + assembleDebug + **toplu** commit + push
  (çalışan agent varken commit ATMA — yarım iş yakalar).

## 7. Kullanıcı test listesi (bekleyen)
1. Ayna: swipe/drag çalışıyor mu + çökme azaldı mı (log'da `frame eof` sayısı).
2. Ses: PC→telefon (SES KÖPRÜSÜ → BAŞLAT), `PC SESİ` kapat-aç, mikrofon gecikmesi ~140ms.
3. Klavye: dikey üst touchpad/alt klavye; yatay tam ekran + X/+-çip; CTL+C; döngüde durum korunuyor mu.
4. Arka plan: ayna açıkken Home→20sn→dön (FGS), ek bildirim "Gelen bağlantılar için arka planda açık".
5. PC'den: `northbridge ping M2101K6G` arka planda; PC'de kopyala → telefonda pano.
6. Screen: tam ekran + yatay.

## 8. Teknik hafıza (dersler — tekrar açılmasın)
- Frame kuyrukları FIFO + geniş; `take_newest` REFERENCE KIRARDI (Native:18).
- Gruplama: her yeni görüntüde flush; SPS/PPS yalnız stream başında.
- x11rb `create_gc(cid, drawable)` — İLK PARAM GC.
- screenrecord60sn restart = periyodik IDR = self-heal (ama eof yarışı olabilir — ayna agent'i bakıyor).
- Android arka plan socket öldürür → FGS (2 tane: MirrorKeepAliveService + PacketKeepAliveService).
- Injector gerçek ekran ölçüsü kullanmalı (video boyutu DEĞİL).
- Motion/Enter tap GÖNDERMEZ (ekran-kapatma çift dokunuğu riski) — hover'ı root'lu imleç çözecek.
- Ses: cpal buffer + jitter prebuffer toplamı gecikmedir; ama küçük = underrun. Ölçerek ayarla.
- PipeWire yakalama48kHz dışı cihazda frame atıyordu → resample+fold_to_stereo eklendi.
- `PacketHandler` elinde `peer_fp` + `remote` var → adres kaydı oradan (nb.addr).
- localsend discovery `POST /register` kullanır, `/info` çağıran kod yok (dead surface — bilgi).
