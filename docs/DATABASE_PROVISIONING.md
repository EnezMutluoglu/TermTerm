# DB ve kullanıcı kurulumu — 0.5.0-dev.8

Veritabanları ekranında yönetim bağlantısını seçip **DB / kullanıcı oluştur** açılır. Bu geliştirme sürümü yayınlanmamıştır. PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295 ve Oracle 26ai Free 23.26.3.0.0 ile Windows ve WSL Java testleri yapılmıştır. PostgreSQL 14–17, diğer SQL Server/Oracle sürümleri, gerçek Azure/AWS/Huawei ve macOS doğrulanmamıştır; otomatik kurulum PG major 18, MS major 16 ve Oracle major 23 ile sınırlıdır. Başka patch/edition'ın aynı major'da olması doğrulama sayılmaz.

## Kullanıcı şablonu

Örnek ad `uygulama_api_dev`; küçük harf/rakam/alt çizgi, harfle başlayan en fazla 48 karakter. Oracle adları büyük harfe çevrilir, yeni PDB adı en fazla 30 karakterdir. Var olan DB, kullanıcı veya rol yeniden kullanılmaz/değiştirilmez.

| Son ek | PostgreSQL | SQL Server | Oracle |
|---|---|---|---|
| Yok | NOLOGIN NOINHERIT bağlantı grubu | DB bağlantı rolü | CREATE SESSION rolü |
| `_owner` | LOGIN, DB/schema sahibi | Login, DB sahibi; DB içinde dbo | Giriş yapabilen owner schema; sınırlı nesne oluşturma ve tablespace kotası |
| `_app` | NOLOGIN CRUD/rutin grubu | dbo schema CRUD/rutin rolü | Owner schema kapsamlı CRUD/rutin rolü |
| `_developer` | NOLOGIN CRUD/rutin grubu | dbo schema CRUD/rutin rolü | Owner schema kapsamlı CRUD/rutin rolü |
| `_readonly` | Tablo/sequence SELECT | dbo SELECT | Owner schema READ ANY TABLE; FOR UPDATE yok |
| `_appuser` | LOGIN, app üyesi | Login + DB user, app üyesi | Giriş kullanıcısı, app üyesi |

Developer grubu ayrı bir LOGIN oluşturmaz, DDL yetkisi almaz. DDL owner hesabıyla/rolüyle yapılır. Yeni oluşturulan owner ve app hesaplarına sunucu yöneticiliği verilmez. SQL Server owner'ın dbo eşlemesi normal DB owner yetkisini taşır.

PostgreSQL locale varsayılanı `tr_TR.UTF-8`, UTF8/template0/libc'dir. Locale sunucuda yoksa hata döner; C/başka locale'e otomatik dönülmez. Bu testte Türkçe locale WSL test ortamına eklendi ve test kümesi yeniden başlatıldı; ana PostgreSQL kümesi değiştirilmedi. Yönetici için CREATEDB + CREATEROLE yeterlidir; gerçek superuser şart değildir. Yöneticiye owner üyeliği (INHERIT/SET) verilir; owner'a yönetici rolü verilmez. DB önce bağlantıları kapalı oluşturulur; PUBLIC erişimi kaldırılıp grup izinleri verildikten sonra açılır. Public schema erişimi sınırlanır. Owner-scoped default privileges gelecekteki nesnelere uygulanır; nesneleri başka hesapla oluşturmak bu şablonu kullanmaz. PUBLIC'in fonksiyon EXECUTE/tip USAGE varsayılanları da kapatılır. App/developer sequence USAGE/SELECT/UPDATE; readonly yalnız SELECT alır.

SQL Server varsayılanı `Turkish_CI_AS`; collation sunucunun kataloğundan doğrulanır. Yeni dbo nesneleri schema grant'lerini devralır. Diğer schema'lara kendiliğinden erişim verilmez. Oracle 23+ schema-scoped grant'leri mevcut/gelecek owner nesnelerini kapsar. Oracle readonly rolüne sequence yetkisi verilmez: SELECT ANY SEQUENCE, NEXTVAL'i de mümkün kılar. PostgreSQL readonly sequence okuması birebir taklit edilmez.

## Oracle: schema veya yeni PDB

