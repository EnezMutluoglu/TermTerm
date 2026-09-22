# Kullanım ve yedekleme

## Kasa ve bağlantılar

1. **Create local vault** ile dosya konumu ve parola seçin. Varsayılan dosya platformun kullanıcı uygulama veri klasöründe `Personal.ttvault` olur.
2. **New host** ile isim/adres ekleyin. **Connection** sekmesinden kullanıcı, parola veya Keychain kimliği seçin.
3. **Advanced → Host chain** ile ara sunucuları sırayla ekleyin; proxy bilgilerini aynı bölümde girin. Grup bağlantı ayarları alt gruplara ve hostlara devredilir; boş bırakılan alanlar devralma için kullanılır.
4. İlk bağlantıda sunucu parmak izini kontrol edin. Değişmiş anahtar otomatik kabul edilmez; doğrulandıktan sonra eski **Known hosts** kaydı silinerek yeniden bağlanılır.
5. Host kartına çift tıklayın. Windows/Linux'ta terminal araması `Ctrl+Shift+F`, yerel terminal `Ctrl+Shift+T`, kasa kilidi `Ctrl+Shift+L`. macOS uygulama kısayollarında Command kullanır; terminalin Control tuşları shell'e gönderilir. Güncel liste **Settings → Shortcuts** ekranındadır.

**Hosts** ana dizininde üst klasörler görünür; gruba atanmamış hostlar **Ungrouped** klasöründedir. Bir grup klasörüne çift tıklayınca doğrudan o gruptaki hostlar ve alt klasörler açılır. Sol menüdeki klasörler tek tıkla gezinme içindir. Üstteki klasör yolu ile geri dönün. Arama açık klasörün hostlarını arar. Yeni host, açık gruba eklenir.

## Kartlar, bilgiler ve sağ tık menüsü

Host, grup, kimlik, snippet, tünel, workspace, known host veya log kaydına **tek tıklayınca** sağda salt okunur bilgiler açılır. Parola ve özel anahtar içeriği bu panelde gösterilmez. Düzenlemek için **sağ tık → Edit** kullanın. **Ctrl/Command + tık** ayrı kayıtlar, **Shift + tık** aralık seçer; seçim kutucuğu veya üstte seçime özel işlem çubuğu yoktur. Klavyeyle kart odaktayken **Shift+F10** menüyü açar; oklar, Home/End, Enter ve Escape kullanılabilir.

- Host: **Connect**, **Open SFTP**, düzenle, çoğalt, gruba taşı/kopyala, kes/kopyala/yapıştır, adres/kullanıcı adı kopyala, seçili şifreli yedek ve sil.
- Grup: **Open group**, **Quick connect hosts**, düzenle, alt gruplar ve hostlarla birlikte çoğalt/taşı/kopyala/yedekle/sil. Quick Connect mevcut oturumlarla birlikte en fazla 16 terminal açar. Grup çoğaltılırken içerideki host-chain ilişkileri yeni kayıtlara bağlanır; dışarıdaki kimlik/jump host referansları korunur. **Ungrouped** sanal bir klasördür; adı değiştirilemez veya klasör olarak silinemez.
- Diğer kayıtlar: türe göre snippet çalıştırma, tünel başlat/durdur, workspace açma, public key/parmak izi/log kopyalama. Başka kayıtlar tarafından kullanılan kimlik veya host silinmek istendiğinde kasa işlemi reddeder; önce ilişkileri düzenleyin.
- **Copy/Cut** seçtikten sonra hedef klasöre veya açık klasörün boş alanına sağ tıklayıp **Paste here** seçin. **Export encrypted backup** yalnızca seçimi ve ihtiyaç duyduğu grup/kimlik/chain referanslarını içerir.
- Üst terminal sekmesine sağ tıklayınca odak/bölünmüş görünüm, aynı hosta yeni bağlantı, sekme adını kopyalama ve kapatma işlemleri açılır. Terminalin metin alanındaki sağ tık, önceki tercihinize göre panodan yapıştırmaya devam eder.

## Terminal görünümü ve kaynak göstergeleri

**Settings → General → Terminal color theme** alanında Graphite, Midnight, Forest, Ember veya Paper seçin; **Save preferences** kasaya kaydeder. Açık terminal bağlantıları kapanmadan renkler değişir. Aynı ekrandaki **Local shell** Windows'ta PowerShell/pwsh/cmd/WSL seçtirir; Unix'te kurulu shell'in tam yolu kullanılır. **External editor** isteğe bağlı editör çalıştırılabilir dosyasının tam yoludur.

