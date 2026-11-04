# Aidoku AI Sources

Свой репозиторий нативных источников для Aidoku AI: **18 источников**, включая
MangaLib, MangaBuff и остальные сайты из списка для переноса из Mihon.
Источники загружают каталог, главы и страницы прямо на iPhone. Для AI-улучшения
можно использовать существующий Companion на ПК.

## Добавление в приложение

В Aidoku AI открой **Настройки → Список источников → +** и вставь:

```text
https://raw.githubusercontent.com/Ranennder/Aidoku-AI-Sources/repo/index.min.json
```

Затем открой **Обзор → +** и установи нужные источники. В языковом фильтре выбери
русский и английский: Asura Scans находится среди английских источников.
Если нужный сайт скрыт, проверь также фильтр возрастного рейтинга.
У многоязычных сайтов язык чтения выбирается в настройках источника.
Обновления приходят через тот же список; для обычных обновлений источников
пересобирать IPA не требуется.

Для новых портов установи **Aidoku AI 0.9.9 (13)** из
[репозитория приложения](https://github.com/Ranennder/Aidoku-AI-GBox).
Эта версия исправляет перенос сессии входа с родительского домена и загрузку
изображений, когда источник возвращает ссылку на другой сервер.

Это список источников чтения внутри Aidoku. В Feather/GBox остаётся отдельный
источник обновлений самого приложения.

## Источники

| Источник | Сайт | ID |
| --- | --- | --- |
| Nude-Moon | https://nude-moon.org | `ru.nudemoon` |
| AllHentai | https://20.allhen.online | `ru.allhentai` |
| Asura Scans (английский) | https://asurascans.com | `en.asurascans` |
| Desu | https://desu.uno | `ru.desu` |
| HenChan | https://xxl.hentaichan.live | `ru.henchan` |
| HentaiZap (несколько языков) | https://hentaizap.com | `multi.hentaizap` |
| Kagane (несколько языков) | https://kagane.moe | `multi.kagane` |
| Manga-shi | https://manga-shi.org | `ru.mangashi` |
| MangaBuff | https://mangabuff.ru | `ru.mangabuff` |
| Mangadotnet (несколько языков) | https://mangadot.net | `multi.mangadotnet` |
| MangaHub RU | https://mangahub.ru | `ru.mangahub` |
| ReadManga | https://a.zazaza.me | `ru.readmanga` |
| Senkognito | https://senkognito.com | `ru.senkognito` |
| Senkuro | https://senkuro.me | `ru.senkuro` |
| Simply Hentai (несколько языков) | https://www.simply-hentai.com | `multi.simplyhentai` |
| SlashLib | https://slashlib.me | `ru.slashlib` |
| WaManga | https://wamanga.ru | `ru.wamanga` |
| MangaLib | https://mangalib.me | `ru.mangalib` |

Доступность глав зависит от сайта и разрешений аккаунта. Источники не обходят
закрытые главы. Если сайт требует вход, используй его в веб-интерфейсе источника.
Для входа в MangaBuff v2 требуется **Aidoku AI 0.9.8 или новее**: эта версия
исправляет передачу сессии между окном входа и загрузкой глав. При обновлении
с MangaBuff v1 войди заново в настройках источника.

Порты проверяются локальными примерами ответов и страниц, затем собираются
официальным SDK и проходят проверку формата `.aix`. Это не подтверждает вход
с личным аккаунтом или доступность каждого сайта из любой сети.
При публичной проверке с этого ПК Nude-Moon, Senkuro и часть запросов Mangadotnet
ограничены защитой сайтов; HenChan вернул служебный ответ вместо каталога,
а MangaHub RU завершил TLS-соединение. Эти случаи не подменяются успешным
пустым результатом. Открой сайт через источник и проверь доступ из сети iPhone.
Подробности отдельных портов находятся в их `VALIDATION.md` / `UPSTREAM.md`.

Kagane может отдавать JPEG XL: для таких страниц нужна версия iOS с поддержкой
этого формата (iOS 17 или новее). Поддержка устройства и ограничения самого
сайта сохраняются при использовании нативного источника.

Существующая библиотека, импортированная через Suwayomi, сохраняет свои серверные
идентификаторы. Добавление этого списка само по себе не переносит такие записи
в нативные источники и не переписывает историю чтения. Оригиналы бэкапов Mihon
сохраняются приложением; текущий импорт Mihon через Suwayomi остаётся доступен.

## Разработка и сборка

Каждый источник — отдельный Rust-проект. Общего Cargo workspace нет: WebAssembly
каждого источника собирается и упаковывается отдельно. SDK и тестовый runtime
закреплены на `ae6485e3c14ecd1fd5497d170eae884c68d04a87`.

Сценарии сборки и проверки находятся в `tools/`, автоматическая проверка — в
`.github/workflows/`. Собранный каталог содержит `index.min.json`, пакеты `.aix`
и иконки; он публикуется в ветку `repo` после успешных проверок.

Сборка выполняется в Linux CI на Rust 1.99.0 и Python 3.12. После установки
`aidoku-cli` и `aidoku-test-runner` из указанной ревизии SDK:

```sh
python tools/build.py all
python -m unittest discover -s tools -p 'test_*.py' -v
aidoku verify dist/sources/*.aix
```

Обычные WASM-тесты используют локальные данные. Для отдельной проверки текущих
сайтов без аккаунта:

```sh
python tools/live_smoke.py --source all
```

Этот сценарий проверяет исходную пару MangaLib/MangaBuff. В GitHub Actions он
также доступен через **Build and test sources → Run workflow → live_smoke**.
Для новых портов результаты отдельных проверок описаны в их документации.
Проверка HTTP не заменяет проверку чтения на реальном iPhone.

Изменение поведения источника сопровождается увеличением `info.version` в
`res/source.json`. ID источника и ключи манги/глав сохраняются стабильными,
чтобы обновление не ломало библиотеку. Публикация отклоняет изменение уже
выпущенного пакета с тем же номером версии и сохраняет предыдущие пакеты.

## Происхождение и лицензии

Готовые нативные реализации адаптированы из
[Aidoku Community](https://github.com/Aidoku-Community/sources) с сохранением
Apache-2.0/MIT и уведомлений авторов. Остальные протоколы перенесены из
[расширений Mihon/Keiyoushi](https://github.com/keiyoushi/extensions-source)
под Apache-2.0. Подробности и закреплённые ревизии — в
[PROVENANCE.md](PROVENANCE.md) и [NOTICE](NOTICE).
Лицензия включённого SDK Aidoku сохранена отдельно в `LICENSE-AIDOKU-SDK`.
