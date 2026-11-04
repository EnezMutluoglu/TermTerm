# 0.5.0-dev.12 — SQL kitaplığı / Team belgeleri (2026-10-05)

Geliştirme dalı `codex/feature-database-workspace`; ayrı Windows uygulama kimliği. Release/main/tag/updater/CI/macOS yayını yapılmadı. Aşağıdaki sonuçlar yalnız belirtilen kapsam içindir.

## Bu tur doğrulananlar

- **65 Rust testi geçti, 8 ortam/yardımcı testi atlandı** (`.tools/dev12-rust-final.log`). Yeni belge sınırları, çalışma parametrelerini kaydetmeme, cache v4, şifreli yeniden açma, bağımlılık sırası, atomik toplu kayıt geri alma, eski revizyon reddi ve belge silmenin SQL bağlantısını iptal etmemesi; mevcut SSH anahtar/known-host, kasa dayanıklılığı, import/backup ve hatırlama regresyonu. Atlananlar topluca başarılı sayılmaz.
- Ayrıca **2 gerçek PostgreSQL TLS Team testi geçti** (`.tools/dev12-team-postgres.log`): belge okuma ile SQL çalıştırma ayrımı, veri anahtarları, tek bağlantı ilişkisi, yanlış/eksik ilişki reddi, yetkisiz export, kaynak yetkisi iptali, yeniden gönderimin tek uygulanması ve bağımlı bağlantıyı silememe. Mevcut şifreli Team/terminal/history testi de geçti.
- **52 TypeScript / 15 dosya geçti** (`.tools/dev12-unit.log`); frontend tip denetimi ve derleme başarılı. Mevcut büyük SQL chunk uyarısı (>500 KiB) sürüyor.
- **10 Windows native Team senaryosu geçti** (`.tools/dev12-native-final.log`, 2 dakika 33 saniye): PostgreSQL, SQL Server ve Oracle üzerinde kaydedilmiş Unicode/büyük decimal sorgusu ve ikinci runbook adımı gerçekten çalıştı. Üç bağlantı, üç sorgu, üç runbook ve klasör ilişkisinin şifreli yedeği/önizlemesi, hatalı belge reddi, kaydet–oturumu kapat–yeniden aç, runbook düğmeleriyle oluştur/sırala/editöre yükle/sil, hiçbir adımı otomatik çalıştırmama, salt okuyucu kitaplığı, izleyici native IPC denemeleri ve ikinci istemciyle SQL izni iptali doğrulandı. DOM/WebDriver etkileşimidir; fiziksel klavye/donanım testi değildir.
- **2 Windows native kişisel kitaplık senaryosu geçti** (`.tools/dev12-native-personal.log`, 5,5 saniye): erişilemeyen hedefle sunucusuz runbook oluşturma, SQL otomatik yürütmeme, şifreli kasa kapat/aç, kaynakta açık SQL bulunmaması, eksik bağlantı reddi, bağlı bağlantıyı silememe ve seçili yedek ilişkileri. `sql-library-personal.png` çalışan uygulamadan alındı.
- WSL **Team sunucu testi** (`.tools/dev12-wsl-team.log`) hesap/token, ACL/delegasyon, CAS/çakışma, geçmiş/pin ve iptal sonrası teslim/audit için geçti. Migration betiği 007 uygulanmış ve belge içeren DB'de tekrar çalıştırıldı; eski 006 tür kısıtını yeniden uygulamadı. Bu, Linux native SQL arayüz testi değildir.

## Bulunan ve giderilen sorun

İlk native koşu 6 başarılı / 3 başarısızdı. Runbook silinince sahibin belge üzerindeki genel SQL/connect izinleri, gerçek bağlantı yetkisinin iptali gibi yorumlanıyordu; uygulama SQL'i askıya alıyordu. Belge türleri bağlantı materyali iptalinden ayrıldı, Rust regresyonu eklendi; son 10 native senaryonun tamamı geçti. İlk PostgreSQL koşusu kapalı Windows–WSL relay nedeniyle erişemedi; laboratuvar relay/container'ları açıldıktan sonra canlı testler tekrar geçti. İlk başarısız koşular başarı diye sayılmadı.

## Sınırlar

Runbook, kullanıcının tek tek editöre yüklediği şifreli yönerge/adım belgesidir; otomatik yürütme, ikinci onay veya hedef DB hesabının haklarını azaltan SQL güvenlik duvarı değildir. Team SSH/chain tüneli, çevrimdışı SQL, SQL metrik/şema geçmişi, tarihsel SQL bağlantısı ve şirket onay servisi hâlâ açıktır. Bu tur 135 katalog kontrolü veya bütün yönetim/provisioning senaryoları tekrar koşulmadı. SQL Server/Oracle Team hedefleri yerel TLS-disabled lab profilleriydi; hedef TLS olumlu testi önceki turlardaki kanıttır. Team hesap sunucusu ve PostgreSQL hedefi TLS doğrulamalıdır. macOS, gerçek PaaS/üretim, donanım anahtarları ve Linux native UI test edilmedi.

---

# Database Workspace doğrulama raporu

## 2026-10-05 · 0.5.0-dev.11 Team SQL ilk aşaması

Yalnız geliştirme dalı ve ayrı uygulama kimliği kullanıldı. WSL `termterm_team_e2e` veritabanına 006 migration uygulandı; kullanıcının kasaları ve kararlı uygulaması değiştirilmedi. GitHub/CI/macOS/release çalıştırılmadı.

