# TermTerm Team 0.4.1 kullanım ve yönetim kılavuzu

Bu belge **0.4.1** sürümü içindir. Kurulum için `TEAM_SERVICE_INSTALL_TR.md`, sonraki sürümlere geçiş için `TEAM_UPGRADES_TR.md` belgesini kullanın. Doğrulama ve ertelenen ortamlar `VALIDATION_0.4.1.md` içinde açıklanır.

## Okuma sırası ve ilk kurulum

1. Sunucu yöneticisi `TEAM_SERVICE_INSTALL_TR.md` içindeki Compose veya systemd yolunu tamamlar; `/ready` ve HTTPS CA kontrolünü yapar.
2. Aynı belgedeki `manage.py bootstrap` komutuyla ilk lideri oluşturur. Oluşan takım kimliği, HTTPS adresi ve geçici parolayı yalnız lidere iletir.
3. Lider **Team hesabı** üzerinden ilk girişte parolasını değiştirir; Team yönetiminden şirket kurtarma dosyasını oluşturup sunucudan ayrı şifreli yedekler.
4. Ortak kasa, klasör ve hostları oluşturur; aşağıdaki adımlarla kullanıcıları ekler ve yalnız gereken kaynakları açar.
5. Bir test kullanıcısıyla izinli hosta bağlantıyı ve izinsiz hostun görünmemesini kontrol eder. Sonra kullanıcı dağıtımına geçer.

## Kullanıcı oluşturma ve izin verme

Lider **Team → Üyeler ve yetkiler → Kullanıcı oluştur** yolunda kullanıcı adı, görünen ad ve başlangıç rolünü seçer. 21 karakterli geçici parola tek sefer gösterilir ve 24 saat geçerlidir. Güvenli kanalla kullanıcıya iletin, sonra **Kaydettim, kapat** deyin. Kullanıcı aynı HTTPS adresi ve takım kimliğiyle ilk girişini yapıp yeni parolasını belirler. İlk aktivasyon bitmeden kaynak anahtarları bu kullanıcıya dağıtılamaz.

Hesap açılması veya rol seçilmesi tek başına makinelere erişim sağlamaz:

| Rol | Kaynak izni verildiğinde kullanım |
|---|---|
| Gözlemci | İzin verilen bilgileri görüntüler; bağlantı açamaz |
| Operatör | Görüntüleme ve bağlantı; ortak kayıt düzenleme yok |
| Editör | İzinli kaydı görüntüler, bağlantı açar ve düzenler |
| Kapsam yöneticisi | Sahibin ayrıca devrettiği kaynak ve izin sınırında yönetir |
| Sahip | Takım, hesap ve yetki yönetimi |

**Hosts** alanında klasör veya hosta sağ tıklayıp **Üye izinleri** açın. Etkin üyeyi seçin; **Görüntüle**, **Bağlan**, **Düzenle** satırlarında **İzin ver / Engelle / Devral** seçin. **Değişikliği önizle** ile etkilenen kayıtları inceleyin, sonra **Anahtarları hazırla ve izinleri uygula** deyin. Klasör izni alt kayıtlara yayılır; açık engel devralınan izinden önce gelir. “Devral” mevcut açık tercihi kaldırır, kendi başına izin vermez.

- Sadece iki makine açılacaksa geniş klasör izni vermeyin; iki hosta ayrı izin uygulayın. Üst klasörler yalnız gezinme yolu olarak görünebilir.
- Üretim klasörünü açıp hassas alt klasörü kapatmak için o alt klasöre açık engel koyun; kullanıcıyla sonucu kontrol edin.
- Kapsam yönetimini sahibin **Yetki devri** ekranından ayrıca tanımlayın. Yönetici kendisini yükseltemez veya atanmış kapsamı genişletemez.
- Host bağlantı izni özel anahtarı görme/dışa aktarma izni değildir. Ortak kimlik paylaşımında kullanma, görme ve dışa aktarma izinlerini ayrıca seçin.

