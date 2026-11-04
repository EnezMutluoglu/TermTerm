# SSH + Database Operations Workspace
## Codex uygulama planı ve PostgreSQL / SQL Server / Oracle SQL kataloğu

**Belge sürümü:** 1.0 · **Hazırlanma tarihi:** 3 Ekim 2026 · **Dil:** Türkçe

**Amaç:** Var olan Termius benzeri SSH uygulamasını bozmadan; veritabanı gezgini, veri inceleme, SQL editörü, hazır DBA kontrolleri, güvenli kullanıcı/yetki oluşturma ve AWS/Azure yönetilen veritabanı entegrasyonuyla genişletmek. Üreticinin merkezi bulutu zorunlu değildir. Team Vault, müşterinin kendi altyapısında isteğe bağlıdır.

> **Codex için yürütme talimatı:** Bu belgeyi sadece öneri olarak özetleme. Önce mevcut depoyu incele; çalışan SSH/vault mimarisini koru. Bölüm 2'deki aşamalarla geliştirme görevlerine ayır. SQL kataloglarını motor adaptörlerine ve sürümlü action registry'ye dönüştür. Her aşamada çalışan kod, migration, otomatik test ve güncellenmiş dokümantasyon üret. Eksik özellikleri çalışıyormuş gibi gösteren sahte ekran veya başarılı yanıt döndüren stub bırakma. Desteklenmeyen özelliği açıkça işaretle.

> **Bu bir toplu çalıştırma scripti değildir.** SQL blokları bağımsız aksiyonlar veya sıralı provisioning adımlarıdır. `{{...}}` alanları güvenli renderer tarafından doldurulur; `:parametre` alanları sürücünün bind parametrelerine çevrilir. Aynı dosyanın tamamını bir SQL editörüne yapıştırıp çalıştırmayın. Bu ortamda canlı PostgreSQL, SQL Server veya Oracle sunucusu bulunmadığından sorgular gerçek sunucularda çalıştırılarak doğrulanmadı. Resmî dokümanlara dayalı şablonlar ve uygulanması zorunlu test planı sağlanmıştır. "Bütün sürümlerde hatasız" veya "bütün olası DBA SQL'leri" garantisi değildir; günlük operasyonlar ve kritik olaylar için kapsamlı bir başlangıç kataloğudur.

**Katalog özeti:** PostgreSQL **76**, SQL Server **69**, Oracle **71**; toplam **216 aksiyon** ve **62 resmî kaynak**. Cloud varyantları bu sayılara dahildir. SQL şablonlarına ek olarak geliştirme aşamaları, güvenli executor sözleşmesi, provisioning akışları, olay runbookları ve test kabul koşulları vardır.

### İçindekiler

