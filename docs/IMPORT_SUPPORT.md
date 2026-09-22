# 0.3 import kapsamı

Kaynaklar değiştirilmez ve kaynak dosyadaki komutlar çalıştırılmaz. Önizlemedeki `ready`, `review`, `missing-identity`, `skipped` durumlarını ve Import report bölümünü inceleyin. Varsayılan kopyalama yeni kayıt kimlikleri üretir. Güncelle/atla eşlemesi grup yolunu, host adresini, portu ve kullanıcıyı dikkate alır. Group, identity, chain, tunnel, workspace ve inventory ilişkileri yeniden eşlenir.

| Kaynak | Uygulanan | Sınır |
|---|---|---|
| TermTerm | 0.1/0.2/0.3 kasa ve yedekleri, ilişki remap, yeni cihaz kimliği | Donanım anahtarı taşınmaz; sync profilleri kendiliğinden başlamaz |
| OpenSSH | Include/glob, Include döngü/boyut sınırı, ilk değer önceliği, wildcard/negation, HostName/User/Port, IdentityFile/CertificateFile, ProxyJump, TCP Local/Remote/DynamicForward | Match, ProxyCommand, LocalCommand, RemoteCommand çalıştırılmaz. Ek IdentityFile ve Unix socket tünelleri raporlanır. Göreli Include/key yolları OpenSSH kullanıcı semantiğiyle `~/.ssh` altında çözülür |
| CSV | İçerik algılama, UTF-8/BOM/UTF-16, seçilebilir Windows-1254/1252/1251, alıntılı/çok satırlı alanlar, Termius alanları, sütun eşleme, grup/etiket | Geçersiz satır/eksik adres raporlanır; düz metin export sırları varsayılan olarak içermez |
| PuTTY | Registry metni, oturumlar, PPK yolu, HTTP/SOCKS5 kimlik doğrulaması, local/remote/dynamic TCP forwarding | Registry içe uygulanmaz; diğer proxy türleri ve gelişmiş seçenekler raporlanır |
| Ansible | INI/YAML, vars/children, üst-alt grup ve host önceliği, aynı seviyede group_priority/ad sırası, çoklu üyeliklerin hosta uygulanması, SSH key/common_args ProxyJump | Jinja, dynamic inventory, ayrı group_vars/host_vars dosyaları, Vault ciphertext ve aralıklar yürütülmez/çözülmez; desteklenmeyen değişkenler raporlanır |
| MobaXterm | `.ini` içeriğinden tanıma, `.mxtsessions` dahil Bookmark SSH bağlantıları, boş/iç içe klasörler, tanınan anahtar yolları | Üreticiye özel gateway/proxy/tünel positional alanlarının tamamı çözümlenmiş değildir. Şifreli parolalar çözümlenmez |
| SecureCRT | XML session/gruplar, anahtar/sertifika yolu, adla çözülebilen gateway, açık TCP forwarding alanları | Şifreli credential alanları, üreticiye özel port-forward listelerinin tüm sürümleri ve firewall profilleri tam desteklenmez |

21 Eylül 2026 tarihinde kullanıcı gerçek bir `.mxtsessions` dosyası sağladı. Windows-1254 kodlaması seçilerek 414 SSH oturumu ve 155 klasör aktarıldı; ad/adres/port/kullanıcı/klasör ilişkileri, şifreli yeniden açılış ve kaynak SHA-256 eşitliği doğrulandı. Dosyanın üretildiği MobaXterm sürümü bildirilmedi. MobaXterm 26.5 ve SecureCRT 9.7.3 sürüm bilgisi doğrulanmış şifreli exportlar sağlanmadı. Bu sürümlerde şifreli import uyumluluğu **doğrulanmadı**. Parola alanının bulunması desteklenmeyen üretici şifrelemesinin çözülebildiği anlamına gelmez. Kaynağın içerdiği düz bağlantı alanları aktarılır; çözülemeyen alanlar uyarı olarak kalır. Tam ve kayıpsız TermTerm taşıması `.ttbackup` ile yapılır.

Tekrarlanabilir sentetik sınır örnekleri `tests/import/corpus` altında, beklenen sonuçlar `tests/import/expected.json` içindedir. Kullanıcının kişisel dosyası fixture veya kaynak ZIP'ine kopyalanmaz. `tests/desktop/vendor-import.mjs`, açıkça verilen `TERMTERM_IMPORT_FILE` yolu üzerinden yerel kabul testi çalıştırır; kişisel sunuculara bağlanmaz.


UTF-8 olmayan eski exportta ilk önizleme anlaşılır kodlama hatası verir. **Text encoding → Turkish (Windows-1254)** seçilip Preview tekrarlanır. Kasa/yedek dosyalarında bu seçim etkisizdir. Taşınmayan özel anahtar dosyaları ve üreticiye özel seçenekler kayıt bazında raporlanır; host kayıtlarının gelmesi parolaların/harici anahtarların da mevcut olduğu anlamına gelmez.
