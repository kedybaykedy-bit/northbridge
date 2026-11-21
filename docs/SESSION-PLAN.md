# SESSION-PLAN (self-prompt / compact sonrası hafıza)

Bu dosya chat compact'inden SONRA okunacak birincil bağlamdır. Oku, sonra devam et.

## 1. Proje
northbridge — KDE ekosisteminde beslenen, GPL-3.0, Rust workspace + Android (Compose,
MM-01 monokrom manşet tasarımı: /home/baykedy/temp/design/MONOKROM_MANSET.md zorunlu).
Repo: /home/baykedy/git/kde/northbridge → https://github.com/kedybaykedy-bit/northbridge (private, main).
KDE Connect baz değil; ../scrcpy salt referans (GPL-2, kod kopyası YOK). Kod yorumu YOK.

## 2. Altyapı (her zaman geçerli)
- Rust test: `cargo test --workspace` →23 paket yeşil beklenir (.cargo/config.toml BINDGEN env var).
- Release daemon: `cargo build --release -p nb-cli` → **çalışan: ./target/release/northbridge listen**
  (DEBUG binary fps'i öldürüyordu:5fps → release15fps). Portlar53317(http)53318(packet)
  53319(screen)53320(audio)53321(mirror), config ~/.config/northbridge.
- Android: `JAVA_HOME=/home/baykedy/opt/jdk-21.0.13.1+2 ~/opt/gradle-8.10.2/bin/gradle -C android assembleDebug`
  (ya da android/ içinden). APK: android/app/build/outputs/apk/debug/app-debug.apk → `adb install -r`.
- Cihaz: Redmi Note 10 Pro (sweet), **Android16/LineageOS**, root VAR (su kabul),
  adb b1a403c6, IP DHCP (son:192.168.1.106), PC192.168.1.111.
- pkill zinciri TEHLİKELİ: `pkill -f "[n]orthbridge listen"` AYRI çağrılacak (aynı komutta
  start varsa kendi kabuğunu öldürür —3 kez oldu).
- adb UI: tap koordinatları için `uiautomator dump` (bounds [x1,y1][x2,y2] → ortala);
  ekran görüntüsü `adb exec-out screencap -p > f.png` (read ile bakılır).
- Kullanıcı Türkçe, jargon sevmez, telefon UI etkileşimleri KENDİNE ait (anlaşma var;
  am start/install/logcat okuma serbest), "devam" deyince kaldığın yerden sürdür.
- sudo: kedy. PC X11 :0, x11vnc + picom çalışıyor (kompositör!).

## 3. Doğrulanmış DURUM (kullanıcı onaylı)
- Keşif (multicast224.0.0.167) ✓, eşleşme (PIN) ✓, touchpad+Türkçe-Q klavye ✓,
  KVM capture/release ✓, PC→telefon EKRAN akışı ✓ (takıla takıla, optimize edilecek),
  telefon→PC SES (mikrofon, PCM) ✓ ama **kopuklu (bug-B)**, AYNA video15fps kullanılabilir ✓,
  tıklama doğru mapleniyor + tek tap ✓, hover güvenli ✓, başlıkta fps/lat/queue sayacı ✓.
- E2E: `fake_phone`10/10 (nb-cli/examples/fake_phone.rs), decode_sim + wintest örnekleri repo'da.
- Son commit: b602703 (mirror usability batch), tree temiz.

## 4. BİLİNEN BUG'LAR (kanıtla)
- **B1 (kopuk ses)**: telefon→PC ses hızlı-az kesikli (a_A_A_A). Büyük ihtimal:20ms NBA1
  frame'lerde paket/çerçeve düşüşü veya cpal push underrun. Bak: nb-audio playback.rs
  (cpal buffer boyutu/latency), Android AudioTrack buffer, Android64 slotlu mirror-vari kuyruklar.
- **B2 (SCREEN akışı takılma)**: M3-a JPEG akışı — adaptif kalite/FPS, SHM capture, latency.
- **B3 (AYNA meta yarışı)**: UI "MASAUSTU YANITI BEKLENIYOR" kalabiliyor (Opening her
  reopen'da set ediliyor + statsJob yarışı) — MirrorViewModel state/metin eşleşmesi.
- **B4**: Discover'da eşleşmiş cihazda hâlâ `Pair` butonu + tekrar pair'lenebiliyor.
  KOD NOKTALARI: DiscoveryScreen.kt:131 DeviceCard → TextButton "Pair" (satır~184);
  DiscoveryViewModel.requestPair (satır115) → pairDialog PIN açıyor. Düzeltme: VM'e
  deviceStore.devices'ten `paired: StateFlow<Set<String>>` ekle; requestPair'de fp varsa
  mesaj + return; kartta fp paired ise "ESLESTI" yazısı (buton yok).
- **B5**: Keşif döngüsü listen'de60sn (nb-cli STAGE_INTERVAL) →15sn'ye düşür.
- **B6 (klavye)**: Türkçe-Q klavyede Ctrl/Backspace eksik — kullanıcı yeni tasarım istedi (A).
- **B7**: ekran kayıtta telefon timeout sorunu → FLAG_KEEP_SCREEN_ON eklendi (app ön planda);
  arka plan = FGS KURULDU (b602703) ama KULLANICICA TEST EDİLMEDİ — ilk iş test.

## 5. Kullanıcının YENİ İSTEKLERİ (öncelik sırasıyla konuşacağız)
- **R-A KLAVYE/TOUCHPAD YENİDEN TASARIM** (acele yok, fikir):
  - Klavye HORIZONTAL (yatay), tam standart bilgisayar klavyesi (Ctrl/Backspace/etc. eksik).
  - Ayrı küçük touchpad olmayacak; TEK büyük alan: **basılı tut = mouse tuşu**,
    **kaydırılırsa klavye yokmuş gibi tüm alan touchpad** (hold=button, drag=touchpad).
    Anladığım bu — kullanıcıya TEYİT ettir (tek satır özetle).
- **R-B ÇİFT YÖNLÜ SES**: (1) bilgisayar ses output'u TELEFONDAN çalınsın (PC→phone
  yönü yok — NBA1 zaten çift yönlü, Android AudioTrack playback var; eklenecek akış yönü +
  UI seçici). (2) mevcut telefon→PC mikrofon **kopukluğu (B1)** düzeltilecek.
- **R-C KVM native fare**: sanal capture/warp yerine Android'in NATIVE mouse imleci
  ( gerçek pointer enjeksiyonu — scrcpy yaklaşımı: sendevent/uinput benzeri veya
  `input` yerine gerçek imleç). Araştırma + redesign.
- **R-D DOSYA PAYLAŞIMI**: dosya transferi + **iki cihazın dosya sistemlerinin birbirinde
  görünmesi** (telefon deposu PC'de, PC klasörleri telefonda — browse UI).
  localsend path-dep zaten var (packages/core) — HTTP API mevcut, UI eksik.
- **R-E ÇOKLU CİHAZ**: 2 PC ↔ 2 telefon (peer listesi,2+ eşleşme aynı anda).
- **R-F FPS**: mümkünse biraz daha yükselt (MIT-SHM PutImage adayı; kompositör/picom
  maliyeti var;15fps kullanıcıca "kullanılabilir" — iyi olur ama zorunlu değil).
- **R-G arka plan** (FGS) → test + gerekirse iyileştirme.

## 6. UYGULAMA PLANI (sıra)
1. compact sonrası: bu dosyayı oku → kullanıcıdan gelen YENİ istekleri bekle
   (dedi "birkaç isteğim var" — onları dinle, listeye ekle, önceliği BİRLİKTE belirle).
2. FGS testi (arka planda ayna kalıyor mu — kullanıcı: çık-araya gir, ben logcat
   "packet connected"/servis notification kontrol ederim).
3. B4 + B5 (küçük, hızlı) → commit.
4. R-B önce B1 (kopuk ses) → nb-audio ölçümleri; sonra PC→phone yönü.
5. B3, B6/R-A, B7 testi, R-C, R-D, R-E, B2, R-F sırasıyla (kullanıcı önceliği varsa o).
6. Her iş bitiminde: `cargo test --workspace` + assembleDebug + commit + push (main).

## 7. Teknik hafıza (mirror/ölüm hikayesi dersleri — tekrar açılmasın)
- Frame kuyrukları FIFO + geniş (PC512, telefon64); `take_newest` REFERENCE KIRARDI
  (Native:18 = dsNoParamSets|dsRefLost;1 kare düşüş →27 kare hata).
- Gruplama: her yeni görüntüde flush (first_mb=0 heuristic); SPS/PPS yalnız stream başında.
- x11rb `create_gc(cid, drawable)` — İLK PARAM GC.
- screenrecord60sn restart = periyodik IDR = self-heal.
- Android arka plan socket öldürür (Software caused connection abort) → FGS.
- Injector gerçek ekran ölçüsü kullanmalı (video boyutu DEĞİL).
- Motion/Enter tap GÖNDERMEZ (ekran-kapatma çift dokunuğu riski); tap sadece butonda.
