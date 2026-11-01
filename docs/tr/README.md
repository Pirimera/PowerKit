# PowerKit

![PowerKit](../../brand/social/readme-header-1280x240.png)

Rust ile geliştirilen hafif Windows araç paketi. Birincil hedef Windows 11 ARM64,
ikinci hedef x64. Microsoft ile bağlantılı olmayan bağımsız bir projedir.

AlwaysOn, AlwaysTop, Velopack kurulumu ve güncelleme altyapısı hazırdır.
ScreenRuler, ImageResizer ve MediaRenamer planlanmıştır. MouseMapper kapsam dışıdır.
Arayüz şu anda Türkçedir; topluluk belgelerinin ana dili İngilizcedir.

GitHub Releases üzerinde yayın varsa mimarinize uygun Setup.exe dosyasını indirin.
Repo private iken GitHub erişimi gerekir. Uygulamada token yoktur; repo public olduğunda
anonim otomatik güncelleme çalışır. İlk paketler imzasızdır.

AlwaysOn süresiz veya 30/60/120 dakika çalışır. Ekranı da açık tutabilirsiniz.
AlwaysTop, odaklanan pencereleri üste sabitler; birden fazla pencere aynı anda
üste kalabilir. Win+Ctrl+T (veya doluysa Win+Alt+T) o anda öndeki pencereyi
sabitler veya kaldırır, diğer pinlere dokunmaz. Sabitlenen pencerenin çevresinde
vurgu rengi, PowerToys ile aynı yuvarlatılmış D2D çerçeve, pencerenin hemen
arkasında durur. Dosya Gezgini
de diğer uygulamaların üstünde kalır. Tepsi etiketi ve işaret hangi pencerenin
sabitlendiğini gösterir. Win+Ctrl++ / Win+Ctrl+- sabitlenen pencerenin
saydamlığını değiştirir; sabitleme kalkınca eski hali döner. Tepsideki Ses çal
işareti pin ve saydamlık seslerini açıp kapatır. Pencere kapanınca veya
PowerKit çıkınca sabitleme kalkar; pin ve saydamlık kaydedilmez.
X düğmesi tepsiye gizler; tam çıkış için uygulamadaki çıkış düğmesini kullanın.
Manuel uyku ve kapak kapatma engellenmez. Her açılışta AlwaysOn ve AlwaysTop kapalı başlar.

- [Ana rehber](../../README.md)
- [Katkı süreci](../../CONTRIBUTING.md)
- [Geliştirme ve test](../development.md)
- [Yayınlama](../releasing.md)
- [Fiziksel kabul listesi](../testing.md)
- [Sürümlü doğrulama kayıtları](../validation/README.md)
- [MIT lisansı](../../LICENSE)