Üyenin **Kimliklerim** alanındaki özel anahtarı/parolası kendisine aittir. Hostta **Kişisel bağlantı profilim** ile seçer; ortak kullanıcı adı zorunluysa kullanıcı adını değiştiremez. Böylece aynı ortak hosta farklı kullanıcılar kendi kimlikleriyle bağlanır.

**Parolayı sıfırla** yeni geçici parola üretir ve oturumları iptal eder; eski şifreli sırları silmez veya kendiliğinden açmaz. Gerekirse ayrı **Kimlikleri kurtar** akışını çalıştırın; kullanıcı kendi cihazında kurtarmayı doğrular. **Hesabı devre dışı bırak** erişimi kaldırır, geçmişi silmez. Çevrimdışı cihazın yeni iptali bağlantı kurulana kadar öğrenemeyeceğini hesaba katın.

## Masaüstü ve bağlantı profili

TermTerm Windows/Linux üzerinde yerel masaüstü uygulamasıdır. HTTPS yalnız uygulamanın Rust katmanıyla Team Service arasında hesap, izin ve şifreli kayıt iletişimidir. Servis bir tarayıcı arayüzü sunmaz. SSH/SFTP trafiği bilgisayardan hedefe gider.

Kullanıcıya yalnız aşağıdaki profili dağıtın. PostgreSQL adresi, DB kullanıcı adı veya DB parolası bu profile eklenmez.

```json
{
  "serviceUrl": "https://team.company.example",
  "teamId": "TAKIM-UUID",
  "caPem": null
}
```

Şirket CA'sı kullanılıyorsa `caPem` alanına public CA sertifikasını koyun. Özel sertifika anahtarını dağıtmayın. Sunucu adı sertifikayla eşleşmelidir; TLS doğrulamasını kapatma seçeneği yoktur.

Masaüstünde **Team hesabı** seçilir. Liderin oluşturduğu kullanıcı adı ve geçici parola ile giriş yapılır; ilk girişte yeni parola belirlenir. Hesap etkinleşmesi host erişimi vermez; lider klasör/host izinlerini ayrıca tanımlar.

## Sunucu kurulum dosyaları

`team-service/deploy` altında mevcut TLS PostgreSQL için Compose, Caddy ve systemd örnekleri bulunur. Mevcut TLS PostgreSQL ve ayrı PostgreSQL içeren Compose seçenekleri ile systemd kurulumu laboratuvarda doğrulandı. Ayrıntılı adımlar `TEAM_SERVICE_INSTALL_TR.md` içindedir.

- Uygulama servisi yalnız `127.0.0.1:41841` üzerinde dinler.
- Caddy dışarıya HTTPS sunar. Public DNS için `Caddyfile`, şirket CA denemesi için `Caddyfile.internal` örneğini kullanın.
- PostgreSQL bağlantısı `verify-full` doğrulaması, uygun DNS adı ve public CA gerektirir.
- Migration hesabı şemanın sahibidir. Çalışma zamanı hesabı superuser, `BYPASSRLS`, tablo sahibi veya migration rolünün üyesi olamaz.
- `/etc/termterm-team/service.json` DB parolası içerir; yalnız servis hesabına okunabilir yapın. Örnek dosyayı gerçek sırlarla kaynak depoya koymayın.

## Şema hazırlığı ve ilk lider

DB yöneticisi ayrı veritabanı ve iki ayrı giriş rolü hazırlar: migration hesabı ve sınırlı servis hesabı. İki rolün parolalarını PostgreSQL'in etkileşimli `\password` komutuyla veya şirket sır yönetimiyle verin. `pgcrypto` kurulabilmelidir; uygulama kendi yetkisini yükseltmez.

Migration betiğini **migration hesabıyla**, public PostgreSQL CA'sı ve yalnız sahibine okunabilen `PGPASSFILE` kullanarak çalıştırın:

