# TermTerm Database Workspace — 0.5.0-dev.12

Dal: `codex/feature-database-workspace`. Ayrı uygulama kimliği: `local.termterm.desktop.databasedev`. Kararlı sürüm, main, GitHub Release ve güncelleme kaynağı değiştirilmedi. macOS/CI çalıştırılmadı.

## SQL kitaplığı — dev.12

Kişisel ve Team kasalarında adlandırılmış sorgu, açıklama, klasör, sıralı runbook adımları ve tek adımı editöre yükleme eklendi. Team okuma/düzenleme/çalıştırma/export yetkileri ayrı tutulur. Bağlantı ilişkileri, seçili yedek ve kopyalama eşlemeleri korunur; canlı belge açma ve güncel revizyon kontrolü kullanılır. Runbook otomatik yürütücüsü ve zorunlu ikinci onay yoktur. [Kullanım ve sınırlar](DATABASE_TEAM.md).

## Önceki Team SQL aşaması — dev.11

Ortak kasada PostgreSQL, SQL Server ve Oracle doğrudan bağlantıları; ayrı SQL çalıştır/izle/yönet/export izinleri, Rust IPC + PostgreSQL doğrulaması, revizyona bağlı oturum ve izin iptalinde kapanış eklendi. Klasör izinleri SQL kaynaklarına uygulanır; SSH `connect` SQL parolasını açmaz. Paylaşılan public CA PEM desteklenir. [Kurulum, matris ve sınırlar](DATABASE_TEAM.md).

SQL her işlemde çevrimiçi yetki kontrolü gerektirir. Team SSH tüneli, çevrimdışı SQL, Team metrik/şema geçmişi ve şirketin ikinci onay servisi bu aşamada yoktur. Serbest SQL hedef DB hesabının yetkileriyle çalışır; `sql_manage` ayrı bir DDL güvenlik duvarı değildir.

## Bu geliştirmede kullanılabilir olanlar

- **dev.8 DB/hesap şablonu:** PostgreSQL ve SQL Server yeni DB; Oracle mevcut PDB'de schema veya aynı CDB içinde yeni PDB. Altı rol/hesap, bağımsız 21 karakterli tek seferlik parolalar, SQL önizleme ve tek kullanımlık uygulama. Gerçek app CRUD ve readonly erişim testleri tamamlandı. PostgreSQL superuser şartı yerine CREATEDB/CREATEROLE; Oracle yeni PDB'de SYSDBA bağlantısı. [Şablon, PaaS ve sınırlar](DATABASE_PROVISIONING.md).
- **dev.8 PaaS/extension:** sağlayıcı rolleri/izin listeleri/kurulu sürümler okunur. Mevcut yöneticiyle extension önizleme/transaction/son kontrol; otomatik owner yükseltme veya sağlayıcı ayarı değişikliği yok. Yerel PostgreSQL olumlu/olumsuz kontroller geçti; gerçek Azure/AWS/Huawei testleri bekliyor. Katalogdaki genel yönetim satırları bu sınırlı şablon nedeniyle topluca tamamlanmış sayılmaz.

