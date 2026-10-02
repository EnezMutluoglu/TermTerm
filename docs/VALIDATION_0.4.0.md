# TermTerm 0.4.0 doğrulaması

2 Ekim 2026. Sahip, Team 0.4.0 ve parola üreticinin kontrolleri sorunsuzsa Windows/Linux için yayımlanmasını açıkça istedi. `codex/team-passwords-040`, develop/0.3.7 ve Team geliştirme dalını birleştirir. macOS/GitHub derleme işleri başlatılmadı.

## Sonuçlar

| Kontrol | Sonuç |
|---|---|
| Windows genel Rust | 53 geçti, 7 ortam/yardımcı testi varsayılan koşuda atlandı. Parola rastgele karakter sınıfları/uzunluk, şifreli dosya/50 kayıt/yeniden açma, kayıtların outbox/export dışında olması dahil |
| Windows gerçek TLS PostgreSQL Team | Ayrı hedefli ignored test geçti. Şifreli kayıtlar, veri/sır ayrımı, yetki, geçmiş ve ortak terminal taşıma testleri |
| Linux Team/parola/ölçüm | 8 Team testi (bir yardımcı dahil), 2 parola, 7 kaynak ölçümü testi geçti; gerçek TLS PostgreSQL dahil |
| Gerçek SQL | Hesap ve token rotation, doğrudan SQL/rol/GUC reddi, ACL/devir, host-only edit, eşzamanlı edit/delete CAS, idempotency, karar, geçmiş/pin, iptal edilmiş aday/audit teslimi geçti |
| TypeScript ve Vitest | Üretim frontend derlemesi, 25 test geçti |
| Native Windows parola panosu | 3/3, 18,4 sn. Gerçek sistem panosu, seçilen karakter sınıfları, gizle/göster, şifreli diskte açık metin bulunmaması, yeniden açma, 50 kayıt/silme, yanlış kasa kapsamı reddi; Team hesabına özel yerel geçmiş ve başka hesaptan yalıtım |
| Native Windows Team | 5/5, 2 dk 47,8 sn. Gerçek SSH chain/SFTP, izinli iki host/gezinti yolu, yetkisiz export/edit/jump reddi, iptalde kapanma/karar, gerçek PostgreSQL kapatma/açma + offline SSH/edit, geçmiş/restore ve aktarım |
| Kısa tarayıcı regresyonu | 7 senaryo geçti: 4 ağ/disk paneli, 2 palet, 1 terminal resize. İlk paralel derleme koşusunda birleşik palet testi 30 sn zaman aşımına uğradı; aynı test tek başına 4,4 sn geçti |
| Kararlı paketler | Windows x64 NSIS derlendi; CRC, birebir EXE payload, Mosh/Cygwin, çevrimdışı WebView2 ve güncel lisans içerikleri doğrulandı. Linux DEB 0.3.7 üzerine kuruldu; DEB/AppImage grafik pencereleri açıldı, Mosh ve sistem bağımlılıkları geçti. RPM digest/metadata/payload kontrolü geçti |
| İmza ve kaynak arşivi | Son NSIS/AppImage imzaları ve değiştirilmiş dosya reddi testleri geçti. SHA-256, taşınabilir runtime/önkoşullar ve kaynak arşivi kontrol edildi. Team şifreleme bağımlılıklarının lisansları paketlere eklendi. Python test önbelleğinin kaynak paketine alınması engellendi |

## Test ortamı ve sınırlar

WSL Ubuntu 24.04'te yalnız `18/termterm` 55432, `termterm_team_e2e` ve disposable SSH 22222 kullanıldı. Windows'un geçici sistem port rezervasyonu 55432'yi kapsadığı için yalnız localhost:45432 → WSL localhost:55432 test iletimi kullanıldı; TLS CA/hostname doğrulaması açık kaldı. Bu geçici laboratuvar iletimi ürün/paketlere dahil değildir. 5432 main kümesi ve kullanıcının mevcut kasaları değiştirilmedi.

Team şeması migration kimliğiyle kurulur; uygulama rolü şema oluşturmaz. 001 ilk kurulum; 002–005 RPC/ACL migration'ları ve grant_app.sql kaynak paketindedir. Önceki geliştirme denemesinin ayrıntıları `VALIDATION_TEAM_0.4.0-dev.1.md` içindedir.

Parola panosu `.ttvault` veya hesaba ait `.ttteam` dosyasında şifrelidir. Team üyelerine, audit'e, PostgreSQL'e, CSV/OpenSSH veya `.ttbackup` exportuna eklenmez. Kişisel portable `.ttvault` kopyası panoyu içerir. Kişisel kasa/yedek formatı değişmez.

- macOS/hardware keys/seri aygıt/üretim SSH sunucusu ve iki ayrı bilgisayarda native ortak terminal konuk arayüzü doğrulanmadı. SMTP/e-posta doğrulama/parola sıfırlama yoktur. FIDO2/Hello ve üretici şifreli import sınırları sürer.
- Linux tabanı Ubuntu 22.04, glibc 2.34+/WebKitGTK 4.1. Her dağıtımda kurulum doğrulanmış değildir. RPM kurulum testi farklı bir dağıtımda yapılmadı.
- Windows kurucusunun mevcut kullanıcı kurulumunu değiştiren kur/kaldır testi yapılmaz; NSIS CRC, EXE payload ve bağımlılıklar denetlenir. Authenticode sertifikası yoktur. NSIS/AppImage updater imzasıyla doğrulanır; DEB/RPM elle paket yöneticisiyle kurulur.
- Audit bakımı Team politika ekranından sınırlı çağrıyla yapılır. Çevrimdışı cihaz merkezdeki yeni iptalleri sunucuya erişene kadar öğrenemez; önceden dışarı kopyalanmış sırlar uzaktan geri alınamaz.
- Kullanıcı tercihi doğrultusunda uzun yük testi yapılmadı. Bağımsız üretim güvenlik denetimi/sertifikasyonu iddia edilmez.