```bash
export PGHOST=postgres.company.example PGPORT=5432
export PGDATABASE=termterm_team PGUSER=termterm_team_migrator
export PGSSLROOTCERT=/etc/termterm-team/postgres-ca.crt
export PGPASSFILE=/secure/termterm-migrator.pgpass
export TERMTERM_SERVICE_ROLE=termterm_team_service
bash team-service/deploy/migrate.sh
```

Betik uygulanmış migration'ları tekrar uygulamaz; açık fonksiyon izinlerini servis hesabına verir. Uygulanmış 001–062 SQL dosyalarını düzenlemeyin. Servis başlatıldığında gizli migration çalıştırılmaz.

İlk lider yalnız migration bağlantısından `termterm_team_v2.bootstrap_team` ile oluşturulur; HTTP bootstrap veya açık kayıt endpoint'i yoktur. Takım adı, kullanıcı adı ve görünen adı psql değişkenleriyle geçirin. Sonuçtaki takım kimliğini profile ekleyin; geçici parolayı ilgili lidere şirketin güvenli kanalıyla verin. Çıktıyı uygulama loglarına veya kaynak dosyalarına eklemeyin.

Servis binary'sinin Linux bağımlılıklarını `ldd` ile kontrol edin. systemd örneğinde `/opt/termterm-team/termterm-team-service` ve `/etc/termterm-team/service.json` kullanılır. `termterm-team` ayrı sistem hesabı olmalıdır. Sonrasında servis başlatma/durdurma ve HTTPS `/health` kontrolünü kurulum ortamında doğrulayın.

## Kişisel kimlik, ortak kimlik ve chain

Günlük kullanım kişisel kasa düzenini takip eder: klasöre tek tıklayın; hostta tek tıklama bilgileri, çift tıklama bağlantıyı açar. İşlemler sağ tık menüsündedir. SFTP'de son başarılı bağlantılar, klasör haritası ve ad araması aynı dosya yöneticisinde bulunur. Team bağlantısı izin iptali veya oturum kapanmasıyla sonlandığında eski dosya listesi temizlenir; yeniden bağlanma mesajı gösterilir.

Kimliklerim'de arama, özel/ortak filtresi ve kart/liste seçimi vardır. Tek tıklama yalnız tür/sürüm/görünürlük/izin bilgilerini gösterir; sır alınmaz. **Sağ tık veya Shift+F10** ile varsayılan yapma, paylaşma ve yetkili ortak kimlik işlemleri açılır. Şirket dosyasıyla kurtarma **Team yönetimindedir**.

Üst terminal sekmelerinde sağ tık odaklanma, bölünmüş görünüm, ad kopyalama ve kapatma sunar. Terminal araç çubuğundaki **Toplu giriş** veya **Ctrl+Shift+B** yalnız en az iki bağlı terminalde açılır; açıkken görünen uyarı kaç terminale yazıldığını belirtir. Bağlı terminal sayısı ikinin altına düşünce kapanır. **Ctrl+Shift+L** Team oturumunu kapatır ve terminal giriş kuyruklarını temizler. macOS birincil kısayollar Command kullanır; macOS bu geliştirmede doğrulanmadı.

**Kimliklerim** içindeki özel anahtar/parola varsayılan olarak yalnız hesabınıza aittir. Ortak kopya oluştururken alıcıları ve kullanma/görme/export izinlerini ayrı seçin. Özel kayıt ile ortak kopya birbirinden bağımsızdır.

Host bilgisi ortak kalır. Her kullanıcı host profili veya kişisel varsayılan kimliğini seçer. Ortak kullanıcı adı zorunluysa değiştirilemez. Başarısız kimlikten başka kimliğe sessiz geçilmez.

