# TermTerm 0.2 doğrulama raporu

18 Eylül 2026. Bu rapor yerelde gerçekten çalıştırılan kontrolleri, yalnızca yapılandırılan hedeflerden ayırır. 0.1 `.ttvault` / `.ttbackup` sürümü değiştirilmedi.

## Tamamlanan işlevler

- Terminal başına Stats kutusu; CPU/RAM 2 saniye, disk 10 saniye; son 60 saniye için küçük grafik, kök/sistem diski ve mount ayrıntıları. Sanal dosya sistemleri ayrıca açılır. Bilinmeyen değerler sıfır değildir. Gizli panel ölçümü durur; workspace tercihi saklanır.
- Ayrı SSH exec kanalları; shell geçmişine veya interaktif loga ölçüm komutu yazılmaz. Yerel sistem, WSL ve Mosh SSH kontrol bağlantısı kaynakları; desteklenmeyen oturumlar için açıklama.
- Graphite/Midnight/Forest/Ember/Paper renkleri, anında uygulama ve şifreli kasa ayarı. Hostlar yalnızca açık klasörde görünür; klasör yolu ve alt gruplar ile gezinilir.
- Platform shell/editör/home/agent/parola hatırlama servisleri; gerçek seri port listesi. Windows/Linux/macOS paket yapılandırmaları ve dört mimari CI hedefi.
- OpenSSH Include/öncelik/tünel, PuTTY proxy/tünel, Ansible vars/children ve import ilişki eşlemesi iyileştirmeleri. Eksik/şifreli vendor alanlarında açık rapor; [kapsam tablosu](IMPORT_SUPPORT.md).
- Tek kısayol kaynağı, terminalde Ctrl+K/N korunması ve sıralı IPC girdi kuyruğu. Kuyruk, aynı terminale hızlı gelen tuşların Rust'a ters sırada ulaşmasını önler; 1 MiB sınırı ve Unicode parça bütünlüğü vardır.

## Yerel testler

| Kontrol | Sonuç ve kapsam |
|---|---|
| Rust / Windows | 21 test geçti, 2 ignored; kriptografi/dayanıklılık, import semantiği, ilişkiler, parser ve gerçek CIM komutu. Son tam koşu 23,19 saniye, `--test-threads=1` |
| Rust / Linux | 20 test geçti, 2 ignored; gerçek Linux proc ölçüm komutu dahil |
| TypeScript birim | 6 test geçti; kısayollar, AltGr/composition ayrımı, sıra/Unicode/iptal |
| Playwright | 4 test geçti; klasör içinde hostlar, tema seçimi, 1366/1440 düzenleri, 10.000 kayıt araması 46,4 ms |
| Windows native regresyon | 11 test geçti, 48,3 saniye; gerçek SSH/chain/proxy/tünel, SFTP rastgele dosya bütünlüğü, ConPTY/Mosh, import, backup, API Bridge, PostgreSQL ekip/paylaşım ve stop/start sonrası eşitleme |
| Windows kaynak/klavye | 4 test geçti, 63,1 saniye; gerçek Linux SSH ölçümü, gizli/kapalı panel duraklaması, F1–F12 ve gezinme/Ctrl+K/N baytları, yerel Windows ölçümü, kilitte temizlik, 16 panel kısa kontrolü |
| Linux native WebKitGTK | Encrypted vault, gerçek yerel PTY/shell, CPU/RAM/kök disk ve portable copy açma smoke testi geçti |
| Windows klasör/tema native | 1 test geçti, 26,5 saniye; root/outer/inner host görünürlüğü, breadcrumb, Forest tema ayarının kasaya kaydı ve gerçek xterm arka planı, portable copy açma |
| Windows → Linux → Windows kasa | Her yönde native test geçti (Linux 2,7 saniye; Windows 1,3 saniye); yalnızca kasa parolasıyla açma, tema/grup/host ilişki eşitliği, Unicode düzenleme, backup restore ve tekrar portable copy |

16 panel kontrolünde bekleme **30 saniye** idi. Kullanıcı isteğiyle 30 dakikalık izleme yapılmadı; uzun süreli bellek sızıntısı testi geçti iddiası yoktur. Testlerde sentetik klavye olayları gerçek SSH hedefinde yakalanan baytlarla karşılaştırıldı; fiziksel Türkçe klavye/AltGr testi yapılmadı.

