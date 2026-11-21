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
- **YENİ İSTEKLER (kullanıcı, not al — sonra ekle):**
  (a) **Batarya paylaşımı**: iki cihaz birbirinin batarya yüzdesini görsün (telefon→PC
      bildirim/satır + PC GUI'de gösterim; PC→telefon da mümkün — keşif duyurusuna veya
      hafif bir `nb.battery` paketine eklenebilir, periyodik ~60sn).
  (b) **Bildirim yansıtma**: telefonda gelen bildirimler PC'de görünsün (KDE Connect gibi)
      — Android NotificationListenerService (izin verilmesi gerekir) + `nb.notify` paketi +
      PC'de GUI/masaüstü gösterimi (kapat/sil eylemleri sonra).
- **KVM EKRANINDAKİ SANAL PENCERE/İMLEÇ HÂLÂ DURUYOR** (kullanıcı: "hala KVM penceresinde
  sanal pencere var") → o ekran R-C'den etkilenmedi; CursorRenderer kaldırılıp gerçek imleç
  KVM yoluna da taşınacak. Dosyalar: ui/KvmScreen.kt, ui/KvmViewModel.kt, input/CursorRenderer.kt.
- **R-C çıktı ama kullanıcı: "istediğim tam olarak böyle değildi"** (acelesi yok) → ne
  beklediğini NETLEŞTİR. Muhtemel fark: imlecin göründüğü yer/mod veya tıklama davranışı.
  Değişiklik ayna + KVM yolunu birlikte etkileyecek; R-C dosyaları: input/NativePointer.kt,
  input/PointerDaemon.kt, input/PointerUserService.kt, android/app/src/main/cpp/, MirrorInjector.
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

## 9. GÜNCEL DURUM (son güncelleme — bunu oku)
- Commit'ler: `55cebdc` (B3/B4/B5) → `6989b64` (klavye+ses+X11) → `e9309bb` (adres kendini onarımı)
  → `b73cf81` (batch: klavye/ses/ayna swipe/screen/GUI/PC→telefon/native imleç)
  → `61e22f8` (wake/open/apps/sleep/keepawake) → `5796821` (GUI uzaktan kontrol).
  Tree bu son commit'te temizdi.
- **Kullanıcı kararları:** KVM = **sadece giriş paylaşımı** (video yok, Barrier gibi),
  tetik/bırak **telefondaki buton**, PC imleci grab'de gizli. **Sanal ekran: tek tane** (çoklu
  = onayla), panel **hem kapalı hem açık** senaryosu istenecek. **Shizuku YAPILMAYAK** (not).
  Dosya paylaşımı = **basit gönder/al** (detay sonra). GUI = **hepsi** (görsel + özellik).
  Yatayda **sekmeler yana**, dikeyde altta. Testler **en sonda**. R-E (2PC↔2telefon) sıraya alındı.
