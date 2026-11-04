# TermTerm — tüm proje durum ve eksik envanteri

5 Ekim 2026 · geliştirme dalı `codex/feature-database-workspace` · `0.5.0-dev.12`.

Bu rapor önceki istekleri, mevcut kaynakları ve sürüm doğrulama raporlarını karşılaştırır. **Yeni release, main birleştirmesi, tag, otomatik güncelleme veya macOS/CI yayını yapılmadı.** 0.4.0 kararlı işlevlerinin üzerine ayrı kimlikli SQL geliştirme uygulaması hazırlanır; 0.5 kararlı sürüm değildir.

“Çalışıyor” belirtilen kapsam ve test ortamıyla sınırlıdır. Eski bir raporda geçen test bu tur yeniden koşulmuş sayılmaz. Kod bulunması gerçek ortam testi sayılmaz. Her butonun her hata birleşimi için eksiksiz test veya bütün proje için tamamlanma yüzdesi iddia edilmez.

## Bu tur — dev.12

- Kişisel/Team SQL sorgu kitaplığı; ad, açıklama, klasör, düzenleme/silme ve 1–50 runbook adımı. Bir adım editöre yüklenir; otomatik yürütme yok.
- Belge ve bağlantı okuma kontrolü, güncel belge revizyonu, DB bağlantısına tek ilişki, bağımlı bağlantıyı silme engeli; yedek/copy ilişkileri. Kayıtlar Team genel geçmiş/karar mekanizmasındadır.
- Team cache v4, eski 2/3 okuma ve forward-only laboratuvar migration uygulaması. Üretim Team DB veya kararlı kanal değiştirilmez.

## Önceki adım — dev.11

- Team kasasında şifreli DB bağlantısı ve SQL çalıştır/izle/yönet/export izinlerinin ilk aşaması. Doğrudan üç motor bağlantısı, gömülü R0 kontrolü, native IPC/dosya export yetkisi ve yetki/bağlantı revizyonu denetimi.
- SSH izni ile SQL sırrı ayrıldı; yeni kayıt alıcı listeleri ve mevcut kayıt ACL iptalleri aynı tür denetimine bağlandı. Paylaşılan CA için public PEM; cihaz dosya yolu ve Team SQL SSH tüneli kapalı.
- İlk yükleme öncesi yeni kayıtların yanlışlıkla yetki iptali sayılıp SQL bağlantılarını kilitlemesi düzeltildi.

## Önceki adım — dev.10

- Üç motorda **Yeni satır → tipli/default/NULL alanlar → SQL önizlemesi → açık onay → tek INSERT**. Üretilen PK geri okunur; hassasiyet kaybı ve kısıt hatası rollback olur. Editör transaction'ı ayrı kalır. Trigger/özel tip/unique-only sınırları korunur; toplu işlem değildir.
- **Mevcut / yarım DB kurulumunu denetle**: DB/PDB, hesap/rol, üyelik ve izin kanıtlarını salt okunur gösterir. Aynı nesne adını önceki kurulumun kanıtı saymaz. Otomatik devam veya onarım henüz yoktur.
- **KeePass KDBX / KeePass 2 XML bağlantı importu**: parola/keyfile, iç içe klasörler ve SSH bilgileri; web/OTP/attachment/history ve çözümlenmeyen referanslar için kayıp raporu. KDBX4 sentetik testleri kullanıldı; gerçek üretici exportları ve KDBX3 doğrulaması bekler. [Ayrıntılı kapsam](IMPORT_SUPPORT.md).

Bugünkü sonuçlar [SQL doğrulama raporunda](DATABASE_TEST_RESULTS.md); eski raporlar tarihsel kanıt olarak kalır. Bu tur bütün proje tamamlanmadı. Team SQL’in kalan kapsamı/şirket onay servisi, 71 genel yönetim işlemi, gerçek PaaS ve donanım/üretici kontrolleri aşağıda açık kalır.

## Kasa, host ve terminal

