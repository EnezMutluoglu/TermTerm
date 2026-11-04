# SQL dalı — 4 Kasım 2026 devir teslimi

Giriş: [HANDOFF_TR.md](../HANDOFF_TR.md). Dal `codex/feature-database-workspace`; devir öncesi HEAD ve dal tabanı `59652374014cd8f65bb34673b9d9d900bb836bb8`. Dal güncel develop/VPN ile birleştirilmedi. Sürüm **0.5.0-dev.12**, ayrı uygulama kimliği `local.termterm.desktop.databasedev`; kararlı sürüm değildir.

## Kaldığımız yer ve önceki kanıt

Son iş SQL kitaplığıdır: kişisel/Team adlandırılmış sorgular, klasörler ve kullanıcı tarafından tek tek editöre yüklenen runbook adımları. Otomatik runbook yürütme ve ikinci onay servisi yok. Team belge yetkileri ile canlı SQL bağlantısı iptali ayrımı düzeltilip regresyonu eklenmişti.

5 Ekim dev.12 raporu **65 Rust PASS / 8 skip**, ayrıca 2 gerçek PostgreSQL TLS Team testi, 52 TypeScript test, 10 Windows native Team ve 2 native kişisel kitaplık senaryosu; WSL Team sunucu regresyonu ve frontend derlemesi kaydeder. Bunlar **tarihsel, rapordan devralınan sonuçlardır**; 4 Kasım devir çalışmasında yeniden test veya derleme yapılmadı. SQL chunk >500 KiB uyarısı sürüyor. Önceki katalog sonucu 216 işlemin 135 R0 kontrolüdür; 71 yönetim işlemi, 7 ortam/lisans koşullu kontrol ve farklı sürümlerin kapsamı tamamlanmış sayılmaz.

- [DATABASE_WORKSPACE_STATUS.md](DATABASE_WORKSPACE_STATUS.md): kullanılabilir özellikler, sınırlar ve paket komutları.
- [DATABASE_TEST_RESULTS.md](DATABASE_TEST_RESULTS.md): tarihli test kanıtları ve tekrar komutları.
- [PROJECT_STATUS_2026-10-05.md](PROJECT_STATUS_2026-10-05.md): bütün proje eksik envanteri.
- [DATABASE_TEAM.md](DATABASE_TEAM.md), [DATABASE_PROVISIONING.md](DATABASE_PROVISIONING.md), [DATABASE_CATALOG_VALIDATION.md](DATABASE_CATALOG_VALIDATION.md): izinler, provisioning ve işlem bazında açık işler.

## Yeni makinede hazırlama

Git, Node/pnpm, Rust/Cargo, Python 3; Windows'ta C++ Build Tools/SDK ve WebView2 gerekir. `db:prepare` Java 21 JDK ve PostgreSQL/SQL Server/Oracle JDBC bağımlılıklarını indirir/doğrular ve Java yardımcı kaynağını derler. Ağ erişimi gerekir. İndirilen Java/JAR/Mosh/DLL dosyaları Git tesliminde yoktur.

```powershell
pnpm install --frozen-lockfile
pnpm db:prepare
node scripts/fetch-mosh.mjs
pnpm test:unit
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm db:dev
```

Lab ve e2e testlerini açmadan önce yeni test kasası oluşturun. WSL'de disposable PostgreSQL 18 test cluster'ı `termterm`/55432, Docker ve yeterli RAM hazırlığı gerekir; scriptler bunların hepsini sıfırdan kurmaz. `scripts/db-lab.sh up` SQL Server/Oracle disposable container/volume oluşturabilir ve mevcut adları başlatabilir; yalnız bu işe ayrılmış ortamda kullanın. PostgreSQL hazırlığı `scripts/db-lab-postgres.sh`, Windows loopback relay `scripts/db-lab-proxy.py`; TLS için `scripts/db-lab-tls.sh` ve Oracle relay bulunur. SQL Server Developer/Oracle Free lisans koşullarını yeni ortamda kabul edin.

`.lab/database/credentials.env`, `profiles.json`, test CA/özel anahtar ve `.lab/team-connection.json` taşınmadı. Yeni rastgele parolalar/profiller üretin; Windows relay portları PostgreSQL 41432, SQL Server 41433, Oracle 41521 olarak test kodunda beklenir. `scripts/database-rpc.mjs` ve mevcut testlerin beklediği profil alanlarını okuyun; gerçek/üretim hedefini bu dosyaya koymayın. Team testleri ayrı TLS Team DB ve `scripts/team-lab.sh termterm_team_e2e` hazırlığı ister; migration 006/007 ve izinler bu eski dalın sözleşmesidir, VPN dalının 065/066 zinciriyle karıştırmayın.

Lab hazır olduktan sonra ilgili testleri [rapordaki](DATABASE_TEST_RESULTS.md) sırayla seçin. Örneğin `node scripts/test-database-helper.mjs` ve `node scripts/test-database-row-edit-disconnect.mjs` gerçek DB'ye bağlanır; bağlantı/commit cevabı kaybı sonrası `unknown_outcome` ve otomatik tekrar olmaması korunmalıdır. Native kitaplık kabulü `tests/desktop/database-library.mjs`, Team SQL kabulü `tests/desktop/database-team.mjs`; e2e build ayrı `tauri.database-e2e.conf.json` ile yapılır. E2E özelliği dağıtılacak yerel pakette kalmamalıdır.

Test özelliği olmadan ayrı yerel paket hazırlama (yeni kabulden sonra):

```powershell
pnpm db:build
node scripts/package-database-dev.mjs
node scripts/verify-database-dev-package.mjs
```

Çıktı `artifacts/database-workspace/TermTerm-0.5.0-dev.12-windows` altındadır. Paket script'i mevcut çıktıyı kullanabilir; taze build ve manifest kontrolü olmadan eski paketi kabul etmeyin. Kararlı kurucuyu çalıştırmayın, updater feed/yayın yapmayın.

## Önerilen sıradaki dar işler

1. Team SQL SSH/chain bağımlılık izinleri ve iptal davranışı; mevcut doğrudan/çevrimiçi SQL sınırını koruyun. Çevrimdışı SQL, Team metrik/şema geçmişi, tarihsel bağlantı ve ikinci onay servisi açıktır.
2. Provisioning kesinti/yarım durum kurtarma; gerçek sunucu durumunu okuyarak planlayın, otomatik DROP ile temizlemeyin.
3. Unique-only/özel tip roundtrip ve tek satır düzenleme hata vakaları; toplu işlem eklenmiş sayılmaz. Kalan yönetim işlerini motor başına preflight/apply/sonuç kanıtıyla açın.
4. Gerçek Azure/AWS/Huawei, Oracle 19c/RAC/lisanslı AWR/ASH, Linux native SQL UI, macOS ve diğer sürümler ancak ilgili ortama erişilince doğrulanır. Şu anda test edilmiş değiller.

Eski `.tools` logları, `.lab` DB hacimleri, test kasaları, ekran görüntüleri ve `artifacts` paketleri eski bilgisayarda korunur; Git ile taşınmaz. Yeni makinede geçmiş test sonuçları yeniden üretilene kadar rapordaki kapsamla sınırlı kabul edin.