Her terminal başlığındaki **Stats** kutusu o panelin göstergesini açar/kapatır. CPU/RAM yaklaşık 2 saniyede, diskler 10 saniyede güncellenir. Disk göstergesine tıklayarak diğer mount noktalarını açın; sanal dosya sistemleri **Show virtual mounts** ile görünür. Aynı kapasiteyi paylaşan mount'lar toplanmaz. Dar panellerde kısa değerler, açılır ayrıntıda tam kapasite bilgisi bulunur.

SSH ölçümleri mevcut doğrulanmış bağlantıda ayrı exec kanallarını kullanır; interaktif terminale komut yazmaz. Yerel terminal bulunduğu sistemi, Windows'ta WSL shell seçildiyse varsayılan WSL dağıtımını ölçer. Telnet/seri/paylaşılan terminal konuk görünümünde ölçüm kaynağı bulunmaz. Erişilemeyen ölçümler sıfır gösterilmez; hata veya **Stale** görünür. Gizli/kapalı göstergeler duraklar; workspace kaydı Stats tercihlerini saklar. Grafik geçmişi yalnızca RAM'dedir.

**Keychain** parola, OpenSSH/PEM/PPK anahtar ve SSH sertifikası saklar. Şifreli anahtar importundan önce passphrase alanını doldurun. Ed25519 anahtar oluşturulabilir. **Install public key** POSIX sunucunun `~/.ssh/authorized_keys` dosyasına anahtar ekler; mevcut anahtarları silmez.

Windows OpenSSH agent ve Pageant; Unix'te `SSH_AUTH_SOCK` üzerinden agent imzalama desteklenir. Donanım anahtarı agent tarafından sunuluyorsa özel anahtar uygulamaya alınmaz. TermTerm içinde yerel FIDO2/Windows Hello anahtar kaydı bu sürümde yoktur.

Mosh için sunucuda `mosh-server` ve UTF-8 locale gerekir. SSH el sıkışması chain/proxy üzerinden yapılabilir; UDP sunucuya doğrudan ulaşmalıdır. **Mosh UDP address** alanı WSL/NAT gibi ayrı adres kullanılan durumlar içindir. Mosh istemcisi Windows paketine dahildir.

## 0.3.3 geliştirme sürümünde terminal kullanımı

Vault ve SFTP yanında her bağlantının kendi üst sekmesi vardır. Sekme değişimi bağlantıyı kapatmaz. Bölme, broadcast, paylaşım ve workspace kaydetme araçları terminalin altındaki ince çubuktadır. Stats her panelin sağ üstünde kalır.

Fareyle seçip bırakınca metin panoya kopyalanır; sağ tık yapıştırır. Windows/Linux: **Ctrl+Insert** veya **Ctrl+Shift+C** kopyalar; **Shift+Insert** veya **Ctrl+Shift+V** yapıştırır. macOS: **Command+C/V**. Vim/tmux fareyi kullanıyorsa **Shift** basılı tutarak yerel seçim/yapıştırma yapın. Çok satırlı yapıştırma terminalin bracketed-paste moduna uyar.

**Ctrl+Tab / Ctrl+Shift+Tab** üst sekmelerde ilerler/geri gider. **Ctrl+Shift+W** aktif terminali kapatır (macOS Command+W). **Ctrl+Shift+F** terminal aramasını açar; Shift+PageUp/PageDown geçmişi kaydırır. Shell'in Control+C/D/Z/L/K/N tuşları korunur. Diğer gerçek kısayollar **Settings → Keyboard** bölümündedir. **Settings → General** içindeki seçimle kopyalama ve sağ tıkla yapıştırma tercihleri **Save preferences** ile kasaya kaydedilir.

Sol alttaki **Updates + sürüm** bağlantısı doğrudan güncelleme ekranını açar. Kontrol ve kurulum kullanıcı tarafından başlatılır; geliştirme paketi kararlı sürümü otomatik kurmaz.

## Taşınabilir dosya ve tam yedek

- Uygulama kapalıyken `.ttvault` dosyasını kopyalayabilirsiniz. Başka Windows hesabında aynı kasa parolasıyla açılır.
- Uygulama açıkken **kasa menüsü → Save portable copy** kullanın. SQLite'ın tutarlı yedeği alınır; yeni cihaz kimliği oluşturulur, bekleyen gönderim kuyruğu kopyadan temizlenir.
- **Create backup** bir veya daha fazla kapalı ek kasayı `.ttbackup` içine alır. Yedek parolası kasa parolasından farklı olabilir.
- PostgreSQL/entegrasyon profilleri ayrı seçimle dahil edilir. Varsayılan olarak dışarıda bırakılır.
- Dosya yoluyla kullanılan SSH anahtarları için **Include referenced SSH key and certificate files** seçeneği vardır. Eksik dosyalar ve donanıma bağlı anahtarlar raporlanır.
- **Restore a backup** önce kaydın içeriğini ve sayılarını gösterir, ardından yeni `.ttvault` oluşturur. Mevcut dosyanın üzerine yazılmaz. Yeni parola seçilmezse yedek parolası kullanılır.
- Mevcut kasaya birleştirmek için **Import connections → TermTerm encrypted backup/vault** seçin. Önizleme ve yinelenen kayıt politikası uygulanır.
- Restore'da kayıt kimlikleri ve ilişkiler yeniden eşlenir. Sync profilleri pasiftir; eski gönderim kuyruğu/otomatik bağlantılar çalıştırılmaz.

