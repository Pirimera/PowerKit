# PowerStore

[Kullanıcı kılavuzu](README.md) · [English](../power-store.md)

PowerStore bilgisayarınızda kurulu yazılımları tek bir tabloda gösterir ve
zaten sahip olduğunuz paket yöneticileri üzerinden kurar, günceller ve
kaldırır. O yöneticilerin yerini almaz; onları sizin yerinize çalıştırır.

## Paket yöneticileri

| Yönetici | Notlar |
| --- | --- |
| WinGet | App Installer adıyla Windows'un parçasıdır. `winget` ve `msstore` kaynaklarını kapsar |
| Scoop | |
| Chocolatey | Sürüm 2.0 veya üstü |
| npm, pip, Cargo, .NET Tool | Yalnızca genel (global) kurulan araçlar; bir projenin paketleri değil |
| Windows PowerShell, PowerShell 7 | Kurulu modüller |
| vcpkg | |

Kurulu olmayan bir yönetici tabloda yer almaz. PowerStore yönetici kurmaz;
ayarları her birinin kurulum komutunu ya da bağlantısını gösterir.

## Paket tablosu

Power Store sayfası kurulu paketleri listeler. Güncellemesi olan paketler en
üstte **Güncellemesi olanlar** başlığında toplanır, ardından **Yüklü
uygulamalar** gelir. Sayfayı açtığınızda son liste hemen gösterilir,
yöneticiler arka planda yeniden sorulur.

| Denetim | Ne yapar |
| --- | --- |
| Filtre kutusu | Siz yazdıkça listeyi ada ya da paket kimliğine göre daraltır. Çevrimiçi katalogları değil, listeyi arar |
| Kaynaklar | Yalnızca seçilen kaynakları gösterir |
| Tüm durumlar, Güncellemesi var, Güncel, Yoksayılan / Atlanan | Güncelleme durumuna göre süzer |
| Tüm kapsamlar | Sizin için ya da bütün bilgisayar için kurulan paketlere göre süzer |
| Sırala | Ada, paket kimliğine, yüklenme tarihine ya da son güncellemeye göre |
| Yenile | Yöneticileri yeniden sorar |

Her satırda bir ana düğme (**Güncelle**, **Kaldır**, **Kur** ya da **Bilgi**)
ve diğer komutları içeren bir **⋯** menüsü vardır. Birden çok pakette işlem
yapmak için satırları işaretleyip **Seçilenleri güncelle**, **Kaldır** ya da
**Güncellemeleri yoksay** komutunu kullanın. **Tümünü güncelle**, geçerli
filtrelerle eşleşen paketleri günceller.

## Yeni yazılım bulma ve kurma

**Yeni Paket Ara**'yı seçin, bir ad yazın ve arayın. Sonuçlar aynı tabloda
görünür; **Kur** birini ekler ya da birkaçını işaretleyip **Seçilenleri
yükle**'yi seçin. **Yüklülere dön** listenize geri döner.

## Güncellemeler

PowerStore güncellemeleri arka planda denetler: ilk kez PowerKit başladıktan
bir dakika sonra, ardından belirlediğiniz aralıkla. Sayıyı tepsi menüsünde
gösterir (örneğin "Power Store (3)") ve yeni güncellemeler çıktığında bildirim
verir. **Hiçbir güncellemeyi kendiliğinden kurmaz.**

Güncellemek istemediğiniz bir paket için:

| Komut | Etkisi |
| --- | --- |
| Bu sürümü atla | Önerilen sürümü gizler. Daha yeni bir sürüm yeniden önerilir |
| Güncellemeleri yoksay | Siz **Yoksaymayı kaldır**'ı seçene kadar paketin bütün güncellemelerini gizler |

## İşlemler

Kurma, güncelleme ve kaldırma işlemleri başlattığınız sırayla, teker teker
çalışır. Satır ilerlemeyi, sıradaki paket için **Bekliyor** yazısını gösterir.