| Alan | Bugün mevcut olan | Eksik veya sınır | Kanıt |
|---|---|---|---|
| Yerel kasa | SQLite kapsayıcı, Argon2id/XChaCha20-Poly1305, parola ile aç/kilitle, dosya kilidi, atomik kayıt; PostgreSQL olmadan çalışma | Her depolama/disk dolması/ani güç kesintisi çeşidi için kabul matrisi tamamlanmış değil | `vault.rs`, `crypto.rs`; [0.3](VALIDATION_0.3.md) |
| Hatırlama | Windows DPAPI, Linux Secret Service, macOS Keychain kodu; açılış/elle kilitleme/unutma ayrımı | Windows gerçek yeniden başlatma geçti; Linux gerçek masaüstü keyring ve macOS Keychain testi bekliyor | `remember.rs`; [0.3.4](VALIDATION_0.3.4.md) |
| Taşıma ve yedek | `.ttvault`, `.ttbackup`, portable copy, çoklu kasa, ilişki remap ve önizlemeli restore; Windows–Linux parola ile taşıma | Donanım sırları taşınmaz; Team hesap/ACL/token yedeği değildir. SQL metrik/şema önbellekleri backup'a girmez | `archive.rs`; [0.3](VALIDATION_0.3.md), [Team kılavuzu](TEAM_GUIDE_TR.md) |
| Hosts/Groups | İç içe klasör, devralma, klasör içinde hostlar, arama, kart/liste, taşıma/kopyalama, sağ tık, çoklu seçim; klasör tek/host çift tık ayrımı | Özel sistem çift tık sürelerinin tamamı denenmedi. 10.000 host hız sonucu test makinesine ait | `App.tsx`, `recordActions.ts`; [0.3.5](VALIDATION_0.3.5.md) |
| SSH ve chain | Gerçek parola/anahtar, çok adımlı jump, HTTP CONNECT/SOCKS5, known-host doğrulama, adım hataları | Her eski cipher/SSH ürünüyle evrensel uyumluluk yok; DSA/SSH1 desteği hedef değil | `connections.rs`, `ssh_compat.rs`; [0.3.2](VALIDATION_0.3.2.md) |
| SSH anahtarları | RSA 2048/3072/4096, Ed25519, ECDSA P256/384/521; şifreli/şifresiz PEM/PKCS/PPK v2/v3 laboratuvar testleri | FIDO2 ve Windows Hello/TPM doğrudan kayıt/imzalama adaptörleri **yok**. Agent/Pageant köprüsü bunların yerine geçmez | `keys.rs`, `ssh_legacy_pem.rs`, `ssh_compat_tests.rs`; [0.3.2](VALIDATION_0.3.2.md) |
| MFA / SSH sertifika | Keyboard-interactive ve sertifika bağlantı kodu | Ayrı MFA sunucusu/SSH CA ile kabul testi **yapılmadı** | `connections.rs`; [0.3](VALIDATION_0.3.md) |
| Diğer terminaller | Yerel ConPTY/Unix shell, Telnet, Mosh ve seri port kodu; yerel terminal/Mosh testleri | Fiziksel COM/seri cihaz testi yok. Her protokol için bütün hata/yeniden bağlanma varyantları kapsanmış değil | `connections.rs`, `platform.rs`; [0.3](VALIDATION_0.3.md) |
| Terminal kullanım | Üst sekmeler, xterm, split/16 panel, broadcast, workspace, paletler, arama/pano/kısayollar, terminal resize | Tüm Türkçe/AltGr/IME, gerçek tmux/Vim, fiziksel giriş ve ölçek/DPI kombinasyonları doğrulanmış değil | `TerminalPane.tsx`, `shortcuts.ts`; [0.3.3](VALIDATION_0.3.3.md), [0.3.6](VALIDATION_0.3.6.md) |
| Kaynak monitörü | CPU/RAM/disk; ağ Mbit/s, disk MiB/s; aygıt seçimi, kritik/dolu mount, stale/bilinmeyen; gizlide duraklama | Windows ve Linux gerçek ölçümleri mevcut. macOS kod/parser örnekleri var, gerçek Mac yok. Her uzak Windows/macOS SSH ölçüm yolu denenmedi | `metrics.rs`, `metrics_io.rs`; [0.3.7](VALIDATION_0.3.7.md) |
| Tünel/snippet | Local/remote/dynamic tünellerde gerçek trafik; snippet ve başlangıç komutları | Her ağ kesintisi/proxy/server bileşimi test edilmiş değil | `tunnels.rs`, `connections.rs`; [0.3](VALIDATION_0.3.md) |
| Log | Oturum kayıtları, arama/saklama ve not alanları | Zaman çizelgesinde zaman damgalı işaretleme arayüzü **yok** | `connections.rs`, `Editor.tsx`; [0.3](VALIDATION_0.3.md) |
| Parola üretici | Karakter sınıfları/uzunluk, son 50 şifreli yerel kayıt, tarih/saat, göster/kopyala/sil; kişisel ve Team hesap kapsamı | Team'e/DB'ye/backup exportuna otomatik dağıtılmaz; bu bilinçli yerel davranış | `passwords.rs`, `PasswordBoard.tsx`; [0.4.0](VALIDATION_0.4.0.md) |
| Türkçe arayüz | Giriş, menü, ayarlar, dosya ve SQL iş akışları Türkçe | JDBC/işletim sistemi/SSH teşhislerinin bazıları İngilizce; tüm hata metinlerinde tam yerelleştirme yok | `i18n.ts`; [0.3.4](VALIDATION_0.3.4.md) |

