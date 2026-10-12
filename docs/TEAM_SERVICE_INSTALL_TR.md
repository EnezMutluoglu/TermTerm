# TermTerm Team Service 0.4.1 — kurulum kılavuzu

Bu paket **0.4.1** sürümü içindir. Doğrulama ve ertelenen ortamlar `VALIDATION_0.4.1.md` içinde açıklanır. Masaüstü Windows/Linux programıdır; aşağıdaki servis tarayıcı arayüzü sunmaz.

## Gereksinimler ve ayrım

Linux x64, Docker Engine + Compose eklentisi veya systemd; yönetim bilgisayarında Python 3, OpenSSL ve PostgreSQL `psql` gerekir. Masaüstü doğrudan PostgreSQL'e bağlanmaz. SSH/SFTP masaüstünden hedefe gider. Servis, TLS doğrulamalı PostgreSQL'e sınırlı bir rolle bağlanır. Servis parolası yalnız sunucuda kalır.

- İstemciler → HTTPS 443 (şirketin DNS adına bağlı sertifika).
- Caddy → yalnız `127.0.0.1:41841` Team API.
- Ayrı PostgreSQL seçeneği → yalnız `127.0.0.1:55433`, TLS/SCRAM.
- Migration hesabı ayrı; normal servis rolü tablo veya şema sahibi olamaz, doğrudan tablo yetkileri alamaz.

Arşivi ayrı bir dizine çıkarın. `bin/termterm-team-service`, `migrations/` ve `deploy/` birlikte kalmalıdır. SHA256 dosyasını doğrulayın. Özel yapılandırmaları paket dizininin dışında tutun; bunları Git'e eklemeyin.

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
  -f compose.yaml -f compose.postgres.yaml up -d --build team https
```

CA özel anahtarı yalnız sertifika yenileme içindir; çevrimdışı, erişimi kısıtlı yedekleyin. Caddy şirket CA'sı ile PostgreSQL CA'sı farklıdır: masaüstüne PostgreSQL CA'sı değil **HTTPS CA'sının public sertifikası** verilir.

## B — Mevcut TLS PostgreSQL ile Compose

DB yöneticisi ayrı veritabanı ve migration/servis hesaplarını oluşturur. İkisi de `NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS` olmalı; servis migration hesabının üyesi olmamalıdır. Veritabanının sahibi migration hesabıdır. `pgcrypto` DBA tarafından kurulur. `public` şemasından PUBLIC CREATE, veritabanından PUBLIC erişimleri kaldırılır; servis yalnız CONNECT alır. `migrate` açık fonksiyon izinlerini sonra verir.

`deploy/service.example.json` dosyasını özel bir dizine `service.json` olarak kopyalayıp DB adresi/port/kullanıcı/parola ve CA yolu girin. Container içinde CA yolu `/etc/termterm-team/postgres-ca.crt` olmalıdır. Servis dizini root:10001, izin 0750; `service.json` root:10001, izin 0640; public CA 0644. Migration hesabının `PGPASSFILE` dosyası 0600 olmalı. Parola içindeki `:` ve `\` karakterlerini libpq pgpass kurallarına göre kaçışlayın.

Yukarıdaki migration komutunu mevcut DB alanlarıyla çalıştırın. `TERMTERM_CONFIG_DIR`, `TERMTERM_DOMAIN` ve gerekirse `TERMTERM_CADDY_FILE` içeren özel env dosyası oluşturun. Yalnız `compose.yaml` ile `up -d --build` çalıştırın; `compose.postgres.yaml` eklenmez ve yeni DB açılmaz.

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
