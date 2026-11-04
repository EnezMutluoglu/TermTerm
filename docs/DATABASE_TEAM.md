# Team SQL — geliştirme kapsamı

`0.5.0-dev.12`, yalnız geliştirme. Kişisel kasalar ve kararlı güncelleme kanalı değişmez.

## Kullanım

1. Ayrı test Team veritabanında migration hesabıyla `migrations/team/007_sql_documents.sql` uygulanır. Bu migration mevcut 1–6 şemasını gerektirir; kullanıcı/Team tablolarını silmez. Normal uygulama hesabı şema hazırlayamaz. Önce veritabanı yedeği alınır. Sıfır kurulumda 001–007 sırayla uygulanır; `grant_app.sql` ile yalnız mevcut beş giriş fonksiyonu uygulama hesabına açılır.
2. Team hesabıyla ortak kasa açılır. **Veritabanları → Bağlantı ekle** üzerinden motor, hedef, kullanıcı/parola ve klasör seçilir. Team veritabanı ile yönetilecek veritabanı ayrı bağlantılardır.
3. Özel CA gerekiyorsa **public PEM sertifikası** bağlantı içine eklenir. Başka cihazdaki dosya yolu kullanılmaz. Sertifika yalnız bağlantı ömründe geçici dosyaya açılır; gizli anahtar PEM'i kabul edilmez. Sistem güven zinciri de kullanılabilir.
4. **Team → Üyeler ve yetkiler** ekranında kaynak veya klasör kapsamı seçilerek aşağıdaki izinler ayrıca verilir. Eski Operatör/Editör şablonları kendiliğinden SQL yetkisi kazanmaz.

| İzin | Kapsam |
|---|---|
| Bilgiler/geçmiş (`read`) | Bağlantı bilgileri ve görünür kaynak; diğer SQL izinlerinin ön koşulu |
| SQL çalıştır (`sql_run`) | Serbest sorgu/script, transaction, tablo verisi, plan ve büyük değer okuma |
| SQL sabit kontrollerini izle (`sql_monitor`) | Gömülü R0 katalog kontrolleri, indeks/yedek kanıtları; renderer'dan gelen alternatif SQL kullanılmaz |
| SQL önizlemeli yönetim (`sql_manage`) | Ayrı bağlantıdaki mevcut bakım/provisioning/extension ve satır değişikliği akışları |
| SQL sonuçlarını dışa aktar (`sql_export`) | SQL/CSV/TXT/BIN dosya yazma; native komut da bu izni ister |
| Sırları görüntüle (`reveal`) | Parolanın arayüze açılması; SQL kullanım izni bunu vermez |
| Dışa aktar (`export`) | Kayıt/backup exportunun genel izni; SQL bağlantı kaydında ayrıca `sql_export`, sır içeriyorsa `reveal` gerekir |

Serbest SQL **hedef DB hesabının yetkileriyle** çalışır. `sql_manage` kapalıyken `sql_run` verilmesi, güçlü bir DB hesabının DDL/DML yapmasını sunucuda engellemez. Bu ayrım SQL metnini güvenilir biçimde sınıflandıran bir güvenlik duvarı değildir. Dar yetkili DB hesapları kullanılmalı; zorunlu ikinci onay gerektiren şirket yürütme servisi henüz bu sürümde yoktur.

## Yetki, sır ve oturum davranışı

- SSH `connect` izni SQL bağlantısını açmaz ve SQL sırrı için anahtar zarfı üretmez. Kayıt ilk kez oluşturulurken de kayıt türü değerlendirilir. Fazladan veya eksik alıcı zarfları sunucuda reddedilir.
- Her SQL işleminden önce sunucu token'ı ve güncel kaynak izni doğrulanır. Sonuç dönerken tekrar doğrulanır. İstemcinin yolladığı kullanıcı kimliği, bağlantı profili veya izin dizisi yetki kanıtı değildir.
- Oturum kasa, bağlantı revizyonu ve Team ACL revizyonuna bağlıdır. Bağlantı veya ACL değiştiğinde yeniden bağlanmak gerekir. Bu ilk aşamada ilgisiz bir ACL değişikliği de SQL oturumlarını kapatabilir.
- Açık oturumlar 5 saniyede bir doğrulanır. Yetki kontrolünün 8 saniyelik zaman bütçesi vardır; PostgreSQL kesintisinde çevrimdışı SQL'e düşülmez. Hedef JDBC bağlantısının kapanması ayrıca sürücü/ağ temizliği süresine bağlıdır; anlık iptal garantisi verilmez.
- Yetki sonradan doğrulanamazsa sonuç arayüze verilmez. Yazma yapılmış olabileceği için `unknown_outcome` gösterilir; otomatik tekrar yoktur. İptal/kapatma, aynı kasaya ait oturumda çevrimiçi izin kontrolünün başarısına bağlı değildir.
- Sunucuda `sql.authorized` audit kaydı izin sınıfı, kullanıcı/cihaz, kayıt kimliği ve zaman içerir. Bu, hedef DB'de işlemin başarılı olduğunun kanıtı değildir. SQL metni, parametreler, sonuçlar veya parolalar audit'e yazılmaz.
- Team SQL kaydı içeren yerel önbellek sürümü 3 olur. Şifreleme değişmez; eski sürüm 2 Team uygulaması bu dosyayı açmaz. Kişisel `.ttvault`/`.ttbackup` biçimi değişmez.

