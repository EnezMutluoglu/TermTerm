# VPN dalı — 4 Kasım 2026 devir teslimi

Giriş: [HANDOFF_TR.md](../HANDOFF_TR.md). Dal `codex/feature-vpn-workspace`; devir öncesi HEAD `9a7fd9b63671fa28c43233d446cea0e4a44dde60`, ortak develop tabanı `b749b35c66670a47941b572db44cd66097e88ef3`. SQL dalı bu çalışmaya birleştirilmedi.

`package.json`/temel uygulama 0.4.2; ayrı VPN geliştirme ayarı **0.4.3-dev.1**, uygulama kimliği `local.termterm.desktop.vpndev`. Yalnız `tauri.vpn-dev.conf.json` ve ilgili VPN paket ayarlarını kullanın. Bu sürüm yayınlanmış/dağıtıma hazır sayılmaz.

## Kaldığımız yer

| Alan | Korunan önceki sonuç | Açık iş / sınır |
|---|---|---|
| Authenticator | 28 Ekim raporunda Team/Rust, PostgreSQL yükseltme ve Windows native kapsamı var; ayrıntı [AUTHENTICATOR_TR.md](AUTHENTICATOR_TR.md) | Bu teslimde tekrar koşulmadı; Google toplu taşıma QR/push MFA yok |
| Linux VPN / Team | 30 Ekim raporlarında WireGuard kernel 12/12, Team izin iptali ve full-tunnel kabulü var | WSL/izole lab; her dağıtım, üretim kimlik doğrulaması veya fiziksel suspend kabulü değildir |
| Linux kaynak kapanışı | 3 Kasım teslim kaydında **391 dosyalı** ek kaynak kapanışı bağımsız GO aldı; [yöntem ve sınırlar](VPN_LINUX_SOURCE_CLOSURE_TR.md) | Ham kaynak arşivleri, makbuzlar ve replay lock özel `artifacts` içinde, Git tesliminde yok; paket/runtime kabul kapıları açılmadı |
| Windows WireGuard v5 | İlk kalıcı kayıp sonrası devnode kaybolduğu halde sahipli MIB interface kaydı **720 saniye** gözlem sınırında kaldı: kabul **FAIL** | Sonraki journal 0 ve eşit Windows/Linux baseline snapshot'ı bu başarısızlığı PASS yapmaz. Sonraki reconnect vakaları tamamlanmadı; eski paket/source freeze güncel kaynak kabulü değildir |
| Windows saf Wintun tanısı | 3 Kasımda normal/raw kayıtlar kapamadan sonra kaldı; yaklaşık 1048 ms'de normal/raw MIB kayıtları boş, normal/phantom PnP devnode görünümü yoktu; 1080 ms'de bitti; journal 0 ve baseline eşitliği kaydedildi | Özgün wrapper `ok=false`, `ExitCode=null` olarak korunur. OS gözlemi wrapper exit-code başarısı değildir |
| PowerShell exit code | İnert `cmd` çocukları 0/17 ile, `Process.Handle` beklemeden önce okununca 0/17 kanıtı var | Özgün başarısız wrapper raporu değiştirilmedi; yeni makinede yeniden kontrol gerekir |
| Yapılandırılmış Wintun v2 | Kaynak yazıldı; ilk derleme **E0603**, `network::route_apply_order` private (`src/main.rs:123`) | **Derlenebilir değil. Üç inert test çalışmadı.** Native wrapper `CONFIGURED_EXE_HASH_PENDING`; freeze, self-test, yetkisiz ret ve bağımsız inceleme tamamlanmadı. Çalıştırmayın |
| Windows 1175 düzeltmesi | `scripts/vpn_lab_json.py` ve `scripts/test-vpn-lab-json.py` içinde dar bounded retry; önceki kaynak incelemesi/odaklı testler geçti | Native 1175 yeniden üretilemedi. Eski 55 dosyalı freeze ve paket testleri bu son iki dosyayı doğrulamaz; yeniden paketleme/inceleme gerekir |

[VPN_RESUME_TR.md](VPN_RESUME_TR.md), [VPN_DEVELOPMENT_STATUS_TR.md](VPN_DEVELOPMENT_STATUS_TR.md) ve broker [HANDOFF.md](../vpn-windows-broker/HANDOFF.md) uzun tarihsel kayıtlardır. Üstlerinde bu belgeye yönlendirme bulunur; 30 Ekim başlıklarını son durum sanmayın. Kaynakta değişiklik olması eski native test kanıtını yenilemez.

## Tanı kaynaklarının korunması

[handoff-diagnostics](../handoff-diagnostics/README_TR.md) yalnız genel Rust tanı kaynakları, Cargo kilitleri ve ağ değiştirmeyen exit-code self-check kaynağını içerir. Yeni konum özgün göreli broker bağımlılığını korur. UAC/native wrapper, çalıştırılabilir dosya, Wintun DLL, baseline/journal dökümü veya özel fixture taşınmadı. Özgün `.lab` ve `.tools` kayıtları eski makinede korunur.