## SFTP, import ve entegrasyon

| Alan | Bugün mevcut olan | Eksik veya sınır | Kanıt |
|---|---|---|---|
| SFTP | Yerel PC dosyaları ve uzak panel, local–remote/remote–remote, kuyruk/retry, mkdir/rename/chmod/remove, hash kontrolleri | Bütün sunucu dosya sistemleri ve izin modelleri denenmedi | `Sftp.tsx`, `transfers.rs`; [0.3](VALIDATION_0.3.md) |
| Transfer kontrolü | Pause/resume/cancel, geçici hedef, kaynak değişiminde yeniden başlama, 30 sn ilerlemesiz süre sınırı | Gerçek kullanıcının harici editör akışı ve fiziksel masaüstü sürükle-bırak testi bekliyor; native drop kodu var | [0.3](VALIDATION_0.3.md) |
| CSV/OpenSSH/PuTTY/Ansible | Önizleme, alan eşleme, Türkçe kodlama, kopyala/güncelle/atla, anahtar/chain/tünel/grup ilişkileri ve kaynak hash testleri | OpenSSH exec komutları çalıştırılmaz; dynamic inventory/Jinja/Ansible Vault/group_vars/host_vars ve tüm vendor seçenekleri yok | [Import kapsamı](IMPORT_SUPPORT.md), `imports/*` |
| MobaXterm/SecureCRT | Düz oturum/klasör/anahtar yolu ve tanınan gateway/tünel alanları | Üretici şifre çözücüleri **yok**; gelişmiş positional/firewall alanları kısmi. Sürümü bilinen MobaXterm 26.5/SecureCRT 9.7.3 şifreli export kabulü bekliyor | [Import kapsamı](IMPORT_SUPPORT.md) |
| Gerçek MobaXterm/Termius verisi | Kullanıcının Moba dosyasında host/grup ilişkisi ve yeniden açma doğrulaması; önceki Termius dönüşüm çalışması | Bu özel dosyalar genel fixture değildir; kaynağın vermediği sır/anahtarlar aktarılamaz. Her Termius sürümünün tam kasa biçimi desteklenmiş sayılmaz | [0.3](VALIDATION_0.3.md); özel dosyalar Git dışında |
| KeePass | dev.10 KDBX parola/keyfile ve KeePass 2 XML; klasör/SSH kullanıcı/parola/not/etiket, kopyala/güncelle/atla | Tam parola kasası taşıması değildir; gerçek üretici exportu/KDBX3 doğrulanmadı. OTP/attachment/history, Windows hesabı, hardware challenge-response ve dinamik referanslar yok | `keepass_import.rs`, [Import kapsamı](IMPORT_SUPPORT.md) |
| Dosya export | TermTerm şifreli yedek, CSV/OpenSSH; sırların açık exportu tercihe bağlı | PuTTY/MobaXterm/SecureCRT özgün export yazıcıları yok; kayıpsız aktarım `.ttbackup` | `archive.rs`, `imports.rs` |
| Host keşfi | AWS EC2 CLI ve DigitalOcean API adaptörleri, önizleme/import | Gerçek hesap testi bekliyor; Azure VM keşfi yok. Bu özellik SQL bulut yönetim adaptörü değildir | `integrations.rs`, `Integrations.tsx` |
| API Bridge | Localhost, dar komut listesi ve kasa/Team kontrolü | Kurumsal uzaktan SQL yönetim/onay servisi değildir | `integrations.rs`; [0.4.0](VALIDATION_0.4.0.md) |

## PostgreSQL sync ve Team

