# Linux / macOS VPN geliştirme yardımcısı

Bu yardımcı yalnız `local.termterm.desktop.vpndev` geliştirme uygulaması içindir. Servis kimliği `local.termterm.desktop.vpndev.broker` olur. Kurulu kararlı TermTerm, kararlı güncelleme kanalı, kullanıcı kasaları ve diğer VPN servisleri bu hazırlığın kapsamı dışındadır. Hazırlama komutu kurulum veya servis başlatma yapmaz.

## Güven sınırı

Linux servis soketi `/run/termterm-vpn-dev/broker.sock`, ayarı `/etc/termterm-vpn-dev/broker.json`, sahiplik günlüğü `/var/lib/termterm-vpn-dev/owned.json` konumundadır. Servis root olarak çalışır; istemci için yapılandırılmış kullanıcı kimliği, çekirdek `SO_PEERCRED`/`pidfd`, çalışan dosyanın yolu ve SHA-256 özeti birlikte doğrulanır. Aynı kullanıcıya ait farklı bir program kabul edilmez. İstemci komut, çalıştırılacak dosya veya sistem yolu gönderemez.

macOS kaynakları, soketin denetim belirtecini (`LOCAL_PEERTOKEN`) kullanarak Security.framework üzerinden çalışan kodun imzasını, ayrı geliştirme uygulaması kimliğini ve yönetici tarafından seçilmiş designated requirement değerini doğrular. `kqueue` gerçek istemci sürecinin ömrünü izler. macOS ayarı `/Library/Application Support/TermTerm VPN Dev/broker.json`, yardımcıları `/Library/PrivilegedHelperTools/local.termterm.desktop.vpndev.broker/` altındadır.

macOS launchd şablonu sistemde `/usr/bin/python3` bulunduğunu varsaymaz. İkili hazırlıkta `--python-runtime <kanonik-mutlak-python-yolu>` zorunludur; root sahibi ve grup/diğer kullanıcılarca yazılamayan çalıştırıcı ile bütün üst dizinleri, sürüm sorgusu çalıştırılmadan önce denetlenir. Seçilen Python en az 3.10 olmalı; yolu ve SHA-256 özeti ayara yazılır ve broker başlangıcında yeniden doğrulanır. Standart kitaplık, framework ve dinamik bağımlılıkların güvenliği yalnız çalıştırıcı özetiyle kanıtlanmaz; bu bağımlılıklar ayrıca root sahipli, yazılamaz ve yerel olarak doğrulanmış olmalıdır. Kaynak paketi bunları içermez. Kaynak-only plist içindeki `/REQUIRES_VERIFIED_PYTHON_3_10_OR_NEWER/python3` yer tutucusu çözülmeden servis başlatılamaz.

İstemci, yardımcılar, Python çalıştırıcısı ve yüklenen bütün dinamik kütüphaneler root sahipli ve kullanıcılarca değiştirilemez olmalıdır. Kullanıcı tarafından yazılabilen Homebrew kitaplıklarını root servisine bağlamak güvenli değildir; macOS paketi bunları imzalı, root sahipli yardımcı paketine alıp bağımlılık yollarını doğrulamadan hazır sayılmaz. Hazırlama manifesti bu kontrolü tamamlanmış göstermez.

## Ağ ve kapanış

Protokol 2, bağlantı nesliyle bağlı arayüz makbuzu ve `quiesceRequired` / `quiesced` kapanış el sıkışması sağlar. TermTerm'e bağlı SSH/SFTP/tünel soketleri işletim sistemi arayüzüne sabitlenir; gerekli VPN makbuzu yoksa bağlantı açılmaz. Bu mekanizma diğer uygulamalar için sistem çapında bir kill-switch değildir.

