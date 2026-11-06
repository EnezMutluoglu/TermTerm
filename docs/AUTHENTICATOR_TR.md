# Authenticator geliştirme durumu

28 Ekim 2026 — yalnız kaynak ve izole geliştirme uygulaması; yayın veya otomatik güncelleme onayı değildir.

Authenticator, Kasa ve SFTP yanında üst çalışma alanı sekmesidir. Standart `otpauth://totp` adresi, Base32 kurulum anahtarı veya cihazda çözümlenen PNG/JPEG/WebP QR görseliyle hesap eklenir. SHA1/SHA256/SHA512, 6/8 hane ve 1–300 saniye dönem desteklenir. Kodlar baştaki sıfırları korur; koda tıklamak güncel kodu yeniden doğrulayıp panoya kopyalar. VPN OTP alanına yapıştırma kullanıcıya aittir. Cihaz saati doğru olmalıdır.

Google hesabıyla eşitleme, Google Authenticator toplu taşıma QR biçimi (`otpauth-migration`), HOTP, push MFA ve otomatik VPN kod doldurma bu kapsamda yoktur. QR dosyası dış servise gönderilmez; boyut sınırı 4 MiB, çözümlenen görsel sınırı 4096 × 4096 ve 12 milyon pikseldir. Kamera izni gerekmez.

## Kişisel kasa ve yedek

Kurulum sırları genel kayıt alanında bulunmaz. Kasa anahtarıyla doğrulamalı şifrelenen ayrı durum verisi kullanılır; genel kasa bilgisi, kayıt listesi, senkronizasyon outbox'ı ve kayıt dışa aktarımları sır içermez. Listeler yalnız hesap bilgisi, kod, dönem ve son kullanma zamanını döndürür. Kurulum sırrını göstermek ayrı bir kullanıcı işlemidir.

İlk hesap ekleme, şifreli veri ile kasa başlık sürümü **3** değişikliğini aynı SQLite transaction içinde yapar. Eski v1/v2 okuyucuları dosyayı yazmaya açmadan reddeder. Güncel okuyucu eski v1/v2 kasaları açabilir; VPN profil yazımı sürümü düşürmez. Boş veya eski yedek geri yüklemek gereksiz kasa yükseltmesi yapmaz.

Tam şifreli yedekler ve taşınabilir kasa kopyaları Authenticator hesaplarını korur. Authenticator içeren yedek biçimi **3** kullanır; eski uygulamalar bu yedeği reddeder. Güncel okuyucu yedek v1/v2 biçimlerini de okur. Yedek önizlemesi kurulum sırlarını arayüze göndermez. Genel kayıt içe aktarma Authenticator hesaplarını taşımaz ve bunu uyarıda belirtir; **Yedekten geri yükle** kullanılmalıdır. Seçili kayıt dışa aktarımı Authenticator içermez. Geri yükleme mevcut kasa veya Authenticator verisinin üzerine yazmaz.

Kasa kilidi, hesap/kasa değişimi ve sekmeden çıkış arayüzdeki kodları, kurulum girişini ve açık sır gösterimini temizler. Her açılışın kapsam kimliği farklıdır; eski kapsamla yapılan native işlem reddedilir. Kuyrukta bekleyen pano yazımı da kapsam değişmişse atlanır. Kullanıcının panoya önceden kopyaladığı kod, işletim sisteminin pano/geçmiş politikalarına tabidir.

## Team

Yeni hesap varsayılan olarak kişiseldir. Team kimliklerinin mevcut veri/sır anahtar zarfları, şirket kurtarma anahtarı, kimlik kurtarma ve anahtar döndürme akışları kullanılır. Açık paylaşım, mevcut izin önizlemesiyle bağımsız ortak kopya oluşturur; özel kaynak korunur. Ortak kopyanın alıcıları veya içeriği revizyon denetimiyle güncellenir. Kapsam klasör veya hosttur.

Ortak TOTP için **Kod kullan** mevcut `connect` iznine, **Kurulum sırrını gör** `reveal` iznine, dosyaya sır aktarma `export` iznine bağlıdır. Kod kullanımı tek başına native arayüzden kurulum sırrını döndürmez. Sunucu ayrıca güncel kapsam yetkisini denetler. TOTP bir SSH bağlantı kimliği olarak seçilemez; TOTP ve diğer kimlik türleri arasında dönüştürme sunucuda reddedilir. Başka kimlik türündeki kayıt, Authenticator silme işlemiyle kaldırılamaz.

Team kodları bu sürümde çevrimiçi doğrulanır; cihazdaki çevrimdışı kimlik/bağlantı önbelleğine TOTP tohumu yazılmaz. Ağ, bakım, oturum veya izin hatası çevrimdışı kod kullanma yetkisine dönüşmez. Oturum kapanışı geç tamamlanan sonuçları geçersiz kılar. Şirket sahibi ortak kimliği kurtarabilir; kişisel kimlik kurtarma mevcut üye kurtarma ve kullanıcının kendi cihazında doğrulama akışını izler.

Bu izinler uygulama ve native API sınırlarıdır. Kod üreten yetkili cihazın belleğinde tohum bulunur; cihazın sahibi veya belleği okuyabilen bir saldırgana karşı tohumun çıkarılamaz olduğu iddia edilmez. Bu sınır mevcut SSH kimliği kullanım modeliyle aynıdır.

