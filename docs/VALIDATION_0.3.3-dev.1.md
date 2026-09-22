# TermTerm 0.3.3-dev.1 doğrulama raporu

Bu rapor paketlenmiş ilk geliştirme teslimatına aittir. Sonraki kaynak değişiklikleri ve kurucuların durumu için [palet düzeltmesi raporuna](VALIDATION_PALETTE_2026-09-22.md) ve [sağ tık menüsü raporuna](VALIDATION_CONTEXT_MENUS_2026-09-22.md) bakın.

Tarih: 22 Eylül 2026. Dal: `feature/terminal-usability`, develop tabanı `d254a4d`. Yerel geliştirme teslimatıdır; main/tag/GitHub Release/latest.json değiştirilmedi. Mevcut stable sürüm 0.3.2'dir.

## Değişiklikler

Yumuşak antrasit–mavi yüzeyler, yuvarlak butonlar, hafif kenarlıklar ve 160 ms geçişler. Azaltılmış hareket tercihi geçişleri kapatır. Her bağlantı üst sırada kendi sekmesindedir; ikinci sekme satırı ve tekrar eden “Focused view” başlığı kaldırıldı. Workspace/broadcast/split araçları alt çubukta, Stats sağ üsttedir. Sekmeler bağlantıyı yeniden oluşturmaz; son terminal kapatılınca Vault açılır.

Graphite ANSI paleti daha belirgin, varsayılan font 15 px, satır yüksekliği 1.25 ve minimum kontrast 4.5. Kayıtlı özel font boyutu korunur. Gerçek xterm tema önizlemesi vardır. Seçim bitince kopyalama, sağ tık/Shift+Insert yapıştırma, Ctrl+Insert, platform kısayolları ve remote mouse override ortak uygulama davranışına bağlandı. Panoya yalnızca metin okuma/yazma yetkisi verilir. Geç gelen yapıştırma kapalı oturuma gönderilmez. Tercihler mevcut şifreli settings kaydındadır; kasa biçimi değişmedi.

Sol altta sürümle birlikte Updates bağlantısı vardır. Geliştirme paketinde stable kurulum devre dışıdır; kullanıcının onayı olmadan yeni sürüm yayınlanmaz.

## Geçen kontroller

- 39 Rust testi geçti; 4 test gerektirdiği dış fixture/helper nedeniyle varsayılan koşuda atlandı. Şifreli kasa/backup, kesinti ve dolu disk kurtarma, import corpus/ilişkiler, gerçek RSA4096/Ed25519/ECDSA el sıkışmaları, Windows ölçümü, DPAPI ve transfer kontrolü dahil.
- 13 TypeScript birim testi: shell Control tuşlarının korunması, macOS Command tanımları, AltGr/composition, uygulama ve metin alanı kısayolları, girdi sırası ve klasör ilişkileri.
- 18 farklı Playwright senaryosu: mevcut 17 senaryo tam koşuda, ek tercih kaydı senaryosu hedefli koşuda geçti. Son alt araç çubuğu konumu da sınandı. Seçim/kopyalama, sağ tık, Shift+Insert, Unicode ve bracketed-paste, kapalı oturuma gecikmiş yapıştırma, remote mouse reporting + Shift override, Updates, sekme kapama/geçiş, 16 panel, 1366/1920 px ve %100/%150 CSS ölçek kontrolleri. 10.000 host araması yük altındaki son koşuda 137.8 ms.
- 6 Windows native senaryosu: gerçek Linux lab SSH, gerçek CPU/RAM/root ve mount değerleri, Windows sistem panosu ile ayrı TextBox copy/paste, Vim'e Unicode/çok satırlı yapıştırma ve kaydedilen dosya baytları, shell Ctrl+C/D/Z/L/K/N ve F1/F12/ok/UTF-8 baytları, Vault/SFTP/terminal geçişleri, Windows yerel terminal/gerçek kaynak göstergesi, 16 SSH panelinde ölçüm ve broadcast, kilitlemede ölçümlerin durması. Son tam native koşu 37.2 saniye; 30 dakika izleme yapılmadı.
- İlave Windows native platform smoke geçti: kasa oluşturma, yerel shell/Stats, klasör gezinme, tema kaydı, taşınabilir kopyayı parolayla yeniden açma. Eski native testlerin Ungrouped ve yerel terminal seçicileri yeni gezinmeyle uyumlandı; geniş workspace/metrics paketlerinin tamamı bu tur yeniden koşulmadı.
- Windows/Linux normal frontend üretim çıktıları derlendi. Masaüstünde kullanılmayan staticlib/cdylib çıktıları kaldırıldı; yürütülebilir rlib üzerinden bağlanır. Debug WebDriver yalnızca e2e özelliğindedir; normal ön yüz VITE_E2E olmadan üretilir.

