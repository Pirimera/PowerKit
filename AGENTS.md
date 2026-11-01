# PowerKit çalışma kuralları

- Windows 11 ARM64 birincil hedeftir; derleme ile fiziksel doğrulamayı ayrı raporla.
- MouseMapper bu projenin mevcut kapsamı dışındadır.
- Windows FFI işlemlerini windows.rs ve settings.rs sınırlarında tut; unsafe gerekçesini açıkla.
- Güç isteği onu oluşturan iş parçacığında kaldırılmalıdır.
- Kullanıcı istemeden Windows servisi, başlangıç kaydı veya installer çalıştırma.
- Gerçek dosyalarda toplu işlem için önizleme, çakışma kontrolü ve geri alma tasarlanmalıdır.
- Değişikliğe uygun cargo test ve clippy kontrollerini çalıştır.
