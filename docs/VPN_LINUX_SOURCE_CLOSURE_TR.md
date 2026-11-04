# Linux VPN geliştirme paketinin ek kaynak kapanışı

Bu kayıt, `0.4.3-dev.1` Linux geliştirme paketinde önceden eksik olarak kaydedilen Ubuntu kitaplıkları, AppRun ve gömülü AppImage runtime kaynaklarını tamamlar. Uygulama, DEB, AppImage, yardımcı DEB veya 540 dosyalı v3 kaynak ZIP'i değiştirilmez. Yeni toplayıcı ve bu belge, `SOURCE-DELTA.json` içinde ayrıca listelenir; v3 ZIP'inin içinde oldukları iddia edilmez.

## İçerik ve doğrulama zinciri

- 179 ELF dosyasının 116 Ubuntu ikili paketine eşlemesi ve yardımcı bağımlılıkları, 94 ayrı kaynak paketi/sürüm çiftine karşılık gelir. Bunların 301 `.dsc`, özgün kaynak ve dağıtım yaması dosyası tam sürümleriyle sağlanır.
- Ubuntu zinciri: dağıtım anahtarlığıyla doğrulanan `InRelease` imzası → `Sources.xz` SHA-256 → `.dsc` ve bütün kaynak dosyalarının SHA-256 değerleri. Her `.dsc` içindeki kaynak adı, sürüm ve dosya listesi ayrıca karşılaştırılır. Ayrı geliştirici `.dsc` imzaları doğrulanmış sayılmaz; bunlar imzalı arşiv indeksiyle doğrulanır.
- AppRun, hem Tauri aynasındaki hem upstream AppImageKit yayınındaki dosyayla byte düzeyinde aynıdır. Kaynak commit'i `5735cc5bed206497cddfbd2a75e1982c2606c35d`; kaynak ve MIT lisansı dahildir.
- Runtime commit'i `fedfad51acc2860b2916a1eef32ba4a500ff31ef` olarak gömülü metinden ve resmi yayın/build kaydından doğrulanır. AppImage öneki, yalnız gerçek ELF `.digest_md5` bölümündeki 16 bayt sıfırlandığında resmi runtime dosyasının SHA-256 değeriyle eşleşir. Kod veya SquashFS değiştirilmez.
- Resmi x86_64 CI işi `118293253809`, koşu `39320578745`, gerçek statik bağımlılık sürümlerini ve Alpine 3.21 base-image digest'ini gösterir. `zlib 1.3.2-r1`, `zstd 1.5.6-r2`, `mimalloc2 2.1.7-r0` ve `musl 1.2.5-r11` için tam aports commit'leri, tarifler, yamalar ve SHA-512 ile doğrulanan özgün kaynaklar sağlanır.
- İlk üç Alpine paketin kontrol akışları ve indeksleri resmi Alpine anahtarıyla RSA olarak doğrulanır. Güncel indekste musl r12 bulunduğu için r11 yerine kullanılmaz: r11 commit'i, CI kaydında sabitlenmiş OCI katmanının kurulu paket veritabanından doğrulanır.
- `libfuse 3.15.0` ve `squashfuse 0.5.2`, runtime'ın sabit kaynak tarifindeki SHA-256 değerleriyle doğrulanır; runtime'ın libfuse yaması da dahildir. Bağımlılıkların özgün lisans bildirimleri `runtime/notices/` içindedir.

`SOURCE-CLOSURE.json` kapsamı, kanıt yollarını ve her dosyanın hash'ini; `SOURCE-REPLAY-LOCK.json` tam tekrar indirme girdilerini; `ORIGINAL-ARTIFACTS.json` değişmeyen eski teslimatı kaydeder. Eski `APPIMAGE-PROVENANCE.json` ve paket makbuzları oluşturuldukları tarihin kayıtları olarak korunur; bu ek, orada kaydedilen kaynak eksiklerini kapatır.

## Tekrarlama ve çevrimdışı kontrol

Ek dizin: `artifacts/vpn-delivery-20261103/linux/source-closure-20261103/`.

Toplayıcı `recipe/collect-vpn-linux-sources.py` olarak da sağlanır. Yalnız eski v3 kaynak ZIP'i açılmışsa bu yeni dosyayı deponun `scripts/` dizinine ekleyin; mevcut farklı bir dosyanın üstüne yazmayın. Bu belge de ek kaynak değişim kaydının parçasıdır.

WSL/Linux üzerinde, depo kökünde:

```sh
python3 scripts/collect-vpn-linux-sources.py --self-test
python3 scripts/collect-vpn-linux-sources.py \
  --verify-closure artifacts/vpn-delivery-20261103/linux/source-closure-20261103
```

Gereken yerel kontrol araçları: Python 3, `gpgv`, OpenSSL, `readelf` ve `unsquashfs`. Bu işlem uygulamayı veya runtime'ı çalıştırmaz, servis kurmaz ve ağı değiştirmez.

Musl'un OCI kökenini bağımsız olarak tekrar doğrulamak için resmi CI logunda belirtilen `sha256:16333ee0c00fc65e025a2a4f839703ad37a74728832977fbdf080984de1b8e5a` Alpine x86_64 katmanını ayrıca alın ve `--oci-layer /yerel/yol/alpine-layer.tar.gz` ekleyin. Toplama ortamındaki bu katman sadece kanıt olarak okunmuştur; içindeki sistem dosyaları kurulmamış ve bu kaynak paketine ikili base image eklenmemiştir. İndirme adresi ve exact digest manifestte kayıtlıdır; Docker çalıştırmak gerekmez.

Eksik özgün kaynak dosyalarını aynı hash'lerle yeniden almak için:

```sh
python3 scripts/collect-vpn-linux-sources.py \
  --replay-lock artifacts/vpn-delivery-20261103/linux/source-closure-20261103/SOURCE-REPLAY-LOCK.json \
  --output "$PWD/artifacts/vpn-delivery-20261103/linux/source-closure-20261103"
```

Kilitte adresi olmayan imzalı metadata ve CI kanıtları sağlanan kopyadan korunur. Değişmiş bir yayın adresi hash uyuşmazlığı verirse en yeni sürümle değiştirilmez. Kaynak toplama komutu, mevcut dosyaları hash ile kontrol edip atlar. Ubuntu indekslerini yeniden değerlendirmek için aynı toplayıcının `--delivery`, `--metadata`, `--output` modu kullanılabilir.

## Sınırlar

Bu, belirtilen kaynak ve lisans kanıtlarının teknik kapanışıdır; genel bir hukuki uygunluk garantisi veya yayın onayı değildir. Kurulum, masaüstü açılışı, yerel VPN trafiği, gerçek sunucu kimlik doğrulaması veya macOS testi yapılmış sayılmaz. AppImage hâlâ yardımcı servisle eşleştirilmemiş masaüstü önizlemesidir; `distributionReady` ve yerel çalışma zamanı kabul kapıları bu kaynak toplamayla açılmaz.