- Çalışan paketin **⋯** düğmesi onu iptal eder, bekleyen paketinki onu sıradan
  çıkarır. **Bekleyenleri iptal et** sırayı boşaltır.
- Bir yükleyici kendi penceresinde yönetici izni isteyebilir. PowerStore'un
  kendisi hiçbir zaman yönetici olarak çalışmaz.
- Bir yükleyici kendi penceresini açtıktan sonra iptal, ancak o yükleyici
  geçerli adımını bitirdiğinde etkili olabilir.
- **Geçmiş**, biten işlemleri çıktılarıyla listeler; bir kurulum başarısız
  olduğunda işe yarar. En son 200 işlemi tutar.

## Paket ayrıntıları ve kurulum seçenekleri

Yayımcı, lisans ve ana sayfa gibi ayrıntıları açmak için paketin adına
tıklayın. Destekleyen yöneticilerde aynı panelde kurulum seçenekleri bulunur;
PowerStore bunları o paketin kurulumunda ve sonraki güncellemelerinde
kullanır:

| Seçenek | Kullanılabildiği yöneticiler |
| --- | --- |
| Kapsam (Kullanıcı ya da Makine), Mimari | WinGet |
| Sürüm | WinGet, Scoop, Chocolatey, pip |
| Ek yükleyici bağımsız değişkenleri | WinGet, Chocolatey |

## Ayarlar

Dişli düğmesi ayarları açar.

| Ayar | Ne yapar |
| --- | --- |
| Paket yöneticileri | Her yönetici için bir anahtar. Kapalı bir yönetici listede, aramada ve güncelleme denetiminde yer almaz. Yanındaki ok o yöneticinin kendi ayarlarını açar: kullanılacak program, WinGet için varsayılan kapsam ve mimari, vcpkg için varsayılan triplet |
| Güncelleme denetim aralığı | 15 dakikadan 7 güne kadar |
| Güncelleme bildirimi göster | Yukarıda anlatılan bildirimi gösterir |
| Uzak paket logoları | Eksik logoları UniGetUI simge veritabanından indirir. Varsayılan olarak açıktır; ağ gerekir |
| Yoksayılan paketler, Atlanan sürümler | Yukarıdaki komutlarla oluşturduğunuz listeler; her girdiyi çıkaran bir düğmeyle |
| Dışa aktar, İçe aktar | Kurulu paketleri bir dosyaya kaydeder ya da böyle bir dosyadaki paketleri kurar |

**Dışa ve içe aktarma:** dosya `winget export` biçimindedir; `winget import`
WinGet kısmını okuyabilir. Diğer yöneticilerin paketleri ve kurulum
seçenekleriniz ayrı bir bölümde saklanır. İçe aktarma her paketi bir onay
kutusuyla gösterir ve yalnızca işaretli bıraktıklarınızı kurar. Yalnızca bir
yöneticinin yeniden kurabileceği paketler dışa aktarılır.

## Bilmekte fayda var

- Hiçbir paket yöneticisinin tanımadığı yazılımlar `local` kaynağıyla
  görünür. Listelenir ama buradan güncellenemez.
- Microsoft Store'un dağıttığı uygulamalar, Store'un kendi güncellemeler
  sayfasını açan **Store'da güncelle** düğmesini gösterir. PowerStore'dan
  kaldırılamazlar.
- PowerKit'in kendisi PowerStore tarafından güncellenmez ya da kaldırılmaz;
  bunu Store yapar.
- WinGet "yüklü değil" ya da "çok eski" olarak bildirilirse Microsoft
  Store'dan **App Installer**'ı kurun ya da güncelleyin.
- PowerStore; Cargo, .NET Tool, vcpkg ve PowerShell paketlerinin
  güncellemelerini algılamaz. Böyle bir pakette **Güncelle**, yöneticiden en
  yeni sürümü ister.
- Henüz elle denenmeyenler: `msstore` kaynağından kurulum, yönetici izni
  isteyen yükleyiciler ve kurulu oldukları bir bilgisayarda Chocolatey ile
  vcpkg yöneticileri.
