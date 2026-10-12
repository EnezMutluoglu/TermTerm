# TermTerm Team Service 0.4.2 — kurulum kılavuzu

Bu paket **0.4.2** sürümü içindir. Doğrulama ve ertelenen ortamlar `VALIDATION_0.4.2.md` içinde açıklanır. Masaüstü Windows/Linux programıdır; aşağıdaki servis tarayıcı arayüzü sunmaz.

**Aynı sunucu ve alan adı olmayan kurulum:** [DB parolası/sertifikası olmadan Unix soketi; IP ile yerel HTTPS ve çalışanlara public sertifika dağıtımı](TEAM_SOCKET_KURULUM_TR.md). Aşağıdaki TLS/DB parolalı yollar da desteklenmeye devam eder.

## Gereksinimler ve ayrım

PostgreSQL sertifikası, `postgresql.conf` / `postgresql.auto.conf`, HBA ve TermTerm'deki **Şirket CA sertifikası** alanı için [ayrıntılı TLS/CA kurulum rehberini](TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md) izleyin. Aynı sunucu ve ayrı DB sunucusu örnekleri, değiştirilecek değerler ve bağlantı kontrolleri bulunur.

Şirket rol şablonu için [owner/app/developer/readonly/appuser ile DB oluşturma ve yapılandırma](TEAM_DB_OWNER_TEMPLATE_TR.md) alternatifini kullanabilirsiniz. Aynı servis imajıyla çalışır; sadece yeni DB hazırlığı ve hesap isimleri değişir. İki ilk kurulum SQL'ini aynı DB'de çalıştırmayın.

Linux x64, Docker Engine + Compose eklentisi veya systemd; yönetim bilgisayarında Python 3, OpenSSL ve PostgreSQL `psql` gerekir. Masaüstü doğrudan PostgreSQL'e bağlanmaz. SSH/SFTP masaüstünden hedefe gider. Servis sınırlı DB rolü kullanır; aynı sunucuda Unix soketi, uzak DB'de doğrulamalı TLS seçilir. DB parolası kullanılan yollarda parola yalnız sunucuda kalır.

