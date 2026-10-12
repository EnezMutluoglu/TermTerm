# TermTerm 0.4.1 — doğrulama raporu

12 Ekim 2026. Normal uygulama kimliği `local.termterm.desktop`; Team şema64 / API1 / cache8. 0.5 SQL Workspace değişiklikleri dahil değildir. Kullanıcı 0.4.1 yayınını, aşağıdaki ortam sınırlamalarını kabul ederek onayladı.

## Bu kararlı hazırlıkta yeniden çalıştırılanlar

- Frontend birim testleri: **26/26 geçti**.
- Windows Rust: **63 geçti**, 8 varsayılan olarak dışarıda. Ayrı süreç yardımcıları üst testlerinden çalıştı; kişisel PostgreSQL testleri ayrıca 2/2 geçti. Eski v1 Team testi kapsam dışı; Windows/AppImage imza testleri ayrıca geçti.
- Linux Team istemcisi: **33/33**, servis **6/6** geçti.
- Kişisel PostgreSQL TLS: **2/2 geçti**. İlk bağlama, yerel kayıtların korunması, çakışma, üye zarfları ve RLS reddi.
- Team 63→64: gerçek özel dump/restore, kayıtların korunması, yanlış hedef/başarısız restore, kesinti/devam, tekrar güvenliği, checksum ve eski şema/binary reddi geçti.
- Team sürüm politikası: gerçek HTTPS ile lider seçimi, üye reddi, eski revizyon, tekrar, takım izolasyonu ve uyumlu/uyumsuz istemci erişimi geçti.
- Uygulanmış 001–064 migration dosyalarının önceki doğrulanmış paketle byte eşitliği geçti.

## Aynı uygulama işlevleri için önceki tamamlanmış kabul

0.4.1-dev.5 işlevsel kabulü: **22/22 gerçek Windows native**, **44 arayüz + 1 geçmiş yenileme regresyonu**, **2/2 Mosh**. Chain, HTTP/SOCKS proxy parolası saklama, iki kullanıcı, SSH/SFTP bütünlüğü, üç tünel, yetki iptali, kurtarma/değişim, günlük, taşıma ve workspace akışları test edildi. Windows/Linux paket açılışı, systemd ve ayrı/mevcut TLS PostgreSQL ile iki Compose kurulumu geçti.

Bu sonuçlar kararlı kurucuda yeniden 22/22 test çalıştırılmış gibi sunulmaz. Kararlı hazırlıkta sürüm sözleşmesi, paketleme, belgeler ve testlerin sürüm beklentileri değişti; işlevsel bağlantı kodu değişmedi.

## Paket ve kararlı yükseltme

Normal kimlikle optimize paketler kaynak kodundan yeniden derlendi. Geliştirme paketlerinin adı değiştirilmedi.

- Windows NSIS: optimize derleme, offline WebView2 içeren kurucu, kurulum/kaldırma ve paketlenmiş Mosh çalıştırma geçti. Taşınabilir ZIP ve kaynak arşivi bütünlüğü, x64 PE, Mosh/WebView2/lisans ve kılavuzlar doğrulandı.
- Normal kimlikle gerçek 0.4.0→0.4.1 kurucu yükseltmesi geçti. Eski sürümde oluşturulan host/sır/font ayarı korundu; kasa açılışta DPAPI ile kendiliğinden açıldı. Yeni kurucudan Team HTTPS aktivasyonu, şifreli ortak kasa oluşturma, çıkış/hatırlanan giriş ve 0.4.1 sürüm politikası da geçti. Kullanıcının asıl recent dosyası byte eşitliğiyle geri bırakıldı.
- Linux DEB/AppImage: gerçek pencere, kurulum/kaldırma ve test portu yokluğu geçti. RPM: 0.4.1/x86_64 metadata, bağımlılıklar ve payload/header digest doğrulaması geçti; RPM dağıtımında native açılış yapılmadı.
- Linux servis paketi: temiz64 migration, checksum/tekrar/yeni şema reddi, ilk lider, systemd+Caddy/CA, restart ve sınırlı rol geçti. Ayrı PostgreSQL ve mevcut TLS PostgreSQL ile paketlenmiş iki Compose seçeneği geçti.
- Windows kurucu ve AppImage: mevcut updater anahtarıyla imza doğrulaması ve tek byte değişikliğinde ret ayrı testlerle geçti.
- Tam kaynak arşivi, özel veri/test kancası taraması ve SHA256 geçti. Kaynaklar ve çalışma zamanı arşivleri ayrı; yalnız açıkça izinli `tests/ssh-keys` tek kullanımlık fixture anahtarları kaynakta bulunur. Üretim dependency graph ve iki frontend bundle içinde test sürücüsü/kancası yoktur. `SOURCE_FINGERPRINT.json` kaynak dosyalarını, `SHA256SUMS.txt` dağıtım dosyalarını doğrular.
- Son kaynakta gerçek HTTPS kapsamlı entegrasyon geçti: iki kullanıcı izolasyonu, şifreleme, kimlikler, kaynaklar, günlük/kurtarma dahil; yoğun derleme sırasında 621,75 saniye. Güvenilir/yanlış CA, hostname, sahte aktör ve iptal edilen oturum kontrolleri geçti.

## Açıkça ertelenen veya doğrulanmayan ortamlar

Windows agent/Pageant ve gerçek şirket OTP sağlayıcısı doğrulanmadı. Linux agent ve sertifika, laboratuvar MFA doğrulandı. macOS/FIDO2/Windows Hello kullanıcı kararıyla ertelendi. RPM dağıtımında native açılış yapılmadı; Linux çalışma tabanı Ubuntu 22.04. Linux parola hatırlama Secret Service gerektirir; güvenli saklama yoksa parola girişi devam eder. Windows kurucu Authenticode imzalı değildir; güncelleme paketi ayrı mevcut updater anahtarıyla doğrulanır.

Kullanılmamış 0.4.0 Team aktarımı kapsam dışıdır. Gelecekteki v2 verisini koruyan sürüm yükseltmesi kapsam içindedir. Mosh UDP erişimi gerektirir. Üretim sunucusu ve şirket CA/OTP ortamları laboratuvar sonuçlarıyla eşdeğer sayılmamıştır.

## İlk denemeler ve düzeltmeler

Windows yükseltme harness'inin ilk denemesinde Playwright import yolu düzeltildi. Sonraki denemede uygulama kasayı otomatik açmışken test yeniden açmaya çalışıp kilit aldı; test otomatik açılmış kasayı okumaya düzeltildi. Devam betiğinin PowerShell dizi hatası da düzeltildi. Son deneme kurulu stable binary üzerinde geçti; ürün koduna bu sebeplerle dokunulmadı. Paket taramasındaki ilk PowerShell koşulu boş eşleşme yerine birden çok `false` değerini dolu dizi saydı; doğrudan byte/metin taraması iki bundle için sıfır eşleşme doğruladı.

Kararlı sürüme geçişte bir istemci testinin eski prerelease beklentisi yanlış kaldı: 32 geçti / 1 başarısız. Beklenti semver'e göre güncellendi; yeniden 33/33 geçti. Linux özel build çağrısının ilk denemesi CRLF nedeniyle başlamadı; LF düzeltmesiyle derleme tekrar başlatıldı. Uygulanmış SQL veya güvenlik denetimi değiştirilmedi.