| Alan | Bugün mevcut olan | Eksik veya sınır | Kanıt |
|---|---|---|---|
| Kişisel sync | Ayrı migration hesabı, şema kontrolü, ilk upload/download/merge önizleme, offline outbox, revision/tombstone/idempotency, TLS/RLS | Kişisel eski paylaşım modeliyle yeni Team hesap modeli farklıdır. Tek kasa aynı anda iki DB hedefi desteklemez; eski kişisel paylaşımda otomatik kasa anahtarı rotasyonu yok | `sync.rs`; [kişisel akış](POSTGRES_FLOW.md) |
| Team hesapları | Kayıt/giriş, bcrypt12, session/refresh hashleri, TLS, ayrı app/migration rolü, izinli RPC | SMTP/e-posta doğrulama/parola sıfırlama **yok**. Kurumsal SSO/LDAP/Entra entegrasyonu yok | `team/*`, `migrations/team/*`; [0.4.0](VALIDATION_0.4.0.md) |
| Team kaynak izinleri | Owner/roller, klasör/host ACL/deny/devir sınırı, yalnız izinli gezinme, native/DB denetimi, ayrı veri/sır zarfları, bağımlılık kullanımı | Üyelik iptali çevrimdışı cihaza anında ulaşamaz; daha önce dışarı kopyalanmış SSH sırrı geri alınamaz. SSH sunucusu parolasını otomatik döndürmez | [Team kılavuzu](TEAM_GUIDE_TR.md) |
| Offline ve karar | Şifreli `.ttteam`, 24 saat düzenleme sınırı, bağlantıya devam, sıralı yerel günlük, güncel yetki kontrolü, CAS/idempotency, yetkili karar kuyruğu | Sunucuya ulaşılmayan iş “DB'ye kaydedildi” sayılmaz. Her ağ/çok cihaz uzun süreli senaryo test edilmedi | [0.4.0](VALIDATION_0.4.0.md) |
| Geçmiş/audit | Güncel + önceki 10 revizyon, pin, geçici kullanım/güncele dönüş, kalıcı restore, 90 günlük sınırlı bakım | SSH'de yapılan işlemleri/SFTP içeriğini geri almaz. Pinned kayıtlar 10 sınırının üstünde yer tutabilir; bakım kullanıcı çağrılıdır | [Team kılavuzu](TEAM_GUIDE_TR.md) |
| Ortak terminal | Şifreli kısa ömürlü kareler, tek yazıcı ve devir, eski input reddi | İki ayrı fiziksel makinede native konuk UI kabulü eksik; SQL ortak editör değildir | `shared_terminal.rs`, `team/*`; [0.4.0](VALIDATION_0.4.0.md) |
| Team SQL | dev.12 sorgu/runbook kitaplığı; dev.11 ortak DB bağlantısı, ayrı run/monitor/manage/export, Rust + DB doğrulaması, online lease/ACL revizyonu, şifreli anahtar dağıtımı | Team SSH tüneli, offline SQL, otomatik runbook yürütme, Team metrik/şema geçmişi ve ikinci onay servisi yok; serbest SQL DB hesabının yetkileriyle çalışır | [Team SQL kapsamı](DATABASE_TEAM.md) |
| Şirket yürütme servisi | Tasarım kapsamı mevcut | **Sunucuda yönetim sırrı tutan ikinci onay/yürütme servisi yok.** Masaüstündeki onay kutusu bunun yerine geçmez | [SQL durum](DATABASE_WORKSPACE_STATUS.md) |

## SQL çalışma alanı — üç motor

Gerçek laboratuvar: **PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0**. Oracle ürün adındaki 26ai ile JDBC major 23 farklıdır. Windows ve WSL Java testleri vardır; bu bütün major/edition desteği demek değildir.

