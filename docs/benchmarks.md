# Benchmarks

## Method

- **Questions:** 70 questions whose correct answer depends on the version, in [`bench/questions.json`](https://github.com/SylphxAI/lockdocs/blob/main/bench/questions.json), over zod 3/4, Next.js 14/15/16, React Router 6/7, pydantic 1/2, axum 0.7/0.8, tokio, Tailwind CSS 3/4, ESLint 8/9, Prisma 5/6, React 18/19, Vite 5/6, Express 4/5, SQLAlchemy 1.4/2.0, Django 4.2/5.1 and FastAPI 0.88/0.115. Twin questions are worded identically for both versions; each records its source of truth.
- **Projects:** one project per version in [`bench/projects`](https://github.com/SylphxAI/lockdocs/tree/main/bench/projects), installed at those pins by [`bench/setup.sh`](https://github.com/SylphxAI/lockdocs/blob/main/bench/setup.sh).
- **Grading:** an answer passes when it contains at least one string from every `expect` group (the version-correct API, in code or prose form) and none of the `reject` strings (the other version's API). Case-insensitive; the same grader for every tool.
- **lockdocs**, default settings (1,200-token budget), in three configurations: keyword only (`LOCKDOCS_EMBED=0`) on package files; hybrid on package files (the offline default once the model is downloaded); hybrid after `lockdocs fetch` added upstream docs at each version's tag. Index and fetch times are reported separately.
- **Context7:** the anonymous API as its MCP server uses it: search the library, pick the top result and its listed version with the same major (exact when listed), then fetch context for the question. Rate-limit responses are recorded, not retried.
- **Tokens:** tiktoken `o200k_base`. **Latency:** wall time per call from the same GitHub-hosted runner (for lockdocs: a fresh CLI process per question, including loading the model).
- **Runner:** [`bench/run.py`](https://github.com/SylphxAI/lockdocs/blob/main/bench/run.py) via the [`bench` workflow](https://github.com/SylphxAI/lockdocs/actions/workflows/bench.yml). Reproduce: `bash bench/setup.sh && python3 bench/run.py target/release/lockdocs bench/projects out.json --fetch --context7`.

## Results

<!-- results -->

From [this run](https://github.com/SylphxAI/lockdocs/actions/runs/36118537122).

Tokenizer: tiktoken o200k_base. Runner: Linux x86_64.

| | correct | median tokens | median latency | p95 latency |
|---|---|---|---|---|
| lockdocs | 31/36 | 1531 | 16 ms | 20 ms |
| context7 | 27/36 | 979 | 1723 ms | 3001 ms |

| subset | questions | lockdocs | Context7 |
|---|---|---|---|
| Older major (zod 3, Next 14, React Router 6, pydantic 1, axum 0.7) | 16 | 14/16 | 8/16 |
| Newer major (zod 4, Next 15, React Router 7, pydantic 2, axum 0.8) | 17 | 15/17 | 16/17 |
| tokio | 3 | 2/3 | 3/3 |

Context7 (anonymous): 47 HTTP calls, 0 rate-limited (429), 0 other errors; ratelimit-limit header 200, remaining 147.

| question | version | lockdocs | tokens | ms | Context7 | tokens | ms | library |
|---|---|---|---|---|---|---|---|---|
| zod3-strict | zod@3.23.8 | ✅ | 1719 | 5 | ✅ | 685 | 2552 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-strict | zod@4.1.5 | ❌ | 1829 | 10 | ✅ | 688 | 2620 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-email | zod@3.23.8 | ✅ | 1701 | 6 | ✅ | 1576 | 1324 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-email | zod@4.1.5 | ✅ | 2030 | 10 | ✅ | 1328 | 1591 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-error | zod@3.23.8 | ✅ | 1602 | 5 | ✅ | 893 | 1723 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-error | zod@4.1.5 | ✅ | 1842 | 9 | ❌ | 846 | 1459 | /colinhacks/zod/v4.0.1 (same major) |
| zod4-record | zod@4.1.5 | ✅ | 1771 | 10 | ✅ | 811 | 1656 | /colinhacks/zod/v4.0.1 (same major) |
| next14-cookies | next@14.2.15 | ✅ | 1480 | 16 | ❌ | 987 | 2974 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-cookies | next@15.1.0 | ✅ | 1351 | 18 | ✅ | 980 | 2950 | /vercel/next.js/v15.1.11 (same major) |
| next14-headers | next@14.2.15 | ✅ | 1405 | 15 | ✅ | 1119 | 1703 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-headers | next@15.1.0 | ✅ | 1400 | 20 | ✅ | 918 | 2366 | /vercel/next.js/v15.1.11 (same major) |
| next14-nostore | next@14.2.15 | ✅ | 1468 | 15 | ✅ | 979 | 1678 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-connection | next@15.1.0 | ✅ | 1464 | 18 | ✅ | 1198 | 1489 | /vercel/next.js/v15.1.11 (same major) |
| next15-after | next@15.1.0 | ✅ | 1446 | 18 | ✅ | 1951 | 1665 | /vercel/next.js/v15.1.11 (same major) |
| rr6-json | react-router@6.26.2 | ✅ | 1481 | 4 | ❌ | 606 | 2420 | /websites/reactrouter (unversioned) |
| rr7-data | react-router@7.1.1 | ✅ | 1586 | 7 | ✅ | 572 | 2711 | /websites/reactrouter (unversioned) |
| rr6-defer | react-router@6.26.2 | ✅ | 1508 | 4 | ✅ | 975 | 1515 | /websites/reactrouter (unversioned) |
| rr6-future | react-router@6.26.2 | ✅ | 1559 | 4 | ✅ | 420 | 1415 | /websites/reactrouter (unversioned) |
| rr7-router | react-router@7.1.1 | ✅ | 1490 | 7 | ✅ | 1220 | 1985 | /websites/reactrouter (unversioned) |
| pyd1-dict | pydantic@1.10.18 | ✅ | 1532 | 17 | ❌ | 908 | 2975 | /pydantic/pydantic (unversioned) |
| pyd2-dict | pydantic@2.9.2 | ✅ | 1437 | 20 | ✅ | 839 | 3001 | /pydantic/pydantic (unversioned) |
| pyd1-parse | pydantic@1.10.18 | ❌ | 1620 | 17 | ❌ | 2146 | 3102 | /pydantic/pydantic (unversioned) |
| pyd2-parse | pydantic@2.9.2 | ❌ | 1543 | 20 | ✅ | 2146 | 1350 | /pydantic/pydantic (unversioned) |
| pyd1-validator | pydantic@1.10.18 | ✅ | 1577 | 17 | ❌ | 1201 | 2932 | /pydantic/pydantic (unversioned) |
| pyd2-validator | pydantic@2.9.2 | ✅ | 1386 | 20 | ✅ | 1207 | 1608 | /pydantic/pydantic (unversioned) |
| pyd1-schema | pydantic@1.10.18 | ✅ | 1525 | 17 | ❌ | 1742 | 2395 | /pydantic/pydantic (unversioned) |
| pyd2-schema | pydantic@2.9.2 | ✅ | 1449 | 20 | ✅ | 1618 | 1458 | /pydantic/pydantic (unversioned) |
| pyd1-config | pydantic@1.10.18 | ❌ | 1540 | 17 | ❌ | 819 | 1512 | /pydantic/pydantic (unversioned) |
| pyd2-config | pydantic@2.9.2 | ✅ | 1527 | 20 | ✅ | 683 | 1768 | /pydantic/pydantic (unversioned) |
| axum07-path | axum@0.7.9 | ✅ | 1581 | 6 | ✅ | 1674 | 2514 | /tokio-rs/axum (unversioned) |
| axum08-path | axum@0.8.1 | ✅ | 1573 | 7 | ✅ | 1465 | 2160 | /tokio-rs/axum (unversioned) |
| axum07-extractor | axum@0.7.9 | ✅ | 1512 | 6 | ❌ | 1652 | 1666 | /tokio-rs/axum (unversioned) |
| axum08-optional | axum@0.8.1 | ✅ | 1479 | 7 | ✅ | 469 | 1569 | /tokio-rs/axum (unversioned) |
| tokio-blocking | tokio@1.43.0 | ✅ | 1435 | 18 | ✅ | 789 | 2347 | /websites/rs_tokio_tokio (unversioned) |
| tokio-select | tokio@1.43.0 | ❌ | 1574 | 18 | ✅ | 498 | 1405 | /websites/rs_tokio_tokio (unversioned) |
| tokio-timeout | tokio@1.43.0 | ✅ | 1531 | 18 | ✅ | 921 | 1295 | /websites/rs_tokio_tokio (unversioned) |

Index build (cold cache, all direct deps):

- axum07: 201 ms
- axum08: 205 ms
- next14: 403 ms
- next15: 391 ms
- pydantic1: 51 ms
- pydantic2: 98 ms
- rr6: 175 ms
- rr7: 372 ms
- zod3: 30 ms
- zod4: 51 ms

## Reading the results

- lockdocs answers only from what the package ships. When a library keeps its guides on a website only (Next.js ships no docs in its npm package), lockdocs relies on type declarations and doc comments, and a hosted index can know more.
- Context7 serves a few indexed versions per library; when the pinned version is not one of them, the table says which one it used.
- The question set is small and fixed on purpose, so anyone can rerun it and check every answer. Contributions of new version-sensitive questions are welcome.
