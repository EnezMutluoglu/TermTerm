# Ağ hızı, disk etkinliği ve doluluk uyarıları — 0.3.7-dev.1

Geliştirme dalı: `codex/feature-network-disk-stats` (0.3.6/develop tabanlı). Team geliştirmesi ayrı kalır. Kararlı sürüm/güncelleme kaynağı değiştirilmez.

## Kullanım

Terminalin Stats/İzleme panelindeki ikinci satır, gelen ve giden ağ hızını ayrı ayrı **Mbit/s**; disk okuma ve yazma hızını ayrı ayrı **MiB/s** gösterir. Satıra tıklayınca ağ arayüzü ve disk aygıtı seçilebilir. Hızlar bağlı hedef makinenin seçili aygıtına aittir; yalnız TermTerm trafiği veya belirli SFTP aktarımı değildir.

Linux'ta varsayılan rota arayüzü önceliklidir; yoksa ilk ölçümde etkin aygıt seçilir. Windows/macOS'ta sağlayıcı varsayılan arayüzü vermiyorsa ilk ölçümün etkin aygıtı kullanılır. Kullanıcı seçimi panel açık olduğu sürece korunur. Aygıt kaldırılırsa mevcut başka aygıta geçilir. Seçim ayrıntılarda ve satırın üzerine gelince görünür. Arayüzler, diskler, bölümler veya aynı kapasiteyi paylaşan mount'lar toplanmaz.

Diskler listesinde **%90 ve üzeri kritik**, kullanılabilir alan sıfırsa veya kullanılan alan toplam kapasiteye ulaşmışsa **dolu** uyarısı vardır. Rezerve alan nedeniyle yüzde 100'den düşük bir dosya sisteminin kullanılabilir alanı sıfır olabilir; bu durumda da dolu sayılır. Önce dolu, sonra kritik, sonra diğer mount'lar gösterilir. Ana panelde kök disk boş olsa bile diğer disklerin uyarı sayısı görünür. Sanal mount'lar mevcut tercihle gizlenir; kök mount her zaman değerlendirilir.

## Toplama ve hata davranışı

Ağ/disk etkinliği 2 saniyelik ayrı, sıralı ölçüm döngüsünde toplanır; kapasite ölçümü 10 saniyede kalır. SSH'de doğrulanmış hedef bağlantısının ayrı exec kanalı kullanılır. Chain sonunda hedef ölçülür. Terminale komut yazılmaz, ölçüm çıktısı terminal geçmişine girmez. Stats kapalı/gizli olduğunda döngü durur; tekrar açılınca başlangıç sayacı sıfırlanır. Süre veya sayaç geri giderse, aygıt kaybolup geri gelirse ya da 30 saniyelik büyük boşluk varsa ani sahte hız üretilmez.

İlk hız için iki örnek gerekir; eksik ölçüm sıfır değildir, **—** gösterilir. Ağ ve disk hataları ayrı taşınır. Tam probe hatası veya 10 saniyeden eski etkinlik örneği hızları kullanılamıyor olarak gösterir; CPU/RAM ve kapasite döngüleri çalışmayı sürdürür. Olayda `activity`, `activityAt`, `activityError` alanları bulunur. Kasa, PostgreSQL ve yedek biçimi değişmez; örnekler kalıcı depoya yazılmaz.

- Linux: `/proc/net/dev`, `/proc/uptime`, `/sys/block/*/stat`. Loopback ağ sayılmaz; loop/ram/zram/dm/md cihazları I/O seçiminden çıkarılır, alttaki tam blok aygıtları gösterilir. Bu fiziksel/çekirdeğin sunduğu aygıt etkinliğidir; mount bazlı I/O değildir. Sektör sayacı 512 bayt ile çarpılır. [Kernel I/O alanları](https://docs.kernel.org/admin-guide/iostats.html), [sektör birimi](https://kernel.org/doc/html/v5.12/block/stat.html).
- Windows: CIM raw NetworkInterface ve PhysicalDisk sayaçları. `_Total` eklenmez. Her aygıtın performans zaman damgası ve frekansı üzerinden fark alınır. [Microsoft fiziksel disk sayaçları](https://learn.microsoft.com/en-us/previous-versions/aa394308(v=vs.85)), [performans verisi](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/WmiSdk/monitoring-performance-data.md).
- macOS: `netstat -ibn`, `route`, IOBlockStorageDriver `ioreg` istatistikleri. Disk adı I/O registry sürücüsü/kimliğidir. Kaynak ve parser örnekleri hazırdır; Mac üzerinde çalıştırılmadı. [Apple network araçları](https://github.com/apple-oss-distributions/network_cmds), [IOBlockStorageDriver](https://developer.apple.com/documentation/kernel/ioblockstoragedriver/1811976-getstatistics).

## Kısa doğrulama — 28 Eylül 2026

- TypeScript/Vite derlemesi; 25 Vitest testi geçti.
- 4 yeni Playwright testi: birimler, aygıt seçimi, eski/hatalı örnek, Stats kapatma; kök olmayan kritik/dolu diskler ve sıralama; dar bölünmüş terminalde taşma/yeniden boyutlandırma; Türkçe etiketler. Mevcut palet (2) ve resize (1) kontrolleri de geçti.
- Rust ölçüm testleri: Windows 7/7 ve Ubuntu 22.04 chroot 7/7. Windows CIM ve Linux gerçek yerel probe; Linux delta/sıfırlama/aygıt kaybı; Windows/macOS parser örnekleri dahil. macOS örnek ayrıştırma, gerçek Mac doğrulaması değildir.
- Native Windows E2E 2/2: Linux SSH chain üzerinde gerçek hızlar, kontrollü 8 MiB dosya yazma/fsync/doğrudan okuma, ölçümün terminal çıktısını değiştirmemesi, kapat/aç döngüsü; Windows yerel terminalde gerçek ağ/fiziksel disk hızları. Koşu 27 saniye sürdü; uzun yük testi yapılmadı. Geçici dosya silindi.
- Kritik/dolu disk uyarıları sentetik mount örnekleriyle sınandı; kullanıcının diski doldurulmadı. Türkçe örnek ekran bu nedenle “Örnek ölçümler” olarak işaretlidir.
- E2E sürücüsü olmayan ayrı kimlikli Windows Stats Dev uygulaması derlendi; gerçek pencere açılışı/kapanışı ve 4445 test portunun bulunmadığı doğrulandı.
- Yeni macOS/donanım/üretim sunucusu testi ve kararlı yayın yapılmadı.

## Deneme derlemesi

Ayrı uygulama kimliği `local.termterm.desktop.stats-dev`; kurulu uygulamanın son kasa/parola kaydıyla karışmaz. E2E sürücüsü olmayan Windows deneme uygulaması:

```powershell
$env:TERMTERM_RELEASE_CHANNEL='development'
pnpm tauri build --debug --config src-tauri/tauri.stats-dev.conf.json --no-bundle
```

Bu debug derlemesi performans veya kurulum paketi ölçümü değildir. Gerçek kasaya yazmamak için denemede yeni bir test kasası veya yedekten oluşturulan ayrı kopya kullanın. Kasa dosyası taşıma/geri yükleme biçimi aynıdır. macOS ve GitHub derlemeleri başlatılmaz.
