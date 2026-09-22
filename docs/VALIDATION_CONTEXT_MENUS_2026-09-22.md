# Sağ tık menüleri — doğrulama, 22 Eylül 2026

Sürüm: `0.3.3-dev.1`, dal: `feature/terminal-usability`. Kaynak ve ön yüz geliştirme çıktısı güncellendi. Bu çalışma için yeni kurulum paketi, GitHub Release veya otomatik güncelleme yayımlanmadı; mevcut paketler bu son değişiklikleri içermez.

## Son davranış

Kart/satırda tek tık salt okunur bilgileri açar. Host/grup kartlarında çift tık bağlantı/klasör açar. Ctrl/Command ve Shift ile kutucuksuz çoklu seçim yapılır; seçime özel üst işlem çubuğu kaldırıldı. Düzenleme, silme, taşıma, çoğaltma, kes/kopyala/yapıştır ve seçili şifreli yedekleme sağ tık menüsündedir.

Menüler host, grup, kimlik, snippet, tünel, workspace, known host, log, SFTP dosyaları ve üst terminal sekmelerinde kayda uygun işlemleri gösterir. SFTP çoklu silmede başarısız dosyaları ayrı bildirir; yeniden deneme yalnızca kalanları siler. Klasör silme boş dizinlerle sınırlıdır. Terminal metin alanındaki sağ tıkla yapıştırma korunur.

Grup çoğaltma alt grupları/hostları taşır, içerideki ilişkileri yeni kimliklerle eşler, dışarıdaki kimlik/jump host bağlantılarını korur. Taşıma alt gruba döngü oluşturamaz. Seçili yedek gerekli referansları içerir. Kullanılan kimliği silme isteği backend tarafından reddedilir.

## Sonuçlar

| Kontrol | Sonuç |
|---|---|
| Vitest | **17/17 geçti** |
| Playwright / Windows Edge | **27/27 geçti**, tam koşu 53,6 saniye |
| Windows native / gerçek Rust backend | **4/4 geçti** |
| `pnpm build` | TypeScript ve Vite üretim çıktısı başarılı |
| `git diff --check` | Başarılı |
| 10.000 host araması | UI testindeki ölçüm **37,4 ms**; genel performans garantisi değildir |

### Tarayıcı senaryoları

Salt okunur detaylar ve sırların gösterilmemesi; tek/çoklu taşıma-kopyalama; iç içe grup ilişkileri; kes/yapıştır; kayıt hatası ve tekrar deneme; hızlı çift işlem koruması; klavye/ekran kenarı/dış tık; tüm kayıt türleri için ilgili menüler; seçili backup komutunun doğru argümanları; gruptan toplu bağlantı; terminal sekme menüsü; SFTP hedef/patika argümanları; çoklu silmede kısmi hata ve yalnızca kalan dosyanın tekrar denenmesi.

Mevcut pano/terminal kısayolları, sekmeler, 16 panel, tema, kaynak göstergesi ve Updates UI testleri de aynı tam koşuda geçti. Tarayıcı IPC'si sentetiktir; bu testler gerçek SSH bağlantısı veya uzak dosya aktarımı olarak sayılmaz.

### Native Windows senaryoları

1. Gerçek `.ttvault` dosyasında grup ve alt hostları çoğaltma, chain/kimlik ilişkilerini karşılaştırma, host kopyasını başka gruba taşıma ve silme. Kasayı kilitleyip parola ile yeniden açınca kayıtların eşitliği. Dosyada örnek adres/parolanın düz metin bulunmaması.
2. Hâlen hostlar tarafından kullanılan kimliği UI menüsünden silme girişiminin gerçek backend tarafından reddedilmesi; kaydın korunması.
3. Native UI'da seçili export önizlemesi ve gerçek backend ile `.ttbackup` oluşturma/şifreyi çözerek geri okuma. Tek host için yalnızca host, üst grup ve kimliğinin çıktığı doğrulandı.
4. SFTP menüsünde gerçek yerel dosyayı yeniden adlandırma, iki yerel panel arasında kopyalama, Unicode içeriği karşılaştırma, klasör oluşturma, kaynağı silince hedef kopyanın korunması.

Native test, daha önce derlenmiş E2E Rust uygulaması üzerinde güncel Vite ön yüzüyle çalıştı; bu menü işinde Rust bağlantı/depolama kodu değiştirilmedi. Ayrı test uygulaması kimliği `local.termterm.desktop.e2e` kullanıldı. Test kasaları ve dosyaları `.lab` altında kaldı; son kasa işaretçisi eski içeriğine döndürüldü.

## Test sınırları

Embedded WDIO çift tık, select change ve contextmenu olaylarını eksik ürettiğinden native test yardımcıları bunları gerçek WebView içinde DOM olayı olarak gönderir. Playwright tarafında gerçek tarayıcı mouse/keyboard akışları ayrıca sınanır. Bu fark ilk native denemelerin başarısızlığına yol açtı; yardımcılar düzeltildikten sonra son koşu geçti.

Windows **Save as** diyaloğu native otomasyonda kontrol edilemedi; WDIO'nun global invoke mock'u uygulamanın ES-module dialog API'sini yakalamıyor. Native backup testi bu OS diyaloğunu çalıştırmaz: UI önizlemesini doğrular, dosya yolunu doğrudan gerçek `backup_bundle` komutuna verir ve dosyayı çözüp karşılaştırır. UI'nin seçili ID'leri backend'e aktarması Playwright'ta sınanmıştır. Native dosya seçicisinin uçtan uca otomatik doğrulandığı iddia edilmez.

Bu tur yeni uzak SSH/SFTP bağlantısı, Linux/macOS native çalışma veya paket kurulumu yapılmadı. Önceki bağlantı ve platform raporları kapsamlarıyla birlikte geçerlidir. Doğrudan donanım anahtarı adaptörleri gibi önceki ürün eksikleri bu UI değişikliğiyle tamamlanmış sayılmaz.

## Görseller ve kullanım

- `artifacts/context-review/native-host-menu.png`: Windows test kasasında host menüsü ve bilgi paneli.
- `artifacts/context-review/native-sftp-menu.png`: Windows'ta gerçek dosya kopyası ve SFTP menüsü.
- `artifacts/context-review/host-menu.png`, `sftp-menu.png`: tarayıcı test verileriyle aynı görünümler.
- [Kullanım kılavuzu](USER_GUIDE_TR.md), kart/sağ tık ve SFTP bölümleri güncellendi.

Referans alınan belgeler: [Termius gruplar ve Quick Connect](https://docs.termius.com/organize-and-connect-to-hosts/groups-and-tags), [Termius SFTP menüleri](https://docs.termius.com/organize-and-connect-to-hosts/managing-files-with-sftp). Belgelerdeki akışlar kullanıldı; belgelenmemiş tüm Termius menülerinin birebir eşitliği iddia edilmez.

## Yeniden çalıştırma

```powershell
pnpm.cmd test:unit
pnpm.cmd test:ui
pnpm.cmd build
```

Native test için ayrı terminalde `$env:VITE_E2E='true'; pnpm.cmd dev` başlatın. Test terminalinde `TERMTERM_BINARY` E2E yapılandırmasıyla derlenmiş debug uygulamasını göstermeli; `TERMTERM_SPEC='./tests/desktop/context-menu.mjs'` ile `pnpm.cmd test:native` çalıştırın. Normal üretim derlemesinde `VITE_E2E` etkinleştirilmez.
