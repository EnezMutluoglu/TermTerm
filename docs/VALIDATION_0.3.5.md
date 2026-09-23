# TermTerm 0.3.5 doğrulama raporu

23 Eylül 2026. Kullanıcı bu tıklama düzeltmesinin Windows/Linux yeni sürümü olarak yayımlanmasını onayladı; uzun testlerin tekrarlanmamasını istedi. macOS beklemede, GitHub CI derlemesi kullanılmadı.

## Kapsam

Klasörler tek tıkla açılır. Klasör bilgileri sağ tık menüsündedir. Hostta ilk fare tıklaması bilgileri 500 ms erteler; native çift tıklama gelirse bekleme iptal edilip mevcut bağlantı servisi çağrılır. İkinci fare basışı da beklemeyi iptal eder; basılı tutulan ikinci tık sırasında kart kaymaz. Normal klavye aktivasyonu beklemeden çalışır. Ctrl/Command/Shift seçimleri gezinme veya bağlantı başlatmaz.

Bekleme sağ tık, Escape, klasör/ekran/kasa değişimi, düzenleyici açılışı ve bileşenin kaldırılması sırasında temizlenir. Klasör açılışı mevcut bilgi panelini kapatır. Tek/çift tık ayrımı hızlı çift tıklama içindir; özel olarak 500 ms'den uzun sistem çift tıklama eşiği bu koşuda doğrulanmadı.

Backend bağlantı, SSH anahtarları, SFTP, Stats, kasa şeması, PostgreSQL veya kriptografi değiştirilmedi. Önceki sonuçlar [0.3.4 raporunda](VALIDATION_0.3.4.md) geçerlidir; bu sürümde hepsinin yeniden çalıştırıldığı iddia edilmez.

## Kısa doğrulamalar

| Kontrol | Sonuç |
|---|---|
| TypeScript ve frontend | Derleme geçti |
| Hedefli tarayıcı kontrolleri | 19/19 geçti, yaklaşık 1 dakika. Klasör tek tık, menüden bilgiler, alt klasörler, bekleyen bilgiyi iptal, Ctrl/Shift seçimleri, klavye, Türkçe giriş ve mevcut menü işlemleri dahil |
| Host tek/çift tık | 1366×768 ve 1920×1080, kart/liste görünümlerinde geçti. İkinci tık 300 ms sonra aynı ekran koordinatına gönderildi; bilgiler araya girmedi, doğru host için tek `session_start` çağrısı doğrulandı. Bu arayüz testi sentetik IPC kullanır; üretim SSH bağlantısı değildir |
| Windows kurucusu | NSIS CRC, kurucu içindeki EXE'nin derlenen dosyayla eşitliği (Tauri bundle işareti hariç), sürüm 0.3.5, Mosh, lisanslar ve çevrimdışı WebView2 önkoşulu geçti. Kullanıcının TermTerm uygulaması açık olduğundan Windows'ta yeniden kurulum/kaldırma ve GUI açılışı bu sürüm için tekrarlanmadı; çalışan oturumlara dokunulmadı |
| Linux paketleri | Ubuntu 22.04 ortamında DEB 0.3.4 üzerine kuruldu; sürüm/mimari, APT, ldd, dpkg ve gerçek GUI açılışı geçti. AppImage gerçek pencere açtı. RPM sürüm/mimari/digest/içerik kontrolü geçti; RPM dağıtımında kurulum yapılmadı. En yüksek GLIBC gereksinimi 2.34; Mosh çalıştı. Üretim bağımlılıklarında test sürücüsü yok |
| Updater imzası | Windows kurucusu ve Linux AppImage mevcut public key ile doğrulandı; her paketin bir baytı değiştirildiğinde doğrulama reddedildi |
| Kaynak denetimi | 275 dosyada özel kasa/export/anahtar ve kimlik bilgisi sızıntısı denetimi geçti |
| Son arşivler | Windows x64, SHA-256 manifesti, portable ZIP'teki Mosh/WebView2/lisans/kılavuzlar ve kaynak allowlist kontrolü geçti. Kaynak ZIP'teki 275 dosyanın tamamı Git dosya listesi ve çalışma ağacındaki içerikle birebir karşılaştırıldı |

Tekrar çalıştırma: `node node_modules/@playwright/test/cli.js test tests/ui/context-menu.spec.ts tests/ui/interface.spec.ts tests/ui/locale.spec.ts --grep-invert '10,000|10000|10k'`. Performans/yük senaryosu bu küçük değişiklikte tekrarlanmadı. Native test dosyalarının klasör adımları yeni tek tık davranışına uyarlandı; tüm native süit yeniden çalıştırılmadı.

## Sınırlar

- Windows x64 NSIS ve portable ZIP; Linux x64 DEB/RPM/AppImage. ARM, Alpine/musl ve macOS paketi yoktur.
- Linux test ortamı Ubuntu 22.04'tür; her Linux dağıtımında kurulum iddiası yoktur. RPM yalnızca paket içeriği/metadata/digest doğrulaması alır.
- Donanım FIDO2/Hello/seri portlar, üreticiye özel şifreli importlar ve kullanıcı üretim sunucuları bu düzeltme için tekrar test edilmedi. Yeni işlev eklenmediği için uzun SSH/Stats yük testleri yapılmadı.
- Windows Authenticode sertifikası yoktur. Updater imzası mevcut anahtarla hazırlanır. DEB/RPM güncellemesi elle paket yöneticisiyle yapılır; APT/YUM deposu yoktur.

Yayın sırasında kaynak dosya listesi, kurulum dosyalarının SHA-256 değerleri, imzalar ve canlı Windows/Linux güncelleme adresleri doğrulanır. Kişisel kasalar, exportlar ve imzalama sırları kaynaklara/paketlere alınmaz.