| Katman | Sonuç | Bu tur doğrulanan |
|---|---|---|
| Windows native | **7/7 geçti** | Team kasasından üç motorda gerçek sorgu, Unicode/büyük decimal; sunucuda kayıtlı profilin kullanılması; CSV yazma; şifreli üç bağlantı yedeği ve yeniden okuma; public CA PEM; yerel CA yolu/SSH tüneli reddi; yalnız izinli bağlantı/şifre gizleme; monitor için sorgu/yönetim/export/backup ve doğrudan Team RPC reddi; gömülü R0 kontrolü; ikinci istemciyle izin iptali ve boşta oturum kapanış olayı |
| PostgreSQL gerçek Rust testleri | **2/2 geçti** | Önceki şifreli ACL/geçmiş/ortak terminal protokolü + yeni SQL izinleri, yeni kayıt alıcı türü, fazladan zarf reddi, SSH izniyle SQL sırrı alınamaması, sahte kullanıcı kimliği, monitor/run/manage/export ayrımı, genel export ek izni, doğrudan tablo/helper SQL reddi, iptalde sır zarfının kaldırılması |
| WSL PostgreSQL regresyonu | **Geçti** | `tests/team_postgres.py`: hesap/token, SQL/RLS, devir sınırı, eşzamanlı edit/delete CAS, tekrar gönderim, karar/geçmiş/pin, yetkisi kaldırılan aday ve audit teslimi. Linux native SQL arayüz testi değildir |
| Rust genel regresyon | **62 geçti / 8 ignored** | Kasa/import/SSH anahtarları/remember/metrikler/Team önbelleği ve mevcut servis testleri. Ignored testler toplam başarıya eklenmedi; iki gerçek Team testi yukarıda ayrıca çalıştırıldı |
| Son değişiklik kontrolü | **1/1 geçti** | Son kind-aware sır temizliği ve v2→v3 cache, yanlış parola, şifreli günlük, yeni taslağın sahte revocation sayılmaması. Bu kontrol son export/önbellek değişikliklerinden sonra koşuldu |
| TypeScript | **50 test / 14 dosya geçti** | Yeni SQL izin ayrımı + mevcut terminal/host/SQL yardımcıları |
| Frontend / Rust derleme | **Geçti** | Native e2e derlemesi ve `cargo check`; SQL chunk >500 kB uyarısı sürüyor. Dağıtım için ayrıca test özelliği olmadan geliştirme derlemesi kullanılır |

Gerçek hedefler: PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0. Team hesabının PostgreSQL bağlantısı TLS doğrulamalıdır. Paylaşılan PostgreSQL profili public CA PEM ile doğrulandı; SQL Server/Oracle Team native sorgu profilleri yerel laboratuvar TLS kapalı modunu kullanır. Önceki TLS testleri bu tur Team üzerinde yeniden geçmiş sayılmaz.

Testte bulunan ürün hatası: yeni ve henüz yüklenmemiş Team kayıtları, sunucu ön kontrolünde erişim iptali sayılıyor ve SQL bağlantısını kilitli bırakıyordu. İptal değerlendirmesi artık sunucuda revizyonu bulunan kayıtlara uygulanır. SQL Server Unicode test sorgusuna `N` öneki eklendi; bu bir test sorgusu düzeltmesidir.

Kapsam sınırı: Team SQL doğrudan/çevrimiçi kullanımıdır. Team SSH tüneli, offline SQL, ortak kayıtlı sorgu/runbook, metrik/şema geçmişi ve zorunlu ikinci onay servisi kapalıdır. Yönetim izninin olumlu her işlemi Team üzerinden yeniden koşulmadı; mevcut kişisel SQL yönetim testleri ayrı tarihsel kanıttır. Serbest SQL hedef DB hesabının yetkileriyle çalışır; izin iptalinin hedef DB'de başlamış bir yazmayı geri aldığı iddia edilmez. Tüm alternatif hata birleşimlerinin, üretim/PaaS, diğer sürümlerin veya macOS'un kabulü değildir.

Kaynak testler: `tests/desktop/database-team.mjs`, `src-tauri/src/team/tests.rs`, `src-tauri/src/team/cache.rs`, `tests/database-team-permissions.test.ts`. Yerel kanıtlar: `.tools/dev11-native-team-sql-final.log`, `.tools/dev11-postgres-team.log`, `.tools/dev11-cache-final.log`, `.tools/dev11-rust.log`. Görüntüler: `artifacts/database-workspace/team-sql-owner.png`, `team-sql-monitor.png`. [İzin matrisi ve kurulum](DATABASE_TEAM.md).

## 2026-10-05 · 0.5.0-dev.10 ekleme, kurulum denetimi, KeePass

Bu tur Windows ve WSL laboratuvarları kullanıldı; 30 dakikalık izleme, macOS/CI veya yayın yapılmadı. Sürümler PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0. Kapsam dışındaki sürüm/edition ve gerçek üretim/PaaS ortamları geçti sayılmadı.

| Katman | Sonuç | Gerçek kapsam |
|---|---|---|
| Satır ekleme · Windows | 42 kontrol geçti | Her motorda 14: identity/default, Unicode/bind, büyük decimal/NULL, zorunlu alan/identity müdahalesi, constraint rollback, discard/replay/session/hash/renderer SQL, DDL conflict, editor transaction ayrımı, bounded lock, manuel PK/duplicate, trigger reddi, readonly INSERT reddi, kapalı session |
| Satır ekleme · WSL Linux | Aynı 42 kontrol geçti | Linux javac/Java ile aynı üç canlı motor; native Linux arayüz testi değil |
| Kurulum/denetim · Windows + WSL | Her ortamda 48 kontrol geçti | PG17/MS15/Oracle16: eski gerçek kurulum/RW/RO sınamaları + boş hedef, yalnız rol/login kalmış hedef ve tamamlanmış hedef kanıtları. Denetim sır döndürmüyor veya mevcut hedefi yeniden oluşturmuyor. Bu tur yeni PDB modu koşulmadı |
| Silme regresyonu · Windows | 39 kontrol geçti | Ortak INSERT/DELETE guard değişikliği sonrası üç motorun önceki silme kapsamı |
| Rust | 60 geçti, 7 ignored | Kasa/backup/Team cache kesinti kontrolleri, import, yerel Windows metrik/remember, bağımsız SSH key biçimleri ve gerçek loopback SSH handshake dahil. Ignored testler passed sayılmaz; bazıları alt süreç fixture girişidir |
| TypeScript | 47 / 13 dosya geçti | Mevcut host/kısayol/SQL tanı/şema/satır yardımcıları |
| Java birim | 18 script/bind + satır planı süre testi geçti | SQL motor sınırları ve monotonic süre |

`row-insertion-tests.json`, `linux-row-insertion-tests.json`, `provision-tests.json`, `linux-provision-tests.json`, `row-deletion-tests.json` sonuçları `artifacts/database-workspace` altında. Önceki dev.8 bölümündeki 53 sayısı yeni PDB dahil tarihsel koşudur; bugünkü provisioning dosyaları 48 kontrolü taşır.

