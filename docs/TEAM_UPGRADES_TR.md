# Team servis yükseltmeleri

Bu yükseltme akışı aynı `termterm_team_v2` veritabanının yeni şemasına geçiş içindir. **Kullanılmamış v0.4.0 Team aktarımı kullanıcı kararıyla kapsam dışıdır.** Masaüstü uygulaması masaüstü olarak kalır; aşağıdaki araçlar Linux Team Service yöneticisi içindir.

## Sözleşme ve sınırlar

Paketin `compatibility.json` dosyası servis sürümü, API sürümü, gereken şema ve bakım protokolünü bildirir. Bu kaynakta şema **64**, API **1**, Team cache **8**. Masaüstü kurucusunu güncellemek sunucuya habersiz migration uygulamaz. Servis uyumsuz şemayı hazır göstermez.

Kısa bakım aralığı kullanılır; sıfır kesinti garantisi verilmez. Yeni servis her DB isteği boyunca paylaşımlı advisory lock tutar. Yükseltme özel kilit alır; işlemdeki istek bitmeden başlayamaz, kilit varken yeni istek 503 `service_maintenance` döner. Önceki servis binary'lerinde bu protokol yoktur: **bütün Team servis örneklerini durdurmak her yükseltmede zorunludur**. DBA'nın eşzamanlı DDL/yazma işlemleri de durdurulmalıdır. Araç yalnız verilen sınırlı servis rolünün açık bağlantılarını denetleyebilir.

Yükseltme; hesap, özel/ortak kimlik, izin, geçmiş ve sırları tekrar oluşturan bir import değildir. Sıralı migration çalıştırır. Bekleyen istemci değişiklikleri yerel şifreli günlükte kalır. 503, yetki yenilenmiş veya otomatik çevrimdışı erişime izin verilmiş anlamına gelmez. Yetkiler bağlantı geri geldiğinde yeniden doğrulanır.

## 1. Hazırlık ve salt okunur plan

Eski executable/image'ı saklayın. Yeni paketi ayrı bir dizine açın; çalışan dosyaların üzerine henüz yazmayın. PostgreSQL araçları (`psql`, `pg_dump`, `pg_restore`) sunucu sürümüyle uyumlu olmalıdır. Farklı PostgreSQL major sürümüne geçiş bu aracın kapsamı değildir.

Ayrı migration hesabını kullanın; parola argv'ye yazılmaz:

```bash
export PGHOST=db.example.internal PGPORT=5432 PGDATABASE=termterm_team
export PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-private/postgres-ca.crt
export PGPASSFILE=/etc/termterm-private/migration.pgpass
python3 deploy/upgrade.py plan --service-role termterm_team_service \
  --output /var/lib/termterm-upgrades/plan-61.json
```

`PGPASSFILE` 0600, plan/backup dizini 0700 olmalıdır. Plan; veritabanı kimliği, migration kullanıcısı, uygulanmış revizyonlar, kaynak hash'leri, bekleyen dosyalar ve hedef sözleşmesini içerir. Sırları içermez; plan yine özel dosyadır. Mevcut dosyayı ezmez. Plan aşaması DB'ye yazmaz.

Daha yeni şema, revizyon boşluğu, eksik/değişmiş migration hash'i veya ayrıcalıklı servis rolü reddedilir. Önceki kurulumda hash tablosu yoksa ancak kurulu sürümün **aynı eski kaynakları** bağımsız doğrulandıktan sonra o sürümün `manage.py migrate` komutuyla başlangıç hash'leri alınabilir. Bu, geçmişte hangi SQL'in çalıştığının geriye dönük kanıtı değildir.

## 2. Geri açma doğrulama veritabanı

DBA aynı sunucuda yalnız bu test için **boş ve özel** bir veritabanı açar:

```sql
CREATE DATABASE termterm_upgrade_verify OWNER termterm_team_migrator;
REVOKE ALL ON DATABASE termterm_upgrade_verify FROM PUBLIC;
```

