# Kurulum ve yayınlama

## Mimari

PowerKit 0.2.0, Velopack Rust SDK ve vpk 1.2.161 kullanır. Uygulama için .NET
kurulumu gerekmez; .NET 10 yalnızca paketleme aracını çalıştırır.
MSVC çalışma zamanı statik bağlanır; ayrıca Visual C++ Redistributable kurulmaz.
Paket kimliği Pirimera.PowerKit, görünen ad PowerKit'tir.
Windows 11 (build 22000+) hedeflenir. Kurulum kullanıcı bazlıdır, yönetici
gerektirmez; Başlat menüsüne kısayol ekler. Otomatik Windows başlangıcı eklenmez.
Ayarlar mevcut %LOCALAPPDATA%\PowerKit\settings.txt konumundadır;
kurulumun Pirimera.PowerKit dizininden ayrıdır.

ARM64: win-arm64 kanalı, aarch64-pc-windows-msvc.
x64: win-x64 kanalı, x86_64-pc-windows-msvc.
Kanalları ve paket kimliğini yayımladıktan sonra değiştirmeyin.

## Private repo ve public geçişi

Actions her iki durumda GITHUB_TOKEN ile release yayımlar. Token yalnızca
publish job'unda contents:write yetkisi alır; uygulamaya veya pakete konmaz.
Uygulama https://github.com/Pirimera/PowerKit adresini anonim kullanır.
Private repo durumunda otomatik indirme çalışmaz; denetim başarısızlığında
AlwaysOn çalışmaya devam eder. Kurucuyu GitHub'a giriş yaparak indirin.
Repo public yapıldığında aynı kurulum, adres ve kanallar ile otomatik güncelleme
çalışır. Kod veya yeni token gerekmez.

## Güncelleme davranışı

Kurulu uygulama açılışta, ardından 6 saatte bir denetler. Manuel düğme en az
60 saniye aralıkla denetler. Ağ işi ayrı iş parçacığındadır.
Yeni kararlı sürüm indirilir; ön sürümler alınmaz ve sürüm düşürme kapalıdır.
İndirme tamamlanınca normal "PowerKit'ten çık" komutunda uygulanır.
"Güncelle ve yeniden başlat" AlwaysOn kapalıyken kullanılabilir.
Güç isteği UI iş parçacığında temizlenmeden updater başlatılmaz.
X düğmesi tepsiye gizlediğinde güncelleme uygulanmaz. Windows oturumu kapanırken
güncelleme başlatılmaz. Eksik/başarısız indirme mevcut sürümü değiştirmez.
Ham geliştirme exe'sinde otomatik güncelleme kapalıdır.
İlk sürümde yalnızca tam paket indirilir; delta optimizasyonu sonraya bırakılmıştır.

## GitHub Actions

main push, pull request ve workflow_dispatch: test, lint, iki mimaride native
derleme, AlwaysOn smoke testi, paket indirme/bozulma testi ve installer artifact üretimi. Yayın yapmaz.
v* etiketi push: aynı kontroller, ardından iki mimariyi tek GitHub Release'e
yayımlar. Etiket Cargo.toml sürümü ile tam eşleşmelidir (ör. v0.2.0).
Publish job ancak iki build başarılıysa çalışır. Önce draft oluşturur, tüm
dosyaları yükler, sonra yayımlar. Yayımlanmış sürümler değiştirilemez; yeni sürüm
numarası kullanın. Başarısız draft işi aynı etiketle tekrar çalıştırılabilir.
Action sürümleri commit SHA ile, Rust 1.99.1 ve Velopack 1.2.161 ile sabitlenmiştir.

İlk deneme: değişiklikleri main'e push edin, Actions'tan ARM64 artifact'ini indirin.
Kontroller geçtikten ve cihazda kurulum denendikten sonra:
~~~powershell
git tag v0.2.0
git push origin v0.2.0
~~~
Sonraki sürümde Cargo.toml, Cargo.lock, app.manifest ve RELEASE-NOTES.md güncellenir;
örneğin v0.2.1 etiketi yayımlanır. Yeni sürüm eski sürümden büyük olmalıdır.

## Yerel üretim

Rust/MSVC, Windows SDK, LLVM clang ve .NET 10 SDK gerekir. LLVM, Velopack'in
TLS bağımlılığı ring'in ARM64 derlemesi içindir; kullanıcılara dağıtılmaz.
~~~powershell
cargo test --locked --target aarch64-pc-windows-msvc
cargo clippy --locked --all-targets --target aarch64-pc-windows-msvc -- -D warnings
cargo build --release --locked --target aarch64-pc-windows-msvc
./scripts/Test-AlwaysOn.ps1
dotnet tool install vpk --version 1.2.161 --tool-path .tools
./scripts/Package.ps1 -Architecture arm64 -VpkPath "$PWD/.tools/vpk.exe"
~~~
Çıktı dist/0.2.0/arm64/releases altındadır. Tekrar üretimde yeni OutputRoot seçin;
script var olan çıktı dizinini silmez. Feed JSON ve nupkg dosyaları kurucu kadar
önemlidir; GitHub Release'e hepsi birlikte yüklenir.

## İmzalama ve fiziksel kabul

İlk paketler Authenticode imzasızdır. SHA256 dosyaları bozulmayı denetler;
yayıncı imzası yerine geçmez. Public dağıtımdan önce sertifika veya Azure Artifact
Signing kurulabilir; vpk signTemplate / azureTrustedSignFile ile exe ve installer
imzalanır. Sertifika ve özel anahtar repoya konmamalıdır.

1. ARM64 Setup.exe ile kurun; Başlat menüsü ve Windows Yüklü uygulamalar kaydını kontrol edin.
2. AlwaysOn'u, ekran tercihini, tepsiye gizlemeyi ve normal çıkışı deneyin.
3. Ayarı değiştirip kapatın; sonraki sürüm kurulunca tercih korunmalı, AlwaysOn kapalı başlamalı.
4. İkinci sürüm yayımlandığında public repoda indirmeyi test edin. Private repoda iki kurucuyla
   manuel yükseltme denenebilir; bu, anonim otomatik indirme testinin yerine geçmez.
5. Güncelleme hazırken AlwaysOn açık olsun: uygulama kendiliğinden kapanmamalı,
   yeniden başlat düğmesi oturumu kesmemeli. Normal çıkıştan sonra yeni sürüm açılmalı.
6. Ağ yokken ve private repoda açılış/AlwaysOn çalışmalı. Yarım indirmeden sonra yeniden deneyin.
7. Windows Ayarlar'dan kaldırın: çalışan süreç, Başlat kısayolu ve uygulama kaydı kalkmalı.
   Kullanıcı ayarı ayrı PowerKit dizininde kalır.

Kaynaklar:
https://docs.velopack.io/getting-started/rust
https://docs.velopack.io/reference/cli/content/vpk-windows
https://docs.github.com/en/actions/reference/runners/github-hosted-runners