Sahip hosta sağ tıklayıp **Host chain** üzerinden sıralı jump listesini hazırlar ve erişim önizlemesini onaylar. Her hop kendi kişisel profilini kullanır. Bağımlı hop hakkı, o hosta bağımsız bağlantı veya export hakkı vermez. Hop/alıcı anahtarı değiştiğinde sahibin önizlemeyi yenilemesi gerekebilir. Mosh/agent/ileri adaptör sınırları durum raporundadır.

## Çevrimdışı önbellek

İlk giriş çevrimiçi yapılır. Sonraki girişte gerçek ağ kesintisi varsa, parola daha önce kaydedilmiş şifreli kimliği açabilir. Yanlış parola, TLS hatası ve açık yetki reddi bu yola geçirilmez.

- `.ttlogin` yalnız parola ile korunan kimlik indeksidir. Oturum tokenları ve bağlantı sırları şifreli `.ttcache` içindedir.
- Bu dosyalar servis/takım/hesap/kimlik kapsamına bağlıdır; kişisel taşınabilir `.ttvault` veya `.ttbackup` yerine kullanılmaz.
- Mevcut kayıt düzenlemeleri kalıcı sırayla tutulur. Sunucu makbuzu gelmeden kaydedildi sayılmaz.
- Son doğrulanmış kayıt yetkisi 24 saatten eskiyse düzenleme kapanır. Daha önce doğrulanmış bağlantıların önbelleği bu yazma süresinden bağımsızdır.
- Ağ dönünce önce oturum ve yetkiler doğrulanır. Çakışma varsa sunucudaki sürümle yerel aday ayrı karşılaştırılır.
- Çevrimdışı yeni kayıt, önceden doğrulanmış oluşturma yetkisi bulunan kapsamda açılır. Hesabı kapatılan kişinin önceden yetkilendirilmiş taslağı aşağıdaki sınırlı teslim yolunu kullanır.

## Kurtarma ve yedekleme

Lider şirket kurtarma dosyasını uygulamada oluşturur; güçlü parolayla şifrelenmiş dosyayı servis dışında korur. Hesap parolası sıfırlama eski sırları silmez veya kendiliğinden kurtarmaz. Mevcut özel kimlik kurtarması yeniden doğrulama, gerekçe, yeniden sarma ve alıcı uygulamada doğrulama gerektirir.

Şirket anahtarı değiştirme ve ortak kimlik kurtarma son native kabul testinde doğrulandı. Devam eden yeniden sarma bitmeden veya eski yedeklerin anahtar ihtiyacı değerlendirilmeden eski kurtarma dosyasını kaldırmayın.

Sunucu yedeğinde PostgreSQL veritabanını, migration sürümünü, erişimi sınırlandırılmış servis yapılandırmasını ve HTTPS/CA malzemesini ayrı koruyun. Liderin kurtarma dosyası da ayrıca yedeklenmelidir. Şifreli içerik taşısa bile DB yedeği hassastır.

`pg_dump --format=custom` ile alınan yedeği mevcut DB üzerine yazmak yerine ayrı test veritabanına geri yükleyin; rol sahipliği, sınırlı servis izinleri, TLS, hesap girişi ve şifre çözmeyi kontrol edin. Başarılı komut çıkışı tek başına geri yükleme kabulü değildir.

## Geçiş, geri dönüş ve paketler

Kullanılmamış v0.4.0 Team aktarımı kullanıcı kararıyla kapsam dışıdır. Eski şemayı/dosyayı yerinde değiştirmeyin; kullanıcı şifreli içeriklerini SQL kopyasıyla yeni hesaplara mal etmeyin. Yeni sistemde yapılan değişiklikler eski sisteme otomatik geri taşınmaz.

Windows deneme paketi ayrı `local.termterm.desktop.teamdev` uygulama kimliğini kullanır. Portable ZIP'in tamamını çıkarın; yardımcı dosyaları executable yanında tutun. WebView2 yoksa paketteki resmi çevrimdışı kurucuyu kullanın. Paketlerde `.lab`, test hesapları, kullanıcı anahtarları veya native test sürücüsü bulunmamalıdır.