- İstemciler → HTTPS 443 (DNS adına veya IP'ye uygun sertifika).
- Caddy → yalnız `127.0.0.1:41841` Team API.
- Ayrı PostgreSQL seçeneği → yalnız `127.0.0.1:55433`, TLS/SCRAM.
- Migration hesabı ayrı; normal servis rolü tablo veya şema sahibi olamaz, doğrudan tablo yetkileri alamaz.

Arşivi ayrı bir dizine çıkarın. `bin/termterm-team-service`, `migrations/` ve `deploy/` birlikte kalmalıdır. SHA256 dosyasını doğrulayın. Özel yapılandırmaları paket dizininin dışında tutun; bunları Git'e eklemeyin.

## Hangi adımı kim yapar?

**Elle kurduğunuz Debian/Linux PostgreSQL uygundur; B bölümünü izleyin.** DBA veritabanını ve iki PostgreSQL hesabını hazırlar. `deploy/manage.py migrate` ilk kurulumda `termterm_team_v2` şemasını, tabloları, indeksleri ve API fonksiyon izinlerini birlikte oluşturur. Tabloları tek tek oluşturmayın, migration SQL dosyalarını elle sırayla çalıştırmayın. Masaüstü ve normal Team servisi otomatik migration yapmaz. Mevcut veritabanını yeni sürüme geçirmek için yedekli `upgrade.py` akışı kullanılır.

PostgreSQL hesapları yalnız sunucudadır: **migrator** kurulumu/yükseltmeyi yapar; **service** sınırlı API fonksiyonlarını çağırır. Team üyeleri bunlardan ayrıdır; lider masaüstündeki Üyeler alanından normal uygulama hesabı oluşturur. Her üyeye PostgreSQL kullanıcısı açılmaz.

## Hazır Docker image — sunucuda derleme yok

0.4.2 [Release sayfasından](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.4.2) Team Service arşivini, Team Image arşivini, image kimlik JSON'unu ve SHA256SUMS.txt dosyasını indirin. Image `docker save` biçimindedir; `docker import` kullanılmaz. Servis arşivi config/migration/kılavuzları, image arşivi hazır Debian 13 çalışma ortamını içerir. İkisini de indirin.

```bash
# İndirilen dosyaların olduğu dizin; örnek sürüm 0.4.2.
sha256sum --check --ignore-missing SHA256SUMS.txt
sudo docker load --input TermTerm-Team-Image-0.4.2-linux-x64.tar.gz
tar -xzf TermTerm-Team-Service-0.4.2-linux-x64.tar.gz
cd team-service-linux-x64
sudo docker image inspect termterm/team-service:0.4.2 --format '{{.Id}}'
```

Image ID, `.json` içindeki `imageId` ile aynı olmalıdır. Özel `deployment.env` içindeki `TERMTERM_SERVICE_IMAGE` indirilen sürüme sabitlenir. `latest` kullanılmaz. Aşağıdaki `--no-build` komutları image yoksa hata verir; sunucuda beklenmedik derleme veya farklı image indirmez. HTTPS katmanı ilk kullanımda sabit digest ile Caddy image'ını indirir. Rust/Node veya kaynak derleme araçları gerekmez; ilk migration için Python 3 ve `psql` gerekir.

Linux x64 image, **Debian 13 Trixie slim + OpenSSL 3 (`libssl3t64`)** kullanır; root yerine UID/GID 10001 ile çalışır. Alpine/musl image sunulmaz: mevcut doğrulanmış executable glibc/OpenSSL ile bağlıdır. Taban image digest ile sabittir; güvenlik yenilemesinde digest/paketler yeniden doğrulanıp yeni sürümlü image çıkarılır. Çalışan container içinde paket güncellemesi yapılmaz. Debian 13'ün tam desteği 9 Ağustos 2028, LTS dönemi 30 Haziran 2030'a kadardır ([Debian sürüm bilgisi](https://www.debian.org/releases/trixie/)). Host Linux ile container dağıtımı aynı olmak zorunda değildir.

## A — Ayrı PostgreSQL ile Compose

Aşağıdaki ilk komut yeni dizin ister; var olan dizini/parolaları üzerine yazmaz. PostgreSQL için yerel CA ve 825 günlük sunucu sertifikası, ayrı migration ve servis parolaları oluşturur. İlk şema/hesap henüz oluşturulmaz.

```bash
sudo python3 deploy/prepare-compose.py --directory /etc/termterm-team-private --port 55433
```

`/etc/termterm-team-private/deployment.env` içindeki `TERMTERM_DOMAIN` değerini şirket DNS adıyla değiştirin. Gerçek DNS ve 80/443 erişimi varsa varsayılan `Caddyfile` sertifikayı alır. İç şirket CA'sı için `TERMTERM_CADDY_FILE` değerini paket içindeki `deploy/Caddyfile.internal` dosyasının **mutlak yolu** yapın. Sertifika doğrulamasını kapatmayın.

```bash
cd deploy
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f compose.yaml -f compose.postgres.yaml up -d --build postgres
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f compose.yaml -f compose.postgres.yaml ps
cd ..
```

DB sağlıklı olduktan sonra açık migration adımı:

```bash
export PGHOST=localhost PGPORT=55433 PGDATABASE=termterm_team PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-team-private/config/postgres-ca.crt
export PGPASSFILE=/etc/termterm-team-private/migration.pgpass
python3 deploy/manage.py migrate --service-role termterm_team_service
```

Komutu özel dosyalara okuyabilecek yönetim hesabıyla çalıştırın. Araç TLS `verify-full` kullanır, migration'ları tek oturum kilidi altında sıralar, uygulanmış dosyaların SHA256 değerini denetler ve yalnız açık servis fonksiyon izinlerini verir. Parola komut satırına koyulmaz. Önceden bu araç olmadan uygulanmış şemada ilk çalıştırma mevcut dosya hash'lerini başlangıç kaydı olarak alır; eski uygulama tarihindeki dosyaları geriye dönük doğruladığı iddia edilmez. Yeni sürümde eski migration dosyasını değiştirmek yerine yeni dosya eklenmelidir.

```bash
cd deploy
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f compose.yaml -f compose.postgres.yaml up -d --no-build team https
```

CA özel anahtarı yalnız sertifika yenileme içindir; çevrimdışı, erişimi kısıtlı yedekleyin. Caddy şirket CA'sı ile PostgreSQL CA'sı farklıdır: masaüstüne PostgreSQL CA'sı değil **HTTPS CA'sının public sertifikası** verilir.

## B — Mevcut TLS PostgreSQL ile Compose

### 1. PostgreSQL'i hazırlayın

PostgreSQL 18 üzerinde doğrulandı. Debian'da `postgresql-contrib`/sürümünüzün pgcrypto paketi, servis yönetimi yapılan makinede `postgresql-client`, `python3`, `openssl` bulunmalıdır. PostgreSQL TLS sertifikasında `service.json` içindeki DB DNS adı (aynı sunucuda `localhost` kullanılacaksa localhost SAN) bulunmalıdır. `ssl=on`, sunucu sertifikası/anahtarı ve `password_encryption=scram-sha-256` ayarlarını DBA uygular; doğrulama kapatılmaz.

**Yalnız boş ilk kurulumda**, PostgreSQL sunucusunda DBA hesabıyla:

```bash
sudo -u postgres psql -X -d postgres -f /opt/termterm-team/deploy/prepare-database.sql
```

Paket yolunu kendi konumunuza göre değiştirin; `postgres` kullanıcısı SQL dosyasını okuyabilmelidir. Uzak DBA bağlantısında aynı SQL, TLS `verify-full` kullanan yetkili `psql` ile çalıştırılır. SQL iki yeni rol oluşturur ve her birinin parolasını gizli istemde iki kez sorar; bu parolaları ilgili özel config/pgpass dosyasına kaydedin. Veritabanını zaten açtıysanız scripti olduğu gibi tekrar çalıştırmayın: içindeki rol/owner/grant/pgcrypto adımlarını mevcut adlara göre DBA ile uygulayın. Script mevcut rol/DB'yi silmez veya sıfırlamaz; ad çakışmasında durur. Kesilen kurulumda hangi adımların tamamlandığını incelemeden tekrarlamayın.

İki rol de `NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOINHERIT` olur; servis migrator üyesi değildir. Veritabanı sahibi migrator'dır. PUBLIC veritabanı erişimi ve `public` şemasındaki CREATE kaldırılır. DBA `pgcrypto` kurar. Servis yalnız CONNECT alır; `migrate` gereken fonksiyon izinlerini sonra verir.

`pg_hba.conf` için **daha geniş izinlerden önce**, aynı Linux sunucusunda örnek:

```text
hostnossl termterm_team all                                        0.0.0.0/0 reject
hostnossl termterm_team all                                        ::/0      reject
hostssl   termterm_team termterm_team_service,termterm_team_migrator 127.0.0.1/32 scram-sha-256
hostssl   termterm_team termterm_team_service,termterm_team_migrator ::1/128      scram-sha-256
host      termterm_team all                                        0.0.0.0/0 reject
host      termterm_team all                                        ::/0      reject
```

Uzak servis sunucusu için yalnız onun sabit IP/CIDR'ını son reject satırlarından önce ekleyin; ayrı yönetim/yedek IP'si gerekiyorsa açıkça ekleyin. PostgreSQL dinleme adresi ve firewall aynı erişim sınırını taşımalıdır. Ayarları reload edin; sertifika/dinleme değişikliği için PostgreSQL'in gerektirdiği restart adımını uygulayın.

### 2. Özel dosyaları hazırlayın

```bash
sudo install -d -m 0700 /etc/termterm-team-private
sudo install -d -m 0750 -o root -g 10001 /etc/termterm-team-private/config
sudo install -m 0640 -o root -g 10001 deploy/service.example.json /etc/termterm-team-private/config/service.json
sudo install -m 0644 /guvenli-konum/postgres-ca.crt /etc/termterm-team-private/config/postgres-ca.crt
sudo install -m 0600 deploy/deployment.env.example /etc/termterm-team-private/deployment.env
sudoedit /etc/termterm-team-private/config/service.json
sudoedit /etc/termterm-team-private/deployment.env
sudo install -m 0600 /dev/null /etc/termterm-team-private/migration.pgpass
sudoedit /etc/termterm-team-private/migration.pgpass
```

Bu dosya oluşturma komutları ilk kurulum içindir; var olan parolaların üzerine uygulamayın. `service.json` DB host/port/database/username/password alanlarını doldurun. `listen` **127.0.0.1:41841** kalır. Container içindeki `caPath` `/etc/termterm-team/postgres-ca.crt` olmalıdır. `deployment.env` içinde HTTPS DNS adını ve image sürümünü ayarlayın; parola koymayın.

`migration.pgpass` tek satır biçimi: `postgres.company.example:5432:termterm_team:termterm_team_migrator:PAROLA`. Parola içindeki `:` ve `\` karakterlerini libpq pgpass kurallarına göre kaçışlayın. JSON parolalarında da JSON kaçışları geçerlidir. Servis parolası yalnız `service.json` içinde bulunur; iki hesap için farklı parola kullanın.

### 3. Şema ve tabloları oluşturun

Özel dosyaları okuyabilen yönetim shell'inde, paket kökünden:

```bash
export PGHOST=postgres.company.example PGPORT=5432 PGDATABASE=termterm_team PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-team-private/config/postgres-ca.crt
export PGPASSFILE=/etc/termterm-team-private/migration.pgpass
python3 deploy/manage.py migrate --service-role termterm_team_service
```

0.4.1 için **64 migration doğrulandı** mesajı beklenir. Tabloları uygulama girişinde oluşturmaya çalışmaz; başarısız migration düzeltilmeden servis hazır olmaz. Aynı sürümde tekrar çalıştırma checksum/izin doğrulamasıdır. Yeni şema sürümüne yükseltmede `upgrade.py` gerekir.

### 4. Hazır image ile servisi başlatın

```bash
sudo systemctl enable --now docker
cd deploy
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f compose.yaml up -d --no-build team https
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f compose.yaml ps
curl --fail https://team.company.example/ready
```

Şirket CA'sında curl için `--cacert /guvenli-konum/https-ca.crt` ekleyin. **compose.postgres.yaml eklenmez**: mevcut PostgreSQL ve verisi sizin yönetiminizde kalır. İlk lideri aşağıdaki komutla oluşturun.

## Kaynak, ağ, disk ve yeniden başlatma

| Container | CPU üst sınırı | RAM / swap | Süreç sınırı | Kalıcı yazma |
|---|---:|---:|---:|---|
| Team | 1 çekirdek | 512 MiB / kapalı | 128 | Yok; /tmp RAM alanı 16 MiB |
| HTTPS/Caddy | 0,5 çekirdek | 128 MiB / kapalı | 128 | Sertifikalar için caddy_data/config |
| İsteğe bağlı PostgreSQL | 2 çekirdek | 1 GiB / kapalı | 256 | postgres_data volume |

Bunlar kapasite garantisi değil başlangıç üst sınırlarıdır. `deployment.env.example` içindeki CPU/RAM değerleriyle değiştirilebilir; değişiklikten sonra `compose up -d --no-build` container'ı yeniden oluşturur. Ana makinede işletim sistemi ve elle kurduğunuz PostgreSQL için ayrıca RAM bırakın. PostgreSQL container sınırları harici PostgreSQL'e uygulanmaz. systemd alternatifi Team için aynı 1 CPU/512 MiB/128 süreç sınırına sahiptir.

- Loglar container başına **10 MiB × 3** döndürülür. Team kök dosya sistemi salt okunurdur. Image'lar, Caddy volume'leri, yedekler ve DB diski bu log sınırına dahil değildir. Volume tanımlamak disk kotası koymaz; kesin kota için ayrı disk/LVM veya dosya sistemi kotası ve boş alan alarmı kullanın. Dolu diskte DB yazamaz; yedekleri aynı dolan diske yığmayın.
- Team ve Caddy Linux **host network** kullanır. API yalnız localhost'tadır; dışarıya HTTPS 443, sertifika yöntemine göre HTTP 80 açılır. Docker `ports` NAT eşlemesi kullanılmaz. Dış DB erişimini host firewall'u/DB ACL'i sınırlar. Compose burada bant genişliği kotası vermez; gerekiyorsa Linux ağ QoS'u uygulanır.
- `restart: unless-stopped`, süreç çökerse veya Docker/host yeniden başlarsa otomatik açar. Elle `compose stop/down` ile durdurulan servis kendiliğinden açılmaz. DB bağlantısı kesilirse DB'ye tekrar erişim sonraki isteklerde denenir; şema otomatik değiştirilmez.
- Healthcheck sürecin `/health` adresini kontrol eder. **Unhealthy tek başına Docker restart tetiklemez.** DB/TLS/şema için HTTPS `/ready` ayrıca izlenmelidir; hatalı sertifika veya DB kesintisini restart döngüsüyle gizlemeyin. `docker stats`, `compose logs --tail 100`, `df -h` ve `docker system df` ile durumu izleyin. Rastgele `docker system prune --volumes` çalıştırmayın.

Docker davranış kaynakları: [Compose servis ayarları](https://docs.docker.com/reference/compose-file/services/), [restart politikaları](https://docs.docker.com/engine/containers/start-containers-automatically/), [image yükleme](https://docs.docker.com/reference/cli/docker/image/load/).

## C — systemd alternatifi

Linux paketindeki executable OpenSSL 3 ve glibc tabanlı Linux x64 içindir. Sisteminizde CA/OpenSSL çalışma zamanı bulunmalıdır. Ayrı `termterm-team` sistem kullanıcısı oluşturun. Executable'ı `/opt/termterm-team/termterm-team-service` konumuna 0755, özel `service.json` dosyasını `/etc/termterm-team/` altına root:termterm-team 0640 olarak yerleştirin. Dizin 0750 olmalıdır. `caPath` alanını gerçek public CA dosyasına ayarlayın.

`deploy/termterm-team.service` dosyasını `/etc/systemd/system/` altına kurun:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now termterm-team
sudo systemctl status termterm-team
curl --fail http://127.0.0.1:41841/ready
```

Caddy'yi ayrıca sistem servisi veya Compose HTTPS servisi olarak çalıştırın. Aynı API portunda hem Compose Team hem systemd Team başlatmayın. `/health` yalnız sürecin canlılığını, `/ready` gerçek sınırlı DB bağlantısını ve şema sürümünü gösterir.

## İlk lider ve istemci dağıtımı

Migration ortamı ayarlarıyla bir defa:

```bash
python3 deploy/manage.py bootstrap --team "Şirket takımı" --username leader \
  --display-name "Takım lideri" --output /guvenli-dizin/ilk-lider.json
```

Yeni dosyada takım kimliği ve 24 saatlik, 21 karakterli geçici parola bulunur. Araç parolayı loglamaz ve var olan çıktı dosyasını üzerine yazmaz. Ağ/disk hatasında tekrar yeni takım yaratmadan önce DB sonucunu inceleyin. Lider ilk girişte parolayı değiştirir. Kullanıcılara yalnız HTTPS servis adresi, takım kimliği, gerekiyorsa public HTTPS CA ve liderin oluşturduğu kişisel hesap verilir. İlk giriş ve kurtarma dosyası adımları `TEAM_041_GUIDE_TR.md` içindedir.

## Yedek, güncelleme ve hata giderme

- DB yedeğini ayrı DBA/backup hesabıyla `pg_dump -Fc` kullanarak alın; servis hesabına yedek için tablo erişimi vermeyin. Parolayı argv'ye yazmayın, özel `PGPASSFILE` kullanın.
- DB yedeğine ek olarak özel sunucu yapılandırması, şirket kurtarma dosyası ve CA yenileme anahtarlarını ayrı şifreli yerde saklayın. Kurtarma özel anahtarı DB'de tutulmaz.
- Mevcut DB yükseltmesi için [TEAM_UPGRADES_TR.md](TEAM_UPGRADES_TR.md) içindeki `upgrade.py plan/apply/confirm` akışını kullanın. Doğrulanmış geri açma, bakım kilidi ve kesinti sonrası devam bu yoldadır; `manage.py migrate` mevcut şemayı yükseltmek için kullanılamaz.
- Geri dönüş için eski executable ve **ona uygun önceki DB yedeği** gerekir. Yeni sürümde sonradan yazılan veriler eski yedeğe kendiliğinden taşınmaz. Yeni şemayı eski executable ile zorlamayın.
- Sertifika bitmeden yenileyin. CA değişiyorsa istemcilere yeni public HTTPS CA dağıtılmalıdır; doğrulamayı kapatmak çözüm değildir.
- 503: DB/TLS/rol ayrımını kontrol edin. 401/403 ağ kesintisi değildir; hesabı/yetkiyi inceleyin. Config, token veya özel anahtarları hata raporuna eklemeyin.

macOS, gerçek donanım anahtarları ve erişilmeyen üretim ortamları bu kurulum doğrulamasına dahil değildir.
