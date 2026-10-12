# TermTerm 0.4.2 — Soket DB ve IP üzerinden HTTPS kurulumu

Bu yol Debian 13/Linux'ta PostgreSQL ve Team servisinin aynı sunucuda olduğu kurulum içindir. DB bağlantısı Unix soketiyle **DB parolası ve DB sertifikası olmadan** yapılır. Masaüstü IP üzerinden HTTPS kullanır; Caddy yerel sertifikayı üretir. Alan adı veya dış sertifika firması gerekmez. Team kullanıcıları yine liderin verdiği kullanıcı adı/parolasıyla giriş yapar.

Örnek sunucu **10.20.30.10**, PostgreSQL **18/main**, port **5432**. Kendi değerlerinizle değiştirin. Komutlar root shell (sudo -i) içindir. Mevcut veri üzerinde ilk kurulum SQL'ini tekrar çalıştırmayın.

## 1. Paketler

GitHub [v0.4.2 Release](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.4.2) sayfasından servis arşivini, Docker imajını ve SHA256SUMS.txt dosyasını indirin. Docker Engine/Compose plugin ve PostgreSQL kurulu olmalı; [ana kılavuzdaki](TEAM_SERVICE_INSTALL_TR.md) gereksinimleri izleyin.

    sha256sum --check --ignore-missing SHA256SUMS.txt
    install -d -m 0755 /opt/termterm-team
    tar -xzf TermTerm-Team-Service-0.4.2-linux-x64.tar.gz --strip-components=1 -C /opt/termterm-team
    gzip -dc TermTerm-Team-Image-0.4.2-linux-x64.tar.gz | docker load
    apt-get install python3 postgresql-client locales curl openssl
    docker compose version

İmaj Debian 13 tabanlıdır; sunucuda Rust derlemesi gerekmez. Mevcut kurulumun üzerine açmak yerine yeni sürümü ayrı dizinde hazırlayıp [yükseltme akışını](TEAM_UPGRADES_TR.md) uygulayın.

## 2. DB, owner ve uygulama grupları

[Şirket rol şablonu](TEAM_DB_OWNER_TEMPLATE_TR.md) korunur: owner DB/tablo/migration sahibidir; appuser yalnız Team API fonksiyonlarını çağırır. Servisi owner hesabıyla çalıştırmayın.

    pg_lsclusters
    locale -a
    # en_US.utf8 yoksa:
    localedef -i en_US -f UTF-8 en_US.UTF-8
    sudo -u postgres psql -X -p 5432 -d postgres -v socket_auth=true -f /opt/termterm-team/deploy/prepare-database-owner-template.sql

Yeni DB: termterm_team. LOGIN hesapları: termterm_team_owner ve termterm_team_appuser. NOLOGIN grupları: termterm_team, termterm_team_app, termterm_team_developer, termterm_team_readonly. **Parola sorulmaz**, LOGIN hesaplarının DB parolası olmaz. UTF8, en_US.UTF-8, libc, template0 ve pgcrypto hazırlanır; Team tabloları 4. adımda oluşturulur. Treasury DB'si değiştirilmez.

## 3. Soket ve HBA

    sudo -u postgres psql -X -p 5432 -d postgres -c "SHOW unix_socket_directories; SHOW hba_file; SHOW config_file;"

Beklenen soket dizini /var/run/postgresql. Farklıysa config host, Compose mount ve PGHOST aynı gerçek dizini kullanmalı. Port 5432 burada TCP yerine .s.PGSQL.5432 soketini seçer.

SHOW hba_file çıktısındaki dosyayı düzenleyin. Aşağıdaki blok genel local/host kurallarından **önce** gelmeli; dosyanın tamamını değiştirmeyin, DBA'nın mevcut peer yolunu koruyun:

    # Yalnız bu DB ve bu iki hesap için soketten parolasız:
    local termterm_team termterm_team_owner,termterm_team_appuser trust
    local termterm_team postgres peer
    local termterm_team all reject
    # Team DB için TCP kapalı; diğer DB'lerin kuralları korunur:
    host termterm_team all 0.0.0.0/0 reject
    host termterm_team all ::/0 reject