Windows kurucu, Linux `.deb`/AppImage ve servis paketi ancak son kaynak durumu doğrulandığında teslim kanıtı sayılır. SHA256 değerlerini teslim klasöründeki `SHA256SUMS.txt` ile karşılaştırın. Windows ve AppImage güncellemeleri kullanıcı tarafından başlatılır ve paket imzası doğrulanır.


## Şirket anahtarını değiştirme ve kullanıcı adı kurtarma (dev.3)

Team yönetimindeki **Kurtarma anahtarını değiştir** eski dosya, eski dosya parolası, yeni dosya yolu/parolası, Team parolası ve gerekçe ister. Yeni dosya mevcut bir dosyanın üzerine yazılmaz. Dosya kaydedilip yeniden açıldıktan sonra parçalar doğrulanır. Bütün özel/ortak kimlik sürümleri ve kurtarma zarflı kişisel kullanıcı adı sürümleri doğrulanmadan eski anahtar emekliye ayrılmaz. İşlem sırasında eklenen kayıtlar son kontrolde tekrar hesaba katılır.

Yarım kalan iş için aynı eski ve yeni dosyaları koruyun. Yeniden açarken **Yeni dosya oluştur** seçimini kaldırın ve mevcut yeni dosyayı seçin. **Kapat; daha sonra devam et** sunucudaki işi silmez. Son yanıt kaybolursa aynı dosyalarla yeniden açma tamamlanmış işlemin makbuzunu bulur. Eski dosyayı, kuruluşunuzun saklama politikasına göre koruyun; yeni dosya yedeklenmeden eski dosyayı silmeyin.

**Team → Ortak kimliği kurtar → kimlik seçimi** liderin hesap parolasıyla yeniden doğrulama ve gerekçe ister. Önizleme yalnız güncel izinli alıcıları gösterir. Uygulama yeni revizyon oluşturur; önizleme sonrasında izin veya kimlik değişmişse yeni önizleme gerekir. Özel kaynak kopyası değişmez.

Hesap sıfırlamasından sonra yeni biçimde kaydedilmiş kişisel kullanıcı adları da liderin mevcut hesap kurtarma işlemine dahildir. Kullanıcının kendi masaüstünden **Kurtarmayı doğrula** adımını tamamlaması gerekir. Eski kurtarma zarfı bulunmayan alanlar şirket dosyasıyla açılamaz; bunlar değiştirilmeden korunur, kullanıcı açık onayla yeniden giriş yapabilir.

**Kişisel profil → Önceki kullanıcı adı sürümleri** yalnız o hesaba ait arşivi açar. Açılabilen sürüm önce önizlenir, ardından yeni revizyon olarak kaydedilir. Açılamayan sürüm uygulanamaz. Ortak kullanıcı adı zorunluluğu, kimlik erişimi ve güncel revizyon sunucuda yeniden kontrol edilir. Bu işlem SSH sunucusundaki kullanıcıları veya dosyaları değiştirmez.

## Çevrimdışı taslak teslimi

İlk giriş çevrimiçi yapılır. Şifreli önbellek servis, takım ve hesap için ayrıdır. Son yetki doğrulamasından sonra 24 saat yazma kullanılabilir; saat geri alınarak uzatılamaz. Süre dolunca önceden indirilmiş izinli bağlantılar kullanılabilir fakat ortak kayıt yazma durur.

Hesabı kapatılmış kullanıcı normal Team oturumu açamaz. Girişteki taslak teslimi seçeneği yalnız o cihazda önceden verilmiş, 90 gün süreli teslim yetkisini kullanır. Bu yol veri okumaz, SSH açmaz, ortak kayıt uygulamaz; aday değişikliği karar kuyruğuna yollar ve teslim sayısını gösterir. Yetki yoksa veya süresi dolmuşsa otomatik olarak hesap erişimi verilmez.

## Team Mosh ve SSH agent

