# Team DB kurulumu — şirket owner/appuser şablonu

**0.4.2 aynı sunucu için tercih edilen yol:** [Parolasız Unix soketi + IP üzerinden yerel HTTPS](TEAM_SOCKET_KURULUM_TR.md). Aşağıdaki metin ayrıca parolalı/TLS DB isteyen kurulum içindir; rol yapısı iki yolda da aynıdır.

Bu yol, şirketinizin `treasury_preprod` rol düzenini **yeni `termterm_team` veritabanına** uyarlar. Treasury veritabanında çalıştırılmaz. Mevcut Team veritabanını veya hesaplarını yeniden adlandırmaz. PostgreSQL 18 üzerinde doğrulanır; Debian 13/Linux servis kurulumu içindir.

Bu dosya ile [TLS/CA rehberini](TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md) birlikte kullanın. Aşağıdaki hesapları seçiyorsanız eski `prepare-database.sql` dosyasını **çalıştırmayın**. Önceden kurulu `migrator/service` düzeni çalışmaya devam eder; bu şablona geçmek için yeniden kurulum gerekmez.

## 1. Hesapların görevi

| DB rolü | Kullanım |
|---|---|
| `termterm_team` | NOLOGIN, NOINHERIT taban grubu; DB CONNECT/TEMP ve public USAGE |
| `termterm_team_owner` | LOGIN; DB/public/Team şeması sahibi, tablo ve indeks oluşturma, migration ve yönetim |
| `termterm_team_app` | NOLOGIN uygulama grubu; owner üyeliği ve doğrudan Team tablo izni yok |
| `termterm_team_developer` | NOLOGIN grup; Team verilerine doğrudan erişim verilmez |
| `termterm_team_readonly` | NOLOGIN grup; adına bakarak Team sırlarını okuma hakkı verilmez |
| `termterm_team_appuser` | LOGIN; servisin DB hesabı, app grubunda; yalnız izinli Team API fonksiyonları |

**Veri sahipliği ve yönetimi owner'dadır.** Servis de normal kullanıcı işlemlerini kaydeder; bunu owner kimliğiyle bağlanarak değil, migration'ın izin verdiği sunucu fonksiyonlarını çağırarak yapar. Team üyeleri bu PostgreSQL hesaplarını kullanmaz; liderin oluşturduğu Team kullanıcı adı/parolasıyla masaüstünden HTTPS servisine girer.

