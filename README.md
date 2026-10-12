# TermTerm

Tauri 2, Rust ve React ile geliştirilmiş kişisel SSH çalışma alanı. Windows x64, Linux x64 ve macOS Intel/Apple Silicon derleme hedefleri vardır. PostgreSQL olmadan çalışır. Bağlantılar, kimlikler ve oturum kayıtları parola korumalı `.ttvault` dosyasında tutulur.

**Sürüm: 0.4.2.** Terminal başına canlı CPU/RAM/disk göstergesi, terminal temaları, klasör içinde host görünümü ve geliştirilmiş import adaptörleri içerir. 0.1 kasa/yedek biçimi korunur. Termius düzeninden esinlenen bağımsız uygulamadır. Planın tamamıyla özellik eşitliği henüz sağlanmış değildir. Özellikle doğrudan Windows Hello/FIDO2 anahtar oluşturma ve bazı üreticiye özel şifreli import biçimleri tamamlanmamıştır. Ayrıntılar: [0.4.2 doğrulama raporu](docs/VALIDATION_0.4.2.md), [platform derlemeleri](docs/BUILD_PLATFORMS.md), [import kapsamı](docs/IMPORT_SUPPORT.md), [PostgreSQL tablo ve kurulum akışı](docs/POSTGRES_FLOW.md).

## Team ve parola panosu — 0.4.2

Team hesapları, kaynak bazında yetkiler, şifreli çevrimdışı kuyruk, karar ekranı ve kayıt geçmişi içerir. Team için Linux Team Service ve PostgreSQL gerekir; masaüstü yalnız HTTPS servis profili kullanır, DB parolası cihazlara verilmez. Lider kullanıcı oluşturur, kullanıcı kendi SSH/parola profilini saklar; ortak host bilgileri yetkili üyelerde aynıdır. Kişisel kasalar sunucusuz çalışır. [Servis kurulumu](docs/TEAM_SERVICE_INSTALL_TR.md), [yedekli yükseltme](docs/TEAM_UPGRADES_TR.md). [Team kullanım/DB kılavuzu](docs/TEAM_041_GUIDE_TR.md).

Sol alttaki **Parola üretici**, karakter sayısı ve harf/rakam/noktalama/sembol/parantez seçimleriyle güvenli rastgele parolalar üretir. Son 50 parola tarih-saatle yerelde şifrelenir. Team panosu yalnız hesaba aittir ve paylaşılmaz. [Parola panosu kılavuzu](docs/PASSWORD_BOARD_TR.md).

## Team kurulumu

**0.4.2 aynı sunucu, alan adı yok:** [DB Unix soketi, owner/appuser grupları, IP ile yerel HTTPS ve çalışanlara sertifika dağıtımı](docs/TEAM_SOCKET_KURULUM_TR.md). DB parolası/DB CA'sı gerekmez; masaüstü HTTPS ve Team kullanıcı adı/parolası kullanır.