Mosh mevcut Team hop planında SSH kontrol kanalıyla başlar. Hedefin UDP portuna bilgisayardan ulaşılmalıdır; SSH zinciri UDP trafiğini kendiliğinden taşımaz. Mosh istemcisi/uzak mosh-server bulunmaması ayrı hata verir. Team çıkışı ve terminal kapatma alt süreci sonlandırır.

Kişisel agent kimliği belirli sağlayıcı ve parmak izine bağlanır. Normal anahtar `SHA256:`, sertifikalı kimlik `CERT-SHA256:` seçicisiyle ayrı gösterilir. Sertifika yenilenince yeni kimliği seçin. Başka anahtara sessiz geçiş yapılmaz. Windows OpenSSH/Pageant ve Unix SSH_AUTH_SOCK seçenekleri cihazın sunduğu anahtarlarla kullanılır; agent hizmeti yoksa uygulama bunu bildirir. Donanım anahtarlarının gerçek cihaz doğrulaması ayrı gerekliliktir.

SSH parolası veya anahtar ve anahtar parolasını **Kimliklerim** altında kaydedip **Bağlantı profilim** üzerinden seçin; her bağlantıda yeniden yazmanız gerekmez. Sunucu bu kimliği kabul ettikten sonra ek doğrulama isterse ilgili hop için parola/MFA sorusu açılır. Tek kullanımlık kod kaydedilmez; yanlış kod veya iptal bağlantıyı durdurur. **Bağlanırken sor** seçimi ise her bağlantıda bilerek parola sorar. Team hesabını hatırlama ile SSH kimliğini kaydetme iki ayrı ayardır.


## SFTP'de makine bulma (dev.3)

SFTP'nin sol veya sağ bağlantı düğmesine basın. Açılan alanda **Sunucu adına göre ara** kutusuyla isim arayın veya mini klasör gezgininden alt klasöre girin. Bir makineye basmak SFTP bağlantısını başlatır. Yerel bilgisayar aynı alandan seçilebilir. Klasör içindeki makineler o klasöre girildiğinde gösterilir; isim araması izinli bütün klasörlerde çalışır.

Son başarılı SFTP bağlantıları **Son bağlantılar** bölümünün başına gelir. Bu liste çalışma oturumunun belleğindedir; uygulama yeniden açıldığında saklanmaz. Başarısız bağlantılar listeye girmez. Aktif aktarım varsa makine değiştirmeden önce aktarımı tamamlayın veya iptal edin. Team'de yalnız bağlantı yetkiniz olan makineler gösterilir.

## Chain ve taşıma yetkileri (056–058)

Kapsam yöneticisi chain'i yalnız sahibin verdiği yetki sınırında değiştirebilir. Hedefin ve eski/yeni hopların bağlantı izinleri sunucuda yeniden denetlenir. Hop adresi veya şifreli kaydı değişince, mevcut izinli hedef alıcılarının bağımlılık anahtarları güncellenir. Bir hedefe yeni izin verilirken gerekli hop anahtarları aynı önizleme/uygulama işleminde aktarılır; bağımlı hosta bağımsız terminal erişimi verilmez.

Host/klasörde sağ tık **Taşı** hedefi seçtirir. Erişim farkını inceleyin, anahtar hazırlığını tamamlayın, ardından **Onayla ve taşı** kullanın. Arada yetki/kimlik/chain değişirse yeni önizleme gerekir. Yanıt alınmadan bağlantı kesilirse uygulamanın sonucu bilinmeyebilir; kapattıktan sonra güncel klasör konumunu kontrol edin. İptal, zaten uygulanmış taşımayı geri almaz.


## Team komut paketleri ve çalışma alanları

Ortak kasayı seçtikten sonra **Komutlar / Workspaces** alanını açın. Klasörler aynı ortak kasa ağacını kullanır. Tek tıklama içeriği önizler; sağ tık düzenleme, silme, geçmiş ve izin/taşıma işlemlerini açar.