- **İsimlendirme:** kullanıcıya R-A/R-B gibi etiket DEME — düz Türkçe isim kullan.
- **Kilit:** telefonda **desen kilidi var** → uzaktan desen girişi isteyecek (aynada kilit
  ekranı + MotionEvent ile yol enjeksiyonu; `input swipe` düz çizgi olduğu için yetmez →
  root helper gerekir, sanal ekran helper'ıyla aynı).
- **Açık şikayetler:** (1) GUI çirkinmiş (eğimli tile yok) → yenileniyor,
  (2) **ses döngüsü**: telefon mikrofonu bir süre sonra TELEFON hoparlöründen çıkıyor →
  muhtemel kök: PC'nin yakalama kaynağı kendi çıkışının monitor'ü (loopback) → düzeltme koşuyor,
  (3) yatayda sekmeler yana.
- **ÇALIŞAN5 AGENT (dosyaları ayrık):** KVM giriş paylaşımı (nb-mirror + KvmScreen +
  PacketServer) · dosya gönder/al (nb-transport + nb-cli/main.rs + yeni ui/files — **nav
  bağlantısını LEAD ekleyecek**) · GUI yenileme (nb-gui tek sahip) · yatay yan sekmeler
  (NorthbridgeRoot + theme + CommonUi) · ses döngüsü (nb-audio + ui/audio).
  **Kural: aynı dosyayı iki agent paylaşmaz; nb-mirror'a A1 bitmeden dokunma.**
- **Sırada:** sanal ekran (faz0 keşif: `ServiceManager.getDisplayManager().createVirtualDisplay`
  root `app_process` ile + `screenrecord --display-id` sanal ekranı yakalar mı + `input -d <id>`)
  → kilit deseni (aynı helper) → fps (SHM) → R-E → **en sonda test turu (11 madde)**.
- Telefon bazen sökülüyor (kullanıcı takıp söküyor) — adb b1a403c6.

## 10. OTURUN SONU DURUM (22.11 gece — bilgisayar kapatılıyor)
- **Yedek (tmp silinir!)**: `/home/baykedy/temp/nb-backup-20261122/` (11MB) →
  `nb-dev.log` (PC daemon), `kvm.log`+`nb-full.log` (agent teşhisleri), `nb-tel*.log`,
  `nb-ev2.log` (getevent6.6MB), tüm ekran görüntüleri (UI doğrulama),
  `legacy_keysyms.txt`+`legacy_table.rs` (Türkçe keysym tablosu kaynağı).
- **Ağaç**: commit `a34a749` + **COMMIT EDİLMEMİŞ** işler: Türkçe keysym tablosu
  (`crates/nb-mirror/src/events.rs`, testli40+2 yeşil), IME servisi (`input/NbIme.kt` yeni,
  manifest+`res/xml/nb_method.xml` lead'te), duvar-fix + hız/sıkıştırma (`NativePointer.kt`),
  IME aktive/paket (`MirrorInjector.kt`, `PointerDaemon.kt`, `KvmViewModel.kt`, `KvmScreen.kt`).
  **Yarın önce `cargo test --workspace` + `assembleDebug` → commit.**
- **KVM imleç: ÇÖZÜLDÜ ve kullanıcıca doğrulandı** (`GÖRÜNÜYOR OLDU`). Kök nedenler sırayla:
  (1) SELinux `sock_file` etiketi → soket `filesDir`'e taşındı, (2) yön ölçeklemesi (stored/live
  eşleşiyor), (3) savurma duvarı → tek monoton sıkıştırıcı `64+(in-64)*0.3` (VEL_* silindi),
  (4) Türkçe `ğ İ ı Ş` → PC tarafı keysym0x100-0x2FF aralığı bilinmiyordu →79 satırlık tablo.
- **Bekleyen**: APK kurulumu `waiting for device` (telefon takılınca tamamlanır) →
  **TEK KAPSAMLI TEST**: dikey BAĞLA → normal gezdir → yavaş üst-aşağı (duvar?) → hızlı savur
  (uzaklaşmalı/fırlamamalı) → PC'den `öçğüşıİ ÖÇĞÜŞ` (görünüyor mu? **toast?**) → yatay → tekrarla
  → KOPAR → "bitti".
- **Subagent yaşam döngüsü**: her rapor sonu idle olur → **lead aynı sessionID'ye mesaj atınca
  devam eder** (`ses_e004d0fb0ffeH3YCOn5OU6bzdL`). Bilgisayar kapanınca süreç ölür → **yarın
  YENİ subagent açılacak** (aynı görev tanımı + bu dosyadaki kanıtlarla).
- **Kurallar hatırlatma**: `pkill` deseni komutun kendisini öldürür (`pkill -f "[a]db logcat"` /
  `[n]orthbridge listen` — HER ZAMAN ayrı çağrı); daemon'ı kullanıcı testi sırasında YENİDEN
  BAŞLATMA; telefonda UI'a dokunma; kod yorumu YOK; aynı dosyayı iki agent paylaşmaz.
