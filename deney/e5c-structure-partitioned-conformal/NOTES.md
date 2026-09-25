# E5c — notes during the experiment

- Smoke test (order 0): full wrapper 99.7% accuracy, realized risk 0.003, 1.11 per call; the all-off cell 98.3%, 0.017, 1.42.
- The all-off cell (C0 G0 N0 S0) holds out the most recent **40%** of labels (the plan fixes the share for every cell), while E5 and E5b held out 25% and trained on 75%: it is the right contrast within E5c but not identical to E5's loop; E5b's L0 is reported beside it.

- **Durduruldu (2026-09-25):** tam çalıştırma (190 simülasyon, 8 çekirdek) 30 dakikada bitmedi ve kullanıcı tarafından durduruldu; sonuç yok. Yavaşlığın nedeni: her 30 vakada yeniden eğitim + her kalibrasyon etiketinin yeniden skorlanması + eğitim metinleri arası tam benzerlik matrisi. Devam etmeden önce hızlandırılmalı (örneğin önce tek bir akış sırasında 16 hücre, ya da daha seyrek yeniden eğitim; hangisi olursa plana sapma olarak not edilecek).
