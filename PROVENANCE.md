# Происхождение портов

Ревизии исходного кода закреплены; это нативные WASM-пакеты для Aidoku, а не
Android APK. Изменения портов распространяются под Apache-2.0.

## Готовые нативные реализации

Следующие каталоги адаптированы из
[Aidoku Community, `4ca350a`](https://github.com/Aidoku-Community/sources/tree/4ca350a031e6e708ad1308a6f4127a69fe517841),
лицензия Apache-2.0 OR MIT. Оригинальные уведомления и обе лицензии сохранены.

| Пакет | Исходный каталог |
| --- | --- |
| `en.asurascans` | `sources/en.asurascans` |
| `multi.kagane` | `sources/multi.kagane` |
| `multi.mangadotnet` | `sources/multi.mangadotnet` |
| `ru.desu` | `sources/ru.desu` |
| `ru.mangalib` | `sources/ru.mangalib`, `templates/libgroup` |
| `ru.senkuro` | `sources/ru.senkuro` |
| `ru.senkognito` | `sources/ru.senkuro`, с отдельными адресами API и App-Id |
| `ru.slashlib` | `sources/ru.slashlib`, общий `templates/libgroup` |

## Протоколы расширений Mihon

Порты и исправления используют реализации
[Keiyoushi, `ee661f5`](https://github.com/keiyoushi/extensions-source/tree/ee661f5233b346559fbdd53993d447c318c5aef0),
лицензия Apache-2.0. Имена оригинальных авторов и уведомления, присутствующие
в исходных файлах, остаются за их авторами.

| Пакет | Исходный каталог / общий протокол |
| --- | --- |
| `multi.hentaizap` | `src/all/hentaizap`, `lib-multisrc/galleryadults` |
| `multi.simplyhentai` | `src/all/simplyhentai` |
| `ru.allhentai` | `src/ru/allhentai`, `lib-multisrc/grouple` |
| `ru.henchan` | `src/ru/henchan`, `lib-multisrc/multichan` |
| `ru.mangabuff` | `src/ru/mangabuff`; авторы перечислены в `NOTICE` |
| `ru.mangahub` | `src/ru/mangahub` |
| `ru.mangashi` | `src/ru/mangashi` |
| `ru.nudemoon` | `src/ru/nudemoon` |
| `ru.readmanga` | `src/ru/readmanga`, `lib-multisrc/grouple` |
| `ru.wamanga` | `src/ru/wamanga` |
| `en.asurascans`, `ru.desu` | соответствующие каталоги `src/en` / `src/ru`: сверка авторизации |
| `ru.senkuro`, `ru.senkognito` | `lib-multisrc/senkuro`: домены и заголовки API |

## Уведомления в установочных пакетах

Каждый пакет содержит `LICENSE.txt`, `LICENSE-MIT.txt`, `LICENSE-AIDOKU-SDK.txt`
и `NOTICE.txt`. Новые порты дополнительно включают собственный
`UPSTREAM-NOTICE.txt`. Исходный общий `NOTICE` сохранён без изменения, чтобы
байты уже опубликованных версий MangaLib и MangaBuff оставались неизменными.

SDK Aidoku закреплён на
[`ae6485e3`](https://github.com/Aidoku/aidoku-rs/tree/ae6485e3c14ecd1fd5497d170eae884c68d04a87),
MIT, Copyright (c) 2022–2025 Skittyblock. Названия и иконки сайтов идентифицируют
соответствующих владельцев; репозиторий не размещает страницы манги.