Başka kullanıcılara CONNECT vermeyin. PGPASSFILE bu veritabanı için de kimlik doğrulamayı kapsamalıdır. İlgili doğrulama DB'si aynı migration hesabına ait olmalı; kaynak DB olamaz. Mevcut kullanıcı tabloları/rutinleri/şemaları varsa işlem reddedilir, hiçbir şey silinmez. Başarılı doğrulama bu DB'de veri bırakır; tekrar yeni bir doğrulama gerektiğinde DBA yeni boş DB sağlar.

## 3. Durdur, yedekle, doğrula, yükselt

systemd veya Compose ile yalnız kendi Team servis örneklerinizi durdurun; PostgreSQL çalışmaya devam eder:

```bash
sudo systemctl stop termterm-team
# veya kendi proje/env ayarlarınızla: docker compose stop team
python3 deploy/upgrade.py apply --plan /var/lib/termterm-upgrades/plan-61.json \
  --state-directory /var/lib/termterm-upgrades/run-61 \
  --verification-database termterm_upgrade_verify --service-stopped
```

Araç:

1. Paket/DB kimliğini ve kaynak hash'lerini yeniden karşılaştırır; aynı anda ikinci yükseltmeyi engeller.
2. Özel bakım kilidi altında tutarlı snapshot ile tam veritabanı `pg_dump -Fc` alır. Dosya 0600'dür; önceki yedek üzerine yazılmaz.
3. Yedeği ayrı özel DB'de gerçek `pg_restore --single-transaction --no-owner --no-privileges` ile açar; Team tablolarının satır sayılarını karşılaştırır. Yedek okunabilirliği dışında ciphertext eşitliği de laboratuvarda ayrıca test edilmiştir; araç üretimde her satırın kriptografik eşitliğini tek tek ölçmez.
4. Her migration ve dosya hash'ini aynı transaction içinde kaydeder. Bütün eski hash'ler yeni SQL'den önce denetlenir. İzin listesi güncellenir.
5. `state.json` içinde aşama ve sonuçları saklar; `awaiting_service` aşamasında durur. Binary/image kurmaz, servis başlatmaz ve release yayımlamaz.

Yedek, rol tanımları/rol parolaları, özel `service.json`, CA özel anahtarları veya şirket kurtarma dosyasının yedeği değildir. Bunları ayrı şifreli depoda koruyun. Yedekte şifreli kayıtların yanında hesap/izin metadata'sı da vardır; `.dump` dosyasını herkese açık alana koymayın.

## 4. Aynı planla devam

Disk/ağ/süreç hatasında servisleri kapalı tutun. `state.json` ve yedekleri silmeyin. **Aynı paket, plan ve state dizini** ile apply komutunu tekrar çalıştırın. Uygulanmış revizyonlar tekrarlanmaz. Önceki backend hâlâ bir sorguyu bitiriyorsa kilit hatası verir; kilit boşaldıktan sonra tekrar deneyin. Araç kullanıcı işlemini başarısız diye yeniden göndermez.

Yedek hash'i değişmişse devam reddedilir. Paket veya DB kimliği değişmişse yeni plan gerekir; eski geri dönüş noktasını koruyun. Yedek doğrulaması tamamlanmadan kesilen denemelerde yeni yedek dosyası alınabilir, önceki dosya tutulur. Doğrulama DB'si doluysa yeni boş DB gerekir. Araç kaynağa `DROP`, `--clean` veya otomatik downgrade uygulamaz.

## 5. Servisi başlat ve doğrula

Yeni executable'ı/image'ı kurup kendi Team servisinizi başlatın. Kullanıcılara açmadan önce:

```bash
python3 deploy/upgrade.py confirm --state-directory /var/lib/termterm-upgrades/run-61 \
  --url https://team.example.com --ca-file /etc/termterm-private/https-ca.crt
```