**trust**, işletim sistemi kullanıcısını doğrulamaz. Sokete erişebilen yerel süreçler owner/appuser adlarını kullanabilir. Bu yol güvenilir ve bu işe ayrılmış sunucu içindir; paylaşımlı sunucuda peer/pg_ident eşlemesi veya SCRAM seçin. Genel local all all trust eklemeyin. [PostgreSQL açıklaması](https://www.postgresql.org/docs/18/auth-trust.html).

    sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT line_number,error FROM pg_hba_file_rules WHERE error IS NOT NULL;"
    sudo -u postgres psql -X -p 5432 -d postgres -c "SELECT pg_reload_conf();"

İlk sorgu sıfır satır dönmeli. HBA için reload yeterli; ssl veya listen_addresses değiştirilmez, diğer DB'lerin sertifikaları silinmez. Soketi elle oluşturmayın veya chmod 777 uygulamayın.

## 4. Owner ile migration

    export PGHOST=/var/run/postgresql PGPORT=5432 PGDATABASE=termterm_team PGUSER=termterm_team_owner
    export PGSSLMODE=disable
    unset PGPASSWORD PGPASSFILE PGSSLROOTCERT PGHOSTADDR PGSERVICE
    psql -X -w -v ON_ERROR_STOP=1 -c "SELECT current_user,current_database(),inet_client_addr();"
    cd /opt/termterm-team
    python3 deploy/manage.py migrate --service-role termterm_team_appuser

Beklenen: owner, termterm_team, inet_client_addr=NULL (soket), **64 migration** doğrulaması. Tabloları/indeksleri elle oluşturmayın. Servis otomatik migration yapmaz. Sonraki DB yükseltmeleri yedekli upgrade.py akışını kullanır. 0.4.1 → 0.4.2 şema değişikliği yoktur.

## 5. Servis config ve Compose

    install -d -m 0700 /etc/termterm-team-private
    install -d -m 0750 -o root -g 10001 /etc/termterm-team-private/config
    install -m 0640 -o root -g 10001 deploy/service.socket.example.json /etc/termterm-team-private/config/service.json

service.json içeriği:

    {
      "listen": "127.0.0.1:41841",
      "database": {
        "host": "/var/run/postgresql",
        "port": 5432,
        "database": "termterm_team",
        "username": "termterm_team_appuser",
        "sslMode": "disable"
      }
    }

password ve caPath alanları yoktur. Servis HTTP dinlemesi loopback'te kalır; çalışanlar yalnız Caddy HTTPS portuna erişir.

/etc/termterm-team-private/deployment.env dosyası oluşturun:

    TERMTERM_CONFIG_DIR=/etc/termterm-team-private/config
    TERMTERM_DOMAIN=10.20.30.10
    TERMTERM_CADDY_FILE=/opt/termterm-team/deploy/Caddyfile.internal
    TERMTERM_SERVICE_IMAGE=termterm/team-service:0.4.2
    TERMTERM_TEAM_CPUS=1.0
    TERMTERM_TEAM_MEMORY=512m
    TERMTERM_HTTPS_CPUS=0.5
    TERMTERM_HTTPS_MEMORY=128m

IP'yi değiştirin. Caddyfile.internal özellikle seçilir; ücretli/genel CA kullanılmaz. İki Compose dosyasını birlikte kullanın:

    cd /opt/termterm-team
    docker compose --env-file /etc/termterm-team-private/deployment.env -f deploy/compose.yaml -f deploy/compose.socket.yaml up -d --no-build
    docker compose --env-file /etc/termterm-team-private/deployment.env -f deploy/compose.yaml -f deploy/compose.socket.yaml ps

Soket dizini container'a salt okunur bağlanır; bu DB sorgularının yazma yapmasını engellemez. DB veri dizini mount edilmez. CPU/RAM/pids/log sınırları ve unless-stopped ana Compose'dadır. Unhealthy tek başına restart yapmaz; süreç çökerse yeniden başlar. Çalışanlara yalnız 443/TCP açın; PostgreSQL/41841 portlarını dışarı açmayın.

## 6. Sertifika üretimi ve çalışanlara dağıtım

Caddy ilk açılışta yerel CA ve IP'ye uygun HTTPS sertifikası üretir, volume'da tutar ve sunucu sertifikasını yeniler. Başlatma tamamlanınca:

    cd /opt/termterm-team
    docker compose --env-file /etc/termterm-team-private/deployment.env -f deploy/compose.yaml -f deploy/compose.socket.yaml cp https:/data/caddy/pki/authorities/local/root.crt /etc/termterm-team-private/team-https-ca.crt
    openssl x509 -in /etc/termterm-team-private/team-https-ca.crt -noout -subject -fingerprint -sha256
    curl --fail --cacert /etc/termterm-team-private/team-https-ca.crt https://10.20.30.10/ready

Beklenen /ready cevabı şema 64'tür. Yalnız **team-https-ca.crt** public dosyasını çalışanlara verin; sertifika SHA256 parmak izini ayrıca güvenilir kanaldan doğrulatın. root.key, Caddy volume içeriği, owner erişimi ve kurtarma private key'i paylaşılmaz.

Çalışanın TermTerm → Team girişinde:

1. Servis adresi: https://10.20.30.10 (aynı IP).
2. Takım kimliği ve liderin verdiği kullanıcı adı/geçici parola.
3. team-https-ca.crt dosyasını metin editörüyle açıp BEGIN/END CERTIFICATE dahil PEM metnini **Şirket CA sertifikası** alanına yapıştırın. Dosya yolunu yazmayın.
4. Giriş yapıp ilk parolayı değiştirin. Profil/parolayı hatırla seçeneğiyle sonraki girişlerde tekrar yazmanız gerekmez.

Alan adı “Şirket CA” olsa da dış firmadan sertifika istemez; kendi sunucunuzun güven sertifikasıdır. Bu sertifika tanıtılmadan tarayıcı güven uyarısı verir, TermTerm bağlantıyı doğrulama hatasıyla durdurur. **Tüm sertifikaları kabul et / curl -k kullanılmaz.** Sertifikayı tanıtmak hem şifrelemeyi hem doğru sunucuya bağlanmayı sağlar. [Caddy yerel HTTPS](https://caddyserver.com/docs/automatic-https#local-https).

## 7. İlk lider, üyeler ve yükseltme

4. adımdaki owner ortamı açıkken:

    cd /opt/termterm-team
    python3 deploy/manage.py bootstrap --team "Sirket" --username lider --display-name "Takim Lideri" --output /etc/termterm-team-private/ilk-lider.json

Özel çıktı dosyasındaki takım kimliği ve tek kullanımlık lider parolasıyla masaüstünde giriş yapın. Lider parolasını değiştirir; Team → Üyeler'den hesapları ve klasör/host izinlerini verir. Kayıt otomatik tüm hostlara erişim vermez. Günlük Hosts/SSH/SFTP normal TermTerm ekranlarında kalır. [Üye/yetki kılavuzu](TEAM_041_GUIDE_TR.md).

Yükseltmede DB ve Caddy caddy_data/caddy_config volume'ları korunur. **docker compose down -v çalıştırmayın**; CA değişirse çalışanların güven sertifikası da değişir. [Yedekli upgrade kılavuzunu](TEAM_UPGRADES_TR.md) owner/soket ortamıyla uygulayın. Upgrade'ın HTTPS CA parametresi bu team-https-ca.crt dosyasıdır; DB CA değildir.

Mevcut 0.4.1 TLS veya parolalı DB hesaplarını yeniden oluşturmayın. Yeni servis imajını yükleyin, şema/API uyumunu ve yedekli yükseltmeyi doğrulayın; isterseniz HBA/config'i soket moduna çevirip yeniden başlatın. Eski DB parolaları mevcutsa silmek gerekmez; socket trust yolunda kullanılmaz. 0.4.1 masaüstleri API 1/şema 64 ile çalışmaya devam eder; takımın yönetici sürüm seçimi korunur.
