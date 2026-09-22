# TermTerm 0.3 doğrulama raporu

21 Eylül 2026. Üretimden ayrı WSL PostgreSQL 18/termterm:55432 ve SSH:22222 laboratuvarı kullanıldı. Kişisel sunuculara test bağlantısı açılmadı. 30 dakikalık izleme yapılmadı.

| Kontrol | Sonuç / kanıt |
|---|---|
| Windows Rust | 28 test geçti; geçersiz ProxyJump portları, Türkçe kodlama, hızlı duraklat/devam, şifreli kayıt ve hata dayanıklılığı dahil |
| TypeScript | 6 test geçti: ortak kısayollar, terminal girdi kuyruğu |
| Tarayıcı UI | 4 test geçti; 10.000 host araması 117,7 ms; klasör/tema/kart/liste/düzen kontrolleri |
| Windows native çalışma alanı | 11 test geçti: gerçek SSH parola/anahtar/chain, HTTP/SOCKS, üç tünel, SFTP, ConPTY, Mosh, 16 panel/broadcast, DB restart/offline sync, ortak terminal, log, yedek |
| Native import/SFTP | 4 test geçti: dokuz biçimde preview/apply/copy-update-skip/reopen/SSH, kaynak hash, yanlış hedef/port reddi, pause/resume/changed-source/cancel, remote-to-remote hash, mkdir/rename/chmod/remove ve workspace ilişki koruması |
| Kullanıcının MobaXterm dosyası | 1 test geçti: 414 host, 155 klasör, 569 eklenen kayıt, 0 başarısız; Windows-1254; alan/ilişki eşitliği, şifreli yeniden açma ve kaynak SHA-256 doğrulandı |
| Stats ve gerçek tuş baytları | 4 native test geçti; uzak Linux ve yerel Windows, gizlide/kapalıyken duraklama, F tuşları ve Ctrl+K/N, kasa kilidinde temizleme, 16 monitörde 15 saniyelik kısa kontrol |
| Windows platform smoke | 1 test geçti: onboarding, yerel shell, canlı metrik, üç seviyeli klasör, tema, taşınabilir kasa kopyası |
| PostgreSQL | 2 entegrasyon testi geçti: TLS, conflict/RLS/üye zarfı/ortak terminal; ilk bağlamada yerel-only/different kayıtların korunması, değişen önizleme token reddi ve uygulama hesabının doğrudan DELETE yetkisinin reddi |
| Ubuntu 22.04 | 27 Rust, 6 TypeScript, 4 tarayıcı UI testi geçti; 10.000 host araması 89,6 ms. Native platform smoke ve 4 import/SFTP kabul testi geçti |
| Windows ↔ Linux kasa taşıma | Windows kasası Linux'ta yalnızca parolayla açıldı; ilişkiler/tema doğrulandı, Türkçe kayıt eklendi, yedek geri yüklendi; oluşan kasa Windows'ta tekrar açıldı. İki native test geçti |
| Windows NSIS | Sessiz kurulum, paket işareti dışında release ile birebir exe eşitliği, Mosh çalıştırma, gerçek uygulama penceresi, düzgün kapanış, test portunun kapalı olması ve kaldırma geçti |
| Linux paketleri | Ubuntu 22.04 ortamında DEB kurulumu ve AppImage/DEB gerçek pencere açılışı geçti. GLIBC ihtiyacı en fazla 2.34; dinamik bağımlılıklar çözüldü, Mosh dahil |
| Teslim arşivleri | Windows x64 PE, kurucu, portable Mosh/DLL ve çevrimdışı WebView2, kaynak allowlist ve SHA-256 kontrolleri geçti. Microsoft WebView2 imzası geçerli; TermTerm uygulaması imzasız. Normal Cargo bağımlılık ağacında WebDriver yok |
| macOS Intel/ARM64 | Kullanıcı isteğiyle derleme bekliyor; kaynak/CI/ad-hoc imza/Mosh hazırlığı sağlandı |

## Düzeltmelerin doğrudan kanıtı