Linux ortamı WSL2 Ubuntu 24.04 x64'tür. Üretim hedefi Ubuntu 22.04 CI tabanıdır; 24.04 paketinin 22.04 uyumluluğu yerel testle kanıtlanmadı. Root headless testinde WebKit test sürecine özel sandbox/compositing değişkenleri kullanıldı; uygulama bunları ayarlamaz.

## Doğrulanmayan alanlar

macOS Intel/Apple Silicon runner erişimi bulunmadığından bu hedeflerin derlemesi, DMG/.app çıktısı ve gerçek macOS ölçümleri doğrulanmadı. CI dosyası eklendi ancak bu klasörün remote'u olmadığından uzakta çalıştırılmadı. Windows SSH hedefi, fiziksel seri/FIDO2/Hello donanımı, masaüstü Linux Secret Service ve erişilemeyen ağ mount'ları için gerçek cihaz testi yapılmadı.

MobaXterm 26.5/SecureCRT 9.7.3 gerçek şifreli export örnekleri yoktur; bu biçimlerin şifre çözme uyumluluğu tamamlanmış sayılmaz. Kaynak örnekleri sentetik düz metindir. İlk bağlantıda iki farklı kasayı otomatik görsel birleştirme sihirbazı, native donanım anahtarı adaptörleri ve diğer sınırlar [STATUS](STATUS.md) belgesindedir.

## Tekrar çalıştırma ve kanıtlar

- Windows native: `tests/desktop/workspace.mjs`, `metrics.mjs`, `platform-smoke.mjs`; platform komutları [BUILD_PLATFORMS](BUILD_PLATFORMS.md).
- Rust: `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`; ignored girişler WSL laboratuvarı gerektiren DB testi ile yalnızca dayanıklılık testinin alt süreçte çağırdığı crash yardımcısıdır. Gerçek DB regresyonu native suite'te ayrıca çalıştırıldı. Paralel ağır derleme/kriptografi koşusunda CIM testinin 5 saniyelik zaman aşımı görüldü; tek iş parçacıklı tam koşu geçti. Uygulama böyle bir durumda ölçüm hatasını gösterir.
- Birim/arayüz: `pnpm test:unit`, `pnpm test:ui`.
- Loglar `.tools/v02-*.log`, ekran görüntüleri `artifacts/` altında yereldir. `.lab` kimlik bilgileri, kasalar ve loglar kaynak ZIP'ine alınmaz.
- E2E derlemesi ayrı uygulama kimliği kullanır. WebDriver Rust eklentisi yalnızca `e2e` + debug, JavaScript test eklentisi yalnızca E2E build komutuyla eklenir. Üretim paketinin normal dependency graph'ında test sürücüsü yoktur.

Yeni çıktılar `artifacts/release-0.2.0` altında, eski 0.1 teslimatı `artifacts/release` altında korunur. Kaynak ZIP'i, migrations, WSL betikleri, import şablonları, platform CI/komutları ve kılavuzları içerir. Windows paketleri imzasızdır.

Windows NSIS kurucusu ve taşınabilir ZIP üretildi; x64 PE başlığı, SHA-256 özetleri, Mosh çalışma dosyaları, kaynak ZIP izinli dosya listesi ve normal Cargo dependency graph'ında test sürücüsü bulunmaması doğrulandı.

Linux x64 release derlemesi Ubuntu 24.04 üzerinde tamamlandı. AppImage ve `.deb` üretildi; ELF x86-64 başlığı, `.deb` sürüm/mimari metaverisi, normal Cargo dependency graph'ı, SHA-256 özetleri, AppImage runtime komutu ve paketlenecek Mosh helper'ın çalışması doğrulandı. Linux için yeni bir makinede kurulum/güncelleme testi yapılmadı.

| Çıktı | Dosya |
|---|---|
| Windows kurucu | `TermTerm_0.2.0_x64-setup.exe` |
| Windows taşınabilir | `TermTerm-0.2.0-windows-x64-portable.zip` |
| Linux AppImage | `TermTerm_0.2.0_amd64.AppImage` |
| Linux Debian paketi | `TermTerm_0.2.0_amd64.deb` |
| Kaynak ve kılavuzlar | `TermTerm-0.2.0-source.zip` |
| Bütünlük | `SHA256SUMS.txt`, platform bazlı SHA-256 listeleri |
| macOS Intel / Apple Silicon | Yapılandırma ve CI hazır; paket üretilmedi, doğrulanmadı |
