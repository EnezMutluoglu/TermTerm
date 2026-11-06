# Windows VPN motor hazırlığı — 22 Ekim 2026

Bu belgede açıklanan hazırlık ve driverless motor kontrolleri kurulu TermTerm'i,
sürücüleri, adaptörleri ve ağ ayarlarını değiştirmez. Ayrı onaylanan yükseltilmiş
Windows ağ nesnesi testinin kapsamı ve sonucu ana VPN durum raporunda izlenir.
İndirilen kaynaklar/ikili dosyalar `.tools/vpn-windows/`, kanıtlar
`artifacts/vpn-windows/` içindedir; Git'e eklenmez. Bu hazırlık/driverless kontrolleri tek başına
Windows broker/kernel bağlantı kanıtı değildir. Sonraki scoped full-v9 gerçek Windows testi
24 payload ile geçti; bağımsız doğrulama ve hâlâ kapalı masaüstü readiness sınırı
[güncel durum raporunda](VPN_DEVELOPMENT_STATUS_TR.md) açıklanır.

## Tekrar çalıştırma

```powershell
python scripts/prepare-vpn-windows.py --self-test
python scripts/prepare-vpn-windows.py
python scripts/prepare-vpn-windows.py --build-core --probe-openconnect
python scripts/build-openconnect-windows.py --self-test
python scripts/build-openconnect-windows.py --prepare-only
python scripts/build-openconnect-windows.py
```

İlk komut yalnız kilit dosyasını denetler. İkinci komut sabit hash'li resmi
OpenConnect/Wintun arşivlerini ve eşleşen OpenConnect kaynak/lisansını indirip
doğrular. Son komut mevcut Visual Studio 2022 C++ araçlarıyla upstream OpenVPN 3
null-TUN örneğini derler; ardından vcpkg'nin yerel 7-Zip aracıyla OpenConnect
NSIS arşivini **çalıştırmadan açar** ve MSVC ile sürüm okuyan C ABI testi derleyip
çalıştırır. `--probe-openconnect` tek başına mevcut yerel 7-Zip'i kullanabilir.
Komutlar kurucu çalıştırmaz, sürücü yüklemez, adapter oluşturmaz ve sunucuya bağlanmaz.
İlk Core derlemesi vcpkg'nin sabitlediği derleme araçlarını ve bağımlılıkları da indirir.

`build-openconnect-windows.py` ayrı kaynak derlemesidir. MSYS2'nin arşiv sürümünü,
52 derleme paketinin URL/sürüm/SHA256 ve imza dosyalarını
`scripts/vpn-windows-msys2-lock.json` üzerinden doğrular. Pacman yalnız taşınabilir
kökün içinde imzalı yerel paketleri kurar; değişken güncel paket seçimi yapmaz.
Sabit resmi 9.21 kaynağına `--fuzz=0` dry-run ile iki dağıtılabilir yama uygulanır:
`openconnect-9.21-packet-handle.patch` ve `openconnect-9.21-mingw14.patch`.
İkincisi yalnız artık bulunmayan `sec_api/stdlib_s.h` include satırını kaldırır;
mevcut `stdlib.h`, `errno_t` ve `size_t` tanımlarını sağlar. Güvenli fonksiyonların
semantiği ve eksik bildirim derleyici kontrolleri değiştirilmez.

Değiştirilmiş DLL ayrı `openconnect-build/.libs/` dizininde kalır. MSVC ile derlenen
`scripts/test-openconnect-packet-handle.cpp` yeni export'u yükler ve yalnız yerel
pipe API'sinde geçersiz/event/byte-pipe reddini, message-pipe kabulünü, sahiplik
devrini ve mainloop öncesi free temizliğini denetler. Bu test sunucuya bağlanmaz ve
IP paketi taşımaz; paket aktarım/iptal sınır testlerinin yerine geçmez.

## Kaynaklar ve doğrulanan bölüm

- MSVC 14.44, Visual Studio Build Tools 2022 17.14, Windows SDK ve beraber gelen
  CMake 3.31.6/Ninja bulundu. MSVC x64 Windows SDK örneği derlenip çalıştı.
  Başlangıçta MinGW/MSYS2/Cygwin bulunmadı. Sonraki kaynak derlemesi için resmi
  MSYS2 20260927 arşivi yalnız `.tools/vpn-windows/msys2-portable` içine açılıyor;
  global kurulum yapılmıyor. SHA256 değeri resmi GitHub yayın özetiyle eşleşir.
- OpenVPN 3 kaynağı mevcut `vpn-engine/dependencies.json` içindeki
  `2f74c607f56294e39c9fab253f6dd87a2ed6e9ab` commit'idir. Bu commit'in resmi
  Windows iş akışı vcpkg `3aea538b2bb21a586502c67b00eb474fdd2e3098` ve
  `win-x64-release` preset'ini kullanır. MPL-2.0 seçimi korunur; OpenVPN 2 ile değiştirilmez.
  Deneme `CLI_OVPNDCOWIN=OFF`, `CLI_NULLTUN=ON` kullanır; sürücü gerektiren DCO yolu değildir.
- OpenConnect 9.21 resmi tag kaynağı `8b702bf2dbaf11302ed98629214b1df5d50a12aa`.
  GitLab pipeline `2605045915`, süresiz saklanan resmi yayın işi `14877945032`.
  Windows x64 GnuTLS ikilisi yalnız arşivden çıkarıldı. `openconnect.exe --version`
  `gp` ve `fortinet` dahil protokolleri listeler. `libopenconnect-5.dll` sürüm API'si
  hem Python'dan hem MSVC ile derlenen resmi başlık kullanan testten `v9.21` döndürdü.
  GNU `__attribute__((format(...)))` tanılaması MSVC testinde yerel makroyla atlanır;
  upstream başlık değiştirilmez. Bu sonuç protokol/sunucu bağlantısı testi değildir.