KeePass Rust testleri sentetik KDBX4 parola+keyfile, yanlış parola/eksik keyfile/bozuk dosya, XML grup/Unicode/parola, korumalı/referanslı alan, DTD/derinlik/invalid port/duplicate reddini kapsar. Fixture üretim testi özel `.lab` altında dosya üretir; gerçek üretici exportu değildir. Kaynak hash eşitliği ve kalıcı kasada açık adres/parola bulunmaması native testte ayrıca kontrol edilir. KDBX3, donanım challenge-response ve Windows hesabı koruması doğrulanmadı.

Windows native son koşular: **13 senaryo geçti**. Üç motorun ekleme formu (3), KeePass worker/XML/kopyala-güncelle-atla/yeniden aç/iptal/kasa kilidi/gerçek import sihirbazı (5), üç motorun kurulum + mevcut nesne denetimi/PaaS paneli/kilitli IPC (5). `.tools/dev10-native-*-final.log` ve `dev10-native-keepass-wizard.log`. Gerçek WebView DOM olaylarıdır, fiziksel klavye/fare testi değildir.

İlk UI koşusundaki açılır liste olay üretimi testte düzeltildi. Ayrıca bağlantı açılırken kurulum formunun eski hedefle açılıp geç gelen bağlantıyla sıfırlanması gerçek UI hatasıydı: düğme bağlantı boyunca kapalı; dialog hedefi açıldığı anda sabitlenir. Son koşuda üç motorun tüm form/denetim adımları geçti.

Ayrıca gerçek WSL `termterm_team_e2e` üzerinde **Team şifreli ACL/geçmiş regresyonu 1 test geçti** (9,5 sn); SQL paylaşım desteği anlamına gelmez. Diğer ignore girişleri tekrar topluca geçirilmiş sayılmaz.

Görüntüler: `{postgresql,mssql,oracle}-row-insert-{preview,result}.png`, `{postgresql,mssql,oracle}-provision-inspection.png`, `keepass-{kdbx,xml}-import.png`, `keepass-wizard-preview.png`. KeePass ve PostgreSQL önizlemeleri görsel olarak incelendi. Ekranlar yalnız sentetik veridir; üretilen provisioning parolaları ekran görüntüsüne alınmadı.

Yerel paket doğrulaması `package-dev10-validation.json` dosyasında ayrı tutulur. Sadece geliştirme uygulaması; release/updater değildir. INSERT commit cevabı kaybı, her DDL adımında kopma ve gerçek cihaz/bulut matrisi tam olarak enjekte edilmedi.


## 2026-10-05 · 0.5.0-dev.9 tek satır silme ve bütün proje envanteri

| Alan | Sonuç | Kapsam |
|---|---|---|
| Windows Java | 39 kontrol geçti | Üç motorda 13'er kontrol: salt okunur önizleme, büyük PK/NULL/Unicode, hash/oturum/renderer SQL reddi, eşzamanlı edit/silme, discard, DDL değişimi, 5 sn kilit sınırı, cascade/trigger reddi, FK rollback, SELECT-only kullanıcı reddi, editör transaction ayrımı, gerçek silme/ikinci uygulama reddi, kapalı oturum |
| WSL Java | 39 kontrol geçti | Aynı üç motorda Linux runtime ve kaynak derlemesi; native Linux UI değildir |
| Windows native | 4 senaryo geçti, 18,8 sn | Üç motor UI önizleme/onay kutusu/çatışma/silme/yenileme ve kapalı oturum/kasa kilidi IPC reddi. Veritabanındaki satır sayısıyla doğrulandı; fiziksel fare yerine WebView DOM olayları |
| SQL regresyon | 30 kontrol geçti | Üç motor mevcut transaction/metadata/tipli parametre/sayfalama testleri |
| TypeScript | 47 test / 13 dosya geçti | Mevcut SQL ve ortak davranış birim kontrolleri; yeni Java silme kontrollerinin yerine geçmez |
| Genel inceleme | Kaynak ve geçmiş raporları karşılaştırıldı | [Tüm proje eksik envanteri](PROJECT_STATUS_2026-10-05.md); 0.3 STATUS/Import/kişisel sync belgelerine güncel kapsam ayrımı eklendi |

Kanıtlar: `artifacts/database-workspace/row-deletion-tests.json`, `linux-row-deletion-tests.json`, `dev2-tests.json`; `.tools/wdio-database-delete-dev9-fixed`, `.tools/dev9-unit.log`. Native ekranlar `{postgresql,mssql,oracle}-row-delete-{preview,result}.png`. Testler rastgele adlı disposable tablolarda çalışır; mevcut kullanıcı verisi silinmez, yeni test tabloları/hesapları finally ile temizlenir. İlk native koşu test dosyasının yanlış metin kodlaması nedeniyle Türkçe seçiciyi bulamadı; UTF-8 düzeltilince dört senaryo geçti. Unicode fixture UTF-8 ile okunur; SQL Server Unicode sabitine gereken N öneki eklendi ve açık değer karşılaştırmasıyla tekrar doğrulandı.

Üç motor ve sürüm kapsamı dev.8 ile aynı. Yeni native uygulama `databasedev.e2e` kimliğindedir; teslimde test sürücüsü olmayan `databasedev` derlemesi kullanılır. Kişisel SSH/SFTP/Team/135 katalog testleri bu tur tekrar koşulmadı; geçmiş kanıtları genel raporda sürümleriyle belirtilir. macOS/CI/release çalıştırılmadı; Linux native SQL paketi üretilmedi.

Silme sınırları: yalnız desteklenen PK'li tek tablo/tipler; trigger/rule/cascade yok; SQL Server VIEW DEFINITION ve Oracle DBA katalog görünürlüğü gerekir. Oracle sadece role bağlı DELETE yetkisi bu preflight'ta desteklenmez. Tablo kilidi kısa süre yazıcıları bekletebilir. Başarılı commit sonrası undo yoktur. Sunucu kapanışının tam commit sınırında hata enjeksiyonu, ağ kopması/disk dolması kombinasyonları ve bütün engine patch/edition'lar bu yeni silme yolunda doğrulanmadı. Kod `unknown_outcome` döndürür ve otomatik tekrar etmez; bunun varlığı tüm arıza matrisi geçti anlamına gelmez.

