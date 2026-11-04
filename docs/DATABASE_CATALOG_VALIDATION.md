# Database Workspace katalog karşılaştırması

0.5.0-dev.8 · 2026-10-05T09:50:02.732Z

216 tanım: PostgreSQL 76, SQL Server 69, Oracle 71. Başlangıçta genel SQL editörü veya bu katalog için bir yürütücü mevcut değildi. Tanımlar kaynak katalogdan çıkarıldı; aşağıdaki sonuçlar gerçek testleri temsil eder. SQL dosyasının bulunması tamamlanma sayılmaz.

135 R0 işlem PostgreSQL 18.3, SQL Server 2022 ve Oracle Free laboratuvarlarında tipli parametre yürütücüsü üzerinden geçti. Dev.7 ile üç istatistik bakımı önizleme/onay akışında doğrulandı (PG-OPS-004, MS-OPS-003, OR-OPS-003). Dev.8 yeni DB/hesap ve extension şablonunun kapsamı DATABASE_PROVISIONING.md içinde ayrıca doğrulanır; genel katalog varyantları topluca tamamlanmış sayılmaz. Kalan 71 katalog yönetim işlemi, bir standby, dört bulut ve iki lisanslı Oracle kontrolü açık kaldı. Boş sonuçlar yalnız sorgunun çalıştığını kanıtlar, sistemin sağlıklı olduğunu değil. Her tanımın SQL'i, risk, yetki, maliyet, parametre ve kaynak bilgisi src/database/catalog.json içindedir. Parametre adları, gerçek sürüm, sonuç kolonları ve süreler src/database/validation.json içindedir; parametre değerleri ve ham sonuçlar bu rapora yazılmaz.