- **Mevcut PDB:** uygulamaya ait owner schema ve roller; ayrı DB oluşturulmuş sayılmaz. DBA yönetim bağlantısı ve mevcut çevrimiçi, kalıcı tablespace gerekir.
- **Yeni PDB:** aynı CDB kaynaklarını paylaşan yeni pluggable database. CDB$ROOT service, açıkça seçilmiş SYSDBA bağlantı rolü ve sunucuda OMF (`db_create_file_dest`) gerekir. Sadece DBA rolü yeterli değildir; eksiklik kurulum başlamadan reddedilir. Dosya yolu tahmin edilmez, uygulama sunucu ayarını otomatik değiştirmez.
- Yeni PDB için ayrıca `_pdbadmin` DBA hesabı oluşturulur ve **üçüncü parola** tek sefer gösterilir. Owner/app ayrı kalır. PDB OPEN/SAVE STATE ardından aynı schema şablonu uygulanır. Owner kota ve toplam PDB üst sınırı ayrı alanlardır; kaynak rezervasyonu değildir. Edition/lisans, toplam depolama ve PDB sayısı sınırları sunucuya göre geçerlidir. Oracle Free testi Enterprise/RAC doğrulaması değildir.

Laboratuvarda yalnız disposable Oracle container'ında OMF bellek ayarı hazırlandı. Yeni PDB'nin owner/app/readonly gerçek giriş ve yetkileri test edildi; test PDB'si ve dosyaları temizlendi. SYSDBA bağlantı ayarı, oluşturulan normal kullanıcıların giriş kontrolüne taşınmaz.

## Tek seferlik parolalar ve hata davranışı

