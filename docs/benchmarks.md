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

From [this run](https://github.com/SylphxAI/lockdocs/actions/runs/36125903106).

Tokenizer: tiktoken o200k_base. Runner: Linux x86_64. 70 questions.

| | correct | older major | newer major | single version | median tokens | median latency | p95 latency |
|---|---|---|---|---|---|---|---|
| lockdocs, keyword only (BM25), package files | 41/70 | 20/33 | 19/34 | 2/3 | 898 | 22 ms | 397 ms |
| lockdocs, hybrid (BM25 + embeddings), package files | 46/70 | 22/33 | 22/34 | 2/3 | 915 | 49 ms | 94 ms |
| lockdocs, hybrid + upstream docs (after `lockdocs fetch`) | 55/70 | 24/33 | 29/34 | 2/3 | 875 | 87 ms | 491 ms |
| Context7 (anonymous API) | 49/70 | 12/33 | 34/34 | 3/3 | 908 | 2011 ms | 3327 ms |

Context7 (anonymous): 4 HTTP calls, 0 rate-limited (429), 0 other errors; ratelimit-limit header 200, remaining 196. 67 answers reused from the previous run's identical question and version (see bench/run.py --context7-cache).

| question | version | keyword | hybrid | fetched | context7 | tokens (last lockdocs) | ms | Context7 library |
|---|---|---|---|---|---|---|---|---|
| zod3-strict | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 1019 | 84 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-strict | zod@4.1.5 | ✅ | ❌ | ✅ | ✅ | 946 | 134 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-email | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 1167 | 40 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-email | zod@4.1.5 | ✅ | ✅ | ✅ | ✅ | 965 | 59 | /colinhacks/zod/v4.0.1 (same major) |
| zod3-error | zod@3.23.8 | ✅ | ✅ | ✅ | ✅ | 985 | 46 | /colinhacks/zod/v3.24.2 (same major) |
| zod4-error | zod@4.1.5 | ✅ | ✅ | ✅ | ✅ | 989 | 52 | /colinhacks/zod/v4.0.1 (same major) |
| zod4-record | zod@4.1.5 | ❌ | ❌ | ✅ | ✅ | 1026 | 57 | /colinhacks/zod/v4.0.1 (same major) |
| next14-cookies | next@14.2.35 | ❌ | ✅ | ❌ | ❌ | 849 | 342 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-cookies | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 892 | 392 | /vercel/next.js/v15.1.11 (same major) |
| next14-headers | next@14.2.35 | ✅ | ✅ | ✅ | ✅ | 892 | 80 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-headers | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 872 | 84 | /vercel/next.js/v15.1.11 (same major) |
| next14-nostore | next@14.2.35 | ✅ | ✅ | ✅ | ✅ | 761 | 82 | /vercel/next.js/v14.3.0-canary.87 (same major) |
| next15-connection | next@15.1.0 | ✅ | ✅ | ✅ | ✅ | 798 | 93 | /vercel/next.js/v15.1.11 (same major) |
| next15-after | next@15.1.0 | ✅ | ✅ | ❌ | ✅ | 879 | 92 | /vercel/next.js/v15.1.11 (same major) |
| rr6-json | react-router@6.26.2 | ✅ | ✅ | ❌ | ❌ | 796 | 99 | /websites/reactrouter (unversioned) |
| rr7-data | react-router@7.1.1 | ❌ | ✅ | ✅ | ✅ | 892 | 122 | /websites/reactrouter (unversioned) |
| rr6-defer | react-router@6.26.2 | ✅ | ✅ | ✅ | ✅ | 895 | 47 | /websites/reactrouter (unversioned) |
| rr6-future | react-router@6.26.2 | ✅ | ✅ | ✅ | ✅ | 815 | 52 | /websites/reactrouter (unversioned) |
| rr7-router | react-router@7.1.1 | ✅ | ✅ | ✅ | ✅ | 838 | 51 | /websites/reactrouter (unversioned) |
| pyd1-dict | pydantic@1.10.18 | ✅ | ✅ | ✅ | ❌ | 948 | 151 | /pydantic/pydantic (unversioned) |
| pyd2-dict | pydantic@2.9.2 | ❌ | ❌ | ✅ | ✅ | 909 | 245 | /pydantic/pydantic (unversioned) |
| pyd1-parse | pydantic@1.10.18 | ❌ | ❌ | ✅ | ❌ | 884 | 63 | /pydantic/pydantic (unversioned) |
| pyd2-parse | pydantic@2.9.2 | ❌ | ❌ | ✅ | ✅ | 843 | 72 | /pydantic/pydantic (unversioned) |
| pyd1-validator | pydantic@1.10.18 | ❌ | ✅ | ✅ | ❌ | 965 | 62 | /pydantic/pydantic (unversioned) |
| pyd2-validator | pydantic@2.9.2 | ❌ | ✅ | ✅ | ✅ | 764 | 67 | /pydantic/pydantic (unversioned) |
| pyd1-schema | pydantic@1.10.18 | ✅ | ✅ | ✅ | ❌ | 878 | 62 | /pydantic/pydantic (unversioned) |
| pyd2-schema | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 817 | 72 | /pydantic/pydantic (unversioned) |
| pyd1-config | pydantic@1.10.18 | ❌ | ❌ | ❌ | ❌ | 945 | 59 | /pydantic/pydantic (unversioned) |
| pyd2-config | pydantic@2.9.2 | ✅ | ✅ | ✅ | ✅ | 934 | 71 | /pydantic/pydantic (unversioned) |
| axum07-path | axum@0.7.9 | ✅ | ✅ | ✅ | ❌ | 960 | 88 | /websites/rs_axum (unversioned) |
| axum08-path | axum@0.8.1 | ✅ | ✅ | ✅ | ✅ | 925 | 94 | /websites/rs_axum (unversioned) |
| axum07-extractor | axum@0.7.9 | ✅ | ✅ | ✅ | ❌ | 926 | 46 | /websites/rs_axum (unversioned) |
| axum08-optional | axum@0.8.1 | ✅ | ✅ | ✅ | ✅ | 898 | 42 | /websites/rs_axum (unversioned) |
| tokio-blocking | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 820 | 316 | /websites/rs_tokio_1_49_0 (unversioned) |
| tokio-select | tokio@1.43.0 | ❌ | ❌ | ❌ | ✅ | 969 | 60 | /websites/rs_tokio_1_49_0 (unversioned) |
| tokio-timeout | tokio@1.43.0 | ✅ | ✅ | ✅ | ✅ | 920 | 55 | /websites/rs_tokio_1_49_0 (unversioned) |
| tw3-css | tailwindcss@3.4.19 | ❌ | ❌ | ❌ | ❌ | 959 | 62 | /rails/tailwindcss-rails (unversioned) |
| tw4-css | tailwindcss@4.1.18 | ❌ | ❌ | ❌ | ✅ | 849 | 146 | /rails/tailwindcss-rails (unversioned) |
| tw3-theme | tailwindcss@3.4.19 | ✅ | ✅ | ✅ | ✅ | 1035 | 41 | /rails/tailwindcss-rails (unversioned) |
| tw4-theme | tailwindcss@4.1.18 | ❌ | ❌ | ✅ | ✅ | 1147 | 54 | /rails/tailwindcss-rails (unversioned) |
| eslint8-config | eslint@8.57.1 | ✅ | ✅ | ✅ | ✅ | 813 | 407 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-config | eslint@9.39.5 | ✅ | ✅ | ✅ | ✅ | 741 | 360 | /eslint/eslint/v9.39.3 (same major) |
| eslint8-ignore | eslint@8.57.1 | ❌ | ❌ | ✅ | ✅ | 854 | 84 | /eslint/eslint/v8.57.1 (exact) |
| eslint9-ignore | eslint@9.39.5 | ❌ | ❌ | ✅ | ✅ | 825 | 83 | /eslint/eslint/v9.39.3 (same major) |
| prisma5-bytes | @prisma/client@5.22.0 | ❌ | ❌ | ❌ | ❌ | 878 | 78 | /websites/prisma_io (unversioned) |
| prisma6-bytes | @prisma/client@6.19.3 | ✅ | ✅ | ❌ | ✅ | 878 | 95 | /prisma/web (unversioned) |
| prisma5-fts | prisma@5.22.0 | ❌ | ❌ | ❌ | ❌ | 915 | 70 | /prisma/web (unversioned) |
| prisma6-fts | prisma@6.19.3 | ❌ | ❌ | ❌ | ✅ | 850 | 90 | /prisma/web (unversioned) |
| react18-action | react@18.3.1 | ✅ | ✅ | ❌ | ❌ | 951 | 129 | /reactjs/react.dev (unversioned) |
| react19-action | react@19.2.8 | ❌ | ✅ | ✅ | ✅ | 869 | 417 | /reactjs/react.dev (unversioned) |
| react18-use | react@18.3.1 | ❌ | ❌ | ✅ | ❌ | 804 | 48 | /reactjs/react.dev (unversioned) |
| react19-use | react@19.2.8 | ❌ | ❌ | ✅ | ✅ | 859 | 94 | /reactjs/react.dev (unversioned) |
| vite5-env | vite@5.4.21 | ✅ | ✅ | ✅ | ❌ | 907 | 164 | /vitejs/vite/v5.4.21 (exact) |
| vite6-env | vite@6.4.3 | ✅ | ✅ | ✅ | ✅ | 803 | 142 | /vitejs/vite (unversioned) |
| express4-wildcard | express@4.21.2 | ❌ | ❌ | ✅ | ❌ | 730 | 86 | /expressjs/express (unversioned) |
| express5-wildcard | express@5.2.1 | ❌ | ❌ | ✅ | ✅ | 944 | 87 | /expressjs/express/v5.2.0 (same major) |
| sa14-column | sqlalchemy@1.4.54 | ✅ | ✅ | ✅ | ❌ | 869 | 855 | /websites/sqlalchemy_en_20 (unversioned) |
| sa20-column | sqlalchemy@2.0.54 | ❌ | ✅ | ✅ | ✅ | 841 | 1036 | /websites/sqlalchemy_en_20 (unversioned) |
| sa14-base | sqlalchemy@1.4.54 | ✅ | ✅ | ✅ | ❌ | 831 | 152 | /websites/sqlalchemy_en_20 (unversioned) |
| sa20-base | sqlalchemy@2.0.54 | ✅ | ✅ | ✅ | ✅ | 860 | 167 | /websites/sqlalchemy_en_20 (unversioned) |
| dj42-dbdefault | django@4.2.30 | ❌ | ❌ | ❌ | ✅ | 852 | 483 | /django/django/4.2.21 (same major) |
| dj51-dbdefault | django@5.1.15 | ❌ | ❌ | ❌ | ✅ | 858 | 491 | /django/django (unversioned) |
| dj42-login | django@4.2.30 | ✅ | ✅ | ✅ | ✅ | 816 | 87 | /django/django/4.2.21 (same major) |
| dj51-login | django@5.1.15 | ✅ | ✅ | ✅ | ✅ | 855 | 92 | /django/django (unversioned) |
| fa088-lifespan | fastapi@0.88.0 | ❌ | ❌ | ❌ | ❌ | 833 | 181 | /websites/fastapi_tiangolo (unversioned) |
| fa0115-lifespan | fastapi@0.115.14 | ❌ | ❌ | ✅ | ✅ | 867 | 231 | /websites/fastapi_tiangolo (unversioned) |
| fa088-annotated | fastapi@0.88.0 | ❌ | ❌ | ✅ | ❌ | 875 | 69 | /websites/fastapi_tiangolo (unversioned) |
| fa0115-annotated | fastapi@0.115.14 | ✅ | ✅ | ✅ | ✅ | 838 | 70 | /websites/fastapi_tiangolo (unversioned) |
| next15-proxy | next@15.1.0 | ✅ | ✅ | ✅ | ❌ | 831 | 90 | /vercel/next.js/v15.1.11 (same major) |
| next16-proxy | next@16.3.6 | ✅ | ✅ | ✅ | ✅ | 869 | 630 | /vercel/next.js/v16.2.9 (same major) |

Index build per project (cold cache, all direct deps, package files):

axum07 345 ms, axum08 401 ms, django42 473 ms, django51 480 ms, eslint8 182 ms, eslint9 80 ms, express4 72 ms, express5 48 ms, fastapi0115 83 ms, fastapi088 71 ms, next14 378 ms, next15 603 ms, next16 868 ms, prisma5 92 ms, prisma6 123 ms, pydantic1 98 ms, pydantic2 176 ms, react18 361 ms, react19 659 ms, rr6 279 ms, rr7 524 ms, sqlalchemy14 466 ms, sqlalchemy20 604 ms, tailwind3 50 ms, tailwind4 46 ms, vite5 91 ms, vite6 90 ms, zod3 65 ms, zod4 106 ms

`lockdocs fetch` per project (one-time; upstream docs from GitHub at the version tag):

- axum07: 1636 ms (axum@0.7.9 2 files, tokio@1.41.1 1 files)
- axum08: 2101 ms (tokio@1.43.0 1 files)
- django42: 4897 ms (django@4.2.30 601 files)
- django51: 4876 ms (django@5.1.15 629 files)
- eslint8: 3197 ms (eslint@8.57.1 409 files)
- eslint9: 3391 ms (eslint@9.39.5 435 files)
- express4: 2984 ms (@types/express@4.17.25 6 files, express@4.21.2 10 files)
- express5: 3046 ms (@types/express@5.0.6 38 files, express@5.2.1 40 files)
- fastapi0115: 2482 ms (fastapi@0.115.14 180 files)
- fastapi088: 2335 ms (fastapi@0.88.0 113 files)
- next14: 4976 ms (next@14.2.35 318 files, react@18.3.1 3 files, react-dom@18.3.1 3 files)
- next15: 3429 ms (next@15.1.0 365 files, react@19.0.0 184 files, react-dom@19.0.0 184 files)
- next16: 4143 ms (next@16.3.6 458 files, react@19.2.8 183 files, react-dom@19.2.8 183 files)
- prisma5: 1697 ms (@prisma/client@5.22.0 2 files, prisma@5.22.0 2 files)
- prisma6: 1739 ms (@prisma/client@6.19.3 2 files, prisma@6.19.3 2 files)
- pydantic1: 1635 ms (pydantic@1.10.18 176 files)
- pydantic2: 1223 ms (pydantic@2.9.2 80 files)
- react18: 2396 ms (react@18.3.1 3 files, react-dom@18.3.1 3 files)
- react19: 3314 ms (@types/react@19.2.18 180 files, react@19.2.8 183 files, react-dom@19.2.8 183 files)
- rr6: 2184 ms (react@18.3.1 3 files, react-dom@18.3.1 3 files, react-router@6.26.2 118 files, react-router-dom@6.26.2 118 files)
- rr7: 2112 ms (react@19.0.0 184 files, react-dom@19.0.0 184 files, react-router@7.1.1 68 files)
- sqlalchemy14: 3270 ms (sqlalchemy@1.4.54 179 files)
- sqlalchemy20: 3201 ms (sqlalchemy@2.0.54 198 files)
- tailwind3: 725 ms (tailwindcss@3.4.19 2 files)
- tailwind4: 2472 ms (tailwindcss@4.1.18 200 files)
- vite5: 1379 ms (vite@5.4.21 37 files)
- vite6: 1511 ms (vite@6.4.3 47 files)
- zod3: 612 ms (zod@3.23.8 4 files)
- zod4: 1219 ms (zod@4.1.5 18 files)

## Reading the results

- **lockdocs is ahead overall (55/70 vs 49/70), on older majors by a wide margin (24/33 vs 12/33), on tokens (875 vs 908 median) and on latency (87 ms vs 2,011 ms).** Context7 serves one or a few indexed versions per library, so questions about the version you actually pinned often get the newest API.
- **Context7 is ahead on the newest majors (34/34 vs 29/34) and on tokio (3/3 vs 2/3).** lockdocs misses where the answer lives only in a docs website that tracks a different major than the pinned one (Prisma 6: the Prisma docs now describe a later major), and on a few ranking misses (Next.js `after`, Tailwind 4's `@import "tailwindcss"`, Django `db_default`, tokio `select!` for "whichever finishes first").
- **Configurations matter.** Keyword-only on package files: 41/70. Adding the local embedding model: 46/70. Adding `lockdocs fetch` (upstream docs at each version's git tag, plus docs-site repositories when the pinned major is the current one): 55/70.
- Latency for lockdocs is a fresh CLI process per question, including loading the embedding model; the MCP server keeps it loaded.
- Context7 answers for questions unchanged since the previous run on the same pinned version were reused from that run (disclosed in the results line) to stay within the anonymous quota.
- Contributions of new version-sensitive questions are welcome.