**Şirketiniz owner/app/developer/readonly/appuser rol düzeni kullanıyorsa:** [TermTerm DB oluşturma SQL'i ve owner/appuser bağlantı örnekleri](docs/TEAM_DB_OWNER_TEMPLATE_TR.md). Owner migration ve veri sahipliğini, appuser yalnız izinli servis işlemlerini üstlenir.

**İlk kez PostgreSQL TLS/şirket CA'sı kuruyorsanız:** [komutlar, örnek conf/HBA içerikleri ve TermTerm CA alanıyla ayrıntılı rehber](docs/TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md). Hangi değeri nereye yazacağınız, `postgresql.auto.conf` davranışı ve başarılı/başarısız bağlantı testleri adım adım gösterilir.

**Linux x64 hazır Debian 13 servis/imajı:** [0.4.2 dosyaları](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.4.2) · [soket + IP HTTPS kurulumu](docs/TEAM_SOCKET_KURULUM_TR.md) · [TLS/diğer kurulumlar](docs/TEAM_SERVICE_INSTALL_TR.md).

Team, normal Windows/Linux masaüstü uygulamasından kullanılır. Sunucudaki HTTPS servisi hesapları, izinleri ve şifreli eşitlemeyi yönetir; web arayüzü değildir. SSH/SFTP bağlantıları kullanıcı bilgisayarından hedefe gider. Sunucuda Rust/Node derlemesi gerekmez.

### 1. Paketi indir ve hazır image'ı yükle

Sunucuda Docker Engine/Compose; kurulum yönetimi için Python 3, OpenSSL ve PostgreSQL `psql` bulunmalıdır. İki indirme bağlantısındaki dosyaları aynı klasöre koyup:

```bash
sha256sum --check TermTerm-Team-Docker-0.4.1-debian13-linux-x64.tar.gz.sha256
tar -xzf TermTerm-Team-Docker-0.4.1-debian13-linux-x64.tar.gz
cd termterm-team-docker-0.4.1
sha256sum --check SHA256SUMS.txt
sudo docker load --input TermTerm-Team-Image-0.4.1-linux-x64.tar.gz
tar -xzf TermTerm-Team-Service-0.4.1-linux-x64.tar.gz
cd team-service-linux-x64
```

Bu paket image, servis/Compose dosyaları, SQL ve Türkçe kılavuzları birlikte içerir. Image etiketi `termterm/team-service:0.4.1` olur. Ayrı eski servis arşivini ayrıca indirmeniz gerekmez.

### 2. PostgreSQL'i ve iki sunucu hesabını hazırla

**PostgreSQL'i Debian/Linux'a kendiniz kurabilirsiniz.** DBA, yeni boş kurulum için paketteki `deploy/prepare-database.sql` dosyasını `psql` ile çalıştırır. SQL; `termterm_team` veritabanını, sahibi olan `termterm_team_migrator` hesabını, sınırlı `termterm_team_service` hesabını ve `pgcrypto` eklentisini hazırlar. Parolalar gizli istemle sorulur. Mevcut DB/hesap varsa scripti tekrar uygulamak yerine [kılavuzun B bölümündeki](docs/TEAM_SERVICE_INSTALL_TR.md#b--mevcut-tls-postgresql-ile-compose) mevcut DB adımlarını izleyin.

DB bağlantısında doğrulanan TLS sertifikası ve SCRAM gerekir. PostgreSQL kullanıcılarını Team üyeleriyle karıştırmayın: **üyelere DB hesabı/parolası verilmez**; lider daha sonra uygulama hesabı oluşturur. PostgreSQL'i de Compose kursun istiyorsanız [A bölümünü](docs/TEAM_SERVICE_INSTALL_TR.md#a--ayrı-postgresql-ile-compose) kullanın.

### 3. Bağlantı, HTTPS ve özel dosyaları ayarla

[Kılavuzdaki izinlerle](docs/TEAM_SERVICE_INSTALL_TR.md#2-özel-dosyaları-hazırlayın) `/etc/termterm-team-private` altında şunları hazırlayın:

| Dosya | Girilecek bilgi |
|---|---|
| `config/service.json` | DB adresi/portu, DB adı, **service** kullanıcı/parolası, DB public CA yolu; API `127.0.0.1:41841` üzerinde kalır. |
| `config/postgres-ca.crt` | PostgreSQL sunucu sertifikasını doğrulayan public CA. |
| `migration.pgpass` | Yalnız kurulum/yükseltme için **migrator** parolası; dosya izni 0600. |
| `deployment.env` | HTTPS DNS adı, config dizini, sabit image sürümü ve CPU/RAM sınırları; parola içermez. |

Örnekler `deploy/service.example.json` ve `deploy/deployment.env.example` içindedir. Caddy için şirket DNS adını ayarlayın; public sertifika yönteminde 80/443 erişimi gerekir. Şirket CA'sı kullanılıyorsa masaüstüne HTTPS **public** CA verilir. DB CA ile HTTPS CA farklı olabilir; TLS doğrulaması kapatılmaz.

### 4. Şema ve tabloları açık migration adımıyla oluştur

Paket kökünde, özel dosyaları okuyabilen yönetim hesabıyla; DB adresini kendi sunucunuza göre değiştirin:

```bash
export PGHOST=postgres.company.example PGPORT=5432 PGDATABASE=termterm_team PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-team-private/config/postgres-ca.crt
export PGPASSFILE=/etc/termterm-team-private/migration.pgpass
python3 deploy/manage.py migrate --service-role termterm_team_service
```

**Tabloları tek tek oluşturmayın.** Araç şemayı, tabloları, indeksleri ve sınırlı fonksiyon izinlerini kurar; 0.4.1 için 64 migration doğrulanır. Normal servis/masaüstü otomatik migration yapmaz. Mevcut Team DB'sini yeni sürüme geçirmek için bu ilk kurulum komutu yerine [yedekli yükseltme akışı](docs/TEAM_UPGRADES_TR.md) kullanılır.

### 5. Servisi başlat ve bağlantıyı doğrula

Elle kurduğunuz mevcut PostgreSQL için:

```bash
sudo systemctl enable --now docker
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f deploy/compose.yaml up -d --no-build team https
sudo docker compose --project-name termterm-team --env-file /etc/termterm-team-private/deployment.env \
  -f deploy/compose.yaml ps
curl --fail https://team.company.example/ready
```

URL'yi kendi DNS adınızla değiştirin; şirket CA'sında curl'e `--cacert /yol/https-ca.crt` ekleyin. `/ready` DB/TLS/şema uyumunu doğrular. Mevcut PostgreSQL kullanımında `compose.postgres.yaml` eklenmez.

### 6. İlk lideri ve üyeleri oluştur

4. adımdaki migration ortamı hâlâ seçiliyken, paket kökünden:

```bash
python3 deploy/manage.py bootstrap --team "Şirket takımı" --username leader \
  --display-name "Takım lideri" --output /etc/termterm-team-private/ilk-lider.json
```

Yeni özel dosyada takım kimliği ve 24 saatlik geçici parola bulunur. Lider masaüstündeki **Team hesabı** girişinde HTTPS adresi, takım kimliği ve bu hesabı kullanır; ilk girişte parolasını değiştirir. Şirket kurtarma dosyasını oluşturup güvenli sakladıktan sonra **Team → Üyeler** üzerinden kullanıcı ekler ve klasör/host izinlerini verir. Hesap açılması otomatik host erişimi sağlamaz. Günlük Hosts/SFTP/Kimliklerim kullanımı normal TermTerm düzenindedir. [Üye, yetki ve kurtarma kılavuzu](docs/TEAM_041_GUIDE_TR.md).

**İşletim:** Team varsayılan sınırı 1 CPU / 512 MiB RAM, Caddy 0,5 CPU / 128 MiB; loglar container başına 10 MiB × 3 döner. Süreç çökerse `unless-stopped` yeniden başlatır; yalnız `unhealthy` olması restart tetiklemez. PostgreSQL disk kotası/boş alanı ayrıca yönetilir. [Kaynak ve disk ayrıntıları](docs/TEAM_SERVICE_INSTALL_TR.md#kaynak-ağ-disk-ve-yeniden-başlatma) · [Debian 13 Docker test raporu](docs/TEAM_DOCKER_VALIDATION_TR.md).

## Önceki düzeltmeler ve güncellemeler

**0.3.7**, Stats paneline gelen/giden ağ hızını (Mbit/s), seçili disk okuma/yazma hızını (MiB/s) ve tüm bağlı diskler için kritik/dolu uyarılarını ekler. %90 ve üzeri kritik, kullanılabilir alan sıfırsa dolu gösterilir; uyarılı diskler listede öne çıkar. [Ölçüm ayrıntıları](docs/RESOURCE_ACTIVITY.md). Önceki terminal düzeltmesi korunur: bağlantı kurulurken kaybolan ilk terminal boyutunu ve pencere büyütme/küçültme sırasında Vim/pg_activity alanının eksik kullanılmasını düzeltir. Terminal boyutu bağlantı, pencere, font, Stats ve panel değişimlerinde yenilenir. 0.3.5 davranışı korunur: klasörler tek tıkla açılır; hostlarda tek tık bilgi, hızlı çift tık bağlantı davranışını düzeltir. 0.3.4 sürümündeki parola hatırlama düzeltmesi ve Türkçe arayüz korunur. Kayıtlı parola açılışta otomatik kullanılır; elle kilitleme korunur. Önceki sürümdeki yumuşak antrasit–mavi kontroller, üst sırada bağlantı sekmeleri, altta terminal araçları, seçimle kopyalama, sağ tık/Shift+Insert yapıştırma ve doğrudan Updates bağlantısını içerir. Hostlar tek tıkla bilgileri gösterir; klasör bilgileri sağ tık → Bilgiler üzerinden açılır; düzenleme, taşıma, çoğaltma ve silme sağ tık menüsündedir. Tek tık klasörü açar; hosta hızlı çift tık bağlantıyı başlatır. SFTP dosya işlemleri de sağ tık menüsündedir. Windows x64 ve Linux x64 (DEB, RPM, AppImage) paketleri [sürüm sayfasında](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.4.1) bulunur. macOS Intel/Apple Silicon derleme yapılandırması hazırdır; sahibinin isteğiyle Mac CI işleri durdurulmuştur ve bu sürümde Mac paketi yayımlanmaz. [Derleme komutları](docs/BUILD_PLATFORMS.md).

RSA 2048/3072/4096, Ed25519 ve ECDSA anahtar uyumluluğu ile kayıtlı sunucu anahtarı seçimi düzeltmeleri korunur. **Sol alt Güncellemeler → Güncellemeleri denetle** veya **Ayarlar → Güncellemeler** ile onaylanmış yeni sürümler kontrol edilir. Windows ve Linux AppImage kurulmadan önce güncelleme imzasını doğrular. DEB/RPM için aynı sürümden uygun paket indirilip paket yöneticisiyle kurulur. Güncelleme kullanıcı tarafından başlatılır. Geliştirme dalı `develop`, onaylanan sürümler `main` üzerindedir. 0.3.1 ve geliştirme kanalı kullanıcıları kararlı paketi bir kez elle kurmalıdır; Windows 0.3.2 güncelleme desteği içerir. Ayrıntılar: [yayın akışı](docs/RELEASE_PROCESS.md).

## Çalıştırma

0.3.1 klasör düzeltmesi: Hosts ana ekranı yalnızca klasörleri gösterir. Klasörsüz kayıtlar `Ungrouped` sanal klasöründen açılır; kayıtların gerçek grup ilişkileri değiştirilmez. Klasör kartları ve sol menü, alt klasörleri de içeren toplam host sayısını gösterir. Bu sürümün kontrol sonuçları [0.3.1 raporunda](docs/VALIDATION_0.3.1.md) bulunur.

Windows için [0.4.1 sürüm sayfasındaki](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.4.1) `TermTerm_0.4.1_x64-setup.exe` kurucusunu kullanın. Kurucu WebView2 çevrimdışı önkoşulunu ve Mosh çalışma dosyalarını içerir. Taşınabilir ZIP'i tercih ederseniz **tamamını** aynı klasöre çıkarıp `TermTerm.exe` çalıştırın. Kaynak deposu büyük kurucuları içermez.

Debian/Ubuntu paketini `sudo apt install ./TermTerm_0.4.1_amd64.deb`, uygun RPM dağıtımında `sudo dnf install ./TermTerm-0.4.1-1.x86_64.rpm` ile kurun. AppImage dosyasına çalıştırma izni verip açın. Eksik sistem bağımlılıkları paket yöneticisi üzerinden indirilir. Linux tabanı glibc 2.34+ ve WebKitGTK 4.1'dir; her dağıtımda test edildiği anlamına gelmez. Platform sınırları ve gerçek testler [doğrulama raporunda](docs/VALIDATION_0.3.7.md) yer alır.

İlk ekranda **Yerel kasa oluştur**, **Kasa dosyası aç** veya **Yedeği geri yükle** seçin. En az 8 karakterlik kasa parolası belirleyin. Veritabanı hesabı gerekmez. Kasa parolasının sıfırlanması mümkün değildir.

## Kaynaktan geliştirme

Windows gereksinimleri: Windows 11 x64, Node.js 22.12+ (CI: 24), pnpm 11.19, Rust 1.98.1 MSVC, Visual Studio C++ Build Tools ve Windows SDK, WebView2. Unix bağımlılıkları ve komutları [platform kılavuzunda](docs/BUILD_PLATFORMS.md).

```powershell
pnpm install --frozen-lockfile
node scripts/fetch-mosh.mjs
pnpm desktop
```

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 check
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 test
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
powershell -ExecutionPolicy Bypass -File scripts/verify-package.ps1
```

`pnpm dev` tarayıcıda yalnızca arayüzü gösterir. Kasalar, bağlantılar ve dosya işlemleri native uygulamada çalışır. Geliştirme ortamındaki `/?preview` görünümü sentetik örnek kayıtlar kullanır; üretim paketinde kapalıdır.

## Temiz kaynak, GitHub ve Syncthing

Kaynak klasöründe indirilen bağımlılıklar ve derleme çıktıları tutulmak zorunda değildir. `node_modules`, `src-tauri/target`, `dist`, üretilen Tauri şemaları ve Mosh çalışma dosyaları yeniden oluşturulabilir. Kaynak kodu, testler, ikonlar, lisans belgeleri, `pnpm-lock.yaml`, `Cargo.lock`, `rust-toolchain.toml` ve derleme yapılandırmaları korunur. Hazır kurucuları çalıştırmak için geliştirme bağımlılıklarını yeniden kurmak gerekmez.

Windows'ta temiz kaynakla yeniden başlamak için yukarıdaki sistem gereksinimlerini kurup şu komutları çalıştırın:

```powershell
pnpm install --frozen-lockfile
node scripts/fetch-mosh.mjs
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
```

Geliştirme için son komut yerine `pnpm desktop` kullanabilirsiniz. Rust bağımlılıkları ve `src-tauri/gen` şemaları derlemede yeniden üretilir. Linux/macOS'ta [platform kılavuzundaki](docs/BUILD_PLATFORMS.md) sistem bağımlılıklarını kurduktan sonra:

```bash
pnpm install --frozen-lockfile
bash scripts/build.sh package
```

Bu betik Mosh çalışma dosyalarını da hazırlar. Bağımlılıkları yeniden indirmek için internet gerekir; ilk temiz derleme daha uzun sürer. macOS her mimarinin kendi Mac ortamında veya GitHub Mac runner'larında derlenir.

`.gitignore` kaynak dışı dosyaları Git'ten; `.stignore-shared` Syncthing'den hariç tutar. `artifacts` içindeki kurucular ve import görüntüleri ile `.lab` içindeki özel test verileri bu bilgisayarda korunur, bu kurallarla **GitHub'a veya Syncthing'e gönderilmez**. Başka bilgisayara kurucu taşımak isterseniz ilgili paketi ayrıca kopyalayın. Kaynak ZIP'leri üretildikleri sürümün anlık kopyasıdır; mevcut klasördeki daha yeni temizlik belgeleri ve ignore kuralları için doğrudan kaynak klasörünü kullanın.

Syncthing'de paylaşılan klasörün kökü **termterm** olmalıdır. Her cihazda, ilk eşitlemeden önce `.stignore-shared` dosyasını ve aşağıdaki tek satırlık UTF-8 `.stignore` dosyasını köke koyun. Mevcut yerel kurallarınız varsa bu satırı onların yanına ekleyin:

```text
#include .stignore-shared
```

`.stignore` cihazlar arasında kendiliğinden taşınmaz; ortak `.stignore-shared` dosyası eşitlenir. Ortak dosya bulunmadan `#include` kullanmayın. `.git` veritabanı da Syncthing dışında kalır; her cihaz kendi Git klonunu kullanır. Ayrıntılar: [Syncthing hariç tutma kuralları](https://docs.syncthing.net/users/ignoring.html).

Temizlik, bilgisayarda kurulu Node/Rust/Visual Studio araçlarını, genel önbellekleri veya WSL/PostgreSQL ortamını kaldırmaz. Kapsam ve mevcut test sınırlamaları [doğrulama raporunda](docs/VALIDATION_0.3.md) bulunur.

## Test ortamı

Mevcut PostgreSQL `18/main:5432` değiştirilmez. Testler Ubuntu-24.04 WSL dağıtımında ayrı `18/termterm:55432` kümesi kullanır.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/lab.ps1 setup
wsl -d Ubuntu-24.04 -u root -- bash /mnt/c/Users/sonx/Desktop/termterm/scripts/migrate-lab.sh
wsl -d Ubuntu-24.04 -u root -- bash /mnt/c/Users/sonx/Desktop/termterm/scripts/ssh-lab.sh
```

Dosya yolu farklıysa WSL yollarını proje konumuna göre değiştirin. PostgreSQL rol parolaları `.lab/connection.json`, SSH fikstürü `.lab/ssh.json` içinde tutulur; `.lab` sürüm kontrolüne veya dağıtım paketine alınmaz. Windows klasör erişimi geçerli kullanıcı ve SYSTEM ile sınırlandırılır. `Settings → PostgreSQL sync → Load WSL test profile` uygulama hesabını yükler. Migration hesapları uygulama profiline taşınmaz.

```powershell
scripts/lab.ps1 status
scripts/lab.ps1 start
scripts/lab.ps1 backup
scripts/lab.ps1 stop
```

WSL'nin oturumlar bittikten sonra kapanmasını önlemek için `start/setup` gizli bir bekletme süreci başlatır. `stop` bu projeye ait süreci durdurur. SSH testi ayrı loopback `22222` hizmetidir; kişisel SSH sunucuları kullanılmaz.

## Doğrulama komutları

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --include-ignored
pnpm test:ui
pnpm test:unit
```

İkinci komut WSL PostgreSQL laboratuvarını gerektirir. Windows WebView2 içinde gerçek native kabul testleri:

```powershell
pnpm tauri build --debug --no-bundle --features e2e --config src-tauri/tauri.e2e.conf.json
pnpm test:native
```

WebDriver yalnızca açıkça seçilen `e2e` debug derlemesinde yüklenir. Dağıtım paketinde test erişimi ve global Tauri nesnesi bulunmaz.

## Yapı

- `src`: React arayüzü; terminal, SFTP, import/export, ayarlar, ekip ve entegrasyon ekranları.
- `src-tauri/src`: şifreli kasa, SSH/proxy/terminal, SFTP, tünel, sync ve entegrasyon servisleri.
- `migrations`: kişisel senkronizasyon PostgreSQL şeması.
- `team-service`: HTTPS servisi, Team şeması, kurulum ve yükseltme araçları.
- `team-client`: masaüstü Team şifrelemesi, yetki denetimi ve çevrimdışı günlük.
- `scripts`: WSL laboratuvarı, Mosh indirme, test ve paketleme betikleri.
- `templates`: örnek dosya import kaynakları.
- `docs`: kullanım, yedekleme, mimari ve doğrulama raporu.

TermTerm markası ve arayüz kaynakları bu projeye aittir; Termius kodu veya özel varlıkları kullanılmamıştır. Üçüncü taraf bağımlılıklar kendi lisanslarını korur. [Bağımlılık notları](docs/THIRD_PARTY.md).