İki bağımsız (yeni Oracle PDB'de üç) 21 karakterli parola, Java SecureRandom ile yalnız uygulanırken oluşturulur. Büyük/küçük harf, rakam ve `!#%+-_=` işaretlerinden en az birer karakter içerir. Kullanıcının verdiği örnek parolalar kullanılmaz. Önizlemede yalnız yer tutucular bulunur.

Parolalar işlem sonucu penceresinde açık gösterilir. Kopyalama kullanıcı seçimiyle olur; pano işletim sisteminin denetimindedir. Pencere kapanınca yeniden gösterilmez. Kasa, parola panosu, SQL metin geçmişi, metrik, backup veya uygulama loguna otomatik yazılmaz. Oluşturulan hedef otomatik bağlantı kaydı olarak eklenmez. Kaybolan parola yönetici tarafından sıfırlanmalıdır. Bu, JVM/işletim sistemi belleğinde adli incelemeye karşı sıfırlama garantisi değildir; sunucunun kendi audit/log politikası ayrı konudur.

Plan kimliği/SHA-256 hedefe, bağlantıya ve SQL'e bağlıdır; beş dakika ve tek uygulama denemesi için geçerlidir. Uygulamadan önce hedef/izin/locale tekrar doğrulanır. Paralel çift tıklama engellenir. Kurulum ayrı bağlantı kullanır; editör transaction'ını commit etmez.

CREATE DATABASE/PDB ve çok aşamalı hesap kurulumu tek atomik transaction değildir. Komut gönderildikten sonra hata/kopmada `unknown_outcome`, tamamlanan adım sayısı ve oluşturulmuş olabilecek hesapların parolaları gösterilir. Otomatik DROP/yeniden deneme yapılmaz. Tamamlanma bütün adımlar ve owner/app gerçek giriş kontrollerini gerektirir. Son cevap kaybolursa sonucun/parolaların kurtarılacağı vaat edilmez. Kapanışta bağlantılar temizlenir; yarım kurulum DBA tarafından kontrol edilir.

## PaaS / extension akışı

PostgreSQL kurulum penceresindeki **PaaS ve extension'lar** bölümü seçili DB'nin hesabını, sağlayıcı rol ipuçlarını, görünür allowlist/preload ayarlarını, kurulu/kullanılabilir sürümleri ve CREATE yetkili schema'ları okur. Rol adı bulunması sağlayıcı kimliğinin kanıtı sayılmaz.

- Azure: `azure_pg_admin`, `azure.extensions`, trusted extension koşulları. Görünür izin listesinde bulunmayan extension'ın planı reddedilir.
- AWS RDS: `rds_superuser` ve desteklenen sürümlerde sınırlı `rds_extension`; `rds.allowed_extensions` / `rds.allowed_delegated_extensions`. Delegasyon yetkisi ve provider hook'ları sunucuda uygulanır. RDS testi Aurora testi sayılmaz.
- Huawei RDS: root/root üyeliği, sürüme bağlı ayrıcalıklar. Azure rol adı veya eski Huawei `rds_hwdrs_privs` yaklaşımı tüm sürümlere kopyalanmaz.

Extension kurulumu seçilmiş **mevcut yönetim hesabıyla** yapılır. App/owner'a azure_pg_admin, rds_superuser veya root verilmez. Kullanıcının örneğindeki ters yönlü owner/yönetici üyelikleri döngü oluşturabileceğinden dört GRANT/REVOKE komutu körlemesine yürütülmez. Mevcut üyelik, grantor, ADMIN, INHERIT ve SET seçenekleri değişmeden kalır. **Geçici owner yükseltip geri alma otomasyonu bu sürümde açık değildir.** Gerçek sağlayıcı testi, eşzamanlı üyelik değişikliği kontrolü ve eski seçenekleri eksiksiz geri yükleme doğrulanmadan eklenmemelidir.

Mevcut yöneticiyle sabit CREATE EXTENSION planı uygulanabilir: kurulu sürüm değiştirilmez; varsayılan mevcut sürüm ve hedef schema katalogdan doğrulanır. Eksik bağımlılıkta CASCADE kullanılmaz. Plan sunucu/DB/kullanıcı, görünür ayarlar ve üyelik snapshot'ına bağlıdır; değişirse yeni önizleme gerekir. Ayrı transaction, 30 saniye statement ve 5 saniye lock sınırı, gerçek sürüm son kontrolü; hata rollback, commit cevabı kaybı unknown_outcome. Rol yükseltme, sağlayıcı parametre değiştirme veya restart yoktur. Extension içindeki dış sistem yan etkilerinin rollback edileceği garanti edilmez.

**Doğrulama sınırı:** Yerel PostgreSQL 18'de trusted extension başarı, untrusted yetki reddi/rollback ve tüm rol üyeliği seçeneklerinin değişmemesi test edildi. Azure/AWS/Huawei hesapları olmadan sağlayıcı akışları geçti sayılmadı. Sunucu politikası, extension'a özel preload/kurulum ve ek nesne grant'leri gerektiğinde DBA tarafından ayrıca tamamlanır.

## Kaynaklar

- [PostgreSQL rol üyelikleri](https://www.postgresql.org/docs/18/role-membership.html), [varsayılan nesne izinleri](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html).
- [SQL Server izin modeli](https://learn.microsoft.com/en-us/sql/relational-databases/security/authentication-access/getting-started-with-database-engine-permissions?view=sql-server-ver17).
- [Oracle CDB/PDB](https://docs.oracle.com/en/database/oracle/oracle-database/26/multi/introduction-to-the-multitenant-architecture.html), [PDB oluşturma](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/CREATE-PLUGGABLE-DATABASE.html), [schema yetkileri](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/GRANT.html).
- [Azure kullanıcı rolleri](https://learn.microsoft.com/en-us/azure/postgresql/security/security-manage-database-users), [extension izin listesi](https://learn.microsoft.com/en-us/azure/postgresql/extensions/how-to-create-extensions).
- [AWS RDS yönetici rolü](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Appendix.PostgreSQL.CommonDBATasks.Roles.rds_superuser.html), [delegated extension desteği](https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/RDS_delegated_ext.html).
- [Huawei root yetki sınırları](https://support.huaweicloud.com/intl/en-us/productdesc-rds-pg/rds_02_0012.html), [eski rds_hwdrs_privs sürüm sınırı](https://support.huaweicloud.com/intl/en-us/usermanual-rds-pg/rds_09_0068.html).

## 0.5.0-dev.10 — mevcut / yarım kurulum denetimi

DB/kullanıcı oluştur ekranındaki **Mevcut / yarım kurulumu denetle** seçeneği sadece okur. DB/PDB, hesap/rol bayrakları, üyelik/grantor bilgileri ve görünür ACL/default izinleri ile beklenen şablonu yan yana incelemek içindir. Her bölüm observed/missing/unknown taşır; boş sonuç veya bağlantı/yetki hatası başarı sayılmaz. Parola hashleri ve parolalar okunmaz.

Aynı isimde nesne bulunması TermTerm'in önceki kurulumuna ait olduğunu kanıtlamaz. Denetim otomatik DDL devamı, DROP, parola sıfırlama veya yetki değişikliği yapmaz. Kalıcı kurulum günlüğü ve güvenli devam/onarım planı henüz uygulanmadı. SQL Server için sysadmin metadata görünürlüğü, Oracle için ilgili DBA katalog erişimi gerekir; sonuçlar DBA incelemesine yöneliktir.