- Import önizlemesinden sonra ilişkilerin remap edilmesi, bulunmayan dizi alanlarına JSON null yazıyordu. Gerçek SSH bağlantı testi bu sorunu ortaya çıkardı; alan yalnızca varsa dönüştürülüyor ve her import hostunun gerçek Host tipine çözülebilmesi test ediliyor.
- 10.000 host aramasında gizli SFTP bağlantı listeleri tekrar çiziliyordu. İlk açılışa kadar SFTP yüklenmiyor, bileşen memo ile korunuyor, arama metni kayıt değiştiğinde hazırlanıyor. Ölçülen 626 ms başlangıçtan 117,7 ms'ye indi; 150 ms sınırı gevşetilmedi.
- Windows cold-start kaynak probuna daha geniş ilk süre bütçesi verildi. Aynı kaynağın sorguları seri; zaman aşımı sonraki sorguları biriktirmiyor.
- Windows Rust test exe'si native diyalog kodu nedeniyle Common Controls v6 manifestine ihtiyaç duyuyordu. Test manifesti eklendi; Tauri uygulamasında ikinci manifest üretilmesi engellendi.
- Gerçek MobaXterm dosyası eski Türkçe ANSI kodlamasındaydı. Kodlama seçimi ve boş klasör koruması eklendi. Gerçek dosya kaynak ZIP'ine dahil edilmedi.
- SFTP iptal testi, beklenen hata nesnesini WebDriver protokol hatası gibi yorumluyordu. Sonuç JSON zarfıyla taşınıyor; iptal, hedef hash'i ve geçici dosya temizliği üzerinden doğrulanıyor.
- Linux chroot ortamında kök dosya sistemi bind mount ile tanımlandı. Önceki test ortamında `/` yerine dış bağlama yolu görünüyordu; düzeltilen ortamda kök disk ve canlı sayaç kontrolü geçti.
- NSIS, Tauri'nin sabit genişlikli `UNK` paket işaretini `NSS` olarak değiştirir. Kurulum doğrulaması yalnızca bu üç byte farkını normalize eder; kalan exe SHA-256 ile aynı olmalıdır.

Native testler üretimden ayrı E2E uygulamasında çalıştırıldı. Son üretim paketlerinde ayrıca kurulum/başlatma ve test sürücüsü bulunmaması kontrol edildi. Windows kapsamlı native koşularından sonraki küçük ProxyJump/seri duraklatma/şema düzeltmeleri son Rust ve PostgreSQL koşularıyla doğrulandı; Linux import/SFTP native koşusu bu düzeltmeleri içeren derlemede çalıştı. Başka fiziksel Windows cihazında çevrimdışı WebView2 kurulumu ve ayrı Linux masaüstü cihazı denenmedi.

## Doğrulanmadı / tamamlanmadı

Doğrudan FIDO2/Windows Hello kayıt-imzalama adaptörleri, üreticiye özel şifreli MobaXterm/SecureCRT çözücüleri, tam ileri vendor alanları ve zaman çizelgeli log işaretleri tamamlanmış değildir. Fiziksel seri/donanım anahtarı, MFA/CA sunucusu, gerçek harici editör etkileşimi, masaüstünden fiziksel dosya bırakma, gerçek bulut hesabı, macOS ve başka fiziksel Windows PC kontrolleri geçti sayılmadı. Bütün UI işlemlerinin her hata kombinasyonu için eksiksiz kapsama iddiası yoktur.

MobaXterm bağlantı sayılarının eşitliği harici özel anahtarların veya üretici şifrelerinin mevcut olduğu anlamına gelmez. 761 uyarı, ileri seçenekler ve bulunmayan anahtar referanslarını içerir; bunlar sessizce kayıp sayılmadı.

Test kaynakları `tests/import`, `tests/desktop`, `tests/ui` ve Rust modüllerindedir. Kişisel kaynak dosyası ve `.lab` sırları teslim arşivine alınmaz. Paketlerin son boyut/hash/başlatma kontrolleri teslimat manifestinde yer alır.