| Alan | Bugün mevcut olan | Kalan |
|---|---|---|
| Bağlantı/editör | Java21/JDBC paketli, kişisel kasada bağlantı, doğrudan/SSH tüneli, CA/kimlik, metadata, çoklu sonuç, script sınırları, transaction/iptal | Diğer major/edition/üretim ve bütün kesilme varyantları; Team SQL’in kalan kapsamı; Linux SQL native paket/arayüz doğrulaması |
| SQL editörü | Renklendirme, seçili/script çalıştırma, undo/scroll/sekme, tablo/sütun tamamlama, dosya/kayıt, sonuç/mesaj/plan | Tam IDE refactoring/debugger yok; Oracle canlı PL/SQL debugger hedeflenmedi |
| Veri gezgini | 100/500/1000 satır, tipli filtre/sort, güvenilir anahtarda keyset, metadata | Tüm sonuçlar aynı anın snapshot'ı değil; anahtarsız/özel tipli tablolarda sınırlı görüntü |
| Veri değişikliği | dev.4 tek satır UPDATE; dev.9 DELETE; **dev.10 INSERT**; önizleme/CAS/bağımsız transaction | **Toplu düzenleme yok.** Unique-only, view, geniş/LOB/JSON/floating/özel tablolar için düzenleme genişletilmeli; silmede trigger/cascade kapalı |
| Büyük değer/export | PK ile metin/binary 8 MiB'ye kadar tam okuma ve `.txt`/`.bin`, byte/hash | 8 MiB üstü streaming, arbitrary SQL LOB, JSON/XML/BFILE vb. yok. Kesilmiş SQL sonucu CSV'si yalnız görünen satırlar |
| 216 katalog | 142 R0 sözleşmesi; önceki testlerde **135 gerçek R0 geçti**; 3 bakım işlemi uygulanmış | **71 genel yönetim işlemi uygulanmadı. 7 R0 ortam koşullu:** 1 standby, 4 bulut, 2 lisanslı Oracle. SQL dosyası varlığı çalışır özellik sayılmaz |
| Bakım | PG ANALYZE, MS tek istatistik UPDATE STATISTICS, Oracle DBMS_STATS; onay/son kontrol | Diğer yönetim varyantları için komuta özel preflight, yetki ve gerçek son kontrol; otomatik KILL/DDL yok |
| DB/hesap kurulumu | Verilen PostgreSQL rol şablonu; MS yeni DB; Oracle mevcut PDB'de schema veya SYSDBA/OMF ile yeni PDB; 21 karakterli bağımsız parolalar bir kez gösterilir | dev.10 salt okunur mevcut/yarım kurulum kanıtı var; var olan hedefi uyarlama, yarım DDL sonrası güvenilir devam/temizlik, bütün arıza enjeksiyonları eksik. Tek sihirbaz 71 katalog varyantını tamamlamaz |
| PaaS extension | Provider/rol/allowlist kanıtı, sabit CREATE EXTENSION planı, mevcut admin bağlantısı, üyelik değişmemesi kontrolü | Gerçek Azure/AWS/Huawei ortamı bekliyor; geçici owner yükseltip grantor/options koruyarak geri alma **otomasyonu yok** |
| İzleme | Durum ayrımları, görünürken kontrollü yenileme, blocking ağacı, iki pencere yavaşlama, alan büyümesi, oturum/uzun transaction, tahmini plan | Standby/Query Store/eklentiler/yetkiler koşullu; Oracle19/RAC/licensed AWR/ASH doğrulanmadı. Otomatik tuning/sonlandırma yok |
| İndeks/yedek kanıtı | İndeks kullanım kapsamı; SQL full/diff/log+restore lab; Oracle RMAN/PG WAL bağlamı | PG tam yedek sağlayıcı bağlantısı, SQL Agent başarısız işler, Oracle dolu RMAN/restore kanıtı ve harici restore testi eksik |
| Metrik geçmişi | Şifreli ayrı önbellek, 7 gün/50 MiB, ham SQL/sonuç yok | Uygulama kapalıyken toplamaz; backup/sync yok; 5000 görünür örnek sınırı |
| Şema karşılaştırma | Şifreli 40 görüntü/48 MiB; tablo/sütun/indeks/PK/FK metadata farkları | Tam DDL, trigger/rutin/view SQL gövdeleri, CHECK ifadeleri ve eşzamanlı DDL snapshot garantisi yok; otomatik migration yok |
| Bulut DB adaptörleri | Katalog ve bazı sağlayıcı kanıt/koşulları | **AWS/Azure keşif/kimlik/izleme/yönetim uçtan uca adaptörleri tamamlanmadı**; gerçek cloud hesapları yok |

Ayrıntı: [SQL durum](DATABASE_WORKSPACE_STATUS.md), [216 satırın tek tek durumu](DATABASE_CATALOG_VALIDATION.md), [provisioning/PaaS](DATABASE_PROVISIONING.md), [test sonuçları](DATABASE_TEST_RESULTS.md). Katalog sonuçlarıyla GUI'deki bağımsız provisioning/silme özelliği sayıları toplanıp yapay başarı yüzdesi üretilmez.

## Paket, güncelleme ve doğrulama boşlukları

