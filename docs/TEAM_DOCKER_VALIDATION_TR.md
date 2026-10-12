# Team 0.4.1 — Debian 13 Docker teslim doğrulaması

Tarih: 12 Ekim 2026. Dağıtım kaynağı: `8ef04fc327efe057e5b48c873b8451b93a4b6879`.
Servis executable'ı yayımlanmış 0.4.1 ile aynıdır; yeni API/şema/masaüstü sürümü yoktur.
Şema 64 / API 1. Uygulanmış migration dosyaları değiştirilmedi.

## Image ve paket

- Linux amd64, Debian 13 Trixie slim, UID/GID 10001, OpenSSL 3.
- Image: `termterm/team-service:0.4.1`.
- Image ID: `sha256:75e2efca2917a97ddd4605522b9e34dcbb270926435855837f7a692ae9db8941`.
- Sıkıştırılmış image: 37.749.474 bayt; Docker image boyutu 148.570.576 bayt.
- Servis SHA256: `eeb7bbbd52389b9b0a4c2202cee64c80575041835e9bd3fce28ed0a68c554e76`.
- Image arşivi SHA256: `f4d64700e7a4a51339488a822a91ab3e66de927928b6bf39b0714e440e4805ff`.
- Teslim klasörü: `artifacts/team-docker-0.4.1-ready`.
- Image içinde yalnız çalışma zamanı ve servis vardır; Rust/Node, özel config, lab parolası veya kullanıcı kasası yoktur. Docker build context allowlist ile sınırlandı.

## Kısa kontroller

| Kontrol | Sonuç ve kanıt |
|---|---|
| DBA SQL hazırlığı | `tests/team_v2_database_setup.py` geçti: ağsız geçici PostgreSQL, dört gizli parola istemi, SCRAM, DB sahipliği, pgcrypto, sınırlı roller; tekrar çalıştırma mevcut hesapları değiştirmeden reddedildi. |
| İlk kurulum / systemd | `tests/team_v2_install.py --package` geçti: 64 migration, tekrar/checksum/yeni şema reddi, özel lider dosyası, gerçek TLS PostgreSQL, Caddy HTTPS, CA reddi ve restart. Güncel systemd sınırlarıyla çalıştı. |
| Hazır image / Compose | `tests/team_v2_compose.py` son image ve yeni paketle geçti: archive SHA256, docker load/ID, Debian 13, executable SHA256, root olmayan kullanıcı. |
| İki PostgreSQL yolu | Aynı Compose testi ayrı PostgreSQL ve önceden kurulmuş TLS PostgreSQL'e bağlandı; sınırlı hesabın doğrudan tablo okuması ve TLS'siz erişim reddedildi. |
| Kaynaklar / toparlanma | Docker'dan 1 CPU, 512 MiB, swap kapalı, 128 süreç, salt okunur kök ve 10 MiB × 3 log sınırı doğrulandı. Healthcheck geçti. API süreci zorla sonlandırıldı; restart sayısı arttı ve HTTPS readiness geri geldi. |
| Release dosya listesi | `pnpm exec vitest run tests/releaseAssets.test.ts`: 4/4 geçti. 0.4.2 ve sonraki onaylı yayınlar image + kimlik JSON dosyasını zorunlu tutar; mevcut 0.4.1 listesi değiştirilmez. |
| Paketleme korumaları | Eksik/geçersiz kaynak commit'i reddedilir; mevcut sürümlü çıktı üzerine yazılmaz. Python sözdizimi ve Git fark kontrolü geçti. |

Testler WSL Ubuntu 24.04 üzerindeki Docker Engine'de **gerçek Debian 13 container** ile yapıldı. Fiziksel Debian 13 host kurulumu, şirket DNS/üretim sertifikası, yük kapasitesi ve host reboot testi yapılmadı. Kaynak üst sınırları bir kullanıcı kapasitesi garantisi değildir. Unhealthy durumu tek başına Docker restart tetiklemez; DB/CA sorunu ayrıca `/ready` ile izlenir.

## Dal düzeni ve yayın

Beş eski uzak dalın ve sekiz yerel dalın commit'leri önce `main` atası olarak doğrulandı, sonra tamamlanmış dallar silindi. `develop` 0.4.1'e fast-forward edildi. Eski çalışma dizini detached HEAD ile korundu. Kaydedilmemiş 0.5 SQL Workspace çalışması korunur.

Yeni dağıtım kaynağı `codex/team-linux-deployment` dalındadır. Bu teslim **yereldir**: yeni GitHub Release/tag, main birleştirmesi veya otomatik güncelleme yayını yapılmadı; mevcut yayımlanmış 0.4.1 dosyaları değiştirilmedi. Yeni image'ın Release'e eklenmesi sürüme özel yayın kararına bağlıdır.
