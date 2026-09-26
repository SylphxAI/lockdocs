# Benchmarks

## Method

- **Questions:** 105 questions whose correct answer depends on the version, in [`bench/questions.json`](https://github.com/SylphxAI/lockdocs/blob/main/bench/questions.json), over zod 3/4, Next.js 14/15/16, React Router 6/7, pydantic 1/2, axum 0.7/0.8, tokio, Tailwind CSS 3/4, ESLint 8/9, Prisma 5/6, React 18/19, Vite 5/6, Express 4/5, SQLAlchemy 1.4/2.0, Django 4.2/5.1 and FastAPI 0.88/0.115. Twin questions are worded identically for both versions; each records its source of truth.
- **Held-out questions:** 18 questions marked `held-out` are not used for tuning: they were written before the ranking changes they measure and have their own column. Held-out questions that are later used to diagnose a miss join the main set (their `history` field says so), and new ones replace them. The 0.3 round did this once: 17 questions first written as held-out (lockdocs 11/17, Context7 12/17 on their first run) are now in the main set.
- **Projects:** one project per version in [`bench/projects`](https://github.com/SylphxAI/lockdocs/tree/main/bench/projects), installed at those pins by [`bench/setup.sh`](https://github.com/SylphxAI/lockdocs/blob/main/bench/setup.sh).
- **Grading:** an answer passes when it contains at least one string from every `expect` group (the version-correct API, in code or prose form) and none of the `reject` strings (the other version's API). Case-insensitive; the same grader for every tool.
- **lockdocs**, default settings (1,200-token budget), in three configurations: keyword only (`LOCKDOCS_EMBED=0`) on package files; hybrid on package files (the offline default once the model is downloaded); hybrid after `lockdocs fetch` added upstream docs at each version's tag. Index and fetch times are reported separately.
- **Context7:** the anonymous API as its MCP server uses it: search the library, pick the top result and its listed version with the same major (exact when listed), then fetch context for the question. When that library answers HTTP 404, the next search result is tried, as an agent would. Rate-limit responses are recorded, not retried.
- **Tokens:** tiktoken `o200k_base`. **Latency:** wall time per call from the same GitHub-hosted runner (for lockdocs: a fresh CLI process per question, including loading the model).
- **Runner:** [`bench/run.py`](https://github.com/SylphxAI/lockdocs/blob/main/bench/run.py) via the [`bench` workflow](https://github.com/SylphxAI/lockdocs/actions/workflows/bench.yml). Reproduce: `bash bench/setup.sh && python3 bench/run.py target/release/lockdocs bench/projects out.json --fetch --context7`.

## Results

<!-- results -->

From [this run](https://github.com/SylphxAI/lockdocs/actions/runs/36204404336).

Tokenizer: tiktoken o200k_base. Runner: Linux x86_64. 105 questions.

| | correct | older major | newer major | single version | held-out | median tokens | median latency | p95 latency |
|---|---|---|---|---|---|---|---|---|
| lockdocs, keyword only (BM25), package files | 59/105 | 29/45 | 26/55 | 4/5 | 8/18 | 882 | 25 ms | 276 ms |
| lockdocs, hybrid (BM25 + embeddings), package files | 60/105 | 27/45 | 29/55 | 4/5 | 6/18 | 903 | 52 ms | 103 ms |
| lockdocs, hybrid + upstream docs (after `lockdocs fetch`) | 96/105 | 38/45 | 53/55 | 5/5 | 16/18 | 866 | 97 ms | 474 ms |
| Context7 (anonymous API) | 77/105 | 19/45 | 53/55 | 5/5 | 15/18 | 908 | 2583 ms | 3398 ms |

Context7 (anonymous): 8 HTTP calls, 0 rate-limited (429), 4 other errors; ratelimit-limit header 200, remaining 190. 103 answers reused from the previous run's identical question, grading and version (see bench/run.py --context7-cache).

| question | version | keyword | hybrid | fetched | context7 | tokens (last lockdocs) | ms | Context7 library |
|---|---|---|---|---|---|---|---|---|
| zod3-strict | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 1075 | 87 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-strict | zod@4.1.5 | ✅ | ❌ | ✅ | ✅ | 1015 | 142 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-email | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 1119 | 47 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-email | zod@4.1.5 | ✅ | ✅ | ✅ | ✅ | 903 | 60 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-error | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 991 | 47 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-error | zod@4.1.5 | ✅ | ✅ | ✅ | ✅ | 991 | 60 | /colinhacks/zod/v4.0.1 (same major) |
| zod4-record | zod@4.1.5 | ❌ | ❌ | ✅ | ✅ | 946 | 55 | /colinhacks/zod/v4.0.1 (same major) |
| next14-cookies | next@14.2.35 | ❌ | ✅ | ❌ | ❌ | 926 | 395 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-cookies | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 866 | 448 | /vercel/next.js/v15.1.11 (same major) |
| next14-headers | next@14.2.35 | ✅ | ✅ | ✅ | ✅ | 896 | 94 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-headers | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 870 | 106 | /vercel/next.js/v15.1.11 (same major) |
| next14-nostore | next@14.2.35 | ✅ | ✅ | ✅ | ✅ | 843 | 97 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-connection | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 754 | 108 | /vercel/next.js/v15.1.11 (same major) |
| next15-after | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 827 | 100 | /vercel/next.js/v15.1.11 (same major) |
| rr6-json | react-router@6.26.2 | ✅ | ✅ | ❌ | ❌ | 796 | 120 | /websites/reactrouter (unversioned) |
| rr7-data | react-router@7.1.1 | ✅ | ❌ | ✅ | ✅ | 876 | 144 | /websites/reactrouter (unversioned) |
| rr6-defer | react-router@6.26.2 | ✅ | ✅ | ✅ | ✅ | 872 | 50 | /websites/reactrouter (unversioned) |
| rr6-future | react-router@6.26.2 | ✅ | ✅ | ✅ | ✅ | 755 | 56 | /websites/reactrouter (unversioned) |
| rr7-router | react-router@7.1.1 | ✅ | ✅ | ✅ | ✅ | 807 | 58 | /websites/reactrouter (unversioned) |
| pyd1-dict | pydantic@1.10.18 | ✅ | ✅ | ✅ | ❌ | 846 | 157 | /pydantic/pydantic (unversioned) |
| pyd2-dict | pydantic@2.9.2 | ❌ | ✅ | ✅ | ✅ | 952 | 279 | /pydantic/pydantic (unversioned) |
| pyd1-parse | pydantic@1.10.18 | ❌ | ❌ | ✅ | ❌ | 845 | 64 | /pydantic/pydantic (unversioned) |
| pyd2-parse | pydantic@2.9.2 | ❌ | ✅ | ✅ | ✅ | 825 | 76 | /pydantic/pydantic (unversioned) |
| pyd1-validator | pydantic@1.10.18 | ✅ | ✅ | ✅ | ❌ | 961 | 62 | /pydantic/pydantic (unversioned) |
| pyd2-validator | pydantic@2.9.2 | ❌ | ✅ | ✅ | ✅ | 775 | 77 | /pydantic/pydantic (unversioned) |
| pyd1-schema | pydantic@1.10.18 | ✅ | ✅ | ✅ | ❌ | 818 | 68 | /pydantic/pydantic (unversioned) |
| pyd2-schema | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 880 | 79 | /pydantic/pydantic (unversioned) |
| pyd1-config | pydantic@1.10.18 | ❌ | ❌ | ❌ | ❌ | 923 | 64 | /pydantic/pydantic (unversioned) |
| pyd2-config | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 952 | 80 | /pydantic/pydantic (unversioned) |
| axum07-path | axum@0.7.9 | ✅ | ✅ | ✅ | ❌ | 1020 | 103 | /websites/rs_axum (unversioned) |
| axum08-path | axum@0.8.1 | ✅ | ✅ | ✅ | ✅ | 962 | 97 | /websites/rs_axum (unversioned) |
| axum07-extractor | axum@0.7.9 | ✅ | ✅ | ✅ | ❌ | 912 | 46 | /websites/rs_axum (unversioned) |
| axum08-optional | axum@0.8.1 | ✅ | ✅ | ✅ | ✅ | 891 | 41 | /websites/rs_axum (unversioned) |
| tokio-blocking | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 823 | 363 | /websites/rs_tokio_1_49_0 (unversioned) |
| tokio-select | tokio@1.43.0 | ❌ | ❌ | ✅ | ✅ | 962 | 69 | /websites/rs_tokio_1_49_0 (unversioned) |
| tokio-timeout | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 941 | 63 | /websites/rs_tokio_1_49_0 (unversioned) |
| tw3-css | tailwindcss@3.4.19 | ❌ | ❌ | ✅ | ❌ | 971 | 170 | /rails/tailwindcss-rails (unversioned) |
| tw4-css | tailwindcss@4.1.18 | ❌ | ❌ | ✅ | ✅ | 979 | 193 | /rails/tailwindcss-rails (unversioned) |
| tw3-theme | tailwindcss@3.4.19 | ✅ | ✅ | ✅ | ✅ | 1003 | 63 | /rails/tailwindcss-rails (unversioned) |
| tw4-theme | tailwindcss@4.1.18 | ❌ | ❌ | ✅ | ✅ | 1144 | 64 | /rails/tailwindcss-rails (unversioned) |
| eslint8-config | eslint@8.57.1 | ✅ | ✅ | ✅ | ✅ | 796 | 474 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-config | eslint@9.39.5 | ✅ | ✅ | ✅ | ✅ | 720 | 417 | /eslint/eslint/v9.39.3 (same major) |
| eslint8-ignore | eslint@8.57.1 | ❌ | ❌ | ✅ | ✅ | 854 | 88 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-ignore | eslint@9.39.5 | ❌ | ❌ | ✅ | ✅ | 849 | 99 | /eslint/eslint/v9.39.3 (same major) |
| prisma5-bytes | @prisma/client@5.22.0 | ❌ | ❌ | ❌ | ❌ | 846 | 385 | /websites/prisma_io (unversioned) |
| prisma6-bytes | @prisma/client@6.19.3 | ✅ | ✅ | ✅ | ✅ | 848 | 438 | /prisma/web (unversioned) |
| prisma5-fts | prisma@5.22.0 | ❌ | ❌ | ✅ | ❌ | 849 | 385 | /prisma/web (unversioned) |
| prisma6-fts | prisma@6.19.3 | ❌ | ❌ | ✅ | ✅ | 846 | 471 | /prisma/web (unversioned) |
| react18-action | react@18.3.1 | ✅ | ❌ | ❌ | ❌ | 832 | 409 | /reactjs/react.dev (unversioned) |
| react19-action | react@19.2.8 | ❌ | ✅ | ✅ | ✅ | 872 | 464 | /reactjs/react.dev (unversioned) |
| react18-use | react@18.3.1 | ❌ | ❌ | ✅ | ❌ | 823 | 99 | /reactjs/react.dev (unversioned) |
| react19-use | react@19.2.8 | ❌ | ❌ | ✅ | ✅ | 854 | 104 | /reactjs/react.dev (unversioned) |
| vite5-env | vite@5.4.21 | ✅ | ✅ | ✅ | ❌ | 897 | 186 | /vitejs/vite/v5.4.21 (exact) |
| vite6-env | vite@6.4.3 | ✅ | ✅ | ✅ | ✅ | 839 | 165 | /vitejs/vite (unversioned) |
| express4-wildcard | express@4.21.2 | ❌ | ❌ | ✅ | ❌ | 813 | 96 | /expressjs/express (unversioned) |
| express5-wildcard | express@5.2.1 | ❌ | ❌ | ✅ | ✅ | 932 | 97 | /expressjs/express/v5.2.0 (same major) |
| sa14-column | sqlalchemy@1.4.54 | ✅ | ✅ | ✅ | ❌ | 852 | 978 | /websites/sqlalchemy_en_20 (unversioned) |
| sa20-column | sqlalchemy@2.0.54 | ❌ | ✅ | ✅ | ✅ | 815 | 1167 | /websites/sqlalchemy_en_20 (unversioned) |
| sa14-base | sqlalchemy@1.4.54 | ✅ | ✅ | ✅ | ❌ | 828 | 166 | /websites/sqlalchemy_en_20 (unversioned) |
| sa20-base | sqlalchemy@2.0.54 | ✅ | ✅ | ✅ | ✅ | 863 | 190 | /websites/sqlalchemy_en_20 (unversioned) |
| dj42-dbdefault | django@4.2.30 | ❌ | ❌ | ✅ | ✅ | 812 | 1185 | /django/django/4.2.21 (same major) |
| dj51-dbdefault | django@5.1.15 | ❌ | ✅ | ✅ | ✅ | 802 | 1214 | /django/django (unversioned) |
| dj42-login | django@4.2.30 | ✅ | ✅ | ✅ | ✅ | 790 | 206 | /django/django/4.2.21 (same major) |
| dj51-login | django@5.1.15 | ✅ | ✅ | ✅ | ✅ | 736 | 207 | /django/django (unversioned) |
| fa088-lifespan | fastapi@0.88.0 | ❌ | ❌ | ❌ | ❌ | 873 | 203 | /websites/fastapi_tiangolo (unversioned) |
| fa0115-lifespan | fastapi@0.115.14 | ❌ | ❌ | ✅ | ✅ | 828 | 257 | /websites/fastapi_tiangolo (unversioned) |
| fa088-annotated | fastapi@0.88.0 | ❌ | ❌ | ✅ | ❌ | 865 | 76 | /websites/fastapi_tiangolo (unversioned) |
| fa0115-annotated | fastapi@0.115.14 | ✅ | ✅ | ✅ | ✅ | 766 | 82 | /websites/fastapi_tiangolo (unversioned) |
| next15-proxy | next@15.1.0 | ✅ | ✅ | ✅ | ❌ | 797 | 104 | /vercel/next.js/v15.1.11 (same major) |
| next16-proxy | next@16.3.6 | ✅ | ✅ | ✅ | ✅ | 816 | 718 | /vercel/next.js/v16.2.9 (same major) |
| next14-params | next@14.2.35 | ✅ | ✅ | ✅ | ✅ | 881 | 97 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-params | next@15.1.0 | ❌ | ❌ | ✅ | ✅ | 880 | 105 | /vercel/next.js/v15.1.11 (same major) |
| tw3-dark | tailwindcss@3.4.19 | ✅ | ✅ | ✅ | ✅ | 896 | 66 | /erimicel/select2-tailwindcss-theme (unversioned) |
| tw4-dark | tailwindcss@4.1.18 | ❌ | ❌ | ✅ | ❌ | 874 | 68 | /erimicel/select2-tailwindcss-theme (unversioned) |
| react18-ref | react@18.3.1 | ✅ | ✅ | ✅ | ✅ | 899 | 101 | /reactjs/react.dev (unversioned) |
| react19-ref | react@19.2.8 | ❌ | ❌ | ✅ | ✅ | 796 | 111 | /reactjs/react.dev (unversioned) |
| rr6-types | react-router@6.26.2 | ✅ | ✅ | ✅ | ❌ | 790 | 57 | /websites/reactrouter (unversioned) |
| rr7-types | react-router@7.1.1 | ❌ | ❌ | ✅ | ✅ | 851 | 58 | /websites/reactrouter (unversioned) |
| eslint8-globals | eslint@8.57.1 | ❌ | ❌ | ❌ | ✅ | 768 | 95 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-globals | eslint@9.39.5 | ❌ | ❌ | ✅ | ✅ | 741 | 96 | /eslint/eslint/v9.39.3 (same major) |
| dj51-generated | django@5.1.15 | ❌ | ❌ | ✅ | ✅ | 750 | 218 | /django/django (unversioned) |
| fa0115-querymodel | fastapi@0.115.14 | ❌ | ❌ | ✅ | ✅ | 887 | 85 | /websites/fastapi_tiangolo (unversioned) |
| tokio-channel | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 881 | 71 | /websites/rs_tokio_tokio (unversioned) |
| zod3-datetime | zod@3.23.8 | ✅ | ✅ | ✅ | ❌ | 1162 | 44 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-datetime | zod@4.1.5 | ✅ | ✅ | ✅ | ✅ | 1158 | 61 | /colinhacks/zod/v4.0.1 (same major) |
| pyd1-frozen | pydantic@1.10.18 | ✅ | ❌ | ✅ | ❌ | 930 | 69 | /pydantic/pydantic (unversioned) |
| pyd2-frozen | pydantic@2.9.2 | ❌ | ❌ | ✅ | ✅ | 884 | 82 | /pydantic/pydantic (unversioned) |
| next15-form (held-out) | next@15.1.0 | ❌ | ❌ | ✅ | ✅ | 898 | 103 | /vercel/next.js/v15.1.11 (same major) |
| next16-cache (held-out) | next@16.3.6 | ✅ | ✅ | ✅ | ✅ | 856 | 142 | /vercel/next.js/v16.2.9 (same major) |
| zod3-meta (held-out) | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 975 | 45 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-meta (held-out) | zod@4.1.5 | ❌ | ❌ | ✅ | ✅ | 888 | 55 | /colinhacks/zod/v4.0.1 (same major) |
| pyd2-computed (held-out) | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 892 | 82 | /pydantic/pydantic (unversioned) |
| pyd1-json (held-out) | pydantic@1.10.18 | ❌ | ❌ | ✅ | ❌ | 938 | 70 | /pydantic/pydantic (unversioned) |
| pyd2-json (held-out) | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 872 | 79 | /pydantic/pydantic (unversioned) |
| sa20-dataclass (held-out) | sqlalchemy@2.0.54 | ✅ | ✅ | ✅ | ✅ | 867 | 193 | /websites/sqlalchemy_en_20 (unversioned) |
| eslint8-plugins (held-out) | eslint@8.57.1 | ❌ | ❌ | ✅ | ✅ | 784 | 90 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-plugins (held-out) | eslint@9.39.5 | ❌ | ❌ | ❌ | ✅ | 788 | 101 | /eslint/eslint/v9.39.3 (same major) |
| vite6-envapi (held-out) | vite@6.4.3 | ✅ | ❌ | ✅ | ✅ | 673 | 62 | /vitejs/vite (unversioned) |
| react18-context (held-out) | react@18.3.1 | ✅ | ❌ | ✅ | ❌ | 823 | 98 | /reactjs/react.dev (unversioned) |
| react19-context (held-out) | react@19.2.8 | ❌ | ❌ | ✅ | ✅ | 812 | 104 | /reactjs/react.dev (unversioned) |
| rr7-routesfile (held-out) | react-router@7.1.1 | ❌ | ❌ | ✅ | ✅ | 898 | 55 | /websites/reactrouter (unversioned) |
| dj51-facets (held-out) | django@5.1.15 | ❌ | ❌ | ✅ | ✅ | 769 | 209 | /django/django (unversioned) |
| tw3-source (held-out) | tailwindcss@3.4.19 | ❌ | ❌ | ✅ | ✅ | 860 | 62 | /rails/tailwindcss-rails (unversioned) |
| tw4-source (held-out) | tailwindcss@4.1.18 | ❌ | ❌ | ❌ | ❌ | 939 | 67 | /rails/tailwindcss-rails (unversioned) |
| tokio-interval (held-out) | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 890 | 65 | /websites/rs_tokio_tokio (unversioned) |

Index build per project (cold cache, all direct deps, package files):

axum07 391 ms, axum08 382 ms, django42 532 ms, django51 534 ms, eslint8 185 ms, eslint9 78 ms, express4 81 ms, express5 48 ms, fastapi0115 89 ms, fastapi088 69 ms, next14 391 ms, next15 576 ms, next16 1070 ms, prisma5 97 ms, prisma6 125 ms, pydantic1 106 ms, pydantic2 184 ms, react18 370 ms, react19 662 ms, rr6 290 ms, rr7 515 ms, sqlalchemy14 500 ms, sqlalchemy20 650 ms, tailwind3 46 ms, tailwind4 44 ms, vite5 93 ms, vite6 100 ms, zod3 73 ms, zod4 115 ms

`lockdocs fetch` per project (one-time; upstream docs from GitHub at the version tag):

- axum07: 2469 ms (axum@0.7.9 2 files, tokio@1.41.1 18 files)
- axum08: 2192 ms (tokio@1.43.0 18 files)
- django42: 4160 ms (django@4.2.30 601 files)
- django51: 4787 ms (django@5.1.15 629 files)
- eslint8: 3476 ms (eslint@8.57.1 409 files)
- eslint9: 3330 ms (eslint@9.39.5 435 files)
- express4: 3582 ms (@types/express@4.17.25 6 files, express@4.21.2 10 files)
- express5: 2913 ms (@types/express@5.0.6 38 files, express@5.2.1 40 files)
- fastapi0115: 2264 ms (fastapi@0.115.14 180 files)
- fastapi088: 2594 ms (fastapi@0.88.0 113 files)
- next14: 3699 ms (next@14.2.35 318 files, react@18.3.1 140 files, react-dom@18.3.1 140 files)
- next15: 4137 ms (next@15.1.0 365 files, react@19.0.0 184 files, react-dom@19.0.0 184 files)
- next16: 3730 ms (next@16.3.6 458 files, react@19.2.8 183 files, react-dom@19.2.8 183 files)
- prisma5: 6014 ms (@prisma/client@5.22.0 236 files, prisma@5.22.0 236 files)
- prisma6: 3632 ms (@prisma/client@6.19.3 247 files, prisma@6.19.3 247 files)
- pydantic1: 1757 ms (pydantic@1.10.18 176 files)
- pydantic2: 1177 ms (pydantic@2.9.2 80 files)
- react18: 2923 ms (@types/react@18.3.31 137 files, react@18.3.1 140 files, react-dom@18.3.1 140 files)
- react19: 2912 ms (@types/react@19.2.18 180 files, react@19.2.8 183 files, react-dom@19.2.8 183 files)
- rr6: 2148 ms (react@18.3.1 140 files, react-dom@18.3.1 140 files, react-router@6.26.2 118 files, react-router-dom@6.26.2 118 files)
- rr7: 2033 ms (react@19.0.0 184 files, react-dom@19.0.0 184 files, react-router@7.1.1 68 files)
- sqlalchemy14: 2995 ms (sqlalchemy@1.4.54 179 files)
- sqlalchemy20: 3057 ms (sqlalchemy@2.0.54 198 files)
- tailwind3: 3464 ms (tailwindcss@3.4.19 192 files)
- tailwind4: 7799 ms (tailwindcss@4.1.18 224 files)
- vite5: 1136 ms (vite@5.4.21 37 files)
- vite6: 1367 ms (vite@6.4.3 47 files)
- zod3: 630 ms (zod@3.23.8 4 files)
- zod4: 1337 ms (zod@4.1.5 18 files)

## Reading the results

- **lockdocs is ahead overall (96/105 vs 77/105) and on older majors by 2x (38/45 vs 19/45), ties Context7 on the newest majors (53/55 each) and on tokio (5/5 each), and is ahead on the held-out questions (16/18 vs 15/18).** It also uses fewer tokens (866 vs 908 median) and answers in 97 ms instead of 2,583 ms.
- **Where each still misses.** lockdocs: seven older-major questions (among them Next.js 14 `cookies()`, React Router 6 `json()`, FastAPI 0.88 `on_event`, Prisma 5 `Buffer`, ESLint 8 `env`), and two held-out newer-major questions (ESLint 9 plugins, Tailwind 4 `@source`). Context7 mostly answers older-major questions with the newest API, and two of its Tailwind answers came from unrelated libraries its search ranked first.
- **Configurations matter.** Keyword-only on package files: 59/105. Adding the local embedding model: 60/105. Adding `lockdocs fetch` (upstream docs at each version's git tag, plus docs-site repositories for your major): 96/105.
- Latency for lockdocs is a fresh CLI process per question, including loading the embedding model; the MCP server keeps it loaded.
- Context7 answers for questions whose wording, grading and pinned version are unchanged since the previous run were reused from that run (disclosed in the results line) to stay within the anonymous quota.
- Contributions of new version-sensitive questions are welcome.
