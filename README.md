# TermTerm

Tauri 2, Rust ve React ile geliştirilmiş kişisel SSH çalışma alanı. Windows x64, Linux x64 ve macOS Intel/Apple Silicon derleme hedefleri vardır. PostgreSQL olmadan çalışır. Bağlantılar, kimlikler ve oturum kayıtları parola korumalı `.ttvault` dosyasında tutulur.

**Sürüm: 0.3.4.** Terminal başına canlı CPU/RAM/disk göstergesi, terminal temaları, klasör içinde host görünümü ve geliştirilmiş import adaptörleri içerir. 0.1 kasa/yedek biçimi korunur. Termius düzeninden esinlenen bağımsız uygulamadır. Planın tamamıyla özellik eşitliği henüz sağlanmış değildir. Özellikle doğrudan Windows Hello/FIDO2 anahtar oluşturma ve bazı üreticiye özel şifreli import biçimleri tamamlanmamıştır. Ayrıntılar: [0.3.4 doğrulama raporu](docs/VALIDATION_0.3.4.md), [platform derlemeleri](docs/BUILD_PLATFORMS.md), [import kapsamı](docs/IMPORT_SUPPORT.md), [PostgreSQL tablo ve kurulum akışı](docs/POSTGRES_FLOW.md).

## 0.3.4 ve güncellemeler

**0.3.4** kasa parolasını hatırlama hatasını düzeltir ve varsayılan Türkçe arayüzü getirir. Kayıtlı parola açılışta otomatik kullanılır; elle kilitleme korunur. Önceki sürümdeki yumuşak antrasit–mavi kontroller, üst sırada bağlantı sekmeleri, altta terminal araçları, seçimle kopyalama, sağ tık/Shift+Insert yapıştırma ve doğrudan Updates bağlantısını içerir. Host ve klasörler tek tıkla bilgileri gösterir; düzenleme, taşıma, çoğaltma ve silme sağ tık menüsündedir. Çift tık klasörü açar veya bağlantıyı başlatır. SFTP dosya işlemleri de sağ tık menüsündedir. Windows x64 ve Linux x64 (DEB, RPM, AppImage) paketleri [sürüm sayfasında](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.3.4) bulunur. macOS Intel/Apple Silicon derleme yapılandırması hazırdır; sahibinin isteğiyle Mac CI işleri durdurulmuştur ve bu sürümde Mac paketi yayımlanmaz. [Derleme komutları](docs/BUILD_PLATFORMS.md).

RSA 2048/3072/4096, Ed25519 ve ECDSA anahtar uyumluluğu ile kayıtlı sunucu anahtarı seçimi düzeltmeleri korunur. **Sol alt Güncellemeler → Güncellemeleri denetle** veya **Ayarlar → Güncellemeler** ile onaylanmış yeni sürümler kontrol edilir. Windows ve Linux AppImage kurulmadan önce güncelleme imzasını doğrular. DEB/RPM için aynı sürümden uygun paket indirilip paket yöneticisiyle kurulur. Güncelleme kullanıcı tarafından başlatılır. Geliştirme dalı `develop`, onaylanan sürümler `main` üzerindedir. 0.3.1 ve geliştirme kanalı kullanıcıları kararlı paketi bir kez elle kurmalıdır; Windows 0.3.2 güncelleme desteği içerir. Ayrıntılar: [yayın akışı](docs/RELEASE_PROCESS.md).

## Çalıştırma

0.3.1 klasör düzeltmesi: Hosts ana ekranı yalnızca klasörleri gösterir. Klasörsüz kayıtlar `Ungrouped` sanal klasöründen açılır; kayıtların gerçek grup ilişkileri değiştirilmez. Klasör kartları ve sol menü, alt klasörleri de içeren toplam host sayısını gösterir. Bu sürümün kontrol sonuçları [0.3.1 raporunda](docs/VALIDATION_0.3.1.md) bulunur.

Windows için [0.3.4 sürüm sayfasındaki](https://github.com/EnezMutluoglu/TermTerm/releases/tag/v0.3.4) `TermTerm_0.3.4_x64-setup.exe` kurucusunu kullanın. Kurucu WebView2 çevrimdışı önkoşulunu ve Mosh çalışma dosyalarını içerir. Taşınabilir ZIP'i tercih ederseniz **tamamını** aynı klasöre çıkarıp `TermTerm.exe` çalıştırın. Kaynak deposu büyük kurucuları içermez.

Debian/Ubuntu paketini `sudo apt install ./TermTerm_0.3.4_amd64.deb`, uygun RPM dağıtımında `sudo dnf install ./TermTerm-0.3.4-1.x86_64.rpm` ile kurun. AppImage dosyasına çalıştırma izni verip açın. Eksik sistem bağımlılıkları paket yöneticisi üzerinden indirilir. Linux tabanı glibc 2.34+ ve WebKitGTK 4.1'dir; her dağıtımda test edildiği anlamına gelmez. Platform sınırları ve gerçek testler [doğrulama raporunda](docs/VALIDATION_0.3.4.md) yer alır.

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
- `migrations`: RLS, revizyonlar, üyelik ve ortak terminal PostgreSQL şeması.
- `scripts`: WSL laboratuvarı, Mosh indirme, test ve paketleme betikleri.
- `templates`: örnek dosya import kaynakları.
- `docs`: kullanım, yedekleme, mimari ve doğrulama raporu.

TermTerm markası ve arayüz kaynakları bu projeye aittir; Termius kodu veya özel varlıkları kullanılmamıştır. Üçüncü taraf bağımlılıklar kendi lisanslarını korur. [Bağımlılık notları](docs/THIRD_PARTY.md).