- **Komut paketi:** ad ve komutu kaydedin. Çalıştırmak için izinli bir SSH/Mosh hostunu seçip önizlenen bağlantıyı açın. Yeni terminal kullanılır; başka açık terminale sessizce komut gönderilmez.
- **Çalışma alanı:** 1–16 farklı host, hedef başına isteğe bağlı başlangıç komutu ve Stats tercihi, sekme veya bölünmüş görünüm. Hedeflerin bağlantı izni her açılışta Rust katmanında ayrıca doğrulanır. Workspace okuma izni hostlara bağlantı hakkı vermez.
- Başlangıç komutu SSH kimlik doğrulaması ve shell açılışından sonra gönderilir. Önizleme sonrası belge değişirse açılış reddedilir; yeni önizleme gerekir. Mevcut SSH/shell yetkileri aynen geçerlidir.
- Kayıtlar PostgreSQL'de ayrı snippet/workspace/üyelik tabloları ve şifreli içerikle tutulur. Kayıt geçmişi, izinler, çevrimdışı günlük ve taşıma altyapısı ortaktır. Aynı içeriğin yeniden okunması önizlemeyi gereksiz yere geçersiz kılmaz.
- Her komut en fazla 16 KiB UTF-8, belge toplamı 32 KiB'dir. NUL ve yinelenen workspace hedefleri reddedilir.

Yeni istemci Team cache sürümünü 8'e yükseltir; eski istemci bu dosyayı değiştirmeden reddeder. Kişisel `.ttvault`/`.ttbackup` biçimi değişmez. Bu paket için servis şeması **064** olmalıdır; migration'ı ayrı yetkili hesapla çalıştırın.


## Team güncelleme yönetimi

Team → **Sürüm politikası**, yalnız takım sahibine açıktır. Uyumlu istemci sürümünü seçin, **Etkiyi önizle** ile etkilenen kullanımını inceleyin ve **Onayla ve tüm takıma bildir** düğmesiyle kaydedin. Karar, takım bazında revizyon ve yapan hesapla saklanır. Başka bir yetkili arada değiştirirse yeni önizleme gerekir; yanıt kaybından sonra aynı karar ikinci audit oluşturmaz.

Seçilen sürüm altındaki üyeler ve takım sahibi girişte, yeniden bağlantıda, sonraki Team isteğinde veya açık uygulamanın 30 saniyelik denetiminde uyarılır. Team bağlantıları kapatılır; kişisel kasa kullanılabilir. Şifreli kimlikler ve bekleyen günlük silinmez. Tamamen çevrimdışı cihaz henüz öğrenmediği sunucu kararını bilemez; bağlantı döndüğünde karar uygulanır. Önceden alınmış zorunluluk çevrimdışı yeniden açılışla aşılmaz.

Liste servis paketinin `compatibility.json` dosyasındaki **clientVersions** alanından gelir; internetteki rastgele sürümler veya kullanıcı tarafından yazılan URL'ler kabul edilmez. Bu pakette yalnız **0.4.1** yer alır. Üretim paketini hazırlayan kişi, gerçekten bu API ile uyumlu ve dağıtımı hazır istemci sürümlerini listeye ekler. Yönetici kararından önce Windows/Linux paketlerini erişime açmalıdır. Ekran seçimi GitHub Release yayımlamaz, migration çalıştırmaz ve dosya indirme adresini değiştirmez.

Kararlı Windows/AppImage kurulumları mevcut imza doğrulamalı güncelleyiciyi kullanır; gerekli sürüm yayımlanmamışsa işlem açıklamayla durur. Geliştirme paketlerinde stable güncelleme kapalıdır; Linux deb/rpm kurulumlarında paket yöneticisiyle yükseltme gerekir. Kişisel uygulama güncellemeleri kullanıcı tarafından başlatılır.

