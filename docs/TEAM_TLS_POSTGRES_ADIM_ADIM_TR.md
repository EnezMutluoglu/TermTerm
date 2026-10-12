# Team kurulumu: PostgreSQL TLS, HBA ve şirket CA'sı — adım adım

Bu kılavuz **TermTerm 0.4.1** içindir. Ana örnek: PostgreSQL elle kurulmuş ve Team Docker servisiyle **aynı Linux x64 sunucusunda** çalışıyor. İşletim sistemi Debian 13 olabilir; örnekte PostgreSQL **18 / main / 5432** kullanılır. Farklı sürüm/küme/port varsa bütün komutları gerçek değerlerle değiştirin. Ayrı DB sunucusunun farkları 12. bölümde bulunur. PostgreSQL sunucusunu sıfırdan paket deposuyla kurmak bu belgenin dışında; çalışan PostgreSQL üzerinden ilerliyoruz.

## 1. Örnek adresler ve üç farklı anahtar

| Örnek | Anlamı / değiştireceğiniz yer |
|---|---|
| `10.20.30.10` | Team sunucusunun örnek özel IP'si; kendi IP'nizi yazın. |
| `team.sirket.test` | Örnek HTTPS adı; şirket DNS'inizde sunucu IP'sine çözülmeli. Gerçek bir internet alan adı değildir. |
| `localhost:5432` | Aynı sunucudaki PostgreSQL; Docker host network kullandığı için Team container'ı da buraya erişir. |
| `termterm_team` | Uygulamaya ayrılan DB. |
| `termterm_team_migrator` | Yalnız kurulum/yükseltme hesabı, DB sahibi. |
| `termterm_team_service` | Servisin sınırlı çalışma hesabı; üyelerin hesabı değildir. |

**CA (Certificate Authority)**, sunucu sertifikasını imzalayan otoritedir. Public CA sertifikası karşı tarafa “bu imzayı güvenilir kabul et” demek için verilir. Özel anahtarı dağıtılmaz.

| Malzeme | Nerede kullanılır? | Kimlere verilir? |
|---|---|---|
| `postgres-ca.crt` | Team servisi ve migration aracının PostgreSQL sunucusunu doğrulaması | Sunucu yöneticisi ve Team servis config'i; masaüstünün Şirket CA alanına konmaz. |
| `team-https-ca.crt` | TermTerm masaüstünün HTTPS Team servisini doğrulaması | Team üyeleri; girişte **Şirket CA sertifikası** alanına bunun PEM içeriği yazılır. |
| Şirket kurtarma dosyası | Parola sıfırlama sonrası şifreli kimliklerin yetkili kurtarılması | Liderin uygulamada oluşturduğu, ayrı parola ile korunan dosya. TLS/CA sertifikası değildir. |

DB'ye **kullanıcı adı + parola / SCRAM** ile girilir. Sunucu sertifikası bağlantıyı şifreler ve sunucu kimliğini doğrular. Kullanıcıya/servise istemci özel sertifikası üretmeyeceğiz; `clientcert=...` veya `cert` HBA yöntemi kullanmayacağız.

## 2. Paketi hazırlayın, gerçek PostgreSQL yollarını öğrenin