HTTPS CA/hostname, API/şema/paket sözleşmesi ve planlanan DB'nin kurulum kimliği karşılaştırılır. Yanlış sunucuda aynı sürümün çalışması başarılı yükseltme sayılmaz. Redirect takip edilmez. Kontrol geçtiğinde aşama `ready` olur. Yanlış CA veya yanlış DB halinde `awaiting_service` korunur.

Ardından test hesabıyla giriş, host listeleme ve izinli bağlantı kontrolü yapın; bakımda bekleyen istemcilerin kuyruklarının normal yetki/çakışma yoluyla eşitlendiğini izleyin.

## Geri dönüş

Otomatik ters migration yapılmaz. Servisi durdurun; önceki binary ile uyumlu **yükseltme öncesi doğrulanmış yedeği yeni bir DB'ye** DBA gözetiminde geri açın. Sahiplik ve sınırlı servis hesabı grant'lerini önceki paketten yeniden doğrulayın. TLS bağlantı profilini bu DB'ye yönlendirip önceki servisle kontrol edin. Kaynak/yeni DB'yi doğrulama bitmeden silmeyin. Yeni sürüm başladıktan sonra yazılan kayıtlar eski yedeğe kendiliğinden taşınmaz; geri dönüş bu zaman aralığının kaybını önceden değerlendirmeyi gerektirir.

## Geliştirme kuralı

Uygulanmış migration dosyaları sabittir; yeni numara eklenir. Her yeni şema için temiz kurulum, eski veriyle yükseltme, kesinti/devam, yanlış sürüm reddi ve yedekten geri açma testi gerekir. API/cache değişiklikleri sürümlendirilir. Sadece SQL dosyasının varlığı başarılı yükseltme sayılmaz.

Kaynaklar: [PostgreSQL pg_dump](https://www.postgresql.org/docs/current/app-pgdump.html), [pg_restore](https://www.postgresql.org/docs/current/app-pgrestore.html), [advisory lock davranışı](https://www.postgresql.org/docs/current/explicit-locking.html).

## Takımın seçeceği istemci sürümü

Şema yükseltmesi ile kullanıcılara zorunlu istemci güncellemesi aynı işlem değildir. Uyumluluğu bozmayan tablo eklemelerinde eski istemci çalışabilir. Paketin `minimumClientVersion` ve `apiVersion` alanları teknik alt sınırdır; `clientVersions` desteklenen istemci seçenekleridir. Normal Team sürüm seçimini takım sahibi **Team → Sürüm politikası** ekranında yapar. Etkin alt sınır teknik gereklilik ile sahibin seçiminin daha yüksek olanıdır. Sahip teknik açıdan uyumsuz bir sürüme çalışma izni veremez.

Önce desteklenen Windows/Linux paketlerini hazırlayıp doğrulayın ve yetkili yayın sürecini tamamlayın. Servis/veritabanı yükseltmesini bu kılavuzdaki ayrı bakım akışıyla gerçekleştirin. Sonra takım sahibi uygun istemci sürümünü seçip onaylasın. Teknik alt sınırı yükseltmek gerekiyorsa bunu bakım planında önceden duyurun; dosyada yeni tablo var diye otomatik olarak minimum sürümü artırmayın.

062 migration'ı `client_update_policies` ve `client_update_history` tablolarını ekler; takım başına tek güncel karar, önceki 10 karar ve mevcut audit politikası vardır. Üyeler politikayı değiştiremez. API token hash'inden takım belirler; istemcinin takım başlığına güvenilmez. 426 yanıtı ağ kesintisi değildir. SQLite `.ttpolicy` dosyasında yalnız servis/takım kapsamına ait açık sürüm bilgisi saklanır; sır/token/günlük içermez ve dağıtım paketine girmez. DB, şifreli yerel günlük ve kimlik biçimi değiştirilmeden kalır.