- Wintun 0.14.1 resmi ZIP SHA256 değeri yayınlanan değerle eşleşti. AMD64 DLL için
  Authenticode sonucu `Valid`, imzalayan `WireGuard LLC`. DLL yüklenmedi; sürücü kurulmadı.
  Kaynak GPL lisansı ile resmi imzalı DLL'nin ZIP içindeki ayrı `Prebuilt Binaries License`
  metni aynı değildir. Resmi DLL ve bildirimler korunmalı; uygulama yalnız izin verilen API'yi kullanmalıdır.

Sabit URL/commit/SHA256 değerleri `scripts/vpn-windows-dependencies.json` içindedir.
Kaynak tarball hash'i resmi HTTPS kaynağından indirilerek sabitlendi; bu çalışmada
PGP imzası ayrıca doğrulanmadı. Çıkarılan DLL hash envanteri ve sonuçlar
`artifacts/vpn-windows/preparation-20261022.json` içindedir.

Sonuç: iki yamalı DLL MinGW GCC 16.2/GnuTLS ile derlendi; SHA256
`59d60129ea53ca903448a17d61a25034a38849ceb64c13d2733be1b647f363bf`.
Yeni `openconnect_setup_tun_packet_handle` export'u MSVC testinden çağrıldı;
yerel API/temizlik kontrolleri **11/11 geçti**. Sürüm metni `v9.21-unknown`;
tam resmi kaynak ve iki yama hash'i raporda sabittir. Paket aktarımı yapılmadı.
MSYS2'nin 52 imzalı paketinin sabit yerel arşivlerle tekrar doğrulaması da geçti.
Tam `build-openconnect-windows.py` komutu baştan sona tek süreçte yeniden koşulmadı;
aynı configure/make adımları ve son API testi çalıştırıldı.

OpenVPN Core bağımlılıkları vcpkg ile tamamlandı. İlk upstream configure işlemi final cache
yazmadan sonlandı; yeniden bağımlılık indirmek yerine aynı `vcpkg_installed` dizini gerçek
`vpn-engine/windows` projesinde kullanıldı. Gerçek ExternalTun packet factory ile iki MSVC
helper derlendi; null-TUN hedefi bağlantı kanıtı olarak kullanılmadı. Komutlar ve log yolu
`docs/VPN_DEVELOPMENT_STATUS_TR.md` içindedir.

## Gerçek Windows bağlantısı için kalan işler

`vpn-engine` Linux namespace kilidi korunur; Windows için ayrı job/private pipe denetimi ve
ham IP packet bridge eklendi. Üç gerçek Windows engine iki OpenVPN ve bir ocserv tünelinde
eşzamanlı doğrulanmış paket taşıdı; ortadaki tünel kapanınca diğerleri sürdü. Bu kontrol
Wintun veya host ağ ayarı kullanmaz. Yükseltilmiş broker'ın Wintun/IP Helper ağ sahipliği ve
süreç ölümü/watchdog temizliği ayrı gerçek doğrulama gerektirir. OpenVPN Core'un Windows Wintun Setup
arayüzü Linux `TunBuilder` int fd yoluyla aynı değildir; varsayılan Setup'ın rota/DNS
değişiklikleri politika onayını atlayacak biçimde kullanılmamalıdır.

OpenConnect DLL C ABI'si MSVC'den çağrılabilir; ancak mevcut DLL'nin `os_setup_tun`
yolu mevcut adaptörleri benimseyebilir ve ilgisiz kapalı adaptörlerden IP adresi
silebilir. Bu API sahiplik sınırı için kullanılamaz. Gerçek port için sabit 9.21
kaynağında adapter/rota/DNS oluşturmayan, TAP IOCTL çalıştırmayan ek paket-HANDLE
başlatıcısı ve karşılık gelen LGPL kaynak yaması ile yeniden derleme gerekir.
Wintun ve ağ ayarlarının tek sahibi broker olmalıdır. ABI/sürüm testi bu güvenlik
işini tamamlanmış saymaz. Dağıtılacak GnuTLS ve diğer
DLL'lerin tam kaynak/sürüm/lisans zinciri henüz tamamlanmadı; manifest bu yüzden
`distributionReady: false` kalır. Hiçbir ikili ürün paketine otomatik eklenmez.

Gerçek Windows Wintun tünel trafiği, DNS/rota geri yükleme, uyku/uyanma,
Fortinet/GlobalProtect üretici sunucuları ve şirket kimlikleri test edilmedi.

Resmi kaynaklar:
[OpenVPN 3 sabit kaynak](https://github.com/OpenVPN/openvpn3/tree/2f74c607f56294e39c9fab253f6dd87a2ed6e9ab),
[OpenConnect Windows paketleri](https://www.infradead.org/openconnect/packages.html),
[OpenConnect yayın işi](https://gitlab.com/openconnect/openconnect/-/jobs/14877945032),
[OpenConnect derleme ve Wintun desteği](https://www.infradead.org/openconnect/building.html),
[OpenConnect lisansı](https://www.infradead.org/openconnect/licence.html),
[Wintun resmi imzalı ikilileri](https://www.wintun.net/).