- Üç motorda sürüm, edition/DB/PDB, eklenti ve görünür izin keşfi. Destek aralığı bir uyumluluk iddiası değildir; gerçek doğrulama aşağıdaki üç sürümle sınırlıdır. Hatalı veya erişilemeyen kontrol gerekçesiyle döner.
- 142 R0 sözleşmesi; metin, tam sayı, zaman, enum, dizi ve nesne parametreleri. **135 kontrol canlı laboratuvarda geçti.** Bir standby, dört bulut ve iki lisanslı Oracle kontrolü ortam bekler. Üç bakım işlemi dev.7 ile açıldı; 71 yönetim işlemi uygulanmadı.
- Değerler bind edilir; tablo/sütun/anahtar isimleri metadata ile doğrulanır ve motora göre kaçışlanır. Kaynak katalog korunur; iki doğrulanmış SQL düzeltmesi `catalogOverrides.json` ile sürümlenir.
- Ayrı editör, nesne gezgini ve izleme bağlantıları. İşlem kimliği ve running/succeeded/failed/cancelled/unknown_outcome durumları. Sunucunun doğruladığı iptal ile iptal isteği ayrıdır. Sonucu belirsiz yazma tekrar gönderilmez.
- PostgreSQL hata sonrası transaction, Oracle DDL örtük commit ve gerçek transaction durumuna bağlı kapatma. İletişim sonucu belirsizse ayrı kapatma kararı gösterilir.
- SQL Server ve Oracle için bağlantıya özel PEM CA; yanlış CA ve hedef adı reddi. SSH yönlendirmesinde mantıksal DB kimliği korunur. Oracle tünelinde sertifika DN alanı gerekir.
- Sekmelerde metin, seçim, undo ve kaydırma durumu; tablo/sütun tamamlama. Nesne gezgininde sütun, indeks, PK/FK/kısıt ve rutin parametreleri.
- 100/500/1000 satırlı tablo gezgini; tipli filtre, sıralama ve güvenilir anahtarda keyset sayfalama. Nullable veya taşınamayan anahtar tiplerinde sınırlı görünüm. Bu görünüm canlı verinin transaction snapshot'ı değildir; arada değişen/silinen kayıtlar yeni okumada değişebilir.
- PostgreSQL tahmini JSON, SQL Server tahmini XML, Oracle cached cursor DBMS_XPLAN planı. Tahmini plan yolu tek SELECT ile sınırlı; ANALYZE çalıştırılmaz. Anahtarlı tabloda büyük metin/binary okuma sınırları aşağıdadır.
- Kilitleyen oturum ağacı; PostgreSQL/Oracle sayaçlarının iki zaman penceresi; SQL Server'da tamamlanmış iki saatin ayrı Query Store sonuçları. Query Store kapalıysa gerekçe döner. Kümülatif sayaç azalışı, reset epoch/dealloc ve farklı kapsam karşılaştırmayı geçersizleştirir.
- Slot/WAL, transaction log/tempdb ve tablespace/FRA metrikleri. Yeterli örnekte büyüme hızı; yüzde kapasitesinde yeterli gözlenen eğilim varsa dolma tahmini. Başlangıç, NULL, eski, eksik yetkili ve kısmi veriler sağlıklı sayılmaz. Uyarı/kritik eşikleri %80/%90, değiştirilebilir; üç ardışık örnek gerekir.
- **Şema geçmişi:** seçili şemadaki görünür tablo/görünümler ve sütun/indeks/PK/FK/kısıt metadata alanları şifreli yerel görüntüye alınır. Ekran/kasa kapatılıp açılınca korunur. Aynı bağlantı, hedef, DB, kullanıcı ve şema için iki tarih seçilip karşılaştırılır. Yeni görünür, artık görünmeyen ve değişen alanlar ayrılır; kaybolan nesne kesin silinmiş kabul edilmez. Otomatik DDL yoktur.
- Şema yakalama ayrı JDBC bağlantısında, 30 saniye okuma / 200 nesne / 4 MB görüntü sınırında çalışır; sınır, yetki veya okuma hatasında yarım görüntü kaydedilmez. JDBC metadata okuyucusunun sessiz 1000 satır kesmesi açık hataya dönüştürüldü. Eşzamanlı DDL için tutarlı snapshot garantisi yoktur; tetikleyici/rutin/görünüm SQL gövdeleri, CHECK ifadeleri ve tüm üretici özellikleri bu karşılaştırmada değildir.
- Şema geçmişi ayrı SQLite dosyasında XChaCha20-Poly1305 ile korunur. Kasa başına 40 görüntü / 48 MiB içerik kotası; otomatik budama yoktur, kota dolduğunda mevcut görüntüler korunarak yeni kayıt reddedilir. Backup/sync'e dahil değildir. Eski seçili tablo/bellek karşılaştırması ayrıca kalır.
- SQL Server indeks metadata'sı performans DMV yetkisi gerektirmeyen katalog görünümlerinden okunur. INCLUDE sütunları, filtreli/devre dışı indeksler benzersiz sayfalama adayı sayılmaz. Dar SELECT yetkili kullanıcı ve yetkinin kaldırılması canlı test edildi.
- Kasa başına ayrı **7 gün / en fazla 50 MiB** şifreli SQLite metrik önbelleği. XChaCha20-Poly1305 ve kasa anahtarı kullanılır. SQL metni, ham sonuç tablosu, parola ve parametre değerleri yazılmaz. Yalnız sayısal metrikler, metrik kimlikleri, zaman ve durum tutulur; parametre kapsamı anahtarlı özetle ayrılır. Backup/sync'e katılmaz. Ekran en yeni 5000 örneği okur; uygulama kapalıyken toplama yoktur.