[Hazır Docker paketi ve ilk indirme komutları](../README.md#team-kurulumu). Dış arşivi ve iç SHA256 listesini doğrulayın, `docker load` yapın. Bu rehberde servis arşivinin `team-service-linux-x64` içeriğinin **`/opt/termterm-team`** altında olduğunu varsayıyoruz. Burada `bin/`, `deploy/`, `migrations/` bulunmalı. Örneğin `deploy/manage.py` tam yolu `/opt/termterm-team/deploy/manage.py` olmalı; fazladan iç içe klasör oluşturmayın.

Aşağıdaki sunucu komutlarını yönetici hesabıyla çalıştırın; `sudo -i` ile açılan root shell içinde `sudo` yazmadan devam edebilirsiniz. Hata görürseniz sonraki adıma geçmeyin.

```bash
sudo -i
apt-get update
apt-get install -y openssl python3 postgresql-client ca-certificates curl nano
pg_lsclusters
sudo -u postgres psql -X -p 5432 -d postgres -c "SHOW config_file;"
sudo -u postgres psql -X -p 5432 -d postgres -c "SHOW hba_file;"
sudo -u postgres psql -X -p 5432 -d postgres -c "SHOW data_directory;"
```

18/main örneğinde beklenen yollar `/etc/postgresql/18/main/postgresql.conf`, `/etc/postgresql/18/main/pg_hba.conf`, `/var/lib/postgresql/18/main` olur. Çıktı başka ise o yolları kullanın. Önce config ve HBA dosyalarının tarihli kopyasını alın. Var olan sunucu başka uygulamalara da hizmet veriyorsa mevcut TLS sertifikasını ve dinleme adreslerini körlemesine değiştirmeyin; geçerli sunucu sertifikası/CA zaten varsa 3–4 yerine onun public CA'sını kullanın.

```bash
cp -a /etc/postgresql/18/main/postgresql.conf /etc/postgresql/18/main/postgresql.conf.before-termterm
cp -a /etc/postgresql/18/main/pg_hba.conf /etc/postgresql/18/main/pg_hba.conf.before-termterm
cp -a /var/lib/postgresql/18/main/postgresql.auto.conf /var/lib/postgresql/18/main/postgresql.auto.conf.before-termterm
```

Bu yedek isimleri ilk kurulum içindir; mevcut yedeğin üzerine yazmayın, tekrarında başka tarihli isim verin.

## 3. PostgreSQL için CA ve sunucu sertifikası üretin

Yeni/ayrılmış kurulum içindir. `/root/termterm-pki` zaten varsa mevcut anahtarları inceleyin; üzerine yeni CA üretmeyin. `postgres-ca.key` güçlü bir parola ile şifrelenir; OpenSSL istediğinde bu parolayı girin ve saklayın. Sunucu anahtarı servis otomatik başlayabilsin diye dosya izinleriyle korunur, interaktif parola istemez.

```bash
umask 077
mkdir /root/termterm-pki
cd /root/termterm-pki
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -aes-256-cbc -out postgres-ca.key
openssl req -x509 -new -sha256 -days 3650 -key postgres-ca.key -out postgres-ca.crt \
  -subj "/CN=Sirket PostgreSQL Root CA" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign"
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out postgres-server.key
openssl req -new -key postgres-server.key -out postgres-server.csr -subj "/CN=localhost"
cat > postgres-server.ext <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1
EOF
openssl x509 -req -sha256 -days 365 -in postgres-server.csr \
  -CA postgres-ca.crt -CAkey postgres-ca.key -CAcreateserial \
  -extfile postgres-server.ext -out postgres-server.crt
openssl verify -purpose sslserver -CAfile postgres-ca.crt postgres-server.crt
openssl x509 -in postgres-server.crt -noout -dates -ext subjectAltName
```

Beklenen: `postgres-server.crt: OK`; SAN alanında localhost ve loopback IP'leri görünür. `IP:::1`, OpenSSL'de `IP:` öneki + IPv6 `::1` değeridir. **Ayrı DB sunucusu kullanacaksanız imzalamadan önce** 12. bölümdeki gerçek DB DNS/IP değerlerini SAN listesine ekleyin. Yalnız CN yazmak yeterli değildir.

CA private key ve parolası yalnız yenileme/imzalama içindir; çevrimdışı güvenli yedeğe alın. Team container'ına CA private key'i veya DB sunucu private key'i verilmez.

## 4. PostgreSQL'e sertifikayı yerleştirin

```bash
install -d -m 0750 -o postgres -g postgres /etc/postgresql/18/main/termterm-tls
install -m 0600 -o postgres -g postgres /root/termterm-pki/postgres-server.key /etc/postgresql/18/main/termterm-tls/server.key
install -m 0644 -o postgres -g postgres /root/termterm-pki/postgres-server.crt /etc/postgresql/18/main/termterm-tls/server.crt
sudo -u postgres test -r /etc/postgresql/18/main/termterm-tls/server.key
```

Son komut hatasız bitmeli. `server.key` yalnız postgres tarafından okunur; dünya/grup okuması açık private key ile PostgreSQL TLS açılışı hata verebilir.

## 5. postgresql.conf ve postgresql.auto.conf farkı

`postgresql.conf` ana yapılandırmadır. `ALTER SYSTEM` değerleri veri dizinindeki **`postgresql.auto.conf`** dosyasına yazar; bu dosya ana config'deki aynı ayardan önceliklidir. İkisini aynı ayar için ayrı ayrı düzenlemeyin. Aşağıdaki yol **ALTER SYSTEM** kullanır; auto.conf'u elle yazmanız gerekmez. Komutları tek bir `psql -c` metnine sıkıştırmayın veya `BEGIN` içine koymayın.

```bash
sudo -u postgres psql -X -p 5432 -d postgres -v ON_ERROR_STOP=1 <<'SQL'
ALTER SYSTEM SET ssl = 'on';
ALTER SYSTEM SET ssl_cert_file = '/etc/postgresql/18/main/termterm-tls/server.crt';
ALTER SYSTEM SET ssl_key_file = '/etc/postgresql/18/main/termterm-tls/server.key';
ALTER SYSTEM SET ssl_min_protocol_version = 'TLSv1.2';
ALTER SYSTEM SET password_encryption = 'scram-sha-256';
SELECT pg_reload_conf();
SQL
```

Auto.conf içinde oluşan ilgili satırlar yaklaşık şöyledir; diğer mevcut ayarlar korunur:

```conf
ssl = 'on'
ssl_cert_file = '/etc/postgresql/18/main/termterm-tls/server.crt'
ssl_key_file = '/etc/postgresql/18/main/termterm-tls/server.key'
ssl_min_protocol_version = 'TLSv1.2'
password_encryption = 'scram-sha-256'
```

`ssl_ca_file` bu akışta zorunlu değildir: sunucunun **istemci sertifikasını** doğrulaması için kullanılır. Biz istemci sertifikası istemiyoruz; normal SCRAM parola kullanıyoruz. Public DB CA, PostgreSQL'e değil bağlantıyı açan Team servisine ve `psql` istemcisine verilir.

Aynı sunucu için PostgreSQL'in mevcut `listen_addresses='localhost'` ayarı yeterlidir. Yeni ayrılmış sunucuda bunu seçmek gerekirse:

```bash
sudo -u postgres psql -X -p 5432 -d postgres -c "ALTER SYSTEM SET listen_addresses = 'localhost';"
```

Dinleme adresi değişikliği restart ister. Bu örnek ayar başka uygulamaların uzaktan erişimini kapatır; ortak kullanılan DB'de mevcut gerekli adresleri koruyun. `password_encryption` mevcut parolaları dönüştürmez; eski MD5 hesap varsa DBA `\password HESAP_ADI` ile SCRAM parola yeniden kaydeder.

## 6. pg_hba.conf: yalnız bu DB için TLS + SCRAM

Gerçek `SHOW hba_file` yolunu düzenleyin. Dosyayı aşağıdaki parçayla tamamen değiştirmeyin. Yerel yönetim için mevcut `local ... peer` satırlarını koruyun; bu bloğu **geniş `host all all ...` izinlerinden önce** yerleştirin. HBA ilk eşleşen kuralı kullanır.

```conf
# Mevcut local/peer yönetim kuralları korunur.
# Team DB için şifrelenmemiş TCP bağlantısını reddet:
hostnossl termterm_team all                                         0.0.0.0/0      reject
hostnossl termterm_team all                                         ::/0           reject
# Aynı sunucudaki service ve migrator yalnız TLS ile:
hostssl   termterm_team termterm_team_service,termterm_team_migrator 127.0.0.1/32   scram-sha-256
hostssl   termterm_team termterm_team_service,termterm_team_migrator ::1/128        scram-sha-256
# Diğer TCP kaynaklarını/hesaplarını bu DB için kapat:
host      termterm_team all                                         0.0.0.0/0      reject
host      termterm_team all                                         ::/0           reject
```

DBA/yedekleme başka makineden geliyorsa onun **belirli hesabı ve IP'si** için ayrıca `hostssl` kuralını son reject satırlarından önce ekleyin. `0.0.0.0/0 scram-sha-256` gibi bütün interneti açan izin eklemeyin. Bu blok başka DB'lerin kurallarını değiştirmez.

```bash
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT line_number,error FROM pg_hba_file_rules WHERE error IS NOT NULL;"
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT sourcefile,sourceline,name,error FROM pg_file_settings WHERE error IS NOT NULL;"
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT pg_reload_conf();"
```

İlk iki sorguda sıfır satır beklenir. Reload cevabı `t` sadece sinyalin gönderildiğini gösterir; sonraki kontrolü atlamayın. Dinleme adresi değiştiyse, doğru kümede kısa bakım kesintisi planlayıp:

```bash
pg_ctlcluster 18 main restart
pg_lsclusters
sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT name,setting,source,pending_restart FROM pg_settings WHERE name IN ('ssl','ssl_cert_file','ssl_key_file','ssl_min_protocol_version','password_encryption','listen_addresses');"
```

`ssl=on`, beklenen dosya yolları ve `pending_restart=f` görünmeli. Hata olursa `journalctl -u postgresql@18-main --no-pager -n 50` ve `/var/log/postgresql/postgresql-18-main.log` dosyasını inceleyin. Sertifika sorununun çözümü `ssl=off` değildir.

## 7. DB ve iki hesabı oluşturun

Şirketinizin `owner/app/developer/readonly/appuser` düzenini kullanmak için [alternatif DB şablonunu](TEAM_DB_OWNER_TEMPLATE_TR.md) izleyin. O yolda SQL dosyası, HBA hesap adları, migration kullanıcısı ve service.json birlikte değişir; aşağıdaki migrator/service kurulumunu ayrıca çalıştırmayın.

**Yalnız yeni boş kurulumda** (mevcut DB/rol varsa tekrar çalıştırmayın):

```bash
sudo -u postgres psql -X -p 5432 -d postgres -f /opt/termterm-team/deploy/prepare-database.sql
```

Script migrator ve service parolalarını ayrı ayrı iki kez sorar. Güçlü, farklı parolalar kullanın; örnekte gerçek parola yayımlamıyoruz. Scriptin varsayılanları DB=`termterm_team`, owner=`termterm_team_migrator`, service=`termterm_team_service`. Tablolar henüz oluşmaz. `pgcrypto` yok hatasında DBA, PostgreSQL sürümünüzün contrib/pgcrypto paketini kurar; migration hesabına superuser verilmez.

## 8. Team'in özel DB config'ini yazın

Root shell'de, yalnız ilk kurulumda dosyaları oluşturun. Mevcut dosyaları boşaltmayın:

```bash
install -d -m 0700 /etc/termterm-team-private
install -d -m 0750 -o root -g 10001 /etc/termterm-team-private/config
install -m 0644 /root/termterm-pki/postgres-ca.crt /etc/termterm-team-private/config/postgres-ca.crt
install -m 0640 -o root -g 10001 /opt/termterm-team/deploy/service.example.json /etc/termterm-team-private/config/service.json
nano /etc/termterm-team-private/config/service.json
```

İçerik aşağıdaki gibi olmalı. `SERVICE_PAROLASINI_BURAYA_YAZIN` yerine **7. adımda service hesabına verdiğiniz parolayı** yazın. Bu örnek parola değildir. Parolada çift tırnak veya ters eğik çizgi varsa JSON kaçışlarını kullanın (`\"`, `\\`).

```json
{
  "listen": "127.0.0.1:41841",
  "database": {
    "host": "localhost",
    "port": 5432,
    "database": "termterm_team",
    "username": "termterm_team_service",
    "password": "SERVICE_PAROLASINI_BURAYA_YAZIN",
    "caPath": "/etc/termterm-team/postgres-ca.crt"
  }
}
```

`caPath` **container içindeki yoldur**; hosttaki `/etc/termterm-team-private/config` dizini container'da `/etc/termterm-team` olarak bağlanır. Kullanıcı/parola kaydedilir ve her açılışta bu özel dosyadan okunur. Dosyayı üyelere, GitHub'a veya loglara koymayın.

Migration için root'a özel dosya:

```bash
umask 077
touch /etc/termterm-team-private/migration.pgpass
chmod 0600 /etc/termterm-team-private/migration.pgpass
nano /etc/termterm-team-private/migration.pgpass
```

Tek satır; son alanı **migrator parolasıyla** değiştirin:

```text
localhost:5432:termterm_team:termterm_team_migrator:MIGRATOR_PAROLASINI_BURAYA_YAZIN
```

Paroladaki `:` karakteri `\:`, `\` karakteri `\\` olarak yazılır. Dosya 0600 değilse libpq parolayı kullanmayabilir. `host` ve port, sonraki export değerleriyle aynı olmalıdır.

## 9. TLS/parola bağlantısını sınayın ve migration yapın

```bash
export PGHOST=localhost PGPORT=5432 PGDATABASE=termterm_team PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-team-private/config/postgres-ca.crt
export PGPASSFILE=/etc/termterm-team-private/migration.pgpass
export PGSSLMODE=verify-full
psql -X -v ON_ERROR_STOP=1 -c "SELECT current_user,current_database(),ssl,version FROM pg_stat_ssl WHERE pid=pg_backend_pid();"
openssl s_client -starttls postgres -connect localhost:5432 -servername localhost \
  -verify_hostname localhost -CAfile "$PGSSLROOTCERT" -verify_return_error </dev/null
```

Beklenen: kullanıcı `termterm_team_migrator`, DB `termterm_team`, `ssl=t`, TLSv1.2/1.3. OpenSSL sonunda `Verify return code: 0 (ok)` görülmeli; yalnız bu test DB parolasını doğrulamaz, ilk `psql` sorgusu ikisini birlikte doğrular.

```bash
# Bu bağlantı BİLEREK başarısız olmalıdır: HBA şifrelenmemiş erişimi kapattı.
PGSSLMODE=disable psql -X -c 'SELECT 1;'
# Yeniden normal, doğrulanan TLS ile ilk şema kurulumu:
cd /opt/termterm-team
python3 deploy/manage.py migrate --service-role termterm_team_service
```

0.4.1'de 64 migration doğrulanır. Servis normalde tablo/şema oluşturmaz. `manage.py migrate` ilk kurulum veya aynı şema doğrulamasıdır; ileri sürüm için [yedekli upgrade akışı](TEAM_UPGRADES_TR.md) gerekir.

## 10. Şirket HTTPS CA'sını üretin: Caddy bunu otomatik yapar

Burada DB CA'sını tekrar kullanmıyoruz. Paket içindeki **`tls internal`** ayarı Caddy'ye ayrı bir HTTPS CA üretmesini ve Team DNS adı için sertifika vermesini söyler. Caddy sertifikaları yeniler; kendi CA verisi kalıcı volume'de korunur. `docker compose down -v` ile bu volume'ü silmeyin.

Önce istemcilerin ve sunucunun `team.sirket.test` adını `10.20.30.10` IP'sine çözmesini sağlayın. Tercih şirket DNS kaydıdır. Sadece test için Linux `/etc/hosts`, Windows yönetici olarak `%SystemRoot%\System32\drivers\etc\hosts` dosyasına `10.20.30.10 team.sirket.test` satırı eklenebilir. Gerçek adres/adlarınızla değiştirin. Bu alan adı için internette public sertifika alınmaz; bu örnek internal CA kullanır.

Root'a özel `/etc/termterm-team-private/deployment.env` dosyasını oluşturun (0600). İçerik:

```dotenv
TERMTERM_CONFIG_DIR=/etc/termterm-team-private/config
TERMTERM_DOMAIN=team.sirket.test
TERMTERM_SERVICE_IMAGE=termterm/team-service:0.4.1
TERMTERM_CADDY_FILE=/opt/termterm-team/deploy/Caddyfile.internal
TERMTERM_TEAM_CPUS=1.0
TERMTERM_TEAM_MEMORY=512m
TERMTERM_HTTPS_CPUS=0.5
TERMTERM_HTTPS_MEMORY=128m
```

Paketin `Caddyfile.internal` temel içeriği şöyledir (hazır dosyadaki header/request sınırlarını koruyun):

```caddyfile
{
    admin localhost:2019
}
{$TERMTERM_DOMAIN} {
    tls internal
    reverse_proxy 127.0.0.1:41841
    header {
        Strict-Transport-Security "max-age=31536000"
        X-Content-Type-Options "nosniff"
        Cache-Control "no-store"
    }
    request_body {
        max_size 256KB
    }
}
```

Host firewall'unda üyelerin ağından HTTPS 443 erişimi gerekir; internal CA için internetten 80 açmanız gerekmez. Caddy HTTP 80'de yönlendirme dinleyebilir. API 41841 ve Caddy yönetim 2019 portlarını dışarıya açmayın. Önceden 80/443 kullanan başka servis varsa port çakışmasını çözün.

```bash
cd /opt/termterm-team
systemctl enable --now docker
docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f deploy/compose.yaml up -d --no-build team https
docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f deploy/compose.yaml ps
install -d -m 0755 /opt/termterm-team-public
docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f deploy/compose.yaml cp https:/data/caddy/pki/authorities/local/root.crt /opt/termterm-team-public/team-https-ca.crt
chmod 0644 /opt/termterm-team-public/team-https-ca.crt
openssl x509 -in /opt/termterm-team-public/team-https-ca.crt -noout -subject -dates -fingerprint -sha256
curl --fail --cacert /opt/termterm-team-public/team-https-ca.crt https://team.sirket.test/ready
```

CA dosyası henüz yoksa Caddy başlamamıştır; `compose logs --tail 50 https` ile nedeni inceleyin. `/ready` HTTP 200 ve şema 64 / API 1 döndürmeli. DNS henüz kurulmamışsa **yalnız tanılama için**, gerçek IP'nizle:

```bash
curl --fail --noproxy '*' --resolve team.sirket.test:443:10.20.30.10 \
  --cacert /opt/termterm-team-public/team-https-ca.crt https://team.sirket.test/ready
```

Bu komut sertifika kontrolünü kapatmaz; geçici DNS eşlemesi yapar. `curl -k` kullanmayın. Company CA, container dışındaki istemcilere kendiliğinden güvenilir hale gelmez.

## 11. TermTerm'deki “Şirket CA sertifikası” alanına ne yazacağım?

Sunucudan **yalnız** `/opt/termterm-team-public/team-https-ca.crt` dosyasını güvenilir yolla kullanıcılara ulaştırın. SHA256 parmak izini ayrı güvenilir kanaldan yöneticinin değeriyle karşılaştırın. Dosyanın içeriği şöyledir; aşağıdaki örnek gerçek sertifika değildir:

```text
-----BEGIN CERTIFICATE-----
...team-https-ca.crt dosyasının gerçek Base64 satırları...
-----END CERTIFICATE-----
```

TermTerm → **Team hesabı**:

1. Servis adresi: `https://team.sirket.test` (kendi DNS adınız).
2. Takım kimliği: bootstrap dosyasındaki UUID.
3. Kullanıcı adı/parola: lider hesabı veya liderin oluşturduğu Team hesabı; PostgreSQL kullanıcısı değil.
4. **Şirket CA sertifikası (isteğe bağlı)** bölümünü açın. `team-https-ca.crt` dosyasının **tam metnini**, BEGIN/END satırları dahil yapıştırın. Dosya yolu yazılmaz.
5. **Beni hatırla** ile bu cihazda bilgilerin korunmasını seçebilirsiniz.

`postgres-ca.crt`, `server.crt`, herhangi bir `.key` dosyası veya şirket kurtarma dosyası bu alana konmaz. Caddy public, işletim sistemince güvenilen bir HTTPS sertifikası kullanıyorsa alan boş kalabilir. Başka bir şirket PKI'sı zaten varsa onun yöneticisinden sunucu sertifikasının uygun public CA zincirini alın; bu belgedeki yeni Caddy CA'yı zorunlu olarak üretmeniz gerekmez.

İlk lider henüz oluşturulmadıysa 9. adımdaki migration ortamı ile:

```bash
cd /opt/termterm-team
python3 deploy/manage.py bootstrap --team "Şirket takımı" --username leader \
  --display-name "Takım lideri" --output /etc/termterm-team-private/ilk-lider.json
```

Yeni dosyada takım kimliği ve 24 saat geçerli geçici parola olur. İlk girişte parola değiştirilir. Sonra lider **Team → Üyeler** üzerinden kullanıcı ekleyip kaynak izinlerini verir; şirket kurtarma dosyasını uygulamada ayrıca oluşturur. [Kullanıcı/yetki/kurtarma adımları](TEAM_041_GUIDE_TR.md).

## 12. PostgreSQL ayrı sunucudaysa değişecek yerler

Örnek: Team=`10.20.30.10`, DB=`10.20.30.20`, DB DNS=`db.sirket.test`. Bunlar gerçek adreslerinizle değiştirilir.

- 3. adımda DB sunucu sertifikasının SAN satırı: `subjectAltName=DNS:db.sirket.test,IP:10.20.30.20,DNS:localhost,IP:127.0.0.1,IP:::1`. Sertifikayı bu dosyayla imzalayın; mevcut sertifikaya sonradan `.ext` düzenlemek onu değiştirmez.
- DB'de `listen_addresses='localhost,10.20.30.20'` ayarlayıp doğru kümeyi restart edin. Firewall 5432'yi yalnız Team sunucusundan ve gerekiyorsa belirli yönetim adresinden kabul etsin.
- HBA'nın son reject satırlarından **önce**: `hostssl termterm_team termterm_team_service,termterm_team_migrator 10.20.30.10/32 scram-sha-256`. Ağda NAT varsa DB'nin gerçekten gördüğü kaynak IP'yi kullanın.
- `postgres-ca.crt` public dosyasını Team sunucusuna kopyalayın. DB server.key ve CA private key'i kopyalamayın.
- `service.json` host=`db.sirket.test`, port=`5432`; pgpass ilk alanı ve `PGHOST` da aynı DNS adı olmalı. Aynı CA ve `verify-full` kullanılır.
- DB hazırlama/TLS/HBA adımları DB sunucusunda; image/Compose/HTTPS CA adımları Team sunucusunda yapılır. Migration komutu, CA/pgpass ve DB ağ erişimi olan yönetim makinesinde çalışır.

## 13. Hızlı hata tablosu, yenileme ve yedek

| Hata | Kontrol |
|---|---|
| `connection refused` / timeout | Doğru IP/port, DB dinleme adresi, firewall, DNS ve servis durumu. |
| `no pg_hba.conf entry` veya `pg_hba.conf rejects connection` | HBA sırası, DB/kullanıcı adı ve kaynak IP; TLS'siz testte ret beklenir. |
| `password authentication failed` | Doğru service/migrator parolası, pgpass kaçışları/0600, mevcut hesapta SCRAM parola kaydı. |
| `certificate verify failed` / issuer hatası | Doğru DB veya HTTPS CA; iki CA karıştırılmış olabilir. |
| Hostname mismatch | Bağlandığınız DNS/IP sertifikanın SAN alanında bulunmalı. Sertifikayı doğru adla yeniden imzalayın. |
| `Database CA unavailable` | `service.json` container CA yolu, volume mount ve UID 10001 okuma izinleri. |
| `/health` 200 ama `/ready` 503 | Süreç canlı; DB/TLS/şema hazır değil. Migration ve DB bağlantısını inceleyin. |
| 401/403 | Team hesabı/oturum/izin sorunu; TLS'yi kapatarak çözülmez. |

DB sunucu sertifikası örnekte **365 gün** geçerlidir. Bitişten önce aynı CA ile yeni sunucu sertifikası üretip doğrulayın, sonra dosyaları izinleri korunarak değiştirip PostgreSQL reload yapın ve `verify-full` testini tekrarlayın. Aynı CA kalırsa istemcinin CA dosyası değişmez; CA değişirse geçiş ayrıca planlanır. Caddy kendi HTTPS leaf sertifikasını otomatik yeniler; CA volume'ü, private config, DB yedeği ve uygulamadaki şirket kurtarma dosyası ayrı güvenli yedeklere alınmalıdır. PostgreSQL config yedeği **DB veri yedeğinin yerine geçmez**.

## 14. Bu rehber nasıl doğrulandı?

12 Ekim 2026'da `tests/team_v2_tls_guide.py` ile WSL'de ayrı, geçici PostgreSQL 18 kümesi kuruldu; mevcut `main` ve Team test kümeleri değiştirilmedi. Belgedeki CA/OpenSSL komutları çalıştırıldı; yalnız test otomasyonu için CA parolası gizli dosyadan verildi. Anahtar izinleri, auto.conf/HBA sözdizimi, restart, TLS/SCRAM girişi, 64 migration ve ilk lider oluşturma geçti. TLS'siz bağlantı, yanlış hostname ve iki CA'nın yer değiştirmesi reddedildi. Gerçek Debian 13 Team container'ı ve Caddy ile HTTPS CA dışa aktarımı ve `/ready` doğrulandı; test servisleri kapatıldı.

Fiziksel üretim sunucunuz, gerçek DNS/firewall ve sizin IP'leriniz test edilmedi. Yukarıdaki adresler örnektir; kendi ortamınızda her kontrolün beklenen sonucunu görmeden sonraki aşamaya geçmeyin. Eski 0.4.1 Docker arşivi değiştirilmedi; bu genişletilmiş rehber GitHub'da ayrıca sunulur.

Kaynaklar: [PostgreSQL 18 TLS ve anahtar izinleri](https://www.postgresql.org/docs/18/ssl-tcp.html), [HBA sırası](https://www.postgresql.org/docs/18/auth-pg-hba-conf.html), [ALTER SYSTEM / auto.conf](https://www.postgresql.org/docs/18/sql-altersystem.html), [Caddy internal HTTPS](https://caddyserver.com/docs/automatic-https), [Docker'dan Caddy CA çıkarma](https://caddyserver.com/docs/running#docker-compose).
