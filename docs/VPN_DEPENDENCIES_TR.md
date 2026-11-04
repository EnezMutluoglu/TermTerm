# VPN geliştirme paketinin kaynak ve bağımlılık makbuzu

Bu akış yalnız `TermTerm VPN Dev 0.4.3-dev.1` ve
`local.termterm.desktop.vpndev` kimliği için yerel dosya üretir. Kurucu
çalıştırmaz, servis kurmaz ve yayın/güncelleme onayı anlamına gelmez.
`distributionReady` değeri bu makbuzdan dolayı açılmaz.

## Doğrulanan eşleşmeler

`scripts/collect-vpn-runtime.py`, Windows motor klasöründeki 23 DLL'yi
kaynaklarına bağlar: 18 DLL için 15 MSYS2 paketi, 4 DLL için vcpkg
jsoncpp/lz4/OpenSSL ve ayrı düzeltilmiş OpenConnect 9.21 DLL'si. vcpkg
Asio/fmt/xxhash başlık/statik bağımlılıkları da dahil edilir. Asio, vcpkg'in
varsayılan portu yerine sabitlenmiş OpenVPN kaynağındaki gerçek overlay'dir.

MSYS2 ikili arşivi mevcut kilit dosyasının SHA256 değerine uymalıdır; DLL
doğrudan o arşivdeki dosyayla eşleşir. Eşleşen `pkgbase-version.src.tar.zst`
resmî MSYS2 kaynağından alınır. İmza mevcut MSYS2 anahtarlığında tam/güçlü
güvenli bir imzalayana doğrulanır; anahtar eklenmez. Kaynak PKGBUILD SHA256
değeri ikili arşivin `.BUILDINFO` kaydıyla aynı olmalıdır. Bazı GNU
paketlerinde kurulu bildirim bulunmadığından lisans metinleri aynı doğrulanmış
kaynak arşivindeki upstream tar dosyasından alınır.

vcpkg portları sabitlenmiş Git revision'ına ve kurulu paketin SPDX kaynak
dosyası özetlerine uyar. Upstream arşiv ve harici düzeltmelerin SHA512 değerleri
gerçek port tarifinden doğrulanır. Tam vcpkg build kaynağı, OpenVPN Core Git
arşivi, seçilen MPL-2.0 metni, OpenConnect upstream tar dosyası ve üç yerel
düzeltmesi, beş WireGuard Go modülünün sabitlenmiş ZIP'leri ve Go çalışma
zamanı lisansı ayrıca taşınır. Bu dosya eşleştirme kanıtıdır; genel bir
lisans/hukuk uygunluğu garantisi değildir.

Port ve overlay tarifleri bu iki sabitlenmiş Git arşivinde bulunur; yerel
port çalışma dizinleri ayrıca kopyalanmaz. Böylece temiz `git diff` çıktısına
rağmen çalışma dizininde bulunabilecek ignored/untracked özel dosyalar
bağımlılık kaynak paketine alınmaz. Asio overlay'i OpenVPN Core arşivindeki
`deps/vcpkg-ports/asio` yolunda, diğer tarifler vcpkg arşivindeki `ports`
yolundadır.

Uygulama kaynak ZIP'i, izin verilen kod/belge/build köklerinde yalnız
commit edilmiş `HEAD` dosya yollarından ve
`scripts/vpn-public-source-files.json` içindeki tek tek incelenmiş yeni
dosya yollarından üretilir. Untracked, ignored veya yalnız index'e eklenmiş
bir dosya kendiliğinden pakete alınmaz; yeni genel kaynak dosyası açık
listeye eklenmelidir. `.lab`, `.tools`, kasa, özel anahtar ve build çıktıları
bu listeye alınmaz.
Unix kaynak hazırlığı da aynı incelenmiş listeyi ve symlink/junction
denetimini kullanır. Kaynak alt kümesi bu seçicinin betiğini ve açık liste
dosyasını taşır; tekrar hazırlama özgün Git checkout'unda yapılmalıdır.
Her dosyanın özeti ZIP içindeki `SOURCE-RECEIPT.json` ve dış makbuzda tutulur.
Paketleme öncesinde aynı dosyalar tekrar karşılaştırılır; kaynak değişmişse
yeni bağımlılık/kaynak seti hazırlanır.