En fazla üç bağlantı açılır. Örtüşen rotalar, başka bir VPN'e giden taşıma uçları ve belirsiz DNS politikaları reddedilir. Hariç tutulan ağlar ile taşıma uçları etkin tünel rotalarından çıkarılır. Mevcut rotalar değiştirilmez. Ağ değişikliklerinin niyeti önce özel sahiplik günlüğüne kalıcı yazılır. Temizlik yalnız kanıtlanan kendi arayüzünü veya tam eşleşen kendi DNS anahtarını kapsar; adı benzer yabancı arayüzler silinmez. Eksik kapanış onayı yeni bağlantıları engeller. Kontrol soketinin kapanması, istemci sürecinin öldüğünün kanıtı sayılmaz.

Linux DNS için systemd-resolved ve `/run/systemd/resolve/stub-resolv.conf` bağlantısı gereklidir. DNS arayüze `resolvectl` ile uygulanır; `/etc/resolv.conf` üzerine yazılmaz. Linux'ta `searchDomain` sonekleri kısa-ad araması için düz alan adı olarak, yalnız `dnsDomains` içinde bulunan alanlar `~alan` biçiminde yalnız DNS yönlendirmesi için uygulanır. Arama sırası korunur; aynı alan iki biçimde eklenmez. İkisinin birleşimi paylaşılan Rust çakışma politikasıyla kontrol edilir. macOS kaynağı yalnız kendi `State:/Network/Service/local.termterm.desktop.vpndev.<oturum>/DNS` anahtarını kullanır ve silmeden önce tam eşleşmeyi doğrular. Gelişmiş DNS taşıma/DNSSEC seçenekleri, proxy/PAC ve IPv6 şu sürümde reddedilir. Linux'ta otomatik IPv6 kontrol paketlerini engellemek için yalnız kendi IPv4 TUN arayüzünün `disable_ipv6` değeri arayüz açılmadan önce ayarlanır; genel sistem ayarı değiştirilmez.

macOS DNS kaynağı arama soneklerini sırasıyla `SearchDomains` alanında, yönlendirme alanlarını `SupplementalMatchDomains` alanında tutar; `SupplementalMatchDomainsNoSearch=1` yalnız yönlendirme amaçlı alanların arama listesine eklenmesini önler ([Apple DNS anahtarları](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.vpn.managed.yaml)). Temizlik yalnız kendi güncel DNS sözlüğünün veya önceki sürümün tam eşleşen kendi sözlüğünün silinmesine izin verir; yabancı değişiklik korunur. Bu SystemConfiguration davranışı gerçek Mac üzerinde henüz doğrulanmadı.

**Bilinen macOS kaynak eksiği:** Linux'taki doğrulanmış fiziksel bağlı ağ istisnası Darwin rota ön kontrolünde henüz uygulanmadı. Fiziksel LAN ile örtüşen tam tünel planı, sunucunun açık istisnaları bu örtüşmeyi kaldırmadıkça `existing_route_conflict` ile ağ değişmeden reddedilir. Bu yalnız test bekleyen bir davranış değildir; macOS tam tünel hazırlığı bu kaynak eksiği giderilmeden tamamlanmış sayılmaz. Arayüz adına güvenen veya mevcut yerel/komşu/statik rotaları genel olarak izinli sayan bir gevşetme eklenmedi.