Yapılandırılmış v2'nin disposable adres/domain/GUID sabitleri bir gerçek profil değildir; yeni koşudan önce yeni sahiplik kimliği ve ağ çakışması kontrolü gerekir. Üretim API'sini public yaparak tanı derlemesini düzeltmeyin. Tanının 150 s DNS öncesi beklemesi, 150 s bağlı DNS veya motor/gerçek trafik/Windows resolver davranışını test etmez; >720 s neden hâlâ bilinmiyor.

## Yeni makinede devam sırası

1. Ayrı VPN worktree ve yeni disposable kasa hazırlayın. Sır/OTP/CA güveni yeniden girilir. Eski makine fixture'ı, process kimliği veya pinleri kullanılmaz.
2. Önce kaynak kontrollerini ve araçları kurun. Windows: Visual Studio 2022 C++/SDK, Git Bash, Node/pnpm, Python 3, Rust, Go ve CMake/Ninja; motor bağımlılıkları [Windows hazırlık](VPN_WINDOWS_PREPARATION_TR.md) ve [bağımlılık makbuzunda](VPN_DEPENDENCIES_TR.md). Linux: ayrı disposable WSL/VM, derleyici/Tauri WebKitGTK 4.1 ve namespace yetkileri; [Unix kılavuzu](VPN_UNIX_KURULUM_TR.md). Eski makinenin araç/cache klasörlerini kopyalamayın.
3. Yarım v2'nin sadece standalone private-helper erişimini yerel bilinen rota sırasıyla düzeltin; ürün dışa aktarımını genişletmeyin. İlgili üç inert testi çalıştırın. Sonra bağımsız inceleme, taze DLL/EXE kaynak hash'leri ve yeni wrapper hazırlığı gerekir. Bu teslim bunu yapmadı.
4. Native denemeden önce salt okunur baseline, journal/native envanter ve üçüncü parti VPN çakışması kontrolü alın. UAC'yi ancak incelenmiş dar deney ve kullanıcı erişilebilirliğiyle açın. Yapılandırılmış sonuç incelenmeden uzun v5 kabulünü tekrarlamayın.
5. Engel çözüldüğünde normal kapanış, kalıcı çift/tek yön kayıp, aynı reconnect sırası, client-death ve ortak OpenConnect/OpenVPN cleanup regresyonlarını taze paketle doğrulayın. Recovery sonucu başarısız kabul yerine geçmez; journal korumalarını gevşetmeyin.

Aşağıdaki komutlar sonraki geliştirme içindir; bu devirde çalıştırılmadı:

```powershell
pnpm install --frozen-lockfile
pnpm test:unit
cargo test --locked --manifest-path vpn-core/Cargo.toml
cargo test --locked --manifest-path vpn-windows-broker/Cargo.toml
cargo test --locked --manifest-path vpn-windows-broker/Cargo.toml --features privileged-validation
python scripts/prepare-vpn-windows.py --self-test
python scripts/build-openconnect-windows.py --self-test
python scripts/build-wireguard.py --self-test
python scripts/test-vpn-packaging.py
pnpm vpn:build
```

Motorları yeniden hazırlama: `python scripts/prepare-vpn-windows.py --build-core`, `python scripts/build-openconnect-windows.py`, `python scripts/build-wireguard.py --target windows --arch amd64`. Bunlar ağdan bağımlılık indirir/derler; hash kilitleri korunmalı. Yerel ayrı VPN paketinin girişi `scripts/package-vpn-dev.ps1`, Linux hazırlık/paket girişleri `scripts/build-vpn-linux.py` ve `scripts/package-vpn-linux.py`; parametreleri ilgili belge ve `--help` ile seçin. Kaynak ZIP/vendor input/replay lock yoksa eski paket komutunu varmış gibi çalıştırmayın. Genel `pnpm package` kullanılmaz.

Bu teslimde yeni test/derleme/native/paket kurulumu yapılmadı. Windows Team SSH/SFTP tam native kabulü, donanım anahtarı, fiziksel suspend ve tüm üretim kimlik doğrulaması doğrulanmış değildir. **macOS fiziksel LAN ile örtüşen full-tunnel planında bilinen kaynak eksiği vardır:** Darwin rota ön kontrolü Linux'taki bağlı fiziksel ağ istisnasını uygulamaz; açık sunucu istisnası örtüşmeyi kaldırmazsa `existing_route_conflict` ile mutation öncesi reddeder. Bu yalnız test bekleyen bir alan değildir; kaynak düzeltmesi ve gerçek Mac kabulü gerekir. Linux fiziksel bağlı önek istisnası da Ethernet/veth ile sınırlıdır; bridge/bond/VLAN/TUN/TAP kapsamı tamamlanmadı.