Kurucuya ve portable ZIP'e kopyalanan lisanslar, belgeler ve Mosh dosyaları
da tek tek doğrulanmış listeden alınır; klasörler kontrolsüz kopyalanmaz.
Bağımlılık, lisans veya Mosh dizinindeki beklenmeyen dosyalar; değişen
dosya özetleri; kaynak, runtime veya çıktı yolundaki symlink/junction/reparse
ataları build ve çıktı dizini oluşturulmadan reddedilir. Dosyası listede
olmayan özel veri okunmaz veya paketlenmez.

## Tekrarlanabilir yerel hazırlık

Önce mevcut Windows hazırlık komutlarıyla sabitlenmiş kaynakları ve yerel
toolchain'i hazırlayın; komutlar global toolchain veya VPN kurmaz:

```powershell
python scripts/prepare-vpn-windows.py --build-core
python scripts/build-openconnect-windows.py
python scripts/build-wireguard.py --target windows --arch amd64
node scripts/fetch-mosh.mjs
python scripts/collect-vpn-runtime.py --fetch-sources --output .tools/vpn-windows/runtime-source-NEW
python scripts/test-vpn-packaging.py
```

OpenConnect build betiği upstream 9.21 kaynağına sıfır fuzz ile
`packet-handle`, `mingw14` ve `gp-sso` düzeltmelerini sırayla uygular.
Configure seçenekleri ve motorun kullandığı dinamik kütüphane sınırı aynı
betikte korunur. Orijinal kaynak ve düzeltmeler bağımlılık arşivinde ayrı
kalır; alıcı kütüphaneyi değiştirebilir ve yeniden derleyebilir. İkili
başlatma özeti değiştiğinde uygulamanın denetlenmiş pin'leri yeniden
incelenmeden paket kabul edilmez.

Motorların CMake derlemesi için
`vpn-engine/windows/CMakeLists.txt`, sabitlenmiş OpenVPN Core ve yerel
OpenConnect include dizini kullanılır. Üretilen motorlar ve gerçek çalışma
zamanı DLL kapanışı `.tools/vpn-windows/engine` içinde olmalıdır; Core
hazırlığının null-TUN probe çıktısı çalışan ürün motoru yerine geçmez.

Mosh hazırlığı mevcut resmî Cygwin indirme akışını kullanır; ikili ve karşılık
gelen kaynak arşivlerini aynı indeksin boyut/SHA512 değerleriyle doğrular.
Her ikili paketin kaynak URL, boyut ve özeti `packages.json` içinde bağlıdır.
Tekrarlanan kaynaklar yalnız bir kez `.tools/cygwin/sources` altında
sıkıştırılmış biçimde tutulur; açılmaz veya derlenmez. `SOURCES.json`, bu
tam kaynak dosyası listesini taşır. VPN kaynak toplayıcısı listeyi yeniden
doğrular, yalnız eşleşen arşivleri `sources/cygwin` altına ekler ve runtime
`packages.json` özetine bağlar. Paketleme başka Mosh runtime'ı seçilirse
yeni kaynak toplama ister.

`runtime-files.json`, yalnız bu
doğrulanmış arşivlerden seçilen runtime/lisans dosyalarını ve üretilen
`packages.json` / `NOTICE.txt` dosyalarını SHA256 ile listeler. Var olan
başka yerel dosya listeye eklenmez veya silinmez; paketleme bu ek dosyaları
görürse durur. Bu makbuz yoksa paketleme pahalı derlemelerden önce
`node scripts/fetch-mosh.mjs` komutunu ister. Mosh kaynak konumları, sabitlenen
kaynak özetleri ve mevcut lisans bildirimleri `packages.json`, `SOURCES.json`
ve `NOTICE.txt` dosyalarında korunur.

