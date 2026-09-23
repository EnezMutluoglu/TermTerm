# TermTerm Team 0.4.0-dev.1

Bu, ayrı uygulama kimliğiyle çalışan yerel geliştirme sürümüdür. Kararlı uygulamanın kasasını otomatik açmaz; güncelleme yayımlamaz. Team hesabı için PostgreSQL gerekir. Kişisel kasalar sunucusuz çalışmaya devam eder.

## İlk kullanım

1. Girişte **Team hesabı** seçin. Şirket PostgreSQL adresini, portunu, veritabanını, sınırlı uygulama kullanıcısını ve TLS CA dosyasını girin.
2. **Kayıt ol** ile kullanıcı adı, e-posta ve hesap parolası belirleyin. En az 12 karakter, en fazla 72 UTF-8 baytı kabul edilir. Bu denemede e-posta doğrulaması veya e-postayla parola sıfırlama yoktur.
3. **Takım oluştur** işlemi sahibini belirler. Sahip ortak kasa oluşturabilir, kayıtlı kullanıcı arayabilir ve üye ekleyebilir. Kayıt olmak veya takıma eklenmek kendiliğinden host erişimi vermez.
4. **Üyeler ve yetkiler** altında üyeyi ve kapsamı seçin. Kasanın tamamı, bir klasör ve altı veya tek kayıt için izin/engel tanımlayın. Hazır roller izin şablonlarıdır; sahip dışındaki üyelerde kaynak izinlerini ayrıca uygulayın.
5. Hosts üzerindeki sağ tık menüsünden **Team · Geçmiş ve erişim** açılır. Team klasör taşıması aynı ekrandaki çevrimiçi erişim önizlemesinden onaylanır.

Kaldırılmış üyeyi yeniden eklemek eski erişimini geri vermez; izinleri yeniden atayın. Kapsam yöneticisi kendi yetki sınırını aşamaz, kendini yükseltemez ve başka yönetici atayamaz.

## İzinler

| İzin | Etkisi |
|---|---|
| Bilgiler ve geçmiş | İzinli kaydı ve geçmişini okur. |
| Bağlantı | Hostta terminal, SFTP ve tünel bağlantısı açar; izinli ortak terminale katılır. |
| Oluştur/düzenle/sil | Kaynağı değiştirir; çakışma kararı ve kalıcı geri yükleme yapabilir. |
| Sırları görüntüle | Parola/özel anahtar içeriğini arayüzde okuyabilir. |
| Dışa aktar | İzinli kaydı dışarı aktarır. Sır içeren yedek ayrıca sırları görüntüleme izni gerektirir. Seçili hostun klasör/kimlik/jump bağımlılıklarında da bu izinler aranır. |
| Erişim ver | Sahibin tanımladığı yetki devri sınırı içinde izin atar. |

Açık engel devralınan izinden üstündür. Tek host erişiminde üst klasörler yalnız gezinme yolu olur. Jump host ve kimlik bağımlılıkları bağlantı için native serviste kullanılır; bağımsız bağlantı veya sır gösterme hakkı vermez. Yeni paylaşılan jump/kimlik referansı eklemek ilgili kaynakta erişim yönetimi gerektirir.

Yetki hem PostgreSQL RPC'lerinde hem Rust servislerinde denetlenir. Import, yedek, SFTP ve localhost API aynı kontrolleri kullanır. API'den host listesi almak dışa aktarma izni gerektirir. Ortak terminal yalnız ilgili hosta bağlantı izni olan üyeler arasında açılır; tek kişi yazar. Kontrol devri eski girdileri geçersiz kılar. DB bağlantısı veya kaynak yetkisi değişince eski ortak terminal girdileri tekrar gönderilmez.

## Çevrimdışı çalışma ve karar kuyruğu

Team verileri ayrı format 2 `.ttteam` dosyasında şifreli önbelleğe alınır. Son başarılı izin doğrulamasından itibaren 24 saat yerel düzenleme yapılabilir. Süre dolunca düzenleme kapanır; indirilmiş izinli bağlantılar kullanılabilir. Yeni üyelik, izin değişikliği, kapsam taşıma ve kalıcı geri yükleme çevrimiçi yapılır.

Her yerel değişiklik kayıtla aynı SQLite transaction'ında sıralı günlüğe yazılır. PostgreSQL dönünce güncel yetkiler alınır, benzersiz işlem kimliği ve beklenen revizyonla gönderilir. İstemci saati kazanan sürümü belirlemez. İzin kaldırılmışsa bekleyen aday, ortak kaydı değiştirmeden yetkili kararına teslim edilir. Gönderilmiş adayın yerel gizli içeriği temizlenir; işlem sırası ve sonucu korunur.

**Bekleyen kararlar** sunucu ve aday alanlarını karşılaştırır. Yetkili kullanıcı sunucudakini, gönderileni veya seçtiği alanları koruyabilir. Anahtarı eksik/bozuk aday sağlam kaydın üzerine yazılmaz. Ekrandaki bekleyen sayı ve en eski zaman, henüz gönderilmemiş işleri gösterir; sunucuya ulaşılamamış iş kaydedilmiş sayılmaz.