Team migration **065_authenticator.sql**, şema **65**, API **1**, Team cache **8** kullanılır. Uygulanmış migration dosyaları değiştirilmez. Mevcut DB için [Team yükseltme süreci](TEAM_UPGRADES_TR.md) zorunludur; sonraki migration numarası en az 066 olmalıdır. Sunucu uyumsuz şemayla hazır olmaz. İstemci sürüm kataloğuna yeni bir yayın eklenmemiştir.

## Kontroller ve kalan kapsam

- `team-client/src/authenticator.rs`: RFC 6238'in 18 SHA1/SHA256/SHA512 vektörü, doğru 20/32/64 bayt test anahtarları, 6 hane, dönem sınırı, URI dönüşümü, yinelenen parametre, issuer uyuşmazlığı, bozuk Base32/encoding, HOTP ve taşıma QR reddi.
- `src-tauri/src/authenticator.rs`: şifreli saklama, yeniden açma, kapsam/revizyon denetimi, transaction hatasında geri alma, eski okuyucu sınırı, VPN sürüm koruması, genel listelerde sır bulunmaması, taşınabilir kopya, yedek v1/v2/v3 ve kaynağı değiştirmeyen geri yükleme.
- `team-client/tests/live_service.rs`: gerçek şifreli özel/ortak TOTP, native liste redaksiyonu, kod izni ile sır izni ayrımı, SSH profil/tür/silme sınırları, şirket anahtarıyla kurtarma ve iptal edilen erişim.
- `tests/team_v2_upgrade.py`: ayrı DB üzerinde 64→65 yükseltme, doğrulanmış özel yedek/geri açma, korunmuş kayıtlar, kesinti/yeniden başlatma, checksum ve downgrade reddi.
- `tests/ui/authenticator.spec.ts`, `tests/ui/vpnDesktop.spec.ts` ve `tests/clipboard.test.ts`: yerel QR, kod/kopyalama/süre, hatalı yetki, geciken sonuç, kilit ve hesap değişimi, üst sekme/VPN OTP geçişi ve kuyrukta eski pano yazımı.

UI testleri IPC taklidi ile çalışır; bunlar canlı VPN veya üretim MFA doğrulaması değildir. macOS/Linux masaüstü, üretim VPN/MFA servisleri, donanım anahtarları ve Google hizmetleri için test iddiası yoktur. Kurulu kararlı uygulama ve kararlı güncelleme akışı değiştirilmez.

28 Ekim doğrulama sonuçları: Team client Rust birim testleri **35/35**, Windows native Authenticator **2/2**, ilgili native VPN sınırları **7/7** ve kuyrukta pano yazımı testi **1/1** geçti. Bağımsız inceleme; gerçek PostgreSQL üzerinde temiz şema 65 kurulumu, sınırlı rol/izin/tür/silme denetimleri, 64→65 yedekli yükseltme ve kesinti/yeniden başlatma akışını doğruladı. Gerçek HTTPS + PostgreSQL üzerinde şifreli Team TOTP yaşam döngüsü **1/1** geçti; şirket kurtarma ve izin iptali dahildir. Arayüz IPC taklidi testleri **8/8**, gerçek App bileşenindeki OTP ve genel `secret` challenge geçişleri **2/2** geçti. TypeScript, kaynak whitespace ve repository hook kontrolleri geçti. Bunlar dağıtım, kurulu uygulama yükseltmesi veya üretim VPN bağlantısı değildir.

## Gerçek Windows arayüz kontrolü — 28 Ekim 2026

Son kaynakla ayrı VPN Dev kimliğinde derlenen `src-tauri/target/debug/termterm.exe`
kullanıldı. ProductName **TermTerm VPN Dev**, ProductVersion/FileVersion
**0.4.3-dev.1**, kimlik `local.termterm.desktop.vpndev`; SHA256
`4A3BD784031B6DCFA8431A0BBCFF25B6ED52CC0280573CDC31423C647F11A7A6`.
Kurucu çalıştırılmadı ve kurulu kararlı uygulama değiştirilmedi. Uygulama alt
bilgisindeki Cargo kaynaklı `v0.4.2` yazısı mevcut kozmetik farktır; executable
sürümünden ayrıdır ve kararlı kurulum kullanıldığı anlamına gelmez.

Root yalnız CUA ile yeni disposable kasa oluşturdu. Gerçek Windows dosya seçicisi
üzerinden public `tests/ui/authenticator-fixtures/rfc6238.png` seçildi; QR cihazda
çözümlendi ve hesap kaydedildi. Kod, geri sayım ve dönem değişiminde yeni kod
görüldü. Koda tıklama gerçek Tauri/Windows panosuna yazdı; Ctrl+V ile arama
alanına yapıştırılan değer boşluksuz altı haneli kodla eşleşti, ardından arama
temizlendi. Kasa kilidi kodları gizledi; parolayla aynı kasa tekrar açılınca bir
hesap ve güncel kod korundu. Ekran kanıtı
`artifacts/authenticator-ui/native-windows.png` yalnız public test hesabını içerir.
Geliştirme uygulaması bu zararsız test kasasıyla açık bırakıldı.

Bu kontrol kişisel Windows Authenticator arayüzü, yerel QR, pano ve kilit/yeniden
açma kanıtıdır. Yeni VPN bağlantısı veya gerçek sağlayıcı OTP doğrulaması
yapılmadı. Team Windows GUI, macOS/Linux masaüstü ve üretim MFA doğrulaması
olarak raporlanmaz.
