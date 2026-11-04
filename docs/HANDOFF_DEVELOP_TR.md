# develop — 4 Kasım 2026 devir teslimi

Giriş: [HANDOFF_TR.md](../HANDOFF_TR.md). Devir öncesi HEAD `fcf3775` (`Require PostgreSQL setup kit in Team delivery packages [skip ci]`), uzak develop tabanı `b749b35c66670a47941b572db44cd66097e88ef3`. Temel paket/Team sürümü 0.4.2. Bu dal SQL veya son VPN çalışmasını içermez.

`fcf3775`, Team servis teslimatında PostgreSQL SQL dosyaları, örnek ayarlar ve adım adım kurulum kılavuzlarından biri eksik/boşsa paketleme/yayın hazırlığını durduran kapıdır. `scripts/verify_team_setup.py` hem staged dizin hem `.tar.gz` doğrular. `tests/team_setup_package.py` gerekli dosyaların var, eksik ve boş durumlarını iki biçimde denetler. Çağıranlar `scripts/package-team-service.sh`, `scripts/verify-team-delivery.py` ve mevcut onaylı yayın hazırlığıdır; geliştirme CI kontrolü de kapıyı çağırır. Bu devir CI/yayın politikasını değiştirmez.

Bu teslim sırasında mevcut commit kaynak olarak incelendi; test, paketleme, production DB veya kurucu çalıştırılmadı. Önceki commit'e ait test sonucunu yeni test sonucu diye sunmayın.

Yeni bilgisayarda Python 3 ile, servis/DB kurmadan dar kontrol:

```powershell
python tests/team_setup_package.py
```

Gerçek servis arşivi daha sonra hazırlandığında `python scripts/verify_team_setup.py <arsiv.tar.gz>` ile tekrar doğrulayın. `team-service/README.md` paket içindeki kurulum sırasını anlatır. Var olan DB yükseltmesinde `docs/TEAM_UPGRADES_TR.md` içindeki yedekli plan/apply/confirm gerekir; ilk kurulum SQL'ini yeniden çalıştırmayın.

Uzak kaynak checkpoint'ini çekmek yayın veya release onayı değildir. `main`, tag, Release, `latest.json`, dağıtım ve kurulu kararlı uygulama korunur. Bu devirden sonraki uygulama işi ilgili feature dalında sürdürülür.