| İşlem | Başlık | Risk | Durum | Gerçek test motoru/sürümü |
|---|---|---|---|---|
| PG-PROV-001 | DB/rol çakışması ve oluşturma yetkisi keşfi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-PROV-002 | Owner, izin rolleri ve bağlantı kimlikleri oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-003 | Doğru owner ile yeni database oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-004 | Yeni DB erişimini daraltma ve uygulama schema oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-005 | Mevcut nesneler için RW ve readonly izinleri | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-006 | Gelecek nesnelerin owner bazlı default privileges ayarı | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-007 | Migration oturumunu doğru nesne sahibiyle yürütme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-008 | İsteğe bağlı rol/DB oturum politikaları | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-PROV-009 | Oluşturma sonrası owner ve effective izin doğrulaması | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-PROV-010 | Ayrı monitoring hesabı oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-MON-001 | Bağlantı kimliği, sürüm ve primary/replica durumu | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-002 | Database listesi ve boyutları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-003 | Temel çalışma ayarları ve restart bekleyen ayarlar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-004 | Bağlantı doluluğu ve durum dağılımı | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-005 | Aktif ve uzun süren sorgular | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-006 | Idle in transaction ve eski transactionlar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-007 | Blocking zinciri ve bekleyen oturumlar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-008 | Seçili oturumun lock ayrıntıları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-009 | Primary üzerindeki replication durumu ve byte gecikmeleri | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-010 | Replica üzerindeki receive/replay ilerlemesi | R0 | Standby ortamı bekliyor | — |
| PG-MON-011 | WAL receiver sürüm bağımsız alan çıkarımı | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-012 | Replication slotları ve WAL tutulması | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-013 | WAL üretim sayaçları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-014 | Archive sağlığı | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-015 | pg_wal dosya alanını ölçme | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-016 | Prepared transactionlar ve eski 2PC işlemler | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-017 | Transaction ID wraparound riski: DB seviyesinde | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-018 | Eski frozen XID sahibi tablolar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-019 | Autovacuum/analyze ve dead tuple tahminleri | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-020 | Vacuum ve index oluşturma ilerlemesi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-021 | Tablo, index, TOAST ve yardımcı alan boyutları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-022 | Partition ağacı ve child boyutları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-023 | Index envanteri, kullanım ve constraint bağımlılıkları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-024 | Geçersiz veya hazır olmayan indexler | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-025 | İstatistik tabanlı büyük sequential scan adayları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-026 | pg_stat_statements en maliyetli sorgular | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-027 | Database sayaçları: cache, temp, deadlock ve transaction | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-028 | IO sayaçları: PostgreSQL 16+ sürüm uyumlu projection | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-029 | Checkpointer ve bgwriter sürüm uyumlu sayaçlar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-030 | Checkpointer sayaçları: PostgreSQL 17+ | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-031 | Roller ve ayrıcalıklı hesaplar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-032 | Role üyelik grafiği | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-033 | Schema ve nesne bazlı effective izin matrisi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-034 | Default privileges görünümü | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-035 | RLS ve SECURITY DEFINER incelemesi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-036 | TLS oturumları ve şifrelenmeyen bağlantılar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-037 | Publication listesi ve izlenen tablolar | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-038 | Logical subscription worker durumu | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-039 | Prepared olmayan büyük/uzun COPY ve analyze ilerlemesi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-040 | Extension ve capability keşfi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-041 | Seçili tablonun kolonları ve default/identity bilgisi | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-042 | Constraint ve foreign key bağımlılıkları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-043 | Kontrollü veri önizleme | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-044 | Seçili tabloda pgstattuple yaklaşık fiziksel ölçüm | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-MON-045 | Seçili tabloda tam pgstattuple ölçümü | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-MON-046 | Standby query conflict sayaçları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-047 | Tablo autovacuum override ve persistence kontrolü | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-MON-048 | Index dışındaki nesne sahiplik sapmaları | R0 | Canlı laboratuvarda geçti | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-OPS-001 | Seçili sorguyu iptal etme | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-002 | Seçili oturumu sonlandırma | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-003 | Tek tabloda VACUUM ANALYZE | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-004 | Tek tabloda ANALYZE | R2 | Önizlemeli bakım canlı laboratuvarda geçti (dev.7 sınırlarıyla) | 18.3 (Ubuntu 18.3-1.pgdg24.04+1) |
| PG-OPS-005 | Concurrent index oluşturma | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-006 | Concurrent index yeniden oluşturma | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-007 | VACUUM FULL: uzman bakım | R3 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-008 | Replication slotunu silme | R3 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-009 | Seçili tablolardan publication oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-010 | Yalnız onaylanan routine için EXECUTE verme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-011 | Bağlantı hesabını devre dışı bırakma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-012 | Parola rotasyonu | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-013 | Readonly rolünden üyelik kaldırma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-014 | Tek schema/table için owner düzeltmesi | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-OPS-015 | Tehlikeli DROP DATABASE planı | R3 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-001 | Sürüm, platform, DB ve principal çakışma keşfi | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-PROV-002 | Self-managed database ve fiziksel dosya planı | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-003 | Dedicated DB owner principal oluşturma ve atama | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-004 | Uygulama, readonly ve migration loginleri | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-005 | DB user, schema owner ve özel izin rolleri | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-006 | İzole owner ile uygulama schema oluşturma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-007 | Schema kapsamlı RW/readonly izinleri ve üyelikler | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-008 | Ayrı migration yetki rolü | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-009 | Greenfield DB güvenlik ve güvenilirlik ayarları | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-010 | Azure SQL Database contained kullanıcı varyantı | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-011 | Monitoring permission planı: server ve DB ayrı | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-PROV-012 | Target kimlikle effective izin testi | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-001 | Sunucu ve DB kimliği | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-002 | Database durumu, recovery modeli ve log reuse nedeni | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-003 | Oturum sayısı ve uygulama dağılımı | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-004 | Aktif istekler ve statement metni | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-005 | Blocking ve blocker kimliği | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-006 | Eski açık transactionlar ve kullanılan log | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-007 | Bekleme sayaçları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-008 | Şu an bekleyen görevler | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-009 | Transaction log doluluğu | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-010 | Data/log file kapasitesi ve autogrowth | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-011 | DB file IO gecikme sayaçları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-012 | Self-managed disk/volume boşluğu | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-013 | Tempdb alan dağılımı | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-014 | Tempdb kullanan sessionlar | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-015 | Memory grant bekleyen sorgular | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-016 | SQL process memory ve host memory | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-017 | Scheduler ve CPU baskısı adayları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-018 | En pahalı cached queryler | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-019 | Query Store etkinlik ve kapasite durumu | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-020 | Query Store zaman aralığında en maliyetli sorgular | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-021 | Tablo boyutları ve tahmini satır sayısı | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-022 | Index envanteri ve kullanımı | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-023 | Tek index/tablo fragmentation incelemesi | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-MON-024 | Missing-index öneri adayları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-025 | İstatistik güncelliği ve modification counter | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-026 | Backup geçmişi ve son full/diff/log | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-027 | Agent job son durumları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-028 | Always On replica ve queue durumu | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-029 | Login, user ve role üyelikleri | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-030 | Explicit DB permission matrisi | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-031 | Login eşleşmesi eksik SQL user adayları | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-032 | Şifreli bağlantı kontrolü | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-033 | Kolonlar, identity, computed ve defaultlar | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-034 | PK/unique/FK metadata | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-035 | Kontrollü veri önizleme | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-036 | Azure SQL Database kaynak kullanım aralığı | R0 | Bulut adaptörü ve hesabı bekliyor | — |
| MS-MON-037 | Aktif bakım/backup/restore ilerlemesi | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-038 | Disabled indexler ve untrusted constraintler | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-039 | VLF ve log truncation ayrıntısı | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-MON-040 | Sorgu deadlock olayları: event session keşfi | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-OPS-001 | Oturumu sonlandırma | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-002 | Rollback ilerleme kontrolü | R0 | Canlı laboratuvarda geçti | 16.00.4295 |
| MS-OPS-003 | Tek istatistiği güncelleme | R2 | Önizlemeli bakım canlı laboratuvarda geçti (dev.7 sınırlarıyla) | 16.00.4295 |
| MS-OPS-004 | Tek index reorganize | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-005 | Tek index offline rebuild | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-006 | DBCC CHECKDB fiziksel doğrulama | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-007 | DBCC CHECKDB tam doğrulama | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-008 | COPY_ONLY checksum full backup | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-009 | Login devre dışı bırakma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-010 | Login parola rotasyonu | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-011 | Sadece onaylı stored procedure için EXECUTE | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-012 | Yetki rolü üyeliğini kaldırma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-OPS-013 | Kontrollü DROP DATABASE | R3 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-001 | Container, session ve sürüm keşfi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-PROV-002 | Opsiyonel yeni PDB — yalnızca self-managed OMF | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-003 | Opsiyonel uygulama tablespace — OMF | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-004 | NO AUTHENTICATION schema owner ve sınırlı kota | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-005 | Kontrollü migration proxy hesabı | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-006 | Runtime ve readonly kimlikleri / permission rolleri | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-007 | Onaylı tabloda RW ve gerçekten dar RO grantleri | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-008 | Gerekli sequence / package erişimi | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-009 | Migration sonrası nesne grant reconciliation girdisi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-PROV-010 | En az ayrıcalıklı monitoring hesabı — başlangıç profili | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-PROV-011 | Gerçek app/reader/migrator bağlantısıyla yetki doğrulama | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-001 | Instance ve database rol / açık durumu | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-002 | PDB listesi ve açık durumu | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-003 | Sessions / processes kaynak limitleri | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-004 | Aktif kullanıcı oturumları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-005 | Blocking ve final blocker | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-006 | Uzun çalışan SQL adayları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-007 | Açık transaction ve UNDO tüketimi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-008 | Enqueue lock detayları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-009 | Long operations ilerlemesi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-010 | Wait event kümülatif sayaçları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-011 | Redo, commit, logical/physical IO sayaçları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-012 | Kısa dönem sistem metrikleri | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-013 | SQL cache toplam elapsed / CPU / buffer gets | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-014 | Seçili SQL child cursor detayları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-015 | Seçili local cached cursor planı | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-016 | Tablespace kapasitesi ve gerçek kullanılan bloklar | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-017 | Datafile allocation / autoextend / maxsize | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-018 | TEMP alanının kullanılabilir kapasitesi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-019 | TEMP kullanan oturumlar | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-020 | UNDO geçmişi / snapshot-too-old / alan hataları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-021 | FRA limiti / kullanılan / geri kazanılabilir alan | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-022 | Archive destination hataları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-023 | Data Guard lag ve veri tazeliği | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-024 | Standby archive gap ve apply süreçleri | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-025 | RMAN backup job geçmişi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-026 | Kaydedilmiş block corruption | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-027 | Yakın tarihli alert kayıtları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-028 | Invalid nesneler ve derleme hataları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-029 | Unusable index ve partitionlar | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-030 | Tablo istatistikleri güncelliği | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-031 | Schema segment boyutları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-032 | Scheduler başarısızlıkları / planlı işler | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-033 | Kullanıcı, expiry ve authentication tipi | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-034 | Role ve system privilege grafiği | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-035 | Nesne ve sütun grantleri | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-036 | Tablespace kotaları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-037 | Explorer: sütun / default / nullability | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-038 | Explorer: constraint / PK / FK sütunları | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-039 | Data Browser: sınırlı ilk sayfa | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-040 | Service ve parametre snapshot | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-041 | SGA / PGA snapshot | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-042 | Redo log grupları ve üye sağlığı | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-043 | Dağıtık in-doubt transaction | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-044 | Proxy erişim envanteri | R0 | Canlı laboratuvarda geçti | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-MON-045 | Lisans kapılı ASH özeti — varsayılan kapalı | R0 | Lisans nedeniyle kapalı | — |
| OR-MON-046 | Lisans kapılı Real-Time SQL Monitoring — varsayılan kapalı | R0 | Lisans nedeniyle kapalı | — |
| OR-OPS-001 | Seçili SQL yürütmesini iptal et | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-002 | Kontrollü session kill | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-003 | Tek tablo için kontrollü optimizer statistics | R2 | Önizlemeli bakım canlı laboratuvarda geçti (dev.7 sınırlarıyla) | Oracle AI Database 26ai Free Release 23.26.3.0.0 - Develop, Learn, and Run for Free Version 23.26.3.0.0 |
| OR-OPS-004 | Onaylı nesneyi yeniden derleme | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-005 | Tek nonpartitioned index rebuild — offline | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-006 | Tablespace kotasını açık sınırla değiştirme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-007 | OMF ile tek tablespacee sınırlı datafile ekle | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-008 | Hesabı yeni girişlere kilitleme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-009 | Parola döndürme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-010 | Migration proxy yetkisini kaldırma | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-011 | Nesne yetkisini kontrollü revoke | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-OPS-012 | DROP USER CASCADE — uygulama schema verisini siler | R3 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-CLOUD-001 | RDS PostgreSQL: IAM login role ve dar data rolü | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| PG-CLOUD-002 | Azure PostgreSQL: Entra principal envanteri | R0 | Bulut adaptörü ve hesabı bekliyor | — |
| PG-CLOUD-003 | Azure PostgreSQL: Object ID ile Entra hesabı eşleme | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-CLOUD-001 | Azure SQL: Entra kullanıcı/grup ve dar rol üyeliği | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| MS-CLOUD-002 | Azure SQL: Geo-replication durumu | R0 | Bulut adaptörü ve hesabı bekliyor | — |
| MS-CLOUD-003 | RDS SQL Server: Native task durumu | R0 | Bulut adaptörü ve hesabı bekliyor | — |
| MS-CLOUD-004 | RDS SQL Server: Yeni S3 nesnesine native FULL backup | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-CLOUD-001 | RDS Oracle: Provider prosedürüyle SQL cancel | R2 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
| OR-CLOUD-002 | RDS Oracle: Seçili SYS view için monitor grant | R1 | Yönetim değişiklik planı ve uygulaması bekliyor | — |
