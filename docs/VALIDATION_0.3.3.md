# TermTerm 0.3.3 doğrulama raporu

Tarih: 23 Eylül 2026. Bu sürümün yayımlanması sahibi tarafından açıkça istendi. Sonraki kararlı sürümler ayrıca onay gerektirir. macOS derlemesi daha sonra açıkça durduruldu; o kapsam yayıma alınmadı.

## Paketler ve güncellemeler

- Windows x64 NSIS kurucusu, portable ZIP; Linux x64 DEB, RPM ve AppImage. Kaynak ZIP'i bağımlılık önbelleği, kullanıcı kasası, SSH dışa aktarımı veya özel anahtar içermez. Yalnızca belgelenmiş, herkese açık test anahtarı örnekleri `tests/ssh-keys` altında bulunur.
- `TERMTERM_RELEASE_CHANNEL=stable` ile üretim derlemeleri; `e2e` özelliği ve `VITE_E2E` kapalı. Önceki 0.3.2 public updater anahtarı değiştirilmedi.
- Windows ve Linux AppImage için imzalı `latest.json` hedefleri. DEB/RPM yeni paketin indirilip paket yöneticisiyle kurulmasını gerektirir. Mac derlenmediği için feed'de Mac URL'si yoktur.
- Kasa/yedek biçimi ve veri yolları değişmez. Güncelleme kullanıcı eylemiyle başlar; otomatik zorunlu kurulum yoktur.

## Tamamlanan kontroller

| Kontrol | Sonuç |
|---|---|
| TypeScript ve üretim frontend derlemesi | Geçti |
| Rust Windows testleri | 39 geçti, 0 hata; 4 özel/yardımcı test varsayılan koşuda atlandı. Gerçek SSH üzerinden RSA4096, Ed25519/ECDSA ve bağımsız anahtar biçimleri; değiştirilmiş sunucu anahtarının reddi; import/yeniden açma; kasa kesinti/disk dolması; Windows kaynak ölçümü dahil |
| TypeScript birim testleri | 19/19 geçti; klavye eşlemeleri, kayıt ilişkileri, giriş kuyruğu ve platform yayın listesi |
| Windows Chromium arayüz testleri | 27/27 geçti; sağ tık menüleri, gerçek kaydetme sonuçları, klasör işlemleri, pano/tuş olayları, Updates hata/kurulum akışı, 16 terminal ve farklı ekran boyutları |
| Windows NSIS kurulumu | Sessiz kurulum, kurulu EXE'nin bundle işareti dışında derleme ile hash eşitliği, Mosh çalıştırma, gerçek pencere, düzgün kapanış, test portunun yokluğu ve kaldırma geçti |
| Linux DEB | İzole Ubuntu 22.04 ortamında kuruldu; APT bağımlılık kontrolü, paket doğrulaması ve gerçek grafik pencere açılışı geçti |
| Linux AppImage | Aynı ortamda extract-and-run ile gerçek grafik pencere açılışı geçti |
| Linux RPM | Mimari/sürüm, dosya listesi ve paket digest kontrolü geçti; RPM dağıtımına kurulum yapılmadı |
| Linux bağımlılıkları | Eksik dinamik kütüphane yok; en yüksek GLIBC gereksinimi 2.34; paket içindeki Mosh çalıştı |
| Üretim paket grafiği | Linux normal Cargo bağımlılık grafiğinde WebDriver yok; Windows kurulu uygulamada test dinleme portu yok |
| Updater imzaları | Gerçek Windows kurucusu ve Linux AppImage, mevcut public key ile doğrulandı. Her dosyanın bir baytı değiştirilince doğrulamanın reddedildiği iki ayrı Rust testinde geçti |

Son sağ tık değişikliklerinde ayrıca dört gerçek Windows native senaryosu geçti: şifreli kasa içinde kopyalama/taşıma/silme ve yeniden açma; kullanılan kimliğin silinmesini engelleme; seçili kaydın gerçek Rust yedek/restore dönüşü; yerel SFTP dosyalarında Unicode oluşturma, kopyalama, yeniden adlandırma ve silme. Bu çalışma geliştirme paketinde yapıldı; ayrıntılar [sağ tık raporunda](VALIDATION_CONTEXT_MENUS_2026-09-22.md). İşletim sisteminin Save As penceresi otomatik sürülmedi; arayüz kapsamı ve gerçek Rust yedek işlemi ayrı doğrulandı.

SSH/chain/proxy, gerçek Windows panosu, kaynak göstergeleri ve önceki regresyonların sonuçları [0.3.3 geliştirme raporunda](VALIDATION_0.3.3-dev.1.md), [palet raporunda](VALIDATION_PALETTE_2026-09-22.md) ve [0.3.2 raporunda](VALIDATION_0.3.2.md) korunur. Bu yayın çalışmasında tüm geçmiş bağlantı/donanım senaryoları yeniden çalıştırıldığı iddia edilmez.

## Sınırlar ve bekleyen ortamlar

- macOS Intel/Apple Silicon CI denemelerinde Mosh'un `@rpath` bağımlılığının eksik paketlendiği görüldü. Özgün Mach-O yollarını kullanarak özyinelemeli toplama düzeltmesi yapıldı; bir sonraki Apple Silicon denemesi bu aşamayı geçip Rust test derlemesine ulaştı. Sahibi kredi kullanımını durdurmayı istediği için çalışan iki CI koşusu iptal edildi. Son kaynak için tam Mac derlemesi, native test, DMG veya updater arşivi yoktur; başarılı kabul edilmez. Yeni istek olmadan Mac CI başlatılmaz.
- Mac yapılandırması güncel Homebrew bağımlılıkları için macOS 15+ ve ad-hoc imzayı hedefler; notarization yoktur. Bu yalnızca gelecekteki derleme hazırlığıdır.
- Ayrı Debian/Fedora/openSUSE masaüstü, Linux ARM/Alpine/musl, fiziksel FIDO2/Windows Hello/seri cihazları, her üretici şifreli export sürümü ve üretim sunucuları bu koşuda doğrulanmadı.
- 30 dakikalık izleme yapılmadı. Windows Authenticode imzası ve otomatik APT/YUM deposu yoktur. Güncelleme testleriyle gerçek kullanıcının kurulu uygulaması veya açık kasası değiştirilmedi.

Paket SHA-256 değerleri sürümün `SHA256SUMS.txt` dosyasındadır. Yayın sırasında GitHub'ın kaydettiği digest değerleri yerel dosyalarla karşılaştırılır; yayın sonrası canlı feed ayrıca kontrol edilir.