## Sorgu kitaplığı ve runbook — dev.12

- Açık SQL sekmesinde **Sorgu kitaplığı** seçilir. Bağlantı satırındaki kitaplık düğmesi, SQL bağlantısı açmadan belge okumayı da sağlar. Aynı bağlantının sorguları ve runbook'ları listelenir. Yeni belge için ad, açıklama, klasör ve SQL girilir. Runbook 1–50 sıralı adım içerir; her adımın başlığı, açıklaması ve SQL'i vardır. Adımlar yukarı taşınabilir/silinebilir.
- **Editöre yükle**, yalnız seçili adımın metnini getirir ve parametre değerlerini temizler. Otomatik yürütme, toplu runbook çalıştırma, adımlar arası gizli parametre aktarımı veya başarı takibi yoktur. Çalıştır/Commit/Rollback mevcut SQL sekmesinden açıkça kullanılır. Runbook, bir onay servisi değildir.
- Team'de belgeyi okuyabilmek hedef bağlantıda SQL çalıştırma hakkı vermez. Belgeyi açarken ve editöre yüklerken PostgreSQL güncel belge/bağlantı okuma yetkisini doğrular; hedefte sorgu çalıştırmak ayrıca `sql_run` gerektirir. Kişisel kasada aynı kitaplık sunucusuz çalışır.
- Belge oluşturma/düzenleme/silme genel `edit` iznine bağlıdır. Bağlantı aynı kasadaki gerçek bir `databaseConnection` olmalıdır. Yeni bağ, sunucuda tek `member` ilişkisi olarak doğrulanır; sorgu üzerinden bağlantı sırrı dağıtılmaz. Açık belgenin revizyonu değişmişse yeniden açıp karşılaştırma istenir; sunucudaki CAS ve karar kuyruğu ayrıca korunur.
- Belge ve bağlı bağlantı/klasör seçili yedeğe birlikte girer. Tüm bağımlılıklarda export, bağlantıda ayrıca güncel `sql_export` gerekir; sır içeren yedekte `reveal` de gerekir. Başkasının göremediği bağlantıya ait belge açılamaz veya eksik ilişkiyle yedeklenemez. Import/copy ilişkileri yeniden eşler ve SQL'i çalıştırmaz.
- Çalışma parametreleri/sonuçlar/parola alanları belge şemasına alınmaz. SQL metnine kullanıcının açıkça yazdığı sabit değerler metnin parçasıdır; belgeyi okuyabilen kişi bunları görür. SQL metni uygulama tarafından otomatik olarak sırdan arındırıldığı iddia edilmez.
- Team'de kaydetme önce şifreli yerel günlüğe gider; ekranda eşitleme beklediği belirtilir. Çevrimiçi doğrulama isteyen **Editöre yükle** için Team eşitlemesi tamamlanmalıdır. Kayıtlar normal Team geçmiş/CAS mekanizmasını kullanır; SQL sonuçları kaydedilmez.
- Sorgu/runbook içeren Team önbelleği **4** sürümüne atomik yükselir; 2/3 okunmaya devam eder. Eski istemci 4'ü değiştirmeden reddeder. Kişisel kasa/yedek biçimi değişmez.
- Laboratuvar migration betiği `schema_version` üzerinden yalnız uygulanmamış migration'ları çalıştırır. Eski tür kısıtlarının yeni kayıtlar üzerine tekrar uygulanması önlenir. Yalnız `termterm_team_e2e` bu tur yükseltilir; kullanıcının Team sunucusu kendiliğinden değiştirilmez.

## Açık kalanlar

- Team SQL üzerinden SSH/chain tüneli ve bağımlılık izni; şu an doğrudan bağlantı desteklenir.
- SQL çevrimdışı kullanım ve SQL metrik/şema önbellekleri. Bağlantı kayıtlarının mevcut şifreli Team günlüğü/geçmişi ayrı kalır; sorgu sonuçları kalıcı saklanmaz.
- SQL geçmiş sürümünü geçici bağlantı olarak kullanma. Kayıt geçmişini inceleme/kalıcı geri yükleme mevcut Team mekanizmasını kullanır; native kabul matrisi ayrıca genişletilecek.
- Zorunlu ikinci onay, masaüstüne dağıtılmayan yönetim sırları ve şirket yürütme servisi.
- İzin kaldırılmadan önce kullanıcı cihazına verilmiş bir sırrın dış kopyasını geri alma. Masaüstü izinleri hedef DB hesap izinlerinin yerine geçmez.
- Gerçek üretim/PaaS, diğer DB sürümleri, macOS ve Linux native arayüz doğrulaması.