Kaynak içindeki doğrudan veya iç içe telif/lisans metinleri de alınır;
`COPYING3`, `COPYING.RUNTIME`, `.tgz` ve gettext'in `.tar.lz` biçimi dahildir.
Lzip için mevcut `tar` yalnız liste/çıktı okur; kaynak kod çalıştırılmaz,
dosya sistemi ağacı açılmaz. Her bildirim 1 MiB, lzip listesi 16 MiB ve
harici okuma 30 saniyeyle sınırlıdır. Bu ek bildirimler kaynak toplayıcının
`notices/cygwin` dizinine girer; aynı doğrulanmış dosya listesi kurucuya da
taşınır.

Kurulum/native kabulünden bağımsız yerel paket hazırlığı ayrı çıktı dizininde
yapılır. Windows UAC ve kurulum testleri beklemedeyken bu komut yalnız dosya üretir:

```powershell
pwsh -NoProfile -File scripts/package-vpn-dev.ps1 -Runtime .tools/vpn-windows/runtime-source-NEW -Output .tools/vpn-windows/package-NEW -WebViewRuntime C:\verified-cache\MicrosoftEdgeWebView2RuntimeInstallerX64.exe -WebViewSha256 <verified-sha256>
```

Betik yeni çıktı dizininde özellik açılmamış release broker'ını ve yalnız
VPN Dev uygulamasını derler; test sürücüsü istemez. Kaynak ve executable
özetini koruyarak Tauri'nin yeniden derleme yapmayan `bundle` komutuyla
ayrı current-user NSIS kurucusu ve portable ZIP üretir. Yardımcılar manifestin
beklediği gibi executable yanında kalır. Kararlı uygulama kimliği, kısayolu,
kullanıcı verisi ve güncelleme kanalı kullanılmaz. Windows paket katmanı
`tauri.vpn-windows-package.conf.json` ile `termterm-vpn-dev.exe` adını zorunlu
kılar; NSIS'in süreç kontrolü kararlı uygulamanın `termterm.exe` adına gitmez.
Portable ve NSIS için ayrı manifestler oluşturulur: Tauri'nin installer içindeki
`UNK` → `NSS` bundle işaretçisi değişikliği installer manifestine önceden tam
olarak yansıtılır ve gerçek arşivden executable/manifest baytları doğrulanır.
Portable ZIP, SHA256 ve geçerli Microsoft yayıncı imzası denetlenen offline
WebView2 kurucusunu `prerequisites` altında ve açık kurulum notunu içerir.
İsteğe bağlı `-BuildCache .tools/vpn-windows/package-PREVIOUS` yalnız önceki
dev derleme önbelleğini kopyalar; yeni kaynak yeniden derlenir, eski paket korunur.
Eski `package-20261030-v1` çıktısı executable adı ve NSIS manifest uyuşmazlığı
nedeniyle reddedilmiştir; kullanılabilir kurucu olarak dağıtılmaz.

Üretilen kurucu burada çalıştırılmaz. Ayrı disposable Windows ortamında
kurulum/açılış/yeniden başlatma/kaldırma, paketli broker ile kişisel ve Team
native bağlantı kabulü, Windows cleanup ve tam rota/DNS/journal temizliği
ayrıca doğrulanmalıdır. macOS ikilisi, imza/notarizasyon ve gerçek macOS
ağ/WebKit testleri bu Windows hazırlığından çıkarılamaz.

## Ayrı Linux hazırlığı