Verdiğiniz genel şablondaki `ALTER DEFAULT PRIVILEGES ... GRANT SELECT, INSERT, UPDATE, DELETE ...` komutları burada bilerek uygulanmaz. Şemasız yazıldığında owner'ın gelecekte oluşturacağı Team tablolarına da yayılır; appuser'a doğrudan tablo erişimi verir. Team servisi böyle bir hesabı reddeder. Team kayıtları `public` içinde değil, `termterm_team_v2` şemasında tutulur. [PostgreSQL default privilege kapsamı](https://www.postgresql.org/docs/18/sql-alterdefaultprivileges.html).

Owner parolasını servise, masaüstü bağlantı profiline veya üyelere vermeyin. `appuser` hesabını owner rolüne üye yapmayın. Developer/readonly gruplarına sırf adları nedeniyle Team tablosu okuma veya API çalıştırma izni eklemeyin; kullanıcı yetkileri Team yönetiminden verilir.

## 2. Locale ve dosyayı hazırlama

Yeni SQL dosyası: [prepare-database-owner-template.sql](../team-service/deploy/prepare-database-owner-template.sql). Paketinizde yoksa bu dosyayı `/opt/termterm-team/deploy/prepare-database-owner-template.sql` olarak kaydedin. Eski Docker paketini yeniden derlemeniz gerekmez; aynı 0.4.1 servisinin DB hesapları yapılandırılabilir.

DB sunucusunda:

```bash
locale -a
```

Çıktıda `en_US.utf8` / `en_US.UTF-8` bulunmalı. Yoksa Debian'da:

```bash
sudo apt-get install locales
sudo localedef -i en_US -f UTF-8 en_US.UTF-8
locale -a
```

SQL açıkça `LOCALE_PROVIDER libc` seçer; UTF8 encoding ve `en_US.UTF-8` collate/ctype kullanır. Locale bulunamadı hatasını başka locale seçip geçmeyin. Çalışan PostgreSQL yeni locale'i görmüyorsa doğru küme için bakım kesintisinde `sudo pg_ctlcluster 18 main restart` yapıp tekrar kontrol edin; mevcut başka servisleri etkileyen restart'ı plansız uygulamayın.

## 3. DB, gruplar ve owner oluşturma

**Sadece ilk kurulumda**, DBA hesabıyla:

```bash
sudo -u postgres psql -X -p 5432 -d postgres \
  -f /opt/termterm-team/deploy/prepare-database-owner-template.sql
```

Sırayla owner ve appuser parolası sorulur; her parola doğrulama için iki kez girilir. Farklı, güçlü parolalar belirleyin ve parola kasanızda saklayın. Boş `password ''` kullanılmaz; parolalar SQL dosyasına/komut geçmişine yazılmaz ve SCRAM olarak kaydedilir.

Özet: DB=`termterm_team`, owner=`termterm_team_owner`, servis=`termterm_team_appuser`. Script tabloları oluşturmaz; `pgcrypto` eklentisini hazırlar. Rol/DB zaten varsa durur, silmez veya üzerine yazmaz. CREATE DATABASE transaction dışında çalıştığından hata sonrası kısmen oluşmuş hesaplar olabilir; DBA sebebi düzeltip kalan SQL adımlarını inceleyerek sürdürmeli, dolu DB'yi silmemelidir.

## 4. TLS ve pg_hba.conf

[TLS rehberinin 1–6. adımlarını](TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md) uygulayın; yalnız HBA'da hesapları aşağıdaki gibi kullanın. `SHOW hba_file` ile bulunan dosyada mevcut yerel peer kurallarını koruyun. Bu blok geniş izinlerden önce gelmeli; bütün dosyanın yerine geçmez:

```conf
hostnossl termterm_team all                                          0.0.0.0/0     reject
hostnossl termterm_team all                                          ::/0          reject
hostssl   termterm_team termterm_team_appuser,termterm_team_owner     127.0.0.1/32  scram-sha-256
hostssl   termterm_team termterm_team_appuser,termterm_team_owner     ::1/128       scram-sha-256
host      termterm_team all                                          0.0.0.0/0     reject
host      termterm_team all                                          ::/0          reject
```

Bu örnek PostgreSQL ve Compose servisinin aynı Linux makinesinde, host network ile çalışması içindir. Ayrı DB makinesinde son reject satırlarından önce yalnız servis/yönetim makinesinin IP'sini açın; sertifikanın SAN alanı bağlantıda kullanılan DB adresini kapsamalı. Rehberin ayrı sunucu bölümünü izleyin. HBA hatalarını kontrol edip reload yapın:

```bash
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT line_number,error FROM pg_hba_file_rules WHERE error IS NOT NULL;"
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT pg_reload_conf();"
```

## 5. Owner ile migration

TLS rehberinin 8. adımındaki özel dizin, CA kopyası ve `migration.pgpass` dosyasını hazırlayın. Dosya root'a özel 0600 olsun. İçindeki satır:

```text
localhost:5432:termterm_team:termterm_team_owner:OWNER_PAROLASINI_BURAYA_YAZIN
```

Paroladaki `:` → `\:`, `\` → `\\` kaçışlarını uygulayın. Parolayı aşağıdaki komutlara yazmayın:

```bash
export PGHOST=localhost PGPORT=5432 PGDATABASE=termterm_team PGUSER=termterm_team_owner
export PGSSLROOTCERT=/etc/termterm-team-private/config/postgres-ca.crt
export PGPASSFILE=/etc/termterm-team-private/migration.pgpass
export PGSSLMODE=verify-full
psql -X -v ON_ERROR_STOP=1 -c "SELECT current_user,current_database(),ssl FROM pg_stat_ssl WHERE pid=pg_backend_pid();"
cd /opt/termterm-team
python3 deploy/manage.py migrate --service-role termterm_team_appuser
```

Beklenen: owner ile TLS bağlantısı ve 0.4.1 için 64 migration doğrulaması. Team şema, tablo, indeks ve fonksiyonlarını bu komut oluşturur. Elle tablo yazmanız gerekmez; servis açılışı otomatik migration **yapmaz**. Sonraki sürümlerde ilk kurulum SQL'ini tekrar çalıştırmayın; [yedekli upgrade plan/apply/confirm yolunu](TEAM_UPGRADES_TR.md) aynı owner hesabı ve `--service-role termterm_team_appuser` ile kullanın.

## 6. Appuser ile servisi çalıştırma

`/etc/termterm-team-private/config/service.json` içeriği:

```json
{
  "listen": "127.0.0.1:41841",
  "database": {
    "host": "localhost",
    "port": 5432,
    "database": "termterm_team",
    "username": "termterm_team_appuser",
    "password": "APPUSER_PAROLASINI_BURAYA_YAZIN",
    "caPath": "/etc/termterm-team/postgres-ca.crt"
  }
}
```

Parola alanına 3. adımda verdiğiniz gerçek appuser parolasını yazın; çift tırnak ve ters eğik çizgileri JSON'a uygun kaçışlayın. Dosya root:10001, 0640 olmalı. `caPath` container içindeki DB CA dosyasıdır. Bu CA masaüstündeki **Şirket CA sertifikası** alanına yazılan HTTPS CA'dan ayrıdır.

[TLS rehberinin 10. adımından](TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md) Caddy/Compose kurulumuna devam edin. İlk lideri, yukarıdaki owner ortam değişkenleri hâlâ ayarlıyken oluşturun:

```bash
cd /opt/termterm-team
python3 deploy/manage.py bootstrap --team "Sirket" --username lider \
  --display-name "Takim Lideri" --output /etc/termterm-team-private/ilk-lider.json
```

Bu dosya tek kullanımlık giriş bilgisidir; üyelere DB parolası dağıtılmaz. Mevcut lider dosyasının üzerine yazılmaz. HTTPS `/ready` kontrolünden sonra lider masaüstünde giriş yapıp üyeleri ve kaynak yetkilerini oluşturur.

## 7. Yetki doğrulaması

Owner bağlantısıyla:

```sql
SELECT datname, pg_get_userbyid(datdba) AS owner, datcollate, datctype
FROM pg_database WHERE datname='termterm_team';
SELECT nspname, pg_get_userbyid(nspowner) AS owner
FROM pg_namespace WHERE nspname IN ('public','termterm_team_v2');
SELECT pg_has_role('termterm_team_appuser','termterm_team_owner','MEMBER') AS app_is_owner;
SELECT count(*) AS accessible_team_tables
FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
WHERE n.nspname='termterm_team_v2' AND c.relkind='r'
AND has_table_privilege('termterm_team_appuser',c.oid,'SELECT, INSERT, UPDATE, DELETE');
```

Beklenen: DB ve iki şema owner=`termterm_team_owner`, `app_is_owner=f`, `accessible_team_tables=0`. Bu sıfır, uygulamanın veri yazamadığı anlamına gelmez; API fonksiyonları izinli işlemleri yapar. Hatalı tablo yetkisini servis hesabına owner vererek çözmeyin.
