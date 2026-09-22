# Platform derlemeleri — 0.3.3-dev.1

Hedefler Windows x64, Linux x64, macOS Intel/Apple Silicon. Node 22.12+ (CI 24), pnpm 11.19+ ve Rust 1.98.1 kullanılır. Normal derleme geliştirme kanalındadır; stable yayın için sahibinin ayrıca onayı ve [yayın akışı](RELEASE_PROCESS.md) gerekir. Workflow yalnızca elle çalışır, release yayımlamaz.

## Windows

Visual Studio C++ Build Tools, Windows SDK ve WebView2 gerekir.

```powershell
pnpm install --frozen-lockfile
node scripts/fetch-mosh.mjs
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
powershell -ExecutionPolicy Bypass -File scripts/verify-package.ps1
```

`artifacts/release-0.3.3-dev.1` altında NSIS, portable ZIP ve kaynak ZIP oluşur. NSIS ve portable paket WebView2 x64 çevrimdışı kurucusunu içerir. Portable ZIP'in tamamını çıkarın; Mosh/DLL/lisans dosyalarını yanında tutun. Authenticode yayıncı imzası yoktur. Geliştirme paketi güncelleme akışına yüklenmez.

## Linux x64

Ubuntu 22.04 derleme tabanı kullanılır. `.deb` Debian/Ubuntu ailesi, `.rpm` uygun Fedora/openSUSE türevleri, AppImage diğer uyumlu glibc masaüstleri içindir. Her dağıtımda çalıştığı iddia edilmez: ARM, Alpine/musl, eski glibc veya eksik WebKitGTK 4.1 sistemleri bu x64 hedefinin dışındadır.

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libwebkit2gtk-4.1-dev libssl-dev \
  libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libudev-dev libdbus-1-dev \
  patchelf mosh xvfb dbus-x11 libfuse2
pnpm install --frozen-lockfile
bash scripts/build.sh package
```

Ubuntu 24.04'te `libfuse2t64` kullanılır. Üç paket Tauri bundle dizininde üretilir; doğrulama betiği hepsini kontrol edip artifacts klasörüne kopyalar. Mosh yardımcı süreç ve kütüphaneleri pakette bulunur; sistem glibc'si taşınmaz. Dağıtım bağımlılıklarının kurulumu internet gerektirebilir.

```bash
sudo apt install ./TermTerm_0.3.3-dev.1_amd64.deb
# RPM dağıtımında:
sudo dnf install ./TermTerm-0.3.3-dev.1-1.x86_64.rpm
# AppImage:
chmod +x TermTerm_0.3.3-dev.1_amd64.AppImage
./TermTerm_0.3.3-dev.1_amd64.AppImage
# FUSE yoksa:
./TermTerm_0.3.3-dev.1_amd64.AppImage --appimage-extract-and-run
```

Yerel terminal kullanıcı shell'ini açar. Parola hatırlama kilidi açık Secret Service gerektirir; yoksa parola ile kasa açılabilir. PostgreSQL isteğe bağlıdır. WSL testi masaüstü keyring veya bütün dağıtımlarda doğrulama anlamına gelmez.

## macOS: kaynak hazır, derleme Mac üzerinde

Intel Mac'te x86_64, Apple Silicon Mac'te arm64 paketi üretilir. Her mimari için ayrı kaynak kopyası/runner kullanın; Mosh dosyalarını mimariler arasında karıştırmayın. Xcode Command Line Tools, Homebrew, Node/pnpm ve Rust gerekir.

```bash
xcode-select --install # zaten kuruluysa gerekmez
brew install mosh
pnpm install --frozen-lockfile
bash scripts/build.sh package
```

Betik Mosh/dylib hazırlığını yapar, `.app` ve DMG üretir. Doğrulama `codesign --verify --deep --strict` çalıştırır, `.app.zip` oluşturur; DMG ve ZIP artifacts klasörüne alınır. Minimum sistem 11.0; imza ad-hoc'tur. Apple Developer sertifikası/notarization yapılandırılmamıştır. Kullanıcı kararıyla Mac derlemesi bekler; yapılandırmanın hazır olması macOS testinin geçtiği anlamına gelmez.

`.github/workflows/desktop.yml` içinde `macos-15-intel` ve `macos-15` runner'ları hazırdır. **Run workflow** ile başlatılır; yayın yapmaz. Kaynak ZIP'i Mac'e taşıyıp yukarıdaki komutları çalıştırabilirsiniz.

## Testler

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
pnpm test:unit
pnpm exec playwright install chromium
pnpm test:ui
pnpm tauri build --debug --features e2e --config src-tauri/tauri.e2e.conf.json --no-bundle
```

Native platform smoke: `TERMTERM_SPEC=./tests/desktop/platform-smoke.mjs pnpm test:native` (PowerShell'de `$env:TERMTERM_SPEC=...`). Linux'ta `dbus-run-session -- xvfb-run -a pnpm test:native`. E2E uygulama kimliği ayrıdır; normal paketlerde WebDriver özelliği bulunmaz.

Gerçek SSH/pano testi Windows'ta `tests/desktop/terminal-usability.mjs`; yerel `.lab/ssh.json` veya `TERMTERM_SSH_FIXTURE` gerektirir. Fixture kaynak arşivine alınmaz. Embedded sürücünün Insert/contextmenu/pointer eksikleri testte açıkça tamamlanır; gerçek tuş baytları SSH sunucusunda, pano Windows TextBox ile doğrulanır. Tarayıcı testleri gerçek browser input olaylarını ayrıca sınar. Donanım klavyesi/OS DPI/macOS sonuçları bu otomasyonla eş tutulmaz.

WSL root headless testinde süreç bazlı `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` gerekebilir; uygulama/paket bunu ayarlamaz. Günlük kullanım normal masaüstü kullanıcısıyladır. Uzun süreli 30 dakika izleme yapılmaz.
