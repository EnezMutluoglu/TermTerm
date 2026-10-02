# TermTerm Team 0.4.0-dev.1 — geliştirme doğrulaması

23 Eylül 2026. Dal: `feature/team-access-history`. Kararlı 0.3.5, main, release etiketleri ve güncelleme kaynağı değiştirilmedi. Bu rapor üretim onayı değildir.

## Uygulanan işlevler

- Ayrı Team giriş/kayıt, hesap oturumları, sahibi olan takımlar, ortak kasalar, kaynak bazında izin/engel ve sınırlı yetki devri.
- Yalnız beş izinli PostgreSQL giriş fonksiyonu; doğrulanmış sunucu oturumu, TLS ve ek RLS. Uygulama rolü tablo/şema yetkilerine sahip değil. Oturum token’ları ve hesabın özel kimlik anahtarı frontend’e verilmez; SSH sırlarını görüntülemek ayrı izne bağlıdır.
- Ayrı şifreli `.ttteam` önbelleği; atomik yerel kayıt + sıralı işlem günlüğü, 24 saat düzenleme sınırı, yeniden gönderim güvenliği ve çevrimdışı bağlantı.
- İzin iptalinde önbellek/bağlantı temizliği; iptal öncesi hazırlanmış adayların sınırlı teslimi, yetkili karar ve alan birleştirme.
- Güncel kayıt + önceki 10 sürüm, sabitleme, geçici host sürümü, güncele dönüş, kalıcı geri yükleme ve silinmiş kayıt kurtarma.
- SSH chain/SFTP/backup/import/API Bridge yollarında Team yetkisi. Seçili yedekte bağlı klasör, kimlik ve jump kayıtları da denetlenir; izinsiz/eksik bağlı kayıt varsa işlem reddedilir.
- Host iznine bağlı şifreli PostgreSQL ortak terminali; tek yazıcı, kontrol devri, kısa ömür ve eski girdilerin reddi. Native servis açık oturumun gerçek hostunu esas alır.
- Türkçe Team ekranları; yetki matrisi, karar karşılaştırması, geçmiş ve kapsam taşıma önizlemesi. Taşıma, üyelik ve kalıcı geri yükleme çevrimiçi yapılır.

## Doğrulanan ortamlar ve sonuçlar

| Kontrol | Gerçek sonuç |
|---|---|
| Windows genel Rust regresyonu | 46 geçti, 7 bilinçli atlandı; yeni seçili Team yedeği testi sonraki hedefli koşuda geçti. |
| Windows Team Rust | 8 geçti (7 esas test + ayrı çökme sürecinin yardımcı giriş noktası); gerçek TLS PostgreSQL testi dahil. |
| Linux Ubuntu 22.04 chroot, Team Rust | 8 geçti (aynı yardımcı dahil), 0 hata; gerçek TLS PostgreSQL testi dahil. Son koşu 18,73 sn. |
| TypeScript / frontend | Tip kontrolü ve üretim Vite derlemesi geçti. 6 dosyada 21 Vitest testi geçti. |
| PostgreSQL SQL senaryoları | Hesap/token, doğrudan SQL/rol/GUC taklidi reddi, ACL/devir, host kapsamlı edit, eşzamanlı CAS, karar, geçmiş ve iptal edilmiş üyeden teslim geçti. |
| Windows native dosya-yedeği | Ek odaklı senaryo geçti (16,7 sn): dosyayı dahil et/açık bırak, eksik dosya raporu ve gerçek geçmiş ekranı. |
| Windows native Team | 5 senaryo geçti; 2 dk 21,8 sn. Gerçek DB kapatma/açma, SSH chain/SFTP, yetki iptali, karar, sürüm/taşıma, seçili yedek-import ve silinmiş kayıt geri yükleme dahil. |

PostgreSQL testleri WSL Ubuntu-24.04 üzerindeki **18/termterm, localhost:55432** kümesinde, yalnız `termterm_team_dev` / `termterm_team_e2e` veritabanlarında yapıldı. `18/main:5432`, kişisel kasalar ve üretim SSH sunucuları kullanılmadı. Bağlantı testleri disposable `127.0.0.1:22222` SSH laboratuvarına gitti.

Gerçek SQL testinde iki ayrı TLS istemcisi aynı başlangıç revizyonundan eşzamanlı düzenleme ve silme/düzenleme gönderir: biri uygulanır, diğeri karara gider. Token yenileme eski token'ı geçersiz kılar. Uygulama hesabının tablo okuması, executor rolüne geçmesi ve sahte SQL oturum değişkeniyle kimlik atlaması reddedilir.

