# WireGuard geliştirme entegrasyonu

TermTerm, resmi `wireguard-go` motorunu ayrı `termterm-wireguard` yardımcısında kullanır. Sabit sürüm, Go modül özeti, kaynak ZIP SHA256 ve lisans SHA256 değerleri `vpn-wireguard/dependencies.json` içindedir. Kaynak: https://git.zx2c4.com/wireguard-go/commit/?id=b3bc704ef0ee21a36298601a9e1e5ff90377ac52 . Windows'un resmi önerilen WireGuardNT/servis paketi ayrıca bir sürücü ve servis yaşam döngüsü gerektirir (https://www.wireguard.com/embedding/); bu entegrasyon mevcut Wintun adaptörünü ve broker günlüğünü kullanır. Şifreleme ve el sıkışması yeniden yazılmaz.

`.conf` içindeki anahtarlar profil yapılandırmasının parçasıdır ve mevcut şifreli kasa/Team profil saklamasından geçer. Motor anahtarları özel stdin kanalından alır; komut satırına, ortama, düz metin geçici dosyaya veya günlük kayıtlarına yazmaz. Rust anahtarları ve geçici kontrol tamponları mümkün olan yerlerde sıfırlanır. Go'nun çöp toplayıcısı ve değişmez dizeleri nedeniyle bütün bellek kopyalarının kesin silinmesi garanti edilmez. UAPI durum çıktısı özel anahtarlar da içerdiği için dışarı verilmez; yalnız peer açık anahtarları ve tamamlanan el sıkışması zaman damgaları bellekte süzülür.

Kapsam: 1–8 IPv4 adresi, 1–64 açıkça tanımlanmış istemci peer'i, toplam en çok 1024 AllowedIPs, en çok 8 IPv4 DNS ve bir arama alanı. Her peer için PublicKey, AllowedIPs ve Endpoint gerekir. PrivateKey ve varsa PresharedKey kanonik 32 bayt Base64 olmalıdır. Endpoint adları ağ rezervasyonundan önce bir kez çözülür; UDP gönderimi ve alımı ayrılmış sayısal adres/port listesine sabitlenir. DNS adresleri AllowedIPs ile tünel üzerinden erişilebilir olmalıdır. IPv6 yapılandırması bu broker sürümünde açık hatayla reddedilir. PostUp/PostDown/PreUp/PreDown, başka komutlar ve harici anahtar dosyası yolları çalıştırılmaz. Varsayılan rota ve birden çok VPN için mevcut çakışma/istisna/DNS rezervasyonları geçerlidir; aynı anda en çok üç bağlantı vardır.

Motor önce ağ planını önerir; broker onayından ve sahip olduğu TUN/Wintun kurulumundan sonra UDP açılır. Her yeni motor bütün peer'ler için gerçek el sıkışması başlatır. Bütün peer'ler 30 saniye içinde taze el sıkışması üretmeden `connected` gönderilmez. Tek peer başarısı çok peer'li profili bağlı yapmaz.

Bağlandıktan sonra her gerekli peer için tamamlanan doğrulanmış el sıkışmasının saniye+nanosaniye damgası izlenir. Damga değişmeden 15 saniye geçince resmi motorun `SendHandshakeInitiation` API'siyle yeni el sıkışması istenir; 45 saniye boyunca yeni kanıt gelmezse `wireguard_peer_unresponsive` başarısızlığı mevcut kapanış yoluna girer. Son neden paket kanalı kapanmadan önce bildirilir. Keepalive cevabı veya uygulama trafiği varsayılmaz; boşta peer de gerçek el sıkışmasına cevap verir. Yaş hesabı yerel monotonik saatle yapılır, UAPI duvar saatiyle yaş kıyaslanmaz. Beş saniyeyi aşan denetim boşluğu ilk kez görüldüğünde hemen probe ve 30 saniye toparlanma süresi verilir; yeni kanıt gelmeden tekrarlanan boşluklar süreyi tekrar uzatmaz. Süreç çalışmadığı sırada deadline uygulayamaz. Yalnız uygulama paketlerini engelleyip el sıkışmasını geçiren seçici filtre bu protokol erişilebilirlik sinyaliyle algılanmaz.

Broker protokolü 2, bağlantı hazır olduğunda `networkLease` olayında id, generation, interfaceGuid, interfaceLuid, interfaceIndex, addressFamilies ve adres CIDR'lerini bildirir. `networkReceipt` aynı canlı/sahip olunan arayüzü yeniden doğrular. Kapatma önce yeni lease'leri engeller ve motoru durdurur; `quiesceRequired` sonrasında istemci aynı id/generation için bağımlı soketleri ve işleri kapatıp `quiesced` yollar. Broker bu ACK'yi beklerken istek döngüsü çalışır. ACK olmadan 10 saniye dolması adaptör/rotaları kaldırmaz; temizlik bekliyor hatası verir. Kontrol borusunun kapanması, yaşayan istemcinin soketlerinin kapandığına kanıt sayılmaz. Gerçek doğrulanmış ana süreç sonlandıktan sonra temizliğe izin verilir. Broker çökmesinde gereken yerel soket sabitlemesi ayrı platform katmanının sorumluluğudur; bu protokol tek başına sistem çapında bir kill switch değildir.