| Alan | Durum |
|---|---|
| Kararlı Windows/Linux | 0.4.0 raporunda NSIS/portable ve DEB/AppImage/RPM paket kanıtları var. Bu tur kararlı paket/feed değiştirilmedi; uzak GitHub'ın bugünkü durumunu yeniden kontrol ettiğimiz iddia edilmiyor |
| 0.5 geliştirme | Ayrı Windows uygulama kimliği; Java/JDBC ve Mosh/DLL/lisansları beraber, WebView2 önkoşulu. WSL Linux servis testleri, Linux native SQL kurucusu yerine geçmez |
| macOS | Kaynak/ayar/CI/Mosh hazırlığı var; kullanıcı isteğiyle derleme bekliyor. Çalışır `.app`/DMG teslimi ve gerçek test yok; notarization yok |
| Linux kapsamı | x64, Ubuntu22.04 tabanı, glibc2.34+/WebKitGTK4.1; DEB/AppImage GUI geçmiş testi var. RPM farklı distroda kurulum, Linux ARM ve Alpine/musl yok; “bütün Linux” iddiası doğru değil |
| Güncelleme | Settings/sol menü Updates; kullanıcı başlatır. Windows NSIS/AppImage imza denetimi geçmişte geçti; DEB/RPM elle paket yöneticisi, APT/YUM deposu yok. Geliştirme updater endpoint'i kapalı |
| Kod imzası | Updater kriptografik imzası ile Authenticode farklı. Windows Authenticode sertifikası yok; Mac noter/onay dağıtımı yok |
| Performans | 10.000 host/16 panel kısa kontrollerinin eski sonuçları var. Her cihazda <2 saniye sıcak açılış veya 30 dakikalık yük/bellek testi iddia edilmiyor. SQL frontend chunk >500 kB build uyarısı var; yeni başlangıç performans ölçümü yapılmadı |
| Güvenlik/arıza | Yetki/SQL/IPC/şifreleme için hedefli otomasyon var; bağımsız güvenlik denetimi/sertifikasyon yok. Fiziksel disk dolması, her noktada kill/commit cevabı kaybı ve bütün TLS/proxy/driver sürümleri kapsanmıyor |
| Erişilebilirlik | Klavye/kısayol çalışmaları var; ekran okuyucu, tüm odak döngüleri ve yüksek kontrast için bütün uygulama kabul matrisi yok |
| Özel veriler | `.lab`, gerçek host/export/anahtar/kasa ve imzalama sırları Git/paket dışında tutulmalı; kişisel arşivleri genel kaynakla paylaşmayın |

## Önerilen sıradaki işler

1. **SQL veri değişikliği genişletmesi:** unique-only ve özel tiplerin güvenilir anahtar/roundtrip testleri; tek satır kapsamı korunarak ilerleme.
2. **Provisioning kurtarma:** her aşamadan sonra bağlantı kesilmesi testi; sunucudaki gerçek duruma göre devam/elle düzeltme planı. Otomatik DROP ile eksik kurulumu temizlememek.
3. **Team SQL devamı:** dev.11 doğrudan bağlantı ve izinler tamamlandı; SSH/chain bağımlılık izni, metrik/şema önbelleği, çevrimdışı SQL ve SQL geçmiş sürümünü geçici kullanma açık kalır.
4. **Onay/yürütme servisi:** sunucuda sır, ayrı ikinci kişi, hedef+SQL+parametre hash'i; kişisel masaüstü onayından ayrı.
5. **KeePass üretici uyumluluğu:** gerçek KDBX3/4 ve farklı KDF/keyfile örnekleri; desteklenmeyen alanlar açık kalmalı. Temel KDBX4/XML bağlantı okuyucusu dev.10’da var.
6. **Kalan 71 yönetim işlemi:** yüksek faydalı/az riskli olanlardan, motor bazında preflight/apply/sonuç testleriyle; birden hepsini etkinleştirmemek.
7. **PaaS kabulü:** gerçek Azure/AWS/Huawei test hedefleri; owner/admin rol değişiminde döngü, grantor, ADMIN/INHERIT/SET ve kesin geri alma. Laboratuvar rol isimlerini gerçek sağlayıcı testi saymamak.
8. **Üretici import ve donanım:** sürümü bilinen şifreli exportlar, gerçek MFA/SSH CA, FIDO2/Hello/seri cihaz, harici editör/drop testleri.
9. **Platform teslimi:** Linux SQL native paket/test; RPM gerçek distro/keyring; kullanıcı istediğinde Mac runner/donanım. Authenticode/notarization ve APT/YUM ayrı işler.

AI, mobil uygulama, abonelik/faturalandırma, Vanta ve Termius SSH ID başlangıçta kapsam dışıydı; eksik uygulanmış özellik olarak sayılmadı. İleri restore/failover/HA orkestrasyonu da katalog planındaki dış kapsam olarak kalır. Kurumsal SSO istenirse ayrı kapsamlandırılmalıdır.
