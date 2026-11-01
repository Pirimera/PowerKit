# 0.2.0 yerel doğrulama

- Windows 11 ARM64: Rust 1.99.1 ile 12 birim testi, clippy -D warnings ve release derleme.
- Yerel vpk 1.2.161: ARM64 ve x64 kurucu, feed JSON ve nupkg üretimi; PE mimari ve SHA256 denetimi.
- Ek paket entegrasyon testi: gerçek feed'den yeni sürümü indirme, yeniden açılışta çevrimdışı
  önbellekten bulma, hazırken tekrar indirmeme ve kesilmiş/bozuk paketi reddetme.
  Sahte kurulum dizini kullanır; Update.exe veya installer çalıştırmaz.
- ARM64 AlwaysOn smoke testi; x64 testleri ve smoke testi ARM64 Windows emülasyonu altında.
  Bu, fiziksel x64 makine doğrulaması değildir.
- ARM64 normal arayüz: güncelleme düğmesi ve kurulum gerektiren durum metni görüldü;
  manuel denetleme tıklandı; normal çıkışla kapandı.
- Workflow actionlint ile kontrol edildi. GitHub Actions üzerinde henüz çalıştırılmadı.
- Gerçek kurulum/kaldırma, iki kurulu sürüm arasında updater'ın dosyaları değiştirmesi ve
  GitHub üzerinden otomatik indirme henüz doğrulanmadı. RELEASING.md fiziksel kabul listesini izleyin.

Yerel paket testi:
~~~powershell
$env:POWERKIT_TEST_FEED = (Resolve-Path dist/0.2.0/arm64/releases).Path
cargo test --locked --target aarch64-pc-windows-msvc packaged_feed_download_cache_and_corruption -- --ignored
~~~

Actions bu testi paket üretiminden sonra iki mimaride de çalıştırır.
Private repo için kişisel token uygulamaya eklenmemiştir. İlk kurucular imzasızdır.