```powershell
pnpm db:prepare
node scripts/test-database-row-deletion.mjs
node scripts/test-database-dev2.mjs
pnpm test:unit
# E2E yapılandırmalı geliştirme build'i ardından:
$env:TERMTERM_SPEC='./tests/desktop/database-row-deletion.mjs'
pnpm test:native
```

Dev.9 paket kontrolünde önceki SQL geliştirme ZIP'inin Mosh çalışma dosyalarını içermediği görüldü. `package-database-dev.mjs` artık `mosh/` runtime, DLL ve lisanslarını kopyalar; eksik kaynakta paketleme hata verir. Doğrulayıcı paket içindeki `mosh-client.exe --version` komutunu çalıştırır. Bu kontrol gerçek yeni Mosh ağ oturumu testi değildir. Paketli Java ile üç motor SELECT bağlantısı, manifest, özel lab parolası taraması ve test frontend yokluğu ayrıca doğrulanır. Windows WebView2 önkoşulu devam eder.

## 2026-10-05 · 0.5.0-dev.8 DB/kullanıcı ve PaaS/extension akışı

| Alan | Sonuç | Kanıt ve kapsam |
|---|---|---|
| Windows Java kurulum | 53 kontrol geçti | `provision-tests.json`: PG 14 kontrol, MS 12, Oracle schema 13, Oracle yeni PDB 14 (biri root/OMF gözlemi). Gerçek yeni DB/PDB ve hesaplar; iki hesabın girişi, 21 karakterli bağımsız parolalar, CRUD, readonly yazma reddi, owner dışı DDL reddi, app'in yeniden provisioning yapamaması, hash/ek SQL/geçersiz ad/mevcut hedef/ikinci uygulama reddi. PG gelecek schema/sequence/function izinleri ve PUBLIC execute reddi, Oracle readonly FOR UPDATE reddi |
| WSL Linux Java kurulum | Aynı 53 kontrol geçti | `linux-provision-tests.json`; Java kaynakları Linux javac ile derlendi; native Linux arayüz testi değildir |
| PostgreSQL extension | Windows 10 + WSL 10 kontrol geçti | `extension-tests.json`, `linux-extension-tests.json`: read-only sağlayıcı kanıtları, metadata/SQL müdahalesi, hash/iptal/tekrar, gerçek trusted hstore kurulumu, untrusted file_fdw reddi/rollback, mevcut extension'ı değiştirmeme, yetki kapsamı değişikliği ve kapalı oturum. Üyelik/grantor/ADMIN/INHERIT/SET sonuçları önce/sonra eşit |
| Parola ve şablon birim kontrolleri | Geçti | `ProvisioningTest`: Windows ve Linux'ta 10.000 parola için 21 karakter/sınıflar/bağımsızlık; ad/SQL sınırları, sır içermeyen şablon ve beş dakika TTL. Rastgelelik için matematiksel kanıt değildir; gerçek SecureRandom kullanılır |
| Windows native | 5 senaryo geçti | `tests/desktop/database-provisioning.mjs`: üç motor gerçek UI kurulumu; yalnız önizlemede kapalı onay düğmesi, parola uzunluğu, kapatıp tekrar açınca parola kalmaması, PaaS metadata/extension önizleme ve kasa kilidinde IPC reddi. WebView DOM olayları kullanır; fiziksel fare/pano testi değildir |
| SQL regresyonu | 30 kontrol geçti | `dev2-tests.json`: gerçek üç motor transaction, metadata, tipli parametre ve sayfalama kontrolleri |
| TypeScript | 47 test / 13 dosya geçti | `.tools/dev8-unit.log`; mevcut SQL/izleme/import testleri |

Üç motor sürümü PG 18.3 / MS 2022 Developer 16.00.4295 / Oracle 26ai Free 23.26.3.0.0. PostgreSQL kurulum yöneticisi özellikle **NOSUPERUSER CREATEDB CREATEROLE** olarak oluşturuldu. Yeni owner/app hesaplarının sunucu yönetici bayrakları kapalı. Oracle schema testi SYSTEM; yeni PDB testi açık SYSDBA bağlantısıyla yapıldı. Turkish locale test kümesine eklendi; Oracle OMF yalnız disposable container'da bellek ayarıyla hazırlandı. Test DB/PDB/schema/rolleri temizlendi; mevcut kullanıcı kasaları değiştirilmedi.

Native ekran dosyaları: `{postgresql,mssql,oracle}-provision-preview.png`, `postgresql-paas-extensions.png`. Üretilen parolalar screenshot/test loguna alınmaz; hata screenshot'ı da `data-secret=password` alanlarını temizler. Native doğrulamada parola sadece tarayıcı içinde kontrol edilir. Kaynak paketine özel `.lab`/kasalar/kimlik bilgileri eklenmez.

Bu tur canlı Azure/AWS/Huawei, Oracle 19c/RAC, diğer motor major sürümleri, macOS, şirket onay servisi ve Team SQL doğrulanmadı. Rol/ayar adlarının okunması sağlayıcı ortam testinin yerine geçmez. Owner'a geçici bulut yönetici rolü verip geri alma otomasyonu açılmadı; mevcut yönetim bağlantısıyla extension kurulur. PaaS server hook/allowlist/preload/özel extension koşulları gerçek sağlayıcı ortamında ayrıca sınanmalıdır. Kurulum kesintisi/disk dolması/commit cevabı kaybının bütün varyantları bu tur kontrollü hata enjeksiyonuyla test edilmedi; DDL geri alma garantisi yoktur. İlk geliştirme denemelerinde PG non-superuser ownership, SQL Server collation sözdizimi ve Oracle DBA/SYSDBA ayrımı hataları bulunup giderildi.

Yeni DB/hesap sihirbazı katalogdaki bütün 71 genel yönetim satırının tamamlandığı anlamına gelmez. Önceki 135 kontrol, SSH/SFTP ve Team testleri bu tur yeniden koşulmuş sayılmaz. Ayrıntılı davranış ve sağlayıcı kaynakları: [DATABASE_PROVISIONING.md](DATABASE_PROVISIONING.md).

