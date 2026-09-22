# TermTerm 0.3.0 teslimat durumu

21 Eylül 2026. Windows x64 ve Ubuntu 22.04 tabanlı Linux x64 paketleri için çalışma sürümü. macOS Intel/Apple Silicon derlemesi kullanıcı isteğiyle bekletildi; kaynak, hazırlık betikleri ve CI matrisi hazırdır.

Uygulama gerçek SSH/SFTP ve şifreli dosya kasası kullanır. Tam plandaki her donanım ve üretici özelliği tamamlandı olarak sunulmaz. Güncel ölçümler ve kalan doğrulamalar [0.3 test raporunda](VALIDATION_0.3.md) bulunur.

## 0.3 değişiklikleri

- Import önizleme/uygulama işleri UI dışına alındı. İşlem kimliği, ilerleme, iptal, çift tıklama koruması, atomik commit, değişen kasa hedefi kontrolü ve gerçek eklenen/güncellenen/atlanan sayıları eklendi.
- Kaynak dosya değişmeden UTF-8/BOM/UTF-16 ve seçilebilir Windows-1254/1252/1251 kodlamaları okunur. Geçersiz portlar varsayılana dönüştürülmez. Boş MobaXterm klasörleri korunur. Kasa importu yalnızca kaynak dosyanın şifreli geçici kopyasını açar.
- SFTP'de iptal, duraklat/devam, aynı geçici dosyada sürdürme, kaynak değişirse yeniden başlatma ve 30 saniyelik ilerlemesiz I/O sınırı vardır. İptal mevcut hedef dosyayı değiştirmez. Native dosya bırakma olayları bağlandı; gerçek masaüstü sürükleme hareketi ayrıca doğrulama bekliyor.
- Windows ölçümünde ilk sorguya 20, sonraki sorgulara 8 saniye bütçe ayrılır; sorgular üst üste birikmez. Stats tercihi, tema, 16 panel ve klavye davranışı korunur.
- PostgreSQL şema kontrolü, ayrı migration kimliğiyle hazırlık ve ilk bağlantı/aktif hedef önizlemesi eklendi. Farklı yerel içerik çakışma olarak korunur. [Veri yapısı ve kurulum](POSTGRES_FLOW.md).
- SFTP ilk açılana kadar yüklenmez; host arama metni kayıtlar değiştiğinde indekslenir. Klasör görünümü ve 120 kayıtlık sayfalama korunur.

## Kapsam sınırları

- Doğrudan Windows Hello/TPM ve FIDO2 kayıt/imzalama adaptörleri henüz yoktur. OpenSSH agent/Pageant köprüsü vardır; fiziksel donanım ve seri port testleri yapılmadı.
- MobaXterm/SecureCRT üreticiye özel şifreleme çözücüleri yoktur; kaynakta bulunmayan veya çözülemeyen parola alanları aktarılmış sayılmaz. Bütün ileri gateway/proxy/positional alanları desteklenmez. Kullanıcının gerçek MobaXterm oturum dosyasındaki 414 host ve 155 klasör bağlantı alanları/ilişkileriyle doğrulandı; üretici sürümü belirtilmemiştir.
- MFA/SSH sertifika kod yolları vardır; ayrı MFA sunucusu ve sertifika CA kabul testi tamamlanmadı. Gerçek kullanıcı editörüyle SFTP düzenleme, bulut hesabıyla keşif ve macOS testleri yapılmadı.
- Üye çıkarıldıktan sonra otomatik kasa anahtarı rotasyonu yoktur. Log notları vardır; zaman çizelgesine bağlı işaretleme arayüzü yoktur. Çeviri altyapısı bütün metinlere uygulanmadı.
- Windows paketleri imzasızdır. macOS yapılandırması ad-hoc imzalı kişisel test içindir; notarization yoktur. Sıcak açılış <2 saniye ve tüm donanımlarda performans garantisi verilmez.

Kişisel bağlantı dosyaları, `.lab` kimlik bilgileri ve test kasaları teslim kaynak arşivine dahil edilmez. Normal uygulama paketinde E2E WebDriver bulunmaz. PostgreSQL kullanmadan yerel kasa açılabilir.