## Yeni satır ve kurulum denetimi — dev.10

Tablo gezginindeki **Yeni satır…** alanları metadata ile oluşturur. Identity/generated alanlar sunucuya bırakılır; belirtilmeyen alan, explicit NULL ve girilen değer ayrıdır. Önizleme yazmaz. Onay ayrı transaction'da tam bir satır ekler; dönen PK ile satır okunur ve tip/hassasiyet doğrulanır. String bind ile büyük decimal/tam sayılar JavaScript sayısına dönüştürülmez. Oracle boş metni NULL sayar. Sequence/identity numaralarında başarısız transaction sonrası boşluk oluşabilir.

Plan beş dakikalık, tek kullanımlık ve oturuma bağlıdır. Uygulamada şema/nesne/yetki yeniden okunur; kısa tablo kilidi kullanılır. Hata rollback, değişen şema conflict, commit sonucu belirsizse unknown_outcome olur; otomatik tekrar yoktur. Mevcut RowEditor'ın dar PK/base-table/tip sınırları geçerlidir. Etkin trigger/rule/temporal tablolar desteklenmez. SELECT ve INSERT yanında guard için yeterli metadata erişimi gerekir; Oracle rol üzerinden nesne grant algılama henüz genişletilmedi. Toplu ekleme yoktur.

DB/kullanıcı oluştur içindeki **Mevcut / yarım kurulumu denetle** sadece mevcut DB/PDB/rol/üyelik/ACL bilgilerini okur. observed/missing/unknown, beklenen şablon ve açıklama gösterilir. Tamamlandı veya otomatik devam iddiası yoktur; [provisioning sınırları](DATABASE_PROVISIONING.md).