1. [Ürün sınırı, güvenlik ve terminoloji](#bolum-1)
2. [Geliştirme sırası ve kabul koşulları](#bolum-2)
3. [Aksiyon motoru, SQL renderer ve sonuç sözleşmesi](#bolum-3)
4. [Provisioning standardı ve üç motorun sahiplik farkları](#bolum-4)
5. [PostgreSQL SQL kataloğu](#postgresql)
6. [Microsoft SQL Server / Azure SQL SQL kataloğu](#mssql)
7. [Oracle SQL kataloğu](#bolum-7)
8. [AWS ve Azure bağlantı / yönetim katmanı](#bolum-8)
9. [Health check kuralları ve olay runbook'ları](#bolum-9)
10. [Test matrisi, teslim koşulları ve Codex görevleri](#bolum-10)
11. [Aksiyon envanteri ve kaynaklar](#bolum-11)

<a id="bolum-1"></a>
# 1. Ürün sınırı ve değişmez kurallar

## 1.1 Hedef ekranlar

```text
Workspace
  SSH / SFTP / Jump Hosts / Tunnels           [mevcut özellikler korunur]
  Databases
    Connections / Favorites / Environment
    Explorer: DB → schema → table/view → columns/indexes/constraints
    Data Browser / SQL Editor / Explain / Export
    Health / Sessions / Locks / Storage / Replication / Backup
    Provisioning / Permissions / Maintenance
  Cloud: AWS / Azure                         [opsiyonel]
  Shared Runbooks / Team Vault               [müşterinin altyapısı]
  Audit / Approvals / Settings
```

SQL editörü ile DBA butonları aynı bağlantı altyapısını kullanabilir; **aynı aktif transaction/session'ı kullanmamalıdır**. Editörde açık transaction varken arka plan health check çalışması transaction'ı commit etmemeli veya lock süresini uzatmamalıdır. Monitoring, editör, migration ve tehlikeli bakım için ayrı session/pool sınıfları kullan.

## 1.2 Gizlilik ve güven sınırı

- Hedef DB parolaları, SSH anahtarları, IAM/Entra token'ları ve SQL sonuçları üretici sunucusuna gönderilmez. Cloud entegrasyonu yalnızca seçilen sağlayıcının resmî uçlarına erişir. Telemetri varsayılan kapalı; hata raporları sır ve veri temizliğinden geçer.
- Vault depolaması ile yönetilen hedef DB farklı kaynaklardır. Bir PostgreSQL Team Vault bağlantısı, otomatik olarak PostgreSQL admin bağlantısı sayılmaz. Vault için ayrı DB, role ve migration kimliği kullan.
- Standart, denetlenmiş şifreleme kütüphaneleri ve anahtar yönetimi kullan; kendi kripto algoritmanı yazma. TLS sertifika/hostname doğrulamasını kapatma. SSH tüneli arkasındaki DB'nin gerçek TLS hostname'ini koru; host-key değişimini sessiz kabul etme.
- **Doğrudan DB kimlik bilgisine sahip istemci kullanıcısı, uygulama dışından da DB'ye bağlanabilir.** UI'da butonu gizlemek veya client-side approval, DB seviyesinde güvenlik sınırı değildir. Zorunlu ekip onayı gerekiyorsa müşterinin altyapısında policy-enforcing execution service veya dar yetkili server-side API kullan; yönetim sırrını onay isteyen istemciye verme.
- Team Vault'tan kullanıcı çıkarmak, daha önce gördüğü/copied ettiği sırrı geri alamaz. Erişim kesme planı: üyeliği kaldır, gerekirse hedef sırları/anahtarları döndür, mevcut DB/SSH oturumlarını ayrıca değerlendir. Denetim kayıtları üretici bulutunda olmak zorunda değildir.

## 1.3 Güvenli varsayılanlar

| Risk | Tanım | Uygulama davranışı |
|---|---|---|
| R0 | Kontrol / metadata okuma | Sınırlı read-only bağlantı, timeout, satır limiti. Düşük risk sıfır maliyet demek değildir. |
| R1 | Kullanıcı/rol/grant ve nesne oluşturma gibi değişiklik | SQL önizleme, hedef doğrulama, diff, açık kullanıcı onayı; otomatik polling yok. |
| R2 | Ağır tarama, index bakımı, session cancel/terminate, servis ayarı | Ek maliyet/lock uyarısı, kapsam sınırı, değişiklik penceresi; prod approval politikası. |
| R3 | Slot/drop/truncate, restore/failover veya geri dönüşü zor veri kaybı riski | Adı yazdırarak onay, iki kişi politikası, güncel preflight, bağımlılık ve kurtarma planı. |

**İlk sürümde otomatik remediation yok.** Read-only sorgu sonuçları yeni write aksiyonu tetiklemez. `SELECT` ile başlayan bir ifade mutasyon yapan fonksiyon çağırabileceğinden, kelime kontrolü güvenlik değildir. Signed/built-in katalog, dialect-aware AST/allowlist ve DB izinleri birlikte kullanılır. Kullanıcının yazdığı serbest SQL ayrıca açık editör yetkisine tabidir.

Yetki yetersizliği `healthy=false` değil `permission_denied`; boş sonuç her zaman `healthy=true` değil. Sistem `ok / warning / critical / unknown / stale / unsupported / permission_denied / partial / failed` ayrımını yapar. NULL metrik, sıfıra dönüştürülmez.

## 1.4 Destek hedefi

Belgenin PostgreSQL tabanı 14–18; SQL Server tabanı 2019/2022/2025; Oracle tabanı 19c'dir. Bu, tüm varyantların test edildiği iddiası değil **uygulanacak test matrisi**dir. Daha yeni PostgreSQL major sürümleri otomatik desteklenmiş sayılmaz. Oracle 21c/23ai/26ai ve bulut sürümleri capability ve integration test sonrası etkinleşir. Oracle pazarlama sürümü, `COMPATIBLE` değeri ve sunucu major sürümünü ayrı sakla. AWS RDS/Aurora, Azure PostgreSQL, Azure SQL Database ve Azure SQL Managed Instance farklı platformlardır. [P01](#kaynak-p01) [M02](#kaynak-m02) [C07](#kaynak-c07)

<a id="bolum-2"></a>
# 2. Geliştirme sırası ve teslim kriterleri

| Aşama | Geliştirme | Kabul koşulu |
|---|---|---|
| 0 | Mevcut uygulamayı incele: stack, sürücüler, vault, testler, lisanslar | Mimari envanter ve ADR; geriye dönük uyumluluk planı; mevcut özellikler için regression test. |
| 1 | Mevcut SSH/SFTP/Vault'u sağlamlaştır; ortak connection ve secret API | Jump host zinciri ve tünel lifecycle testi; sırların loga çıkmadığını kanıtlayan test; migration backup/rollback. |
| 2 | PostgreSQL Explorer, data browser ve SQL editor | Metadata, sayfalama, iptal, transaction göstergesi, TLS, SSH tunnel ve çoklu tab çalışır. |
| 3 | PostgreSQL hazır kontroller / health engine | PG kataloğundaki R0 aksiyonlar; capability, görünürlük ve veri tazeliği doğru gösterilir. |
| 4 | PostgreSQL DB/owner/migrator/appuser/readonly oluşturma ve permission diff | Olumlu ve olumsuz yetki testleri; mevcut ve yeni nesnelerde grant doğrulaması; onaysız write yok. |
| 5 | SQL Server adaptörü ve kendi provisioning modeli | Login/user/schema/database owner ayrı; Azure SQL Database için contained user varyantı. |
| 6 | Oracle adaptörü ve kendi provisioning modeli | PDB bağlamı, schema-only owner, proxy migration, READ/SELECT ayrımı; pack lisans kontrolleri. |
| 7 | AWS discovery + PostgreSQL IAM + CloudWatch | Yalnızca seçilen hesap/region; pagination; control/data-plane izinleri ayrı; token sır olarak işlenir. |
| 8 | Azure discovery + Entra + Azure Monitor | Tenant/subscription seçimi, scoped token, PostgreSQL ve SQL Database/MI ayrımı. |
| 9 | Paylaşılan sorgular / runbook'lar / self-hosted Team Vault | Sürümlü yayın, parametre tipi, imza/hash, ACL, değişiklik diff'i; başka tenant verisi görünmez. |
| 10 | Kurumsal approval, güvenilir execution ve merkezi audit | Approval hedef+SQL+parametre hash'ine bağlı; değişince onay düşer; execution müşteri tarafında enforce edilir. |
| 11 | İsteğe bağlı AI destekli açıklama | AI önce salt-okur; kanıt gösterir; yeni SQL önerisini katalog aksiyonu sanmaz; açık onay olmadan çalıştırmaz. |

Aşama 10'un gelişmiş workflow'u sona bırakılabilir; ancak **temel yetkilendirme, audit, secret redaction ve güvenli SQL yürütme aşama 0–1'den itibaren zorunludur**. Her aşama için working branch/diff, test sonuçları, bilinen eksikler ve migration notu teslim et. Teknoloji yığını bu konuşmada bilinmiyor: mevcut repo uygunken sırf tercihen Electron/Tauri/React/.NET kullanmak için yeniden yazma.

## 2.1 Modül sınırları

```text
connection-core       -> identity, endpoint, TLS, tunnel, pool, cancellation
secret-provider       -> local vault / customer vault / cloud token
engine-adapters       -> postgresql / mssql / oracle
capability-service    -> engine + version + edition + platform + permissions
sql-renderer          -> bound values + safe identifiers + typed fragments
catalog-registry      -> versioned SQL/actions + metadata + result schemas
provisioning-planner  -> desired state -> diff -> preview -> execute -> verify
health-engine         -> evidence + thresholds + windows + freshness
cloud-adapters        -> discovery / auth / metrics / approved changes
policy-audit          -> authorization / approvals / redaction / audit sink
ui                    -> explorer/editor/actions; no embedded engine SQL
```

Driver'ları mevcut stack'e göre seç; sürümleri lockfile ile sabitle, dağıtım lisansını kontrol et. Oracle thick client/wallet gibi bağımlılıkları kullanıcının ortamında keşfet; lisansı doğrulanmadan ticari pakete gömme. CLI metakomutları (`\c`, `\gexec`, `GO`, SQL*Plus `/`) driver SQL'i değildir; session ve batch sınırları runner tarafından yönetilir.

<a id="bolum-3"></a>
# 3. Aksiyon motoru ve SQL sözleşmesi

## 3.1 Her aksiyonda zorunlu alanlar

Bu belgedeki her katalog girdisinin YAML metadata'sını registry'ye dönüştür. Metin olarak verilen `Hedef`, `Yetki`, `Girdi`, `Ön koşul` ve `Yorum` alanlarını gerçek schema/capability testleriyle tamamla; sadece açıklama olarak bırakma.

```yaml
id: PG-MON-007
engine: postgresql
catalog_version: 1
category: monitor
risk: R0
priority: P0
scope: cluster
cost: light
transaction: read_only
poll_seconds: 30
timeout_seconds: 10
max_rows: 500
max_result_bytes: 5242880
min_server_version_num: 140000
capabilities:
  all: [pg_stat_activity, can_observe_target_scope]
permissions:
  requested: [read_monitoring]
  visibility: instance_or_partial
parameters: {}
result_schema:
  pid: int32
  backend_start: timestamptz
  blockers: int32_array
redaction: [query_text, client_address]
preflight: verify_target_identity
postcheck: none
retry: transient_read_only_once
sources: [P05]
```

`priority=P0` kritik/günlük; `P1` sık kullanılır; `P2` uzman/isteğe bağlı. Risk ve öncelik bağımsızdır. Düşük riskli ama pahalı bir sorgu otomatik polling listesine girmez. Metadata'da `poll_seconds=0` yalnızca elle çalıştırma demektir; sıfır milisaniyelik döngü değildir.

## 3.2 Parametre ve identifier kuralları

| Gösterim | Anlam | Derleme |
|---|---|---|
| `:name` | SQL veri değeri | PostgreSQL `$1`, SQL Server `@name`, Oracle `:name`; sürücü bind'i. |
| `{{ident:name}}` | Tek SQL identifier | Motor quoting; PostgreSQL `"..."`, SQL Server `[...]`, Oracle `"..."`; uzunluk ve katalog kontrolü. |
| `{{qualified:table}}` | Ayrı schema+nesne bileşenleri | Her bileşeni ayrı quote et. `schema.table` metnini tek identifier gibi quote etme. |
| `{{literal:name}}` | DDL içinde bind kabul edilmeyen string | Dialect-aware literal serializer; kullanıcıdan raw SQL alınmaz. |
| `{{secret:name}}` | Parola DDL token'ı | Secret ref'ten son anda çöz; motorun parola sözdizimiyle serialize et; preview/audit/logda redakte et. |
| `{{int:name}}` | DDL içinde sayısal sabit | Bounded integer doğrulaması; boş, negatif veya aşırı değer reddedilir. |
| `{{enum:name}}` | Sabit whitelist SQL token'ı | Örn. collation/privilege; katalogdan doğrulanan seçeneğe map edilir. |
| `{{columns:selection}}` | Seçili kolon listesi | Metadata'dan seçilen typed AST; kullanıcı metni değildir. |

Parola bir identifier değildir. Özellikle Oracle `IDENTIFIED BY` için parola token'ı string literalden farklıdır; desteklenmeyen karakterleri şifre üretiminde önle, keyfi escaping varsayma. Parolalar SQL history, exception, crash dump, trace ve clipboard'a kendiliğinden yazılmaz. Parola DDL'i server tarafında loglanabilecekse motor/sürücü/audit davranışını ayrıca doğrula; yalnızca client redaction tam gizlilik garantisi değildir.

SQL'i `;` karakteriyle bölme: dollar-quoted PostgreSQL fonksiyonları, T-SQL batch'leri, Oracle PL/SQL blokları bozulur. Action adımları registry'de ayrı statement/batch olarak saklanır. Driver terminal `;` davranışı Oracle SQL ve PL/SQL için ayrı uygulanır. Template içinde bind adlarını metin replace ile değiştirme: literal, comment, `::type` ve JSON içeriği bozulmamalı.

## 3.3 Yürütme yaşam döngüsü

```text
select target -> identity handshake -> capability + permission probe
  -> typed parameter validation -> plan/diff -> SQL preview
  -> risk policy / approval -> reconnect or pin safe session
  -> preflight yeniden doğrulama -> execute -> postcheck
  -> redacted audit -> normalized result
```

Target fingerprint: endpoint + engine + DB/PDB + server identity + role/replica durumu + cloud resource ID. Aynı hostname başka ortama yönlendiğinde eski onay kullanılamaz. Oturum sonlandırmada PID/SPID/SID tek başına yeterli değil; başlangıç zamanı ve mümkünse request/serial/instance kimliğiyle yeniden kontrol et. Yeniden kontrol ile işlem arasında kalan yarış ihtimalini sıfırmış gibi sunma.

Write işlerinde network timeout = başarısız işlem varsayımı değildir. `unknown_outcome` kaydet, hedef durumu yeniden oku; otomatik CREATE/DROP retry yapma. PostgreSQL transactional DDL, autocommit gerektiren işlemler, SQL Server batch sınırları ve Oracle implicit commit ayrı ele alınır. Oracle DDL öncesindeki DML commit olabileceği için shared session kullanma. [P02](#kaynak-p02) [M01](#kaynak-m01) [O01](#kaynak-o01)

## 3.4 Explorer ve Data Browser gereksinimleri

Varsayılan ilk sayfa 100 satır, en fazla 1000; LOB/BLOB/CLOB, JSON ve büyük binary değerler lazy fetch. Tablo açarken `COUNT(*)` yapma. Kesin sayım ayrı ağır aksiyon; tahmini satır sayısına `estimated` etiketi koy. Üretim verisi export'u ayrı izin ister ve CSV formula injection'a karşı güvenli mod sunar.

PK/unique key varsa keyset pagination; yoksa kararlı sıralama garantisi olmadığını göster. Identifier, filtre ve ORDER BY AST üzerinden oluşur. Data edit ilk sürümde kapalı olabilir; açılırsa yalnızca PK/unique kimliği bulunan satırda optimistic concurrency (original value/version) kullan, affected_rows=1 zorunlu. Sıfır veya birden fazla satırda rollback. Sırasız `LIMIT/TOP/FETCH` ile gösterilen veriyi kesin sayfalama olarak sunma.

Plan ekranı varsayılan **estimated plan**. PostgreSQL `EXPLAIN ANALYZE` sorguyu çalıştırır; DML/yan etkili fonksiyonlar ve sequence/network dış etkileri için `ROLLBACK` bütün etkileri geri almaz. Kullanıcının SQL'i ayrı explicit-execution işlemidir. SQL metninde parola/kişisel veri olabileceğinden query text varsayılan kısaltılır, paylaşım/export ayrıca onaylanır.

## 3.5 Monitoring runtime

Her connection için tek scheduler ve jitter; varsayılan aktif ekran 30 saniye, görünmeyen ekran durdurulur. Cluster-wide aynı metrik her DB için yeniden toplanmaz. Ağır sorgular elle. Bağlantı başına başlangıç önerisi: 1 monitor session, en fazla 2 eşzamanlı hafif sorgu; kullanıcı ayarlayabilir. Bunlar ürün başlangıç değerleridir, evrensel performans eşikleri değildir.

Tarih UTC saklanır, UI kullanıcının saat diliminde gösterir. Sayaç hızları iki örnekten `delta / elapsed_seconds` ile hesaplanır; restart/stats_reset/negatif delta olduğunda baseline yenilenir. Toplamdan anlık CPU%/IOPS/TPS uydurma. İstemci kapalıyken toplanmayan zaman aralığı boş gösterilir; 7/24 alarm için müşterinin altyapısında isteğe bağlı collector gerekir.

<a id="bolum-4"></a>
# 4. Provisioning standardı

## 4.1 Ortak hedef model

```text
Object owner       -> nesne/sahiplik; runtime uygulaması değil
Migration identity -> DDL, kontrollü CI/CD; günlük uygulama bağlantısı değil
RW permission role -> yalnızca seçili alanlarda SELECT/INSERT/UPDATE/DELETE
RO permission role -> yalnızca seçili alanlarda okuma
App identity       -> RW rolüne üye; owner değil
Reader identity    -> RO rolüne üye; DDL, grant ve mutation yok
Monitor identity   -> sistem gözlemi; app/owner kimliğinden ayrı
```

"En iyi SQL" tek bir sabit değildir. Collation, locale, recovery modeli, tablespace, data/log dosya boyutu, bağlantı limiti ve timeout iş yüküne göre seçilir. Dosyadaki `30s`, `1GB`, kota vb. değerler örnek/policy girişidir; otomatik evrensel tuning uygulanmaz.

## 4.2 Motor farkları

| Kavram | PostgreSQL | SQL Server | Oracle |
|---|---|---|---|
| Bağlantı kimliği | LOGIN role | Server login + DB user veya contained user | DB/PDB user |
| Sahiplik | NOLOGIN role DB/schema/nesne sahibi olabilir | DB owner server principal; schema owner ayrı DB principal | User aynı zamanda schema sahibi; NO AUTHENTICATION mümkün |
| Migration | Owner'a `SET ROLE`; doğru object creator | Ayrı user ve kontrollü DDL rolü | Schema-only owner'a proxy session veya açık onaylı alternatif |
| Gelecek nesneler | **Nesneyi yaratan role** ait default privileges | Seçili schema'ya grant kapsamındaki gelecekteki nesneler | 19c: migration sonrası nesne grant reconciliation; yeni sürüm schema privileges ayrıca test edilir |
| Dar readonly | SELECT + schema USAGE; routine/sequence ayrı | Schema/nesne SELECT; geniş `db_datareader` varsayılan değil | Uygun tabloda READ; SELECT FOR UPDATE gibi haklardan ayrım |

PostgreSQL `ALTER DEFAULT PRIVILEGES` yalnızca ilgili DB'de ve belirtilen creator role için geçerlidir; mevcut nesneleri düzeltmez. SQL Server `db_owner` üyeliği ile veritabanının gerçek sahibi aynı kavram değildir. Oracle'da uygulama alanı oluşturmak çoğunlukla mevcut PDB içinde schema yaratmaktır; CDB/PDB yaratmayı her uygulama isteğinde çalıştırma. [P03](#kaynak-p03) [M03](#kaynak-m03) [M05](#kaynak-m05) [O01](#kaynak-o01) [O03](#kaynak-o03)

## 4.3 Planner ve idempotency

`discover -> validate -> desired-state diff -> SQL preview -> approve -> apply -> verify` akışını kullan. Mevcut DB/user/role bulunduğunda parola veya sahipliği sessiz değiştirme. `CREATE IF NOT EXISTS` var diye mevcut nesneyi doğru kabul etme. İsim eşleşip owner/encoding/collation/schema farklıysa `conflict` ver; adoption ayrı onaylı işlem olsun.

Her adımın `created / already_compliant / changed / conflict / failed / unknown_outcome` durumu kaydedilir. Sonraki adım başarısız olursa kullanıcı verisi olabilecek DB'yi otomatik DROP etme. Bağımsız adımların kısmi tamamlanmasını raporla. Role oluşturma ve üyelik değişimleri hedef motorda/sürümde gerekli yönetim yetkileriyle yürür; otomasyon kendisine yeni superuser/sysadmin/DBA hakkı vermez.

## 4.4 Kabul testi zorunlu

Yeni owner altında kontrollü test tablosu oluştur. App identity ile INSERT/SELECT/UPDATE/DELETE başarılı; DDL, grant, başka schema'ya erişim ve owner'a geçiş başarısız. Reader ile SELECT başarılı; INSERT/UPDATE/DELETE, routine üzerinden write ve DDL başarısız. Sonradan oluşturulan ikinci tabloda da aynı testler uygulanır. Bu testleri yalnızca disposable test DB/PDB/schema'da yap; production'a probe tablo bırakma.


<a id="postgresql"></a>
# 5. PostgreSQL SQL kataloğu

## 5.1 Rol standardı ve işlem sırası

Örnek isimler: DB=`orders`, schema=`app`, owner=`orders_owner`, migration login=`orders_migrator`, RW rol=`orders_readwrite`, RO rol=`orders_readonly`, runtime login=`orders_appuser`, reader login=`orders_reader`. Gerçek isimler UI'dan typed input olarak gelir. Runtime/migration sırları farklıdır.

Sıra: `PG-PROV-001 → 002 → 003 → 004 → 005 → 006 → 007 → 008 → 009`. İlk üç adım maintenance DB bağlantısında; DB CREATE autocommit; sonraki adımlar **oluşturulan DB'ye yeni bağlantıda** yürütülür. Planner her adımı keşif/diff ile idempotent hale getirir. Geliştirme ve üretim aynı nesne adlarını farklı connection fingerprint'leriyle kullanabilir; ortam doğrulaması zorunludur.

PG 16+ role membership seçeneklerini (`INHERIT`, `SET`, `ADMIN`) ayrıca keşfet. Migrator owner'a geçebilmeli, runtime ve reader geçememeli; üyelik zincirini yalnızca doğrudan üyelerden ibaret varsayma. `CREATEROLE` varlığı her sürümde bütün rollerin yönetimine izin verildiği anlamına gelmez. [P01](#kaynak-p01) [P04](#kaynak-p04)


<a id="pg-prov-001"></a>
### PG-PROV-001 — DB/rol çakışması ve oluşturma yetkisi keşfi

```yaml
id: PG-PROV-001
engine: postgresql
category: provision
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** requested_roles:text[]; db_name:text

```sql
SELECT current_user, rolsuper, rolcreatedb, rolcreaterole
FROM pg_roles WHERE rolname = current_user;

SELECT rolname, rolcanlogin, rolsuper, rolcreatedb, rolcreaterole,
       rolreplication, rolbypassrls
FROM pg_roles
WHERE rolname = ANY(:requested_roles::text[]);

SELECT datname, pg_get_userbyid(datdba) AS owner,
       pg_encoding_to_char(encoding) AS encoding,
       datcollate, datctype, datallowconn
FROM pg_database WHERE datname = :db_name;
```

**Yorum / kullanım:** İsim çakışmasını ve mevcut ayarları gösterir. Sonuç varsa kör CREATE yerine compliance karşılaştırması yap.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P01](#kaynak-p01) [P02](#kaynak-p02)


<a id="pg-prov-002"></a>
### PG-PROV-002 — Owner, izin rolleri ve bağlantı kimlikleri oluşturma

```yaml
id: PG-PROV-002
engine: postgresql
category: provision
priority: P0
risk: R1
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: transactional
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** CREATE ROLE ve ilgili üyelikleri verebilme; PG sürümüne göre admin/set seçenekleri

**Girdi:** owner_role,rw_role,ro_role,migrator_user,app_user,reader_user:identifier; üç secret ref

```sql
CREATE ROLE {{ident:owner_role}} NOLOGIN NOINHERIT
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
CREATE ROLE {{ident:rw_role}} NOLOGIN
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
CREATE ROLE {{ident:ro_role}} NOLOGIN
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;

CREATE ROLE {{ident:migrator_user}} LOGIN NOINHERIT
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
  PASSWORD {{secret:migrator_password}};
CREATE ROLE {{ident:app_user}} LOGIN INHERIT
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
  PASSWORD {{secret:app_password}};
CREATE ROLE {{ident:reader_user}} LOGIN INHERIT
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
  PASSWORD {{secret:reader_password}};

GRANT {{ident:owner_role}} TO {{ident:migrator_user}};
GRANT {{ident:rw_role}} TO {{ident:app_user}};
GRANT {{ident:ro_role}} TO {{ident:reader_user}};
```

**Yorum / kullanım:** Uygulama ve readonly kimliğe ownership, CREATEDB, CREATEROLE veya BYPASSRLS verilmez. LOGIN hesapları için SCRAM/TLS politikası bağlantı öncesinde doğrulanır.

**Ön koşul / güvenlik:** Roller yalnızca yoksa oluşturulur. Mevcut parolalar yenilenmez. İnsan readonly hesapları tercihen kişi bazlıdır. Yeni owner rolüne provisioning bağlantısının SET ROLE yetkisi gerekiyorsa bunu ayrı, geçici ve onaylı adım olarak yönet.

**Kaynak:** [P01](#kaynak-p01) [P04](#kaynak-p04)


<a id="pg-prov-003"></a>
### PG-PROV-003 — Doğru owner ile yeni database oluşturma

```yaml
id: PG-PROV-003
engine: postgresql
category: provision
priority: P0
risk: R1
scope: maintenance_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** CREATEDB + hedef owner rolüne geçebilme veya superuser

**Girdi:** db_name,owner_role:identifier; lc_collate,lc_ctype:validated locale; locale_provider_clause: PG14 için boş, PG15+ için yalnız LOCALE_PROVIDER = libc

```sql
CREATE DATABASE {{ident:db_name}}
  OWNER {{ident:owner_role}}
  TEMPLATE template0
  {{enum:locale_provider_clause}}
  ENCODING 'UTF8'
  LC_COLLATE {{literal:lc_collate}}
  LC_CTYPE {{literal:lc_ctype}};
```

**Yorum / kullanım:** libc locale şablonudur; PG15+ sağlayıcı açık seçilir, template0 varsayılanına güvenilmez. Türkçe karşılaştırma/encoding kararını kullanıcıya bırak. ICU/builtin locale sağlayıcıları için ayrı sürüm kontrollü şablon üret; bu şablona ekleyip evrensel kabul etme.

**Ön koşul / güvenlik:** CREATE DATABASE transaction içinde çalışmaz. OS/managed platformda locale varlığını doğrula. Var olan DB encoding/collation ayarını sessiz değiştirme. Yeni DB kullanıcı ağına açılmadan provisioning tamamlanmalı.

**Kaynak:** [P02](#kaynak-p02)


<a id="pg-prov-004"></a>
### PG-PROV-004 — Yeni DB erişimini daraltma ve uygulama schema oluşturma

```yaml
id: PG-PROV-004
engine: postgresql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: transactional
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** DB owner/schema owner veya yetkili provisioning hesabı

**Girdi:** db_name,app_schema ve rol adları:identifier

```sql
REVOKE ALL ON DATABASE {{ident:db_name}} FROM PUBLIC;
GRANT CONNECT ON DATABASE {{ident:db_name}}
  TO {{ident:owner_role}}, {{ident:migrator_user}},
     {{ident:rw_role}}, {{ident:ro_role}};

REVOKE CREATE ON SCHEMA public FROM PUBLIC;
CREATE SCHEMA {{ident:app_schema}} AUTHORIZATION {{ident:owner_role}};
REVOKE ALL ON SCHEMA {{ident:app_schema}} FROM PUBLIC;
GRANT USAGE ON SCHEMA {{ident:app_schema}}
  TO {{ident:rw_role}}, {{ident:ro_role}};
```

**Yorum / kullanım:** Sadece greenfield DB için varsayılan sertleştirme. TEMPORARY kaldırılır; ihtiyaç varsa ayrıca grant edilir. PostgreSQL 15+ ile public schema varsayılanları eski/upgraded DBlerden farklı olabilir.

**Ön koşul / güvenlik:** Mevcut DB üzerinde PUBLIC revoke mevcut uygulamaları etkileyebilir; önce effective privilege/bağımlılık diff. Komut hedef DB bağlantısında olmalı. schema zaten varsa sahibi kontrol edilmeden kabul edilmez.

**Kaynak:** [P12](#kaynak-p12) [P04](#kaynak-p04)


<a id="pg-prov-005"></a>
### PG-PROV-005 — Mevcut nesneler için RW ve readonly izinleri

```yaml
id: PG-PROV-005
engine: postgresql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: transactional
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Nesne owner veya grant option

**Girdi:** app_schema,rw_role,ro_role:identifier

```sql
GRANT SELECT ON ALL TABLES IN SCHEMA {{ident:app_schema}}
  TO {{ident:ro_role}};
GRANT SELECT, INSERT, UPDATE, DELETE
  ON ALL TABLES IN SCHEMA {{ident:app_schema}} TO {{ident:rw_role}};
GRANT USAGE ON ALL SEQUENCES IN SCHEMA {{ident:app_schema}}
  TO {{ident:rw_role}};

REVOKE EXECUTE ON ALL ROUTINES IN SCHEMA {{ident:app_schema}}
  FROM PUBLIC;
```

**Yorum / kullanım:** Şema bütünü seçildiğinde kullanılır; içinde hassas tablolar varsa ALL TABLES yerine metadata allowlistinden nesne bazlı GRANT üret. RW rolü TRUNCATE, REFERENCES, TRIGGER, CREATE veya sequence setval yetkisi almaz.

**Ön koşul / güvenlik:** Routine EXECUTE explicit allowlisttir; readonly kullanıcıya bütün fonksiyonları açmak write yapan SECURITY DEFINER fonksiyonuna erişim verebilir. RLS politikaları ayrıca geçerlidir.

**Kaynak:** [P04](#kaynak-p04) [P12](#kaynak-p12)


<a id="pg-prov-006"></a>
### PG-PROV-006 — Gelecek nesnelerin owner bazlı default privileges ayarı

```yaml
id: PG-PROV-006
engine: postgresql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: transactional
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** owner_role default privileges değiştirebilme

**Girdi:** owner_role,app_schema,rw_role,ro_role:identifier

```sql
ALTER DEFAULT PRIVILEGES FOR ROLE {{ident:owner_role}}
  IN SCHEMA {{ident:app_schema}}
  GRANT SELECT ON TABLES TO {{ident:ro_role}};
ALTER DEFAULT PRIVILEGES FOR ROLE {{ident:owner_role}}
  IN SCHEMA {{ident:app_schema}}
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO {{ident:rw_role}};
ALTER DEFAULT PRIVILEGES FOR ROLE {{ident:owner_role}}
  IN SCHEMA {{ident:app_schema}}
  GRANT USAGE ON SEQUENCES TO {{ident:rw_role}};

-- Global-in-this-database revoke: IN SCHEMA ekleme.
ALTER DEFAULT PRIVILEGES FOR ROLE {{ident:owner_role}}
  REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC;
```

**Yorum / kullanım:** Gelecek nesneler gerçekten owner_role olarak yaratılmalı. Function PUBLIC EXECUTE varsayılanı DB içi global default seviyesinde kapatılır; yalnız schema seviyesinde revoke global default grantini geri alamaz.

**Ön koşul / güvenlik:** Başka creator rolleri varsa her biri için ayrı plan gerekir. Migration login kendi adıyla CREATE yaparsa bu owner defaultları uygulanmaz. Diğer şemalardaki owner routine defaultlarını etkileyebileceği için plan açıkça gösterilir.

**Kaynak:** [P03](#kaynak-p03)


<a id="pg-prov-007"></a>
### PG-PROV-007 — Migration oturumunu doğru nesne sahibiyle yürütme

```yaml
id: PG-PROV-007
engine: postgresql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: transactional
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** migrator_user owner_role için SET ROLE yapabilmeli

**Girdi:** owner_role,app_schema:identifier; migration batch ayrı doğrulanır

```sql
BEGIN;
SET LOCAL ROLE {{ident:owner_role}};
SET LOCAL search_path = pg_catalog, {{ident:app_schema}};
-- Onaylanmış migration statement'ları ayrı batch olarak burada yürütülür.
-- Nesne adları schema-qualified olmalıdır.
COMMIT;
```

**Yorum / kullanım:** Migration runner oturumunda DDL object owner tutarlılığı sağlar. Concurrent index gibi transaction dışı işlemler ayrı autocommit adımları ve aynı SET ROLE bağlamında ele alınır.

**Ön koşul / güvenlik:** Poola dönmeden RESET ROLE/search_path temizliği veya bağlantı imhası. Bu iskelet tek başına herhangi bir iş tablosu oluşturmaz; kullanıcı migration SQLiyle karıştırılmamalı.

**Kaynak:** [P03](#kaynak-p03) [P04](#kaynak-p04)


<a id="pg-prov-008"></a>
### PG-PROV-008 — İsteğe bağlı rol/DB oturum politikaları

```yaml
id: PG-PROV-008
engine: postgresql
category: provision
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** İlgili rolleri/ayarları değiştirme yetkisi

**Girdi:** app_user,reader_user,db_name,app_schema:identifier; policy timeout değerleri

```sql
ALTER ROLE {{ident:app_user}} IN DATABASE {{ident:db_name}}
  SET search_path = pg_catalog, {{ident:app_schema}};
ALTER ROLE {{ident:reader_user}} IN DATABASE {{ident:db_name}}
  SET search_path = pg_catalog, {{ident:app_schema}};
ALTER ROLE {{ident:app_user}} IN DATABASE {{ident:db_name}}
  SET idle_in_transaction_session_timeout = {{literal:idle_tx_timeout}};
ALTER ROLE {{ident:reader_user}} IN DATABASE {{ident:db_name}}
  SET statement_timeout = {{literal:reader_statement_timeout}};
ALTER ROLE {{ident:reader_user}} IN DATABASE {{ident:db_name}}
  SET default_transaction_read_only = on;
```

**Yorum / kullanım:** Readonly transaction varsayılanı yardımcı önlem; kullanıcı bunu değiştirebildiği için gerçek güvenlik GRANT/role/RLS katmanıdır. İstemci commit davranışı ve pool yeniden bağlanması test edilir.

**Ön koşul / güvenlik:** Evrensel statement_timeout zorlaması yok; uzun ETL/report işi için ayrı profil. Ayarlar yeni oturumlarda etkili olur. search_path nesne çözümünü değiştirir; uygulama smoke testi gerekir.

**Kaynak:** [P01](#kaynak-p01) [P12](#kaynak-p12)


<a id="pg-prov-009"></a>
### PG-PROV-009 — Oluşturma sonrası owner ve effective izin doğrulaması

```yaml
id: PG-PROV-009
engine: postgresql
category: provision
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** app_user,reader_user,app_schema:text

```sql
SELECT d.datname, pg_get_userbyid(d.datdba) AS owner,
       has_database_privilege(:app_user, d.oid, 'CONNECT') AS app_connect,
       has_database_privilege(:reader_user, d.oid, 'CONNECT') AS reader_connect,
       has_database_privilege(:app_user, d.oid, 'CREATE') AS app_can_create_schema
FROM pg_database d WHERE d.datname = current_database();

SELECT n.nspname, c.relname, pg_get_userbyid(c.relowner) AS owner,
       has_table_privilege(:app_user, c.oid, 'SELECT') AS app_select,
       has_table_privilege(:app_user, c.oid, 'INSERT') AS app_insert,
       has_table_privilege(:app_user, c.oid, 'TRUNCATE') AS app_truncate,
       has_table_privilege(:reader_user, c.oid, 'SELECT') AS reader_select,
       has_table_privilege(:reader_user, c.oid, 'INSERT') AS reader_insert,
       has_table_privilege(:reader_user, c.oid, 'UPDATE') AS reader_update,
       has_table_privilege(:reader_user, c.oid, 'DELETE') AS reader_delete
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname=:app_schema AND c.relkind IN ('r','p','v','m','f')
ORDER BY c.relname;
```

**Yorum / kullanım:** Table privilege sonucu RLS satır görünürlüğü veya routine üzerinden dolaylı etkileri tam göstermez. Ayrı kimlikle pozitif/negatif integration test şarttır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P04](#kaynak-p04) [P12](#kaynak-p12)


<a id="pg-prov-010"></a>
### PG-PROV-010 — Ayrı monitoring hesabı oluşturma

```yaml
id: PG-PROV-010
engine: postgresql
category: provision
priority: P1
risk: R1
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Rol oluşturma + pg_monitor üyeliği verebilme

**Girdi:** monitor_user,db_name:identifier; monitor_password:secret

```sql
CREATE ROLE {{ident:monitor_user}} LOGIN INHERIT
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
  PASSWORD {{secret:monitor_password}};
GRANT pg_monitor TO {{ident:monitor_user}};
GRANT CONNECT ON DATABASE {{ident:db_name}} TO {{ident:monitor_user}};
```

**Yorum / kullanım:** Pratik built-in gözlem profili; minimum tek-view profili değildir. pg_monitor geniş sistem bilgisi ve sorgu metni görünürlüğü verebilir. Bütün uygulama verilerinde SELECT sağlamaz; DBA bakım yetkisi vermez.

**Ön koşul / güvenlik:** Her DB için CONNECT ayrıca planlanır. Managed sağlayıcının pg_monitor verme sınırları kontrol edilir; başarısız olursa superusera yükseltme yapılmaz.

**Kaynak:** [P08](#kaynak-p08)


## 5.2 Günlük, kritik ve ayrıntılı PostgreSQL kontrolleri


<a id="pg-mon-001"></a>
### PG-MON-001 — Bağlantı kimliği, sürüm ve primary/replica durumu

```yaml
id: PG-MON-001
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: connection
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT clock_timestamp() AS sampled_at, current_database() AS database_name,
       current_user, session_user, inet_server_addr() AS server_address,
       inet_server_port() AS server_port,
       current_setting('server_version_num')::int AS server_version_num,
       current_setting('server_version') AS server_version,
       pg_postmaster_start_time() AS instance_start,
       pg_is_in_recovery() AS is_replica,
       current_setting('transaction_read_only') AS transaction_read_only;
```

**Yorum / kullanım:** Her ekranda hedef ve writable durumu göster. Proxy/HAProxy endpointi fiziksel node kimliği değildir; failover sonrası fingerprint yeniden değerlendirilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P07](#kaynak-p07)


<a id="pg-mon-002"></a>
### PG-MON-002 — Database listesi ve boyutları

```yaml
id: PG-MON-002
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Her hedef DB boyutunu görme yetkisi; CONNECT veya uygun istatistik rolü

**Girdi:** Yok

```sql
SELECT d.oid, d.datname, pg_get_userbyid(d.datdba) AS owner,
       pg_encoding_to_char(d.encoding) AS encoding,
       d.datcollate, d.datctype, d.datallowconn, d.datconnlimit,
       has_database_privilege(d.oid,'CONNECT') AS can_connect,
       CASE WHEN has_database_privilege(d.oid,'CONNECT')
              OR pg_has_role(current_user,'pg_read_all_stats','USAGE')
            THEN pg_database_size(d.oid) END AS size_bytes
FROM pg_database d
WHERE NOT d.datistemplate
ORDER BY size_bytes DESC NULLS LAST;
```

**Yorum / kullanım:** Boyut gerçek dosya kullanımını toplar; büyük kataloglarda sık polling yapma. Yetki olmayan DB boyutu NULL/unknown kalır; sıfır olarak sunma. Grant değişikliği nedeniyle çalışma anında permission hatası oluşursa DB başına kontrollü değerlendirme yap.

**Ön koşul / güvenlik:** UI önce erişilebilir DBleri bulur, sonra yalnız görünür hedeflerde boyut sorgular. Hedef DBleri göstermek verilerine SELECT erişimi vermez.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-003"></a>
### PG-MON-003 — Temel çalışma ayarları ve restart bekleyen ayarlar

```yaml
id: PG-MON-003
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT name, setting, unit, context, source, pending_restart
FROM pg_settings
WHERE name IN ('max_connections','superuser_reserved_connections','reserved_connections',
 'shared_buffers','work_mem','maintenance_work_mem','effective_cache_size',
 'autovacuum','autovacuum_max_workers','autovacuum_freeze_max_age',
 'wal_level','max_wal_size','min_wal_size','wal_keep_size','max_slot_wal_keep_size',
 'archive_mode','archive_timeout','max_replication_slots','max_wal_senders',
 'track_io_timing','track_wal_io_timing','shared_preload_libraries',
 'hot_standby_feedback','max_standby_streaming_delay','idle_replication_slot_timeout')
ORDER BY name;
```

**Yorum / kullanım:** Var olmayan parametre sürümde desteklenmiyor olabilir. work_mem bütün sunucunun limiti değildir; operasyon/worker başına çoğalabilir. Otomatik tuning yok.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-004"></a>
### PG-MON-004 — Bağlantı doluluğu ve durum dağılımı

```yaml
id: PG-MON-004
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Tam görünürlük için pg_read_all_stats/pg_monitor

**Girdi:** Yok

```sql
SELECT datname, usename, application_name, state, backend_type,
       count(*) AS sessions
FROM pg_stat_activity
GROUP BY datname, usename, application_name, state, backend_type
ORDER BY sessions DESC;

SELECT count(*) FILTER (WHERE backend_type='client backend') AS client_sessions,
       current_setting('max_connections')::int AS max_connections,
       current_setting('superuser_reserved_connections')::int AS superuser_reserved,
       current_setting('reserved_connections',true) AS reserved_connections
FROM pg_stat_activity;
```

**Yorum / kullanım:** Toplam ve reserve slotları ayrı göster. İstemci sayısı/max_connections yaklaşık doluluk; reserved pool ve özel backend sınırları nedeniyle kabul edilebilir son bağlantıyı kesin hesapladığını iddia etme.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-005"></a>
### PG-MON-005 — Aktif ve uzun süren sorgular

```yaml
id: PG-MON-005
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Başka oturum detayları için pg_read_all_stats/pg_monitor

**Girdi:** database_name:text|null; min_seconds:int (varsayılan 30)

```sql
SELECT pid, backend_start, datname, usename, application_name, client_addr,
       state, wait_event_type, wait_event, query_start, xact_start,
       clock_timestamp()-query_start AS query_age,
       clock_timestamp()-xact_start AS transaction_age,
       left(query,2000) AS query_text
FROM pg_stat_activity
WHERE pid<>pg_backend_pid() AND backend_type='client backend'
  AND state='active'
  AND (:database_name::text IS NULL OR datname=:database_name)
  AND query_start < clock_timestamp()-make_interval(secs=>:min_seconds)
ORDER BY query_start
LIMIT 200;
```

**Yorum / kullanım:** Süre CPU süresi değildir; wait_event ile birlikte göster. Cancel/terminate ayrı aksiyonlara geçişte pid+backend_start yeniden doğrulanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-006"></a>
### PG-MON-006 — Idle in transaction ve eski transactionlar

```yaml
id: PG-MON-006
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_monitor/pg_read_all_stats

**Girdi:** min_seconds:int

```sql
SELECT pid, backend_start, datname, usename, application_name, state,
       xact_start, state_change,
       clock_timestamp()-xact_start AS transaction_age,
       clock_timestamp()-state_change AS state_age,
       age(backend_xid) AS xid_age, age(backend_xmin) AS xmin_age,
       left(query,1500) AS last_query
FROM pg_stat_activity
WHERE pid<>pg_backend_pid() AND xact_start IS NOT NULL
  AND (state LIKE 'idle in transaction%'
       OR xact_start<clock_timestamp()-make_interval(secs=>:min_seconds))
ORDER BY xact_start LIMIT 200;
```

**Yorum / kullanım:** Uzun transaction vacuum temizliğini engelleyebilir. Idle görünen oturumu otomatik öldürme; uygulama sahibi ve rollback etkisini değerlendir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P09](#kaynak-p09)


<a id="pg-mon-007"></a>
### PG-MON-007 — Blocking zinciri ve bekleyen oturumlar

```yaml
id: PG-MON-007
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Tam oturum bilgisi için pg_monitor

**Girdi:** Yok

```sql
SELECT a.pid, a.backend_start, a.datname, a.usename,
       a.wait_event_type, a.wait_event,
       clock_timestamp()-a.query_start AS query_age,
       pg_blocking_pids(a.pid) AS blockers,
       left(a.query,1500) AS query_text
FROM pg_stat_activity a
WHERE cardinality(pg_blocking_pids(a.pid))>0
ORDER BY a.query_start LIMIT 200;
```

**Yorum / kullanım:** UI blocker PIDlerini birleştirerek graph çıkarır; blocker idle olabilir. pg_locks self-joininden kaynaklanan yanlış kilit eşleştirmelerini önler. Çok sık çağrı lock-manager maliyeti yaratabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P07](#kaynak-p07)


<a id="pg-mon-008"></a>
### PG-MON-008 — Seçili oturumun lock ayrıntıları

```yaml
id: PG-MON-008
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** pid:int

```sql
SELECT l.pid, l.locktype, l.mode, l.granted, l.waitstart,
       l.database, l.relation, l.page, l.tuple,
       l.virtualxid, l.transactionid, l.fastpath
FROM pg_locks l WHERE l.pid=:pid
ORDER BY l.granted, l.locktype, l.mode;
```

**Yorum / kullanım:** Relation OID yalnız lockun ait olduğu DBde çözümlenir. Başka DBnin OIDsi current DBdeki regclassa çevrilerek yanlış tablo adı gösterilmez.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-009"></a>
### PG-MON-009 — Primary üzerindeki replication durumu ve byte gecikmeleri

```yaml
id: PG-MON-009
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster_primary
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_monitor/pg_read_all_stats

**Girdi:** Yok

```sql
SELECT pid, application_name, client_addr, state, sync_state,
       sent_lsn, write_lsn, flush_lsn, replay_lsn,
       pg_wal_lsn_diff(pg_current_wal_lsn(),sent_lsn) AS not_sent_bytes,
       pg_wal_lsn_diff(pg_current_wal_lsn(),replay_lsn) AS replay_distance_bytes,
       write_lag, flush_lag, replay_lag, reply_time
FROM pg_stat_replication ORDER BY application_name;
```

**Yorum / kullanım:** Byte mesafesi bir backlog ölçüsüdür. write/flush/replay lag alanları transaction acknowledgment zamanlamasıdır; replica yetişince/boştayken NULL olabilir. Eksik satır, beklenen replica envanteri olmadan sağlıklı demek değildir.

**Ön koşul / güvenlik:** Yalnız primary. Beklenen replica listesi/topoloji ayrı saklanır. Cloud tarafından gizlenen managed HA replika burada görünmeyebilir.

**Kaynak:** [P05](#kaynak-p05) [P07](#kaynak-p07)


<a id="pg-mon-010"></a>
### PG-MON-010 — Replica üzerindeki receive/replay ilerlemesi

```yaml
id: PG-MON-010
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster_replica
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; standby/recovery bağlantısı

**Yetki:** pg_monitor/pg_read_all_stats

**Girdi:** Yok

```sql
SELECT clock_timestamp() AS sampled_at,
       pg_last_wal_receive_lsn() AS receive_lsn,
       pg_last_wal_replay_lsn() AS replay_lsn,
       pg_wal_lsn_diff(pg_last_wal_receive_lsn(),pg_last_wal_replay_lsn()) AS local_apply_backlog_bytes,
       pg_last_xact_replay_timestamp() AS last_replayed_transaction_at,
       pg_is_wal_replay_paused() AS replay_pause_requested;
```

**Yorum / kullanım:** Primary idle iken now-last_xact_replay_timestamp büyür; tek başına replication lag alarmı değildir. Receiver bağlantı bilgisi ayrıca PG-MON-011 ile alınır.

**Ön koşul / güvenlik:** Yalnız replica; recovery durumu preflightta doğrulanır. Receive LSN NULL olabilen arşivden replay senaryosu unknown olarak ele alınır.

**Kaynak:** [P05](#kaynak-p05) [P07](#kaynak-p07)


<a id="pg-mon-011"></a>
### PG-MON-011 — WAL receiver sürüm bağımsız alan çıkarımı

```yaml
id: PG-MON-011
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster_replica
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_monitor/pg_read_all_stats

**Girdi:** Yok

```sql
SELECT j->>'pid' AS pid, j->>'status' AS status,
       COALESCE(j->>'flushed_lsn',j->>'received_lsn') AS received_or_flushed_lsn,
       j->>'written_lsn' AS written_lsn, j->>'received_tli' AS received_tli,
       j->>'last_msg_receipt_time' AS last_message_at,
       j->>'latest_end_lsn' AS latest_end_lsn,
       j->>'slot_name' AS slot_name, j->>'sender_host' AS sender_host,
       j->>'sender_port' AS sender_port
FROM (SELECT to_jsonb(w) AS j FROM pg_stat_wal_receiver w) q;
```

**Yorum / kullanım:** Kolon isimleri değişen sürümler için fail-safe projection. Driver gerekli değerleri typed olarak dönüştürür. conninfo parola/token içerebileceği için sonuçtan özellikle çıkarılmıştır.

**Ön koşul / güvenlik:** Yalnız replica; NULL alan desteklenmiyor veya görünmüyor diye işaretlenir.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-012"></a>
### PG-MON-012 — Replication slotları ve WAL tutulması

```yaml
id: PG-MON-012
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster_primary
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_monitor/uygun katalog görünürlüğü

**Girdi:** Yok

```sql
SELECT slot_name, slot_type, plugin, database, active, active_pid,
       restart_lsn, confirmed_flush_lsn, wal_status, safe_wal_size,
       pg_wal_lsn_diff(pg_current_wal_lsn(),restart_lsn) AS retention_distance_bytes,
       CASE WHEN slot_type='logical'
         THEN pg_wal_lsn_diff(pg_current_wal_lsn(),confirmed_flush_lsn)
       END AS logical_ack_distance_bytes,
       age(xmin) AS xmin_age, age(catalog_xmin) AS catalog_xmin_age,
       to_jsonb(s)->>'invalidation_reason' AS invalidation_reason,
       to_jsonb(s)->>'inactive_since' AS inactive_since,
       to_jsonb(s)->>'failover' AS failover,
       to_jsonb(s)->>'synced' AS synced
FROM pg_replication_slots s
ORDER BY retention_distance_bytes DESC NULLS LAST;
```

**Yorum / kullanım:** Slot mesafelerini toplayarak disk kullanımını hesaplama: slotların tuttuğu WAL aralıkları örtüşür. NULL restart_lsn=0 backlog değildir. lost/unreserved ve safe_wal_size ayrıca gösterilir.

**Ön koşul / güvenlik:** Primaryye özel. active=false tek başına gereksiz slot demek değildir; consumer, Patroni/HA slot yönetimi, backup ve CDC sahipliği incelenmeden drop yok.

**Kaynak:** [P06](#kaynak-p06) [P07](#kaynak-p07)


<a id="pg-mon-013"></a>
### PG-MON-013 — WAL üretim sayaçları

```yaml
id: PG-MON-013
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT clock_timestamp() AS sampled_at, wal_records, wal_fpi, wal_bytes,
       wal_buffers_full, stats_reset
FROM pg_stat_wal;
```

**Yorum / kullanım:** İki örnek arasından WAL byte/s ve FPI oranı hesapla. Tek kümülatif değeri anlık üretim hızı diye sunma. Büyük restore/repack/bulk işlerini slot tüketim hızıyla ilişkilendir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-014"></a>
### PG-MON-014 — Archive sağlığı

```yaml
id: PG-MON-014
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT archived_count, last_archived_wal, last_archived_time,
       failed_count, last_failed_wal, last_failed_time, stats_reset,
       current_setting('archive_mode') AS archive_mode
FROM pg_stat_archiver;
```

**Yorum / kullanım:** Failed_count artışı, son başarılı arşiv ve archive_mode birlikte değerlendirilir. Hiç WAL üretilmeyen sakin sistemde last_archived_time eskiliği tek başına hata değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P09](#kaynak-p09)


<a id="pg-mon-015"></a>
### PG-MON-015 — pg_wal dosya alanını ölçme

```yaml
id: PG-MON-015
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_ls_waldir EXECUTE; çoğu kurulumda pg_monitor/superuser veya açık grant

**Girdi:** Yok

```sql
SELECT count(*) AS file_count, sum(size) AS wal_directory_bytes,
       min(modification) AS oldest_file_mtime,
       max(modification) AS newest_file_mtime
FROM pg_ls_waldir();
```

**Yorum / kullanım:** Bu gerçek WAL dizin boyutudur; slot retained mesafesiyle aynı metrik değildir. Directory boyutu tüm filesystemin boş alanını vermez.

**Ön koşul / güvenlik:** Managed platformda fonksiyon kapalı olabilir. WAL dosyalarını shell/SQL ile silme. OS disk boş alanını SSH/collector veya cloud metriklerinden al.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-016"></a>
### PG-MON-016 — Prepared transactionlar ve eski 2PC işlemler

```yaml
id: PG-MON-016
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT transaction, gid, prepared, owner, database,
       clock_timestamp()-prepared AS prepared_age
FROM pg_prepared_xacts ORDER BY prepared;
```

**Yorum / kullanım:** Uzun prepared transaction cleanup/vacuum ve lockları etkileyebilir. COMMIT PREPARED / ROLLBACK PREPARED yalnız coordinator kararı ve açık runbook ile uygulanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-mon-017"></a>
### PG-MON-017 — Transaction ID wraparound riski: DB seviyesinde

```yaml
id: PG-MON-017
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT datname, age(datfrozenxid) AS xid_age,
       mxid_age(datminmxid) AS multixact_age,
       current_setting('autovacuum_freeze_max_age')::bigint AS autovacuum_freeze_max_age,
       round(100.0*age(datfrozenxid)/
             NULLIF(current_setting('autovacuum_freeze_max_age')::bigint,0),2)
             AS pct_of_autovacuum_freeze_threshold
FROM pg_database ORDER BY age(datfrozenxid) DESC;
```

**Yorum / kullanım:** Bu yüzde autovacuum eşik oranıdır; 100% anında wraparound/data loss demek değildir. Gerçek risk XID tüketim hızı, vacuum blockerları ve kalan güvenlik mesafesiyle değerlendirilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-mon-018"></a>
### PG-MON-018 — Eski frozen XID sahibi tablolar

```yaml
id: PG-MON-018
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT n.nspname, c.relname, c.relkind,
       age(c.relfrozenxid) AS xid_age, mxid_age(c.relminmxid) AS multixact_age,
       pg_total_relation_size(c.oid) AS total_bytes
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE c.relkind IN ('r','m','t') AND c.relfrozenxid<>'0'::xid
ORDER BY age(c.relfrozenxid) DESC LIMIT 100;
```

**Yorum / kullanım:** TOAST tabloları da önemli olabilir. Bir tabloyu kör VACUUM FULL yapmak yerine autovacuum ilerleme/blocker kontrollerini çalıştır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-mon-019"></a>
### PG-MON-019 — Autovacuum/analyze ve dead tuple tahminleri

```yaml
id: PG-MON-019
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT schemaname, relname, n_live_tup, n_dead_tup,
       round(100.0*n_dead_tup/NULLIF(n_live_tup+n_dead_tup,0),2) AS estimated_dead_pct,
       n_mod_since_analyze, n_ins_since_vacuum,
       last_vacuum, last_autovacuum, last_analyze, last_autoanalyze,
       vacuum_count, autovacuum_count, analyze_count, autoanalyze_count
FROM pg_stat_user_tables
ORDER BY n_dead_tup DESC LIMIT 200;
```

**Yorum / kullanım:** Dead tuple sayısı ve yüzdesi istatistik tahminidir, bloat byte hesabı değildir. Son reset, tablo aktivitesi ve ilişkili boyut birlikte gösterilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P09](#kaynak-p09)


<a id="pg-mon-020"></a>
### PG-MON-020 — Vacuum ve index oluşturma ilerlemesi

```yaml
id: PG-MON-020
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT pid, datname, relid, phase, heap_blks_total,
       heap_blks_scanned, heap_blks_vacuumed, index_vacuum_count
FROM pg_stat_progress_vacuum;

SELECT pid, datname, relid, index_relid, command, phase,
       lockers_total, lockers_done, current_locker_pid,
       blocks_total, blocks_done, tuples_total, tuples_done
FROM pg_stat_progress_create_index;
```

**Yorum / kullanım:** Progress her faz için aynı payda/anlamda değildir; 0 total bilinmiyorsa %0 sayma. Tamamlanmış iş viewden düşer, otomatik failed sayılmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-021"></a>
### PG-MON-021 — Tablo, index, TOAST ve yardımcı alan boyutları

```yaml
id: PG-MON-021
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT n.nspname, c.relname, c.relkind,
       pg_relation_size(c.oid) AS main_fork_bytes,
       pg_table_size(c.oid) AS table_including_toast_aux_bytes,
       pg_indexes_size(c.oid) AS index_bytes,
       pg_total_relation_size(c.oid) AS total_bytes,
       c.reltuples::bigint AS estimated_rows
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE c.relkind IN ('r','m')
  AND n.nspname NOT IN ('pg_catalog','information_schema')
  AND n.nspname NOT LIKE 'pg_toast%'
ORDER BY total_bytes DESC LIMIT 200;
```

**Yorum / kullanım:** Partitioned parent boyutu çocukların toplamı değildir. Main fork, table_size ve total_relation_size farkları UI etiketlerinde korunur.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-022"></a>
### PG-MON-022 — Partition ağacı ve child boyutları

```yaml
id: PG-MON-022
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** parent_table:text schema-qualified

```sql
SELECT p.relid, p.parentrelid, p.isleaf, p.level,
       n.nspname, c.relname,
       CASE WHEN p.isleaf THEN pg_total_relation_size(p.relid) END AS leaf_total_bytes
FROM pg_partition_tree(:parent_table::regclass) p
JOIN pg_class c ON c.oid=p.relid
JOIN pg_namespace n ON n.oid=c.relnamespace
ORDER BY p.level,n.nspname,c.relname;
```

**Yorum / kullanım:** Leaflerin boyutları toplamı ayrı metric olarak hesaplanır; parent ve childı iki kez toplamaya dikkat et.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-023"></a>
### PG-MON-023 — Index envanteri, kullanım ve constraint bağımlılıkları

```yaml
id: PG-MON-023
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT s.schemaname, s.relname, s.indexrelname, s.idx_scan,
       s.idx_tup_read, s.idx_tup_fetch,
       pg_relation_size(s.indexrelid) AS index_bytes,
       i.indisprimary, i.indisunique, i.indisvalid, i.indisready,
       pg_get_indexdef(s.indexrelid) AS definition,
       EXISTS(SELECT 1 FROM pg_constraint c WHERE c.conindid=s.indexrelid) AS backs_constraint
FROM pg_stat_user_indexes s JOIN pg_index i ON i.indexrelid=s.indexrelid
ORDER BY pg_relation_size(s.indexrelid) DESC LIMIT 200;
```

**Yorum / kullanım:** idx_scan=0 tek başına drop önerisi değildir. İstatistik penceresi, replica trafik, PK/unique/FK performansı, aylık rapor ve kullanım resetleri incelenir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P07](#kaynak-p07)


<a id="pg-mon-024"></a>
### PG-MON-024 — Geçersiz veya hazır olmayan indexler

```yaml
id: PG-MON-024
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT n.nspname, c.relname AS index_name,
       i.indrelid::regclass AS table_name,
       i.indisvalid, i.indisready, i.indislive,
       pg_get_indexdef(i.indexrelid) AS definition
FROM pg_index i JOIN pg_class c ON c.oid=i.indexrelid
JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE NOT i.indisvalid OR NOT i.indisready OR NOT i.indislive;
```

**Yorum / kullanım:** Başarısız concurrent işlemler araştırılır; kullanımda olan bakımın geçici durumuyla kalıcı sorunu ayır. Constraint bağlı indexi otomatik drop etme.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-025"></a>
### PG-MON-025 — İstatistik tabanlı büyük sequential scan adayları

```yaml
id: PG-MON-025
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT s.schemaname, s.relname, s.seq_scan, s.seq_tup_read,
       s.idx_scan, s.n_live_tup, pg_table_size(s.relid) AS table_bytes
FROM pg_stat_user_tables s
ORDER BY s.seq_tup_read DESC LIMIT 100;
```

**Yorum / kullanım:** PostgreSQL yerleşik olarak kesin missing-index listesi üretmez. Büyük sequential scan analitik işte doğru plan olabilir; ilgili query/EXPLAIN ve mevcut indexlerle değerlendirilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-026"></a>
### PG-MON-026 — pg_stat_statements en maliyetli sorgular

```yaml
id: PG-MON-026
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Extension görünürlüğü; diğer kullanıcı SQLleri için pg_read_all_stats

**Girdi:** pg_stat_statements_view:metadata-resolved qualified view

```sql
SELECT s.userid, s.dbid, s.queryid, s.calls,
       s.total_exec_time, s.mean_exec_time, s.max_exec_time,
       s.rows, s.shared_blks_hit, s.shared_blks_read,
       s.temp_blks_read, s.temp_blks_written, s.wal_bytes,
       left(s.query,2000) AS query_text
FROM {{qualified:pg_stat_statements_view}} s
WHERE s.dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
ORDER BY s.total_exec_time DESC LIMIT 100;
```

**Yorum / kullanım:** total_exec_time CPU zamanı değildir; kümülatif elapsed ölçüsüdür. Birinci ekranda toplam, ortalama, calls ve gözlem penceresini göster. Rate için queryid+userid+dbid ile iki örnek farkı kullan.

**Ön koşul / güvenlik:** Extension/preload kurulu değilse unsupported. Extension schema public varsayılmaz; kolonlar extversion ile keşfedilir. Kontrol tuşu extension kurmaz veya restart yapmaz.

**Kaynak:** [P10](#kaynak-p10)


<a id="pg-mon-027"></a>
### PG-MON-027 — Database sayaçları: cache, temp, deadlock ve transaction

```yaml
id: PG-MON-027
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT clock_timestamp() AS sampled_at, datid, datname, numbackends,
       xact_commit, xact_rollback, blks_read, blks_hit,
       tup_returned, tup_fetched, tup_inserted, tup_updated, tup_deleted,
       conflicts, temp_files, temp_bytes, deadlocks,
       blk_read_time, blk_write_time, stats_reset
FROM pg_stat_database WHERE datname=current_database();
```

**Yorum / kullanım:** Cache hit yüzdesi tek başına sağlık kararı değildir; OS cache ayrıca vardır. deadlocks delta yeni olay sayısıdır; deadlock zinciri için loglar gerekir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-028"></a>
### PG-MON-028 — IO sayaçları: PostgreSQL 16+ sürüm uyumlu projection

```yaml
id: PG-MON-028
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 16+; pg_stat_io varlığı zorunlu

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT j->>'backend_type' AS backend_type, j->>'object' AS object,
       j->>'context' AS context, j->>'reads' AS reads,
       j->>'read_time' AS read_time_ms, j->>'writes' AS writes,
       j->>'write_time' AS write_time_ms, j->>'hits' AS hits,
       j->>'evictions' AS evictions, j->>'fsyncs' AS fsyncs,
       j->>'op_bytes' AS op_bytes,
       j->>'read_bytes' AS read_bytes, j->>'write_bytes' AS write_bytes,
       j->>'stats_reset' AS stats_reset
FROM (SELECT to_jsonb(i) AS j FROM pg_stat_io i) q;
```

**Yorum / kullanım:** Kolon farklarında NULL döndürür; unsupported alanı 0 sayma. Sürümde byte alanı varsa onu kullan; yoksa ops*op_bytes dönüşümünün o sürüm anlamını test et.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-029"></a>
### PG-MON-029 — Checkpointer ve bgwriter sürüm uyumlu sayaçlar

```yaml
id: PG-MON-029
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT to_jsonb(b) AS bgwriter_counters FROM pg_stat_bgwriter b;
```

**Yorum / kullanım:** PG17+ checkpointer sayaçları ayrı viewe taşınmıştır. JSON sonuç bilinmeyen kolonlar için güvenli; UI sürüm mapinden bilinen alanları gösterir. PG-MON-030 ile tamamlanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-030"></a>
### PG-MON-030 — Checkpointer sayaçları: PostgreSQL 17+

```yaml
id: PG-MON-030
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 17+; view existence gate

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT to_jsonb(c) AS checkpointer_counters FROM pg_stat_checkpointer c;
```

**Yorum / kullanım:** 14–16 üzerinde çağırma; pg_stat_bgwriter varyantı kullan. Timed/requested checkpoint hızları delta ile hesaplanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-031"></a>
### PG-MON-031 — Roller ve ayrıcalıklı hesaplar

```yaml
id: PG-MON-031
engine: postgresql
category: security
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT rolname, rolcanlogin, rolsuper, rolcreatedb, rolcreaterole,
       rolreplication, rolbypassrls, rolinherit, rolconnlimit, rolvaliduntil
FROM pg_roles ORDER BY rolsuper DESC,rolcreaterole DESC,rolname;
```

**Yorum / kullanım:** pg_authid parola hashleri gösterilmez. Büyük yetkiler/NOLOGIN owner/LOGIN runtime ayrımı raporlanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P01](#kaynak-p01)


<a id="pg-mon-032"></a>
### PG-MON-032 — Role üyelik grafiği

```yaml
id: PG-MON-032
engine: postgresql
category: security
priority: P0
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT parent.rolname AS granted_role, member.rolname AS member_role,
       grantor.rolname AS granted_by, m.admin_option,
       to_jsonb(m)->>'inherit_option' AS inherit_option,
       to_jsonb(m)->>'set_option' AS set_option
FROM pg_auth_members m
JOIN pg_roles parent ON parent.oid=m.roleid
JOIN pg_roles member ON member.oid=m.member
LEFT JOIN pg_roles grantor ON grantor.oid=m.grantor
ORDER BY parent.rolname,member.rolname;
```

**Yorum / kullanım:** UI transitif üyelikleri cycle-safe graph traversal ile çıkarır. PG14/15te set/inherit kolonları yok; runtime üyelik davranışı sürüm adaptöründe yorumlanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P01](#kaynak-p01) [P04](#kaynak-p04)


<a id="pg-mon-033"></a>
### PG-MON-033 — Schema ve nesne bazlı effective izin matrisi

```yaml
id: PG-MON-033
engine: postgresql
category: security
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** role_name,schema_name:text

```sql
SELECT n.nspname,
       has_schema_privilege(:role_name,n.oid,'USAGE') AS can_use,
       has_schema_privilege(:role_name,n.oid,'CREATE') AS can_create
FROM pg_namespace n
WHERE n.nspname NOT LIKE 'pg_%' AND n.nspname<>'information_schema';

SELECT n.nspname, c.relname,
       has_table_privilege(:role_name,c.oid,'SELECT') AS can_select,
       has_table_privilege(:role_name,c.oid,'INSERT') AS can_insert,
       has_table_privilege(:role_name,c.oid,'UPDATE') AS can_update,
       has_table_privilege(:role_name,c.oid,'DELETE') AS can_delete,
       has_table_privilege(:role_name,c.oid,'TRUNCATE') AS can_truncate
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname=:schema_name AND c.relkind IN ('r','p','v','m','f')
ORDER BY c.relname;
```

**Yorum / kullanım:** Doğrudan grant listelemek yerine üyelikten gelen izinleri de göz önünde bulundurur. RLS ve column-specific izinler ayrıca incelenir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P04](#kaynak-p04) [P12](#kaynak-p12)


<a id="pg-mon-034"></a>
### PG-MON-034 — Default privileges görünümü

```yaml
id: PG-MON-034
engine: postgresql
category: security
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT pg_get_userbyid(d.defaclrole) AS creator_role,
       COALESCE(n.nspname,'<database-global>') AS schema_scope,
       d.defaclobjtype,
       CASE WHEN a.grantee IS NULL THEN '<explicit-empty-acl>'
            WHEN a.grantee=0 THEN 'PUBLIC'
            ELSE pg_get_userbyid(a.grantee) END AS grantee,
       a.privilege_type,a.is_grantable
FROM pg_default_acl d
LEFT JOIN pg_namespace n ON n.oid=d.defaclnamespace
LEFT JOIN LATERAL aclexplode(d.defaclacl) a ON true
ORDER BY creator_role,schema_scope,d.defaclobjtype,grantee;
```

**Yorum / kullanım:** Satır yoksa explicit default ACL yoktur; motorun built-in defaultları halen geçerlidir. explicit-empty-acl, boş ACL kaydının LEFT JOIN ile korunduğunu belirtir; grant değildir. Mevcut nesne ACLleri ayrı sorgudur.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P03](#kaynak-p03)


<a id="pg-mon-035"></a>
### PG-MON-035 — RLS ve SECURITY DEFINER incelemesi

```yaml
id: PG-MON-035
engine: postgresql
category: security
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** schema_name:text

```sql
SELECT schemaname,tablename,policyname,permissive,roles,cmd,qual,with_check
FROM pg_policies WHERE schemaname=:schema_name;

SELECT n.nspname,p.proname,pg_get_function_identity_arguments(p.oid) AS args,
       pg_get_userbyid(p.proowner) AS owner,p.prosecdef,p.proconfig
FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
WHERE n.nspname=:schema_name AND p.prosecdef;
```

**Yorum / kullanım:** SECURITY DEFINER fonksiyonlar sabit/güvenli search_path ve dar EXECUTE listesiyle incelenir. Fonksiyon gövdesi ve dış etkileri manual code review gerektirir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P12](#kaynak-p12) [P04](#kaynak-p04)


<a id="pg-mon-036"></a>
### PG-MON-036 — TLS oturumları ve şifrelenmeyen bağlantılar

```yaml
id: PG-MON-036
engine: postgresql
category: security
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT a.pid,a.datname,a.usename,a.application_name,a.client_addr,
       s.ssl,s.version,s.cipher,s.bits
FROM pg_stat_activity a LEFT JOIN pg_stat_ssl s ON s.pid=a.pid
WHERE a.backend_type='client backend'
ORDER BY s.ssl NULLS LAST,a.datname,a.usename;
```

**Yorum / kullanım:** Unix socket bağlantısını network plaintext ile aynı alarm sınıfına koyma. SSL varlığı clientın sertifika hostnameini doğruladığını ispatlamaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-037"></a>
### PG-MON-037 — Publication listesi ve izlenen tablolar

```yaml
id: PG-MON-037
engine: postgresql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** publication_name:text|null

```sql
SELECT pubname,puballtables,pubinsert,pubupdate,pubdelete,pubtruncate,
       pubviaroot FROM pg_publication ORDER BY pubname;
SELECT pubname,schemaname,tablename FROM pg_publication_tables
WHERE (:publication_name::text IS NULL OR pubname=:publication_name)
ORDER BY pubname,schemaname,tablename;
```

**Yorum / kullanım:** Publication filtrelemesi downstream logical payloadı sınırlar; clusterın diğer tabloları için WAL üretimini ortadan kaldırmaz. Slot tüketicisi yine WAL ilerlemesini sürdürmelidir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P06](#kaynak-p06) [P07](#kaynak-p07)


<a id="pg-mon-038"></a>
### PG-MON-038 — Logical subscription worker durumu

```yaml
id: PG-MON-038
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Subscription katalog görünürlüğü

**Girdi:** Yok

```sql
SELECT subid,subname,pid,relid,received_lsn,last_msg_send_time,
       last_msg_receipt_time,latest_end_lsn,latest_end_time
FROM pg_stat_subscription ORDER BY subname,relid;

SELECT s.subname,r.srrelid::regclass AS table_name,
       r.srsubstate,r.srsublsn
FROM pg_subscription_rel r
JOIN pg_subscription s ON s.oid=r.srsubid
ORDER BY s.subname,table_name;
```

**Yorum / kullanım:** Worker satırı yokluğu subscription disabled/failed/not configured ayrımıyla yorumlanır. pg_subscription.subconninfo okunmaz; bağlantı sırrı içerebilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-039"></a>
### PG-MON-039 — Prepared olmayan büyük/uzun COPY ve analyze ilerlemesi

```yaml
id: PG-MON-039
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT pid,datname,relid,command,type,bytes_processed,bytes_total,
       tuples_processed,tuples_excluded FROM pg_stat_progress_copy;
SELECT pid,datname,relid,phase,sample_blks_total,sample_blks_scanned
FROM pg_stat_progress_analyze;
```

**Yorum / kullanım:** COPY total bilinmiyorsa ETA uydurma. Bulk yük sırasında WAL hızı/replica backlog ile birlikte göster.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-040"></a>
### PG-MON-040 — Extension ve capability keşfi

```yaml
id: PG-MON-040
engine: postgresql
category: discovery
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT e.extname,e.extversion,n.nspname AS extension_schema
FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace
ORDER BY e.extname;

SELECT n.nspname,c.relname,a.attname,format_type(a.atttypid,a.atttypmod) AS data_type
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
JOIN pg_attribute a ON a.attrelid=c.oid
WHERE n.nspname='pg_catalog'
  AND c.relname IN ('pg_stat_io','pg_stat_checkpointer','pg_stat_wal_receiver',
                   'pg_replication_slots','pg_stat_progress_vacuum')
  AND a.attnum>0 AND NOT a.attisdropped
ORDER BY c.relname,a.attnum;
```

**Yorum / kullanım:** Eksik featurei SQL hata verene kadar deneyerek değil, katalogdan sürümlü projection kurarak ele al. DB platformu yalnız endpoint adına bakılarak kesin saptanmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05)


<a id="pg-mon-041"></a>
### PG-MON-041 — Seçili tablonun kolonları ve default/identity bilgisi

```yaml
id: PG-MON-041
engine: postgresql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** table_name:text schema-qualified

```sql
SELECT a.attnum,a.attname,format_type(a.atttypid,a.atttypmod) AS data_type,
       a.attnotnull,a.attidentity,a.attgenerated,
       pg_get_expr(d.adbin,d.adrelid) AS default_expression,
       col_description(a.attrelid,a.attnum) AS comment
FROM pg_attribute a
LEFT JOIN pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
WHERE a.attrelid=:table_name::regclass AND a.attnum>0 AND NOT a.attisdropped
ORDER BY a.attnum;
```

**Yorum / kullanım:** Data browser kolon seçimini bu metadatadan üretir; gizli/binary/büyük kolonları otomatik indirme.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-042"></a>
### PG-MON-042 — Constraint ve foreign key bağımlılıkları

```yaml
id: PG-MON-042
engine: postgresql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** table_name:text schema-qualified

```sql
SELECT conname,contype,conrelid::regclass AS source_table,
       CASE WHEN confrelid<>0 THEN confrelid::regclass END AS referenced_table,
       convalidated,condeferrable,condeferred,
       pg_get_constraintdef(oid,true) AS definition
FROM pg_constraint
WHERE conrelid=:table_name::regclass OR confrelid=:table_name::regclass
ORDER BY contype,conname;
```

**Yorum / kullanım:** Drop/truncate/column değişiminden önce hem incoming hem outgoing bağımlılıklar gerekir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-mon-043"></a>
### PG-MON-043 — Kontrollü veri önizleme

```yaml
id: PG-MON-043
engine: postgresql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Seçili kolonlarda SELECT

**Girdi:** table:qualified; selection:validated columns; unique_key:metadata-verified single unique key; page_size:int 1..1000

```sql
SELECT {{columns:selection}}
FROM {{qualified:table}}
ORDER BY {{ident:unique_key}}
LIMIT :page_size;
```

**Yorum / kullanım:** Tek kolon unique key şablonudur. Composite key için typed tuple-order ve cursor üret; nullable/nonunique keyde kararlı pagination iddiası yapma. Sonraki sayfa aynı sort direction ile keyset predicate kullanır.

**Ön koşul / güvenlik:** Readonly query session, statement timeout ve result byte limiti. Tam tablo COUNT otomatik değildir.

**Kaynak:** [P12](#kaynak-p12)


<a id="pg-mon-044"></a>
### PG-MON-044 — Seçili tabloda pgstattuple yaklaşık fiziksel ölçüm

```yaml
id: PG-MON-044
engine: postgresql
category: monitor
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Extension EXECUTE; kurulum ve sürüme bağlı

**Girdi:** table_name:text; pgstattuple_approx_function:metadata-resolved extension function

```sql
SELECT * FROM {{qualified:pgstattuple_approx_function}}(:table_name::regclass);
```

**Yorum / kullanım:** Yaklaşık tarama bile IO yaratabilir. Dead tuple/free space metriklerinden geri kazanılabilir alan için tahmin üret; kesin repack kazancı garantisi verme.

**Ön koşul / güvenlik:** Extension zaten kurulu olmalı. Tek tablo, açık maliyet onayı; bütün DB taraması veya otomatik CREATE EXTENSION yok.

**Kaynak:** [P11](#kaynak-p11)


<a id="pg-mon-045"></a>
### PG-MON-045 — Seçili tabloda tam pgstattuple ölçümü

```yaml
id: PG-MON-045
engine: postgresql
category: monitor
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pgstattuple EXECUTE

**Girdi:** table_name:text; pgstattuple_function:resolved extension function

```sql
SELECT * FROM {{qualified:pgstattuple_function}}(:table_name::regclass);
```

**Yorum / kullanım:** Tam tablo taraması yapabilir; ayrıntılı tuple/free-space sonucu döndürür. Canlı değişiklikler ve TOAST/index ayrımı nedeniyle bloat tek yüzdeye indirgenmez.

**Ön koşul / güvenlik:** Sadece tek nesne ve bakım penceresi/policy onayı. Büyük tabloda client timeout işlemin bitmesini tek başına garanti etmez; cancellation doğrulanır.

**Kaynak:** [P11](#kaynak-p11)


<a id="pg-mon-046"></a>
### PG-MON-046 — Standby query conflict sayaçları

```yaml
id: PG-MON-046
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: cluster_replica
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT datname,confl_tablespace,confl_lock,confl_snapshot,
       confl_bufferpin,confl_deadlock
FROM pg_stat_database_conflicts
WHERE datname=current_database();
```

**Yorum / kullanım:** Delta al. hot_standby_feedback açmak primary bloatını artırabilir; otomatik çözüm olarak etkinleştirme.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P05](#kaynak-p05) [P09](#kaynak-p09)


<a id="pg-mon-047"></a>
### PG-MON-047 — Tablo autovacuum override ve persistence kontrolü

```yaml
id: PG-MON-047
engine: postgresql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT n.nspname,c.relname,c.relpersistence,c.reloptions
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE c.relkind IN ('r','m')
  AND n.nspname NOT IN ('pg_catalog','information_schema')
ORDER BY n.nspname,c.relname;
```

**Yorum / kullanım:** autovacuum_enabled=false ve agresif override adaylarını göster. UNLOGGED tabloları replica/PITR beklentilerinden ayrı ele al; ayar değiştirme otomatik değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-mon-048"></a>
### PG-MON-048 — Index dışındaki nesne sahiplik sapmaları

```yaml
id: PG-MON-048
engine: postgresql
category: security
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** schema_name,expected_owner:text

```sql
SELECT 'relation' AS object_kind,n.nspname,c.relname AS object_name,
       c.oid AS object_oid,NULL::text AS identity_arguments,
       pg_get_userbyid(c.relowner) AS owner
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname=:schema_name AND c.relkind IN ('r','p','v','m','S','f')
  AND pg_get_userbyid(c.relowner)<>:expected_owner
UNION ALL
SELECT 'routine',n.nspname,p.proname,p.oid,
       pg_get_function_identity_arguments(p.oid),pg_get_userbyid(p.proowner)
FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
WHERE n.nspname=:schema_name AND pg_get_userbyid(p.proowner)<>:expected_owner;
```

**Yorum / kullanım:** REASSIGN OWNED gibi DB geneline yayılan düzeltme otomatik üretilmez; nesne bazlı owner change planı çıkar. Routine overloadlarını OID/arg imzasıyla ayırt et.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [P03](#kaynak-p03) [P07](#kaynak-p07)


## 5.3 PostgreSQL bakım ve kontrollü değişiklikler


<a id="pg-ops-001"></a>
### PG-OPS-001 — Seçili sorguyu iptal etme

```yaml
id: PG-OPS-001
engine: postgresql
category: maintenance
priority: P0
risk: R2
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Hedef role üyeliği/pg_signal_backend veya uygun admin; superuser hedef kısıtları

**Girdi:** pid:int; expected_backend_start:timestamptz

```sql
SELECT pg_cancel_backend(pid) AS signal_sent
FROM pg_stat_activity
WHERE pid=:pid AND backend_start=:expected_backend_start::timestamptz
  AND pid<>pg_backend_pid() AND backend_type='client backend';
```

**Yorum / kullanım:** Oturumu tamamen düşürmeden mevcut statementı iptal eder; uygulama transactionı aborted durumda kalabilir. Signal gönderilmesi işlemin tamamen bittiğini kanıtlamaz.

**Ön koşul / güvenlik:** Taze hedef preview; approved identity. Sistem, replication ve vacuum backendleri bu şablonla hedeflenmez.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-ops-002"></a>
### PG-OPS-002 — Seçili oturumu sonlandırma

```yaml
id: PG-OPS-002
engine: postgresql
category: maintenance
priority: P0
risk: R2
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** pg_signal_backend/uygun admin; superuser hedef kısıtları

**Girdi:** pid:int; expected_backend_start:timestamptz

```sql
SELECT pg_terminate_backend(pid,5000) AS terminated
FROM pg_stat_activity
WHERE pid=:pid AND backend_start=:expected_backend_start::timestamptz
  AND pid<>pg_backend_pid() AND backend_type='client backend';
```

**Yorum / kullanım:** Açık transaction rollback olur, uygulama bağlantısı kopar. Oturum yeniden bağlanabilir; kök neden çözümü değildir.

**Ön koşul / güvenlik:** Önce cancel seçeneği göster. Üretimde approval, rollback maliyeti ve uygulama sahibi bilgisi gerekir.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-ops-003"></a>
### PG-OPS-003 — Tek tabloda VACUUM ANALYZE

```yaml
id: PG-OPS-003
engine: postgresql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Sürüme göre owner/MAINTAIN veya uygun admin

**Girdi:** table:qualified

```sql
VACUUM (ANALYZE, VERBOSE) {{qualified:table}};
```

**Yorum / kullanım:** Alan çoğunlukla tablo içinde tekrar kullanılabilir hale gelir; OSye bütün dosya alanını geri vermesi beklenmez. Autovacuum blockerları önce incelenir.

**Ön koşul / güvenlik:** Transaction dışında. Ek IO/WAL/bakım etkisi; partition parent seçildiğinde alt nesne kapsamını önizle.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-ops-004"></a>
### PG-OPS-004 — Tek tabloda ANALYZE

```yaml
id: PG-OPS-004
engine: postgresql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Owner/MAINTAIN veya uygun admin

**Girdi:** table:qualified

```sql
ANALYZE VERBOSE {{qualified:table}};
```

**Yorum / kullanım:** Plan değişimine yol açabilir; istatistik eskiliği ve yükleme sonrası kullanılabilir. Her query sorununun otomatik çözümü değildir.

**Ön koşul / güvenlik:** Maliyet sınırı ve açık hedef; fonksiyon/statistics collection yan etkileri nedeniyle admin read-only sınıfına konmaz.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-ops-005"></a>
### PG-OPS-005 — Concurrent index oluşturma

```yaml
id: PG-OPS-005
engine: postgresql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Table owner veya uygun yetki

**Girdi:** index_name,column_name:identifier; table:qualified

```sql
CREATE INDEX CONCURRENTLY {{ident:index_name}}
ON {{qualified:table}} ({{ident:column_name}});
```

**Yorum / kullanım:** Basit tek kolon btree şablonu. Composite/partial/include/expression indeksleri AST builder ve sorgu planı incelemesiyle ayrı üretilir; textual WHERE fragment alınmaz.

**Ön koşul / güvenlik:** Transaction dışında; disk/WAL/replica lag, mevcut index benzerliği ve failed invalid index kontrolü. Partitioned parentta aynı sözdiziminin kullanılabilirliğini varsayma.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-ops-006"></a>
### PG-OPS-006 — Concurrent index yeniden oluşturma

```yaml
id: PG-OPS-006
engine: postgresql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** İlgili nesne maintenance/owner yetkisi

**Girdi:** index:qualified

```sql
REINDEX INDEX CONCURRENTLY {{qualified:index}};
```

**Yorum / kullanım:** Disk ve WAL tüketebilir, bazı aşamalarda lock bekler; "kilitsiz" diye sunma. Constraint/type/partition sınırlamalarını sürümde doğrula.

**Ön koşul / güvenlik:** Transaction dışında; bir seferde tek hedef; boş alan, replication ve uzun transaction preflight.

**Kaynak:** [P07](#kaynak-p07)


<a id="pg-ops-007"></a>
### PG-OPS-007 — VACUUM FULL: uzman bakım

```yaml
id: PG-OPS-007
engine: postgresql
category: maintenance
priority: P2
risk: R3
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Owner/uygun admin

**Girdi:** table:qualified

```sql
VACUUM (FULL, ANALYZE, VERBOSE) {{qualified:table}};
```

**Yorum / kullanım:** Tablo rewrite ve güçlü lock gerektirir; ek alan/IO/WAL üretebilir. Normal bloat önerisi olarak bir tık otomatik çalıştırılmaz.

**Ön koşul / güvenlik:** Bakım penceresi, lock etkisi, disk payı, backup/recovery planı ve iki kişi onayı. pg_repack alternatifi ayrıca binary/extension/versiyon kontrollü runbooktur.

**Kaynak:** [P09](#kaynak-p09)


<a id="pg-ops-008"></a>
### PG-OPS-008 — Replication slotunu silme

```yaml
id: PG-OPS-008
engine: postgresql
category: maintenance
priority: P2
risk: R3
scope: cluster_primary
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** REPLICATION/superuser veya sağlayıcının izin verdiği yönetim rolü

**Girdi:** slot_name:text

```sql
SELECT pg_drop_replication_slot(:slot_name);
```

**Yorum / kullanım:** Consumer devamlılığı kaybolabilir; logical resnapshot veya replica reinitialize gerekebilir. WAL hemen küçülmek zorunda değildir; archive/keep/checkpoint koşulları kalabilir.

**Ön koşul / güvenlik:** Slot aynı isim/tip/database/restart_lsn ve active=false olarak taze doğrulanır. Consumer owner, HA yöneticisi ve yeniden oluşturma davranışı doğrulanmadan çalıştırma. Drop/CREATE otomatik telafi değildir.

**Kaynak:** [P06](#kaynak-p06) [P07](#kaynak-p07)


<a id="pg-ops-009"></a>
### PG-OPS-009 — Seçili tablolardan publication oluşturma

```yaml
id: PG-OPS-009
engine: postgresql
category: maintenance
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** CREATE privilege ve published tabloların sahiplik koşulları

**Girdi:** publication_name:identifier; table:qualified; çoklu hedef için typed list

```sql
CREATE PUBLICATION {{ident:publication_name}}
FOR TABLE {{qualified:table}}
WITH (publish='insert, update, delete');
```

**Yorum / kullanım:** Debezium/consumer bu publicationı açıkça kullanmalıdır. Bu seçim başka tabloların WALini veya tüm cluster WALini ortadan kaldırmaz. Update/delete için replica identity gereksinimini kontrol et.

**Ön koşul / güvenlik:** Mevcut publication adoption/diff; FOR ALL TABLES varsayılan değil. Sadece update/delete hedefleniyorsa publish whitelist ona göre değiştirilir.

**Kaynak:** [P06](#kaynak-p06) [P07](#kaynak-p07)


<a id="pg-ops-010"></a>
### PG-OPS-010 — Yalnız onaylanan routine için EXECUTE verme

```yaml
id: PG-OPS-010
engine: postgresql
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Routine owner veya grant option

**Girdi:** function:metadata OIDden çözümlenmiş qualified ad; argument types:metadata-derived AST; grantee:identifier

```sql
GRANT EXECUTE ON FUNCTION {{qualified:function}}({{enum:validated_identity_argument_types}})
TO {{ident:grantee}};
```

**Yorum / kullanım:** Aşırı genel EXECUTE ON ALL FUNCTIONS yerine explicit allowlist. Overload imzası katalogdan üretilir; argument type stringi kullanıcı SQLi değildir.

**Ön koşul / güvenlik:** SECURITY DEFINER, write/external etkiler ve search_path review. Readonly role verilmeden yan etkiler doğrulanır.

**Kaynak:** [P04](#kaynak-p04) [P12](#kaynak-p12)


<a id="pg-ops-011"></a>
### PG-OPS-011 — Bağlantı hesabını devre dışı bırakma

```yaml
id: PG-OPS-011
engine: postgresql
category: security
priority: P1
risk: R1
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** İlgili rolü yönetme yetkisi

**Girdi:** role_name:identifier

```sql
ALTER ROLE {{ident:role_name}} NOLOGIN;
```

**Yorum / kullanım:** Yeni girişleri engeller; mevcut oturumlar ve rolü SET ROLE ile kullanabilen üyeler ayrıca değerlendirilir. Vault üyeliği iptaliyle aynı işlem değildir.

**Ön koşul / güvenlik:** Owner/system/provider rollerini koru. Mevcut session termination ayrıca R2 aksiyondur.

**Kaynak:** [P01](#kaynak-p01)


<a id="pg-ops-012"></a>
### PG-OPS-012 — Parola rotasyonu

```yaml
id: PG-OPS-012
engine: postgresql
category: security
priority: P1
risk: R1
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** İlgili role parola değiştirme yetkisi

**Girdi:** role_name:identifier; new_password:secret

```sql
ALTER ROLE {{ident:role_name}} PASSWORD {{secret:new_password}};
```

**Yorum / kullanım:** Vault güncelleme + app reconnect koordineli workflow. Hata halinde eski/yeni sır durumunu tahmin etme; doğrulama bağlantısı ve unknown_outcome yönetimi.

**Ön koşul / güvenlik:** Sır preview/audit/historyye yazılmaz. IAM/Entra accountta password login politikası ayrı; bunlara otomatik parola atanmaz.

**Kaynak:** [P01](#kaynak-p01)


<a id="pg-ops-013"></a>
### PG-OPS-013 — Readonly rolünden üyelik kaldırma

```yaml
id: PG-OPS-013
engine: postgresql
category: security
priority: P1
risk: R1
scope: cluster
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Role admin option/uygun yetki

**Girdi:** ro_role,member_user:identifier

```sql
REVOKE {{ident:ro_role}} FROM {{ident:member_user}};
```

**Yorum / kullanım:** Direct GRANT, PUBLIC ve diğer transitif roller nedeniyle erişim devam edebilir. Revoke sonrası effective privilege diff zorunlu.

**Ön koşul / güvenlik:** CASCADE varsayılan yok. grantor/admin bağımlılıklarını göster; active session etkisini ayrıca doğrula.

**Kaynak:** [P04](#kaynak-p04) [P12](#kaynak-p12)


<a id="pg-ops-014"></a>
### PG-OPS-014 — Tek schema/table için owner düzeltmesi

```yaml
id: PG-OPS-014
engine: postgresql
category: security
priority: P2
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** Mevcut owner/uygun admin; yeni ownera geçebilme ve schema CREATE koşulları

**Girdi:** table:qualified; new_owner:identifier

```sql
ALTER TABLE {{qualified:table}} OWNER TO {{ident:new_owner}};
```

**Yorum / kullanım:** Sadece tablolar için; sequence/view/routine ayrı DDL türleridir. Owner değişimi default privilegesı geçmişe uygulamaz; grants/defaults ayrıca planlanır.

**Ön koşul / güvenlik:** Runtime kullanıcıyı owner yapmak varsayılan seçenek değil. Lock, dependency ve ownership check.

**Kaynak:** [P03](#kaynak-p03) [P04](#kaynak-p04)


<a id="pg-ops-015"></a>
### PG-OPS-015 — Tehlikeli DROP DATABASE planı

```yaml
id: PG-OPS-015
engine: postgresql
category: maintenance
priority: P2
risk: R3
scope: maintenance_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** PostgreSQL 14–18; managed sağlayıcılarda capability kontrolü

**Yetki:** DB owner veya uygun yönetim yetkisi

**Girdi:** db_name:identifier

```sql
DROP DATABASE {{ident:db_name}};
```

**Yorum / kullanım:** Açık bağlantılarda işlem hata verebilir; FORCE otomatik eklenmez. Üretim verisi kalıcı olarak kaldırılır.

**Ön koşul / güvenlik:** Ayrı maintenance DB bağlantısı; sistem DB denylist, backup/restore doğrulaması, dependent consumer listesi, isim yazdırma ve iki kişi onayı. Oluşturma hatasının otomatik rollback adımı değildir.

**Kaynak:** [P02](#kaynak-p02) [P07](#kaynak-p07)


<a id="mssql"></a>
# 6. Microsoft SQL Server / Azure SQL SQL kataloğu

## 6.1 Güvenlik modeli ve runner kuralları

Üç şey ayrıdır: veritabanının sahibi (`sys.databases.owner_sid`), `db_owner` sabit rolüne üyelik ve uygulama schema'sının sahibi. Runtime kullanıcı bunların hiçbiri değildir. Greenfield self-managed örnekte gerçek DB owner için ayrı, sysadmin olmayan ve login'i kapatılmış dedicated principal; schema owner için `WITHOUT LOGIN` user kullanılır. Kuruluşun onaylı owner politikası varsa ona uyarlanır. Azure SQL owner/login modeli aynı şablona zorlanmaz. [M02](#kaynak-m02) [M03](#kaynak-m03) [M05](#kaynak-m05)

Sıra: keşif → self-managed DB ve owner → loginler → **hedef DB'ye bağlantı** → users/roles → schema → dar grants → migration rolü → test. `CREATE SCHEMA` ve gerekiyorsa CREATE VIEW/PROC gibi batch-sensitive DDL'leri ayrı driver batch'inde gönder. Dosyaya `GO` ekleyip driverın anlamasını bekleme. Azure SQL Database'de `USE başka_db` yerine yeni bağlantı aç; server DMV/Agent/msdb/physical file yönetimini aynı platform gibi sunma. [C07](#kaynak-c07)

DMV yetkileri aksiyon bazında çözülür: 2019 tabanında VIEW SERVER/DATABASE STATE; 2022+ birçok performans görünümünde VIEW SERVER/DATABASE PERFORMANCE STATE; güvenlik görünümlerinde ayrı SECURITY STATE ihtiyacı olabilir. Azure SQL servis seviyesi ve rol şartları ayrıca değişir. Bir "monitor" grantinin bütün DMVleri açtığı iddia edilmez. [M12](#kaynak-m12)


<a id="ms-prov-001"></a>
### MS-PROV-001 — Sürüm, platform, DB ve principal çakışma keşfi

```yaml
id: MS-PROV-001
engine: mssql
category: provision
priority: P0
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** db_name,owner_login,app_login,reader_login,migrator_login:text

```sql
SELECT SERVERPROPERTY('ProductVersion') AS product_version,
       SERVERPROPERTY('ProductMajorVersion') AS major_version,
       SERVERPROPERTY('EngineEdition') AS engine_edition,
       SERVERPROPERTY('Edition') AS edition,
       ORIGINAL_LOGIN() AS original_login,SUSER_SNAME() AS login_name,
       DB_NAME() AS database_name,USER_NAME() AS database_user;
SELECT name,state_desc,collation_name,compatibility_level,
       SUSER_SNAME(owner_sid) AS database_owner
FROM sys.databases WHERE name=:db_name;
SELECT name,type_desc,is_disabled FROM sys.server_principals
WHERE name IN (:owner_login,:app_login,:reader_login,:migrator_login);
```

**Yorum / kullanım:** sys.server_principals sorgusu Azure SQL contained-only varyantta atlanır veya supported visibility olarak gösterilir. Owner SID çözülmüyorsa otomatik sa ataması yapma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M01](#kaynak-m01) [M02](#kaynak-m02) [M15](#kaynak-m15)


<a id="ms-prov-002"></a>
### MS-PROV-002 — Self-managed database ve fiziksel dosya planı

```yaml
id: MS-PROV-002
engine: mssql
category: provision
priority: P0
risk: R1
scope: master
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** Yalnız self-managed SQL Server 2019/2022/2025; RDS/Azure için bu dosya şablonu kapalı

**Yetki:** CREATE ANY DATABASE veya uygun DB oluşturma yetkisi

**Girdi:** DB, logical adlar; onaylı server-side paths; bounded size/growth MB; sys.fn_helpcollations allowlist

```sql
CREATE DATABASE {{ident:db_name}}
ON PRIMARY
( NAME={{literal:data_logical_name}},
  FILENAME={{literal:data_file_path}},
  SIZE={{int:data_initial_mb}}MB,
  MAXSIZE={{int:data_max_mb}}MB,
  FILEGROWTH={{int:data_growth_mb}}MB )
LOG ON
( NAME={{literal:log_logical_name}},
  FILENAME={{literal:log_file_path}},
  SIZE={{int:log_initial_mb}}MB,
  MAXSIZE={{int:log_max_mb}}MB,
  FILEGROWTH={{int:log_growth_mb}}MB )
COLLATE {{enum:collation}};
```

**Yorum / kullanım:** Dosya yolları SQL Server hostuna aittir, masaüstü istemciye değil. Sabit MB growth planlanır; boyutlar iş yükü ve storage bütçesiyle kullanıcı tarafından belirlenir.

**Ön koşul / güvenlik:** Disk boşluğu, volume ve servis hesabı izinleri; dosya çakışması ve existing DB kontrolü. CREATE DATABASE explicit transaction dışında yürür.

**Kaynak:** [M01](#kaynak-m01)


<a id="ms-prov-003"></a>
### MS-PROV-003 — Dedicated DB owner principal oluşturma ve atama

```yaml
id: MS-PROV-003
engine: mssql
category: provision
priority: P0
risk: R1
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Self-managed; managed provider owner değişimine izin veriyorsa ayrı onaylı varyant

**Yetki:** ALTER ANY LOGIN + DB ownership transfer koşulları

**Girdi:** owner_login,db_name:identifier; owner_login_password:secret

```sql
CREATE LOGIN {{ident:owner_login}}
WITH PASSWORD={{secret:owner_login_password}}, CHECK_POLICY=ON;
ALTER AUTHORIZATION ON DATABASE::{{ident:db_name}} TO {{ident:owner_login}};
ALTER LOGIN {{ident:owner_login}} DISABLE;
```

**Yorum / kullanım:** Owner login sysadmin rolüne eklenmez; günlük bağlantı için kullanılmaz. Schema owner bunun yerine geçmez. Mevcut kurumsal owner politikası varsa yeni login açma.

**Ön koşul / güvenlik:** İsim çakışmasında mevcut login parolasını/disable durumunu değiştirme. System DB ownerlarına uygulanmaz.

**Kaynak:** [M02](#kaynak-m02) [M05](#kaynak-m05)


<a id="ms-prov-004"></a>
### MS-PROV-004 — Uygulama, readonly ve migration loginleri

```yaml
id: MS-PROV-004
engine: mssql
category: provision
priority: P0
risk: R1
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Self-managed SQL Server / uygun MI-RDS SQL authentication profili

**Yetki:** ALTER ANY LOGIN/uygun admin

**Girdi:** Login/db adları:identifier; üç secret ref

```sql
CREATE LOGIN {{ident:app_login}}
WITH PASSWORD={{secret:app_password}},CHECK_POLICY=ON,
     DEFAULT_DATABASE={{ident:db_name}};
CREATE LOGIN {{ident:reader_login}}
WITH PASSWORD={{secret:reader_password}},CHECK_POLICY=ON,
     DEFAULT_DATABASE={{ident:db_name}};
CREATE LOGIN {{ident:migrator_login}}
WITH PASSWORD={{secret:migrator_password}},CHECK_POLICY=ON,
     DEFAULT_DATABASE={{ident:db_name}};
```

**Yorum / kullanım:** Windows/Entra federasyonu desteklenip yapılandırılmışsa parola profili yerine o adapter seçilir. Service password expiration ve rotasyon kurumsal politikadır; CHECK_EXPIRATIONı kör etkinleştirip uygulamayı düşürme.

**Ön koşul / güvenlik:** Azure SQL Database contained profilde bu adım yok. Master admin hesabı runtime olarak kullanılmaz.

**Kaynak:** [M02](#kaynak-m02) [C02](#kaynak-c02)


<a id="ms-prov-005"></a>
### MS-PROV-005 — DB user, schema owner ve özel izin rolleri

```yaml
id: MS-PROV-005
engine: mssql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** ALTER ANY USER/ROLE ve ilgili login mapping yetkileri

**Girdi:** User/login/schema/role adları:identifier

```sql
CREATE USER {{ident:schema_owner}} WITHOUT LOGIN;
CREATE USER {{ident:app_user}} FOR LOGIN {{ident:app_login}}
  WITH DEFAULT_SCHEMA={{ident:app_schema}};
CREATE USER {{ident:reader_user}} FOR LOGIN {{ident:reader_login}}
  WITH DEFAULT_SCHEMA={{ident:app_schema}};
CREATE USER {{ident:migrator_user}} FOR LOGIN {{ident:migrator_login}}
  WITH DEFAULT_SCHEMA={{ident:app_schema}};
CREATE ROLE {{ident:rw_role}} AUTHORIZATION dbo;
CREATE ROLE {{ident:ro_role}} AUTHORIZATION dbo;
CREATE ROLE {{ident:ddl_role}} AUTHORIZATION dbo;
```

**Yorum / kullanım:** Hedef DB bağlantısında çalışır. DEFAULT_SCHEMA yetki vermez. Kullanıcı schema owner veya db_owner yapılmaz.

**Ön koşul / güvenlik:** İsim çakışmasında SID/login eşleşmesini doğrula. Var olan farklı login userı sessiz remap etme.

**Kaynak:** [M02](#kaynak-m02) [M03](#kaynak-m03)


<a id="ms-prov-006"></a>
### MS-PROV-006 — İzole owner ile uygulama schema oluşturma

```yaml
id: MS-PROV-006
engine: mssql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** CREATE SCHEMA + seçilen ownerı atama yetkisi

**Girdi:** app_schema,schema_owner:identifier

```sql
CREATE SCHEMA {{ident:app_schema}} AUTHORIZATION {{ident:schema_owner}};
```

**Yorum / kullanım:** Ayrı schema owner kullanımı schema sınırlarının düşünülmesini sağlar. Ownership chaining ve module security yine ayrıca test edilir.

**Ön koşul / güvenlik:** Bu statement kendi batchinde. Mevcut schemaya adoption öncesi mevcut owner doğrulanır.

**Kaynak:** [M04](#kaynak-m04) [M05](#kaynak-m05)


<a id="ms-prov-007"></a>
### MS-PROV-007 — Schema kapsamlı RW/readonly izinleri ve üyelikler

```yaml
id: MS-PROV-007
engine: mssql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Schema grant ve role membership yönetimi

**Girdi:** Schema/role/user adları:identifier

```sql
GRANT SELECT ON SCHEMA::{{ident:app_schema}} TO {{ident:ro_role}};
GRANT SELECT,INSERT,UPDATE,DELETE
  ON SCHEMA::{{ident:app_schema}} TO {{ident:rw_role}};
ALTER ROLE {{ident:rw_role}} ADD MEMBER {{ident:app_user}};
ALTER ROLE {{ident:ro_role}} ADD MEMBER {{ident:reader_user}};
GRANT CONNECT TO {{ident:app_user}};
GRANT CONNECT TO {{ident:reader_user}};
GRANT CONNECT TO {{ident:migrator_user}};
```

**Yorum / kullanım:** Schema kapsamı gelecekteki uygun nesneleri de kapsar; PostgreSQLdeki creator default privilegesla aynı SQL yoktur. Hassas tablo aynı schemadaysa tüm-schema grant yerine nesne allowlist seç.

**Ön koşul / güvenlik:** db_datareader/db_datawriter/db_owner otomatik üyeliği yok. Procedure EXECUTE ayrıca ve sadece gerekli nesnelerde. Role üyeliği zaten varsa planner tekrarlamaz.

**Kaynak:** [M03](#kaynak-m03) [M04](#kaynak-m04)


<a id="ms-prov-008"></a>
### MS-PROV-008 — Ayrı migration yetki rolü

```yaml
id: MS-PROV-008
engine: mssql
category: provision
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** DB/schema düzeyinde ilgili grant ve role yönetimi

**Girdi:** ddl_role,app_schema,migrator_user:identifier

```sql
GRANT CREATE TABLE,CREATE VIEW,CREATE PROCEDURE,CREATE FUNCTION,CREATE TYPE
TO {{ident:ddl_role}};
GRANT ALTER,REFERENCES ON SCHEMA::{{ident:app_schema}} TO {{ident:ddl_role}};
ALTER ROLE {{ident:ddl_role}} ADD MEMBER {{ident:migrator_user}};
```

**Yorum / kullanım:** db_ddladmin/db_owner yerine sınırlı schema ve explicit CREATE izinleri. Migration veriye DML yapacaksa ayrı onayla RW rolü eklenebilir; varsayılan vermeme.

**Ön koşul / güvenlik:** DDL kimliği güvenilir kod deploy edebilir; tamamen zararsız bir kimlik değildir. Aynı ownerı paylaşan başka schemalara ownership-chain etkisini test et.

**Kaynak:** [M03](#kaynak-m03) [M04](#kaynak-m04)


<a id="ms-prov-009"></a>
### MS-PROV-009 — Greenfield DB güvenlik ve güvenilirlik ayarları

```yaml
id: MS-PROV-009
engine: mssql
category: provision
priority: P1
risk: R1
scope: database_admin
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Self-managed SQL Server; managed destekleyen seçenekler ayrı capability gate

**Yetki:** ALTER DATABASE

**Girdi:** db_name:identifier

```sql
ALTER DATABASE {{ident:db_name}} SET AUTO_CLOSE OFF;
ALTER DATABASE {{ident:db_name}} SET AUTO_SHRINK OFF;
ALTER DATABASE {{ident:db_name}} SET PAGE_VERIFY CHECKSUM;
ALTER DATABASE {{ident:db_name}} SET TRUSTWORTHY OFF;
ALTER DATABASE {{ident:db_name}} SET DB_CHAINING OFF;
```

**Yorum / kullanım:** Yeni DB güvenlik profili. CHECKSUM geçmiş bütün sayfaları anında tarayıp doğrulamaz. Recovery modeli ve RCSI için iş yükü/backup planı seçimi ayrıca gerekir.

**Ön koşul / güvenlik:** Mevcut DBde TRUSTWORTHY/DB_CHAINING değişikliği bağımlı kodu etkileyebilir; otomatik retrofit yok. ROLLBACK IMMEDIATE ekleme.

**Kaynak:** [M01](#kaynak-m01) [M11](#kaynak-m11)


<a id="ms-prov-010"></a>
### MS-PROV-010 — Azure SQL Database contained kullanıcı varyantı

```yaml
id: MS-PROV-010
engine: mssql
category: provision
priority: P0
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Azure SQL Database password-contained profili; self-managed yalnız containment ayrıca onaylıysa

**Yetki:** ALTER ANY USER

**Girdi:** User/schema adları; secret refs

```sql
CREATE USER {{ident:app_user}} WITH PASSWORD={{secret:app_password}},
  DEFAULT_SCHEMA={{ident:app_schema}};
CREATE USER {{ident:reader_user}} WITH PASSWORD={{secret:reader_password}},
  DEFAULT_SCHEMA={{ident:app_schema}};
CREATE USER {{ident:migrator_user}} WITH PASSWORD={{secret:migrator_password}},
  DEFAULT_SCHEMA={{ident:app_schema}};
```

**Yorum / kullanım:** Target DB doğrudan connection stringde yer alır. MS-PROV-005teki FOR LOGIN user adımlarının yerine geçer; schema owner/roles ve dar grantler korunur. Entra varyantı Cloud bölümünde.

**Ön koşul / güvenlik:** Parola authentication platform politikasıyla kapalıysa unsupported. Fiziksel dosya oluşturma ve server owner/login scripti bu profilde çalışmaz.

**Kaynak:** [M02](#kaynak-m02) [C06](#kaynak-c06) [C07](#kaynak-c07)


<a id="ms-prov-011"></a>
### MS-PROV-011 — Monitoring permission planı: server ve DB ayrı

```yaml
id: MS-PROV-011
engine: mssql
category: provision
priority: P1
risk: R1
scope: server_and_database_separate_connections
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili permissionları grant edebilme

**Girdi:** monitor_login,monitor_user:identifier

```sql
-- Server-scoped, self-managed 2019 seçeneği (yalnız bu varyant):
GRANT VIEW SERVER STATE TO {{ident:monitor_login}};

-- 2022+ performans DMVleri için alternatif varyant:
GRANT VIEW SERVER PERFORMANCE STATE TO {{ident:monitor_login}};

-- Target database, 2019 seçeneği:
GRANT VIEW DATABASE STATE TO {{ident:monitor_user}};

-- Target database, 2022+ performans seçeneği:
GRANT VIEW DATABASE PERFORMANCE STATE TO {{ident:monitor_user}};
```

**Yorum / kullanım:** Dört komut topluca çalıştırılmaz. Versiyon/platform ve seçilen actionların requirementlarıyla yalnız gereken statementlar planlanır. Security metadata ve bazı DMVler ek izin isteyebilir; least privilege action bazlıdır.

**Ön koşul / güvenlik:** Azure SQL Database server STATE grantlerini çalıştırma. MI/servis seviyesi istisnalarını resmî view dokümanına göre çöz. Monitor runtime ile aynı hesap olmaz.

**Kaynak:** [M12](#kaynak-m12) [M07](#kaynak-m07)


<a id="ms-prov-012"></a>
### MS-PROV-012 — Target kimlikle effective izin testi

```yaml
id: MS-PROV-012
engine: mssql
category: provision
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** schema_name,qualified_table:text

```sql
SELECT ORIGINAL_LOGIN() AS login_name,USER_NAME() AS db_user,DB_NAME() AS db_name;
SELECT permission_name FROM sys.fn_my_permissions(NULL,'DATABASE');
SELECT permission_name FROM sys.fn_my_permissions(:schema_name,'SCHEMA');
SELECT permission_name FROM sys.fn_my_permissions(:qualified_table,'OBJECT');
```

**Yorum / kullanım:** Admin olarak sorgulayarak başka kullanıcının effective haklarını tahmin etmek yerine app/reader credentialıyla ayrı test connection aç. Doğrudan DENY/role ve row-level security etkilerini negatif testlerle tamamla.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M02](#kaynak-m02) [M15](#kaynak-m15)


## 6.2 SQL Server günlük ve kritik kontroller


<a id="ms-mon-001"></a>
### MS-MON-001 — Sunucu ve DB kimliği

```yaml
id: MS-MON-001
engine: mssql
category: monitor
priority: P0
risk: R0
scope: connection
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT SYSUTCDATETIME() AS sampled_at_utc,@@SERVERNAME AS reported_server_name,
       SERVERPROPERTY('ProductVersion') AS product_version,
       SERVERPROPERTY('ProductMajorVersion') AS major_version,
       SERVERPROPERTY('Edition') AS edition,SERVERPROPERTY('EngineEdition') AS engine_edition,
       DB_NAME() AS database_name,ORIGINAL_LOGIN() AS original_login,
       SUSER_SNAME() AS effective_login,USER_NAME() AS database_user,
       DATABASEPROPERTYEX(DB_NAME(),'Updateability') AS updateability;
```

**Yorum / kullanım:** Managed provider/resource kimliği ayrıca APIden gelir. @@SERVERNAME tek başına immutable fingerprint değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M01](#kaynak-m01) [M02](#kaynak-m02)


<a id="ms-mon-002"></a>
### MS-MON-002 — Database durumu, recovery modeli ve log reuse nedeni

```yaml
id: MS-MON-002
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** database_name:text|null

```sql
SELECT name,state_desc,user_access_desc,recovery_model_desc,
       log_reuse_wait_desc,compatibility_level,collation_name,
       is_read_only,is_auto_close_on,is_auto_shrink_on,
       is_read_committed_snapshot_on,snapshot_isolation_state_desc,
       page_verify_option_desc,is_trustworthy_on,is_db_chaining_on,
       SUSER_SNAME(owner_sid) AS database_owner
FROM sys.databases
WHERE (:database_name IS NULL OR name=:database_name)
ORDER BY name;
```

**Yorum / kullanım:** SUSPECT/RECOVERY_PENDING gibi durumları kritik göster; kurtarma/repair otomatik başlatma. Log doluluğu için log_reuse_wait_desc ana kanıtlardan biridir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M01](#kaynak-m01) [M07](#kaynak-m07)


<a id="ms-mon-003"></a>
### MS-MON-003 — Oturum sayısı ve uygulama dağılımı

```yaml
id: MS-MON-003
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server_or_visible_scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** DMV sürüm/platform gözlem yetkileri

**Girdi:** Yok

```sql
SELECT login_name,host_name,program_name,status,
       count(*) AS session_count,sum(open_transaction_count) AS open_transactions
FROM sys.dm_exec_sessions WHERE is_user_process=1
GROUP BY login_name,host_name,program_name,status
ORDER BY session_count DESC;
```

**Yorum / kullanım:** host_name/program_name client tarafından gönderilir; güvenlik kimliği olarak kullanılmaz. Azure görünürlük sınırlıysa partial etiketi.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-004"></a>
### MS-MON-004 — Aktif istekler ve statement metni

```yaml
id: MS-MON-004
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server_or_visible_scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** 2019 VIEW SERVER STATE; 2022+ VIEW SERVER PERFORMANCE STATE; Azure scope kısıtları

**Girdi:** Yok

```sql
SELECT TOP(200) r.session_id,s.login_time,r.request_id,r.start_time,
       DB_NAME(r.database_id) AS database_name,s.login_name,s.host_name,s.program_name,
       r.status,r.command,r.cpu_time,r.total_elapsed_time,
       r.logical_reads,r.reads,r.writes,r.wait_type,r.wait_time,
       r.blocking_session_id,r.wait_resource,r.open_transaction_count,
       LEFT(SUBSTRING(t.text,(r.statement_start_offset/2)+1,
          ((CASE r.statement_end_offset WHEN -1 THEN DATALENGTH(t.text)
            ELSE r.statement_end_offset END-r.statement_start_offset)/2)+1),2000) AS statement_text
FROM sys.dm_exec_requests r
JOIN sys.dm_exec_sessions s ON s.session_id=r.session_id
OUTER APPLY sys.dm_exec_sql_text(r.sql_handle) t
WHERE s.is_user_process=1 AND r.session_id<>@@SPID
ORDER BY r.total_elapsed_time DESC;
```

**Yorum / kullanım:** CPU/elapsed birimi ms. SQL offsetleri byte olup nvarchar karakteri için /2 kullanılır. Parallel requestte bazı sayaçlar tüm worker toplamı değildir; aşırı kesin metrik sunma.

**Ön koşul / güvenlik:** Azure SQL Database bu DMV için sınırlı görünürlük gösterebilir; sıfır satırdan diğer oturumların yokluğu çıkarılmaz.

**Kaynak:** [M06](#kaynak-m06)


<a id="ms-mon-005"></a>
### MS-MON-005 — Blocking ve blocker kimliği

```yaml
id: MS-MON-005
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server_or_visible_scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili DMV gözlem yetkileri

**Girdi:** Yok

```sql
SELECT TOP(200) r.session_id AS blocked_session,
       r.blocking_session_id,r.wait_type,r.wait_time,r.wait_resource,
       DB_NAME(r.database_id) AS database_name,
       b.login_time AS blocker_login_time,b.login_name AS blocker_login,
       b.host_name AS blocker_host,b.program_name AS blocker_program,
       b.status AS blocker_status,b.open_transaction_count AS blocker_open_transactions
FROM sys.dm_exec_requests r
LEFT JOIN sys.dm_exec_sessions b ON b.session_id=r.blocking_session_id
WHERE r.blocking_session_id<>0
ORDER BY r.wait_time DESC;
```

**Yorum / kullanım:** Negatif blocking_session_id gerçek öldürülebilir kullanıcı SPIDsi olmayabilir. Sleeping blocker açık transaction tutuyor olabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M06](#kaynak-m06)


<a id="ms-mon-006"></a>
### MS-MON-006 — Eski açık transactionlar ve kullanılan log

```yaml
id: MS-MON-006
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili server DMV izinleri

**Girdi:** Yok

```sql
SELECT TOP(100) st.session_id,s.login_time,s.login_name,s.status,
       at.transaction_id,at.transaction_begin_time,
       DATEDIFF_BIG(SECOND,at.transaction_begin_time,SYSDATETIME()) AS age_seconds,
       dt.database_id,DB_NAME(dt.database_id) AS database_name,
       dt.database_transaction_log_bytes_used,
       dt.database_transaction_log_bytes_reserved
FROM sys.dm_tran_active_transactions at
JOIN sys.dm_tran_session_transactions st ON st.transaction_id=at.transaction_id
JOIN sys.dm_exec_sessions s ON s.session_id=st.session_id
LEFT JOIN sys.dm_tran_database_transactions dt ON dt.transaction_id=at.transaction_id
WHERE s.is_user_process=1
ORDER BY at.transaction_begin_time;
```

**Yorum / kullanım:** Tek transaction birçok DB satırı üretebilir; tabloyu session count olarak toplama. Local datetime alanlarını UTC zamanlarıyla yanlış çıkarma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-007"></a>
### MS-MON-007 — Bekleme sayaçları

```yaml
id: MS-MON-007
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Server performance/state

**Girdi:** Yok

```sql
SELECT TOP(100) wait_type,waiting_tasks_count,wait_time_ms,
       signal_wait_time_ms,max_wait_time_ms
FROM sys.dm_os_wait_stats
WHERE waiting_tasks_count>0
ORDER BY wait_time_ms DESC;
```

**Yorum / kullanım:** Cumulative delta gerekir. Idle/internal wait ignorelist sürümlü ve görünür olmalı; yüksek toplam bekleme otomatik disk/CPU sorunu değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-008"></a>
### MS-MON-008 — Şu an bekleyen görevler

```yaml
id: MS-MON-008
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Server performance/state

**Girdi:** Yok

```sql
SELECT TOP(200) session_id,exec_context_id,wait_duration_ms,
       wait_type,blocking_session_id,resource_description
FROM sys.dm_os_waiting_tasks
WHERE session_id IS NOT NULL
ORDER BY wait_duration_ms DESC;
```

**Yorum / kullanım:** Parallel session için birden fazla task satırı olabilir. Sonuç blocker/request ekranıyla ilişkilendirilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-009"></a>
### MS-MON-009 — Transaction log doluluğu

```yaml
id: MS-MON-009
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Sürüm ve Azure service tier bazlı state/performance yetkisi

**Girdi:** Yok

```sql
SELECT database_id,total_log_size_in_bytes,used_log_space_in_bytes,
       used_log_space_in_percent,log_space_in_bytes_since_last_backup
FROM sys.dm_db_log_space_usage;
```

**Yorum / kullanım:** Log filelarının birleşik kullanımı. Aktif transaction, LOG_BACKUP, replication/AG veya başka reuse wait nedenleriyle ilişkilendir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M07](#kaynak-m07)


<a id="ms-mon-010"></a>
### MS-MON-010 — Data/log file kapasitesi ve autogrowth

```yaml
id: MS-MON-010
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT file_id,name,type_desc,physical_name,
       CONVERT(bigint,size)*8192 AS size_bytes,
       CASE WHEN type=0 THEN CONVERT(bigint,FILEPROPERTY(name,'SpaceUsed'))*8192 END AS data_used_bytes,
       max_size,is_percent_growth,growth,
       CASE WHEN is_percent_growth=0 THEN CONVERT(bigint,growth)*8192 END AS growth_bytes
FROM sys.database_files ORDER BY type,file_id;
```

**Yorum / kullanım:** Data FILEPROPERTY ile log aktif kullanımını karıştırma; log için MS-MON-009. max_size -1/sentinel değerlerini sınırsız sanıp cloud quota yokmuş gibi gösterme.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M01](#kaynak-m01) [M07](#kaynak-m07)


<a id="ms-mon-011"></a>
### MS-MON-011 — DB file IO gecikme sayaçları

```yaml
id: MS-MON-011
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Sürüm/platform DMV izni

**Girdi:** Yok

```sql
SELECT f.file_id,f.name,f.type_desc,
       v.num_of_reads,v.num_of_bytes_read,v.io_stall_read_ms,
       v.num_of_writes,v.num_of_bytes_written,v.io_stall_write_ms,
       v.size_on_disk_bytes
FROM sys.dm_io_virtual_file_stats(DB_ID(),NULL) v
JOIN sys.database_files f ON f.file_id=v.file_id;
```

**Yorum / kullanım:** Anlık ortalama read latency=delta_stall_read_ms/delta_num_reads; write ayrı. Tek lifetime oran son dakikanın ölçümü değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-012"></a>
### MS-MON-012 — Self-managed disk/volume boşluğu

```yaml
id: MS-MON-012
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Self-managed veya açık destek veren managed profil; Azure SQL Database kapalı

**Yetki:** Server DMV state/performance

**Girdi:** Yok

```sql
SELECT DISTINCT vs.volume_mount_point,vs.logical_volume_name,
       vs.total_bytes,vs.available_bytes
FROM sys.master_files mf
CROSS APPLY sys.dm_os_volume_stats(mf.database_id,mf.file_id) vs;
```

**Yorum / kullanım:** DB file volume bilgisi; tüm işletim sistemi disk envanteri değildir. Cloud kota metriklerinden farklıdır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-013"></a>
### MS-MON-013 — Tempdb alan dağılımı

```yaml
id: MS-MON-013
engine: mssql
category: monitor
priority: P0
risk: R0
scope: tempdb
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Self-managed/MI destekli tempdb bağlantısı; Azure ayrı görünürlük profili

**Yetki:** İlgili DB state/performance

**Girdi:** Yok

```sql
SELECT file_id,total_page_count*CONVERT(bigint,8192) AS total_bytes,
       unallocated_extent_page_count*CONVERT(bigint,8192) AS unallocated_bytes,
       user_object_reserved_page_count*CONVERT(bigint,8192) AS user_object_bytes,
       internal_object_reserved_page_count*CONVERT(bigint,8192) AS internal_object_bytes,
       version_store_reserved_page_count*CONVERT(bigint,8192) AS version_store_bytes
FROM sys.dm_db_file_space_usage;
```

**Yorum / kullanım:** Runnerın gerçekten tempdb bağlantısında olduğunu doğrula; user DB alanını tempdb diye etiketleme. Version store ve spill kaynakları için session/task stats incelenir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-014"></a>
### MS-MON-014 — Tempdb kullanan sessionlar

```yaml
id: MS-MON-014
engine: mssql
category: monitor
priority: P1
risk: R0
scope: tempdb
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili DB/server DMV izni

**Girdi:** Yok

```sql
SELECT TOP(100) u.session_id,s.login_time,s.login_name,s.program_name,
       (u.user_objects_alloc_page_count-u.user_objects_dealloc_page_count)*CONVERT(bigint,8192) AS user_bytes,
       (u.internal_objects_alloc_page_count-u.internal_objects_dealloc_page_count)*CONVERT(bigint,8192) AS internal_bytes
FROM sys.dm_db_session_space_usage u
LEFT JOIN sys.dm_exec_sessions s ON s.session_id=u.session_id
ORDER BY internal_bytes DESC;
```

**Yorum / kullanım:** Session sayaçları aktif taskın bütün anlık kullanımı olmayabilir; aktif task ayrıntısı sys.dm_db_task_space_usage ile ayrı genişletilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-015"></a>
### MS-MON-015 — Memory grant bekleyen sorgular

```yaml
id: MS-MON-015
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Server performance/state

**Girdi:** Yok

```sql
SELECT session_id,request_id,request_time,grant_time,wait_time_ms,
       requested_memory_kb,granted_memory_kb,required_memory_kb,
       used_memory_kb,max_used_memory_kb,queue_id
FROM sys.dm_exec_query_memory_grants
ORDER BY CASE WHEN grant_time IS NULL THEN 0 ELSE 1 END,requested_memory_kb DESC;
```

**Yorum / kullanım:** Büyük tek grant ve bekleyenler; plan/cardinality/concurrency değerlendirilir. max server memory otomatik değiştirilmez.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-016"></a>
### MS-MON-016 — SQL process memory ve host memory

```yaml
id: MS-MON-016
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Self-managed/uygun MI; Azure SQL Database platform metriği kullanır

**Yetki:** Server performance/state

**Girdi:** Yok

```sql
SELECT physical_memory_in_use_kb,locked_page_allocations_kb,
       large_page_allocations_kb,memory_utilization_percentage,
       process_physical_memory_low,process_virtual_memory_low
FROM sys.dm_os_process_memory;
SELECT total_physical_memory_kb,available_physical_memory_kb,
       system_memory_state_desc FROM sys.dm_os_sys_memory;
```

**Yorum / kullanım:** Konteyner/VM/cloud kaynak sınırlarını host toplam RAM ile karıştırma. Memory low flag ve çalışma seti birlikte yorumlanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-017"></a>
### MS-MON-017 — Scheduler ve CPU baskısı adayları

```yaml
id: MS-MON-017
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Server performance/state

**Girdi:** Yok

```sql
SELECT scheduler_id,status,runnable_tasks_count,current_tasks_count,
       active_workers_count,work_queue_count,pending_disk_io_count
FROM sys.dm_os_schedulers WHERE status='VISIBLE ONLINE';
```

**Yorum / kullanım:** Runnable queue baskı göstergesi olabilir, CPU% değildir. Gerçek CPU kullanımını OS/cloud kaynağından al; tek örnek alarm yerine süre penceresi.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-018"></a>
### MS-MON-018 — En pahalı cached queryler

```yaml
id: MS-MON-018
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server_or_visible_scope
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili DMV state/performance

**Girdi:** Yok

```sql
SELECT TOP(100) q.query_hash,q.execution_count,q.creation_time,q.last_execution_time,
       q.total_worker_time,q.total_elapsed_time,q.total_logical_reads,
       q.total_logical_writes,q.total_physical_reads,
       LEFT(SUBSTRING(t.text,(q.statement_start_offset/2)+1,
          ((CASE q.statement_end_offset WHEN -1 THEN DATALENGTH(t.text)
            ELSE q.statement_end_offset END-q.statement_start_offset)/2)+1),2000) AS statement_text
FROM sys.dm_exec_query_stats q
OUTER APPLY sys.dm_exec_sql_text(q.sql_handle) t
ORDER BY q.total_worker_time DESC;
```

**Yorum / kullanım:** Worker/elapsed burada mikro saniye raporlanır; requests DMV ms ile karıştırma. Plan cache eviction/recompile geçmişi silebilir; tarihsel gerçek için Query Store ayrı.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12) [M10](#kaynak-m10)


<a id="ms-mon-019"></a>
### MS-MON-019 — Query Store etkinlik ve kapasite durumu

```yaml
id: MS-MON-019
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT actual_state_desc,desired_state_desc,readonly_reason,
       current_storage_size_mb,max_storage_size_mb,
       query_capture_mode_desc,size_based_cleanup_mode_desc
FROM sys.database_query_store_options;
```

**Yorum / kullanım:** Kapalıysa kontrol ekranı açmaya çalışmaz. readonly_reason bit mask ve quota sorunlarını sürüm mappingiyle açıkla.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M10](#kaynak-m10)


<a id="ms-mon-020"></a>
### MS-MON-020 — Query Store zaman aralığında en maliyetli sorgular

```yaml
id: MS-MON-020
engine: mssql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Query Store görünürlüğü; sürüm bazlı DB state/performance

**Girdi:** from_time,to_time:datetimeoffset; bounded time window

```sql
SELECT TOP(100) q.query_id,MAX(LEFT(qt.query_sql_text,2000)) AS query_text,
       SUM(rs.count_executions) AS executions,
       SUM(rs.avg_duration*rs.count_executions)/1000.0 AS total_duration_ms,
       SUM(rs.avg_cpu_time*rs.count_executions)/1000.0 AS total_cpu_ms,
       SUM(rs.avg_duration*rs.count_executions)/
         NULLIF(SUM(rs.count_executions),0)/1000.0 AS weighted_avg_duration_ms
FROM sys.query_store_query q
JOIN sys.query_store_query_text qt ON qt.query_text_id=q.query_text_id
JOIN sys.query_store_plan p ON p.query_id=q.query_id
JOIN sys.query_store_runtime_stats rs ON rs.plan_id=p.plan_id
JOIN sys.query_store_runtime_stats_interval i ON i.runtime_stats_interval_id=rs.runtime_stats_interval_id
WHERE i.start_time>=:from_time AND i.end_time<=:to_time
  AND rs.execution_type=0
GROUP BY q.query_id
ORDER BY total_duration_ms DESC;
```

**Yorum / kullanım:** Ağırlıklı ortalama kullanılır, ortalamaların düz ortalaması alınmaz. Yalnız bütünü pencere içinde kalan interval seçildiği ve aborted/exception executions dışlandığı UIda gösterilir.

**Ön koşul / güvenlik:** Query Store enabled ve en az bir kapalı interval olmalı. Uzun tarih aralığı kontrolsüz sorgulanmaz.

**Kaynak:** [M10](#kaynak-m10)


<a id="ms-mon-021"></a>
### MS-MON-021 — Tablo boyutları ve tahmini satır sayısı

```yaml
id: MS-MON-021
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Metadata + partition stats görünürlüğü

**Girdi:** Yok

```sql
SELECT TOP(200) s.name AS schema_name,t.name AS table_name,
       SUM(CASE WHEN p.index_id IN (0,1) THEN p.row_count ELSE 0 END) AS approximate_rows,
       SUM(p.reserved_page_count)*CONVERT(bigint,8192) AS reserved_bytes,
       SUM(p.used_page_count)*CONVERT(bigint,8192) AS used_bytes
FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id
JOIN sys.dm_db_partition_stats p ON p.object_id=t.object_id
GROUP BY s.name,t.name ORDER BY reserved_bytes DESC;
```

**Yorum / kullanım:** Her indexin row_count değerini toplayıp tablo satır sayısını şişirme; yalnız heap/clustered satırları sayılır. Columnstore/LOB alanı dahil metriklerin anlamını koru.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-022"></a>
### MS-MON-022 — Index envanteri ve kullanımı

```yaml
id: MS-MON-022
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili metadata/DMV izni

**Girdi:** Yok

```sql
SELECT TOP(300) s.name AS schema_name,t.name AS table_name,i.name AS index_name,
       i.index_id,i.type_desc,i.is_unique,i.is_primary_key,i.is_unique_constraint,
       i.is_disabled,i.has_filter,i.filter_definition,
       u.user_seeks,u.user_scans,u.user_lookups,u.user_updates,
       u.last_user_seek,u.last_user_scan
FROM sys.tables t JOIN sys.schemas s ON s.schema_id=t.schema_id
JOIN sys.indexes i ON i.object_id=t.object_id
LEFT JOIN sys.dm_db_index_usage_stats u
  ON u.database_id=DB_ID() AND u.object_id=i.object_id AND u.index_id=i.index_id
WHERE i.index_id>0 ORDER BY s.name,t.name,i.index_id;
```

**Yorum / kullanım:** NULL kullanım=hiç kullanılmadığı kanıtı değil. Restart/reset ve nadir iş yükü; PK/unique constraint/foreign-key lookup rolü incelenmeden drop önerilmez.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12) [M09](#kaynak-m09)


<a id="ms-mon-023"></a>
### MS-MON-023 — Tek index/tablo fragmentation incelemesi

```yaml
id: MS-MON-023
engine: mssql
category: monitor
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** View/db performance-state ve nesne izinleri

**Girdi:** object_id:int>0; index_id:int|null

```sql
DECLARE @target_object_id int=:object_id;
IF @target_object_id IS NULL OR @target_object_id<=0
  THROW 51000,'Explicit object_id is required.',1;
SELECT object_id,index_id,partition_number,index_type_desc,
       alloc_unit_type_desc,index_level,page_count,
       avg_fragmentation_in_percent,avg_page_space_used_in_percent
FROM sys.dm_db_index_physical_stats(DB_ID(),@target_object_id,:index_id,NULL,'LIMITED');
```

**Yorum / kullanım:** LIMITEDde bazı yoğunluk alanları NULL olabilir. Tek oranla rebuild kararı verilmez; page count, density, workload ve maliyet dikkate alınır.

**Ön koşul / güvenlik:** NULL object_id bütün DB taraması anlamına gelebildiğinden önlenir. Readable AG secondaryde IS lock REDOyu bloklayabilir; varsayılan o hedefte kapalı.

**Kaynak:** [M08](#kaynak-m08)


<a id="ms-mon-024"></a>
### MS-MON-024 — Missing-index öneri adayları

```yaml
id: MS-MON-024
engine: mssql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili server/DB DMV permission

**Girdi:** Yok

```sql
SELECT TOP(50) d.object_id,OBJECT_SCHEMA_NAME(d.object_id) AS schema_name,
       OBJECT_NAME(d.object_id) AS table_name,d.equality_columns,
       d.inequality_columns,d.included_columns,
       s.user_seeks,s.user_scans,s.avg_total_user_cost,s.avg_user_impact,
       s.last_user_seek,s.last_user_scan
FROM sys.dm_db_missing_index_details d
JOIN sys.dm_db_missing_index_groups g ON g.index_handle=d.index_handle
JOIN sys.dm_db_missing_index_group_stats s ON s.group_handle=g.index_group_handle
WHERE d.database_id=DB_ID()
ORDER BY s.avg_total_user_cost*s.avg_user_impact*(s.user_seeks+s.user_scans) DESC;
```

**Yorum / kullanım:** Optimizer tahmini adaydır; kesin kazanç veya eksik index listesi değildir. Örtüşen öneriler, mevcut indexler, write overhead ve included column büyüklüğü incelenir.

**Ön koşul / güvenlik:** DMVden gelen kolon metnini doğrudan CREATE INDEX SQLine yapıştırma; metadata doğrulayıp AST üret. Otomatik oluşturma yok.

**Kaynak:** [M09](#kaynak-m09)


<a id="ms-mon-025"></a>
### MS-MON-025 — İstatistik güncelliği ve modification counter

```yaml
id: MS-MON-025
engine: mssql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Statistics columns SELECT veya uygun nesne yönetim izni

**Girdi:** Yok

```sql
SELECT TOP(300) OBJECT_SCHEMA_NAME(s.object_id) AS schema_name,
       OBJECT_NAME(s.object_id) AS object_name,s.name AS stats_name,
       s.auto_created,s.user_created,s.no_recompute,
       p.last_updated,p.rows,p.rows_sampled,p.modification_counter
FROM sys.stats s
OUTER APPLY sys.dm_db_stats_properties(s.object_id,s.stats_id) p
JOIN sys.objects o ON o.object_id=s.object_id
WHERE o.is_ms_shipped=0
ORDER BY p.modification_counter DESC;
```

**Yorum / kullanım:** NULL last_updated boş tablo/filter veya permission nedeniyle olabilir. Modification counter leading column semantiğini korur; her row değişimiyle birebir aynı sayılmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M17](#kaynak-m17)


<a id="ms-mon-026"></a>
### MS-MON-026 — Backup geçmişi ve son full/diff/log

```yaml
id: MS-MON-026
engine: mssql
category: monitor
priority: P0
risk: R0
scope: msdb
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Self-managed / msdb backup visibility destekleyen MI-RDS; Azure SQL DB için API

**Yetki:** msdb backup history SELECT yetkisi

**Girdi:** database_name:text

```sql
SELECT database_name,
       MAX(CASE WHEN type='D' AND is_copy_only=0 THEN backup_finish_date END) AS last_regular_full,
       MAX(CASE WHEN type='D' AND is_copy_only=1 THEN backup_finish_date END) AS last_copy_only_full,
       MAX(CASE WHEN type='I' THEN backup_finish_date END) AS last_diff,
       MAX(CASE WHEN type='L' THEN backup_finish_date END) AS last_log
FROM msdb.dbo.backupset
WHERE database_name=:database_name
GROUP BY database_name;

SELECT TOP(50) backup_set_id,type,backup_start_date,backup_finish_date,
       backup_size,compressed_backup_size,is_copy_only,has_backup_checksums,
       is_damaged,first_lsn,last_lsn,database_backup_lsn
FROM msdb.dbo.backupset WHERE database_name=:database_name
ORDER BY backup_finish_date DESC;
```

**Yorum / kullanım:** History kaydı backup dosyasının halen mevcut veya restore edilebilir olduğunu ispatlamaz. AGde backup başka replika üzerinde alınmış olabilir; tek node geçmişi eksik olabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M13](#kaynak-m13)


<a id="ms-mon-027"></a>
### MS-MON-027 — Agent job son durumları

```yaml
id: MS-MON-027
engine: mssql
category: monitor
priority: P0
risk: R0
scope: msdb
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server Agent bulunan edition / MI; Azure SQL DB yok

**Yetki:** msdb Agent job görünürlüğü

**Girdi:** Yok

```sql
SELECT j.job_id,j.name,j.enabled,h.run_status,h.run_date,h.run_time,
       h.run_duration,LEFT(h.message,2000) AS last_message
FROM msdb.dbo.sysjobs j
OUTER APPLY (
  SELECT TOP(1) run_status,run_date,run_time,run_duration,message
  FROM msdb.dbo.sysjobhistory h
  WHERE h.job_id=j.job_id AND h.step_id=0
  ORDER BY h.instance_id DESC
) h
ORDER BY j.name;
```

**Yorum / kullanım:** run_duration HHMMSS sayısıdır; milisaniye veya saat olarak doğrudan kullanma. Son geçmiş satırı o an çalışan jobı göstermez; activity sorgusu ayrı.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12) [C07](#kaynak-c07)


<a id="ms-mon-028"></a>
### MS-MON-028 — Always On replica ve queue durumu

```yaml
id: MS-MON-028
engine: mssql
category: monitor
priority: P0
risk: R0
scope: availability_group
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** HADR etkin self-managed/uygun managed exposure; Azure SQL built-in HA için otomatik varsayma

**Yetki:** 2019 server state / 2022+ performance state

**Girdi:** Yok

```sql
SELECT ag.name AS availability_group,ar.replica_server_name,
       adc.database_name,drs.is_local,drs.is_primary_replica,
       drs.synchronization_state_desc,drs.synchronization_health_desc,
       drs.database_state_desc,drs.is_suspended,drs.suspend_reason_desc,
       drs.log_send_queue_size,drs.log_send_rate,
       drs.redo_queue_size,drs.redo_rate,drs.last_commit_time
FROM sys.dm_hadr_database_replica_states drs
JOIN sys.availability_replicas ar ON ar.replica_id=drs.replica_id
JOIN sys.availability_groups ag ON ag.group_id=drs.group_id
JOIN sys.availability_databases_cluster adc
  ON adc.group_database_id=drs.group_database_id
ORDER BY ag.name,adc.database_name,ar.replica_server_name;
```

**Yorum / kullanım:** Queue size KB, rate KB/s. Süre tahmini yalnız rate>0 ve stable ölçümde; 0 rate için sonsuz/kritik diye otomatik sabit çıkarım yapma. Secondaryde remote satır görünürlüğü sınırlıdır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M14](#kaynak-m14)


<a id="ms-mon-029"></a>
### MS-MON-029 — Login, user ve role üyelikleri

```yaml
id: MS-MON-029
engine: mssql
category: security
priority: P0
risk: R0
scope: server_and_current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Security metadata görünürlüğü

**Girdi:** Yok

```sql
SELECT name,type_desc,is_disabled,default_database_name
FROM sys.server_principals
WHERE type IN ('S','U','G','E','X');
SELECT r.name AS role_name,m.name AS member_name
FROM sys.server_role_members x
JOIN sys.server_principals r ON r.principal_id=x.role_principal_id
JOIN sys.server_principals m ON m.principal_id=x.member_principal_id;
SELECT r.name AS role_name,m.name AS member_name
FROM sys.database_role_members x
JOIN sys.database_principals r ON r.principal_id=x.role_principal_id
JOIN sys.database_principals m ON m.principal_id=x.member_principal_id;
```

**Yorum / kullanım:** Azure contained profilde ilk iki sorgu atlanır/ayrı adapter. sysadmin/securityadmin/db_owner gibi haklar açıklanır; fixed role izinleri yalnız sys.database_permissions satırlarıyla tam görülemez.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M03](#kaynak-m03) [M15](#kaynak-m15)


<a id="ms-mon-030"></a>
### MS-MON-030 — Explicit DB permission matrisi

```yaml
id: MS-MON-030
engine: mssql
category: security
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT grantee.name AS grantee,grantor.name AS grantor,
       p.class_desc,p.major_id,p.minor_id,p.permission_name,p.state_desc,
       CASE WHEN p.class=1 THEN OBJECT_SCHEMA_NAME(p.major_id)
            WHEN p.class=3 THEN SCHEMA_NAME(p.major_id) END AS schema_name,
       CASE WHEN p.class=1 THEN OBJECT_NAME(p.major_id) END AS object_name
FROM sys.database_permissions p
JOIN sys.database_principals grantee ON grantee.principal_id=p.grantee_principal_id
JOIN sys.database_principals grantor ON grantor.principal_id=p.grantor_principal_id
ORDER BY grantee.name,p.class,p.major_id,p.minor_id;
```

**Yorum / kullanım:** Column permission minor_id ile ayrı çözülür. Explicit izinler effective izinlerin tamamı değildir: role, ownership, DENY ve chained module etkileri ayrıca test edilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M15](#kaynak-m15)


<a id="ms-mon-031"></a>
### MS-MON-031 — Login eşleşmesi eksik SQL user adayları

```yaml
id: MS-MON-031
engine: mssql
category: security
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Server logins görülebilen SQL Server/MI/RDS profili

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT dp.name,dp.type_desc,dp.authentication_type_desc,dp.sid
FROM sys.database_principals dp
LEFT JOIN sys.server_principals sp ON sp.sid=dp.sid
WHERE dp.type='S' AND dp.authentication_type_desc='INSTANCE'
  AND dp.principal_id>4 AND sp.sid IS NULL;
```

**Yorum / kullanım:** Contained kullanıcılar instance orphan gibi gösterilmez. Server metadata visibility eksikse görünmeyen logini yok sanma.

**Ön koşul / güvenlik:** Remap yalnız onaylı login SID ile; isim benzerliği yeterli değildir.

**Kaynak:** [M02](#kaynak-m02) [M15](#kaynak-m15)


<a id="ms-mon-032"></a>
### MS-MON-032 — Şifreli bağlantı kontrolü

```yaml
id: MS-MON-032
engine: mssql
category: security
priority: P1
risk: R0
scope: server_or_visible_scope
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT c.session_id,s.login_name,s.program_name,c.net_transport,
       c.encrypt_option,c.auth_scheme,c.client_net_address,c.connect_time
FROM sys.dm_exec_connections c
LEFT JOIN sys.dm_exec_sessions s ON s.session_id=c.session_id;
```

**Yorum / kullanım:** encrypt_option=TRUE istemci trust-server-certificate davranışını ispatlamaz. Client configurationda hostname/CA doğrulaması ayrıca zorunlu.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-033"></a>
### MS-MON-033 — Kolonlar, identity, computed ve defaultlar

```yaml
id: MS-MON-033
engine: mssql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** object_id:int verified selected table

```sql
SELECT c.column_id,c.name,t.name AS data_type,c.max_length,c.precision,c.scale,
       c.is_nullable,c.is_identity,c.is_computed,c.collation_name,
       dc.definition AS default_definition,cc.definition AS computed_definition
FROM sys.columns c JOIN sys.types t ON t.user_type_id=c.user_type_id
LEFT JOIN sys.default_constraints dc ON dc.object_id=c.default_object_id
LEFT JOIN sys.computed_columns cc ON cc.object_id=c.object_id AND cc.column_id=c.column_id
WHERE c.object_id=:object_id ORDER BY c.column_id;
```

**Yorum / kullanım:** max_length byte değeridir; nvarchar uzunluğunu karakter gibi göstermeden dönüştür. -1 MAX semantiği korunur.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M02](#kaynak-m02)


<a id="ms-mon-034"></a>
### MS-MON-034 — PK/unique/FK metadata

```yaml
id: MS-MON-034
engine: mssql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** object_id:int

```sql
SELECT kc.name,kc.type_desc,kc.unique_index_id
FROM sys.key_constraints kc WHERE kc.parent_object_id=:object_id;
SELECT fk.name,OBJECT_SCHEMA_NAME(fk.parent_object_id) AS source_schema,
       OBJECT_NAME(fk.parent_object_id) AS source_table,
       COL_NAME(fkc.parent_object_id,fkc.parent_column_id) AS source_column,
       OBJECT_SCHEMA_NAME(fk.referenced_object_id) AS target_schema,
       OBJECT_NAME(fk.referenced_object_id) AS target_table,
       COL_NAME(fkc.referenced_object_id,fkc.referenced_column_id) AS target_column,
       fk.is_disabled,fk.is_not_trusted,fkc.constraint_column_id
FROM sys.foreign_keys fk
JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id=fk.object_id
WHERE fk.parent_object_id=:object_id OR fk.referenced_object_id=:object_id
ORDER BY fk.name,fkc.constraint_column_id;
```

**Yorum / kullanım:** Disabled/untrusted FKyi sağlıklı enforced constraint gibi gösterme. Incoming bağımlılıklar destructive preflightta kullanılır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M02](#kaynak-m02)


<a id="ms-mon-035"></a>
### MS-MON-035 — Kontrollü veri önizleme

```yaml
id: MS-MON-035
engine: mssql
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Seçili kolonlarda SELECT

**Girdi:** page_size:int 1..1000; table:qualified; selection:columns; unique_key:single unique metadata key

```sql
SELECT TOP(:page_size) {{columns:selection}}
FROM {{qualified:table}}
ORDER BY {{ident:unique_key}};
```

**Yorum / kullanım:** TOP bound parameter @page_size olarak derlenir. NOLOCK ekleme; dirty/eksik/tekrarlı veri sorunlarıyla güvenilir inceleme amacı çelişir. Composite cursor ayrı AST.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M02](#kaynak-m02)


<a id="ms-mon-036"></a>
### MS-MON-036 — Azure SQL Database kaynak kullanım aralığı

```yaml
id: MS-MON-036
engine: mssql
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Yalnız Azure SQL Database; desteklenen kolonlar probe edilir

**Yetki:** İlgili Azure DB gözlem yetkileri

**Girdi:** Yok

```sql
SELECT TOP(120) end_time,avg_cpu_percent,avg_data_io_percent,
       avg_log_write_percent,avg_memory_usage_percent,
       max_worker_percent,max_session_percent
FROM sys.dm_db_resource_stats ORDER BY end_time DESC;
```

**Yorum / kullanım:** CPU/data IO/log write/worker/session yüzdeleri service limitlerine göre yorumlanır. Bu metrikler self-managed OS CPU ile aynı denominator değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12) [C07](#kaynak-c07)


<a id="ms-mon-037"></a>
### MS-MON-037 — Aktif bakım/backup/restore ilerlemesi

```yaml
id: MS-MON-037
engine: mssql
category: monitor
priority: P0
risk: R0
scope: server_or_visible_scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Requests DMV yetkileri

**Girdi:** Yok

```sql
SELECT session_id,command,status,percent_complete,start_time,
       total_elapsed_time,wait_type,blocking_session_id
FROM sys.dm_exec_requests
WHERE command LIKE 'BACKUP%' OR command LIKE 'RESTORE%'
   OR command LIKE 'DBCC%' OR command='KILLED/ROLLBACK'
ORDER BY start_time;
```

**Yorum / kullanım:** percent_complete sadece belirli komutlarda anlamlıdır. estimated_completion_time internal alanını güvenilir ETA olarak sunma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M06](#kaynak-m06)


<a id="ms-mon-038"></a>
### MS-MON-038 — Disabled indexler ve untrusted constraintler

```yaml
id: MS-MON-038
engine: mssql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT OBJECT_SCHEMA_NAME(object_id) AS schema_name,OBJECT_NAME(object_id) AS table_name,
       name,type_desc FROM sys.indexes WHERE is_disabled=1;
SELECT 'FOREIGN_KEY' AS constraint_type,name,OBJECT_SCHEMA_NAME(parent_object_id) AS schema_name,
       OBJECT_NAME(parent_object_id) AS table_name,is_disabled,is_not_trusted
FROM sys.foreign_keys WHERE is_disabled=1 OR is_not_trusted=1
UNION ALL
SELECT 'CHECK',name,OBJECT_SCHEMA_NAME(parent_object_id),OBJECT_NAME(parent_object_id),
       is_disabled,is_not_trusted FROM sys.check_constraints
WHERE is_disabled=1 OR is_not_trusted=1;
```

**Yorum / kullanım:** Bilinçli ETL/maintenance durumu olabilir; yeniden enable+validate işlemi scan/lock yaratabilir, otomatik uygulanmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M12](#kaynak-m12)


<a id="ms-mon-039"></a>
### MS-MON-039 — VLF ve log truncation ayrıntısı

```yaml
id: MS-MON-039
engine: mssql
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili log DMV state/performance

**Girdi:** Yok

```sql
SELECT database_id,recovery_model,log_min_lsn,log_end_lsn,
       total_vlf_count,active_vlf_count,total_log_size_mb,
       active_log_size_mb,log_truncation_holdup_reason,
       log_backup_time,log_since_last_log_backup_mb
FROM sys.dm_db_log_stats(DB_ID());
```

**Yorum / kullanım:** VLF fazlalığı tek başına shrink önerisi değildir; file growth/backup/transaction davranışıyla incelenir. Azure görünürlük ve destek ayrı test edilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M07](#kaynak-m07) [M12](#kaynak-m12)


<a id="ms-mon-040"></a>
### MS-MON-040 — Sorgu deadlock olayları: event session keşfi

```yaml
id: MS-MON-040
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Self-managed/MI server XE; Azure DB için database-scoped XE adapter

**Yetki:** Extended Events metadata/read permission

**Girdi:** approved_session_name:text allowlist

```sql
SELECT name,startup_state FROM sys.server_event_sessions
WHERE name='system_health' OR name=:approved_session_name;
SELECT s.name,t.target_name,CAST(t.target_data AS xml) AS target_metadata
FROM sys.dm_xe_sessions s JOIN sys.dm_xe_session_targets t
  ON t.event_session_address=s.address
WHERE s.name=:approved_session_name;
```

**Yorum / kullanım:** Deadlock graphı yalnız canlı locks sorgusundan geriye dönük çıkaramazsın. Event_file parser hedef yolunu bu metadata üzerinden alır; raw arbitrary file read açılmaz.

**Ön koşul / güvenlik:** target_data dosya yolları ve hassas olay bilgisi içerebilir; boyut limiti. Event session oluşturma burada yapılmaz.

**Kaynak:** [M12](#kaynak-m12) [C07](#kaynak-c07)


## 6.3 SQL Server kontrollü bakım işlemleri


<a id="ms-ops-001"></a>
### MS-OPS-001 — Oturumu sonlandırma

```yaml
id: MS-OPS-001
engine: mssql
category: maintenance
priority: P0
risk: R2
scope: server_or_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** ALTER ANY CONNECTION; Azure SQL Database uygun KILL DATABASE CONNECTION yetkisi

**Girdi:** session_id:int>0; expected login_time ve request identity preflight girdisidir

```sql
KILL {{int:session_id}};
```

**Yorum / kullanım:** Clientın kendi querysini iptal etmek için önce driver cancellation kullanılır; KILL bütün sessionı sonlandırır, transaction rollback uzun sürebilir.

**Ön koşul / güvenlik:** session_id+login_time yeniden doğrula; @@SPID, system, backup/HA ve protected kullanıcı denylist. T-SQL KILL id ile atomic identity compare sağlamaz; yarış riskini UIda gizleme.

**Kaynak:** [M16](#kaynak-m16)


<a id="ms-ops-002"></a>
### MS-OPS-002 — Rollback ilerleme kontrolü

```yaml
id: MS-OPS-002
engine: mssql
category: monitor
priority: P1
risk: R0
scope: server_or_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** KILL/status gözlem koşulları

**Girdi:** session_id:int

```sql
KILL {{int:session_id}} WITH STATUSONLY;
```

**Yorum / kullanım:** Yeni kill işlemi başlatmadan daha önce sonlandırılmış transactionın rollback durumunu sorgular. Mesaj sonucu normalized olarak işlenir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [M16](#kaynak-m16)


<a id="ms-ops-003"></a>
### MS-OPS-003 — Tek istatistiği güncelleme

```yaml
id: MS-OPS-003
engine: mssql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** ALTER table/uygun bakım yetkisi

**Girdi:** table:qualified; statistics_name:metadata identifier

```sql
UPDATE STATISTICS {{qualified:table}} {{ident:statistics_name}};
```

**Yorum / kullanım:** FULLSCAN otomatik eklenmez; örnekleme/persisted sample ve iş yükü planı değerlendirilir. Plan değişikliği olabilir.

**Ön koşul / güvenlik:** Tek hedef; Query Store öncesi/sonrası kıyas ve bakım bütçesi. Bütün DB sp_updatestats tek tık varsayılan değil.

**Kaynak:** [M17](#kaynak-m17)


<a id="ms-ops-004"></a>
### MS-OPS-004 — Tek index reorganize

```yaml
id: MS-OPS-004
engine: mssql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** ALTER table

**Girdi:** index_name:identifier; table:qualified

```sql
ALTER INDEX {{ident:index_name}} ON {{qualified:table}} REORGANIZE;
```

**Yorum / kullanım:** Log/IO maliyeti vardır; bütün frag oranlarına otomatik çözüm değildir. İstatistik güncellemesiyle aynı işlem sayılmaz.

**Ön koşul / güvenlik:** Index türü, disabled/allow_page_locks koşulları ve disk/log payı; sadece uygun rowstore profiliyle başlat.

**Kaynak:** [M08](#kaynak-m08)


<a id="ms-ops-005"></a>
### MS-OPS-005 — Tek index offline rebuild

```yaml
id: MS-OPS-005
engine: mssql
category: maintenance
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** ALTER table

**Girdi:** index_name:identifier; table:qualified; maxdop:bounded int

```sql
ALTER INDEX {{ident:index_name}} ON {{qualified:table}}
REBUILD WITH (MAXDOP={{int:maxdop}});
```

**Yorum / kullanım:** Offline operation güçlü lock ve ek alan/log ihtiyacı yaratır. ONLINE/RESUMABLE desteği edition/sürüm/index türüne bağlı ayrı varyanttır; burada sessizce eklenmez.

**Ön koşul / güvenlik:** Bakım penceresi, log send queue ve temp/disk payı. %30 eşiğini evrensel rebuild kuralı olarak uygulama.

**Kaynak:** [M08](#kaynak-m08)


<a id="ms-ops-006"></a>
### MS-OPS-006 — DBCC CHECKDB fiziksel doğrulama

```yaml
id: MS-OPS-006
engine: mssql
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** sysadmin/db_owner veya enginein zorunlu yetkisi

**Girdi:** db_name:identifier

```sql
DBCC CHECKDB ({{ident:db_name}}) WITH PHYSICAL_ONLY,NO_INFOMSGS;
```

**Yorum / kullanım:** Hafif günlük telemetry değil; IO, snapshot ve temp alan maliyeti olabilir. PHYSICAL_ONLY tam mantıksal integrity kontrolünün yerine geçmez.

**Ön koşul / güvenlik:** Restore edilmiş kopyada veya planlı bakımda çalıştır. REPAIR_ALLOW_DATA_LOSS hiçbir otomatik runbooka eklenmez.

**Kaynak:** [M11](#kaynak-m11)


<a id="ms-ops-007"></a>
### MS-OPS-007 — DBCC CHECKDB tam doğrulama

```yaml
id: MS-OPS-007
engine: mssql
category: maintenance
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** CHECKDB için gereken yönetim yetkisi

**Girdi:** db_name:identifier

```sql
DBCC CHECKDB ({{ident:db_name}}) WITH NO_INFOMSGS,ALL_ERRORMSGS;
```

**Yorum / kullanım:** Gerçek integrity taramasıdır. Başarılı restore+CHECKDB kanıtı backup geçmişindeki tarihten daha güçlüdür, ancak her işlevsel uygulama testinin yerine geçmez.

**Ön koşul / güvenlik:** Kaynak ve süre bütçesi; production yerine restore test tercih edilebilir. REPAIR seçenekleri otomatik yok.

**Kaynak:** [M11](#kaynak-m11)


<a id="ms-ops-008"></a>
### MS-OPS-008 — COPY_ONLY checksum full backup

```yaml
id: MS-OPS-008
engine: mssql
category: maintenance
priority: P1
risk: R2
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Self-managed; RDS/MI/Azure farklı backup yolları ve destek koşulları

**Yetki:** BACKUP DATABASE ve backup path servis hesabı izni

**Girdi:** db_name:identifier; unique_backup_path:approved server-side unused path

```sql
BACKUP DATABASE {{ident:db_name}}
TO DISK={{literal:unique_backup_path}}
WITH COPY_ONLY,CHECKSUM,STATS=10;
```

**Yorum / kullanım:** COPY_ONLY mevcut differential basei değiştirmemek için kullanılır. Compression/encryption kuruluşun destek/lisans/politika kararıyla eklenir. Bu tek işlem log backup/PITR planı değildir.

**Ön koşul / güvenlik:** Yeni benzersiz path; mevcut dosyayı INIT/FORMAT ile ezme yok. Hassas backup erişimi/encryption/retention ve restore denemesi planlanır.

**Kaynak:** [M13](#kaynak-m13)


<a id="ms-ops-009"></a>
### MS-OPS-009 — Login devre dışı bırakma

```yaml
id: MS-OPS-009
engine: mssql
category: security
priority: P1
risk: R1
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Server login profili; contained/Entra için ayrı revoke workflow

**Yetki:** ALTER ANY LOGIN/ilgili yönetim hakkı

**Girdi:** login_name:identifier

```sql
ALTER LOGIN {{ident:login_name}} DISABLE;
```

**Yorum / kullanım:** Mevcut sessionları otomatik öldürmez. Sadece login disable ile saklanan Team Vault sırrı yok edilmez.

**Ön koşul / güvenlik:** Owner/system/provider ve aktif servislere etkisi preview. Contained kullanıcıya login varmış gibi uygulama.

**Kaynak:** [M02](#kaynak-m02)


<a id="ms-ops-010"></a>
### MS-OPS-010 — Login parola rotasyonu

```yaml
id: MS-OPS-010
engine: mssql
category: security
priority: P1
risk: R1
scope: server
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** İlgili login parola yönetimi

**Girdi:** login_name:identifier; new_password:secret

```sql
ALTER LOGIN {{ident:login_name}} WITH PASSWORD={{secret:new_password}};
```

**Yorum / kullanım:** Contained user için ALTER USER password varyantı kullanılır; Windows/Entra loginlerde local parola rotasyonu yapılmaz. Vault/app geçişi transaction değildir.

**Ön koşul / güvenlik:** Redaction ve yeni connection doğrulaması. Mevcut parolayı koddan loglama.

**Kaynak:** [M02](#kaynak-m02)


<a id="ms-ops-011"></a>
### MS-OPS-011 — Sadece onaylı stored procedure için EXECUTE

```yaml
id: MS-OPS-011
engine: mssql
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** GRANT EXECUTE verebilme

**Girdi:** procedure:qualified metadata; grantee:identifier

```sql
GRANT EXECUTE ON OBJECT::{{qualified:procedure}} TO {{ident:grantee}};
```

**Yorum / kullanım:** Procedure write yapabilir veya ownership chain/EXECUTE AS üzerinden geniş veri erişimi verebilir. Readonly kimlik için otomatik önerilmez.

**Ön koşul / güvenlik:** Module security review; schema-wide EXECUTE opt-in ve ayrı risk scope.

**Kaynak:** [M04](#kaynak-m04) [M15](#kaynak-m15)


<a id="ms-ops-012"></a>
### MS-OPS-012 — Yetki rolü üyeliğini kaldırma

```yaml
id: MS-OPS-012
engine: mssql
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** SQL Server 2019/2022/2025; Azure varyantı capability kontrolü

**Yetki:** Role membership yönetimi

**Girdi:** role_name,member_user:identifier

```sql
ALTER ROLE {{ident:role_name}} DROP MEMBER {{ident:member_user}};
```

**Yorum / kullanım:** Diğer role/direct/PUBLIC izinleriyle erişim sürebilir; effective negative test zorunlu.

**Ön koşul / güvenlik:** Mevcut olmayan üyelikte planner no-op; fixed/protected rolle çalışırken ekstra policy.

**Kaynak:** [M03](#kaynak-m03)


<a id="ms-ops-013"></a>
### MS-OPS-013 — Kontrollü DROP DATABASE

```yaml
id: MS-OPS-013
engine: mssql
category: maintenance
priority: P2
risk: R3
scope: master
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: autocommit_required
```

**Hedef:** Self-managed/uygun managed SQL yolu; cloud life-cycle API ayrı

**Yetki:** DB drop yetkisi

**Girdi:** db_name:identifier

```sql
DROP DATABASE {{ident:db_name}};
```

**Yorum / kullanım:** DBnin verisi kalıcı olarak kaybolur. Aktif bağlantıları zorla düşüren SINGLE_USER/ROLLBACK IMMEDIATE otomatik eklenmez.

**Ön koşul / güvenlik:** Sistem DB denylist; backup/restore ve AG/replication bağımlılık planı; ayrı master connection, hedef adı yazdırma ve approval.

**Kaynak:** [M01](#kaynak-m01)


<a id="bolum-7"></a>
# 7. Oracle SQL kataloğu

## 7.1 Uygulama alanı, PDB ve sahiplik

**Varsayılan senaryo:** Mevcut, READ WRITE durumundaki doğru PDB içinde uygulama schema'sı oluştur. Oracle schema = user namespace; PostgreSQL'deki database ile birebir eşitleme. "Yeni database" sihirbazında `mevcut PDB içinde schema` ile `yeni PDB` açıkça ayrılır. CDB kurulumu, controlfile/redo/character set/OS kurulumu gerektiren DBCA işi olarak ayrı kapsamdır; masaüstü uygulaması rastgele bir `CREATE DATABASE` scripti çalıştırmaz.

Önerilen kimlikler: `APP_OWNER` (NO AUTHENTICATION schema hesabı), `APP_MIGRATOR` (yalnızca kontrollü owner proxy erişimi), `APP_USER`, `APP_READER`, `APP_RW`, `APP_RO`, `APP_MONITOR`. Bunlar rol isim örnekleridir, sabit isim zorunluluğu değildir. Schema-only owner için proxy bağlantı desteklenmeyen sürücüde bu mod **unsupported** olur; gizlice owner hesabına parola atama. Owner proxy yetkisi sahibi kadar güçlüdür; runtime hesabına verilmez. [O01](#kaynak-o01) [O02](#kaynak-o02)

Oracle 19c temelinde yeni nesneler için PostgreSQL benzeri `ALTER DEFAULT PRIVILEGES` yoktur. Onaylı nesnelere nesne bazlı grant ver ve her migration sonunda grant reconciliation çalıştır. Sonraki Oracle sürümlerinde schema privileges ayrı capability/semantik testiyle eklenebilir; 19c sorgularına yeni syntax karıştırma. [O03](#kaynak-o03) [O11](#kaynak-o11)

**PDB güvenliği:** `CON_NAME`, `CON_ID`, service, DB unique name ve kullanıcı fingerprint'i onay ekranında görünür. Root/PDB$SEED/SYS/SYSTEM/Oracle-maintained account hedefleri normal provisioning ve drop ekranında denylist'tedir. DDL implicit commit yapabildiğinden adımları ayrı connection ve açık kısmi-başarı günlüğüyle yürüt.


<a id="or-prov-001"></a>
### OR-PROV-001 — Container, session ve sürüm keşfi

```yaml
id: OR-PROV-001
engine: oracle
category: discovery
priority: P0
risk: R0
scope: session
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT SYS_CONTEXT('USERENV','DB_NAME') AS db_name,
       SYS_CONTEXT('USERENV','CON_NAME') AS con_name,
       SYS_CONTEXT('USERENV','CON_ID') AS con_id,
       SYS_CONTEXT('USERENV','SERVICE_NAME') AS service_name,
       SYS_CONTEXT('USERENV','SESSION_USER') AS session_user,
       SYS_CONTEXT('USERENV','CURRENT_SCHEMA') AS current_schema,
       SYS_CONTEXT('USERENV','PROXY_USER') AS proxy_user
FROM dual;
SELECT product, version, status FROM product_component_version;
```

**Yorum / kullanım:** CURRENT_SCHEMA değiştirmek owner yetkisi veya kimliği vermez. Gerçek proxy session açıldığını PROXY_USER ve SESSION_USER ile doğrula.

**Ön koşul / güvenlik:** Beklenen PDB/service ile uyuşmazlıkta bütün write adımlarını durdur.

**Kaynak:** [O01](#kaynak-o01) [O02](#kaynak-o02)


<a id="or-prov-002"></a>
### OR-PROV-002 — Opsiyonel yeni PDB — yalnızca self-managed OMF

```yaml
id: OR-PROV-002
engine: oracle
category: provision
priority: P2
risk: R2
scope: CDB$ROOT
cost: light
timeout_seconds: 120
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Self-managed Oracle CDB; OMF konfigürasyonu hazır; edition/PDB lisans hakkı doğrulanmış

**Yetki:** Yetkili CDB yöneticisi; CREATE PLUGGABLE DATABASE ve lifecycle yetkileri

**Girdi:** pdb_name,pdb_admin:identifier; pdb_admin_password:secret

```sql
SELECT name, value FROM v$parameter WHERE name = 'db_create_file_dest';
-- Aşağıdaki DDL ancak OMF hedefi, disk ve lisans ön koşulları doğrulandıysa:
CREATE PLUGGABLE DATABASE {{ident:pdb_name}}
  ADMIN USER {{ident:pdb_admin}} IDENTIFIED BY {{secret:pdb_admin_password}};
ALTER PLUGGABLE DATABASE {{ident:pdb_name}} OPEN;
ALTER PLUGGABLE DATABASE {{ident:pdb_name}} SAVE STATE;
```

**Yorum / kullanım:** Her DDL ayrı adım; sonrasında yeni PDB service ile yeni bağlantı aç. PDB admin runtime owner değildir. PDB sayısı/lisans hakkı sürüm ve sözleşmeye bağlıdır.

**Ön koşul / güvenlik:** RDS/Azure managed Oracle hizmetine generic uygulanmaz. OMF yoksa bu varyant kapalı; dosya adı dönüşümü uydurma. Başarısızlıkta otomatik DROP yok.

**Kaynak:** [O04](#kaynak-o04) [O10](#kaynak-o10)


<a id="or-prov-003"></a>
### OR-PROV-003 — Opsiyonel uygulama tablespace — OMF

```yaml
id: OR-PROV-003
engine: oracle
category: provision
priority: P1
risk: R2
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** CREATE TABLESPACE

**Girdi:** tablespace_name:identifier; initial_mb,growth_mb,max_mb:bounded integers

```sql
CREATE TABLESPACE {{ident:tablespace_name}}
  DATAFILE SIZE {{int:initial_mb}}M
  AUTOEXTEND ON NEXT {{int:growth_mb}}M MAXSIZE {{int:max_mb}}M
  EXTENT MANAGEMENT LOCAL
  SEGMENT SPACE MANAGEMENT AUTO;
```

**Yorum / kullanım:** Mevcut onaylı tablespace kullanımı varsayılan olabilir. AUTOEXTEND üst sınırı sonludur; fiziksel disk/ASM/provider storage ile ayrı kontrol edilir. TEMP tablespace ayrıca seçilir, burada yaratılmaz.

**Ön koşul / güvenlik:** OMF etkin, PDB doğru, READ WRITE, max>=initial, block size ve smallfile/bigfile limitleri testli. Mutlak dosya yollarını istemciden tahmin etme.

**Kaynak:** [O18](#kaynak-o18)


<a id="or-prov-004"></a>
### OR-PROV-004 — NO AUTHENTICATION schema owner ve sınırlı kota

```yaml
id: OR-PROV-004
engine: oracle
category: provision
priority: P0
risk: R1
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** CREATE USER ve listelenen system privileges verebilme

**Girdi:** owner_user,data_tablespace,temp_tablespace:identifier; quota_mb:bounded integer

```sql
CREATE USER {{ident:owner_user}} NO AUTHENTICATION
  DEFAULT TABLESPACE {{ident:data_tablespace}}
  TEMPORARY TABLESPACE {{ident:temp_tablespace}}
  QUOTA {{int:quota_mb}}M ON {{ident:data_tablespace}};
GRANT CREATE SESSION, CREATE TABLE, CREATE VIEW, CREATE SEQUENCE,
      CREATE PROCEDURE, CREATE TRIGGER, CREATE TYPE
TO {{ident:owner_user}};
```

**Yorum / kullanım:** CREATE SESSION proxy akışı için; NO AUTHENTICATION normal parola loginini açmaz. Projenin kullanmadığı CREATE TRIGGER/TYPE/PROCEDURE haklarını planner kaldırır. DBA, RESOURCE, ANY ve UNLIMITED TABLESPACE verilmez.

**Ön koşul / güvenlik:** Owner var ise auth türünü/sahipliği sessiz değiştirme; quota ve tablespace conflict çıkar. Oracle-maintained/common user oluşturma.

**Kaynak:** [O01](#kaynak-o01) [O02](#kaynak-o02) [O03](#kaynak-o03)


<a id="or-prov-005"></a>
### OR-PROV-005 — Kontrollü migration proxy hesabı

```yaml
id: OR-PROV-005
engine: oracle
category: provision
priority: P0
risk: R1
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** CREATE USER, ALTER USER, CREATE SESSION verebilme

**Girdi:** migrator_user,owner_user:identifier; migrator_password:secret

```sql
CREATE USER {{ident:migrator_user}} IDENTIFIED BY {{secret:migrator_password}};
GRANT CREATE SESSION TO {{ident:migrator_user}};
ALTER USER {{ident:owner_user}} GRANT CONNECT THROUGH {{ident:migrator_user}};
```

**Yorum / kullanım:** Sürücünün desteklediği proxy API ile migrator[owner] semantiği kurulur. Owner schema sessionında migration yürür; owner parolasını saklama. Proxy hakkı CI/CD kasasında tutulur, appuserda değil.

**Ön koşul / güvenlik:** Proxy desteği integration testten geçmeli. CURRENT_SCHEMA ayarlamak proxy yerine geçmez. Tam owner yetkisi onay ekranında görünür.

**Kaynak:** [O01](#kaynak-o01) [O02](#kaynak-o02) [O03](#kaynak-o03)


<a id="or-prov-006"></a>
### OR-PROV-006 — Runtime ve readonly kimlikleri / permission rolleri

```yaml
id: OR-PROV-006
engine: oracle
category: provision
priority: P0
risk: R1
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** CREATE ROLE, CREATE USER ve grant yetkileri

**Girdi:** rw_role,ro_role,app_user,reader_user:identifier; app_password,reader_password:secret

```sql
CREATE ROLE {{ident:rw_role}};
CREATE ROLE {{ident:ro_role}};
CREATE USER {{ident:app_user}} IDENTIFIED BY {{secret:app_password}};
CREATE USER {{ident:reader_user}} IDENTIFIED BY {{secret:reader_password}};
GRANT CREATE SESSION TO {{ident:app_user}}, {{ident:reader_user}};
GRANT {{ident:rw_role}} TO {{ident:app_user}};
GRANT {{ident:ro_role}} TO {{ident:reader_user}};
```

**Yorum / kullanım:** Runtime/reader için CREATE TABLE, ANY ve tablespace kotası verilmez. Schema-qualified isimlerle erişilir. Rol grantinde ADMIN OPTION yoktur.

**Ön koşul / güvenlik:** DBA/RESOURCE/CONNECT sabit rol shortcutları kullanma; password profile kurum policy ile uyumlu. Parolaları auditten çıkar.

**Kaynak:** [O01](#kaynak-o01) [O03](#kaynak-o03)


<a id="or-prov-007"></a>
### OR-PROV-007 — Onaylı tabloda RW ve gerçekten dar RO grantleri

```yaml
id: OR-PROV-007
engine: oracle
category: provision
priority: P0
risk: R1
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Nesne sahibi veya ilgili grant yetkisi

**Girdi:** table:qualified metadata table; rw_role,ro_role:identifier

```sql
GRANT SELECT, INSERT, UPDATE, DELETE ON {{qualified:table}} TO {{ident:rw_role}};
GRANT READ ON {{qualified:table}} TO {{ident:ro_role}};
```

**Yorum / kullanım:** READ, SELECT FOR UPDATE gibi kilitleyen okuma yetkileri vermeyen dar tercihtir. View, materialized view, editioning view ve foreign objectlar nesne türüne göre ayrı test edilir. Owner tarafındaki tüm tablolarını otomatik paylaşma; include/exclude nesne politikası uygula.

**Ön koşul / güvenlik:** WITH GRANT OPTION yok; onaylı schema ve tablo listesi dışına çıkma. VPD/Database Vault/diğer güvenlik kontrolleri ayrıca erişimi kısıtlayabilir.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-prov-008"></a>
### OR-PROV-008 — Gerekli sequence / package erişimi

```yaml
id: OR-PROV-008
engine: oracle
category: provision
priority: P1
risk: R1
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Nesne sahibi / grant yetkisi

**Girdi:** sequence,routine:qualified metadata; rw_role:identifier

```sql
-- İki ayrı opsiyonel action variantı; her biri ayrı allowlist ile:
GRANT SELECT ON {{qualified:sequence}} TO {{ident:rw_role}};
GRANT EXECUTE ON {{qualified:routine}} TO {{ident:rw_role}};
```

**Yorum / kullanım:** Oracle sequence SELECT, NEXTVAL tüketebilme etkisi taşır; readonly kimliğe otomatik verilmez. Package EXECUTE bütün public subprogramları kapsayabilir. Definer-rights kodun ihtiyacı olan cross-schema haklar role üzerinden değil doğrudan owner hesabına gerekebilir; bunu ayrı izin analizi çözsün.

**Ön koşul / güvenlik:** Bütün paketlere toplu EXECUTE yok; runtime hesabının gerçekten çağıracağı allowlist ve kod güvenlik incelemesi.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-prov-009"></a>
### OR-PROV-009 — Migration sonrası nesne grant reconciliation girdisi

```yaml
id: OR-PROV-009
engine: oracle
category: provision_plan
priority: P0
risk: R0
scope: PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Owner nesne görünürlüğü + DBA_TAB_PRIVS SELECT; sınırlı alternatif ayrı

**Girdi:** owner_user,rw_role,ro_role:text exact case

```sql
SELECT owner, table_name FROM all_tables
WHERE owner = :owner_user AND nested = 'NO' AND secondary = 'N'
ORDER BY table_name;
SELECT owner, table_name, grantee, privilege, grantable
FROM dba_tab_privs
WHERE owner = :owner_user
  AND grantee IN (:rw_role, :ro_role)
ORDER BY table_name, grantee, privilege;
```

**Yorum / kullanım:** İki sonucu include/exclude policy ile birleştir; eksik grantleri OR-PROV-007 üzerinden planla. Fazla izinler için ayrı revoke diff ve onay üret. Migration başarılı sayılmadan future-object permission testleri geçsin.

**Ön koşul / güvenlik:** Sadece nesne listesini görmek yeterli değil: view/sequence/routine listeleri ayrı ve kontrollü. Trigger ile otomatik grant sistemi kurma; DDL commit ve yetki riskini artırma.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-prov-010"></a>
### OR-PROV-010 — En az ayrıcalıklı monitoring hesabı — başlangıç profili

```yaml
id: OR-PROV-010
engine: oracle
category: provision
priority: P0
risk: R1
scope: PDB / izin kapsamı ayrı
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** SYS view erişimlerini yetkili şekilde verebilen DBA; providerda desteklenen grant yolu

**Girdi:** monitor_user:identifier; monitor_password:secret

```sql
CREATE USER {{ident:monitor_user}} IDENTIFIED BY {{secret:monitor_password}};
GRANT CREATE SESSION TO {{ident:monitor_user}};
GRANT SELECT ON SYS.V_$INSTANCE TO {{ident:monitor_user}};
GRANT SELECT ON SYS.V_$DATABASE TO {{ident:monitor_user}};
GRANT SELECT ON SYS.V_$RESOURCE_LIMIT TO {{ident:monitor_user}};
GRANT SELECT ON SYS.V_$SESSION TO {{ident:monitor_user}};
GRANT SELECT ON SYS.DBA_TABLESPACES TO {{ident:monitor_user}};
GRANT SELECT ON SYS.DBA_TABLESPACE_USAGE_METRICS TO {{ident:monitor_user}};
```

**Yorum / kullanım:** Bu yalnızca başlangıç profilidir; tüm katalog için tam yetki değildir. Seçilen her action bağımlılığından explicit grants üret. RAC kontrollerinde GV_$SESSION vb. gerekir. SYS.V_$ temel view adına grant verilir, public synonym V$ adına değil.

**Ön koşul / güvenlik:** SELECT ANY DICTIONARY, SELECT_CATALOG_ROLE ve DBA default verilmez. Query text/session bilgileri hassastır. CDB çapı gerekmedikçe CONTAINER=ALL yok. AWR/ASH bağımlılıkları lisans onayı olmadan grant profiline girmez.

**Kaynak:** [O03](#kaynak-o03) [O06](#kaynak-o06) [O08](#kaynak-o08) [O10](#kaynak-o10)


<a id="or-prov-011"></a>
### OR-PROV-011 — Gerçek app/reader/migrator bağlantısıyla yetki doğrulama

```yaml
id: OR-PROV-011
engine: oracle
category: security
priority: P0
risk: R0
scope: session
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** Yok

```sql
SELECT SYS_CONTEXT('USERENV','SESSION_USER') AS session_user,
       SYS_CONTEXT('USERENV','PROXY_USER') AS proxy_user,
       SYS_CONTEXT('USERENV','CURRENT_SCHEMA') AS current_schema
FROM dual;
SELECT privilege FROM session_privs ORDER BY privilege;
SELECT role FROM session_roles ORDER BY role;
SELECT owner, table_name, privilege, grantable
FROM user_tab_privs_recd ORDER BY owner, table_name, privilege;
```

**Yorum / kullanım:** USER_TAB_PRIVS_RECD doğrudan grantleri gösterir; bütün role/PUBLIC zincirinin veya row-level policy etkisinin tam sonucu değildir. Actual login ile izinli/yasak senaryoları ayrı disposable ortamda çalıştır.

**Ön koşul / güvenlik:** Admin sessionındaki katalog görüntüsünü runtime kullanıcısının başarılı erişim testi diye sunma.

**Kaynak:** [O02](#kaynak-o02) [O03](#kaynak-o03)


## 7.2 Oracle günlük ve kritik kontroller

`V$` yerel instance, `GV$` instance kümesi bağlamındadır. RAC ve CDB kapsamları yetki/edition/platform ile doğrulanır. PDB içinde görünmeyen root verisini sıfır sayma; `INST_ID` ve `CON_ID` anahtarlarını sonuç modelinde koru.


<a id="or-mon-001"></a>
### OR-MON-001 — Instance ve database rol / açık durumu

```yaml
id: OR-MON-001
engine: oracle
category: monitor
priority: P0
risk: R0
scope: instance/database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$INSTANCE, V_$DATABASE SELECT

**Girdi:** Yok

```sql
SELECT instance_name, host_name, version, startup_time, status,
       database_status, instance_role FROM v$instance;
SELECT dbid, name, db_unique_name, cdb, open_mode, database_role,
       log_mode, force_logging, protection_mode, protection_level
FROM v$database;
```

**Yorum / kullanım:** Standby/READ ONLY bilgisi write butonlarını kapatır. DATABASE_ROLE otomatik failover kararı değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-002"></a>
### OR-MON-002 — PDB listesi ve açık durumu

```yaml
id: OR-MON-002
engine: oracle
category: monitor
priority: P0
risk: R0
scope: CDB/PDB visibility
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$PDBS SELECT

**Girdi:** Yok

```sql
SELECT con_id, name, open_mode, restricted, total_size
FROM v$pdbs ORDER BY con_id;
```

**Yorum / kullanım:** CDB root kapsamı ayrı admin bağlantısıdır; tek PDB loginin tüm PDBleri listelemesi beklenmez. TOTAL_SIZE host filesystem free değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-003"></a>
### OR-MON-003 — Sessions / processes kaynak limitleri

```yaml
id: OR-MON-003
engine: oracle
category: monitor
priority: P0
risk: R0
scope: instance
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$RESOURCE_LIMIT SELECT

**Girdi:** Yok

```sql
SELECT resource_name, current_utilization, max_utilization,
       initial_allocation, limit_value
FROM v$resource_limit
WHERE resource_name IN ('processes','sessions','transactions')
ORDER BY resource_name;
```

**Yorum / kullanım:** LIMIT_VALUE UNLIMITED metni olabilir, sayıya kör cast etme. MAX_UTILIZATION anlık değil geçmiş tepedir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-004"></a>
### OR-MON-004 — Aktif kullanıcı oturumları

```yaml
id: OR-MON-004
engine: oracle
category: monitor
priority: P0
risk: R0
scope: RAC/PDB visible scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SESSION SELECT

**Girdi:** row_limit:int 1..1000

```sql
SELECT inst_id, con_id, sid, serial#, username, status, service_name,
       machine, program, module, action, logon_time, sql_id,
       sql_child_number, sql_exec_start, event, wait_class, state,
       seconds_in_wait, blocking_instance, blocking_session
FROM gv$session
WHERE type = 'USER' AND username IS NOT NULL
ORDER BY CASE WHEN status = 'ACTIVE' THEN 0 ELSE 1 END,
         sql_exec_start NULLS LAST, inst_id, sid
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** ACTIVE her zaman CPUda çalışıyor demek değildir; wait_class/state ile birlikte göster. SQL_EXEC_START NULL olabilir. SQL texti ikinci istekte al.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O06](#kaynak-o06)


<a id="or-mon-005"></a>
### OR-MON-005 — Blocking ve final blocker

```yaml
id: OR-MON-005
engine: oracle
category: monitor
priority: P0
risk: R0
scope: RAC/PDB visible scope
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SESSION SELECT

**Girdi:** Yok

```sql
SELECT inst_id, con_id, sid, serial#, username, sql_id,
       blocking_session_status, blocking_instance, blocking_session,
       final_blocking_session_status, final_blocking_instance,
       final_blocking_session, event, wait_class, seconds_in_wait
FROM gv$session
WHERE blocking_session_status = 'VALID'
   OR final_blocking_session_status = 'VALID'
ORDER BY seconds_in_wait DESC;
```

**Yorum / kullanım:** Blocker inactive/idle olup açık transaction tutabilir. Zincir kenarını instance+sid ile kur, güncel serial/logon time ile doğrula. Bekleme sayısını kesin deadlock tanısı sayma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O06](#kaynak-o06)


<a id="or-mon-006"></a>
### OR-MON-006 — Uzun çalışan SQL adayları

```yaml
id: OR-MON-006
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SESSION SELECT

**Girdi:** min_seconds:int >=1; row_limit:int 1..1000

```sql
SELECT inst_id, con_id, sid, serial#, username, sql_id, sql_exec_start,
       ROUND((SYSDATE - sql_exec_start) * 86400, 1) AS elapsed_seconds,
       event, wait_class, state, module
FROM gv$session
WHERE type = 'USER' AND status = 'ACTIVE' AND sql_exec_start IS NOT NULL
  AND sql_exec_start < SYSDATE - (:min_seconds / 86400)
ORDER BY sql_exec_start
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Elapsed wall-clock süredir, CPU değil. Application batch/maintenance SLOsu ile değerlendir; uzun olması tek başına kill nedeni değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O06](#kaynak-o06)


<a id="or-mon-007"></a>
### OR-MON-007 — Açık transaction ve UNDO tüketimi

```yaml
id: OR-MON-007
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$TRANSACTION, GV_$SESSION SELECT

**Girdi:** Yok

```sql
SELECT s.inst_id, s.con_id, s.sid, s.serial#, s.username, s.status,
       s.sql_id, s.logon_time, t.start_time, t.used_ublk, t.used_urec,
       t.xidusn, t.xidslot, t.xidsqn
FROM gv$transaction t
JOIN gv$session s ON s.inst_id = t.inst_id AND s.taddr = t.addr
ORDER BY t.used_ublk DESC;
```

**Yorum / kullanım:** USED_UBLK block sayısıdır; her zaman tek db_block_size ile byte birimine çevrilemez. START_TIME string biçimini NLS varsayarak parse etme. Sonlandırma uzun rollback yaratabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-008"></a>
### OR-MON-008 — Enqueue lock detayları

```yaml
id: OR-MON-008
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$LOCK SELECT

**Girdi:** Yok

```sql
SELECT inst_id, con_id, sid, type, id1, id2, lmode, request, ctime, block
FROM gv$lock
WHERE request > 0 OR block > 0
ORDER BY ctime DESC;
```

**Yorum / kullanım:** RAC BLOCK=2 uzak instance potansiyel bloklamasıyla ilgili olabilir; lokal block=1 varsayımıyla yorumlama. Object eşlemesi lock türüne göre değişir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-009"></a>
### OR-MON-009 — Long operations ilerlemesi

```yaml
id: OR-MON-009
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SESSION_LONGOPS SELECT

**Girdi:** Yok

```sql
SELECT inst_id, con_id, sid, serial#, opname, target, sofar, totalwork,
       units, start_time, elapsed_seconds, time_remaining, sql_id
FROM gv$session_longops
WHERE totalwork > 0 AND sofar < totalwork
ORDER BY start_time;
```

**Yorum / kullanım:** Her sorgu/işlem longops üretmez; boş sonuç aktif iş olmadığı anlamına gelmez. TIME_REMAINING tahmindir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-010"></a>
### OR-MON-010 — Wait event kümülatif sayaçları

```yaml
id: OR-MON-010
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SYSTEM_EVENT SELECT

**Girdi:** row_limit:int 1..500

```sql
SELECT inst_id, con_id, event, wait_class, total_waits,
       time_waited_micro, total_timeouts
FROM gv$system_event
WHERE wait_class <> 'Idle'
ORDER BY time_waited_micro DESC
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Zaman mikro-saniyedir; iki snapshot delta ile wait/s hesapla. Paralel/session aggregate wait toplamı wall clocktan büyük olabilir. Restart baseline yeniler.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-011"></a>
### OR-MON-011 — Redo, commit, logical/physical IO sayaçları

```yaml
id: OR-MON-011
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SYSSTAT SELECT

**Girdi:** Yok

```sql
SELECT inst_id, con_id, name, value
FROM gv$sysstat
WHERE name IN ('user commits','user rollbacks','redo size',
               'session logical reads','physical reads','physical writes',
               'physical read total bytes','physical write total bytes',
               'parse count (total)','parse count (hard)','execute count')
ORDER BY inst_id, name;
```

**Yorum / kullanım:** Her name için unit registry tanımla; redo size bytes, physical reads block count. İki örnek arası rate gerekir; toplamı anlık hız sayma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-012"></a>
### OR-MON-012 — Kısa dönem sistem metrikleri

```yaml
id: OR-MON-012
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$SYSMETRIC SELECT

**Girdi:** Yok

```sql
SELECT begin_time, end_time, intsize_csec, group_id, metric_name,
       value, metric_unit, con_id
FROM v$sysmetric
WHERE group_id = 2
ORDER BY metric_name;
```

**Yorum / kullanım:** GROUP_ID=2 uzun aralık sistem metrik profili için capability/test gerekir; kayıt üstündeki gerçek begin/end/unit esas alınır. Metrik zaten oran ise tekrar delta alma.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O13](#kaynak-o13)


<a id="or-mon-013"></a>
### OR-MON-013 — SQL cache toplam elapsed / CPU / buffer gets

```yaml
id: OR-MON-013
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SQLSTATS SELECT

**Girdi:** min_executions:int>=0; row_limit:int 1..100

```sql
SELECT inst_id, con_id, sql_id, plan_hash_value, executions,
       elapsed_time, cpu_time, buffer_gets, disk_reads, rows_processed,
       ROUND(elapsed_time / NULLIF(executions,0) / 1000000, 6) AS avg_elapsed_s,
       SUBSTR(sql_text,1,1000) AS sql_text_preview
FROM gv$sqlstats
WHERE executions >= :min_executions
ORDER BY elapsed_time DESC
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Cache kümülatif metrikleridir; cursor yaşları farklıdır ve plan değişimleri olabilir. AVG yalnızca tüm executionsın ortalamasıdır. CPU/elapsed mikro-saniye; query text hassastır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O07](#kaynak-o07)


<a id="or-mon-014"></a>
### OR-MON-014 — Seçili SQL child cursor detayları

```yaml
id: OR-MON-014
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$SQL SELECT

**Girdi:** sql_id:text validated Oracle SQL_ID

```sql
SELECT inst_id, con_id, sql_id, child_number, plan_hash_value,
       executions, elapsed_time, cpu_time, buffer_gets, disk_reads,
       invalidations, loads, last_active_time,
       is_bind_sensitive, is_bind_aware
FROM gv$sql WHERE sql_id = :sql_id
ORDER BY inst_id, child_number;
```

**Yorum / kullanım:** Child cursor artışı otomatik kötü plan demek değildir. Bind değerlerini varsayılan toplama; veri sızıntısı riski vardır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-015"></a>
### OR-MON-015 — Seçili local cached cursor planı

```yaml
id: OR-MON-015
engine: oracle
category: diagnostics
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBMS_XPLAN execute + gerekli fixed-view SELECT

**Girdi:** sql_id:text; child_number:int>=0

```sql
SELECT plan_table_output
FROM TABLE(DBMS_XPLAN.DISPLAY_CURSOR(:sql_id, :child_number, 'TYPICAL'));
```

**Yorum / kullanım:** Sorguyu yeniden çalıştırmaz. Aynı instance cursor cacheinde plan mevcut olmalı. ALLSTATS LAST sadece runtime instrumentation varsa anlamlı; kendiliğinden etkinleştirme. DISPLAY_AWR ayrı lisans gerektiren yol olduğundan burada kullanılmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O19](#kaynak-o19) [O10](#kaynak-o10)


<a id="or-mon-016"></a>
### OR-MON-016 — Tablespace kapasitesi ve gerçek kullanılan bloklar

```yaml
id: OR-MON-016
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_TABLESPACE_USAGE_METRICS, DBA_TABLESPACES SELECT

**Girdi:** Yok

```sql
SELECT m.tablespace_name, t.contents, t.status, t.block_size,
       m.used_space * t.block_size AS used_bytes,
       m.tablespace_size * t.block_size AS effective_capacity_bytes,
       ROUND(m.used_percent,2) AS used_percent
FROM dba_tablespace_usage_metrics m
JOIN dba_tablespaces t ON t.tablespace_name = m.tablespace_name
ORDER BY m.used_percent DESC;
```

**Yorum / kullanım:** Kapasite hesabı büyüyebilir alanı ve ortam limitlerini dikkate alabilir; yalnızca mevcut dosya allocation oranı değildir. Host/ASM/provider fiziksel boş alanı ayrı kaynaktır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O08](#kaynak-o08)


<a id="or-mon-017"></a>
### OR-MON-017 — Datafile allocation / autoextend / maxsize

```yaml
id: OR-MON-017
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_DATA_FILES SELECT

**Girdi:** Yok

```sql
SELECT tablespace_name, file_id, file_name, bytes, autoextensible,
       maxbytes, increment_by, status, online_status
FROM dba_data_files ORDER BY tablespace_name, file_id;
```

**Yorum / kullanım:** INCREMENT_BY Oracle block sayısıdır, MB değil. Filesystem path hassas metadata olarak saklanır; autoextend unlimited varsayımı yapılmaz.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-018"></a>
### OR-MON-018 — TEMP alanının kullanılabilir kapasitesi

```yaml
id: OR-MON-018
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_TEMP_FREE_SPACE SELECT

**Girdi:** Yok

```sql
SELECT tablespace_name, tablespace_size, allocated_space, free_space,
       shared, inst_id
FROM dba_temp_free_space ORDER BY tablespace_name, inst_id;
```

**Yorum / kullanım:** Sütunlar byte birimlidir. FREE_SPACE yeniden kullanılabilir allocated alanı ve henüz allocate edilmemiş alanı içerebilir; allocated_space-free_space kör farkıyla aktif kullanım hesaplama.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O12](#kaynak-o12)


<a id="or-mon-019"></a>
### OR-MON-019 — TEMP kullanan oturumlar

```yaml
id: OR-MON-019
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** GV_$TEMPSEG_USAGE, GV_$SESSION SELECT

**Girdi:** Yok

```sql
SELECT u.inst_id, u.con_id, s.sid, s.serial#, u.username, u.sql_id,
       u.tablespace, u.segtype, SUM(u.blocks) AS allocated_blocks
FROM gv$tempseg_usage u
LEFT JOIN gv$session s
  ON s.inst_id = u.inst_id AND s.saddr = u.session_addr
 AND s.serial# = u.session_num
GROUP BY u.inst_id,u.con_id,s.sid,s.serial#,u.username,u.sql_id,u.tablespace,u.segtype
ORDER BY allocated_blocks DESC;
```

**Yorum / kullanım:** Byte dönüşümü ilgili tablespace block_size ile yapılır. Allocation görüldüğü anda ait olduğu session/sql değişmiş olabilir; kill için yeniden doğrula.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O16](#kaynak-o16) [O06](#kaynak-o06)


<a id="or-mon-020"></a>
### OR-MON-020 — UNDO geçmişi / snapshot-too-old / alan hataları

```yaml
id: OR-MON-020
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$UNDOSTAT SELECT

**Girdi:** lookback_hours:int 1..48

```sql
SELECT begin_time, end_time, undoblks, txncount, maxquerylen,
       maxconcurrency, unxpstealcnt, ssolderrcnt, nospaceerrcnt,
       tuned_undoretention
FROM v$undostat
WHERE begin_time >= SYSDATE - (:lookback_hours / 24)
ORDER BY begin_time DESC;
```

**Yorum / kullanım:** TUNED_UNDORETENTION saniyedir; yalnızca retention artırmak diski çözmez. SSOLDERRCNT ORA-01555 eğilimi için kanıttır, sorgu düzeyinde neden ayrıca incelenir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-021"></a>
### OR-MON-021 — FRA limiti / kullanılan / geri kazanılabilir alan

```yaml
id: OR-MON-021
engine: oracle
category: monitor
priority: P0
risk: R0
scope: database/FRA
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$RECOVERY_FILE_DEST, V_$FLASH_RECOVERY_AREA_USAGE SELECT

**Girdi:** Yok

```sql
SELECT name, space_limit, space_used, space_reclaimable,
       number_of_files, con_id
FROM v$recovery_file_dest;
SELECT file_type, percent_space_used, percent_space_reclaimable,
       number_of_files, con_id
FROM v$flash_recovery_area_usage;
```

**Yorum / kullanım:** Reclaimable alanın hemen ve risksiz silinebileceği varsayılmaz. RMAN retention, standby ihtiyacı ve guaranteed restore pointler ayrı incelenir. FRA yoksa unsupported/not_configured.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-022"></a>
### OR-MON-022 — Archive destination hataları

```yaml
id: OR-MON-022
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$ARCHIVE_DEST_STATUS SELECT

**Girdi:** Yok

```sql
SELECT dest_id, dest_name, status, type, database_mode, recovery_mode,
       destination, error, archived_thread#, archived_seq#,
       applied_thread#, applied_seq#
FROM v$archive_dest_status
WHERE status <> 'INACTIVE'
ORDER BY dest_id;
```

**Yorum / kullanım:** Tek sequence farkı saniye/byte lag değildir; thread ve resetlogs dönemi önemlidir. ERROR mesajı hassas endpoint/path içerebilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-023"></a>
### OR-MON-023 — Data Guard lag ve veri tazeliği

```yaml
id: OR-MON-023
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle physical standby / Data Guard capability

**Yetki:** V_$DATAGUARD_STATS SELECT

**Girdi:** Yok

```sql
SELECT name, value, unit, time_computed, datum_time
FROM v$dataguard_stats ORDER BY name;
```

**Yorum / kullanım:** DATUM_TIME ilerlemiyorsa 0 lag bile stale olabilir. VALUE string/interval formatlıdır; unit-aware parse kullan. Primaryde boş olması standby sağlıklı demek değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O09](#kaynak-o09)


<a id="or-mon-024"></a>
### OR-MON-024 — Standby archive gap ve apply süreçleri

```yaml
id: OR-MON-024
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle Data Guard; V$DATAGUARD_PROCESS view/columns capability

**Yetki:** V_$ARCHIVE_GAP, V_$DATAGUARD_PROCESS SELECT

**Girdi:** Yok

```sql
SELECT thread#, low_sequence#, high_sequence# FROM v$archive_gap;
SELECT name, role, action, client_role, thread#, sequence#
FROM v$dataguard_process ORDER BY name;
```

**Yorum / kullanım:** ARCHIVE_GAP boş = yalnızca raporlanan mevcut gap yok; bütün replication hattı sağlıklı demek değildir. Broker/policy ve remote connection health ayrı.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O17](#kaynak-o17) [O09](#kaynak-o09)


<a id="or-mon-025"></a>
### OR-MON-025 — RMAN backup job geçmişi

```yaml
id: OR-MON-025
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$RMAN_BACKUP_JOB_DETAILS SELECT

**Girdi:** lookback_days:int1..90; row_limit:int1..500

```sql
SELECT session_key, input_type, status, start_time, end_time,
       elapsed_seconds, input_bytes, output_bytes,
       compression_ratio, output_device_type
FROM v$rman_backup_job_details
WHERE start_time >= SYSDATE - :lookback_days
ORDER BY start_time DESC
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Controlfile/catalog geçmişi backup dosyasının hâlâ mevcut, decrypt edilebilir ve restore edilebilir olduğunu kanıtlamaz. Managed backup provider APIsinde olabilir; boş sonuç backup yok tanısı değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-026"></a>
### OR-MON-026 — Kaydedilmiş block corruption

```yaml
id: OR-MON-026
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$DATABASE_BLOCK_CORRUPTION SELECT

**Girdi:** Yok

```sql
SELECT file#, block#, blocks, corruption_change#, corruption_type, con_id
FROM v$database_block_corruption ORDER BY file#, block#;
```

**Yorum / kullanım:** Bu kayıtlı bulguları okur; yeni corruption taraması yapmaz. Boş sonuç bütün databasein fiziksel doğrulandığı anlamına gelmez. RMAN VALIDATE ağır ayrı çalışma ve provider/scope planıdır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-027"></a>
### OR-MON-027 — Yakın tarihli alert kayıtları

```yaml
id: OR-MON-027
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$DIAG_ALERT_EXT SELECT / ADR görünürlüğü

**Girdi:** lookback_minutes:int1..1440; row_limit:int1..500

```sql
SELECT originating_timestamp, message_type, message_level,
       component_id, problem_key, message_text
FROM v$diag_alert_ext
WHERE originating_timestamp >= SYSTIMESTAMP - NUMTODSINTERVAL(:lookback_minutes,'MINUTE')
  AND message_type IN (2,3,4)
ORDER BY originating_timestamp DESC
FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** İç hata/error/warning sınıfları; instance-specific ADR kapsamı ile göster. Yazılım bütün logu tekrar tekrar taramaz. Query/paths/kişisel veri redaction uygula.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O20](#kaynak-o20)


<a id="or-mon-028"></a>
### OR-MON-028 — Invalid nesneler ve derleme hataları

```yaml
id: OR-MON-028
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_OBJECTS, DBA_ERRORS SELECT

**Girdi:** owner_user:text exact case

```sql
SELECT owner, object_name, object_type, status, last_ddl_time
FROM dba_objects WHERE owner = :owner_user AND status <> 'VALID'
ORDER BY object_type, object_name;
SELECT owner, name, type, sequence, line, position, text
FROM dba_errors WHERE owner = :owner_user
ORDER BY name, type, sequence;
```

**Yorum / kullanım:** Invalid sebepleri dependency/privilege/deployment olabilir; toplu recompile otomatik çözüm sayılmaz. Derleme metni hassas olabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-029"></a>
### OR-MON-029 — Unusable index ve partitionlar

```yaml
id: OR-MON-029
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_INDEXES, DBA_IND_PARTITIONS, DBA_IND_SUBPARTITIONS SELECT

**Girdi:** owner_user:text

```sql
SELECT owner, index_name, table_owner, table_name, index_type, status,
       partitioned, visibility FROM dba_indexes
WHERE owner = :owner_user ORDER BY table_name,index_name;
SELECT index_owner, index_name, partition_name, status
FROM dba_ind_partitions
WHERE index_owner = :owner_user AND status <> 'USABLE'
ORDER BY index_name, partition_name;
SELECT index_owner, index_name, subpartition_name, status
FROM dba_ind_subpartitions
WHERE index_owner = :owner_user AND status <> 'USABLE'
ORDER BY index_name, subpartition_name;
```

**Yorum / kullanım:** Partitioned üst index STATUS=N/A olabilir; N/Ayı unusable sayma. INVISIBLE kullanım amacı olabilir ve otomatik visibility değişmez.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-030"></a>
### OR-MON-030 — Tablo istatistikleri güncelliği

```yaml
id: OR-MON-030
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_TAB_STATISTICS SELECT

**Girdi:** owner_user:text

```sql
SELECT owner, table_name, partition_name, subpartition_name,
       num_rows, blocks, sample_size, last_analyzed, stale_stats, stattype_locked
FROM dba_tab_statistics
WHERE owner = :owner_user
ORDER BY last_analyzed NULLS FIRST, table_name;
```

**Yorum / kullanım:** NUM_ROWS kesin COUNT değildir. Stale flag workload/monitoring gecikmesine bağlıdır; otomatik tüm şemayı stats gather yapma. Locked stats bilinçli olabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-031"></a>
### OR-MON-031 — Schema segment boyutları

```yaml
id: OR-MON-031
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_SEGMENTS SELECT

**Girdi:** owner_user:text; row_limit:int1..1000

```sql
SELECT owner, segment_name, partition_name, segment_type, tablespace_name,
       bytes, blocks, extents
FROM dba_segments WHERE owner = :owner_user
ORDER BY bytes DESC FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** LOB/index/partition segmentlerini gerçek table totalinde metadata ile ilişkilendir; segment adı=table adı varsayma. Allocation ile aktif veri boyutu farklıdır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-032"></a>
### OR-MON-032 — Scheduler başarısızlıkları / planlı işler

```yaml
id: OR-MON-032
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_SCHEDULER_JOBS, DBA_SCHEDULER_JOB_RUN_DETAILS SELECT

**Girdi:** owner_user:text; lookback_days:int1..30; row_limit:int1..500

```sql
SELECT owner, job_name, enabled, state, failure_count, last_start_date,
       last_run_duration, next_run_date
FROM dba_scheduler_jobs WHERE owner = :owner_user
ORDER BY job_name;
SELECT owner, job_name, status, error#, actual_start_date, run_duration,
       additional_info
FROM dba_scheduler_job_run_details
WHERE owner = :owner_user
  AND log_date >= SYSTIMESTAMP - NUMTODSINTERVAL(:lookback_days,'DAY')
ORDER BY log_date DESC FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** DBMS_JOB legacy işleri için ayrı adapter eklenir. Job metni/arguments sır içerebilir; varsayılan toplama.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-033"></a>
### OR-MON-033 — Kullanıcı, expiry ve authentication tipi

```yaml
id: OR-MON-033
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_USERS SELECT

**Girdi:** Yok

```sql
SELECT username, account_status, lock_date, expiry_date,
       default_tablespace, temporary_tablespace, profile,
       authentication_type, common, oracle_maintained, created
FROM dba_users
ORDER BY oracle_maintained, username;
```

**Yorum / kullanım:** NO AUTHENTICATION owner ve locked account normal policy olabilir. Parola hashleri okunmaz/exportlanmaz; SYS/Oracle-maintained kullanıcılara otomatik işlem yok.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-034"></a>
### OR-MON-034 — Role ve system privilege grafiği

```yaml
id: OR-MON-034
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_ROLE_PRIVS, DBA_SYS_PRIVS SELECT

**Girdi:** Yok

```sql
SELECT grantee, granted_role, admin_option, default_role, common
FROM dba_role_privs ORDER BY grantee, granted_role;
SELECT grantee, privilege, admin_option, common
FROM dba_sys_privs ORDER BY grantee, privilege;
```

**Yorum / kullanım:** Transitive role graphını cycle-safe client traversal ile çıkar. PUBLIC ve enabled roles/secure application role farkını koru. ANY/DBA/ADMIN OPTION ayrı risk göstergesidir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-035"></a>
### OR-MON-035 — Nesne ve sütun grantleri

```yaml
id: OR-MON-035
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_TAB_PRIVS, DBA_COL_PRIVS SELECT

**Girdi:** owner_user:text

```sql
SELECT owner, table_name, grantee, grantor, privilege, grantable, type
FROM dba_tab_privs WHERE owner = :owner_user
ORDER BY table_name, grantee, privilege;
SELECT owner, table_name, column_name, grantee, privilege, grantable
FROM dba_col_privs WHERE owner = :owner_user
ORDER BY table_name, column_name, grantee;
```

**Yorum / kullanım:** Role grafiğiyle birleştir; READ/SELECT/RW/EXECUTE ve GRANT OPTION ayrımı görünür. VPD/OLS/Database Vault etkinse effective sonucu ek katman kısıtlayabilir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-mon-036"></a>
### OR-MON-036 — Tablespace kotaları

```yaml
id: OR-MON-036
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_TS_QUOTAS SELECT

**Girdi:** owner_user:text

```sql
SELECT username, tablespace_name, bytes, max_bytes, blocks, max_blocks
FROM dba_ts_quotas
WHERE username = :owner_user ORDER BY tablespace_name;
```

**Yorum / kullanım:** MAX_BYTES=-1 limitsiz kota anlamındadır. User kotası dolu ile tablespace/disk dolu farklı olaylar. UNLIMITED TABLESPACE system privilege etkisini ayrıca kontrol et.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-037"></a>
### OR-MON-037 — Explorer: sütun / default / nullability

```yaml
id: OR-MON-037
engine: oracle
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Nesne metadata görünürlüğü

**Girdi:** owner_user,table_name:text exact case

```sql
SELECT owner, table_name, column_id, column_name, data_type,
       data_length, char_length, char_used, data_precision, data_scale,
       nullable, identity_column, virtual_column, data_default
FROM all_tab_columns
WHERE owner = :owner_user AND table_name = :table_name
ORDER BY column_id;
```

**Yorum / kullanım:** DATA_DEFAULT LONG olabilir; sürücü bunu bounded/lazy almalı. Byte ve char length ayrılır. Identity/virtual alanlar editte read-only olur; gizli kolonlar ayrı seçenek.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-038"></a>
### OR-MON-038 — Explorer: constraint / PK / FK sütunları

```yaml
id: OR-MON-038
engine: oracle
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Katalog görünürlüğü / ilgili nesne izni

**Girdi:** owner_user,table_name:text

```sql
SELECT c.owner,c.table_name,c.constraint_name,c.constraint_type,
       c.status,c.validated,c.deferrable,c.deferred,c.delete_rule,
       c.r_owner,c.r_constraint_name,
       cc.column_name,cc.position
FROM all_constraints c
LEFT JOIN all_cons_columns cc
  ON cc.owner=c.owner AND cc.constraint_name=c.constraint_name
WHERE c.owner=:owner_user AND c.table_name=:table_name
ORDER BY c.constraint_name,cc.position;
```

**Yorum / kullanım:** FK target sütunları r_owner+r_constraint_name+position üzerinden ikinci metadata lookup ile çözülür. Constraint name tek başına bütün DBde unique değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-039"></a>
### OR-MON-039 — Data Browser: sınırlı ilk sayfa

```yaml
id: OR-MON-039
engine: oracle
category: explorer
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** READ/SELECT on target

**Girdi:** selection:validated column AST; table:qualified; page_size:int1..1000

```sql
SELECT {{columns:selection}}
FROM {{qualified:table}}
FETCH FIRST :page_size ROWS ONLY;
```

**Yorum / kullanım:** Bu yalnızca sırasız ilk önizlemedir. Devam sayfaları unique keyli keyset ORDER BY ile renderer tarafından üretilir. LOBlar ilk sayfada metadata/preview; SELECT * varsayılan değil.

**Ön koşul / güvenlik:** Filtreler bind; schema/table isimleri metadata allowlist; export ayrı policy. Readonly connection kullan.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-mon-040"></a>
### OR-MON-040 — Service ve parametre snapshot

```yaml
id: OR-MON-040
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$SERVICES, V_$PARAMETER SELECT

**Girdi:** Yok

```sql
SELECT name, network_name, con_id FROM v$services ORDER BY name;
SELECT name, value, isdefault, issys_modifiable, ispdb_modifiable
FROM v$parameter
WHERE name IN ('compatible','db_block_size','open_cursors',
               'processes','sessions','sga_target','pga_aggregate_target',
               'pga_aggregate_limit','undo_retention','db_create_file_dest')
ORDER BY name;
```

**Yorum / kullanım:** Sabit allowlist kullan; bütün parametreleri/sır içeren değerleri kör dışa aktarma. İsPDB modifiable metadata auto ALTER SYSTEM yetkisi değildir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-041"></a>
### OR-MON-041 — SGA / PGA snapshot

```yaml
id: OR-MON-041
engine: oracle
category: monitor
priority: P1
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$SGA, V_$PGASTAT SELECT

**Girdi:** Yok

```sql
SELECT name, value FROM v$sga ORDER BY name;
SELECT name, value, unit FROM v$pgastat
WHERE name IN ('aggregate PGA target parameter','total PGA allocated',
               'total PGA inuse','maximum PGA allocated',
               'over allocation count','cache hit percentage')
ORDER BY name;
```

**Yorum / kullanım:** Unit her satırdan okunur. PGA cache hit ratio tek başına memory tuning hedefi değildir; maximum geçmiş tepe, allocated anlık.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-042"></a>
### OR-MON-042 — Redo log grupları ve üye sağlığı

```yaml
id: OR-MON-042
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** V_$LOG, V_$LOGFILE SELECT

**Girdi:** Yok

```sql
SELECT group#, thread#, sequence#, bytes, members, archived, status
FROM v$log ORDER BY thread#, group#;
SELECT group#, status, type, member FROM v$logfile ORDER BY group#, member;
```

**Yorum / kullanım:** Üye STATUS NULL normal olabilir. ACTIVE/CURRENT log normal rolling statülerdir; otomatik drop/clear logfile yok. Standby redo log ayrı V$STANDBY_LOG varyantıdır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-043"></a>
### OR-MON-043 — Dağıtık in-doubt transaction

```yaml
id: OR-MON-043
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DBA_2PC_PENDING SELECT

**Girdi:** Yok

```sql
SELECT local_tran_id, global_tran_id, state, mixed, advice,
       fail_time, force_time, retry_time
FROM dba_2pc_pending ORDER BY fail_time;
```

**Yorum / kullanım:** COMMIT FORCE/ROLLBACK FORCE otomatik yapılmaz; remote coordinator/uygulama ile mutabakat ve veri tutarlılık planı gerekir.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-mon-044"></a>
### OR-MON-044 — Proxy erişim envanteri

```yaml
id: OR-MON-044
engine: oracle
category: monitor
priority: P0
risk: R0
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** PROXY_USERS görünürlüğü

**Girdi:** Yok

```sql
SELECT proxy, client, authentication, flags
FROM proxy_users ORDER BY proxy, client;
```

**Yorum / kullanım:** Migration proxy hakları ve beklenmeyen runtime proxy erişimi gözden geçirilir. Görünürlük kapsamı sürüm/kullanıcı ile raporlanır.

**Ön koşul / güvenlik:** Yetki eksikliği veya görünmeyen nesne sonucunu sıfır/sağlıklı diye göstermeyin; sonuç kapsamını belirtin.

**Kaynak:** [O02](#kaynak-o02)


<a id="or-mon-045"></a>
### OR-MON-045 — Lisans kapılı ASH özeti — varsayılan kapalı

```yaml
id: OR-MON-045
engine: oracle
category: licensed_diagnostics
priority: P2
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Diagnostics Pack kullanma hakkı açıkça doğrulanmış Oracle ortamı

**Yetki:** V_$ACTIVE_SESSION_HISTORY SELECT + lisans entitlement

**Girdi:** lookback_minutes:int1..60; row_limit:int1..100

```sql
SELECT sql_id, session_state, wait_class, event, COUNT(*) AS sample_count
FROM v$active_session_history
WHERE sample_time >= SYSTIMESTAMP - NUMTODSINTERVAL(:lookback_minutes,'MINUTE')
GROUP BY sql_id, session_state, wait_class, event
ORDER BY sample_count DESC FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Sample_count execution count veya kesin elapsed değildir. CONTROL_MANAGEMENT_PACK_ACCESS parametresi/SELECT yetkisi lisans sahipliği kanıtı değildir.

**Ön koşul / güvenlik:** diagnostics_pack_entitled=true, admin kaydı, platform/edition doğrulaması olmadan SQL hazırlanmaz/çalışmaz. Otomatik polling kesinlikle kapalı.

**Kaynak:** [O10](#kaynak-o10)


<a id="or-mon-046"></a>
### OR-MON-046 — Lisans kapılı Real-Time SQL Monitoring — varsayılan kapalı

```yaml
id: OR-MON-046
engine: oracle
category: licensed_diagnostics
priority: P2
risk: R0
scope: current_database
cost: medium
timeout_seconds: 30
poll_seconds: 0
transaction: read_only
```

**Hedef:** Tuning Pack ve gerekli Diagnostics Pack hakları doğrulanmış Oracle ortamı

**Yetki:** V_$SQL_MONITOR SELECT + lisans entitlement

**Girdi:** row_limit:int1..100

```sql
SELECT sql_id, sql_exec_id, sql_exec_start, status, username,
       elapsed_time, cpu_time, buffer_gets, disk_reads
FROM v$sql_monitor
ORDER BY sql_exec_start DESC FETCH FIRST :row_limit ROWS ONLY;
```

**Yorum / kullanım:** Tek execution parallel worker kayıtları içerebilir; SQL_ID+SQL_EXEC_ID+SQL_EXEC_START+process kimlikleriyle normalize et. Yalnızca lisanslı panelde sun.

**Ön koşul / güvenlik:** Ayrı entitlement kontrolü, varsayılan kapalı. Tuning Pack yokken health check tarafından çağrılmaz.

**Kaynak:** [O10](#kaynak-o10)


## 7.3 Oracle kontrollü bakım / güvenlik aksiyonları

Her DDL/PLSQL action bağımsız sessionda çalışır; bütün akışı tek rollback ile geri alma vaadi yoktur. RDS provider alternatifleri ayrı katalogdur; self-managed ALTER SYSTEM komutları sağlayıcıya kör gönderilmez.


<a id="or-ops-001"></a>
### OR-OPS-001 — Seçili SQL yürütmesini iptal et

```yaml
id: OR-OPS-001
engine: oracle
category: maintenance
priority: P0
risk: R2
scope: instance / allowed PDB
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER SYSTEM veya provider action

**Girdi:** cancel_session_token:yalnızca doğrulanmış sid,serial#,[@inst_id],sql_id bileşenlerinden renderer üretir

```sql
ALTER SYSTEM CANCEL SQL {{literal:cancel_session_token}};
```

**Yorum / kullanım:** RAC örnek token içeriği: 123,456,@2,abc123def4567. Kullanıcı serbest string giremez. SQL_ID olmadan sonraki farklı sorguyu iptal etme riski artar; SQL_ID zorunlu tutulur. Cancel transactionın tamamını kapatmaz.

**Ön koşul / güvenlik:** GV$SESSIONdan sid+serial+inst+con+sql_id+sql_exec_start yeniden eşleşmeli. SYS/background/kendi sessionın denylist; önce cancel, kill ayrı onay.

**Kaynak:** [O14](#kaynak-o14)


<a id="or-ops-002"></a>
### OR-OPS-002 — Kontrollü session kill

```yaml
id: OR-OPS-002
engine: oracle
category: maintenance
priority: P0
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER SYSTEM / managed provider karşılığı

**Girdi:** kill_session_token:validated sid,serial#,@inst_id composite

```sql
ALTER SYSTEM KILL SESSION {{literal:kill_session_token}} IMMEDIATE;
```

**Yorum / kullanım:** Ongoing transaction rollback maliyeti ve application retry/continuity davranışı gösterilir. IMMEDIATE dönüşü işlemin bütün fiziksel etkilerinin anında bitmesi garantisi değildir.

**Ön koşul / güvenlik:** Güncel serial, logon_time, instance ve PDB eşleşmesi; aktif kritik iş/replication/background denylist. Token yalnızca typed sayılardan üretilir.

**Kaynak:** [O14](#kaynak-o14)


<a id="or-ops-003"></a>
### OR-OPS-003 — Tek tablo için kontrollü optimizer statistics

```yaml
id: OR-OPS-003
engine: oracle
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Object owner / uygun DBMS_STATS yetkisi

**Girdi:** owner_user,table_name:text validated metadata

```sql
BEGIN
  DBMS_STATS.GATHER_TABLE_STATS(
    ownname => :owner_user,
    tabname => :table_name,
    estimate_percent => DBMS_STATS.AUTO_SAMPLE_SIZE,
    method_opt => 'FOR ALL COLUMNS SIZE AUTO',
    degree => 1,
    cascade => DBMS_STATS.AUTO_CASCADE,
    no_invalidate => DBMS_STATS.AUTO_INVALIDATE);
END;
```

**Yorum / kullanım:** Histogram/plan değişimi mümkün. Locked stats veya pending/incremental stats politikası varsa ayrı plan gerekir; otomatik lock kaldırma yok. PL/SQL blok drivera bir statement, SQLPlus slash gönderilmez.

**Ön koşul / güvenlik:** Hedef yalnızca bir onaylı tablo; I/O penceresi; current stats timestamp baseline ve plan izleme. Tam schema toplama varsayılan değil.

**Kaynak:** [O15](#kaynak-o15)


<a id="or-ops-004"></a>
### OR-OPS-004 — Onaylı nesneyi yeniden derleme

```yaml
id: OR-OPS-004
engine: oracle
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Owner / gerekli ALTER privilege

**Girdi:** object_type:VIEW|PROCEDURE|FUNCTION|TRIGGER; object:qualified metadata

```sql
ALTER {{enum:object_type}} {{qualified:object}} COMPILE;
```

**Yorum / kullanım:** Enum yalnızca bu dört syntax-compatible türdür. PACKAGE/PACKAGE BODY/TYPE için ayrı doğru syntax variantı gerekir. Metadata status hata sebebini çözmez; derleme sonrası DBA_ERRORS kontrol edilir.

**Ön koşul / güvenlik:** Nesne türü katalogla eşleşmeli; dependency/lock etkisi göster. Toplu SYS compile yok.

**Kaynak:** [O05](#kaynak-o05)


<a id="or-ops-005"></a>
### OR-OPS-005 — Tek nonpartitioned index rebuild — offline

```yaml
id: OR-OPS-005
engine: oracle
category: maintenance
priority: P2
risk: R2
scope: current_database
cost: heavy
timeout_seconds: 120
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Owner / uygun ALTER INDEX

**Girdi:** index:qualified metadata

```sql
ALTER INDEX {{qualified:index}} REBUILD;
```

**Yorum / kullanım:** ONLINE/parallel/partition işlemleri edition, index type ve lisansa bağlı ayrı varyanttır. Her indexi düzenli rebuild gereklidir varsayımı yapılmaz.

**Ön koşul / güvenlik:** Lock/disk/redo/standby etkisi; partitioned/domain/LOB index için generic yolu kapat; backup ve pencere değerlendirmesi.

**Kaynak:** [O05](#kaynak-o05) [O10](#kaynak-o10)


<a id="or-ops-006"></a>
### OR-OPS-006 — Tablespace kotasını açık sınırla değiştirme

```yaml
id: OR-OPS-006
engine: oracle
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER USER

**Girdi:** owner_user,tablespace_name:identifier; quota_mb:int bounded

```sql
ALTER USER {{ident:owner_user}} QUOTA {{int:quota_mb}}M ON {{ident:tablespace_name}};
```

**Yorum / kullanım:** Quota artışı disk eklemez. Runtime kullanıcıya quota vererek ownership modelini delme. Mevcut kullanımın altına indirme ayrı uyarı gerektirir.

**Ön koşul / güvenlik:** Oracle-maintained/system tablespace denylist; limitsiz quota otomatik yok.

**Kaynak:** [O01](#kaynak-o01) [O03](#kaynak-o03)


<a id="or-ops-007"></a>
### OR-OPS-007 — OMF ile tek tablespacee sınırlı datafile ekle

```yaml
id: OR-OPS-007
engine: oracle
category: maintenance
priority: P1
risk: R2
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER TABLESPACE

**Girdi:** tablespace_name:identifier; initial_mb,growth_mb,max_mb:bounded integers

```sql
ALTER TABLESPACE {{ident:tablespace_name}}
  ADD DATAFILE SIZE {{int:initial_mb}}M
  AUTOEXTEND ON NEXT {{int:growth_mb}}M MAXSIZE {{int:max_mb}}M;
```

**Yorum / kullanım:** Smallfile OMF varyantıdır. Bigfile tablespacee ADD DATAFILE kullanılmaz; resize/provider storage API ayrı plan. Storage ücreti ve fiziksel alan onayı gerekir.

**Ön koşul / güvenlik:** OMF/type/free disk/PDB write preflight; maxsize sonlu. Hata alınca dosya adı değiştirip kör retry yok.

**Kaynak:** [O18](#kaynak-o18)


<a id="or-ops-008"></a>
### OR-OPS-008 — Hesabı yeni girişlere kilitleme

```yaml
id: OR-OPS-008
engine: oracle
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER USER

**Girdi:** user_name:identifier

```sql
ALTER USER {{ident:user_name}} ACCOUNT LOCK;
```

**Yorum / kullanım:** Mevcut sessionlar kendiliğinden sonlanmaz; proxy erişim/yetki revoke ve session politikası ayrıca kontrol edilir. Owner veya kritik servis hesabı etkisi gösterilir.

**Ön koşul / güvenlik:** SYS/SYSTEM/Oracle-maintained/root admin denylist; gerekçe ve audit.

**Kaynak:** [O01](#kaynak-o01)


<a id="or-ops-009"></a>
### OR-OPS-009 — Parola döndürme

```yaml
id: OR-OPS-009
engine: oracle
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER USER / self password change desteklenen ayrı akış

**Girdi:** user_name:identifier; new_password:secret

```sql
ALTER USER {{ident:user_name}} IDENTIFIED BY {{secret:new_password}};
```

**Yorum / kullanım:** NO AUTHENTICATION ownera uygulanmaz; harici/enterprise auth hesabını yanlışlıkla password autha çevirme. Vault ve app secret rollout ayrı çokadımlı süreçtir.

**Ön koşul / güvenlik:** Auth type PASSWORD preflight; parola policy; hiçbir log/SQLpreview gerçek secret göstermez; yeni bağlantı doğrulanır.

**Kaynak:** [O01](#kaynak-o01) [O02](#kaynak-o02)


<a id="or-ops-010"></a>
### OR-OPS-010 — Migration proxy yetkisini kaldırma

```yaml
id: OR-OPS-010
engine: oracle
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** ALTER USER

**Girdi:** owner_user,migrator_user:identifier

```sql
ALTER USER {{ident:owner_user}} REVOKE CONNECT THROUGH {{ident:migrator_user}};
```

**Yorum / kullanım:** Yeni proxy login testi başarısız olmalı. Mevcut proxy sessionların sonlandırılması ayrı karardır.

**Ön koşul / güvenlik:** Doğru client/proxy yönünü metadata ile doğrula; değişiklik öncesi dependent migration jobları göster.

**Kaynak:** [O02](#kaynak-o02)


<a id="or-ops-011"></a>
### OR-OPS-011 — Nesne yetkisini kontrollü revoke

```yaml
id: OR-OPS-011
engine: oracle
category: security
priority: P1
risk: R1
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** Yetkiyi geri alabilme

**Girdi:** object_privilege:READ|SELECT|INSERT|UPDATE|DELETE|EXECUTE; object:qualified; grantee:identifier

```sql
REVOKE {{enum:object_privilege}} ON {{qualified:object}} FROM {{ident:grantee}};
```

**Yorum / kullanım:** Tek privilege / tek nesne / tek grantee. Role/PUBLIC/ANY üzerinden erişim sürebilir; effective negative test şart. Dependent PL/SQL invalidation etkisi olabilir.

**Ön koşul / güvenlik:** Enum privilege nesne tipiyle eşleşmeli; grantoption zincirleri diffte görünür.

**Kaynak:** [O03](#kaynak-o03)


<a id="or-ops-012"></a>
### OR-OPS-012 — DROP USER CASCADE — uygulama schema verisini siler

```yaml
id: OR-OPS-012
engine: oracle
category: maintenance
priority: P2
risk: R3
scope: current_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** Oracle 19c tabanı; 21c/23ai/26ai için uyumluluk testi

**Yetki:** DROP USER

**Girdi:** user_name:identifier

```sql
DROP USER {{ident:user_name}} CASCADE;
```

**Yorum / kullanım:** User=Schema nedeniyle ownerın bütün nesneleri/verisi silinebilir. Runtime user ile owner seçimi ayırt edilmelidir. Tek rollback ile geri dönmez; account lock alternatifi göster.

**Ön koşul / güvenlik:** Sistem/common/Oracle-maintained denylist; exact adı yazdırma; object count/bytes/dependency/backup-restore planı ve iki kişi onayı. Mevcut sessionları otomatik kill etme.

**Kaynak:** [O01](#kaynak-o01)


<a id="bolum-8"></a>
# 8. AWS ve Azure bağlantı / yönetim katmanı

## 8.1 Data plane ve control plane ayrı

**Data plane:** Veritabanı protokolü, SQL login, database/schema/table permissions, sorgu ve oturumlar. **Control plane:** Cloud resource discovery, region/subscription, service status, storage tier, firewall/private endpoint, backup/PITR ve cloud metrikleri. Cloud hesabında Reader olmak SQL SELECT hakkı vermez; SQL admin olmak cloud resource değiştirme hakkı vermez. UI iki izin durumunu ayrı gösterir. [C01](#kaynak-c01) [C05](#kaynak-c05) [C06](#kaynak-c06)

Cloud bilgisi olmadan endpoint hostname'inden bölge/account/ürün türünü kesin kabul etme. Fingerprint için API resource ID + DB handshake kullan. İstemci, müşteri seçmeden bütün hesapları/regionları taramaz. Cloud özelliği kapalıyken local/SSH/SQL özellikleri çalışmaya devam eder.

| Platform | DB protokolü | Kimlik doğrulama yaklaşımı | Yönetim sınırı |
|---|---|---|---|
| AWS RDS PostgreSQL / uygun Aurora PostgreSQL | PostgreSQL | Password veya yapılandırılmış IAM | OS/root yok; parameter group, backup/storage cloud API; Aurora ve RDS farklı capability |
| AWS RDS SQL Server | SQL Server | Yapılandırılmış SQL/Windows profili | PostgreSQL IAM tokenı uygulanmaz; RDS task/procedure ve kısıtlı admin |
| AWS RDS Oracle | Oracle | Providerın desteklediği Oracle kimlik profili | SYS/SYSTEM/SYSDBA yok; desteklenen rdsadmin procedures |
| Azure PostgreSQL Flexible Server | PostgreSQL | Password veya Entra | azure_pg_admin gerçek superuser değildir; server parameters/provider API |
| Azure SQL Database | TDS | Contained password user veya Entra | OS/file/server yönetimi self-managed gibi değil; DB scoped katman |
| Azure SQL Managed Instance | TDS | Platformda yapılandırılmış SQL/Entra/uygun profil | SQL Database ile birebir özellik eşitliği yok; MI capability ayrı |

Bu bir bütün auth seçenekleri listesi değildir; ürünün ilk doğrulanacak profilleridir. SQL Server/Oracle için AWS PostgreSQL IAM desteği varsayılmaz. Oracle'ın Azure/çoklu bulut dağıtımları ayrı provider adapter olmadan Azure SQL diye sınıflandırılmaz. [C01](#kaynak-c01) [C02](#kaynak-c02) [C07](#kaynak-c07) [C13](#kaynak-c13)

## 8.2 AWS geliştirme sözleşmesi

Resmî SDK, mevcut credential chain/SSO veya kullanıcı tarafından seçilen assume-role oturumu kullan. Access key'i kodda/config örneğinde gömme. Hesap ve region açık seçimlidir. RDS `DescribeDBInstances`, cluster modelleri için ilgili cluster discovery, pagination ve pagination başarısızlığında partial-result durumu uygulanır. Dönen tüm resource tiplerini desteklenen motor sanma; engine allowlist gereklidir. [C03](#kaynak-c03)

Cloud inventory modeli: `provider, account_id, region, resource_id, engine, engine_version, service_model, endpoint, port, role, state, tags, network_mode, auth_capabilities, last_seen_at`. Credential bu modele düz metin alan olarak eklenmez. Writer endpoint ile reader endpoint ayrı bağlantı amacıdır.

Metrikler için motor/resource tipine göre desteklenen CloudWatch namespace/dimension keşfedilir: CPU, bağlantılar, boş storage, IO throughput/latency, replica lag, burst/credit ölçümleri varsa. Metric adı/birimi/aggregation SDK/API metadata ve testle doğrulanır; bir motordaki lag metriğini diğerine aynen kopyalama. API okuma maliyeti/rate limit; batching, cache, jitter, backoff ve stop-on-hidden-panel uygulanır. Eksik veri interpolasyonla sağlıklı gösterilmez.

IAM authentication etkinleştirme bir cloud değişikliğidir; yalnızca SQL role grantinden ibaret değildir. DB kullanıcısı, `rds-db:connect` resource scope, ağ/TLS ve auth flag birlikte doğrulanır. SDK ile üretilen token login sırasında kullanılır ve kalıcı vault parolası gibi saklanmaz. Tokenın 15 dakikalık ömrü açılmış SQL sessionının 15 dakikada otomatik kapanması demek değildir. rds_iam üyeliği PostgreSQL'de parola girişinin önceliğini değiştirebilir; mevcut üretim hesabına habersiz ekleme. [C01](#kaynak-c01)


<a id="pg-cloud-001"></a>
### PG-CLOUD-001 — RDS PostgreSQL: IAM login role ve dar data rolü

```yaml
id: PG-CLOUD-001
engine: postgresql
category: cloud_provision
priority: P1
risk: R1
scope: cluster + DB permission profile
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** RDS PostgreSQL / IAM destekli ve ayrı test edilmiş Aurora PostgreSQL

**Yetki:** Role create/grant yetkisi + ayrı AWS IAM connect policy

**Girdi:** iam_db_user,permission_role:identifier

```sql
CREATE ROLE {{ident:iam_db_user}} LOGIN
  NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;
GRANT rds_iam TO {{ident:iam_db_user}};
GRANT {{ident:permission_role}} TO {{ident:iam_db_user}};
```

**Yorum / kullanım:** permission_role önceden PG-PROV adımlarıyla dar DB/schema yetkisine sahip olmalıdır. IAM user için password üretme. Var olan kullanıcının auth modelini sessiz değiştirme.

**Ön koşul / güvenlik:** rds_iam varlığı tek başına yeterli değil: cloud flag, region/engine support, TLS endpoint ve IAM policy preflight. IAM rolünü bütün DBA kullanıcılara topluca verme.

**Kaynak:** [C01](#kaynak-c01) [C04](#kaynak-c04)


<a id="pg-cloud-002"></a>
### PG-CLOUD-002 — Azure PostgreSQL: Entra principal envanteri

```yaml
id: PG-CLOUD-002
engine: postgresql
category: cloud_discovery
priority: P1
risk: R0
scope: provider maintenance database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: read_only
```

**Hedef:** Azure PostgreSQL Flexible Server; Entra yapılandırılmış

**Yetki:** pgaadauth_list_principals erişimi

**Girdi:** Yok

```sql
SELECT * FROM pg_catalog.pgaadauth_list_principals(false);
```

**Yorum / kullanım:** false adminler dahil bütün mapped principal listesidir. Object id / tenant id erişim kontrolüne tabi metadata olarak saklanır.

**Ön koşul / güvenlik:** Fonksiyon/sürüm/scope probe; generic PostgreSQLde çağrılmaz. Sonuç sütunlarını provider capability schema ile normalize et.

**Kaynak:** [C08](#kaynak-c08)


<a id="pg-cloud-003"></a>
### PG-CLOUD-003 — Azure PostgreSQL: Object ID ile Entra hesabı eşleme

```yaml
id: PG-CLOUD-003
engine: postgresql
category: cloud_provision
priority: P1
risk: R1
scope: provider maintenance DB; data grants target DBde ayrı
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Azure PostgreSQL Flexible Server; ilgili pgaadauth API mevcut

**Yetki:** Entra admin / provider destekli identity yönetimi + PG grant

**Girdi:** role_name:text=mapped_role exact identifier; object_id:UUID; object_type:user|group|service; permission_role,mapped_role:identifier

```sql
SELECT * FROM pg_catalog.pgaadauth_create_principal_with_oid(
  :role_name, :object_id, :object_type, false, false);
GRANT {{ident:permission_role}} TO {{ident:mapped_role}};
```

**Yorum / kullanım:** SELECT ile başlayan ilk ifade role oluşturur, read-only değildir. isAdmin=false; application için service principal object id kullan, application/client id değil. İkinci false MFA claim kontrolü kapalı başlangıç profili; insan erişiminde tenant policy ve MFA claim testiyle ayrı varyant seç.

**Ön koşul / güvenlik:** Mapping çakışması adoption gerektirir; role_name ile mapped_role aynı olmalı. Runtime rolea azure_pg_admin verme. MFA flag token alma akışını kendi başına açmaz.

**Kaynak:** [C08](#kaynak-c08) [C05](#kaynak-c05)


<a id="ms-cloud-001"></a>
### MS-CLOUD-001 — Azure SQL: Entra kullanıcı/grup ve dar rol üyeliği

```yaml
id: MS-CLOUD-001
engine: mssql
category: cloud_provision
priority: P1
risk: R1
scope: target_database
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** Azure SQL Database / desteklenen MI Entra profili

**Yetki:** Entra yapılandırılmış admin/uygun SQL user yönetimi ve identity lookup izinleri

**Girdi:** entra_principal:validated resolved Entra identity identifier; permission_role:identifier

```sql
CREATE USER {{ident:entra_principal}} FROM EXTERNAL PROVIDER;
ALTER ROLE {{ident:permission_role}} ADD MEMBER {{ident:entra_principal}};
```

**Yorum / kullanım:** Display name çakışması ve directory lookup eksikliği açık conflict döndürür. Object-ID destekli syntax varyantı provider/sürüm testi sonrası eklenir; tahmin edilmiş SID/object mapping üretme.

**Ön koşul / güvenlik:** db_owner verme; ro/rw schema rolü kullan. Microsoft Graph lookup izinlerini SQL data grantleriyle karıştırma; ayrı least-privilege politika.

**Kaynak:** [C06](#kaynak-c06) [M02](#kaynak-m02)


<a id="ms-cloud-002"></a>
### MS-CLOUD-002 — Azure SQL: Geo-replication durumu

```yaml
id: MS-CLOUD-002
engine: mssql
category: cloud_monitor
priority: P0
risk: R0
scope: target_database
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** Azure SQL geo-replication destekli yapılandırma / view capability

**Yetki:** Platform/sürüm için ilgili DMV izni

**Girdi:** Yok

```sql
SELECT link_guid, partner_server, partner_database, last_replication,
       replication_lag_sec, replication_state_desc, role_desc
FROM sys.dm_geo_replication_link_status;
```

**Yorum / kullanım:** Null/stale lag ve primary/secondary tarafındaki görünürlük ayrı değerlendirilir. Partner bilgisi connection oluşturmak için otomatik secret sağlamaz. Failover ayrı R3 cloud workflowdur.

**Ön koşul / güvenlik:** View/column capability yoksa unsupported; self-managed AG sorgusunu bu view yerine kullanma.

**Kaynak:** [C12](#kaynak-c12)


<a id="ms-cloud-003"></a>
### MS-CLOUD-003 — RDS SQL Server: Native task durumu

```yaml
id: MS-CLOUD-003
engine: mssql
category: cloud_monitor
priority: P0
risk: R0
scope: msdb
cost: light
timeout_seconds: 10
poll_seconds: 30
transaction: read_only
```

**Hedef:** AWS RDS SQL Server; procedure görünür ve native task özelliği destekli

**Yetki:** Provider task visibility / procedure execute

**Girdi:** db_name:text exact DB

```sql
EXEC msdb.dbo.rds_task_status @db_name=:db_name;
```

**Yorum / kullanım:** Read-only amaçlı allowlisted stored procedure. %100 veya created row tek başına success değil; lifecycle/status/message birlikte normalize edilir. Managed otomatik PITR backup durumunun yerine geçmez.

**Ön koşul / güvenlik:** Result set versionını test et; mesajlarda ARN/path redaction. EXEC başladı diye mutasyon veya güvenli diye evrensel sınıflandırma yapma.

**Kaynak:** [C11](#kaynak-c11)


<a id="ms-cloud-004"></a>
### MS-CLOUD-004 — RDS SQL Server: Yeni S3 nesnesine native FULL backup

```yaml
id: MS-CLOUD-004
engine: mssql
category: cloud_maintenance
priority: P1
risk: R2
scope: msdb
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: action_specific
```

**Hedef:** AWS RDS SQL Server native backup option yapılandırılmış

**Yetki:** Provider procedure execute + servis S3/KMS erişimi

**Girdi:** db_name:text; s3_object_arn:validated approved new destination

```sql
EXEC msdb.dbo.rds_backup_database
  @source_db_name=:db_name,
  @s3_arn_to_backup_to=:s3_object_arn,
  @overwrite_s3_backup_file=0,
  @type='FULL';
```

**Yorum / kullanım:** Async provider task başlatır; dönen task_id saklanır ve MS-CLOUD-003 ile takip edilir. Uygulama kapanınca taskın durduğu varsayılmaz. Şifreleme politikası SSE-S3/SSE-KMS/client-side KMS varyantı olarak ayrıca doğrulanır.

**Ön koşul / güvenlik:** Allowlisted bucket/prefix ve kesin veri dışa aktarım izni; yeni unique object; overwrite kapalı. Storage/IO maliyeti ve provider maintenance/backup çakışmaları önceden kontrol edilir.

**Kaynak:** [C11](#kaynak-c11)


<a id="or-cloud-001"></a>
### OR-CLOUD-001 — RDS Oracle: Provider prosedürüyle SQL cancel

```yaml
id: OR-CLOUD-001
engine: oracle
category: cloud_maintenance
priority: P0
risk: R2
scope: target RDS Oracle instance
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** AWS RDS Oracle 19c+; desteklenen cancel procedure

**Yetki:** rdsadmin provider procedure execute

**Girdi:** sid,serial_number:positive int; sql_id:validated non-null SQL_ID

```sql
BEGIN
  rdsadmin.rdsadmin_util.cancel(
    sid => :sid, serial => :serial_number, sql_id => :sql_id);
END;
```

**Yorum / kullanım:** Self-managed ALTER SYSTEM yerine provider yolu. V$SESSIONdan güncel sid/serial/sql_id doğrulanır; cancellation full transaction rollback demek değildir.

**Ön koşul / güvenlik:** SYS/RDSADMIN/background denylist, connection fingerprint, açık onay. SQL_ID null kullanma. PL/SQL slash drivera gönderilmez.

**Kaynak:** [C09](#kaynak-c09)


<a id="or-cloud-002"></a>
### OR-CLOUD-002 — RDS Oracle: Seçili SYS view için monitor grant

```yaml
id: OR-CLOUD-002
engine: oracle
category: cloud_provision
priority: P1
risk: R1
scope: RDS Oracle
cost: light
timeout_seconds: 10
poll_seconds: 0
transaction: implicit_commit
```

**Hedef:** AWS RDS Oracle; provider destekli SYS object grant

**Yetki:** Provider masterın sahip olduğu ve devredebileceği ilgili hak

**Girdi:** sys_view_name:verified dependency allowlist; monitor_user:exact metadata name

```sql
BEGIN
  rdsadmin.rdsadmin_util.grant_sys_object(
    p_obj_name => :sys_view_name,
    p_grantee => :monitor_user,
    p_privilege => 'SELECT');
END;
```

**Yorum / kullanım:** Örneğin V_$SESSION dependency; generic SELECT ANY DICTIONARY yerine action bazlı seçim. Provider masterda olmayan hak yaratılmaz.

**Ön koşul / güvenlik:** Lisanslı ASH/AWR viewları otomatik listeden çıkar; hedef view adı uppercase tahmini değil dictionary değeri. ALL/EXECUTE privilege serbest input değil.

**Kaynak:** [C10](#kaynak-c10) [C13](#kaynak-c13)


## 8.3 Azure geliştirme sözleşmesi

Tenant → subscription → resource group → resource ID seçimi. Resmî identity/management SDKları ve desteklenen stable API versionları, uygulamanın bağımlılık kilidi ve CI testiyle kullanılır. PostgreSQL Flexible Server, SQL logical server/database ve SQL Managed Instance ayrılır. Public cloud dışındaki otorite/endpointler hardcode edilmez.

Masaüstünde kullanıcı etkileşimli Entra oturumu veya açık seçilmiş mevcut CLI/credential profili kullanılabilir. **Managed identity**, yalnızca bunu sağlayan Azure execution ortamında kullanılabilir; herhangi bir kullanıcının laptopunda otomatik mevcut değildir. Token audience kaynak türüne uygun SDK ile istenir; ARM tokenını SQL bağlantısında kullanma. Parola ile token auth fallbackı sessiz yapılmaz. [C05](#kaynak-c05) [C06](#kaynak-c06)

İlk sürümde resource discovery, status, server parametersın okunması, auth kabiliyeti, backup/PITR metadata ve Azure Monitor metric okuma yeterlidir. Firewall açma, storage büyütme, HA değiştirme, restart/failover, restore ve private endpoint değişiklikleri **ayrı change plan + RBAC + maliyet/etki + approval** ister. Ağa erişilemiyorsa program otomatik `0.0.0.0/0` açmaz.

## 8.4 Cloud/host entegrasyonu kapsamı

SQL ile her şey görülmez: self-managed disk/CPU/servis/log için müşterinin yetkili SSH veya isteğe bağlı collector bağlantısı; PaaS için cloud metrics/log APIsi. RDS/Azure DB node'una generic SSH düğmesi gösterilmez. Bastion tüneli, DB sunucusunun OS yetkisi demek değildir. [C02](#kaynak-c02) [C07](#kaynak-c07) [C13](#kaynak-c13)

AWS/Azure bağlama salt okunur keşifle başlar. Hassas credential retrieval için Secrets Manager/Key Vault ayrı ve resource-scoped erişimdir; hesap eklendi diye bütün secretları listeleme/indirme. Entra/IAM kimlik bilgisi başarıyla alındığı halde database grant yoksa açık `authenticated_but_not_authorized` tanısı ver.

<a id="bolum-9"></a>
# 9. Health check kuralları ve olay runbook'ları

## 9.1 Çıktı modeli

Her bulgu: `rule_id, resource_fingerprint, detected_at, observed_window, status, severity, evidence_action_ids, evidence_values_with_units, scope, confidence, likely_causes, alternative_causes, next_read_actions, proposed_write_actions, limitations`.

**Tanı = kanıt + kapsam + belirsizlik.** "Slot inactive" tek başına consumer bozuk demek değildir. "Backup listesi boş" backup hiç yok demek değildir. Kullanıcı bu varsayımları UI'da görür. Kesin root cause yoksa `possible_cause` yaz; başka DB/hosttaki korelasyonu kesin nedensellik diye pazarlama.

Başlangıç ürün policy örnekleri (evrensel DBA eşikleri değil): connection kullanımında warning %80/critical %90; storage oranında warning %80/critical %90; ama byte olarak kalan alan ve büyüme hızıyla birleşir. Uzun query eşiği workload profiline göre; 30 saniyelik OLTP ile 2 saatlik ETL aynı değildir. Warning için ardışık 3 örnek, recovery için 3 normal örnek varsayılan önerisidir. Corruption / replication invalidation / DB SUSPECT gibi anlamlı kritik durumlar ayrıca hemen uyarılabilir. Kurum SLO/backup RPO/RTO/replication politikası ayarları baskındır.

## 9.2 PostgreSQL runbook'ları

| Olay | Kanıt akışı | Yorum / güvenli sonraki adım |
|---|---|---|
| WAL büyüyor / disk basıncı | PG-MON-012, 013, 014, 015; PG-MON-009, 011; host/cloud storage | Üretim hızı, archiver başarısızlığı, slot restart LSN, replica apply ve retention parametrelerini ayır. `pg_wal` dosyası elle silinmez. |
| Slot retention artıyor | PG-MON-012 → consumer/replica durumu → 2+ LSN örneği | Inactive, consumer offline veya yavaşlık olabilir; restore/repack/bulk WAL üretim patlaması ayrı kanıt. Slot drop ancak R3 kurtarma/resnapshot planıyla. |
| Blocking / idle transaction | PG-MON-005, 006, 007, 008, 016 | Query süresi ile transaction yaşını ayır; prepared tx görünümünü de kontrol et. Önce uygulama ownerına, sonra kontrollü cancel/terminate. |
| Replica lag | PG-MON-009, 010, 011, 046; primary WAL counters | Network/send/write/flush/replay aşamasını ayır. Primary ve standby örnek zamanı eşleştirilir; boş trafikte replay timestamp yaşı otomatik gerçek lag değildir. |
| Autovacuum / freeze baskısı | PG-MON-017, 018, 019, 020, 047 | XID yaşını yapılandırılmış freeze eşiği ve hard safety limitleriyle ayrı yorumla. Long tx/slot xmin/replica feedback horizon etkisi incelenir. |
| Sorgu yavaşlığı | PG-MON-025, 026, 027, 028, 029; EXPLAIN paneli | CPU sorgusu diye total_exec_time sunma. Yeni index adayı workload/plan/test ile doğrulanır; query text paylaşımı ayrı izin. |
| Sahiplik/yetki sapması | PG-PROV-009; PG-MON-031, 032, 033, 034, 035, 048 | Creator owner/default grant/role graph/PUBLIC/RLS/routine etkisi ayrılır. Önce diff; bütün kullanıcılara broad grant yok. |

**WAL vaka akışı:** Son iki örnekte current WAL LSN üretim hızı artıyor mu? `restart_lsn` ilerliyor mu, `confirmed_flush_lsn` ilerliyor mu? Archiver `failed_count` delta artıyor mu? Slot active ve logical consumer commit ediyor mu? Primaryde uzun transaction veya restore/repack işi var mı? Host/provider diski kalan süreyi kritik hale getiriyor mu? Bunların sonuçları ilişkilendirilir; yalnızca publicationa 10 tablo koymanın PostgreSQL'in bütün diğer işlerinin WAL üretimini durduracağı iddia edilmez. Slot retention, gönderilen logical event sayısıyla birebir aynı metrik değildir. [P05](#kaynak-p05) [P06](#kaynak-p06)

## 9.3 SQL Server runbook'ları

| Olay | Kanıt akışı | Yorum / güvenli sonraki adım |
|---|---|---|
| Transaction log doluyor | MS-MON-002, 006, 009, 010, 039 | LOG_BACKUP, ACTIVE_TRANSACTION, AVAILABILITY_REPLICA, REPLICATION gibi reuse engelleri ayrılır. SHRINK veya SIMPLE'a geçiş otomatik çözüm değil. |
| Blocking / uzun request | MS-MON-004, 005, 006, 008 | Blocker SPID, login_time, open tx ve wait chain; negatif blocker kodları process kill hedefi değildir. KILL sonrası rollback sürebilir. |
| Tempdb baskısı | MS-MON-013, 014, 010, 012 | Version store/user/internal object ayrımı; autogrowth/disk ve uzun transaction birlikte. Tempdb restart/shrink otomatik yok. |
| IO / memory baskısı | MS-MON-007, 011, 015, 016, 017 | Lifetime wait/IO yerine delta; memory grant pending ve runnable queue ayrı; cache clear ile belirtiyi gizleme. |
| AG send/redo queue | MS-MON-028 + host/cloud network/IO | Send queue ve redo queue farklı bottleneck; KB/s birimlerini doğru dönüştür, safety/sync mode kontrol et. |
| Backup / integrity | MS-MON-026, 027; MS-OPS-006/007 elle | Son job success ve backup history yeterli kanıt değildir; restore testi ve checksum/integrity kapsamı ayrı. REPAIR_ALLOW_DATA_LOSS yok. |
| Eksik index / regresyon | MS-MON-018, 019, 020, 022, 023, 024, 025 | Missing index recommendation proof değil; duplicate/overlap/write maliyeti ve QStore planları incelenir. |

Log reuse sorununda backup chain bozan recovery model değişimini önerilen ilk düğme yapma. Azure SQL Database auto backup/PITR modeli için MSDB full backup querysinin boş sonucunu alarm üretiminde kullanma. [M07](#kaynak-m07) [M13](#kaynak-m13) [C07](#kaynak-c07)

## 9.4 Oracle runbook'ları

| Olay | Kanıt akışı | Yorum / güvenli sonraki adım |
|---|---|---|
| Tablespace / kota / disk | OR-MON-016, 017, 031, 036 + host/cloud | Kullanıcı kotası, current allocation, maxsize ve fiziksel disk farklı. Önce doğru sebep, sonra bounded quota/storage plan. |
| TEMP / UNDO | OR-MON-007, 018, 019, 020 | Büyük sort/hash/versioned read, uzun transaction ve ORA-01555 ilişkisi; bütün sessionları kill etme. |
| FRA / archive | OR-MON-021, 022, 023, 025 | RMAN retention, Data Guard tüketimi ve guaranteed restore pointler; archive dosyalarını OSden silme. |
| Blocking / uzun SQL | OR-MON-004, 005, 006, 007, 008, 009 | SID+SERIAL#+INST_ID+CON_ID; cancel tercih edilir, kill ayrı kapsam. Root yerine doğru PDB hedefi. |
| Data Guard lag | OR-MON-001, 022, 023, 024 | Transport/apply ve datum freshness birlikte; 0 stale lag yeşil değil. Otomatik switchover/failover yok. |
| Performance | OR-MON-010, 011, 012, 013, 014, 015, 030, 041 | Pack gerektirmeyen ilk yollar; yalnız açık lisanslı panel OR-MON-045/046ya geçer. |
| Hata / corruption / job | OR-MON-025, 026, 027, 028, 029, 032, 043 | Corruption kaydı kritik; otomatik repair/recover/2PC force işlemi yok. |

Oracle AWR/ASH/SQL Monitoring ekranlarının görünmesi entitlementa bağlıdır. `CONTROL_MANAGEMENT_PACK_ACCESS=DIAGNOSTIC+TUNING` veya view SELECT hakkı lisans hakkının yerini tutmaz. Kullanıcının şirketi lisans hakkını kendi sözleşmesi/sağlayıcı SKU'su üzerinden doğrular. [O10](#kaynak-o10)

## 9.5 Backup ve PITR: SQL dışı ihtiyaçlar

PostgreSQL core katalogları tek başına pgBackRest/Barman/WAL-G repositorydeki bütün backup zincirini doğrulamaz. İlgili toolun sürümlü JSON/info API çıktısı read-only adapterla işlenir. PG base backup başlama/bitiş zamanı, required WAL aralığı, timeline, encryption key erişimi, son successful restore test tarihi ayrı alanlardır. Backup başlangıcından sonraki ama consistency noktasından önceki bir zamanı her base backuptan açılabilir sanma.

SQL Server: FULL/DIFF/LOG zinciri, LSN ve recovery fork uyumu, media erişimi ve encryption key varlığı. Oracle: RMAN catalog/controlfile kayıtları, archivelog zinciri/incarnation ve wallet/TDE erişimi. Cloud: restore window/latest restorable time/provider task durumu. Sadece metadata ile `restore_verified=true` üretilmez.

İlk sürüm **restore planı önizlemesi** üretir, mevcut production DB üzerine restore yapmaz. Restore hedefi ayrı isim/path/endpoint ve izole ağ; uygulama jobları/email/CDC otomatik tetiklenmemeli. Testten sonra cleanup R3 ayrı onaydır. Bu belge tam felaket kurtarma uygulayıcısı/backup orchestration ürününün yerine geçmez.

<a id="bolum-10"></a>
# 10. Test matrisi, teslim koşulları ve Codex görevleri

## 10.1 SQL kataloglarının uygulama koduna dönüşümü

Her `*-PROV`, `*-MON`, `*-OPS`, `*-CLOUD` kaydı için UI butonu/menü girdisi, typed input formu, engine adapter, query template, capability+permission probe, result schema, hata eşlemesi, unit mapping, snapshot test ve integration test üret. Sadece bu MDyi repo içine kopyalamak veya butona statik demo JSON bağlamak tamamlanma sayılmaz.

Aynı SQL bloğunda yer alan ifadeler kör topluca drivera gönderilmez. **Sıralı workflow** olanlar adımlara; **alternatif sürüm/platform seçenekleri** olanlar varyantlara ayrılır. Örneğin MS-PROV-011 dört bağımsız permission varyantı, PG-PROV-003 autocommit adımı, OR-PROV-002 birkaç implicit-commit adımıdır. SQL parser metadata gerektirir; satır veya noktalı virgül bölme yeterli değildir.

`transaction: read_only` uygulamanın niyet sözleşmesidir, üç motora aynı SQL metnini gönderme talimatı değildir. PostgreSQL için kısa `BEGIN READ ONLY` veya read-only session policy; SQL Server için gerçek read-only yetkili kimlik + uygun autocommit/transaction API; Oracle için desteklenen session/transaction yöntemi uygulanır. Dinamik performans viewlarının snapshot tutarlılığı ayrıca test edilir. `R0` altında kendi kendine privilege grant edilmez.

Registry ek zorunlu alanları (bu MDdeki açıklamalardan kodda yapılandır): `parameters` tam tip ve bounds; `variants` exact engine/version/provider; `required_capabilities`; `required_permissions`; `license_requirements`; `statement_boundaries`; `result_columns` type/unit/null semantics; `visibility_scope`; `secret_fields`; `preflight`; `postcheck`; `approval_policy`; `known_failure_modes`. Prose notlar makine-readable implementasyona dönüşmeden action production enabled olamaz.

## 10.2 Render ve güvenlik testleri

| Test | Başarı ölçütü |
|---|---|
| Identifier escaping | Boşluk, Türkçe Unicode, çift tırnak, `]`, nokta, reserved word, mixed case güvenli. Bir identifier içindeki nokta schema ayracı yapılmaz. NUL/control character/uzunluk limit ihlali reddedilir. |
| Bind lexer | PostgreSQL `::type`, dollar-quoted body, Oracle `:=`, T-SQL `@param`, string/comment içindeki `:fake` bozulmaz. Eksik/ek parametre fail-closed. |
| DDL secret serialization | PG/T-SQL string literal ile Oracle password token farklı test edilir. Sırlar log, audit, crash, clipboard preview ve telemetryde maskelenir. |
| Injection negatif testi | `name; DROP...`, quote kaçışı, bozuk order/filter/type inputu kod çalıştırmaz; yalnız validated AST. Metadata kaynağının kendisi untrusted input sayılır. |
| Transaction izolasyonu | Monitor/editor/migrator birbirinin transactionını commit/rollback etmez; pool dönüşünde role/search_path/session state temizlenir. |
| Approval binding | Parametre/SQL/target fingerprint/risk/katalog hash değişince onay geçersiz. Client bypass enforced serviste reddedilir. |
| Network timeout | Write bilinmeyen sonucu success/fail diye tahmin etmez; otomatik tekrarlamaz; reconcile planı çıkarır. |
| Authentication | Hatalı TLS hostname/CA, değiştirilmiş SSH host key, expired token fail-closed. TLS trust-all fallbackı yok. |
| Data privacy | LOB ve çok satırlı sonuç limiti, CSV formula safe export, masked query text, tenant ACL ve vault snapshot sızıntı testleri. |
| Custom runbook | Yayıncı role, hash/signature, review ve risk classification. SELECT ile mutasyon fonksiyonunu read-only saymaz. |

## 10.3 Motor test matrisi

**PostgreSQL:** Test hedefleri 14/15/16/17/18; vanilla primary+physical standby; logical slot/publication; pg_stat_statements var/yok; pgstattuple var/yok; superuser, pg_monitor, dar runtime ve reader. PG16 membership OPTIONS ve PG17 checkpointer ayrımı; sürüme göre wal_receiver/stat view kolonları; RDS/Azure profilleri ayrı integration ortamında. Extension bulunmuyorsa UI kendisi kurmaz.

**SQL Server:** Test hedefleri 2019/2022/2025; ürün edition destek sınırları; CI için lisansa uygun development kurulumu; server login+user, contained users, schema owner, DENY ve nested roles. `VIEW SERVER STATE`/`VIEW SERVER PERFORMANCE STATE`, DB scoped varyantlar; QStore off/on; AG enabled/disabled/secondary; Agent var/yok; Azure SQL DB ve MI / RDS ayrı test ortamları. Developer edition testi production edition hakkı ispatı değildir.

**Oracle:** Hedef 19c; lisans/ortam uygun olduğunda 21c/23ai/26ai ayrıca. PDB service ve root mismatch; NO AUTHENTICATION owner + gerçek proxy session; READ/SELECT FOR UPDATE farkı; migration sonrası grants; pack entitlement false/true; V$/GV$ kolon ve grant varyantları; RAC olmayan ortamda RAC vaadi yok. Ücretsiz test editionı, 19c enterprise/RAC veya managed provider desteğini tek başına doğrulamaz.

**Managed test maliyeti:** CI cloud resource yaratma opt-in, bütçe/kota ve otomatik süre sonu tagleriyle ayrı iş. Hatalı test production subscriptiona yönlenmez. API mockları sadece unit contract testi; gerçek managed uyumluluk iddiası için gerçek sandbox integration gerekir.

## 10.4 Provisioning kabul senaryoları

1. Tamamen boş ortamda DB/schema/owner/migrator/RW/RO/app/reader yaratılır; kimlikler birbirinden ayrı. Gerçek kullanıcı bağlantılarıyla pozitif ve negatif testler geçer.
2. Aynı istenen durum ikinci kez uygulanınca `already_compliant`; parola/sahiplik/değişmez DB parametresi değiştirilmez. Aynı ad/farklı owner-SID-object-ID conflict olur.
3. Migration kimliği yeni tablo yaratır; nesne sahibi doğru olur. Mevcut ve **gelecekteki** tabloda app RW, reader RO doğrulanır. Oracle reconciliation başarısızsa migration sonucu `permissions_incomplete` olur.
4. App: kendi verisinde izinli DML var; owner switch/role admin/user create/DDL/grant/başka schema yok. Reader: write/DDL/mutating routine yok. Oracle reader için SELECT FOR UPDATE reddedilir. PG reader için default read-only ayarı kapatılsa bile gerçek izinler write engeller.
5. Mevcut DBye adoption dry-run mevcut uygulama erişimini bozacak PUBLIC revoke veya owner değişikliğini gösterir, açık ayrı onay olmadan uygulamaz.
6. Bağlantı her adımda kesilerek kısmi başarı/unknown outcome test edilir. Oluşmuş DB otomatik silinmez; eldeki nesneler gerçek discovery ile yeniden değerlendirilir.

## 10.5 Monitoring ve olay testleri

Disposable DBlerde kontrollü blocker/idle transaction/rollback/long query yarat; belirlenen actionın doğru scope ve birimle bulduğunu doğrula. Test süresince watchdog ve cleanup vardır; paylaşılan production üzerinde yapılmaz. Slot ve WAL testleri küçük limitlerle izole volume üzerinde; SQL Server log/tempdb ve Oracle FRA/UNDO için diski gerçekten doldurma yerine güvenli fixture/sınırlandırılmış lab kullan.

NULL/empty/permission-denied/unsupported/stale/reset senaryoları farklı UI badge üretir. Büyük integer LSN/byte/SCN değerleri JavaScript Number gibi 53-bit hassasiyet sınırında kaybolmaz; bigint/string decimal kullan. SQL zaman damgaları offset/NLS farkıyla doğru normalize edilir. Birden çok instance/table aynı adlıysa kimlikler çakışmaz.

Overhead ölçümü gerçek test koşullarıyla raporlanır: sorgu süreleri p50/p95, dönen byte/satır, DB CPU/IO etkisi, connection sayısı, collector bellek. "Sıfır yük" garantisi verilmez. Ağır/statistik tarama ve licensed checks otomatik poll listesine girmez.

## 10.6 UI kabul koşulları

Quick Actions arama/filtre: engine, kategori, önem, salt okunur/yazma, kullanılabilir/yetki gerekli. Her butonda `Çalıştır`, `SQL göster`, `Kopyala`, `Editörde aç`, `Neden bu kontrol?`, `Gerekli izin`, `Son çalıştırma ve süre`. Tehlikeli buton read-only sonuç tablosunun primary CTA'sı yapılmaz; ilgili işlemler menüsündedir.

Provisioning wizard: hedef → sahiplik modeli → uygulama/okuma profili → locale/collation/storage/kota → mevcut/gelecek nesne kapsamı → migration kimliği → effective permission diff → SQL preview → approval → step timeline → test sonucu. `Grant successful` ile `Uygulama yetkisi doğrulandı` farklı durumlardır.

Health panel: raw evidence tıklanabilir; missing metric gri/unknown; sample freshness; scope DB/PDB/cluster; latency, transaction ve query süresi farklı. Dört ekran arasında connection seçimi değişse bile eski yazma onayı kullanılamaz. Production banner yalnız renk değil açık metin ve fingerprint içerir.

## 10.7 Depo ve teslim yapısı önerisi

Mevcut stacke uyarlayın; çalışan proje sırf bu öneri yüzünden yeni frameworke taşınmaz.

```text
core/connections/         # driver, TLS, tunnel, lifecycle
core/secrets/             # vault refs, redaction, token refresh
core/actions/             # registry, typed renderer, executor, policy
engines/postgresql/       # capability, provisioning, actions, mappers
engines/mssql/
engines/oracle/
cloud/aws/                # discovery, auth, metrics, provider variants
cloud/azure/
health/rules/             # evidence-based rules, thresholds, freshness
health/runbooks/
ui/db-explorer/
ui/sql-editor/
ui/admin-actions/
ui/provisioning/
ui/health/
tests/unit/
tests/catalog-golden/
tests/integration/
tests/security/
docs/adr/
docs/support-matrix/
```

Her PR tek aşama veya küçük bağlı alt aşama; migration ve backwards compatibility notları; test edilen engine/provider sürümleri; desteklenmeyen aksiyonların listesi; değişen güvenlik sınırı. Oracle/SQL Server sürücü dağıtımı, üçüncü taraf lisans ve bildirimleri ayrıca incelenir; bağımlılıkların ticari üründe dağıtım hakkı otomatik varsayılmaz.

**Codex definition of done:** Çalışan gerçek bağlantı ve UI; registryde executable capability variants; tests; SQL kaynak izi; SQL validation raporu; yetki negatif testleri; sensitive-data redaction; kullanıcıya açık destek matrisi. Runtime hatasını yutan ve "healthy" döndüren fallback kabul edilmez.

## 10.8 Sürüm 1 kapsamı dışındaki kontrollü genişleme

Bu katalog günlük DBA operasyonlarını kapsayan geniş bir başlangıçtır; her edition, eklenti ve HA ürününün bütün SQLleri değildir. Patroni/etcd/Consul/HAProxy/PgBouncer servis yönetimi; pgBackRest/Barman repository doğrulama; SQL Server transactional replication/SSIS/SSRS; Oracle GoldenGate/ASM/Exadata/Database Vault; restore/failover orchestration ayrı driver, lisans, güvenlik ve test kapsamıdır. UI sahte destek göstermek yerine `planned / unsupported` der. İleride bunlar aynı action contractına eklenir.

**İlk uygulanacak iş:** Aşama 0 depo envanteri; sonra PostgreSQL bağlantı + Explorer + PG-PROV-001 ve P0 read-only kontroller. Provisioning R1 adımları, renderer ve approval testleri tamamlanmadan etkinleştirilmez. Oracle ve cloud kataloglarının dosyada olması, hepsinin tek PRda güvenle uygulanacağı anlamına gelmez.


<a id="bolum-11"></a>
# 11. Aksiyon envanteri ve kaynaklar

## 11.1 Kataloğun kapsamı ve kullanım sırası

Önce provisioning keşfi ve bağlantı kimliği; ardından P0 read-only kontroller; daha sonra P1/P2 ayrıntılı analizler. R1/R2/R3 aksiyonlar otomatik health polling listesinin dışında tutulur. Aynı aksiyon içindeki birden fazla SQL statement tek bir amaç için paketlenmiş olabilir; aşağıdaki sayılar statement değil, **bağımsız action kimliği** sayısıdır.

| Motor | Çekirdek provisioning | Çekirdek kontrol | Çekirdek bakım | Cloud varyantı | Toplam |
|---|---:|---:|---:|---:|---:|
| PostgreSQL | 10 | 48 | 15 | 3 | 76 |
| Microsoft SQL Server | 12 | 40 | 13 | 4 | 69 |
| Oracle | 11 | 46 | 12 | 2 | 71 |


**Toplam: 216 aksiyon.** Öncelik ve risk birbirinden bağımsızdır: P0 kontrolün kritik olduğunu, R3 işlemin geri dönüşünün zor olabileceğini belirtir. Cloud aksiyonları Bölüm 8 içinde, engine prefixleriyle ayrı tutulmuştur.


## 11.2.postgresql — PostgreSQL hızlı aksiyon dizini

| Kimlik | İşlem | Öncelik / risk |
|---|---|---|
| [PG-PROV-001](#pg-prov-001) | DB/rol çakışması ve oluşturma yetkisi keşfi | P0 / R0 |
| [PG-PROV-002](#pg-prov-002) | Owner, izin rolleri ve bağlantı kimlikleri oluşturma | P0 / R1 |
| [PG-PROV-003](#pg-prov-003) | Doğru owner ile yeni database oluşturma | P0 / R1 |
| [PG-PROV-004](#pg-prov-004) | Yeni DB erişimini daraltma ve uygulama schema oluşturma | P0 / R1 |
| [PG-PROV-005](#pg-prov-005) | Mevcut nesneler için RW ve readonly izinleri | P0 / R1 |
| [PG-PROV-006](#pg-prov-006) | Gelecek nesnelerin owner bazlı default privileges ayarı | P0 / R1 |
| [PG-PROV-007](#pg-prov-007) | Migration oturumunu doğru nesne sahibiyle yürütme | P0 / R1 |
| [PG-PROV-008](#pg-prov-008) | İsteğe bağlı rol/DB oturum politikaları | P1 / R1 |
| [PG-PROV-009](#pg-prov-009) | Oluşturma sonrası owner ve effective izin doğrulaması | P0 / R0 |
| [PG-PROV-010](#pg-prov-010) | Ayrı monitoring hesabı oluşturma | P1 / R1 |
| [PG-MON-001](#pg-mon-001) | Bağlantı kimliği, sürüm ve primary/replica durumu | P0 / R0 |
| [PG-MON-002](#pg-mon-002) | Database listesi ve boyutları | P0 / R0 |
| [PG-MON-003](#pg-mon-003) | Temel çalışma ayarları ve restart bekleyen ayarlar | P1 / R0 |
| [PG-MON-004](#pg-mon-004) | Bağlantı doluluğu ve durum dağılımı | P0 / R0 |
| [PG-MON-005](#pg-mon-005) | Aktif ve uzun süren sorgular | P0 / R0 |
| [PG-MON-006](#pg-mon-006) | Idle in transaction ve eski transactionlar | P0 / R0 |
| [PG-MON-007](#pg-mon-007) | Blocking zinciri ve bekleyen oturumlar | P0 / R0 |
| [PG-MON-008](#pg-mon-008) | Seçili oturumun lock ayrıntıları | P1 / R0 |
| [PG-MON-009](#pg-mon-009) | Primary üzerindeki replication durumu ve byte gecikmeleri | P0 / R0 |
| [PG-MON-010](#pg-mon-010) | Replica üzerindeki receive/replay ilerlemesi | P0 / R0 |
| [PG-MON-011](#pg-mon-011) | WAL receiver sürüm bağımsız alan çıkarımı | P1 / R0 |
| [PG-MON-012](#pg-mon-012) | Replication slotları ve WAL tutulması | P0 / R0 |
| [PG-MON-013](#pg-mon-013) | WAL üretim sayaçları | P0 / R0 |
| [PG-MON-014](#pg-mon-014) | Archive sağlığı | P0 / R0 |
| [PG-MON-015](#pg-mon-015) | pg_wal dosya alanını ölçme | P1 / R0 |
| [PG-MON-016](#pg-mon-016) | Prepared transactionlar ve eski 2PC işlemler | P0 / R0 |
| [PG-MON-017](#pg-mon-017) | Transaction ID wraparound riski: DB seviyesinde | P0 / R0 |
| [PG-MON-018](#pg-mon-018) | Eski frozen XID sahibi tablolar | P0 / R0 |
| [PG-MON-019](#pg-mon-019) | Autovacuum/analyze ve dead tuple tahminleri | P0 / R0 |
| [PG-MON-020](#pg-mon-020) | Vacuum ve index oluşturma ilerlemesi | P0 / R0 |
| [PG-MON-021](#pg-mon-021) | Tablo, index, TOAST ve yardımcı alan boyutları | P0 / R0 |
| [PG-MON-022](#pg-mon-022) | Partition ağacı ve child boyutları | P1 / R0 |
| [PG-MON-023](#pg-mon-023) | Index envanteri, kullanım ve constraint bağımlılıkları | P0 / R0 |
| [PG-MON-024](#pg-mon-024) | Geçersiz veya hazır olmayan indexler | P0 / R0 |
| [PG-MON-025](#pg-mon-025) | İstatistik tabanlı büyük sequential scan adayları | P1 / R0 |
| [PG-MON-026](#pg-mon-026) | pg_stat_statements en maliyetli sorgular | P0 / R0 |
| [PG-MON-027](#pg-mon-027) | Database sayaçları: cache, temp, deadlock ve transaction | P0 / R0 |
| [PG-MON-028](#pg-mon-028) | IO sayaçları: PostgreSQL 16+ sürüm uyumlu projection | P1 / R0 |
| [PG-MON-029](#pg-mon-029) | Checkpointer ve bgwriter sürüm uyumlu sayaçlar | P1 / R0 |
| [PG-MON-030](#pg-mon-030) | Checkpointer sayaçları: PostgreSQL 17+ | P1 / R0 |
| [PG-MON-031](#pg-mon-031) | Roller ve ayrıcalıklı hesaplar | P0 / R0 |
| [PG-MON-032](#pg-mon-032) | Role üyelik grafiği | P0 / R0 |
| [PG-MON-033](#pg-mon-033) | Schema ve nesne bazlı effective izin matrisi | P0 / R0 |
| [PG-MON-034](#pg-mon-034) | Default privileges görünümü | P0 / R0 |
| [PG-MON-035](#pg-mon-035) | RLS ve SECURITY DEFINER incelemesi | P1 / R0 |
| [PG-MON-036](#pg-mon-036) | TLS oturumları ve şifrelenmeyen bağlantılar | P1 / R0 |
| [PG-MON-037](#pg-mon-037) | Publication listesi ve izlenen tablolar | P0 / R0 |
| [PG-MON-038](#pg-mon-038) | Logical subscription worker durumu | P1 / R0 |
| [PG-MON-039](#pg-mon-039) | Prepared olmayan büyük/uzun COPY ve analyze ilerlemesi | P1 / R0 |
| [PG-MON-040](#pg-mon-040) | Extension ve capability keşfi | P0 / R0 |
| [PG-MON-041](#pg-mon-041) | Seçili tablonun kolonları ve default/identity bilgisi | P0 / R0 |
| [PG-MON-042](#pg-mon-042) | Constraint ve foreign key bağımlılıkları | P0 / R0 |
| [PG-MON-043](#pg-mon-043) | Kontrollü veri önizleme | P0 / R0 |
| [PG-MON-044](#pg-mon-044) | Seçili tabloda pgstattuple yaklaşık fiziksel ölçüm | P2 / R2 |
| [PG-MON-045](#pg-mon-045) | Seçili tabloda tam pgstattuple ölçümü | P2 / R2 |
| [PG-MON-046](#pg-mon-046) | Standby query conflict sayaçları | P1 / R0 |
| [PG-MON-047](#pg-mon-047) | Tablo autovacuum override ve persistence kontrolü | P1 / R0 |
| [PG-MON-048](#pg-mon-048) | Index dışındaki nesne sahiplik sapmaları | P1 / R0 |
| [PG-OPS-001](#pg-ops-001) | Seçili sorguyu iptal etme | P0 / R2 |
| [PG-OPS-002](#pg-ops-002) | Seçili oturumu sonlandırma | P0 / R2 |
| [PG-OPS-003](#pg-ops-003) | Tek tabloda VACUUM ANALYZE | P1 / R2 |
| [PG-OPS-004](#pg-ops-004) | Tek tabloda ANALYZE | P1 / R2 |
| [PG-OPS-005](#pg-ops-005) | Concurrent index oluşturma | P1 / R2 |
| [PG-OPS-006](#pg-ops-006) | Concurrent index yeniden oluşturma | P1 / R2 |
| [PG-OPS-007](#pg-ops-007) | VACUUM FULL: uzman bakım | P2 / R3 |
| [PG-OPS-008](#pg-ops-008) | Replication slotunu silme | P2 / R3 |
| [PG-OPS-009](#pg-ops-009) | Seçili tablolardan publication oluşturma | P1 / R1 |
| [PG-OPS-010](#pg-ops-010) | Yalnız onaylanan routine için EXECUTE verme | P1 / R1 |
| [PG-OPS-011](#pg-ops-011) | Bağlantı hesabını devre dışı bırakma | P1 / R1 |
| [PG-OPS-012](#pg-ops-012) | Parola rotasyonu | P1 / R1 |
| [PG-OPS-013](#pg-ops-013) | Readonly rolünden üyelik kaldırma | P1 / R1 |
| [PG-OPS-014](#pg-ops-014) | Tek schema/table için owner düzeltmesi | P2 / R2 |
| [PG-OPS-015](#pg-ops-015) | Tehlikeli DROP DATABASE planı | P2 / R3 |
| [PG-CLOUD-001](#pg-cloud-001) | RDS PostgreSQL: IAM login role ve dar data rolü | P1 / R1 |
| [PG-CLOUD-002](#pg-cloud-002) | Azure PostgreSQL: Entra principal envanteri | P1 / R0 |
| [PG-CLOUD-003](#pg-cloud-003) | Azure PostgreSQL: Object ID ile Entra hesabı eşleme | P1 / R1 |


## 11.2.mssql — Microsoft SQL Server hızlı aksiyon dizini

| Kimlik | İşlem | Öncelik / risk |
|---|---|---|
| [MS-PROV-001](#ms-prov-001) | Sürüm, platform, DB ve principal çakışma keşfi | P0 / R0 |
| [MS-PROV-002](#ms-prov-002) | Self-managed database ve fiziksel dosya planı | P0 / R1 |
| [MS-PROV-003](#ms-prov-003) | Dedicated DB owner principal oluşturma ve atama | P0 / R1 |
| [MS-PROV-004](#ms-prov-004) | Uygulama, readonly ve migration loginleri | P0 / R1 |
| [MS-PROV-005](#ms-prov-005) | DB user, schema owner ve özel izin rolleri | P0 / R1 |
| [MS-PROV-006](#ms-prov-006) | İzole owner ile uygulama schema oluşturma | P0 / R1 |
| [MS-PROV-007](#ms-prov-007) | Schema kapsamlı RW/readonly izinleri ve üyelikler | P0 / R1 |
| [MS-PROV-008](#ms-prov-008) | Ayrı migration yetki rolü | P1 / R1 |
| [MS-PROV-009](#ms-prov-009) | Greenfield DB güvenlik ve güvenilirlik ayarları | P1 / R1 |
| [MS-PROV-010](#ms-prov-010) | Azure SQL Database contained kullanıcı varyantı | P0 / R1 |
| [MS-PROV-011](#ms-prov-011) | Monitoring permission planı: server ve DB ayrı | P1 / R1 |
| [MS-PROV-012](#ms-prov-012) | Target kimlikle effective izin testi | P0 / R0 |
| [MS-MON-001](#ms-mon-001) | Sunucu ve DB kimliği | P0 / R0 |
| [MS-MON-002](#ms-mon-002) | Database durumu, recovery modeli ve log reuse nedeni | P0 / R0 |
| [MS-MON-003](#ms-mon-003) | Oturum sayısı ve uygulama dağılımı | P0 / R0 |
| [MS-MON-004](#ms-mon-004) | Aktif istekler ve statement metni | P0 / R0 |
| [MS-MON-005](#ms-mon-005) | Blocking ve blocker kimliği | P0 / R0 |
| [MS-MON-006](#ms-mon-006) | Eski açık transactionlar ve kullanılan log | P0 / R0 |
| [MS-MON-007](#ms-mon-007) | Bekleme sayaçları | P0 / R0 |
| [MS-MON-008](#ms-mon-008) | Şu an bekleyen görevler | P1 / R0 |
| [MS-MON-009](#ms-mon-009) | Transaction log doluluğu | P0 / R0 |
| [MS-MON-010](#ms-mon-010) | Data/log file kapasitesi ve autogrowth | P0 / R0 |
| [MS-MON-011](#ms-mon-011) | DB file IO gecikme sayaçları | P0 / R0 |
| [MS-MON-012](#ms-mon-012) | Self-managed disk/volume boşluğu | P1 / R0 |
| [MS-MON-013](#ms-mon-013) | Tempdb alan dağılımı | P0 / R0 |
| [MS-MON-014](#ms-mon-014) | Tempdb kullanan sessionlar | P1 / R0 |
| [MS-MON-015](#ms-mon-015) | Memory grant bekleyen sorgular | P0 / R0 |
| [MS-MON-016](#ms-mon-016) | SQL process memory ve host memory | P1 / R0 |
| [MS-MON-017](#ms-mon-017) | Scheduler ve CPU baskısı adayları | P1 / R0 |
| [MS-MON-018](#ms-mon-018) | En pahalı cached queryler | P0 / R0 |
| [MS-MON-019](#ms-mon-019) | Query Store etkinlik ve kapasite durumu | P0 / R0 |
| [MS-MON-020](#ms-mon-020) | Query Store zaman aralığında en maliyetli sorgular | P1 / R0 |
| [MS-MON-021](#ms-mon-021) | Tablo boyutları ve tahmini satır sayısı | P0 / R0 |
| [MS-MON-022](#ms-mon-022) | Index envanteri ve kullanımı | P0 / R0 |
| [MS-MON-023](#ms-mon-023) | Tek index/tablo fragmentation incelemesi | P2 / R2 |
| [MS-MON-024](#ms-mon-024) | Missing-index öneri adayları | P1 / R0 |
| [MS-MON-025](#ms-mon-025) | İstatistik güncelliği ve modification counter | P1 / R0 |
| [MS-MON-026](#ms-mon-026) | Backup geçmişi ve son full/diff/log | P0 / R0 |
| [MS-MON-027](#ms-mon-027) | Agent job son durumları | P0 / R0 |
| [MS-MON-028](#ms-mon-028) | Always On replica ve queue durumu | P0 / R0 |
| [MS-MON-029](#ms-mon-029) | Login, user ve role üyelikleri | P0 / R0 |
| [MS-MON-030](#ms-mon-030) | Explicit DB permission matrisi | P0 / R0 |
| [MS-MON-031](#ms-mon-031) | Login eşleşmesi eksik SQL user adayları | P1 / R0 |
| [MS-MON-032](#ms-mon-032) | Şifreli bağlantı kontrolü | P1 / R0 |
| [MS-MON-033](#ms-mon-033) | Kolonlar, identity, computed ve defaultlar | P0 / R0 |
| [MS-MON-034](#ms-mon-034) | PK/unique/FK metadata | P0 / R0 |
| [MS-MON-035](#ms-mon-035) | Kontrollü veri önizleme | P0 / R0 |
| [MS-MON-036](#ms-mon-036) | Azure SQL Database kaynak kullanım aralığı | P0 / R0 |
| [MS-MON-037](#ms-mon-037) | Aktif bakım/backup/restore ilerlemesi | P0 / R0 |
| [MS-MON-038](#ms-mon-038) | Disabled indexler ve untrusted constraintler | P1 / R0 |
| [MS-MON-039](#ms-mon-039) | VLF ve log truncation ayrıntısı | P1 / R0 |
| [MS-MON-040](#ms-mon-040) | Sorgu deadlock olayları: event session keşfi | P1 / R0 |
| [MS-OPS-001](#ms-ops-001) | Oturumu sonlandırma | P0 / R2 |
| [MS-OPS-002](#ms-ops-002) | Rollback ilerleme kontrolü | P1 / R0 |
| [MS-OPS-003](#ms-ops-003) | Tek istatistiği güncelleme | P1 / R2 |
| [MS-OPS-004](#ms-ops-004) | Tek index reorganize | P1 / R2 |
| [MS-OPS-005](#ms-ops-005) | Tek index offline rebuild | P2 / R2 |
| [MS-OPS-006](#ms-ops-006) | DBCC CHECKDB fiziksel doğrulama | P1 / R2 |
| [MS-OPS-007](#ms-ops-007) | DBCC CHECKDB tam doğrulama | P2 / R2 |
| [MS-OPS-008](#ms-ops-008) | COPY_ONLY checksum full backup | P1 / R2 |
| [MS-OPS-009](#ms-ops-009) | Login devre dışı bırakma | P1 / R1 |
| [MS-OPS-010](#ms-ops-010) | Login parola rotasyonu | P1 / R1 |
| [MS-OPS-011](#ms-ops-011) | Sadece onaylı stored procedure için EXECUTE | P1 / R1 |
| [MS-OPS-012](#ms-ops-012) | Yetki rolü üyeliğini kaldırma | P1 / R1 |
| [MS-OPS-013](#ms-ops-013) | Kontrollü DROP DATABASE | P2 / R3 |
| [MS-CLOUD-001](#ms-cloud-001) | Azure SQL: Entra kullanıcı/grup ve dar rol üyeliği | P1 / R1 |
| [MS-CLOUD-002](#ms-cloud-002) | Azure SQL: Geo-replication durumu | P0 / R0 |
| [MS-CLOUD-003](#ms-cloud-003) | RDS SQL Server: Native task durumu | P0 / R0 |
| [MS-CLOUD-004](#ms-cloud-004) | RDS SQL Server: Yeni S3 nesnesine native FULL backup | P1 / R2 |


## 11.2.oracle — Oracle hızlı aksiyon dizini

| Kimlik | İşlem | Öncelik / risk |
|---|---|---|
| [OR-PROV-001](#or-prov-001) | Container, session ve sürüm keşfi | P0 / R0 |
| [OR-PROV-002](#or-prov-002) | Opsiyonel yeni PDB — yalnızca self-managed OMF | P2 / R2 |
| [OR-PROV-003](#or-prov-003) | Opsiyonel uygulama tablespace — OMF | P1 / R2 |
| [OR-PROV-004](#or-prov-004) | NO AUTHENTICATION schema owner ve sınırlı kota | P0 / R1 |
| [OR-PROV-005](#or-prov-005) | Kontrollü migration proxy hesabı | P0 / R1 |
| [OR-PROV-006](#or-prov-006) | Runtime ve readonly kimlikleri / permission rolleri | P0 / R1 |
| [OR-PROV-007](#or-prov-007) | Onaylı tabloda RW ve gerçekten dar RO grantleri | P0 / R1 |
| [OR-PROV-008](#or-prov-008) | Gerekli sequence / package erişimi | P1 / R1 |
| [OR-PROV-009](#or-prov-009) | Migration sonrası nesne grant reconciliation girdisi | P0 / R0 |
| [OR-PROV-010](#or-prov-010) | En az ayrıcalıklı monitoring hesabı — başlangıç profili | P0 / R1 |
| [OR-PROV-011](#or-prov-011) | Gerçek app/reader/migrator bağlantısıyla yetki doğrulama | P0 / R0 |
| [OR-MON-001](#or-mon-001) | Instance ve database rol / açık durumu | P0 / R0 |
| [OR-MON-002](#or-mon-002) | PDB listesi ve açık durumu | P0 / R0 |
| [OR-MON-003](#or-mon-003) | Sessions / processes kaynak limitleri | P0 / R0 |
| [OR-MON-004](#or-mon-004) | Aktif kullanıcı oturumları | P0 / R0 |
| [OR-MON-005](#or-mon-005) | Blocking ve final blocker | P0 / R0 |
| [OR-MON-006](#or-mon-006) | Uzun çalışan SQL adayları | P0 / R0 |
| [OR-MON-007](#or-mon-007) | Açık transaction ve UNDO tüketimi | P0 / R0 |
| [OR-MON-008](#or-mon-008) | Enqueue lock detayları | P1 / R0 |
| [OR-MON-009](#or-mon-009) | Long operations ilerlemesi | P0 / R0 |
| [OR-MON-010](#or-mon-010) | Wait event kümülatif sayaçları | P1 / R0 |
| [OR-MON-011](#or-mon-011) | Redo, commit, logical/physical IO sayaçları | P0 / R0 |
| [OR-MON-012](#or-mon-012) | Kısa dönem sistem metrikleri | P1 / R0 |
| [OR-MON-013](#or-mon-013) | SQL cache toplam elapsed / CPU / buffer gets | P1 / R0 |
| [OR-MON-014](#or-mon-014) | Seçili SQL child cursor detayları | P1 / R0 |
| [OR-MON-015](#or-mon-015) | Seçili local cached cursor planı | P1 / R0 |
| [OR-MON-016](#or-mon-016) | Tablespace kapasitesi ve gerçek kullanılan bloklar | P0 / R0 |
| [OR-MON-017](#or-mon-017) | Datafile allocation / autoextend / maxsize | P0 / R0 |
| [OR-MON-018](#or-mon-018) | TEMP alanının kullanılabilir kapasitesi | P0 / R0 |
| [OR-MON-019](#or-mon-019) | TEMP kullanan oturumlar | P0 / R0 |
| [OR-MON-020](#or-mon-020) | UNDO geçmişi / snapshot-too-old / alan hataları | P0 / R0 |
| [OR-MON-021](#or-mon-021) | FRA limiti / kullanılan / geri kazanılabilir alan | P0 / R0 |
| [OR-MON-022](#or-mon-022) | Archive destination hataları | P0 / R0 |
| [OR-MON-023](#or-mon-023) | Data Guard lag ve veri tazeliği | P0 / R0 |
| [OR-MON-024](#or-mon-024) | Standby archive gap ve apply süreçleri | P0 / R0 |
| [OR-MON-025](#or-mon-025) | RMAN backup job geçmişi | P0 / R0 |
| [OR-MON-026](#or-mon-026) | Kaydedilmiş block corruption | P0 / R0 |
| [OR-MON-027](#or-mon-027) | Yakın tarihli alert kayıtları | P0 / R0 |
| [OR-MON-028](#or-mon-028) | Invalid nesneler ve derleme hataları | P1 / R0 |
| [OR-MON-029](#or-mon-029) | Unusable index ve partitionlar | P0 / R0 |
| [OR-MON-030](#or-mon-030) | Tablo istatistikleri güncelliği | P1 / R0 |
| [OR-MON-031](#or-mon-031) | Schema segment boyutları | P1 / R0 |
| [OR-MON-032](#or-mon-032) | Scheduler başarısızlıkları / planlı işler | P1 / R0 |
| [OR-MON-033](#or-mon-033) | Kullanıcı, expiry ve authentication tipi | P0 / R0 |
| [OR-MON-034](#or-mon-034) | Role ve system privilege grafiği | P0 / R0 |
| [OR-MON-035](#or-mon-035) | Nesne ve sütun grantleri | P0 / R0 |
| [OR-MON-036](#or-mon-036) | Tablespace kotaları | P1 / R0 |
| [OR-MON-037](#or-mon-037) | Explorer: sütun / default / nullability | P0 / R0 |
| [OR-MON-038](#or-mon-038) | Explorer: constraint / PK / FK sütunları | P0 / R0 |
| [OR-MON-039](#or-mon-039) | Data Browser: sınırlı ilk sayfa | P0 / R0 |
| [OR-MON-040](#or-mon-040) | Service ve parametre snapshot | P1 / R0 |
| [OR-MON-041](#or-mon-041) | SGA / PGA snapshot | P1 / R0 |
| [OR-MON-042](#or-mon-042) | Redo log grupları ve üye sağlığı | P0 / R0 |
| [OR-MON-043](#or-mon-043) | Dağıtık in-doubt transaction | P0 / R0 |
| [OR-MON-044](#or-mon-044) | Proxy erişim envanteri | P0 / R0 |
| [OR-MON-045](#or-mon-045) | Lisans kapılı ASH özeti — varsayılan kapalı | P2 / R0 |
| [OR-MON-046](#or-mon-046) | Lisans kapılı Real-Time SQL Monitoring — varsayılan kapalı | P2 / R0 |
| [OR-OPS-001](#or-ops-001) | Seçili SQL yürütmesini iptal et | P0 / R2 |
| [OR-OPS-002](#or-ops-002) | Kontrollü session kill | P0 / R2 |
| [OR-OPS-003](#or-ops-003) | Tek tablo için kontrollü optimizer statistics | P1 / R2 |
| [OR-OPS-004](#or-ops-004) | Onaylı nesneyi yeniden derleme | P1 / R2 |
| [OR-OPS-005](#or-ops-005) | Tek nonpartitioned index rebuild — offline | P2 / R2 |
| [OR-OPS-006](#or-ops-006) | Tablespace kotasını açık sınırla değiştirme | P1 / R1 |
| [OR-OPS-007](#or-ops-007) | OMF ile tek tablespacee sınırlı datafile ekle | P1 / R2 |
| [OR-OPS-008](#or-ops-008) | Hesabı yeni girişlere kilitleme | P1 / R1 |
| [OR-OPS-009](#or-ops-009) | Parola döndürme | P1 / R1 |
| [OR-OPS-010](#or-ops-010) | Migration proxy yetkisini kaldırma | P1 / R1 |
| [OR-OPS-011](#or-ops-011) | Nesne yetkisini kontrollü revoke | P1 / R1 |
| [OR-OPS-012](#or-ops-012) | DROP USER CASCADE — uygulama schema verisini siler | P2 / R3 |
| [OR-CLOUD-001](#or-cloud-001) | RDS Oracle: Provider prosedürüyle SQL cancel | P0 / R2 |
| [OR-CLOUD-002](#or-cloud-002) | RDS Oracle: Seçili SYS view için monitor grant | P1 / R1 |


## 11.3 Kaynaklar ve doğrulama durumu

Referanslar ürünlerin resmî teknik dokümantasyonudur. Çevrimiçi sayfalar değişebileceğinden runtime desteği bu bağlantının varlığına göre değil, ilgili gerçek engine/edition/provider sürümünde capability ve integration testleriyle belirlenir. Kaynak tarihi bağlamı: **3 Ekim 2026**. Bir PostgreSQL 18 sayfasına referans verilmesi, bütün alanların PostgreSQL 14te de mevcut olduğunun kanıtı değildir; katalogdaki sürüm varyantı kuralları uygulanır.

Bu belge için yapılan kontroller: aksiyon kimliği benzersizliği, Markdown kod çitleri, YAML metadata okunabilirliği, kaynak ve iç bağlantı bütünlüğü, risk/polling tutarlılığı ve şablon token türleri. **SQLler canlı motorlara gönderilmedi; engine parserı veya gerçek execution plan ile test edilmedi.** Kod üretimi sırasında Bölüm 10daki gerçek motor entegrasyon ve yetki testleri zorunludur.

SQLlerin önemli bölümü kamuya açık sistem view/function/catalog bilgilerinden ve bu ürünün ihtiyaçlarına yönelik özgün bileşimlerden oluşur. Günlük kullanımda ortam politikası, sağlayıcı sınırları ve sürüm dokümanı her zaman birlikte dikkate alınır.


<a id="kaynak-p01"></a>
**[P01] PostgreSQL 18 — CREATE ROLE**  
https://www.postgresql.org/docs/18/sql-createrole.html


<a id="kaynak-p02"></a>
**[P02] PostgreSQL 18 — CREATE DATABASE**  
https://www.postgresql.org/docs/18/sql-createdatabase.html


<a id="kaynak-p03"></a>
**[P03] PostgreSQL 18 — ALTER DEFAULT PRIVILEGES**  
https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html


<a id="kaynak-p04"></a>
**[P04] PostgreSQL 18 — GRANT**  
https://www.postgresql.org/docs/18/sql-grant.html


<a id="kaynak-p05"></a>
**[P05] PostgreSQL 18 — Cumulative Statistics**  
https://www.postgresql.org/docs/18/monitoring-stats.html


<a id="kaynak-p06"></a>
**[P06] PostgreSQL 18 — pg_replication_slots**  
https://www.postgresql.org/docs/18/view-pg-replication-slots.html


<a id="kaynak-p07"></a>
**[P07] PostgreSQL 18 — System Administration Functions**  
https://www.postgresql.org/docs/18/functions-admin.html


<a id="kaynak-p08"></a>
**[P08] PostgreSQL 18 — Predefined Roles**  
https://www.postgresql.org/docs/18/predefined-roles.html


<a id="kaynak-p09"></a>
**[P09] PostgreSQL 18 — Routine Vacuuming**  
https://www.postgresql.org/docs/18/routine-vacuuming.html


<a id="kaynak-p10"></a>
**[P10] PostgreSQL 18 — pg_stat_statements**  
https://www.postgresql.org/docs/18/pgstatstatements.html


<a id="kaynak-p11"></a>
**[P11] PostgreSQL 18 — pgstattuple**  
https://www.postgresql.org/docs/18/pgstattuple.html


<a id="kaynak-p12"></a>
**[P12] PostgreSQL 18 — Privileges**  
https://www.postgresql.org/docs/18/ddl-priv.html


<a id="kaynak-m01"></a>
**[M01] SQL Server — CREATE DATABASE**  
https://learn.microsoft.com/en-us/sql/t-sql/statements/create-database-transact-sql?view=sql-server-ver17


<a id="kaynak-m02"></a>
**[M02] SQL Server — Create a database user**  
https://learn.microsoft.com/en-us/sql/relational-databases/security/authentication-access/create-a-database-user?view=sql-server-ver17


<a id="kaynak-m03"></a>
**[M03] SQL Server — Database-level roles**  
https://learn.microsoft.com/en-us/sql/relational-databases/security/authentication-access/database-level-roles?view=sql-server-ver17


<a id="kaynak-m04"></a>
**[M04] SQL Server — GRANT schema permissions**  
https://learn.microsoft.com/en-us/sql/t-sql/statements/grant-schema-permissions-transact-sql?view=sql-server-ver17


<a id="kaynak-m05"></a>
**[M05] SQL Server — ALTER AUTHORIZATION**  
https://learn.microsoft.com/en-us/sql/t-sql/statements/alter-authorization-transact-sql?view=sql-server-ver17


<a id="kaynak-m06"></a>
**[M06] SQL Server — sys.dm_exec_requests**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-exec-requests-transact-sql?view=sql-server-ver17


<a id="kaynak-m07"></a>
**[M07] SQL Server — sys.dm_db_log_space_usage**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-db-log-space-usage-transact-sql?view=sql-server-ver17


<a id="kaynak-m08"></a>
**[M08] SQL Server — sys.dm_db_index_physical_stats**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-db-index-physical-stats-transact-sql?view=sql-server-ver17


<a id="kaynak-m09"></a>
**[M09] SQL Server — Missing index suggestions**  
https://learn.microsoft.com/en-us/sql/relational-databases/indexes/tune-nonclustered-missing-index-suggestions?view=sql-server-ver17


<a id="kaynak-m10"></a>
**[M10] SQL Server — Query Store**  
https://learn.microsoft.com/en-us/sql/relational-databases/performance/monitoring-performance-by-using-the-query-store?view=sql-server-ver17


<a id="kaynak-m11"></a>
**[M11] SQL Server — DBCC CHECKDB**  
https://learn.microsoft.com/en-us/sql/t-sql/database-console-commands/dbcc-checkdb-transact-sql?view=sql-server-ver17


<a id="kaynak-o01"></a>
**[O01] Oracle Database 19c — CREATE USER**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-USER.html


<a id="kaynak-o02"></a>
**[O02] Oracle Database — Schema-only and proxy authentication**  
https://docs.oracle.com/en/database/oracle/oracle-database/21/dbseg/configuring-authentication.html


<a id="kaynak-o03"></a>
**[O03] Oracle Database 19c — GRANT**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/GRANT.html


<a id="kaynak-o04"></a>
**[O04] Oracle Database 19c — CREATE PLUGGABLE DATABASE**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-PLUGGABLE-DATABASE.html


<a id="kaynak-o05"></a>
**[O05] Oracle Database 19c — Database Reference**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/


<a id="kaynak-o06"></a>
**[O06] Oracle Database 19c — V$SESSION**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-SESSION.html


<a id="kaynak-o07"></a>
**[O07] Oracle Database 19c — V$SQLSTATS**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-SQLSTATS.html


<a id="kaynak-o08"></a>
**[O08] Oracle Database 19c — DBA_TABLESPACE_USAGE_METRICS**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/DBA_TABLESPACE_USAGE_METRICS.html


<a id="kaynak-o09"></a>
**[O09] Oracle Database 19c — V$DATAGUARD_STATS**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-DATAGUARD_STATS.html


<a id="kaynak-o10"></a>
**[O10] Oracle Database 19c — Licensing Information**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/dblic/Licensing-Information.html


<a id="kaynak-o11"></a>
**[O11] Oracle AI Database 26ai — GRANT**  
https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/GRANT.html


<a id="kaynak-c01"></a>
**[C01] AWS RDS — IAM database authentication**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.IAMDBAuth.html


<a id="kaynak-c02"></a>
**[C02] AWS RDS — Master account privileges**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/UsingWithRDS.MasterAccounts.html


<a id="kaynak-c03"></a>
**[C03] AWS RDS — DescribeDBInstances**  
https://docs.aws.amazon.com/AmazonRDS/latest/APIReference/API_DescribeDBInstances.html


<a id="kaynak-c04"></a>
**[C04] AWS RDS PostgreSQL — rds_superuser**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Appendix.PostgreSQL.CommonDBATasks.Roles.rds_superuser.html


<a id="kaynak-c05"></a>
**[C05] Azure PostgreSQL — Microsoft Entra authentication**  
https://learn.microsoft.com/en-us/azure/postgresql/security/security-entra-concepts


<a id="kaynak-c06"></a>
**[C06] Azure SQL — Configure Microsoft Entra authentication**  
https://learn.microsoft.com/en-us/azure/azure-sql/database/authentication-aad-configure?view=azuresql


<a id="kaynak-c07"></a>
**[C07] Azure SQL Database / Managed Instance comparison**  
https://learn.microsoft.com/en-us/azure/azure-sql/database/features-comparison?view=azuresql


<a id="kaynak-m12"></a>
**[M12] SQL Server — System dynamic management objects**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/system-dynamic-management-objects?view=sql-server-ver17


<a id="kaynak-m13"></a>
**[M13] SQL Server — backupset**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-tables/backupset-transact-sql?view=sql-server-ver17


<a id="kaynak-m14"></a>
**[M14] SQL Server — HADR database replica states**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-hadr-database-replica-states-transact-sql?view=sql-server-ver17


<a id="kaynak-m15"></a>
**[M15] SQL Server — Security catalog views**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/security-catalog-views-transact-sql?view=sql-server-ver17


<a id="kaynak-m16"></a>
**[M16] SQL Server — KILL**  
https://learn.microsoft.com/en-us/sql/t-sql/language-elements/kill-transact-sql?view=sql-server-ver17


<a id="kaynak-m17"></a>
**[M17] SQL Server — Statistics properties**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-db-stats-properties-transact-sql?view=sql-server-ver17


<a id="kaynak-o12"></a>
**[O12] Oracle 19c — DBA_TEMP_FREE_SPACE**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/DBA_TEMP_FREE_SPACE.html


<a id="kaynak-o13"></a>
**[O13] Oracle 19c — V$SYSMETRIC**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-SYSMETRIC.html


<a id="kaynak-o14"></a>
**[O14] Oracle 19c — ALTER SYSTEM**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/ALTER-SYSTEM.html


<a id="kaynak-o15"></a>
**[O15] Oracle 19c — DBMS_STATS**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_STATS.html


<a id="kaynak-o16"></a>
**[O16] Oracle 19c — V$TEMPSEG_USAGE**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-TEMPSEG_USAGE.html


<a id="kaynak-o17"></a>
**[O17] Oracle 19c — V$DATAGUARD_PROCESS**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-DATAGUARD_PROCESS.html


<a id="kaynak-o18"></a>
**[O18] Oracle 19c — CREATE TABLESPACE**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/sqlrf/CREATE-TABLESPACE.html


<a id="kaynak-o19"></a>
**[O19] Oracle 19c — DBMS_XPLAN**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_XPLAN.html


<a id="kaynak-o20"></a>
**[O20] Oracle 19c — V$DIAG_ALERT_EXT**  
https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-DIAG_ALERT_EXT.html


<a id="kaynak-c08"></a>
**[C08] Azure PostgreSQL — Entra database roles**  
https://learn.microsoft.com/en-us/azure/postgresql/security/security-manage-entra-users


<a id="kaynak-c09"></a>
**[C09] AWS RDS Oracle — Cancel SQL**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Appendix.Oracle.CommonDBATasks.CancellingSQL.html


<a id="kaynak-c10"></a>
**[C10] AWS RDS Oracle — SYS object grants**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Appendix.Oracle.CommonDBATasks.TransferPrivileges.html


<a id="kaynak-c11"></a>
**[C11] AWS RDS SQL Server — Native backup / task status**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/SQLServer.Procedural.Importing.Native.Using.html


<a id="kaynak-c12"></a>
**[C12] Azure SQL — Geo replication link status**  
https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-geo-replication-link-status-azure-sql-database?view=azuresqldb-current


<a id="kaynak-c13"></a>
**[C13] AWS RDS Oracle — Managed privileges**  
https://docs.aws.amazon.com/AmazonRDS/latest/UserGuide/Oracle.Concepts.Privileges.html