## Yerel derleme ve kontroller

```powershell
python scripts/build-wireguard.py --self-test
python scripts/build-wireguard.py
cd vpn-wireguard
go test -v ./...
go vet ./...
go test -c -o ../.tools/vpn-wireguard/wireguard-peer-fixture.exe .
cd ..
$env:TERMTERM_WG_ENGINE_TEST=(Resolve-Path .tools/vpn-windows/engine/termterm-wireguard.exe).Path
$env:TERMTERM_WG_PEER_TEST=(Resolve-Path .tools/vpn-wireguard/wireguard-peer-fixture.exe).Path
cargo test --locked --manifest-path vpn-windows-broker/Cargo.toml --features privileged-validation --test wireguard_process -- --nocapture
```

`build-wireguard.py --target linux --arch amd64` Linux yardımcısını `.tools/vpn-wireguard/linux-amd64/termterm-wireguard` altında üretir. `--target darwin --arch arm64` yalnız çapraz derlemedir. Betik yerel sabit Go 1.26.4 ister; kurulum yapmaz. Windows geliştirme paketleme betiği yardımcının ve MIT/BSD bildirimlerinin aynı dizine alınmasını zorunlu tutar.

Linux'ta root yalnız geçici ağ ad alanını yönetir:

```sh
python3 scripts/test-wireguard-lab.py --engine .tools/vpn-wireguard/linux-amd64/termterm-wireguard --report .tools/vpn-wireguard/linux-interop-report.json
```

29 Ekim 2026 kanıtı: gerçek Linux kernel WireGuard peer'iyle yeni el sıkışması ve 512 bayt UDP yankısı SHA256 eşleşti; yanlış anahtar, erişilemeyen peer ve kısmen bağlanan çok peer'li profil 30 saniyede başarısız oldu. İptal ve ana stdin kaybı geçti. Geçici namespace silindi; ana sistem IPv4/IPv6 rotaları ve DNS dosyası özeti değişmedi. Windows'ta gerçek yardımcı exe, doğrulanmış ebeveyn/job ve miras alınmış mesaj borusu üzerinden el sıkışması ve paket SHA256 geçti; adaptör/rota/DNS açılmadı. Ayrı UAC broker+Wintun kabulü henüz bu kanıtın parçası değildir. macOS yalnız kaynak ve çapraz derleme düzeyindedir; gerçek macOS veya şirket VPN'i test edilmedi.

30 Ekim 2026 son liveness kabulü: [son ikiliyle kernel raporunda](../artifacts/vpn-lab/wireguard-liveness-20261030-final.json) **12/12 vaka geçti**. Normal veri, yanlış anahtar, erişilemeyen/kısmen bağlanan peer, iptal ve süreç ölümü; boşta probe, 20 saniye kayıp sonrası toparlanma, çift yön kayıp, tek yönlerin her biri ve iki peer'den birinin kaybı doğrulandı. Kalıcı kayıplar 45.001–45.100 saniyede başarısızlık olayını süreç kapanmadan önce üretti. Tek scheduler-grace sertleştirmesini içeren Linux ikilisi SHA256 `4e8ec052044cafef3293a767caaa0643e0bc423cda7202c4fc2d4a6f328bd5be` kullanıldı; namespace silindi ve host rota/DNS snapshot'ı değişmedi. Windows/WSL Go kontrollerinde 12 test ve bir opt-in skip, `go vet` geçti. Önceki [yedi vakalık rapor](../artifacts/vpn-lab/wireguard-liveness-20261030-implementation.json) tarihsel kanıttır.

Aynı son yardımcıyla [gerçek native Team sessiz UDP kaybı](../artifacts/vpn-lab/native-team-ssh-silent-loss-20261030.json) ve [normal bağlantı kesme regresyonu](../artifacts/vpn-lab/native-team-ssh-manual-20261030-final.json) geçti. Gerçek HTTPS Team, şifreli kişisel VPN profili/host bağlantısı, SSH komut çıktısı ve SFTP SHA256 dönüşü sonrasında UDP kara deliği uygulandı. Sessiz kayıp 41.119 saniyede algılama ve kapanışı tamamladı; SSH/SFTP kapandı, duraklatılmış indirme iptal edildi, mevcut hedef korundu ve `.part` kalmadı. Fiziksel arayüzde hedef TCP paketi 0, journal ve sahip olunan TUN sayısı 0; namespace temizliği ve host ağı eşitliği geçti. [Kaynak/ikili/rapor parmak izi](../artifacts/vpn-lab/wireguard-liveness-20261030-source-receipt.json) test öncesi/sonrası kaynak eşitliğini taşır. Native örnek yalnız Linux `vpn-webkit-fixture` feature'ı ile derlenir; üretim IPC'si eklemez.

Windows adaptör/UAC liveness, fiziksel suspend ve macOS sınanmadı. Bu testler şirket VPN kimlik doğrulaması veya seçici veri-paketi filtresi algılama kanıtı değildir; yalnız el sıkışması erişilebilirliği ölçülür. Bağımsız native tekrar ayrıca raporlanacaktır.