Uygulama sunucunun açık yetki reddini ağ kesintisi saymaz. İptal öğrenilince ilgili kasa önbelleği ve bağlantı malzemeleri temizlenir; güvenli bir geçiş için o kasanın açık bağlantıları kapatılır. Çevrimdışı cihaz merkezdeki yeni iptali öğrenemez. Önceden cihaza ulaşmış SSH sırrının başka bir kopyasını uzaktan geri almak mümkün değildir; uzak SSH/dosya izinleri geçerliliğini korur.

## Geçmiş

- Güncel sürüm ve önceki 10 sürüm tutulur. Geçici kullanım veya geri dönüş için sabitlenen sürümler bu sınırın dışında korunabilir.
- **Geçici kullan**, hostun yalnız sizin yeni bağlantınızda kullanılacak içeriğini değiştirir. Hostta/geçmiş ekranında uyarı görünür; **Güncele dön** kaldırır. Var olan SSH oturumunu yeniden bağlamaz.
- **Kalıcı geri yükle**, seçilen içeriği yeni revizyon olarak yazar. Önceki güncel sürüm dönüş noktası olarak işaretlenir. Onu yeniden geri yükleyerek yeni içeriğe dönülebilir.
- Silinmiş kayıtlar geçmiş seçicisinde görünür. Eksik/izinsiz bağımlılıklar gösterilir. Eski sürüm farklı klasördeyse önce çevrimiçi taşıma önizlemesi gerekir.
- Hostu geri almak kimliği/jump hostu veya uzak makinenin dosyalarını geri almaz.

İşlem günlüğü kayıt/izin değişikliği, bağlantı açma/kapatma, SFTP aktarım özeti ve geri yüklemeyi tutar. Terminal çıktıları, tuşlar ve dosya içerikleri kalıcı audit'e yazılmaz. **Bağlantı ve politika → Alan kullanımını ölç / süresi dolmuş günlüğü temizle**, 90 günü geçen audit'i sınırlı partilerle temizler. Çözümlenmemiş kararlar korunur. Ortak terminal şifreli kareleri kısa ömürlüdür.

## Veri tabanı kurulumu

Şema `termterm_team` olarak ayrıdır. Kontrol verileri ilişkisel kolonlarda; kayıt/sürüm içerikleri şifreli `bytea` alanlarındadır. Tek bir NoSQL/JSON kasa belgesi kullanılmaz. JSON yalnız işlem gövdesi, sınırlı ek metadata ve şifreli aday taşıyıcısında kullanılır.

Migration hesabıyla sırayla `migrations/team/001_team.sql`–`005_terminal.sql`, sonra `grant_app.sql` uygulanır. 001 yalnız ilk kurulum içindir; 002–005 geliştirme sırasında tekrar uygulanabilir. Şema sürümleri `schema_version` tablosundadır. Uygulama hesabı şema/tablo oluşturmaz ve doğrudan tablo okuyamaz; yalnız hesap kayıt/giriş/yenileme, doğrulanmış RPC ve sınırlı teslim fonksiyonlarına erişir. Migration hesabı uygulama profili olarak kabul edilmez.

Bu bilgisayardaki laboratuvar: Ubuntu-24.04, PostgreSQL 18 `termterm`, **localhost:55432**. `main` kümesinin 5432 portu kullanılmaz. Ayrı DB'ler `termterm_team_dev` ve `termterm_team_e2e`:

```powershell
wsl.exe -d Ubuntu-24.04 -u root -- bash /mnt/c/Users/sonx/Desktop/termterm/scripts/team-lab.sh termterm_team_dev
```

Profil/CA ve disposable test hesabı bilgileri yalnız Git dışında `.lab` altında tutulur. Bunları açık depoya veya genel Syncthing paylaşımına eklemeyin. `.ttteam` dosyası hesap önbelleğidir; kişisel kasa/yedek gibi açılmaz. Yetkili `.ttbackup` exportu hesap token'larını, üyelikleri ve Team yetki kurallarını içermez. Başka Team'e import önizleme üzerinden yeni kaynaklar oluşturur; kaynak dosya değişmez.

## Derleme ve kontroller

```powershell
pnpm install --frozen-lockfile
pnpm team:dev
pnpm team:build
```

Geliştirme kimliği `local.termterm.desktop.teamdev`; otomasyon kimliği `local.termterm.desktop.teamdev.e2e`'dir. Kullanıcıya verilen deneme executable'ı `e2e` özelliği olmadan derlenmelidir. Test sürücüsü olan executable dağıtılmaz.

SQL testi `tests/team_postgres.py`, native senaryolar `tests/desktop/team.mjs`, Rust kontrolleri `src-tauri/src/team/*` içindedir. Gerçek ölçümler ve ortam sınırları doğrulama raporunda tutulur. macOS veya GitHub derlemesi bu çalışma için başlatılmaz.