Kaynaklar: [PostgreSQL INSERT](https://www.postgresql.org/docs/18/sql-insert.html), [SQL Server OUTPUT](https://learn.microsoft.com/en-us/sql/t-sql/queries/output-clause-transact-sql?view=sql-server-ver17), [Oracle generated keys](https://docs.oracle.com/en/database/oracle/oracle-database/19/jjdbc/JDBC-standards-support.html).

## Önizlemeli satır silme — dev.9

Tablo gezginindeki **Sil…** güncel satırı, hedef DB/kullanıcıyı ve parametreli DELETE SQL'ini gösterir. Onay kutusu işaretlenmeden uygulanmaz. Beş dakika geçerli, oturuma bağlı tek kullanımlık plan yalnız helper belleğinde tutulur; her yeni önizleme öncekini geçersiz kılar. Renderer SQL veya anahtar değişikliği kabul edilmez. Başarı tam bir satırın silinip ayrı transaction'da commit edilmesidir; editörün açık transaction'ı commit edilmez.

Uygulama kısa tablo kilidi alır; nesne kimliği/metadata/DELETE yetkisi ve bütün satırı yeniden okur. Satır/şema/nesne değişmişse conflict, kısıt/izin/kilit hatasında failed/rollback, commit cevabı belirsizse unknown_outcome olur. Otomatik tekrar ve commit sonrası undo yoktur. PostgreSQL SHARE ROW EXCLUSIVE, SQL Server TABLOCKX/HOLDLOCK, Oracle EXCLUSIVE; kilit bütçesi 5 saniye, sorgu 10 saniye, ağ 20 saniyedir. Bu, yoğun yazılan tabloya geçici etki edebilir; kuyrukta sınırsız beklenmez.

Mevcut satır düzenleyicinin gerçek tablo, NULL olmayan PK, en çok 100 sütun ve sınırlı veri türü koşulları korunur. Etkin trigger/rule, SQL Server temporal/memory-optimized, CASCADE/SET NULL/SET DEFAULT ilişkileri kapalıdır. Restrictive FK satırı varsa sunucu silmeyi reddeder ve transaction geri alınır. SQL Server DB VIEW DEFINITION; Oracle DBA_CONSTRAINTS/DBA_TRIGGERS okuma erişimi gerekir: yalnız kısmi metadata ile başka şemaya etki yok varsayılmaz. Oracle bu aşamada owner/DELETE ANY TABLE/doğrudan nesne DELETE grant'ini tanır; yalnız rol üzerinden gelen DELETE izni için genişletme bekler. Otomatik GRANT verilmez. Yalnız PG18/MS16/Oracle23 lab major'ları açıktır; tüm patch/edition'lar test edilmiş değildir. Bu kısıtlı akış toplu silme veya bütün trigger/FK davranışlarının çözümlendiği anlamına gelmez.

Kaynaklar: [PostgreSQL LOCK](https://www.postgresql.org/docs/current/sql-lock.html), [SQL Server FK etki/görünürlük](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-foreign-keys-transact-sql?view=sql-server-ver15). Tüm projenin kalanları ayrıca [genel envanterde](PROJECT_STATUS_2026-10-05.md).

## Önizlemeli bakım — dev.7

Kontroller ve yönetim ekranından yalnız **PG-OPS-004 (ANALYZE VERBOSE), MS-OPS-003 (tek istatistik UPDATE STATISTICS), OR-OPS-003 (DBMS_STATS.GATHER_TABLE_STATS)** için Değişiklik planı açılır. Şema/tablo metadata'dan seçilir; SQL Server'da istatistik adı da gerçek metadata'dan gelir. Oracle değerleri bind edilir, diğer motorlarda metadata ile doğrulanan adlar kaçışlanır. Renderer'dan SQL kabul edilmez. 74 yönetim işleminden bu üçü açıldı; **71 işlem hâlâ uygulanmadı**.

Önizleme mevcut nesne kimliği, sütun yapısı, istatistik zamanı ve tahmini satır sayısını okur; çalışacak SQL, hedef ve etkiyi gösterir. SHA-256 hedef/SQL/baseline/bağlantı kapsamına bağlanır; beş dakikalık monotonic süre ve oturuma bağlı tek kullanımlık plan kimliği kullanılır. Yeni önizleme eskisini geçersiz kılar. Açık kullanıcı onayı gerekir; plan yalnız bir uygulama denemesi için kullanılır, hata veya belirsizlikte yeniden gönderilmez. Bu kişisel masaüstü onayıdır, bağımsız şirket onay servisi değildir.

Uygulamada ayrı bağlantıda güncel yetki/nesne/baseline yeniden okunur. Değişen hedefte conflict; gönderilmeden önceki hatada failed; sunucuya gönderildikten sonra yanıt veya son kontrol doğrulanamazsa unknown_outcome döner. Gerçek uyarılar partial, PostgreSQL ANALYZE bilgilendirme mesajları başarılı sonuç içinde ele alınır. Başarı, sunucu komutunun tamamlandığı ve son kontrolün okunabildiği anlamındadır; sorguların hızlandığını kanıtlamaz. Açık editör transaction'ı commit edilmez. Bakım ayrı autocommit bağlantısındadır; Oracle dahil geri alma garantisi verilmez. Query 30 saniye / ağ 40 saniye; PG/MS lock wait 5 saniye. İptal/rollback düğmesi yoktur; kasa/oturum kapanışı bağlantıyı kapatır ve sonucu belirsiz bırakabilir. Sonuçlar yalnız bellektedir.

Sınırlar: yalnız PostgreSQL 18, SQL Server major 16 (2022), Oracle major 23 (laboratuvar 26ai Free) için etkin; diğer sürümler gerekçeyle reddedilir. Gerçek doğrulama aşağıdaki tam sürümlerdedir, tüm edition/patch'ler denenmiş değildir. PostgreSQL partition/inheritance, SQL Server memory-optimized/external, Oracle partition/temporary/nested/IOT ile kilitli/pending/incremental stats bu akışta kapalıdır. Oracle kilit/politika otomatik değiştirilmez. Ön kontrol ile yürütme arasındaki eşzamanlı DDL/yetki yarışı tamamen yok edilmez; sunucunun kendi yetkileri ayrıca uygulanır. Bu altyapı diğer yönetim komutlarını kendiliğinden güvenli veya çalışır hale getirmez. Dev.8 sınırlı provisioning şablonu ayrı akıştır; Team SQL izinlerinin doğrudan bağlantı kapsamı dev.11’de eklendi; zorunlu onay servisi henüz yoktur.

Kaynaklar: [PostgreSQL ANALYZE](https://www.postgresql.org/docs/18/sql-analyze.html), [SQL Server UPDATE STATISTICS](https://learn.microsoft.com/en-us/sql/t-sql/statements/update-statistics-transact-sql?view=sql-server-ver17), [Oracle DBMS_STATS](https://docs.oracle.com/en/database/oracle/oracle-database/19/arpls/DBMS_STATS.html).

## Büyük değerler ve oturum incelemesi — dev.6

- **Tam değer:** Tablo gezginindeki büyük metin/binary hücresinden açılır; ikinci bir açık kullanıcı işlemiyle yeniden okunur. Gerçek tablo, güvenilir primary key ve desteklenen metin/binary türü gerekir. Kimlikler metadata ile doğrulanır; anahtar değerleri bind edilir. Ayrı bağlantıda 8 MiB içerik sınırı ve en fazla iki eşzamanlı okuma vardır. Süre/boyut sınırında kısmi içerik başarılı sonuç olarak verilmez. İlk tablo görünümünden sonra değişmiş veya silinmiş satır açıkça ele alınır; eski snapshot garantisi verilmez.
- **Tam içeriği dosyaya kaydet:** Metin UTF-8 `.txt`, binary birebir `.bin` olarak yazılır. Önizleme yalnız ilk 64 KiB metin / 256 bayt hex gösterir; byte sayısı ve SHA-256 tam yüklü içeriğe aittir. NULL ile boş dosya ayrılır. Dosya seçimi iptal edilebilir; kapatılmış ekrana geç sonuç uygulanmaz. Geçici dosya ve rename kullanılır; geçersiz/çok büyük içerik mevcut hedefi değiştirmez. Kasa kilidi ve Team export kısıtı geçerlidir. Değerler kasa, geçmiş, backup veya sync'e yazılmaz.
- **İndeks ve yedek → Bağlantılar ve transaction:** Kullanıcı/uygulama/durum bazında oturumlar, görünür sunucu limitleri ve mevcut DB'deki açık transaction'lar gösterilir. Yaş, bekleme/blokaj ve varsayılan 300 saniyelik oturumluk uyarı eşiği vardır. Arka plan okuması editörün transaction'ını commit etmez. Sınırlı kullanıcıya görünen sayı tüm sunucu toplamı sayılmaz; bilinmeyen limit/kullanım NULL kalır. SQL Server `user connections=0` otomatik limit anlamındadır. Oracle değerleri mevcut instance/container kapsamındadır; RAC toplamı değildir. Sorgu metni/tuşlar kaydedilmez; otomatik KILL yoktur.

Tam değer okuyucu arbitrary SQL sonuçları, anahtarsız tablolar, görünüm, JSON/XML/BFILE veya 8 MiB üzeri streaming export sağlamaz. Metin UTF-8'e dönüştürülür; veritabanının dahili byte kodlamasını export etmez. Görünüm kapatılması sonucu atar; başlamış okuma backend süre sınırıyla biter, ayrı iptal komutu yoktur. PostgreSQL/SQL Server veriyi sunucuda da sınırlar; Oracle LOB stream kullanır. Transaction paneli bir denetim geçmişi değil, anlık görünür kanıttır; Oracle salt okuma transaction'larının tamamı `V$TRANSACTION`'da bulunmayabilir.

Kaynaklar: [PostgreSQL izleme](https://www.postgresql.org/docs/18/monitoring.html), [SQL Server açık transaction'lar](https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-tran-active-transactions-transact-sql?view=sql-server-ver17), [Oracle kaynak limitleri](https://docs.oracle.com/en/database/oracle/oracle-database/26/refrn/V-RESOURCE_LIMIT.html).

## Önceki üç geliştirme — dev.5

1. **Bağımlı metadata seçicileri:** Kontrol parametrelerindeki şema, tablo/görünüm ve sütunlar gerçek bağlantıdan yüklenir. Şema değişince tablo/sütun, tablo değişince sütun/anahtar seçimi temizlenir. Sütunlar checkbox ile seçilir; virgüllü adlar ayrılmaz. Nitelikli adlar motorun identifier kurallarıyla kaçışlanır. `%/_` JDBC wildcard olarak yorumlanmaz. İsim filtresi, yenileme, yükleniyor/boş/hata durumu vardır. 1000 metadata kaydı sınırında kısmi listeyi tam liste gibi sunmaz. Açık sekme yoksa geçici metadata oturumu açılır ve ekran kapanınca kapatılır. Parametre değişince eski sonuç ve otomatik yenileme sıfırlanır.
2. **İndeks ve yedek → İndeks kanıtları:** Seçili tabloya ait indeks türü, constraint bağımlılığı, boyut ve gözlem kapsamı ayrı kaynaklarda gösterilir. PostgreSQL okuma/tabloda yazma sayaçları ile DB reset/track_counts; SQL Server seek/scan/lookup/update ve başlangıç; Oracle mevcut USER_OBJECT_USAGE izlemesi ile segment alanı okunur. İzleme kendiliğinden açılmaz. Kaynak başına yetki/hata/boş/kısmi durumu vardır; NULL sıfır sayılmaz. Sunucu başlangıcı tüm indeks sayaçlarının kesin reset zamanı değildir. PostgreSQL tablo yazmaları ve SQL Server user_updates gerçek süre/maliyet değildir. Oracle nicel indeks yazma maliyeti henüz yoktur. Otomatik DROP/CREATE veya 'gereksiz indeks' kararı üretilmez.
3. **İndeks ve yedek → Yedek güncelliği:** SQL Server son D/I/L yedekleri, copy-only/hasarlı/checksum durumu, sunucu saatinden yaş ve restore geçmişi; Oracle son 50 RMAN işi, iş durumu ve CDB/PDB kapsamı; PostgreSQL WAL arşivleme bağlamı gösterilir. Oturumluk saat eşiği varsayılan 24 saattir. Boş/gelecek/geçersiz tarihler bilinmiyor kalır. PostgreSQL tam yedek ve harici sağlayıcı geçmişi ile SQL Agent başarısız iş geçmişi henüz bağlı değildir. RMAN geçmişi silinmiş veya yetkiyle görünmez olabilir. Yedek kaydı, dosya mevcudiyeti/backup zinciri/restore testi anlamına gelmez; ekran bunları ayrı belirtir.

Yeni paneller yalnız elle okunur, editör transaction'ından ayrı bağlantı kullanır; sorgu başına 5 saniye, kaynak başına 500 satır/1 MB sınırı vardır. 60 saniyeden eski ölçüm işaretlenir. Şema/sekme/panel/kasa değişince geç gelen sonuç gösterilmez. Sonuçlar yalnız bellektedir; backup/sync'e yazılmaz. Oracle RMAN dolu geçmiş senaryosu bu laboratuvarda yoktur; boş geçmişi doğru gösterme test edilmiştir. SQL Server'da gerçek full/diff/log ve ayrı disposable restore senaryosu ayrıca geçti.

Kaynaklar: [PostgreSQL 18 istatistikleri](https://www.postgresql.org/docs/18/monitoring-stats.html), [SQL Server indeks kullanım sayaçları](https://learn.microsoft.com/en-us/sql/relational-databases/system-dynamic-management-objects/sys-dm-db-index-usage-stats-transact-sql), [SQL Server restore geçmişi](https://learn.microsoft.com/en-us/sql/relational-databases/system-tables/restorehistory-transact-sql), [Oracle RMAN iş geçmişi](https://docs.oracle.com/en/database/oracle/oracle-database/19/refrn/V-RMAN_BACKUP_JOB_DETAILS.html).

## Güvenli satır düzenleme — dev.4

Tablo gezginindeki **Düzenle** düğmesi satırı anahtarıyla yeniden okur. Alanlar metin/NULL olarak taşınır; büyük tam sayılar ve decimal değerler JavaScript sayısına çevrilmez. Önce/sonra alanları ve parametreli SQL gösterilir. **Onayla ve kaydet**, editörün açık transaction'ından ayrı bağlantıda tek satırı commit eder.

- Taslak ve tek kullanımlık plan yalnız Java belleğinde, oturuma bağlı ve 5 dakika geçerlidir. En fazla 4 taslak; değiştirilmiş/eski plan veya ikinci uygulama reddedilir. Frontend, onaylanan plandaki değerleri uygulama sırasında değiştiremez.
- Kaydetmede metadata yeniden doğrulanır, satır kilitlenir ve yalnız değiştirilen alanlar değil ilk okunan satırın tamamı karşılaştırılır. Eşzamanlı değişiklik/silme veya şema farkında üzerine yazılmaz; güncel değerler gösterilir.
- Constraint hatası, kilit zaman aşımı ve geri okunan değerde hassasiyet kaybı işlemi rollback eder. Oracle boş metin → NULL, DATE/timestamp ve unconstrained NUMBER farkları uygulanır. Commit cevabı/bağlantı kaybında sonuç belirsiz olarak kalır; otomatik tekrar yoktur.
- Bu aşama yalnız **mevcut satır güncelleme** içindir. Gerçek tek tablo, NULL kabul etmeyen primary key, en fazla 100 sütun gerekir. PK/identity/generated alanlar değiştirilemez. Gezginde kısaltılabilecek uzun metin anahtarları düzenleme ve keyset devamı için kullanılmaz. Unique-index-only, görünüm, PostgreSQL partition/inheritance ve LOB/binary/JSON/floating/sınırsız metin alanlı tablolar gerekçesiyle salt okunurdur. Alan 16.384, satır 131.072 karakterle sınırlıdır; kısaltılmış içerik düzenlenmez.
- Kaydetmeden önceki “Değişiklikleri geri al” taslağı sıfırlar; commit sonrasında undo anlamına gelmez. Trigger'ın değiştirdiği düzenlenen değer doğrulanamazsa rollback yapılır; dış sistem/otonom transaction yan etkilerinin geri alınacağı garanti edilmez. Satır taslakları geçmişe, kasaya veya backup'a yazılmaz.

Kilit mekanizmaları: [PostgreSQL](https://www.postgresql.org/docs/current/explicit-locking.html), [SQL Server tablo ipuçları](https://learn.microsoft.com/en-us/sql/t-sql/queries/hints-transact-sql-table), [Oracle SELECT FOR UPDATE](https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/SELECT.html).

## Doğrulanan ortamlar ve sınırlar

Windows 11 ve Ubuntu 24.04 WSL Java 21: PostgreSQL 18.3, SQL Server 2022 Developer 16.00.4295, Oracle AI Database 26ai Free 23.26.3.0.0. Üç motorda gerçek sorgular ve dar yetkili izleme, iptal ve bağlantı kaybı; SQL Server/Oracle özel CA olumlu/olumsuz TLS testleri yapıldı. Linux kontrolü Java servis testidir, Linux native UI testi değildir.

Oracle cursor cache'ten silinen ve yeniden oluşan SQL için tam geçmiş garanti edilmez; sayaç azalması ve sunucu reset kontrolleri mevcut, Oracle 19c/RAC reset/plan matrisi doğrulanmadı. Query Store yalnız seçilen pencereye tamamen sığan runtime interval'larını karşılaştırır; eksik kapsam açıkça belirtilir. Sonuç tabloları ve planlar boyut sınırına tabidir; kısaltılmış sonuçların CSV'si yalnız görünen kısmı içerir.

Team SQL’in kalan kapsamı, zorunlu onay servisi, bulut adaptörleri, katalogdaki genel yönetim/provisioning varyantları, toplu ekleme/silme, tam DDL/nesne gövdeleri karşılaştırması, sınırsız LOB/diğer veri türleri ve diğer motor sürümleri tamamlanmış sayılmaz. Team kasasında doğrudan SQL IPC/export dev.11 izinleriyle etkin; diğer Team SQL sınırları yukarıda listelidir. macOS, üretim sunucuları, Oracle 19c Enterprise/RAC, AWR/ASH ve bulut ortamları doğrulanmadı.

## Çalıştırma ve paket

```powershell
pnpm install --frozen-lockfile
pnpm db:prepare
pnpm db:dev
# Test özelliği olmadan ayrı Windows geliştirme paketi:
pnpm db:build
node scripts/package-database-dev.mjs
```

Paket: `artifacts/database-workspace/TermTerm-0.5.0-dev.12-windows`. Java 21 çalışma zamanı, üç JDBC sürücüsü ve Mosh/DLL/lisansları dahildir; Windows WebView2 gerekir. `TermTerm-Database-Dev.exe` çalıştırılır. Kurulu kararlı TermTerm'in uygulama kimliği/kasaları değiştirilmez. Paket özel laboratuvar kasası, parola veya CA özel anahtarı içermez; kişisel test kasası oluşturulur.

Laboratuvar: `db-lab.sh`, `db-lab-postgres.sh`; Windows PG portu için `db-lab-proxy.py`. SQL Server/Oracle TLS için `db-lab-tls.sh` ve `db-lab-oracle-tls-proxy.py`. TLS betiği yalnız adları sabit TermTerm test container'larını yeniden başlatır; üretim için kullanılmaz. PostgreSQL pg_stat_statements testi mevcut 18/termterm test kümesinde etkinleştirildi. Özel dosyalar `.lab/database` altında kalır.

Test ayrıntıları: [DATABASE_TEST_RESULTS.md](DATABASE_TEST_RESULTS.md). İşlem bazında 216 satırlık durum: [DATABASE_CATALOG_VALIDATION.md](DATABASE_CATALOG_VALIDATION.md).

## Sıradaki geliştirme önerileri

Mevcut katalog 216 işlem olarak kaldı; yeni görünümler bu kanıtları günlük kullanımda birleştirir. Tamamlanan ve kalan adımlar:

| Öncelik | Geliştirme | Mevcut katalogla ilişkisi |
|---|---|---|
| Tamamlanan ilk adım | Mevcut satır güncelleme: primary key, tam satır karşılaştırması, önizleme ve rollback | dev.4 üç motorda doğrulandı. Dev.9 silmeyi, dev.10 eklemeyi ekledi; toplu işlem ve diğer veri türleri açık kalır. |
| dev.5 / dev.6 | Metadata şema/tablo/sütun seçicileri; açık istekle büyük metin/binary okuma ve export | Üç motorda çalışıyor; tam değer 8 MiB ve güvenilir primary key ile sınırlı. |
| dev.5 | İndeks inceleme paneli: kullanım, yazma işlem göstergeleri, boyut, constraint ve sayaç bağlamı | PG-MON-023, MS-MON-022/024. Salt unused/missing-index sonucuyla otomatik DROP/CREATE yapılmamalı. |
| dev.5 | Yedek güncelliği paneli: full/diff/log/RMAN kayıtları, hata/yaş ve mevcut restore geçmişi | MS-MON-026, OR-MON-025. Harici restore testi kanıtı ve eksik sağlayıcılar henüz bağlı değil. |
| dev.6 | Bağlantı ve uzun transaction incelemesi: kullanıcı/uygulama oturumları ve blokajla birlikte yaş | Üç motorda görünür kapsam/limit ve eksik yetki ayrılır; otomatik oturum sonlandırma yok. |

Dev.8 sınırlı yeni DB/hesap provisioning şablonunu ve gerçek RW/RO testlerini tamamladı. Sonraki sıra: (1) kalan katalog yönetim varyantlarını komuta özgü ön koşul ve son kontrollerle uyarlama, (2) gerçek Azure/AWS/Huawei ortamları ve geçici owner yükseltme/eksiksiz geri alma, (3) Team SQL tüneli/çevrimdışı SQL ve zorunlu onay servisinin masaüstü/IPC sınırları. Toplu ekleme/silme ve 8 MiB üzeri akışlı export ayrıca açık kalır. Bu sonraki adımlar uygulanmış veya test edilmiş değildir.

Metadata görünürlüğü/indeks alanları için resmi kaynaklar: [Microsoft metadata yetkileri](https://learn.microsoft.com/en-us/sql/relational-databases/security/metadata-visibility-configuration?view=sql-server-ver17), [sys.index_columns](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-index-columns-transact-sql?view=sql-server-ver17). Karşılaştırma bu görünürlük sınırını kaldırmaz.