Rust testleri şifreli kayıt/zarf kimliğini, veri-sır ayrımını, son 10 sürümü, revizyon anahtarlarının yeniden sarılmasını ve ortak terminalde iki gerçek hesabın anahtar paylaşımını/yazma kiralarını sınar. Yerel testler saat ilerletilmesi/geri alınması, toplu kayıt rollback'i, bağımlılık sırası, izin temizliği, yarıda kapanan SQLite transaction'ı ve yanlış parolada kaynakların değişmemesini içerir.

Native senaryolar gerçek Rust IPC + PostgreSQL + Windows uygulamasını kullanır. Kayıtların ve dosya sonuçlarının doğrulanmasına ek olarak Team ekranları ve karar butonları çalıştırılır. Embedded WebDriver'ın HTML `select` için yaptığı sentetik option tıklaması seçim değiştirmediğinden geçmiş testinde standart `change` olayı kullanılır; ardından gerçek UI düğmesinin DB geçmişini getirmesi ve göstermesi doğrulanır. Fiziksel fare/klavye donanımı testi sayılmaz.

## Yerel deneme paketi

Paket komutu: `pwsh -File scripts/package-team-dev.ps1`. `e2e` özelliği olmayan Windows x64 debug executable'ı, Mosh çalışma dosyaları, lisanslar, kullanım/doğrulama belgeleri ve önceden indirilmiş çevrimdışı WebView2 kurucusu içerir. Paket script'i kararlı uygulama kimliğini ve etkin updater adresini reddeder; normal bağımlılık ağacında test sürücüsünün bulunmadığını denetler. Geliştirme kimliği `local.termterm.desktop.teamdev`'dir.

Çıktı: `artifacts/team-0.4.0-dev.1/`. Özel profil/hesap bilgileri pakete veya Git'e konmaz; yerel `.lab/TEAM_DEMO_LOCAL.txt` dosyasında kalır. Kişisel kasa/yedek formatı değiştirilmedi.

## Sınırlar ve işletim davranışı

- **macOS/GitHub CI çalıştırılmadı.** Linux ortak servisleri test edildi; bu Team çalışmasında Linux native GUI veya yeni Linux kurucu paketi doğrulanmadı.
- Doğrudan FIDO2/Windows Hello, fiziksel seri aygıt, üretim SSH hesabı, harici bulut hesabı ve bütün üretici export çeşitleri bu Team doğrulamasının kapsamında test edilmedi. Önceki sürümün eksikleri tamamlanmış sayılmaz.
- Ortak terminal iki gerçek hesapla SQL/Rust taşıma katmanında ve native oturum sahibinin gerçek SSH yolunda denendi. İki ayrı bilgisayarda eşzamanlı native konuk arayüzü henüz doğrulanmadı.
- SMTP, e-posta doğrulama ve hesap parolası sıfırlama yoktur; plan gereği bu denemenin dışındadır. Bağımsız güvenlik denetimi/üretim sertifikasyonu yapılmadı.
- Import yerelde tek transaction'dır; PostgreSQL'e bağımlılık sıralı, kayıt bazında güvenli işlemler gönderilir. Bütün import tek uzak transaction değildir; bekleyen sayı kalan kayıtları gösterir.
- Yetki iptali öğrenilince ilgili kasanın bütün açık bağlantıları ihtiyatlı olarak kapanır. Çevrimdışı cihaz iptali sunucuya erişene kadar öğrenemez. Cihazdan ayrıca kopyalanmış bir SSH sırrı uzaktan silinemez.
- 90 günlük audit ve süresi dolmuş ortak terminal verileri Team politika ekranından sınırlı bakım çağrısıyla temizlenir; bağımsız PostgreSQL zamanlayıcı kurulmadı. Sabitlenmiş sürümler bitene kadar 10 sürüm sınırından muaftır.
- Uzun izleme/yük testi yapılmadı; kullanıcı tercihiyle kısa işlem, aç/kapat, kopma ve yeniden bağlanma senaryoları kullanıldı.

Kullanım ve tablo/migration düzeni: [TEAM_GUIDE_TR.md](TEAM_GUIDE_TR.md). Kaldığı yer ve sonraki adımlar: [TEAM-DEVELOPMENT-CHECKPOINT.md](TEAM-DEVELOPMENT-CHECKPOINT.md).