Parolayı hatırlama Windows'ta DPAPI, macOS'ta Keychain, Linux'ta Secret Service kullanır. Güvenli depo yoksa hatırlama kullanılamaz; parola ile devam edin. **Kasanın kendisi bu cihaz depolarına bağımlı değildir.** Hatırlanan parola başka bilgisayara taşınmaz; kasayı gerçek parolayla açmanız gerekir.

`.ttbackup` kayıpsız taşıma biçimidir. CSV ve OpenSSH sınırlı alanları temsil eder. CSV parolaları varsayılan dışarıda bırakır; açıkça seçildiğinde dosyada düz metin bulunur. Dışa aktarım önizlemesindeki alan kaybı raporunu okuyun. CSV formül olarak yorumlanabilecek hücreler korumalı yazılır.

## Import

OpenSSH `config`, `known_hosts`, CSV, PuTTY `.reg`, MobaXterm düz metin bookmark dosyaları, SecureCRT XML bağlantı alanları ve Ansible INI/YAML adaptörleri vardır. CSV için **Map CSV columns** kullanabilirsiniz. `templates` klasörü örnekler içerir.

`.reg` kayıt defterine uygulanmaz. `ProxyCommand`, `Match exec` ve `LocalCommand` import sırasında çalıştırılmaz. `Include` yalnızca başvurduğu config dosyalarını okur; glob ve döngü sınırları vardır. `.ini` dosyaları içerikten MobaXterm/Ansible olarak ayırt edilir. Desteklenmeyen alanlar ve eksik kimlikler kayıt bazında önizlenir. Üreticiye özgü şifreli MobaXterm/SecureCRT kapsayıcıları ve şifreli parolalar henüz çözülemez; kaynak programdan düz bağlantı dışa aktarımı veya CSV kullanın, eksik kimlikleri Keychain'e ekleyin. Termius'un kaynak dosyada vermediği gizli kayıtlar otomatik elde edilmez. Ayrıntılar: [import kapsamı](IMPORT_SUPPORT.md).

## SFTP

**Local** paneli bu bilgisayardaki dosya ve klasörleri gösterir; yol alanına örneğin `C:\Users\sonx\Desktop` veya başka bir sürücü/klasör yolu girilebilir. Erişim mevcut işletim sistemi hesabının dosya izinlerine bağlıdır. Diğer panelde uzak bağlantı seçin. Seçili dosyaları **sağ tık → Copy to target directory** ile ya da paneller arasında sürükleyerek aktarın. Uzak–uzak ve klasör aktarımları desteklenir. Sembolik bağlantılar otomatik izlenmez. Hatalı aktarımı kuyruktan yeniden deneyebilirsiniz.

Dosyaya tek tıklayınca altta ad/boyut/izin bilgisi görünür. Sağ tık menüsünden yol kopyalama, yeniden adlandırma, silme; uzak dosyalarda izinler ve harici editör açılır. Boş alana sağ tıklayarak klasör oluşturabilir, tümünü seçebilir veya yenileyebilirsiniz. Çoklu silmede başarıyla silinenler geri alınmaz; hata verenler bildirilir ve yeniden deneme yalnızca kalanları hedefler. Silinecek klasörler boş olmalıdır.

Varsayılan olarak mevcut dosyanın üzerine yazılmaz. Seçildiğinde aktarım önce geçici kardeş dosyaya tamamlanır. Uzak SFTP v3 sunucusunda eski dosya geçici ad altında korunarak değişim yapılır; süreç tam ad değiştirme anında kapanırsa `termterm-previous` dosyası kurtarma için kalabilir. Yerel dosyalar tamamlandığında yeniden adlandırılır.

**Sağ tık → Open in external editor** uzak dosyanın en fazla 32 MiB'lık geçici düz kopyasını açar. Editör tercihi boşsa Windows Notepad, macOS metin dosyası uygulaması, Linux `xdg-open` kullanılır. Kaydedip **Upload saved changes** seçin. Sunucudaki sürüm değişmişse üzerine yazma ayrıca gerekir. **Finish editing** geçici kopyayı temizler. Kasayı kilitlemeden önce düzenlemelerinizi yükleyin ve aktarım kuyruğunun bitmesini bekleyin. Kuyruktaki **Pause/Resume/Cancel** işlemleri aşağıda açıklanır.

