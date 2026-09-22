# TermTerm platform derlemeleri

Hedefler Windows x64, Linux x64, macOS Intel ve Apple Silicon'dur. Node 22.12+ (CI: 24), pnpm 11.19 ve Rust 1.98.1 kullanılır. Kasa ve yedek dosya biçimi 0.1 ile aynıdır; grafik geçmişi yalnızca RAM'de tutulur.

## Windows

Visual Studio C++ Build Tools, Windows SDK ve WebView2 gerekir.

```powershell
pnpm install --frozen-lockfile
node scripts/fetch-mosh.mjs
scripts/build.ps1 check
scripts/package.ps1
scripts/verify-package.ps1
```

NSIS ve taşınabilir ZIP, proje sürümüne göre `artifacts/release-0.3.2` içine yazılır. Eski teslimat klasörlerine yazılmaz. Yukarıdaki normal komutlar geliştirme kanalı üretir. Sahibinin onayladığı kararlı sürümü ve güncelleme imzasını üretmek için [yayın kılavuzunu](RELEASE_PROCESS.md) izleyin. Windows Authenticode yayıncı sertifikası yapılandırılmamıştır; güncelleme imzası ayrı bir kontroldür. Taşınabilir paketin tamamını çıkarın; Mosh helper ve DLL dosyalarını exe yanında bırakın.

## Linux

Aşağıdaki paket örnekleri daha önce doğrulanan 0.3.1 Linux teslimatına aittir; 0.3.2 Windows düzeltme yayınına yeni Linux paketi eklenmemiştir.

Dağıtım tabanı Ubuntu 22.04'tür. Teslim edilen paketler WSL Ubuntu 24.04 içinde ayrı Ubuntu 22.04 chroot ortamında derlendi ve çalıştırıldı. Uygulamanın GLIBC gereksinimi en fazla 2.34 olarak doğrulandı. PostgreSQL/SSH laboratuvarı dıştaki Ubuntu 24.04 dağıtımında kalır.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libwebkit2gtk-4.1-dev libssl-dev \
  libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libudev-dev libdbus-1-dev \
  patchelf mosh xvfb dbus-x11 libfuse2
pnpm install --frozen-lockfile
bash scripts/build.sh package
```

Ubuntu 24.04'te `libfuse2` yerine `libfuse2t64` kullanılır. AppImage ve `.deb` Tauri bundle dizininde üretilir, doğrulanan paketler proje sürümüne göre `artifacts/release-0.3.1` dizinine kopyalanır. Mosh kütüphaneleri ayrı helper altında paketlenir; sistem glibc'si taşınmaz.

Yalnızca Debian paketi üretmek için sistem bağımlılıklarını kurduktan sonra:

```bash
pnpm install --frozen-lockfile
bash scripts/prepare-mosh.sh
pnpm tauri build --bundles deb
dpkg-deb --info src-tauri/target/release/bundle/deb/TermTerm_0.3.1_amd64.deb
```

Bu komutun çıktısı `src-tauri/target/release/bundle/deb/TermTerm_0.3.1_amd64.deb` olur. `verify-native-package.mjs` tam Linux teslimatı için hem AppImage hem `.deb` arar; yalnızca `.deb` derlemesinde paket ayrıca kurularak ve bağımlılıkları kontrol edilerek doğrulanır. `.deb` kullanımı FUSE gerektirmez.

Yerel terminal `$SHELL` veya kullanıcı hesabının login shell'iyle açılır. Parola hatırlama çalışan ve kilidi açık Secret Service gerektirir; yoksa parola ile açma kullanılabilir. Headless WSL testinde masaüstü keyring entegrasyonu doğrulanmış sayılmaz.

## macOS

Her mimari kendi runner'ında derlenir. Intel: `macos-15-intel`; Apple Silicon: `macos-15`. Xcode Command Line Tools ve Homebrew gerekir.

```bash
brew install mosh
pnpm install --frozen-lockfile
bash scripts/build.sh package
```

`.app` ve DMG ayrı mimarilerde üretilir. Mosh ve Homebrew dylib'leri paket içindeki göreli konumlara bağlanır ve ad-hoc imzalanır. Apple Developer sertifikası/notarization bu yapılandırmaya dahil değildir. macOS derlemesi ve gerçek ölçüm testi yerelde çalıştırılmamıştır; CI'ın çalışması gerekir.

## Native test derlemesi

```bash
pnpm tauri build --debug --features e2e --config src-tauri/tauri.e2e.conf.json --no-bundle
```

E2E yapılandırması ayrı `local.termterm.desktop.e2e` uygulama kimliği ve yalnızca test derlemesine eklenen JavaScript/Rust WebDriver parçaları kullanır. Normal `pnpm tauri build` test özelliklerini içermez. Üretim paketleriyle `--features e2e` kullanılmaz. Test sunucuları loopback içindir.

Windows/macOS:

```powershell
$env:TERMTERM_SPEC='./tests/desktop/platform-smoke.mjs'
pnpm test:native
```

Linux:

```bash
TERMTERM_SPEC=./tests/desktop/platform-smoke.mjs dbus-run-session -- xvfb-run -a pnpm test:native
```

WSL root hesabıyla yapılan headless testte WebKit sandbox için test sürecine özel `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` kullanıldı. Uygulama/paket bu değişkeni ayarlamaz; günlük kullanımda normal kullanıcı hesabı kullanılır.

Gerçek SSH laboratuvarı gerektiren Windows regresyonu: `tests/desktop/workspace.mjs`. Kaynak göstergesi/tuş baytları/16 panel testi: `tests/desktop/metrics.mjs`; kısa kontrol için `TERMTERM_SOAK_SECONDS=30` kullanılır. Daha uzun süre isteğe bağlıdır; bu teslimatta kullanıcı isteğiyle 30 dakikalık izleme yapılmadı. `.github/workflows/desktop.yml` dört hedefte birim, arayüz, native smoke, release ve paket kontrollerini tanımlar. Bu klasör Git remote'a bağlı olmadığından workflow burada gönderilmedi veya uzaktan çalıştırılmadı.


## 0.3 teslimat kararı

macOS Intel ve Apple Silicon derlemeleri 21 Eylül 2026 tarihinde kullanıcı isteğiyle bekletilmiştir. Kaynak arşivinde iki Tauri hedefi, Homebrew Mosh/dylib hazırlığı, ad-hoc imzalama, test komutları ve CI matrisi bulunur. Mac üzerinde derlenip çalıştırılmadan macOS paketi doğrulandı sayılmaz.

Windows NSIS paketi WebView2 x64 çevrimdışı kurucusunu içerir. Windows portable dağıtımında `prerequisites` klasöründeki aynı çalışma zamanı kurucusu gerekirse bir kez çalıştırılır. PostgreSQL kişisel kullanım için zorunlu değildir. WSL betikleri yalnızca isteğe bağlı geliştirme/test sunucusunu hazırlar.

Linux üretim derlemesi Ubuntu 22.04 tabanında hazırlanır. AppImage için masaüstü oturumu ve FUSE2 gerekir; FUSE yoksa `--appimage-extract-and-run` kullanılabilir. `.deb` paketini `sudo apt install ./TermTerm_0.3.1_amd64.deb` ile kurmak sistem bağımlılıklarını çözümleyerek yükler; eksik sistem paketleri için internet bağlantısı gerekir.