## Otomasyonun sınırları

Native embedded WebDriver Insert eşlemesini ve contextmenu/pointer ayrıntılarını eksik üretir; native test bunları doğru DOM olaylarıyla tamamlar. Windows clipboard gerçekten işletim sistemindedir; SSH ve Vim sonucu gerçek sunucuda doğrulanır. Playwright ayrıca gerçek browser input akışını sınar. Bu, fiziksel Türkçe klavyede elle test veya Windows ekran ayarındaki gerçek DPI değişimi yerine geçmez; %150 kontrol CSS ölçeğidir.

macOS Intel/Apple Silicon derleme yapılandırması, Mosh bağımlılık hazırlığı, ad-hoc imza, .app/DMG/.app.zip ve CI komutları hazır; kullanıcı isteğiyle Mac derlemesi bekler. Mac üzerinde derlendi/çalıştı denmez. RPM'nin ayrı Fedora/openSUSE masaüstünde kurulumu ve Linux masaüstü Secret Service panoları bu Windows/WSL ortamında doğrulanmaz. x64 paketleri ARM veya Alpine/musl hedefi değildir.

Önceki raporlardaki eksikler devam eder: doğrudan FIDO2/Windows Hello kayıt adaptörleri, bazı üreticiye özel şifreli import biçimleri ve erişilemeyen donanım/bulut kontrolleri bu görsel sürümle tamamlanmış sayılmaz. SSH/chain/SFTP iş mantığı değiştirilmedi. [Önceki kapsam](STATUS.md).

## Paket doğrulaması

- Linux x64: Ubuntu 22.04 chroot tabanında DEB, RPM ve AppImage üretildi; normal dependency graph WebDriver içermiyor, SHA-256 listesi doğrulandı. En yüksek glibc sembol gereksinimi **GLIBC_2.34**.
- DEB test ortamına kuruldu, `apt-get check`, `dpkg --verify`, `ldd` ve paketlenmiş Mosh çalıştırması geçti; gerçek uygulama penceresi açıldı.
- AppImage FUSE olmadan `--appimage-extract-and-run` ile Xvfb altında açıldı. RPM digest, mimari, bağımlılık metadatası ve içerik kontrolü geçti; Fedora/openSUSE kurulumu yapılmadı.
- Windows x64 optimize üretim derlemesi ve NSIS oluştu. Gerçek üretim penceresi 1.038 saniyede açıldı, düzgün kapandı; 4445 test portu yoktu ve önceden açık kullanıcı uygulaması çalışmaya devam etti. Bu ölçüm pencerenin görünme süresidir; temiz cihaz cold-start benchmarkı değildir.
- Windows NSIS arşivi [resmî 7-Zip 26.03](https://www.7-zip.org/download.html) ile CRC kontrolünden geçti. Çıkarılan exe, Tauri bundle marker farkı dışında derlenen exe ile byte-byte aynı. Mosh, Cygwin DLL, lisanslar ve çevrimdışı WebView2 kurucusu mevcut. Ubuntu tabanının eski p7zip 16 aracı bu NSIS biçimini çözemedi; güncel resmi 7-Zip ile yeniden kontrol edildi.
- Açık kullanıcı TermTerm oturumu nedeniyle tam NSIS kurulum/kaldırma çalıştırılmadı; koruma kontrolü test betiğine de eklendi.
- Portable/source ZIP kontrolü geçti: SHA-256 manifesti, Windows x64 PE mimarisi, kaynak allowlist (kişisel kasa/kimlik bilgisi/derleme önbelleği yok), Mosh/Cygwin/WebView2 ve lisans/kılavuz dosyaları. Portable içindeki Mosh 1.4.0 çalıştırıldı. Son teslim klasöründe platform dosyaları ve hash listeleri birlikte bulunur.
