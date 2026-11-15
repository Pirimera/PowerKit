# PowerKit

![PowerKit](../../brand/social/readme-header-1280x240.png)

Rust ile geliştirilen hafif Windows araç paketi. Birincil hedef Windows 11 ARM64,
ikinci hedef x64. Microsoft ile bağlantılı olmayan bağımsız bir projedir.

AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer, Velopack kurulumu ve güncelleme altyapısı hazırdır.
MediaRenamer planlanmıştır. MouseMapper kapsam dışıdır.
Arayüz şu anda Türkçedir; topluluk belgelerinin ana dili İngilizcedir.

GitHub Releases üzerinde yayın varsa mimarinize uygun Setup.exe dosyasını indirin.
Repo private iken GitHub erişimi gerekir. Uygulamada token yoktur; repo public olduğunda
anonim otomatik güncelleme çalışır. İlk paketler imzasızdır.

AlwaysOn süresiz veya 30/60/120 dakika çalışır. Ekranı da açık tutabilirsiniz.
AlwaysTop, odaklanan pencereleri üste sabitler; birden fazla pencere aynı anda
üste kalabilir. AlwaysTop sayfasından etkinleştirme ve saydamlık kısayolları ile
kenarlık rengi, opaklık ve kalınlık atanır. Varsayılan Win+Ctrl+T ve
Win+Ctrl++ / Win+Ctrl+- (doluysa Win+Alt) o anda öndeki pencereyi
sabitler veya kaldırır, diğer pinlere dokunmaz. Seçilen çerçeve, pencerenin hemen
arkasında durur. Dosya Gezgini
de diğer uygulamaların üstünde kalır. Tepsi etiketi ve işaret hangi pencerenin
sabitlendiğini gösterir. Saydamlık kısayolları sabitlenen pencerenin
saydamlığını değiştirir; sabitleme kalkınca eski hali döner. Tepsideki Ses çal
işareti pin ve saydamlık seslerini açıp kapatır. Pencere kapanınca veya
PowerKit çıkınca sabitleme kalkar; pin ve pencere saydamlığı kaydedilmez,
kısayol ve kenarlık kaydedilir.
ScreenRuler, Win+Ctrl+Shift+M ile açılır. Sınır, aralık, yatay ve dikey ölçer;
sol tık ölçümü panoya kopyalar, Shift işareti ekranda bırakır, Esc kapatır.
Tekerlek kenar toleransını o oturum için 15 değiştirir. Ölçüm kaydedilmez.
ImageResizer, tepsi komutundan veya resmin Explorer menüsünden açılır.
Birden fazla resim tek pencerede açılır. Windows 11 kompakt sağ tık menüsü,
Geliştirici Modu açıkken kaydolan kabuk paketinden gelir. Klasik menü
"Diğer seçenekleri göster" altında durur. Küçük, Orta, Büyük ve Telefon boyutları PowerToys ile
aynıdır. Yazmadan önce önizleme gerekir. Kopyalama özgün dosyanın üzerine
yazmaz; üzerine yazma önce `.bak` bırakır, sonra onu geri dönüşüme göndermeyi
dener. Çok kareli dosyada yalnızca ilk kare işlenir.
Ana pencere AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer ve genel ayar sayfalarından oluşur.
Genel sayfası PowerToys benzeri düzenle sürüm/güncelleme, başlangıçta çalıştırma,
dil, tema ve tepsi simgesi seçeneklerini sunar; paket kimliği ve ayar yolu da burada görünür.
X düğmesi tepsiye gizler; tam çıkış için Genel sayfasındaki çıkış düğmesini kullanın.
Manuel uyku ve kapak kapatma engellenmez. Her açılışta AlwaysOn ve AlwaysTop kapalı başlar.

- [Ana rehber](../../README.md)
- [Katkı süreci](../../CONTRIBUTING.md)
- [Geliştirme ve test](../development.md)
- [Yayınlama](../releasing.md)
- [Fiziksel kabul listesi](../testing.md)
- [Sürümlü doğrulama kayıtları](../validation/README.md)
- [MIT lisansı](../../LICENSE)
