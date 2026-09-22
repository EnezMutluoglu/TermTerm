# Arayüz renk sistemi

Uygulama renkleri `src/palette.css` dosyasında tanımlanır. Bileşen CSS'i ve JSX içinde yeni sabit renk eklemek yerine bu roller kullanılmalıdır. `src/style.css`, paleti doğrudan içe aktarır; tarayıcı önizlemesi ve masaüstü aynı renkleri kullanır.

| Rol | Kullanım |
| --- | --- |
| `--bg`, `--surface*` | Antrasit sayfa, panel, alan ve hover yüzeyleri |
| `--text`, `--text-secondary`, `--muted` | Başlık, normal metin ve yardımcı metin |
| `--accent`, `--accent-hover`, `--accent-bg`, `--accent-border` | Mavi gezinme, seçim, odak ve genel ikonlar |
| `--action-*` | Ana işlem butonları ve işaretli kutular |
| `--success*` | Bağlı oturum ve tamamlanan işlem |
| `--warning*` | Uyarı, eski ölçüm ve broadcast |
| `--danger*` | Hata, silme ve %90 üzeri disk doluluğu |
| `--metric-cpu`, `--metric-memory` | Aynı mavi ailesinde, açıklık farkıyla ayrılan grafikler |

Host/klasör ikonları kaydın adına veya sırasına göre rastgele renklenmez. Host seçimi, SFTP dosya seçimi ve etkin menü aynı seçim yüzeyini kullanır. Başarı/hata yalnızca renkle anlatılmaz; ikon, durum metni veya sayısal ölçüm de bulunur.

Terminal ANSI renkleri ayrı olarak `src/terminalThemes.ts` içinde kalır. Forest gibi yeşil bir terminal teması seçmek uygulama menülerini yeşile çevirmez. Komut çıktılarındaki kırmızı/yeşil gibi anlamlı ANSI renkleri korunur.

`tests/ui/palette.spec.ts`, gerçek bileşenleri sentetik kasa/dosya/ölçüm verisiyle açar; seçili durumların tutarlılığını, önemli metin ve grafiklerin en az 4.5:1 kontrastını, disk uyarısını ve tema ayrımını kontrol eder. Görseller `artifacts/palette-review` altında üretilir. Bu kontroller gerçek SSH, SFTP aktarımı veya işletim sistemi ölçüm testi olarak sayılmaz.