Eski v0.4.0 Team aktarımı kullanılmadığından kapsamdan kaldırıldı; arşiv önizlemesinin kaynakları korunur fakat normal Team menüsünde gösterilmez. Yeni sürümlerde mevcut v2 veritabanını koruyan yükseltme için **TEAM_UPGRADES_TR.md** kullanılır.


## Makine, chain ve proxy (dev.5)

Hosts → Host ile **Makine ekle** açılır; mevcut makinede sağ tık → Düzenle. Adres, SSH portu, SSH/Mosh ve kullanıcı adı politikası ortak makine kaydıdır. Proxy bölümünde Doğrudan / HTTP CONNECT / SOCKS5 seçin. Kimlik doğrulaması isteniyorsa makinede sağ tık → **Kişisel bağlantı profilim → Proxy kimliğim** alanından kullanıcı adı ve parolayı kaydedin. Jump host için hedef makinede **Zincirdeki kişisel profillerim** yolunu kullanın. Kayıt yalnız sizin hesabınıza şifreli saklanır; yeniden girişte ve önceden doğrulanmış çevrimdışı bağlantıda kullanılır. Kaydetmediyseniz bağlantıda sorulur. **Proxy kimliğini unut** bağlantı eşlemesini kaldırır ve kimliği kullanım dışı bırakır; sunucudaki sürüm/kurtarma saklama politikası devam eder. Proxy türü/adresi/portu değişirse eski parola gönderilmez; kimliğinizi yeniden kaydedin. SSH kimliğinizi **Kimliklerim / Bağlantı profilim** üzerinden seçmeye devam edin.

Makinede sağ tık → **Host chain** sıralı jump hostları ve erişim önizlemesini açar. Her hop kendi kişisel bağlantı profilini kullanır. Proxy, tanımlandığı makineye giden adım içindir; önceki jump varsa proxy'ye oradan ulaşılabilmelidir. Mosh UDP trafiği SSH chain/proxy içinde taşınmaz.

Geçmiş → Geçici kullan, kaydedilmiş o sürümün chain/proxy düzenini seçer. Bağımlı hostların bilgileri/kişisel kimlikleri güncel kalır; güncel erişim tekrar doğrulanır. Güncele dön ile seçim kaldırılır. Seçim değiştiğinde eski bağlantı önbelleği temizlenir; yeni rotayı ilk kez çevrimiçi doğrulayın. Eski route bilgisi hiç saklanmamışsa bağlantı engellenir. **Yeni revizyon olarak geri yükle** seçilen sürümün host içeriğini ve chain/proxy düzenini birlikte önizler. Onayda güncel izinler, alıcılar ve revizyon tekrar doğrulanır; host ve chain aynı işlemde kaydedilir. Değişen chain için kapsam yönetim yetkisi gerekir. Önizleme eskirse yenileyin; mevcut kişisel SSH/proxy sırları eski sürüme döndürülmez.

## Team girişini hatırlama

Servis adresini, takım kimliğini, kullanıcı adını ve gerekiyorsa şirket CA sertifikasını bir kez girin. **Beni hatırla (bu cihazın güvenli deposu)** seçiliyken başarılı girişten sonra parola işletim sisteminin güvenli deposuna kaydedilir. Team ekranını tekrar açtığınızda alanlar doldurulur; **Kayıtlı hesabımla devam et** ile parola yazmadan giriş yapabilirsiniz. Bu işlem yeni giriş/yetki kontrolünü atlamaz.

Parola hatırlatma kapalı olsa da son başarılı bağlantı alanları saklanır. Güvenli depo kilitli veya erişilemiyorsa hata gösterilir; parolayla giriş yapılabilir. **Bu cihazda beni unut**, yalnız bu cihazdaki giriş tercihini ve saklanmış parolayı kaldırır; Team verilerini ve gönderilmemiş şifreli işlemleri silmez. Şirket parolayı sıfırlarsa yeni parolayla giriş yapıp yeniden kaydedin. Geçici aktivasyon parolası kaydedilmez.

