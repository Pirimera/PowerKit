# AlwaysOn

[Kullanıcı kılavuzu](README.md) · [English](../always-on.md)

AlwaysOn, ihtiyacınız olduğu sürece Windows'un uykuya geçmesini engeller: uzun
bir indirme, bir sunum ya da bir derleme sırasında. Ekranı da açık tutabilir.

## Başlatma ve durdurma

Always On sayfasından ya da tepsi menüsünden bir kip seçin veya
Win+Ctrl+Shift+A tuşlarına basın.

| Kip | Davranış |
| --- | --- |
| AlwaysOn kapalı | Windows kendi güç ayarlarını izler |
| Süresiz | Siz kapatana kadar uyanık kalır |
| 30 dakika, 1 saat, 2 saat | O süre boyunca uyanık kalır, sonra kapanır |
| Özel süre | Belirlediğiniz saat ve dakika kadar uyanık kalır; en çok 99 sa 59 dk. Varsayılan 4 saattir |
| Belirli bir zamana kadar açık tut | Seçtiğiniz tarih ve saate kadar uyanık kalır. Tarih seçmeden girilen geçmiş bir saat yarın anlamına gelir |
| Uygulama çalıştıkça açık tut | Bir uygulama kapanana kadar uyanık kalır |

Kısayol, AlwaysOn açıkken onu kapatır. Kapalıyken, PowerKit başladığından beri
en son seçtiğiniz kipi başlatır.

**Uygulama çalıştıkça açık tut**, penceresi en son etkin olan uygulamayı
izler. Önce o uygulamanın penceresine tıklayın, ardından sayfadaki **Son
uygulamaya bağla** düğmesini ya da tepsi menüsündeki öğeyi seçin.

AlwaysOn açıkken tepsi simgesi değişir; ipucu metni kipi ve kalan süreyi
gösterir.

## Seçenekler

| Seçenek | Ne yapar |
| --- | --- |
| Ekranı da açık tut | Ekranın kapanmasını da engeller. Kapalıyken yalnızca uyku engellenir, ekran yine kapanabilir |
| Aç/kapat kısayolu | AlwaysOn'u açıp kapatan kısayol |
| Bitmeden önce uyar | Süreli oturum bitmeden 5 dakika önce bildirim gösterir. Tıklamak 30 dakika ekler. Tepsi menüsünde aynı işi yapan **30 dakika uzat** öğesi vardır |
| Yalnızca prizdeyken | AlwaysOn pille çalışırken başlamaz, bilgisayarı prizden çektiğinizde durur |
| Pil şu seviyenin altına düşünce durdur | Seçilen seviyenin altında AlwaysOn'u durdurur; %10'luk adımlarla en çok %90. 0 sınırı kapatır. Prizdeyken uygulanmaz |

## Komut satırından

```powershell
powerkit --always-on on      # süresiz
powerkit --always-on 90      # 90 dakika
powerkit --always-on off
powerkit --always-on-pid 1234   # 1234 numaralı işlem çalıştıkça
```

Komut zaten çalışan PowerKit kopyasını yönetir ya da PowerKit'i tepside
başlatır.

## Bilmekte fayda var

- PowerKit her açılışta AlwaysOn kapalı başlar. Bir oturum yeniden başlatmadan
  sonra sürdürülmez; yukarıdaki seçenekler hatırlanır.
- AlwaysOn yalnızca hareketsizlikten kaynaklanan uykuyu engeller. Bilgisayarı
  sizin uyutmanız, kapağı kapatmanız ve ekran koruyucular engellenmez.
- Bilgisayarın uykuda geçirdiği süre zamanlayıcıdan sayılır. Bitişi uyku
  sırasında geçen bir oturum, bilgisayar uyandığında kapalıdır.
- Çalıştığını denetlemek için yönetici olarak açılmış bir terminalde
  `powercfg /requests` komutunu çalıştırın. İstek "PowerKit AlwaysOn" adıyla
  listelenir.
- PowerKit'in güncellenmesi çalışan bir oturumu kesmez.
