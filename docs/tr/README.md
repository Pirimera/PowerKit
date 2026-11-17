# PowerKit

![PowerKit](../../brand/social/readme-header-1280x240.png)

Rust ile geliştirilen hafif Windows araç paketi. Birincil hedef Windows 11 ARM64,
ikinci hedef x64. Microsoft ile bağlantılı olmayan bağımsız bir projedir.

AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer ve MediaRenamer hazırdır
(MediaRenamer'ın fiziksel kabul testleri sürüyor). Ürün dağıtımı Microsoft Store
(ana kanal) ve Store paketini gösteren winget’tir; MSIX şablonu ve yerel paketleme
betiği hazır, Store listesi henüz yayında değil. GitHub Releases ürün indirme
kanalı değildir.
Cihazdan bağımsız fare tuşu eşleyicisi PowerMouse Orta/Geri/İleri, tekerlek
eğimi ve F13–F24 için tek eylem / uzun basış / jest ve özel kısayol
kayıtını destekler; tepsi alt menüsü ve fiziksel kabul testleri sürüyor.
Arayüz Türkçe, İngilizce ve Arapçadır; topluluk belgelerinin ana dili
İngilizcedir.

Store listesi yayınlandığında Microsoft Store’dan veya winget ile kurun. Paket
MSIX’tir ve Windows App Runtime 2.4+ Store bağımlılığıdır. Geliştirme makinesinde
paketsiz çalıştırma (uygulama içi güncelleme yok) veya self-signed test MSIX
yeterlidir; Store paketi mağaza tarafından yeniden imzalanır.

AlwaysOn süresiz, 30/60/120 dakika, saat ve dakika olarak özel bir süre ya da
seçilen bir tarih ve saate kadar çalışır. Ekranı da açık tutabilirsiniz.
"Uygulama çalıştıkça açık tut" son etkin pencerenin uygulamasına bağlanır ve
uygulama kapanınca durur. Win+Ctrl+Shift+A (doluysa Win+Alt+Shift+A) AlwaysOn'u
açıp kapatır; açarken bu oturumda son seçilen kipi kullanır. Süreli oturum
bitmeden 5 dakika önce bildirim çıkar; tıklamak veya tepsideki "30 dakika uzat"
süreyi uzatır. Güç koşullarıyla AlwaysOn yalnızca prizde çalışabilir ya da pil
belirli bir seviyenin altına düşünce durur. `powercfg /requests` isteği
"PowerKit AlwaysOn" olarak gösterir. Terminalden `powerkit --always-on 90`
(dakika), `--always-on on`, `--always-on off` veya `--always-on-pid <pid>`
çalışan örneği yönetir ya da PowerKit'i gizli başlatır.
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
ScreenRuler, Win+Ctrl+Shift+M ile açılır. Çubuk PowerToys ile aynıdır: birincil
çalışma alanının üstünde simge düğmeleri. Sınır, aralık, yatay ve dikey ölçer;
aynı araca tekrar basmak seçimi kaldırır. Sol tık ölçümü panoya kopyalar,
Shift işareti ekranda bırakır, Esc kapatır.
Tekerlek kenar toleransını o oturum için 15 değiştirir. Ölçüm kaydedilmez.
ImageResizer, tepsi komutundan veya resmin Explorer menüsünden açılır.
Birden fazla resim tek pencerede açılır. Windows 11 kompakt sağ tık menüsü MSIX
kurulumunda paket manifestinden, paketsiz çalıştırmada Geliştirici Modu açıkken
kaydolan kabuk paketinden gelir. Klasik menü
"Diğer seçenekleri göster" altında durur. Küçük, Orta, Büyük ve Telefon boyutları PowerToys ile
aynıdır. Yazmadan önce önizleme gerekir. Kopyalama özgün dosyanın üzerine
yazmaz; üzerine yazma önce `.bak` bırakır, sonra onu geri dönüşüme göndermeyi
dener. Çok kareli dosyada yalnızca ilk kare işlenir.
Ana pencere Ana Sayfa, AlwaysOn, AlwaysTop, ScreenRuler, ImageResizer ve PowerMouse sayfalarından oluşur.
Ana Sayfa PowerToys benzeri düzenle sürüm ve sürüm notları, başlangıçta çalıştırma,
dil, tema ve tepsi simgesi seçeneklerini sunar; paket kimliği ve ayar yolu da burada görünür.
X düğmesi tepsiye gizler; tam çıkış için Ana Sayfa’daki çıkış düğmesini kullanın.
Manuel uyku ve kapak kapatma engellenmez. Her açılışta AlwaysOn ve AlwaysTop kapalı başlar.

- [Ana rehber](../../README.md)
- [Katkı süreci](../../CONTRIBUTING.md)
- [Geliştirme ve test](../development.md)
- [Yayınlama](../releasing.md)
- [Fiziksel kabul listesi](../testing.md)
- [Sürümlü doğrulama kayıtları](../validation/README.md)
- [MIT lisansı](../../LICENSE)