Linux için eski 0.5 veya Team build root'u kullanılmaz. Yeni
`/root/termterm-vpn-desktop/jammy`, Ubuntu'nun imzalı arşivinden minbase
olarak hazırlanır. `/usr/sbin/policy-rc.d` yalnız `exit 101` içerir;
bağımlılık kurulumu bu guest içinde kalır. Host servisleri, rotalar veya
DNS ayarları değiştirilmez. Derleme araçları yalnız doğrulanmış genel
arşivlerden kurulur; host `.cargo`/`.rustup` özel ayarları kopyalanmaz.

Doğrulanan araçlar Node 22.23.0, pnpm 11.19.0, Rust/Cargo 1.98.1 ve Go
1.26.4'tür. Go için guest içinde `GOROOT=/opt/go`, `GOTOOLCHAIN=local` ve
`GOTELEMETRY=off` kullanılır; telemetry için host `/proc` bağlanmaz.
Araç ve vendor girdi arşivleri tek tek SHA makbuzuyla
`.tools/vpn-linux-inputs/<hazırlık>/INPUTS.json` içinde sabitlenir.

`scripts/build-vpn-linux.py` yalnız bu işaretli guest'i ve makbuzdaki tam
dosya kümesini kabul eder. Yeni bir `--run` adı önceki build/log'u korur:

```sh
python3 scripts/build-vpn-linux.py \
  --inputs .tools/vpn-linux-inputs/v1 --run vendor-NEW
```

Komut WSL'de depo kökünden çalıştırılır. Sabitlenmiş OpenVPN Core,
altı upstream Asio düzeltmesi, lz4/fmt port düzeltmeleri ve OpenConnect 9.21
GP SSO düzeltmesi guest içinde derlenir. Ubuntu 24.04 kütüphaneleri
taşınmaz. Sonuç `/opt/termterm-vpn-dev/bin` altında üç packet engine'dir;
dinamik kapanış, SHA256 ve native test yapılmadığına ilişkin alanlar
`/opt/termterm-vpn-build/<run>/ENGINE-RECEIPT.json` içinde tutulur.
Bu komut broker, politika executable'ı, masaüstü veya kurucu üretmez;
VPN bağlantısı açmaz. WireGuard birim testleri ve `vet` derleme sırasında
çalışır, çekirdek VPN kabulünün yerine geçmez.

Masaüstü kaynakları dondurulduktan sonra yalnız incelenmiş kaynak ZIP'i
bu guest'e alınır. Normal Linux overlay'i yerine iki ayrı config kullanılır:

```sh
unset VITE_E2E
export TERMTERM_RELEASE_CHANNEL=development
export CARGO_TARGET_DIR=/opt/termterm-vpn-build/desktop-target
pnpm install --frozen-lockfile
pnpm exec tauri build --no-bundle \
  --config src-tauri/tauri.vpn-dev.conf.json \
  --config src-tauri/tauri.vpn-linux-package.conf.json
```

Linux overlay'i `termterm-vpn-dev` executable'ını, ayrı VPN Dev kimliğini,
yalnız DEB/AppImage hedeflerini ve kapalı updater'ı seçer. Bundle aşaması,
önce ayrıca doğrulanmış Mosh/lisans/belge dosyalarını tek tek gösteren kaynak
haritası ister; boş veya kontrolsüz dizin haritasıyla paket kabul edilmez.
Üretilen DEB'de `Package` kararlı `termterm` olmamalı, `Conflicts`/`Replaces`
boş olmalı, `/usr/bin/termterm-vpn-dev` bulunmalı ve `/usr/bin/termterm`
bulunmamalıdır. Kısayol/ikon/veri yolları da kararlı paketle çakışmamalıdır.
Bu kontrollerden önce kurucu kurulmaz. Broker rootfs'i ayrı
`prepare-vpn-unix.py` akışıyla hazırlanır; Python 3.10 gerçek kontrolü,
root sahipliği, istemci pin'i, runtime kaynakları ve native servis kabulü
tamamlanmadan dağıtım hazır sayılmaz.
