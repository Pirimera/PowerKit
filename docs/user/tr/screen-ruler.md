# ScreenRuler

[Kullanıcı kılavuzu](README.md) · [English](../screen-ruler.md)

ScreenRuler ekrandaki uzaklıkları piksel olarak ölçer: çizdiğiniz bir
dikdörtgenin boyutunu ya da sizin yerinize bulduğu iki kenar arasındaki
boşluğu.

## Ölçme

1. Win+Ctrl+Shift+M tuşlarına basın, tepsi menüsünde **Screen Ruler** öğesini
   ya da Screen Ruler sayfasında **Ölçümü başlat** düğmesini seçin. Ana
   ekranın üstünde küçük bir araç çubuğu açılır.
2. Araç çubuğundan bir araç seçin.
3. Ölçümü panoya kopyalamak için tıklayın.
4. Bitirmek için Esc tuşuna ya da araç çubuğundaki kapat düğmesine basın.

| Araç | Kısayol | Ne ölçer |
| --- | --- | --- |
| Sınırlar | Ctrl+1 | Sürüklediğiniz dikdörtgeni |
| Aralık | Ctrl+2 | İşaretçinin çevresindeki en yakın kenarlara uzaklığı, iki yönde |
| Yatay aralık | Ctrl+3 | Aynısını, yalnızca sola ve sağa |
| Dikey aralık | Ctrl+4 | Aynısını, yalnızca yukarı ve aşağı |

- Etkin aracı yeniden seçmek onu kapatır.
- Tıklarken Shift'i basılı tutmak ölçümü ekranda bırakır; böylece bir ölçüm
  daha alabilirsiniz.
- Fare tekerleği, araç çubuğunu kapatana kadar kenar toleransını 15 değiştirir.
  Çizgiler gürültü ya da renk geçişi yüzünden erken duruyorsa artırın; silik
  bir kenarı aşıp gidiyorsa azaltın.
- Araç çubuğunu taşımak için sol ucundaki noktalı tutamaçtan sürükleyin.
  PowerKit kapanana kadar aynı yerde açılır.

## Ayarlar

| Ayar | Ne yapar |
| --- | --- |
| Ölçüm kısayolu | Varsayılan Win+Ctrl+Shift+M |
| Varsayılan kip | Araç çubuğu açıldığında seçili olan araç. Varsayılan Yok |
| Ölçerken ekranı sürekli yakala | Video gibi değişen içeriği izler. Varsayılan olarak kapalıdır; ekran ölçüm başlarken bir kez yakalanır |
| Renk kanalı başına kenar algılama | Kırmızı, yeşil ve maviyi toplamları yerine ayrı ayrı karşılaştırır. Varsayılan olarak kapalıdır |
| Piksel toleransı | İki pikselin kenar sayılması için ne kadar farklı olması gerektiği. Varsayılan 30 |
| Birim | Piksel değerinin yanında inç, santimetre, milimetre ya da DIP gösterir. Varsayılan yalnızca piksel |
| Çapraz çizgilere ayak çiz | Ölçüm çizgilerinin uçlarına küçük işaretler çizer. Varsayılan olarak açıktır |
| Çizgi rengi | Varsayılan `#FF4500` |

## Bilmekte fayda var

- Ölçüm kaydedilmez; sonuç panoya kopyalanan değerdir.