```powershell
pnpm db:prepare
& database-helper/runtime/bin/java.exe -cp 'database-helper/out;database-helper/lib/*' ProvisioningTest
$env:TEST_NEW_PDB='1' # yalnız hazırlanmış disposable Oracle CDB laboratuvarında
$env:TEST_LOCALE='tr_TR.UTF-8'
node scripts/test-database-provisioning.mjs
node scripts/test-database-extensions.mjs
$env:TERMTERM_SPEC='./tests/desktop/database-provisioning.mjs'
pnpm test:native # öncesinde database-e2e yapılandırmasıyla test derlemesi gerekir
```

## 2026-10-05 · 0.5.0-dev.7 önizlemeli istatistik bakımı

| Alan | Sonuç | Kanıt ve kapsam |
|---|---|---|
| Windows Java / üç motor | 43 kontrol geçti | `change-plan-tests.json`: gerçek metadata/istatistik seçimi, SQL ve baseline, hash/SQL/action değişikliği reddi, geçersiz hedef, oturum sınırı, gerçek istatistik güncelleme/son okuma, tek kullanımlık plan, eşzamanlı DDL, iptal/yeni plan, editör transaction ayrımı, SELECT-only kullanıcı, kaldırılmış yetki, kapalı oturum; Oracle stats lock reddi |
| Ubuntu WSL Java / üç motor | Aynı 43 kontrol geçti | `linux-change-plan-tests.json`; Linux native arayüz testi değildir |
| PostgreSQL bakım sırasında bağlantı kaybı | 3 kontrol geçti | `change-plan-disconnect-tests.json`: yalnız disposable tablonun lock bekleyen ANALYZE bağlantısı sonlandırıldı; unknown_outcome, planın tekrar uygulanamaması, veri satırının korunması |
| SQL/metadata regresyonu | 30 kontrol geçti | `dev2-tests.json`; mevcut editör/plan/sayfalama/parametre davranışı |
| TypeScript | 47 test / 13 dosya geçti | Mevcut SQL/izleme/import testleri; yeni bakımın gerçek davranışı Java/native testlerinde doğrulandı |
| Plan süresi | 3 Java assertion geçti | `ChangePlansTest`: monotonic beş dakika sınırı, sınır öncesi ve sayaç taşması; gerçek süre beklenmedi |
| Windows native | 4 senaryo geçti | `tests/desktop/database-maintenance.mjs`, son koşu `.tools/wdio-database-maintenance-dev7-context` / 32,5 saniye: Unicode/boşluklu tablolar, metadata seçimi, sunucu/DB/hesap bağlamı, önizleme, görünür onay kutusu, onay verilmeden kapalı düğme, gerçek bakım sonucu, eşzamanlı DDL reddi ve kasa kilidinde dört yeni IPC reddi |

Motorlar PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0. Test tabloları ve hesapları disposable laboratuvar kapsamındadır. Önceki Team/SSH/SFTP ve 135 katalog kontrolünün tamamı bu tur yeniden test edilmiş sayılmaz. Native senaryolar WebView DOM olayları kullanır; fiziksel klavye/fare testleri değildir.

SQL Server UPDATE STATISTICS, oluşturulan TABLOCKX senaryosunda kesinti için yakalanamadan tamamlandı. Bu bir kesinti testi başarısı sayılmaz; SQL Server ve Oracle bakımının bağlantı kaybı yolu bu tur canlı doğrulanmadı. Sürüm/edition matrisi, Oracle pending/incremental politikasının bütün varyantları ve üretim ortamı doğrulanmadı. Hatalı/boş hedef, DDL çakışması, kapalı oturum ve yetki reddi üç motorda doğrulandı. Plan süre testi sentetiktir.

İlk denemelerde bulunan iki uyumluluk düzeltmesi: Oracle DBMS_STATS GET_PREFS ve GATHER_TABLE_STATS çağrılarına tırnaklı tablo adı korunarak gönderilir; PostgreSQL ANALYZE INFO/NOTICE mesajları hata/uyarı olarak değerlendirilmez. Oracle locked stats otomatik açılmaz. Başarı performans artışı demek değildir; otomatik tekrar yoktur. İstatistik/SQL hedef bilgileri yalnız bellekte kalır, sync/backup'a yazılmaz.

Ekran görüntüleri: `{postgresql,mssql,oracle}-maintenance-preview.png` ve `*-maintenance-result.png`. Çalışma geliştirme dalındadır; release/main/tag/updater/macOS/CI başlatılmadı. 71 yönetim işlemi, provisioning ve Team SQL yetkileri açık kalır.

```powershell
pnpm db:prepare
node scripts/test-database-change-plans.mjs
node scripts/test-database-plan-disconnect.mjs
& database-helper/runtime/bin/java.exe -cp 'database-helper/out;database-helper/lib/*' ChangePlansTest
$env:TERMTERM_SPEC='./tests/desktop/database-maintenance.mjs'
pnpm test:native
node scripts/report-db-maintenance.mjs
node scripts/report-db-catalog.mjs
```

## 2026-10-05 · 0.5.0-dev.6 tam değer ve bağlantı/transaction incelemesi

| Alan | Sonuç | Kanıt ve kapsam |
|---|---|---|
| Windows Java / üç motor | 39 kontrol geçti | `values-sessions-tests.json`: motor başına 13 kontrol; bounded browse, tam Unicode CLOB ve SHA-256, tüm byte değerleri, NULL/boş, identifier/key reddi, değişmiş/silinmiş satır, 8 MiB aşımı, gerçek açık transaction/yaş/gruplar, editör transaction ayrımı, dar SELECT, kaldırılmış yetki, anahtarsız tablo ve kapalı oturum |
| Ubuntu WSL Java / üç motor | Aynı 39 kontrol geçti | `linux-values-sessions-tests.json`; Linux native arayüz testi değildir |
| SQL/metadata regresyonu | 30 kontrol geçti | `dev2-tests.json`; parametre, sayfalama, transaction, tahmini plan ve temel kontroller |
| TypeScript | 47 test / 13 dosya geçti | Literal UTF-8/çok baytlı kesilmiş önizleme, sınırlı hex önizleme, bilinmeyen/geçersiz transaction yaşı ve eşik; önceki testler |
| Windows native | 4 senaryo geçti | `.tools/wdio-database-values-dev6-verified`, 21,4 saniye; üç motorun tam değer/transaction ekranı, gerçek dosya servisi ve kilitli kasa/bozuk içerik reddi |

