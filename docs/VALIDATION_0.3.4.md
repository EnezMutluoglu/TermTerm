# TermTerm 0.3.4 doğrulama raporu

23 Eylül 2026. Bu çalışma kapsamındaki yeni Windows/Linux sürümünün yayımlanması sahibi tarafından istendi. macOS işleri kullanıcının önceki isteğiyle beklemede; Mac CI başlatılmadı.

## Değişiklik ve veri güvenliği

Kayıtlı parola açılışta otomatik kullanılır. Parola kutusunu elle doldurmak mevcut hatırlama tercihini silmez; yalnızca açıkça kaldırılan işaret kaydı siler. Elle kilitleme otomatik olarak açılmaz. Eksik/bozuk/erişilemeyen sistem deposu normal parola ekranına döner. Depo işlemleri arka plan iş parçacığında yapılır. Parola varlığı sorgusu UI'ye sadece boolean döndürür; parolanın kendisi verilmez. Kasa ve yedek biçimi değişmez.

Türkçe mesaj kataloğu giriş, menüler, sağ tık, ayarlar, SFTP, bağlantı düzenleme, import/export ve güncelleme akışlarında kullanılır. Kullanıcı kayıtları, dosya yolları ve terminal komut/çıktıları çevrilmez. Alt sistemlerin ayrıntılı teşhisleri İngilizce kalabilir. İngilizce test ve çeviri altyapısı korunur.

## Kontroller

| Kontrol | Sonuç |
|---|---|
| TypeScript ve frontend | Derleme geçti |
| TypeScript birim testleri | 21/21 geçti; çeviri alanları ve kullanıcı değerlerinin korunması dahil |
| Chromium arayüz | 35 senaryonun tamamı geçti. İlk koşuda iki yeni testte yanlış Türkçe düğme seçicisi düzeltildi; derleme yükü sırasında 271 ms ölçülen arama ayrı tekrar koşusunda 95,1 ms ile geçti. Diğer 32 senaryo ilk koşuda geçti |
| Hatırlama arayüzü | Açılış, StrictMode tek açma, elle kilitleme, kayıtlı parolayla boş alan üzerinden açma, elle parola girişi, açıkça unutma, bozuk depo ve depo kaydetme hatası geçti |
| Windows Rust | 41 geçti, 0 hata, 5 özel/yardımcı test varsayılan koşuda atlandı. DPAPI ile ayrı süreçte açma, bozuk depo ve açıkça unutma; kasa/import/SSH anahtarları ve Windows kaynak ölçümü dahil |
| Gerçek Windows uygulama yeniden başlatma | Üç ayrı uygulama sürecinde oluştur/hatırla, sonraki açılışta otomatik açma, elle kilitleme, kayıtlı parolayla açma, elle girerken kaydı koruma, açıkça unutma, yeni süreçte yanlış/doğru parola ve kayıt içeriği eşitliği geçti. Türkçe giriş/sunucular/güncellemeler ekranları kaydedildi |
| Windows kurulum | NSIS sessiz kurulum, kurulu EXE hash eşitliği (bundle işareti hariç), paketlenen Mosh, gerçek pencere açılışı, düzgün kapanma, üretimde test portunun bulunmaması ve kaldırma geçti. Kullanıcının son-kasa işaretçisi test sırasında korunup geri kondu |
| Linux paketleri | Ubuntu 22.04 üzerinde DEB 0.3.3 üzerine kuruldu, APT/ldd kontrolü ve gerçek GUI açılışı geçti; AppImage açıldı. RPM mimari/sürüm/digest/içerik kontrolü geçti; RPM dağıtımında kurulum yapılmadı. En yüksek GLIBC gereksinimi 2.34; paketlenen Mosh çalıştı |
| Updater imzası ve değiştirilmiş dosya reddi | Son Windows kurucusu ve Linux AppImage mevcut public key ile doğrulandı; her dosyanın bir baytı değiştirildiğinde doğrulama reddedildi |
| Kaynak ve paket denetimi | Kişisel kasalar, gerçek dışa aktarılmış anahtarlar ve imzalama sırrı kaynaklardan dışlandı. Kaynak ZIP dosya listesi ve içerikleri Git kaynaklarıyla karşılaştırıldı; paket bütünlük denetimi geçti |

Native testler `local.termterm.desktop.e2e` profili ve geçici test kasasında çalışır; üretim kullanıcı profili/test dışı kasa kullanılmaz. Hatırlama testinin üç aşaması ayrı uygulama süreçleridir (`tests/desktop/remember-vault.mjs`). Test harness ve WebDriver üretim build'inde etkin değildir. `playwright.config.ts` mevcut İngilizce regresyonları, `locale.spec.ts` varsayılan Türkçeyi ayrı doğrular.

Yeniden çalıştırma: önce `pnpm tauri build --debug --features e2e --config src-tauri/tauri.e2e.conf.json --no-bundle`, sonra `node scripts/test-remember.mjs`. Farklı konumdaki E2E EXE için `TERMTERM_BINARY` kullanılabilir. Çalıştırıcı üç süreci sırayla açar ve önceki E2E son-kasa kaydını sonunda geri koyar. Son kaynak incelemesinde başlangıç Promise referansı da temizlendi; kilitleme sonrasında açılış isteğinin kasa verilerini tutması önlendi. İlgili sekiz başlangıç/Türkçe senaryosu tekrar geçti.

## Paket ve ortam sınırları

- Windows x64 NSIS ve portable ZIP; Linux x64 DEB, RPM ve AppImage. Windows/AppImage güncellemeleri önceki public key ile imzalanır. DEB/RPM paket yöneticisiyle elle kurulur; APT/YUM deposu yoktur. Windows Authenticode sertifikası yoktur.
- Linux gerçek GUI kontrolü izole Ubuntu 22.04 ortamındadır. Her Debian/Fedora/openSUSE sürümünde kurulum iddiası yoktur. Linux Secret Service ile gerçek masaüstü oturumunda parola saklama ve macOS Keychain bu koşuda denenmedi; Windows DPAPI doğrudan doğrulanır.
- Donanım FIDO2/Hello/seri portlar ve üreticiye özel şifreli import sınırları önceki sürümle aynıdır. Mac, Linux ARM ve Alpine/musl paketleri yoktur. Önceki SSH/chain/SFTP/Stats sonuçları [0.3.3 raporunda](VALIDATION_0.3.3.md) korunur; bu değişiklikte tüm geçmiş donanım/ağ ortamlarının yeniden çalıştırıldığı iddia edilmez.
- 30 dakikalık izleme yapılmadı. Kurulu kullanıcı uygulaması otomatik güncellenmedi; yayın kullanıcı tarafından denetlenip başlatılabilir.

Sürüm dosyalarının SHA-256 değerleri `SHA256SUMS.txt` içindedir. GitHub yayın denetimi sunucu digest değerlerini, canlı güncelleme feed'ini ve indirme adreslerini yerel paketlerle karşılaştırır.