4 Kasım kaynak incelemesinde bu sınır korundu: [Apple netstat belgesi](https://github.com/apple-oss-distributions/network_cmds/blob/main/netstat.tproj/netstat.1) `C`/`W` işaretlerinin cloning/klonlanmış rota, `S` işaretinin statik rota olduğunu açıklar; [route belgesi](https://github.com/apple-oss-distributions/network_cmds/blob/main/route.tproj/route.8) ise `-cloning`, `-iface`, `-llinfo` ve `-nostatic` ile elle rota işaretlerinin değiştirilebildiğini gösterir. Bu nedenle yalnız `UCS`, `UHLWI`, `link#` veya `en0` görünümü fiziksel bağlı önek kanıtı değildir. Gerçek arayüz tipi/adres öneki ve bağlı/yerel/komşu rota ilişkisi birlikte doğrulanmadan Linux istisnası Darwin'e taşınmadı. Linux bridge/bond/VLAN için de alt arayüz zinciri doğrulanmış değildir; Ethernet gibi görünmesi bu türlere izin açmaz.

`python3 scripts/test-vpn-unix-service.py --policy-checks` root veya çalışan broker gerektirmeyen saf kontrolleri çalıştırır. Komut, işletim sistemi komutlarını fixture ile değiştirir; rota/DNS/ağ değişikliği veya servis kurulumu yapmaz. Darwin bağlı/komşu/statik/yabancı VPN rotalarında güvenli reddi, açık LAN istisnasının yalnız o örtüşmeyi kaldırmasını, Linux sınıflandırma ve DNS çakışma engellerini sınar. Bu kaynak testi macOS derlemesi veya gerçek Mac kabulü değildir; yukarıdaki kaynak eksiği ve yerel kabul kapısı açık kalır.

## Derleme ve paket hazırlığı

Linux laboratuvarı Ubuntu 24.04, Python 3.12, OpenConnect `9.12-1ubuntu1.24.04.1`, OpenVPN sunucusu `2.6.19-0ubuntu0.24.04.4`, ocserv `1.2.4-1build2` ve `vpn-engine/dependencies.json` içindeki sabitlenmiş OpenVPN Core kaynağıyla doğrulanır. WireGuard Go sürüm ve kaynak sabitlemeleri `docs/VPN_WIREGUARD_TR.md` içindedir. GlobalProtect SSO için gönderilen OpenConnect sürümünün ilgili SSO düzeltmeleri ayrıca bulunmalıdır; dağıtım manifesti laboratuvar 9.12 paketini bu koşul için doğrulanmış saymaz.

```sh
cmake -S vpn-engine -B /root/termterm-vpn-native-build/engine -G Ninja \
  -DOPENVPN_CORE=/root/termterm-vpn-build/openvpn3 -DCMAKE_BUILD_TYPE=Release
cmake --build /root/termterm-vpn-native-build/engine -j2
cargo build --locked --manifest-path vpn-core/Cargo.toml --bin vpn-policy-lab \
  --target-dir /root/termterm-vpn-native-build/core
install -m 0755 /root/termterm-vpn-native-build/core/debug/vpn-policy-lab \
  /root/termterm-vpn-native-build/engine/vpn-policy-lab
python3 scripts/build-wireguard.py --target linux --arch amd64 \
  --output .tools/vpn-wireguard-unix/termterm-wireguard
install -m 0755 .tools/vpn-wireguard-unix/termterm-wireguard \
  /root/termterm-vpn-native-build/engine/termterm-wireguard
python3 scripts/prepare-vpn-unix.py --platform linux --source-only \
  --output .tools/vpn-unix-linux-source
python3 scripts/prepare-vpn-unix.py --platform macos --source-only \
  --output .tools/vpn-unix-macos-source
```

İkili Linux hazırlığı için `--source-only` yerine `--engines <derleme-klasörü> --client <root-sahipli-ayrı-geliştirme-uygulaması> --uid <kullanıcı-uid>` verilir. macOS buna ek olarak imzalı geliştirme uygulamasının `--client-requirement` ve doğrulanmış `--python-runtime` değerlerini ister. macOS staging ağacı `/private/var/run/termterm-vpn-dev` soket dizinini 0755, özel ayar ve `journal` dizinlerini 0700 moduyla hazırlar; yönetici bunları root sahipli kurmalıdır. `/var/run` bağlantısı üzerinden dizin yaratılmaz ve istemciye dizin oluşturma yetkisi verilmez. Çıktı yalnız yeni bir `.tools` dizininde oluşturulur; mevcut dosya üzerine yazılmaz. Kaynak paketi servis olarak başlatılamaz. Kaynak çıktısı Unix yardımcılarını, politika çekirdeğini, WireGuard kaynağını, lisans bildirimlerini ve masaüstü SSO parçalarını içerir; tam masaüstü derlemesi depo kaynağını ve sabitlenmiş harici bağımlılıkları ayrıca gerektirir.

Apple Silicon için CMake'e `-DCMAKE_OSX_ARCHITECTURES=arm64`, Intel için `-DCMAKE_OSX_ARCHITECTURES=x86_64` verilir; OpenConnect, JsonCpp, OpenVPN Core ve kripto bağımlılıklarının her biri aynı mimaride ayrıca sağlanmalıdır. İki mimari aynı paket için kendiliğinden birleştirilmez. WireGuard karşılıkları `scripts/build-wireguard.py --target darwin --arch arm64` ve `--arch amd64`, Tauri hedefleri `aarch64-apple-darwin` ve `x86_64-apple-darwin` olur. İzole geliştirme kimliği için her Tauri derlemesinde `tauri.vpn-dev.conf.json` kullanılmalıdır. Go WireGuard motoru bu Windows ortamında her iki Darwin mimarisi için derlenebilir; bu kontrol Apple SDK, C++/Objective-C bağlantısı, kod imzası, kurulum veya çalışma zamanı testi değildir.

Yönetici kurulumu ancak manifestteki bağımlılık ve yerel test kapıları tamamlandıktan sonra yapılmalıdır. Linux için üretilen rootfs dosyaları root sahipli ve uygun izinlerle kendi kesin hedeflerine kurulup yalnız `termterm-vpn-dev.service` etkinleştirilir. macOS şablonu ayrı launchd LaunchDaemon içindir; Python çalışma zamanı ve kod imzalama/notarizasyonu yerel makinede doğrulanmalıdır. Bu çalışma `SMAppService` kaydını veya kullanıcı onay akışını uygulanmış/test edilmiş göstermez.

Linux hazırlama manifestinin `linuxRuntime` alanı Ubuntu 24.04 için gereken çalışma zamanı paketlerini listeler: `python3`, `iproute2`, `systemd-resolved`, `binutils`, `libjsoncpp25`, `libopenconnect5`, `libssl3t64`, `liblz4-1`, `ca-certificates`. Diğer dağıtımlarda eşdeğer paketler ve bütün `ldd` bağımlılıkları ayrıca doğrulanmalıdır. Özellikle `libjsoncpp25` bulunmazsa her iki C++ motoru da açılamaz; kaynak paketi bu kitaplıkları içermez. Bu liste GlobalProtect SSO için sabitlenmiş OpenConnect sürümünün ayrıca sağlanması koşulunu kaldırmaz.

Paketin systemd birimi, root olmayan istemcinin `/proc/<pid>/exe` yolunu doğrulamak için `CAP_SYS_PTRACE` gerektirir. Servis istemcinin gönderdiği PID'ye güvenmez; çekirdek `SO_PEERCRED` bilgisi ve açık tutulan `pidfd` üzerinden UID, çalıştırılabilir dosya yolu ve SHA-256 denetimini yapar. Bu yetki kaldırıldığında servis açık görünse bile doğru istemci reddedilir. Birimdeki diğer capability sınırları, salt okunur sistem, özel geçici dizin ve `NoNewPrivileges` korunur.

Kaldırmadan önce ayrı geliştirme uygulamasını kapatıp yalnız bu yardımcı servisi durdurun ve sahiplik günlüğünün boş olduğunu doğrulayın. Günlük boş değilse otomatik dosya/arayüz silme yapmayın. Ardından yalnız manifestteki yardımcı dosyalar, ilgili kesin servis dosyası ve boş kendi ayar/durum dizinleri kaldırılabilir. Kararlı uygulama, başka servisler veya kullanıcı kasaları için genel temizleme komutu yoktur.

## Doğrulama sınırları

[30 Ekim kurulum kontrolü](../artifacts/vpn-lab/linux-installed-services-20261030.json), WSL içindeki ayrı Ubuntu 24.04 rootfs üzerinde gerçek PID 1 systemd ile çalıştırıldı. Paket dosyaları root sahipli kuruldu; gerçek `termterm-vpn-dev.service` biriminde root olmayan sabitlenmiş yerel test istemcisinin protokol 2 bağlantısı, yeniden başlatma ve durdurma, boş sahiplik günlüğü ve soket temizliği geçti. Farklı UID, aynı UID'deki sabitlenmemiş Python istemcisi ve kullanıcı tarafından yazılabilen çalıştırılabilir dosya reddedildi. Eski capability listesiyle başka UID'nin `/proc/<pid>/exe` okuması exit 1, yalnız `CAP_SYS_PTRACE` eklenince exit 0 verdi.

Aynı özel sistemde ayrı PostgreSQL 16 veritabanına `manage.py` ile 66 migration ilk kez uygulandı ve aynı sürüm doğrulaması tekrarlandı. Ayrı kullanıcıyla çalışan gerçek `termterm-team.service`, dar peer eşlemesiyle Unix soketinden bağlandı; `/ready` şema 66/API 1 döndürdü, servis rolünün doğrudan tablo okuması reddedildi ve yeniden başlatma/durdurma geçti. Özel kümenin portu 5433 olarak açıkça verildi. Kurulum boyunca WSL ana sisteminin rotaları, DNS içeriği ve PostgreSQL 18/resolved servis kimlikleri aynı kaldı. Bu kontrol tam Linux masaüstü paketinin açılışını, HTTPS istemci girişini veya şirket VPN kimlik doğrulamasını kapsamaz; kaynak/ikili hazırlık manifesti hâlâ `distributionReady: false` döndürür.

Kısa-ad düzeltmesinin [30 Ekim uygulama koşusunda](../artifacts/vpn-lab/native-unix-service-20261030-search-domains.json) **54 kontrol + 1 tanılama kaydı** geçti. Özel network/mount/D-Bus alanındaki systemd-resolved 255 ile gerçek OpenConnect `default-domain + split-dns` ve OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX` bağlantılarında root olmayan `socket.getaddrinfo` hem FQDN hem düz `probe` adını çözdü; her sorguda otorite DNS cevap sayısının arttığı doğrulandı. Her iki ad tünel öncesinde ve disconnect, istemci ölümü, broker ölümü/yeniden başlatma sonrasında çözülemedi. Her mod DNS/rota baseline'ını, yabancı DNS ayarını, boş günlüğü ve arayüz temizliğini doğruladı. Ana sistem ağı/DNS yapılandırması ve resolved servis kimliği değişmedi; kaynaklar test boyunca aynı kaldı ve namespace'ler temizlendi. Gerçek OpenVPN `DOMAIN` güvenli reddi korundu. Bu koşu hosta kurulu servisi veya macOS'u sınamaz; bağımsız doğrulama ayrıca gerekir.

`scripts/test-vpn-lab.py --native-service` gerçek çekirdek arayüzleriyle, özel ağ namespace'lerinde root sahipli yardımcılar ve sabitlenmiş root olmayan istemciyle OpenConnect/OpenVPN/WireGuard trafiğini, üç paralel bağlantıyı, yanlış istemci kimliğini, çakışmaları, nesil kontrolünü, kapanış onayını ve süreç ölümü/yeniden başlatma günlüğünü sınar. [30 Ekim sakin bağımsız tekrarda](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier-quiet.json) **54 kontrol + 1 tanılama kaydı** geçti. Özel network/mount/D-Bus alanında gerçek systemd-resolved 255 ile OpenConnect ve **OpenVPN `DNS + ADAPTER_DOMAIN_SUFFIX`** tünellerinden ayrı taze DNS kanıtlarıyla root olmayan uygulamanın FQDN çözümü; disconnect, istemci ölümü ve broker ölümü/yeniden başlatma sonrası DNS/rota temizliği ve yabancı DNS ayarının korunması doğrulandı. Gerçek OpenVPN `DOMAIN` split-DNS planı `advanced_dns_policy_unsupported` ile mutation öncesinde beklenen biçimde reddedildi; bu politika desteklenmiş sayılmaz. Kaynak özetleri test sırasında değişmedi, ana sistem rota/DNS yapılandırması ve resolved servis kimliği aynı kaldı, namespace'ler temizlendi. Bu önceki FQDN koşusu Linux kısa-ad aramasını ve hosta kurulu servisi doğrulamadı.

[İlk bağımsız 54 kontrol koşusu](../artifacts/vpn-lab/native-unix-service-20261030-openvpn-dns-verifier.json), iç kontroller geçse de dış host ağ snapshot'ı eşleşmediğinden toplam **FAIL** olarak korunur. Neden kanıtlanmadı; eşzamanlı Windows çalışması yalnız olasılıktır. Sakin tekrar tüm kapıları geçti. Önceki [50 kontrol + 1 tanı](../artifacts/vpn-lab/native-unix-service-20261030-resolved-verifier.json) ve [29 Ekim 44 kontrol + 1 tanı](../artifacts/vpn-lab/native-unix-service-20261029-wg-ipv4fix.json) raporları tarihsel kanıt olarak korunur.

`scripts/test-vpn-webkit.py` gerçek WebKitGTK yanıt sinyalini, geçici WebView'ın kendi özel veri yöneticisini, ACS origin/başlık kontrolünü ve eski yanıtın engellenmesini özel TLS/DNS/ekran fixture'ında sınar. Geçici CA yalnız özel mount namespace'indeki sistem güven paketine eklenir; TLS doğrulaması ve WebKit sandbox'ı açıktır. `native-webkit-20261029-privateca.json` raporunda beş kontrol tamamlandı, ancak süreç 134 koduyla kapandı. GTK başlatmasını ve WebKit kapanışını process main üzerinde birleştiren, `vpn-webkit-fixture` feature'ına bağlı `harness=false` hedefiyle `native-webkit-20261030-process-main.json` raporunda beş kontrol ve temiz exit 0 geçti; ana sistem rota/DNS/hosts/güven paketi değişmedi. Runner hem exit 0 hem beş kontrolün tamamlanma kaydını ister. Gerçek şirket GlobalProtect veya Microsoft kimlik sağlayıcı hesabı testi değildir. Yalnız HTTPS yönlendirme ve ACS yanıt başlığı akışı desteklenir; gömülü POST/body varyantı reddedilir ve dış tarayıcıya geçilmez. macOS ağ, DNS, imza, kurulum ve WebKit çalışması bu Windows/WSL ortamında test edilmemiştir. Tam masaüstü Linux giriş akışı ayrıca gereklidir.

Tekrarlanabilir izole kontroller:

```sh
python3 scripts/test-vpn-lab.py \
  --engines /root/termterm-vpn-native-build/engine \
  --ocserv-root /root/termterm-vpn-build/ocserv \
  --policy /root/termterm-vpn-native-build/core/debug/vpn-policy-lab \
  --report artifacts/vpn-lab/native-unix-review.json --native-service
python3 scripts/test-vpn-webkit.py \
  --test-executable /root/termterm-vpn-native-build/desktop/debug/deps/vpn_webkit_fixture-1765456e583e4547 \
  --report artifacts/vpn-lab/native-webkit-review.json
```

WebKit komutundaki dosya, `src-tauri` dizininde `TAURI_CONFIG="$(cat tauri.vpn-dev.conf.json)" CARGO_TARGET_DIR=/root/termterm-vpn-native-build/desktop cargo test --locked --test vpn_webkit_fixture --features vpn-webkit-fixture --no-run` çıktısından alınır; dosya son eki farklı derlemede değişebilir. Özel hedef libtest worker'ı kullanmaz; fixture process main thread kimliğini ayrıca doğrular. Testler root WSL laboratuvarında çalışır, servis veya kararlı masaüstü kurulumu yapmaz.

## 0.4.3 üretim adayı sözleşmesi

`tauri.vpn-candidate.conf.json` ve `vpn-candidate` özelliği yalnız `local.termterm.desktop` / `0.4.3` birleşimini açar. Yerel önizleme adayında güncelleyici kapalıdır. Nihai yerel yayın adayı için ayrıca `tauri.vpn-release-candidate.conf.json` ve `TERMTERM_RELEASE_CHANNEL=stable` seçilir: mevcut kararlı feed ve imza doğrulama anahtarı korunur; bu seçim yayın onayı değildir. Windows'ta aday yalnız ayrı bir Windows test profili veya sanal makinede değerlendirilmelidir; mevcut kararlı uygulamanın verileri kullanılmaz.

Linux paket hazırlığı `scripts/package-vpn-linux.py --candidate 0.4.3 --uid <yöneticinin seçtiği nonroot UID>` ile yapılır. Nihai yerel yayın adayı için aynı komuta `--release-candidate` eklenir; Windows adaptöründe karşılığı `-Candidate 0.4.3 -ReleaseCandidate` seçimidir. Bu adaptörler yükleyici çalıştırmaz, imza anahtarı kullanmaz, feed değiştirmez veya yayın yapmaz. Önizleme adayından yayın adayına geçiş farklı binary hash üretir; yayın onayı yalnız son doğrulanan paketlere verilmelidir. Eşleşen DEB, `/usr/bin/termterm` dosyasını; broker paketi `/opt/termterm-vpn`, `/etc/termterm-vpn/broker.json`, `/var/lib/termterm-vpn` ve `termterm-vpn.service` adlarını kullanır. Servis `--candidate 0.4.3` ile başlar; root tarafından seçilen UID, kurulu masaüstünün kesin SHA256 değeri, üretim kimliği/sürümü ve sabit kurulum yolları doğrulanır. Paket servis başlatmaz; yönetici, doğru UID, root sahipliği, bağımlılıklar ve DNS önkoşullarını doğrulamalıdır. Geliştirme soketi ve laboratuvar override'ı bu adayda kabul edilmez.

Üretim AppImage VPN yolu, geçici mount yerine sabit root sahipliğindeki AppDir kurulumudur. Broker DEB, masaüstü DEB'e zorunlu bağımlı değildir; iki masaüstü seçeneğinden yalnız yönetici tarafından seçilen kesin dosya yetkilendirilir. Yönetici broker DEB'i kurup `systemctl daemon-reload` yaptıktan sonra şu aracı kullanır:

```sh
sudo python3 /opt/termterm-vpn/bin/install-vpn-appimage.py \
  --appimage ./TermTerm_0.4.3_amd64.AppImage \
  --manifest ./APPIMAGE-INSTALL.json --manifest-sha256 <doğrulanmış_manifest_SHA256> \
  --uid <broker_yapılandırmasındaki_nonroot_UID>
```

Manifest SHA256 değeri bağımsız incelenen paket kaydından alınmalıdır. Araç, kesin AppImage hash'ini doğrular; salt okunur root kopyasından çıkarma işlemini belirtilen nonroot kullanıcıyla yapar. Tüm dosya hash'leri, yalnız ağaç içinde kalan göreli sembolik bağlantılar ve izinler doğrulanır. Aygıt/FIFO, setuid/setgid ve dışarı çıkan bağlantılar reddedilir. Tam kopya yeniden doğrulanır; dinamik bağımlılıklar da nonroot `ldd` ile, yalnız root sahipliğindeki kütüphane yollarında kontrol edilir. Eksik bağımlılık veya kopya değişikliği, mevcut kurulum değiştirilmeden işlemi durdurur.

Kurulum `/opt/termterm-appimage` ve `/usr/bin/termterm-appimage` başlatıcısını üretir; normal kullanıcı bu sabit başlatıcıyı açar. Broker, `clientInstall=appimage` ile yalnız `/opt/termterm-appimage/usr/bin/termterm` dosyasının kesin hash'ini kabul eder. Doğrudan `.AppImage` geçici mount'u, kullanıcı tarafından yazılabilen AppDir veya geliştirme uygulaması yetkilendirilmez. Yardımcı servis otomatik başlatılmaz.

Güncelleme yöneticinin açık işlemidir: VPN'i durdurun, broker servisini durdurun ve sabit AppDir uygulamasını kapatın. Yeni paket/manifest ile aynı komuta mevcut istemci hash'ini `--expected-client-sha256` olarak ekleyin. Araç kendi önceki kurulum kaydı, eşleşen eski hash, değişmemiş başlatıcılar, servis kapalı ve kilitli boş sahiplik günlüğü şartlarını arar; eski AppDir ve private broker yapılandırması yedeklerini korur. Kesinti sonrasında otomatik silme veya rollback yapılmaz; servis kapalı tutulup korunan dosyalar yönetici tarafından incelenmelidir. Uygulama bu root kurulumunda otomatik dosya değiştirmeyi reddeder ve yöneticiyle güncelleme gerektiğini gösterir; mevcut imzalı feed kontrolü korunur.

macOS aday servis/launchd kaynakları `prepare-vpn-unix.py --platform macos --candidate 0.4.3 --source-only` ile hazırlanabilir; bu makinede macOS imza, kurulum veya yerel VPN testi yapılmış sayılmaz. AppDir kaynak/fixture kontrolleri gerçek aday paketinin yerel kurulum/bağlantı kabulünün yerine geçmez. Yükleyici/otomatik güncelleme varlıklarının sahibi tarafından geçerli anahtarla imzalanması hâlâ yayın öncesi ayrı şarttır; bu geliştirme işlemi imza anahtarına erişmez ve yayın yapmaz.

## Linux tam tünel ve fiziksel ağın korunması

Ham motor planında açıkça `0.0.0.0/0` bulunması tam tünel niyetini korur. Bu işaret, uç nokta ve istisnalar çıkarıldıktan sonra da broker'ın kendi planında kalır; istemci bu alanı göndererek izin açamaz. Eski günlükte alan yoksa varsayılan kapalıdır; eski sahiplik temizliği değişmez. Yalnız tam tünelde, Windows politikasındaki gibi multicast ve sınırlı broadcast aralıkları da tünel rotalarından çıkarılır.

Linux, VPN uç noktasına giden fiziksel arayüzdeki gerçek bağlı ağı koruyabilir. İstisna yalnız Ethernet veya `veth`, çekirdeğin `kernel/link` rotası, gerçek IPv4 adres öneki ve tercih edilen kaynak eşleşmesiyle doğrulanır. Korunan rota, çakıştığı her VPN rotasından daha dar olmalıdır. Atanacak tünel alt ağıyla çakışma, elle eklenmiş/on-link rota, başka arayüz veya VPN, eksik metadata, eşit/geniş rota ve split/custom planlar için bu istisna kullanılmaz. Bridge, bond, VLAN, TUN/TAP ve bilinmeyen arayüz türleri bu sınırlı istisnaya dahil değildir. Uç nokta aynı fiziksel alt ağdayken yalnız `/32` uç nokta deliği yeterli olmayabilir; sunucunun açık yerel ağ istisnası yoksa bağlantı güvenli biçimde reddedilir.

VPN DNS adresi korunacak fiziksel önekin içindeyse `dns_conflicts_with_physical_underlay` hatasıyla ağ değişmeden durulur. DNS'in yanlışlıkla fiziksel ağdan gönderilmesine izin verilmez; açık bir DNS host rotası bu sürümde bu reddi kaldırmaz. Gerçek DNS yetkilendirmesi ve kısa ad çözümlemesi mevcut `systemd-resolved` şartlarıyla devam eder.

[Uygulama kabulü](../artifacts/vpn-lab/native-unix-full-tunnel-20261031-dns-fixed.json) ve [bağımsız son tekrar](../artifacts/vpn-lab/native-unix-full-tunnel-20261031-verifier.json) **65 kontrol + 1 tanı** geçti. OpenConnect, OpenVPN ve WireGuard tam tünellerinin her birinde normal kesme, istemci ölümü ve broker ölümü/yeniden başlatma; fiziksel bağlı önekin ve taşıma uç noktasının korunması, gerçek veri ve rakip tam tünelin reddi doğrulandı. OpenConnect tam tünel + DNS için üç kapanış biçiminde gerçek nonroot FQDN/kısa ad sorguları ve otorite yanıt sayacı, yabancı DNS ayarı ve rota/journal temizliği geçti. Fiziksel öneke DNS gönderen gerçek sunucu planı mutation öncesinde reddedildi. Host ağı/servis kimliği ve kaynaklar değişmedi, namespace'ler temizlendi. Bu rapor şirket VPN girişi veya macOS doğrulaması değildir.