Native test betiği: `tests/desktop/database-values-sessions.mjs`. Üç motorda gerçek arayüzden tablo → tam değer açık isteği → SHA-256; gerçek export servisi → dosyadaki byte karşılaştırması; transaction paneli ve rollback; ayrıca binary/SQL dosya dönüşü, hatalı/çok büyük içerikte eski dosyanın korunması ve kilitli kasa reddi. Embedded WebDriver Windows dosya seçicisini kontrol edemiyor; WDIO global invoke mock'u uygulamanın ES-module çağrısını yakalamadığı için bu akış için kullanılamadı. Fiziksel dosya seçici/kaydet düğmesi uçtan uca geçti sayılmaz. Veritabanı ve dosya işlemleri mock edilmedi.

Yeni kanıt ekranları: `{postgresql,mssql,oracle}-full-value.png` ve `*-connection-transactions.png`. Motorlar PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0; kaynaklar izole laboratuvar tabloları/kullanıcılarıdır. Yetkisi kısıtlı kullanıcının görünür oturum sayısı tüm sunucu kapasitesi kabul edilmez. SELECT kaldırılınca yeni okuma reddedilir. İzleme açık editör transaction'ını commit etmez.

Sınırlar: 8 MiB, güvenilir primary key ve gerçek tablo; daha büyük streaming export, arbitrary sorgu sonucundan full LOB, JSON/XML/BFILE ve doğrudan iptal komutu yoktur. Team SQL, 74 yönetim işlemi, satır ekleme/silme, bulut ve diğer sürümler açık kalır. Önceki SSH/SFTP/135 katalog kontrolünün tamamı bu tur yeniden çalıştırılmış sayılmaz. Release/main/tag/updater/macOS/CI çalışması yoktur.

```powershell
node scripts/test-database-values-sessions.mjs
node scripts/test-database-dev2.mjs
pnpm test:unit
$env:TERMTERM_SPEC='./tests/desktop/database-values-sessions.mjs'
pnpm test:native
```

## 2026-10-05 · 0.5.0-dev.5 seçiciler, indeks ve yedek kanıtları

| Alan | Sonuç | Kanıt ve kapsam |
|---|---|---|
| Windows Java / üç motor | 27 kontrol geçti | `evidence-tests.json`: gerçek şema/tablo/sütun metadata; Unicode/virgül/literal wildcard; seçilen sütunlarla katalog yürütme; indeks kaynakları; ayrı transaction; yedek kaynakları; geçersiz işlem/nesne reddi; dar SELECT kullanıcısı ve kapalı oturum |
| Ubuntu WSL Java / üç motor | Aynı 27 kontrol geçti | `linux-evidence-tests.json`; Linux native arayüz testi değildir |
| SQL Server gerçek yedek/restore | 3 senaryo geçti | `backup-evidence-tests.json`: disposable DB'de checksum'lı full/diff/log ve sunucu yaşı; ayrı DB'ye gerçek restore + kayıt verisi/geçmiş; başarısız backup'ın başarıya dönüşmemesi. Oluşturulan DB'ler ve backup dosyaları test sonunda kaldırıldı |
| Temel JDBC regresyonu | 39 kontrol geçti | `helper-tests.json`: Unicode/NULL/decimal, bind, sonuç sınırı, transaction, iptal, yanlış parola/TLS ret |
| TypeScript | 44 test / 12 dosya geçti | Yeni 4 test: yaş/hasar/hata/bilinmeyen ayrımı, decimal/NULL, motora uygun identifier quoting |
| Windows native | 4 senaryo geçti | `tests/desktop/database-evidence.mjs` (`.tools/wdio-database-evidence-dev5-final`, 17 saniye): üç motor gerçek metadata seçimi → katalog sonucu, şema değişiminde bağımlı alan temizliği, indeks ve yedek ekranı, kasa kilidinde iki yeni IPC reddi |

Native testler WebView DOM olaylarıyla çalışır; fiziksel klavye/fare testi değildir. Ekran görüntüleri: `{postgresql,mssql,oracle}-metadata-picker.png`, `*-index-evidence.png`, `*-backup-evidence.png`. Kontroller motorların aynı sürümleriyle sınırlıdır: PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0.

Bulunan ek hata: Microsoft JDBC `uniqueidentifier` alanını character stream'e dönüştüremiyordu. Sonuç okuyucu GUID için `getString` kullanacak şekilde düzeltildi; gerçek RESTORE FILELISTONLY sonucu ve temel JDBC regresyonu geçti.

Oracle'da RMAN dolu/başarısız iş geçmişi, gerçek restore, nicel indeks yazma maliyeti ve izlemeye alınmış indeks kullanım geçmişi bu ortamda doğrulanmadı; mevcut boş kaynak ve dar yetki davranışı doğrulandı. PostgreSQL harici tam backup/restore kanıtı bağlı değil; WAL arşivleme başarıya eşitlenmez. SQL Agent hata geçmişi ve zincir bütünlüğü kapsam dışıdır. UI yaş değerlendirmesinin sentetik testleri gerçek Oracle RMAN backup denemesi sayılmaz. Yeni panellerde DDL/backup/restore komutu yoktur; bu komutlar yalnız izole test betiğinde çalıştırıldı.

Bu tur önceki 135 katalog kontrolünün, satır düzenlemenin veya SSH/SFTP/Team'in tamamı yeniden test edilmiş sayılmaz. Team SQL, 74 yönetim işlemi, tam LOB, bulut ve diğer motor sürümleri açık kalır. Release/main/tag/updater/macOS/CI çalışması yapılmadı.

```powershell
node scripts/test-database-evidence.mjs
node scripts/test-database-backup-evidence.mjs
pnpm test:unit
$env:TERMTERM_SPEC='./tests/desktop/database-evidence.mjs'
pnpm test:native
```

## 2026-10-05 · 0.5.0-dev.4 satır düzenleme