## PostgreSQL ve ekip

`Settings → PostgreSQL sync` içine profil, adres, port, veritabanı, şema, kullanıcı, parola ve CA girin. TLS/hostname doğrulaması zorunludur. Lab profili `localhost:55432` kullanır.

Yeni sunucuda migration hesabıyla SQL dosyalarını çalıştırın, `grant_app.sql` yetkilerini her ayrı uygulama login'i için uygulayın. Uygulama login'i migration hesabıyla aynı olmamalıdır. **Upload local vault** yeni uzak kasa oluşturur; **Open remote vault** uzak kasayı yeni yerel dosyaya indirir. Uzak kasayı çözmek için üyeye ait kasa parolası da gerekir.

**Test connection** eksik tabloları otomatik oluşturmaz. PostgreSQL içindeki yedi tablo, şifreli kayıt modeli, mevcut ilk kurulum ve önerilen sihirbaz [PostgreSQL akış belgesinde](POSTGRES_FLOW.md) açıklanır.

**Sync now** veya arka plan eşitlemesi kullanılabilir. Her değişiklik önce yerelde kaydolur. WSL/DB kapalıyken yerel çalışma devam eder. Arka plan eşitleme açıkken bağlantı geri geldiğinde 15 saniyelik yedek zamanlayıcı yeniden dener. Çakışmalar **Keep local/remote/both** ile çözülür. Birden fazla veritabanına aynı kasayı eşzamanlı bağlamak bu sürümün desteklenen kullanım şekli değildir.

**Team vault** ekranında mevcut PostgreSQL login'lerine owner/editor/viewer verin. Her üyeye ayrı kasa parolasıyla sarılmış anahtar atanır. Üyelikten çıkarma gelecekteki DB erişimini keser; üyenin daha önce indirdiği yerel kopyayı silemez.

Kasa menüsündeki **Shared terminals** veya terminal araç çubuğundaki zincir simgesi canlı terminal paylaşır. Sahip tek editöre yazma hakkı verir; viewer yalnızca izler. Ortak terminal verisi şifreli, kısa ömürlü DB iletileridir. Girdiler iki saniyede, çıktılar otuz saniyede erişilemez olur; sonraki heartbeat fiziksel temizliği yapar. Sahip bağlantısı kesilirse oturum on saniye içinde sona erer. Eski girdiler tekrar yürütülmez.

## Entegrasyonlar

AWS için kurulu/yapılandırılmış AWS CLI v2 profili ve bölge gerekir. DigitalOcean için yalnızca okuma yetkili token kullanın. Keşif kaynak oluşturmaz/silmez; bulunan hostları önizleyip içe alır.

**API Bridge** açıkça başlatılınca rastgele loopback portunda çalışır. Bearer token her başlangıçta değişir. `GET /v1/hosts` sırları döndürmez; `POST /v1/hosts` yalnızca id/label/address/port/username/tags alanlarını kabul eder. Browser Origin, komut/SQL alanları ve yetkisiz istekler reddedilir. Kasa kilidinde servis durur.


## 0.3 import ve aktarım kontrolleri

Eski MobaXterm dosyalarında Türkçe karakter kodlaması için Import ekranında **Text encoding → Turkish (Windows-1254)** seçin. Önizleme ilk 100 kaydı sayfalayarak gösterir. Boş klasörler de alınır. Sonuç bildirimi eklenen, güncellenen, atlanan ve başarısız kayıtların gerçek sayılarını gösterir. Uzun önizleme/uygulama sırasında Cancel kayıt işlemini commit öncesinde iptal eder; mevcut kasada yarım kayıt kalmaz.

SFTP kuyruğunda Pause/Resume/Cancel bulunur. Devam aynı geçici dosya üzerinden yapılır; kaynak boyutu/zamanı değişmişse dosya yeniden başlatılır. Cancel hedefte daha önce bulunan dosyayı korur. Son yeniden adlandırma kısa bir commit adımıdır; bu adım başladıktan sonra iptal kabul edilmez. Klasör aktarımında daha önce tamamlanmış dosyalar korunur; klasörün tümü tek işlem değildir.

Settings → PostgreSQL sync altında bağlantı testi şema/sürüm durumunu da gösterir. Şema eksikse ayrı migration kimliğiyle hazırlanabilir. Mevcut kasanın ilk bağlantısında **Preview & connect** kullanın; hedef değiştirme veya önizleme sırasında kayıtların değişmesi yeniden inceleme gerektirir.