| Alan | Sonuç | Kanıt ve sınır |
|---|---|---|
| Windows Java / üç motor | 57 kontrol geçti | `row-editing-tests.json`: her motorda 19 senaryo; fresh read, exact decimal, Unicode, NULL/boş, tarih/timestamp, Oracle NUMBER, bind/injection reddi, plan tekrarı, iki istemci çakışması, silme/şema farkı, constraint rollback, 5 saniyelik lock wait, generated/PK koruması, editor transaction ayrımı, salt okunur hesap, kısaltılabilecek uzun anahtarın reddi |
| Ubuntu WSL Java / üç motor | Aynı 57 kontrol geçti | `linux-row-editing-tests.json`; Linux native UI doğrulaması değildir |
| Yazma sırasında bağlantı kaybı | 9 kontrol geçti | `row-edit-disconnect-tests.json`: her motorda yalnız test UPDATE bağlantısı kapatıldı; unknown_outcome, eski satırın korunması ve planın tekrar uygulanmaması |
| SQL/metadata regresyonu | 30 kontrol geçti | `dev2-tests.json`; mevcut parametre, sayfalama, transaction ve plan davranışı |
| TypeScript | 40 test / 11 dosya geçti | İki yeni tam değer/NULL/değişiklik çıkarma testi, önceki testler |
| Taslak süresi | Java testi geçti | `RowEditorTest`: monotonic saatle 5 dakika sınırı; gerçek süre beklenmedi |
| Windows native | 4 senaryo geçti | `.tools/wdio-database-row-dev4`; üç motorda önizleme → kaydet → başka istemci değişikliği → çakışma → taslağı geri al; DB gerçek değer kontrolü; kapalı oturum/kasa kilidinde IPC reddi |

Ekranlar: `{postgresql,mssql,oracle}-row-edit-preview.png` ve `*-row-edit-conflict.png`. Native test WebView DOM üzerinden çalışır; fiziksel klavye/fare testi sayılmaz. Oracle JDBC'nin DATE alanını TIMESTAMP metadata tipiyle sunması tarih-only girdide hata veriyordu; gece yarısına açık dönüşüm eklendi ve tarih/timestamp/NUMBER testi üç motorda yeniden geçti.

Ortamlar: PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle 26ai Free 23.26.3.0.0. Denemeler disposable lab tablolarında yapıldı. Mevcut satır UPDATE kapsamındadır; insert/delete, LOB/binary/JSON/floating, unique-index-only ve partition/inheritance düzenlemesi açıkça kapalıdır. Team SQL hâlâ etkin değildir. Sonuçlar eski 135 R0 katalog/SSH/SFTP testlerinin bu tur yeniden çalıştırıldığı anlamına gelmez. macOS, Linux native arayüz, diğer motor sürümleri ve üretim ortamları doğrulanmadı. Release/main/tag/CI/güncelleme yayını yoktur.

Tekrar çalıştırma:

```powershell
pnpm db:prepare
node scripts/test-database-row-editing.mjs
node scripts/test-database-row-edit-disconnect.mjs
& database-helper/runtime/bin/java.exe -cp "database-helper/out;database-helper/lib/*" RowEditorTest
$env:TERMTERM_SPEC='./tests/desktop/database-row-editing.mjs'
pnpm test:native
```

Native testten önce e2e özelliğiyle ayrı test kimliği derlenir; teslim paketi ardından `pnpm db:build` ile e2e olmadan yeniden derlenir.

## 2026-10-05 · 0.5.0-dev.3 ek doğrulama

Bu tur şema geçmişi, metadata sınırları ve SQL Server indeks okumasına odaklandı. Aşağıdaki eski dev.2 sonuçlarının tamamı yeniden çalıştırılmış sayılmaz.

| Alan | Bu turdaki sonuç | Kanıt |
|---|---|---|
| Windows Java / üç motor | 22 kontrol geçti | `schema-snapshot-tests.json`: Unicode ve literal `%/_`, boş/görünmeyen şema, sütun/indeks farkı, editör transaction'ının korunması, silinen nesne, 1000 metadata satırı / 200 nesne limitleri; SQL Server dar yetki/izin kaldırma ve INCLUDE sayfalama |
| Ubuntu WSL Java / üç motor | Aynı 22 kontrol geçti | `linux-schema-snapshot-tests.json`; Linux native arayüz testi değildir |
| Önceki SQL/metadata davranışı | 30 kontrol yeniden geçti | `dev2-tests.json`: aynı üç motor, sayfalama/parametre/transaction/plan regresyonu |
| TypeScript | 38 test geçti | 10 dosya; yeni 8 karşılaştırma testi: farklı hedef, eksik sürüm, belirsiz metadata, görünürlük, sıra ve istatistik değişimi |
| Şifreli şema deposu | 1 birleşik Rust testi geçti | Açık metin sızıntısı, bağlantı kapsamı, 40 görüntü kotasında atomiklik, silme ve bozuk şifreli içerik reddi |
| Windows native | 4 senaryo geçti | `tests/desktop/database-snapshots.mjs`: üç motorda gerçek ekranla yakalama/karşılaştırma, ekran değişimi, kasa kilidi/yeniden açma, eski oturum ve doğrudan yardımcı komut reddi |

Native ekranlar: `postgresql-schema-history.png`, `mssql-schema-history.png`, `oracle-schema-history.png`. Native test seçicileri WebView DOM change olaylarını kullanır; fiziksel fare/klavye testi değildir. İlk koşuda tarih seçicilerinin flex yüksekliği ve test sürücüsünün seçim olayı düzeltildi; son koşuda dört senaryo geçti (`.tools/wdio-database-schema-dev3-final-ui`, 26 saniye). Kısa Türkçe fark özetleri ve milisaniyeli tarih etiketleri son ekran testinde doğrulandı.

Bulunan gerçek SQL Server sorunu: JDBC indeks okuması dar SELECT kullanıcısında `VIEW DATABASE PERFORMANCE STATE` istiyordu. `sys.indexes` / `sys.index_columns` üzerinden sınırlandırılmış metadata okumasıyla düzeltildi; ek performans yetkisi verilmedi. INCLUDE, filtreli ve disabled indeksler güvenilir anahtar adayından ayrıldı.

Görüntüler **görünür metadata** kapsamındadır; eşzamanlı DDL'nin transaction snapshot'ı değildir. Şema/sürüm/kullanıcı/hedef uyuşmazlığında karşılaştırma reddedilir. Kısıt ifadelerinin tamamı, tetikleyici/rutin/görünüm SQL gövdeleri ve üreticiye özel tüm indeks özellikleri bu aşamada kapsanmaz. Sonuç verileri alınmaz. Yeni şema deposu ayrıca yerel şifrelidir; backup/sync dışında kalır.

Yeni test komutları:

```powershell
node scripts/test-database-snapshots.mjs
cargo test --manifest-path src-tauri/Cargo.toml --lib database_snapshots::tests
$env:TERMTERM_SPEC='./tests/desktop/database-snapshots.mjs'
pnpm test:native
```

Release/main/tag/güncelleme kaynağı değiştirilmedi; macOS veya CI başlatılmadı. Önceki SSH/SFTP/Team testleri bu tur tekrarlanmadı; yeni değişiklikler bu bağlantı servislerini değiştirmedi.

Yerel Windows dev.3 paketi: 174 dosyanın SHA-256 manifesti doğrulandı; özel lab parolası ve test frontend yok. Paketlenen Java ile üç gerçek DB bağlantısı geçti (`package-dev3-validation.json`).

## Önceki dev.2 doğrulaması (2026-10-04)

2026-10-04 · **0.5.0-dev.2** · yerel geliştirme. Release/CI/macOS yayını yok.

| Alan | Sonuç | Kanıt ve kapsam |
|---|---|---|
| Tipli canlı katalog | 135 R0 geçti | `dev2-catalog-validation.json`; 216 işlemden 74 yönetim + 7 ortam/lisans koşullu işlem açık |
| Üç motorda temel JDBC | 39 kontrol geçti | `helper-tests.json`: Unicode/NULL/decimal, bind, script, sınır, parola/TLS ret, commit/rollback, iptal |
| Dev.2 SQL/metadata | 30 kontrol geçti | `dev2-tests.json`: Unicode tablo, 2^53 üstü decimal anahtar, 3 sayfa/250 kayıt, filtre, identifier injection ret, metadata/editor ayrımı, PG aborted transaction, Oracle DDL implicit commit, plan, parametreler |
| Dar yetkili hesaplar + kilit | 12 kontrol geçti | `monitoring-tests.json`: her motorda ayrı monitor hesabı, SELECT izinli/UPDATE reddi, gerçek blokaj, eski iptal reddi, sunucuda iptal ve veri bütünlüğü |
| SQL Server/Oracle özel CA | 8 kontrol geçti | `tls-tests.json`: olumlu CA, yanlış CA, yanlış sunucu kimliği, loopback taşımasında mantıksal hedefin korunması. Relay testi kendi başına SSH kimlik doğrulama testi değildir |
| Bağlantı kesilmesi | 6 kontrol geçti | `disconnect-tests.json`: scriptin ilk INSERT'i commit olduktan sonra oturum kapatıldı; üç motorda unknown_outcome ve otomatik tekrar olmaması |
| Windows native arayüz | 5 senaryo geçti | Üç motor editör/sonuç, navigation, undo, üç izleme/metadata ekranı; iki hop SSH üzerinden PG TLS + SFTP dosya bütünlüğü; ayrı monitor transaction; kasa kilidi ve eski IPC reddi |
| TypeScript regresyon | 30 test geçti | 9 dosya: terminal resize/queue/kısayollar, host klasörleri, i18n, kayıt işlemleri ve yeni eşik/reset/kapasite/ağaç testleri |
| Java SQL parser | 18 test geçti | PostgreSQL dollar quote, SQL Server GO, Oracle PL/SQL/q-quote, yorumlar ve bind sınırları |
| Şifreli metrik önbelleği | 1 birleşik Rust testi geçti | Şifreleme/açık metin sızıntısı, 7 gün, saat geri alınması, boyut budaması; fiziksel dosya ≤50 MiB ve kalan örnekler doğrulandı |
| Yedek ilişkileri | 2 Rust testi geçti | DB→SSH/kimlik/grup ve savedQuery→DB seçili yedek, import kimlik eşlemesi |
| Ubuntu WSL Java servisleri | 9 kontrol geçti | Üç motorda bağlantı, Unicode bind ve metadata; `linux-helper-tests.json` |
| Windows geliştirme paketi | Geçti | 173 dosyanın SHA-256 manifesti; özel lab parolaları yok; test frontend yok; paketlenen Java runtime ile üç gerçek DB bağlantısı |
| Diğer motor sürümleri/native ortamlar | Doğrulanmadı | PostgreSQL 14–17, SQL Server 2019/2025, Oracle 19c/RAC, Linux native UI, macOS, bulut ve üretim ortamları |

Test motorları: PostgreSQL 18.3; SQL Server 2022 Developer 16.00.4295; Oracle 26ai Free 23.26.3.0.0. SQL'in çalışması ile sağlıklı sonuç farklıdır; boş sonuçlar `unknown` olarak kalır. Katalog raporu ham satırları/parametre değerlerini içermez.

Native test, CodeMirror DOM paste/klavye olayları kullanır; fiziksel klavye/işletim sistemi panosu testi sayılmaz. Uzun izleme yapılmadı. Native test logları `.tools/wdio-database-dev2-final`, ham raporlar ve üç motorun `*-editor.png`, `*-dev2-monitoring.png`, `*-dev2-metadata.png` görüntüleri `artifacts/database-workspace` altındadır. Ek native senaryoda yalnız test uygulamasının özel Java süreci durduruldu; yeni bağlantının açılması ve ulaşılamayan eski sekmenin kullanıcı kararıyla kapatılması doğrulandı.

## Tekrar çalıştırma

```powershell
pnpm db:prepare
node scripts/test-database-helper.mjs
node scripts/test-database-dev2.mjs
node scripts/test-database-actions.mjs
node scripts/test-database-monitoring.mjs
node scripts/test-database-disconnect.mjs
node scripts/test-database-tls.mjs
node scripts/report-db-catalog.mjs
pnpm test:unit
cargo test --manifest-path src-tauri/Cargo.toml --lib database_history::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib archive::tests
pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/tauri.database-e2e.conf.json
$env:TERMTERM_SPEC='./tests/desktop/database-workspace.mjs'
pnpm test:native
# Dağıtılacak yerel geliştirme paketi test erişimi olmadan yeniden derlenir:
pnpm db:build
node scripts/package-database-dev.mjs
```

Scriptler yalnız `.lab/database/profiles.json` içindeki izole test veritabanlarına yöneltilmelidir. TLS testi ek loopback TCPS relay ve `db-lab-tls.sh` hazırlığı ister. Parolalar/özel anahtarlar rapora, pakete veya Git'e eklenmez.
